//! Walk item syntax and retain only owned source facts; never walk inside macro tokens.
use super::{Facts, attributes, distributed_slice, fragments, imports};
use crate::source::{
    Import, Item, ItemKind, RustSourceProjection, SyntaxError, TestRegions, Visibility,
};
use proc_macro2::Span;
use std::{
    collections::{BTreeMap, BTreeSet},
    ops::RangeInclusive,
    path::Path,
};
use syn::{
    spanned::Spanned,
    visit::{self, Visit},
};

pub(super) struct Walk<'a> {
    path: &'a Path,
    projection: &'a RustSourceProjection,
    lines: Vec<&'a str>,
    line_offsets: Vec<usize>,
    item_ends: BTreeMap<usize, usize>,
    outline: Vec<Item>,
    regions: Vec<RangeInclusive<usize>>,
    gated: BTreeSet<usize>,
    gated_bytes: Vec<std::ops::Range<usize>>,
    module_bytes: Vec<std::ops::Range<usize>>,
    modules: Vec<super::ModuleDeclaration>,
    /// One entry per enclosing item, outermost first: the inline module it
    /// opens, or `None` for any other item. Empty at the top of the file. A
    /// file module is only declared where every enclosing item is an inline
    /// module.
    scopes: Vec<Option<super::InlineModule>>,
    imports: Vec<Import>,
    error: Option<SyntaxError>,
}
impl<'a> Walk<'a> {
    pub fn new(path: &'a Path, projection: &'a RustSourceProjection) -> Self {
        let line_offsets = std::iter::once(0)
            .chain(projection.text().match_indices('\n').map(|(i, _)| i + 1))
            .collect();
        Self {
            path,
            projection,
            lines: projection.text().lines().collect(),
            line_offsets,
            item_ends: BTreeMap::new(),
            outline: Vec::new(),
            regions: Vec::new(),
            gated: BTreeSet::new(),
            gated_bytes: Vec::new(),
            module_bytes: Vec::new(),
            modules: Vec::new(),
            scopes: Vec::new(),
            imports: Vec::new(),
            error: None,
        }
    }
    pub fn finish(self) -> Result<Facts, SyntaxError> {
        if let Some(error) = self.error {
            return Err(error);
        }
        Ok(Facts {
            outline: self.outline,
            regions: TestRegions::new(self.lines.len(), self.regions),
            gated: self.gated,
            gated_bytes: self.gated_bytes,
            module_bytes: self.module_bytes,
            modules: self.modules,
            imports: self.imports,
            functions: Vec::new(),
        })
    }
    fn unsupported(&mut self, span: Span, form: &str) {
        self.error.get_or_insert_with(|| SyntaxError::Unsupported {
            line: span.start().line,
            form: form.into(),
        });
    }
    fn after_attributes(&self, end: usize) -> usize {
        let first = self
            .line_offsets
            .partition_point(|offset| *offset <= end)
            .saturating_sub(1);
        self.projection
            .code_lines()
            .skip(first)
            .enumerate()
            .find_map(|(index, line)| {
                let start = self.line_offsets[first + index];
                let skip = end.saturating_sub(start);
                line.get(skip..)?
                    .find(|c: char| !c.is_whitespace())
                    .map(|local| start + skip + local)
            })
            .unwrap_or(end)
    }
    fn record(
        &mut self,
        kind: ItemKind,
        name: String,
        span: Span,
        attrs: &[syn::Attribute],
        vis: &syn::Visibility,
        header_end: Option<usize>,
    ) -> usize {
        let text = self.projection.text();
        let syntax_start = self.syntax_start(span, attrs);
        let end = span.byte_range().end;
        let line = self
            .line_offsets
            .partition_point(|offset| *offset <= syntax_start);
        let attr_line = attrs
            .iter()
            .filter(|a| matches!(a.style, syn::AttrStyle::Outer))
            .map(|a| a.span().start().line)
            .min()
            .unwrap_or(line);
        let first = self.first_attached_line(attr_line, syntax_start, !attrs.is_empty());
        let attached_start = self.attached_start(first, line, syntax_start, span);
        let summary = self.lines[first - 1..line - 1].iter().find_map(|l| {
            l.trim_start()
                .strip_prefix("///")
                .map(|s| s.trim().to_owned())
        });
        let end_line = span.end().line;
        if attributes::requires(attrs, &[]) {
            self.gated.extend(first..=end_line);
            self.gated_bytes.push(span.byte_range());
        }
        let block_end = header_end.unwrap_or(end).min(end);
        let item = Item {
            kind,
            visibility: fragments::visibility(vis),
            name,
            file: self.path.into(),
            line,
            span: first..=end_line,
            declaration_block: text[syntax_start..block_end].into(),
            doc: summary,
            top_level: self.scopes.is_empty(),
            syntax_span: syntax_start..end,
            byte_span: attached_start..end,
            attribute_names: attribute_names(attrs),
            restricted: fragments::restriction(text, vis),
            trait_name: None,
            self_type: None,
            fields: Vec::new(),
        };
        self.item_ends.insert(end, end_line);
        self.outline.push(item);
        self.outline.len() - 1
    }
    /// The byte where an item's own syntax starts: after its outer
    /// attributes, or the start of its span when it has none.
    fn syntax_start(&self, span: Span, attrs: &[syn::Attribute]) -> usize {
        attrs
            .iter()
            .filter(|a| matches!(a.style, syn::AttrStyle::Outer))
            .map(|a| a.span().byte_range().end)
            .max()
            .map_or(span.byte_range().start, |end| self.after_attributes(end))
    }
    /// The first line attached to an item, scanning up from its first outer
    /// attribute line and never into the previous item. Comments directly
    /// above attach; a blank line attaches only once an attribute or `///`
    /// has, and past one only `///` attaches. Leading blank lines are
    /// dropped again.
    /// The one-based lines `bytes` wholly occupies: nothing but whitespace
    /// before it on its first line, and nothing but whitespace or a comment
    /// after it on its last; `None` when it shares a line with other code.
    fn whole_lines(&self, bytes: &std::ops::Range<usize>) -> Option<RangeInclusive<usize>> {
        let text = self.projection.text();
        let line_of = |offset: usize| self.line_offsets.partition_point(|start| *start <= offset);
        let (first, last) = (line_of(bytes.start), line_of(bytes.end.saturating_sub(1)));
        let before = text.get(self.line_offsets[first - 1]..bytes.start)?;
        let line_end = self
            .line_offsets
            .get(last)
            .map_or(text.len(), |next| next - 1);
        let after = text.get(bytes.end..line_end.max(bytes.end))?.trim();
        (before.trim().is_empty() && (after.is_empty() || after.starts_with("//")))
            .then_some(first..=last)
    }
    fn first_attached_line(
        &self,
        attr_line: usize,
        syntax_start: usize,
        attributed: bool,
    ) -> usize {
        let previous_end = self
            .item_ends
            .range(..=syntax_start)
            .next_back()
            .map_or(0, |(_, line)| *line);
        let mut first = attr_line;
        let mut outer = attributed;
        let mut crossed_blank = false;
        while first > previous_end + 1 {
            let before = self.lines[first - 2];
            let trimmed = before.trim_start();
            if self.projection.starts_in_code(first - 2)
                && trimmed.starts_with("//")
                && !trimmed.starts_with("//!")
                && (!crossed_blank || trimmed.starts_with("///"))
            {
                outer |= trimmed.starts_with("///");
                first -= 1;
            } else if trimmed.is_empty() && outer {
                crossed_blank = true;
                first -= 1;
            } else {
                break;
            }
        }
        // Do not attach blank lines preceding the first actual annotation.
        while first < attr_line && self.lines[first - 1].trim().is_empty() {
            first += 1;
        }
        first
    }
    /// The first byte attached to an item: the start of its first attached
    /// line, or its own span start when nothing is attached and earlier code
    /// shares its line.
    fn attached_start(&self, first: usize, line: usize, syntax_start: usize, span: Span) -> usize {
        if first == line
            && syntax_start > 0
            && self.projection.text()[..syntax_start]
                .rsplit('\n')
                .next()
                .is_some_and(|tail| !tail.trim().is_empty())
        {
            span.byte_range().start
        } else {
            self.line_offsets[first - 1]
        }
    }
    fn macro_member(&mut self, attrs: &[syn::Attribute], mac: &syn::Macro, span: Span) {
        self.record(
            ItemKind::MacroCall,
            self.projection.text()[mac.path.span().byte_range()].to_owned(),
            span,
            attrs,
            &syn::Visibility::Inherited,
            None,
        );
    }
    fn function(
        &mut self,
        attrs: &[syn::Attribute],
        vis: &syn::Visibility,
        sig: &syn::Signature,
        span: Span,
        block: Option<&syn::Block>,
    ) {
        self.record(
            ItemKind::Fn,
            sig.ident.to_string(),
            span,
            attrs,
            vis,
            block.map(|b| b.brace_token.span.open().byte_range().end),
        );
    }
    /// Record an item whose declaration block is all of it: there is no
    /// body for the block to leave out.
    fn whole(
        &mut self,
        kind: ItemKind,
        ident: &syn::Ident,
        span: Span,
        attrs: &[syn::Attribute],
        vis: &syn::Visibility,
    ) {
        self.record(kind, ident.to_string(), span, attrs, vis, None);
    }
    fn struct_item(&mut self, n: &syn::ItemStruct) {
        let i = self.record(
            ItemKind::Struct,
            n.ident.to_string(),
            n.span(),
            &n.attrs,
            &n.vis,
            None,
        );
        self.outline[i].fields = fragments::fields(
            self.projection.text(),
            &n.fields,
            n.struct_token.span.start().line,
        );
    }
    /// A `use` item and its import. A public `use` is a re-export
    /// (`PubUse`); the import keeps every binding the tree writes.
    fn use_item(&mut self, n: &syn::ItemUse) {
        let bindings = imports::item_use_bindings(n);
        let name = use_name(self.projection.text(), n, &bindings);
        let kind = if fragments::visibility(&n.vis) == Visibility::Private {
            ItemKind::Use
        } else {
            ItemKind::PubUse
        };
        let i = self.record(kind, name, n.span(), &n.attrs, &n.vis, None);
        let item = &self.outline[i];
        self.imports.push(Import {
            line: item.line,
            end_line: *item.span.end(),
            top_level: item.top_level,
            target: self.projection.text()
                [n.use_token.span.byte_range().end..n.span().byte_range().end]
                .trim_start()
                .into(),
            statement: std::sync::Arc::clone(&item.declaration_block),
            bindings: Ok(bindings),
            bytes: item.syntax_span.clone(),
        });
    }
    /// A module item. A test-gated inline module is a test region; a file
    /// module declared where every enclosing item is an inline module is a
    /// module declaration, test-gated when it lies in gated bytes.
    fn mod_item(&mut self, n: &syn::ItemMod) {
        let index = self.record(
            ItemKind::Mod,
            n.ident.to_string(),
            n.span(),
            &n.attrs,
            &n.vis,
            n.content
                .as_ref()
                .map(|(b, _)| b.span.open().byte_range().end),
        );
        if n.content.is_some() && attributes::requires(&n.attrs, &[]) {
            let gate = n
                .attrs
                .iter()
                .find(|attr| attributes::requires(std::slice::from_ref(attr), &[]))
                .map_or(n.mod_token.span.start().line, |attr| {
                    attr.span().start().line
                });
            self.regions.push(gate..=n.span().end().line);
            self.module_bytes.push(n.span().byte_range());
        }
        if n.content.is_none() && self.scopes.iter().all(Option::is_some) {
            self.modules.push(super::ModuleDeclaration {
                name: n.ident.to_string(),
                path: path_attribute(&n.attrs),
                alternatives: conditional_paths(&n.attrs),
                test_gated: self
                    .gated_bytes
                    .iter()
                    .any(|range| range.contains(&n.span().byte_range().start)),
                inline: self.scopes.iter().flatten().cloned().collect(),
                item: self.outline[index].syntax_span.start,
            });
        }
    }
    /// An impl block, named by its self type's head, else its trait's last
    /// segment, else `impl`.
    fn impl_item(&mut self, n: &syn::ItemImpl) {
        let trait_name = n
            .trait_
            .as_ref()
            .and_then(|(p, _)| p.segments.last())
            .map(|p| p.ident.to_string());
        let name = type_head(&n.self_ty)
            .or_else(|| trait_name.clone())
            .unwrap_or_else(|| "impl".into());
        let i = self.record(
            ItemKind::Impl,
            name,
            n.span(),
            &n.attrs,
            &syn::Visibility::Inherited,
            Some(n.brace_token.span.open().byte_range().end),
        );
        self.outline[i].trait_name = trait_name;
        self.outline[i].self_type = type_head(&n.self_ty);
    }
    /// A `macro_rules!` definition or an item macro call, named by its
    /// identifier when it has one, else by its written macro path.
    fn macro_item(&mut self, n: &syn::ItemMacro) {
        let kind = if n.mac.path.is_ident("macro_rules") {
            ItemKind::MacroRules
        } else {
            ItemKind::MacroCall
        };
        let name = n.ident.as_ref().map_or_else(
            || self.projection.text()[n.mac.path.span().byte_range()].to_owned(),
            ToString::to_string,
        );
        self.record(
            kind,
            name,
            n.span(),
            &n.attrs,
            &syn::Visibility::Inherited,
            None,
        );
    }
}
impl<'ast> Visit<'ast> for Walk<'_> {
    /// A statement under a test-only gate (`#[cfg(test)] { ... }`, a gated
    /// `let`) is test code by its bytes, so a site in it is selected with
    /// the tests; the lines it wholly occupies are test lines too, so line
    /// accounting agrees with the sites on them. A line it shares with
    /// production code stays mixed.
    fn visit_stmt(&mut self, stmt: &'ast syn::Stmt) {
        let attrs: &[syn::Attribute] = match stmt {
            syn::Stmt::Local(local) => &local.attrs,
            syn::Stmt::Macro(mac) => &mac.attrs,
            syn::Stmt::Expr(expr, _) => attributes::expression_attrs(expr),
            syn::Stmt::Item(_) => &[],
        };
        if attributes::requires(attrs, &[]) {
            let bytes = stmt.span().byte_range();
            if let Some(lines) = self.whole_lines(&bytes) {
                self.gated.extend(lines);
            }
            self.gated_bytes.push(bytes);
        }
        visit::visit_stmt(self, stmt);
    }
    fn visit_file(&mut self, file: &'ast syn::File) {
        if attributes::requires(&file.attrs, &[]) {
            self.gated.extend(1..=self.lines.len());
            self.gated_bytes.push(0..self.projection.text().len());
        }
        visit::visit_file(self, file);
    }
    fn visit_item(&mut self, item: &'ast syn::Item) {
        let private = syn::Visibility::Inherited;
        match item {
            syn::Item::Fn(n) => self.function(&n.attrs, &n.vis, &n.sig, n.span(), Some(&n.block)),
            syn::Item::Struct(n) => self.struct_item(n),
            syn::Item::Enum(n) => self.whole(ItemKind::Enum, &n.ident, n.span(), &n.attrs, &n.vis),
            syn::Item::Union(n) => {
                self.whole(ItemKind::Union, &n.ident, n.span(), &n.attrs, &n.vis);
            }
            syn::Item::Trait(n) => {
                self.record(
                    ItemKind::Trait,
                    n.ident.to_string(),
                    n.span(),
                    &n.attrs,
                    &n.vis,
                    Some(n.brace_token.span.open().byte_range().end),
                );
            }
            syn::Item::TraitAlias(n) => self.unsupported(n.span(), "trait alias"),
            syn::Item::Type(n) => {
                self.whole(ItemKind::TypeAlias, &n.ident, n.span(), &n.attrs, &n.vis);
            }
            syn::Item::Const(n) => {
                self.whole(ItemKind::Const, &n.ident, n.span(), &n.attrs, &n.vis);
            }
            syn::Item::Static(n) => {
                self.whole(ItemKind::Static, &n.ident, n.span(), &n.attrs, &n.vis);
            }
            syn::Item::Use(n) => self.use_item(n),
            syn::Item::Mod(n) => self.mod_item(n),
            syn::Item::Impl(n) => self.impl_item(n),
            syn::Item::Macro(n) => self.macro_item(n),
            syn::Item::ExternCrate(n) => {
                self.whole(ItemKind::Extern, &n.ident, n.span(), &n.attrs, &n.vis);
            }
            syn::Item::ForeignMod(n) => {
                self.record(
                    ItemKind::Extern,
                    n.abi
                        .name
                        .as_ref()
                        .map_or_else(|| "C".into(), syn::LitStr::value),
                    n.span(),
                    &n.attrs,
                    &private,
                    Some(n.brace_token.span.open().byte_range().end),
                );
            }
            syn::Item::Verbatim(tokens) => {
                if let Some(item) = distributed_slice::parse(tokens.clone()) {
                    self.record(
                        ItemKind::Static,
                        item.name.to_string(),
                        tokens.span(),
                        &item.attrs,
                        &item.vis,
                        None,
                    );
                } else {
                    self.unsupported(tokens.span(), "verbatim item");
                }
            }
            _ => self.unsupported(item.span(), "unknown syn item"),
        }
        self.scopes.push(match item {
            syn::Item::Mod(n) if n.content.is_some() => Some(super::InlineModule {
                name: n.ident.to_string(),
                path: path_attribute(&n.attrs),
            }),
            _ => None,
        });
        visit::visit_item(self, item);
        self.scopes.pop();
    }
    fn visit_impl_item(&mut self, item: &'ast syn::ImplItem) {
        match item {
            syn::ImplItem::Fn(n) => {
                self.function(&n.attrs, &n.vis, &n.sig, n.span(), Some(&n.block));
            }
            syn::ImplItem::Const(n) => {
                self.whole(ItemKind::Const, &n.ident, n.span(), &n.attrs, &n.vis);
            }
            syn::ImplItem::Type(n) => {
                self.whole(ItemKind::TypeAlias, &n.ident, n.span(), &n.attrs, &n.vis);
            }
            syn::ImplItem::Macro(node) => self.macro_member(&node.attrs, &node.mac, node.span()),
            _ => self.unsupported(item.span(), "unsupported impl member"),
        }
        self.scopes.push(None);
        visit::visit_impl_item(self, item);
        self.scopes.pop();
    }
    fn visit_trait_item(&mut self, item: &'ast syn::TraitItem) {
        let private = syn::Visibility::Inherited;
        match item {
            syn::TraitItem::Fn(n) => {
                self.function(&n.attrs, &private, &n.sig, n.span(), n.default.as_ref());
            }
            syn::TraitItem::Const(n) => {
                self.whole(ItemKind::Const, &n.ident, n.span(), &n.attrs, &private);
            }
            syn::TraitItem::Type(n) => {
                self.whole(ItemKind::TypeAlias, &n.ident, n.span(), &n.attrs, &private);
            }
            syn::TraitItem::Macro(node) => self.macro_member(&node.attrs, &node.mac, node.span()),
            _ => self.unsupported(item.span(), "unsupported trait member"),
        }
        self.scopes.push(None);
        visit::visit_trait_item(self, item);
        self.scopes.pop();
    }
    fn visit_foreign_item(&mut self, item: &'ast syn::ForeignItem) {
        match item {
            syn::ForeignItem::Fn(n) => self.function(&n.attrs, &n.vis, &n.sig, n.span(), None),
            syn::ForeignItem::Static(n) => {
                self.whole(ItemKind::Static, &n.ident, n.span(), &n.attrs, &n.vis);
            }
            syn::ForeignItem::Type(n) => {
                self.whole(ItemKind::TypeAlias, &n.ident, n.span(), &n.attrs, &n.vis);
            }
            syn::ForeignItem::Macro(node) => self.macro_member(&node.attrs, &node.mac, node.span()),
            _ => self.unsupported(item.span(), "unsupported extern member"),
        }
        visit::visit_foreign_item(self, item);
    }
}
/// A `use` item's outline name: the one local name a single binding
/// introduces, else the first written line of its tree. A path leading to a
/// group still retains the written group label.
fn use_name(text: &str, n: &syn::ItemUse, bindings: &[imports::ImportBinding]) -> String {
    let written = || {
        text[n.tree.span().byte_range()]
            .lines()
            .next()
            .unwrap_or("")
            .trim_end()
            .to_owned()
    };
    if !matches!(n.tree, syn::UseTree::Group(_) | syn::UseTree::Glob(_)) && bindings.len() == 1 {
        if text[n.tree.span().byte_range()].contains(['{', '*']) {
            written()
        } else {
            bindings[0].local_name.clone()
        }
    } else {
        written()
    }
}

/// The written path of each attribute, `::`-joined.
fn attribute_names(attrs: &[syn::Attribute]) -> BTreeSet<String> {
    attrs
        .iter()
        .map(|attr| {
            attr.path()
                .segments
                .iter()
                .map(|segment| segment.ident.to_string())
                .collect::<Vec<_>>()
                .join("::")
        })
        .collect()
}
fn type_head(ty: &syn::Type) -> Option<String> {
    match ty {
        syn::Type::Path(p) => p.path.segments.last().map(|s| s.ident.to_string()),
        syn::Type::Reference(r) => type_head(&r.elem),
        _ => None,
    }
}

/// The `path = "..."` values inside `#[cfg_attr(PREDICATE, ...)]`.
fn conditional_paths(attrs: &[syn::Attribute]) -> Vec<String> {
    use syn::{Meta, Token, punctuated::Punctuated};
    attrs
        .iter()
        .filter(|attribute| attribute.path().is_ident("cfg_attr"))
        .filter_map(|attribute| {
            attribute
                .parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
                .ok()
        })
        .flat_map(|metas| metas.into_iter().skip(1))
        .filter_map(|meta| match meta {
            Meta::NameValue(meta) if meta.path.is_ident("path") => match meta.value {
                syn::Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Str(value),
                    ..
                }) => Some(value.value()),
                _ => None,
            },
            _ => None,
        })
        .collect()
}
/// The value of a `#[path = "..."]` attribute, when it is a string literal.
fn path_attribute(attrs: &[syn::Attribute]) -> Option<String> {
    attrs.iter().find_map(|attribute| match &attribute.meta {
        syn::Meta::NameValue(meta) if meta.path.is_ident("path") => match &meta.value {
            syn::Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Str(value),
                ..
            }) => Some(value.value()),
            _ => None,
        },
        _ => None,
    })
}
