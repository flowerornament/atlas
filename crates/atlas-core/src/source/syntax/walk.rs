//! Walk item syntax and retain only owned source facts; never walk inside macro tokens.
use super::{Facts, attributes, fragments, imports};
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
    depth: usize,
    item_ends: BTreeMap<usize, usize>,
    outline: Vec<Item>,
    regions: Vec<RangeInclusive<usize>>,
    gated: BTreeSet<usize>,
    gated_bytes: Vec<std::ops::Range<usize>>,
    module_bytes: Vec<std::ops::Range<usize>>,
    modules: Vec<(String, Option<String>, bool)>,
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
            depth: 0,
            item_ends: BTreeMap::new(),
            outline: Vec::new(),
            regions: Vec::new(),
            gated: BTreeSet::new(),
            gated_bytes: Vec::new(),
            module_bytes: Vec::new(),
            modules: Vec::new(),
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
        let syntax_start = attrs
            .iter()
            .filter(|a| matches!(a.style, syn::AttrStyle::Outer))
            .map(|a| a.span().byte_range().end)
            .max()
            .map_or(span.byte_range().start, |end| self.after_attributes(end));
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
        let previous_end = self
            .item_ends
            .range(..=syntax_start)
            .next_back()
            .map_or(0, |(_, line)| *line);
        let mut first = attr_line;
        let mut outer = !attrs.is_empty();
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
        let attached_start = if first == line
            && syntax_start > 0
            && text[..syntax_start]
                .rsplit('\n')
                .next()
                .is_some_and(|tail| !tail.trim().is_empty())
        {
            span.byte_range().start
        } else {
            self.line_offsets[first - 1]
        };
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
            top_level: self.depth == 0,
            syntax_span: syntax_start..end,
            byte_span: attached_start..end,
            attribute_names: attrs
                .iter()
                .map(|attr| {
                    attr.path()
                        .segments
                        .iter()
                        .map(|segment| segment.ident.to_string())
                        .collect::<Vec<_>>()
                        .join("::")
                })
                .collect(),
            restricted: fragments::restriction(text, vis),
            trait_name: None,
            fields: Vec::new(),
        };
        self.item_ends.insert(end, end_line);
        self.outline.push(item);
        self.outline.len() - 1
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
}
impl<'ast> Visit<'ast> for Walk<'_> {
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
            syn::Item::Struct(n) => {
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
            syn::Item::Enum(n) => {
                self.record(
                    ItemKind::Enum,
                    n.ident.to_string(),
                    n.span(),
                    &n.attrs,
                    &n.vis,
                    None,
                );
            }
            syn::Item::Union(n) => {
                self.record(
                    ItemKind::Union,
                    n.ident.to_string(),
                    n.span(),
                    &n.attrs,
                    &n.vis,
                    None,
                );
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
                self.record(
                    ItemKind::TypeAlias,
                    n.ident.to_string(),
                    n.span(),
                    &n.attrs,
                    &n.vis,
                    None,
                );
            }
            syn::Item::Const(n) => {
                self.record(
                    ItemKind::Const,
                    n.ident.to_string(),
                    n.span(),
                    &n.attrs,
                    &n.vis,
                    None,
                );
            }
            syn::Item::Static(n) => {
                self.record(
                    ItemKind::Static,
                    n.ident.to_string(),
                    n.span(),
                    &n.attrs,
                    &n.vis,
                    None,
                );
            }
            syn::Item::Use(n) => {
                let text = self.projection.text();
                let bindings = imports::item_use_bindings(n);
                let name = if !matches!(n.tree, syn::UseTree::Group(_) | syn::UseTree::Glob(_))
                    && bindings.len() == 1
                {
                    // A path leading to a group still retains the written group label.
                    if text[n.tree.span().byte_range()].contains(['{', '*']) {
                        text[n.tree.span().byte_range()]
                            .lines()
                            .next()
                            .unwrap_or("")
                            .trim_end()
                            .to_owned()
                    } else {
                        bindings[0].local_name.clone()
                    }
                } else {
                    text[n.tree.span().byte_range()]
                        .lines()
                        .next()
                        .unwrap_or("")
                        .trim_end()
                        .to_owned()
                };
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
            syn::Item::Mod(n) => {
                self.record(
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
                if n.content.is_none() && self.depth == 0 {
                    let path = n.attrs.iter().find_map(|a| match &a.meta {
                        syn::Meta::NameValue(meta) if meta.path.is_ident("path") => {
                            match &meta.value {
                                syn::Expr::Lit(expr) => match &expr.lit {
                                    syn::Lit::Str(value) => Some(value.value()),
                                    _ => None,
                                },
                                _ => None,
                            }
                        }
                        _ => None,
                    });
                    self.modules.push((
                        n.ident.to_string(),
                        path,
                        self.gated_bytes
                            .iter()
                            .any(|range| range.contains(&n.span().byte_range().start)),
                    ));
                }
            }
            syn::Item::Impl(n) => {
                let name = type_head(&n.self_ty)
                    .or_else(|| {
                        n.trait_
                            .as_ref()
                            .and_then(|(p, _)| p.segments.last())
                            .map(|p| p.ident.to_string())
                    })
                    .unwrap_or_else(|| "impl".into());
                let i = self.record(
                    ItemKind::Impl,
                    name,
                    n.span(),
                    &n.attrs,
                    &private,
                    Some(n.brace_token.span.open().byte_range().end),
                );
                self.outline[i].trait_name = n
                    .trait_
                    .as_ref()
                    .and_then(|(p, _)| p.segments.last())
                    .map(|p| p.ident.to_string());
            }
            syn::Item::Macro(n) => {
                let kind = if n.mac.path.is_ident("macro_rules") {
                    ItemKind::MacroRules
                } else {
                    ItemKind::MacroCall
                };
                let name = n.ident.as_ref().map_or_else(
                    || self.projection.text()[n.mac.path.span().byte_range()].to_owned(),
                    ToString::to_string,
                );
                self.record(kind, name, n.span(), &n.attrs, &private, None);
            }
            syn::Item::ExternCrate(n) => {
                self.record(
                    ItemKind::Extern,
                    n.ident.to_string(),
                    n.span(),
                    &n.attrs,
                    &n.vis,
                    None,
                );
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
            syn::Item::Verbatim(tokens) => self.unsupported(tokens.span(), "verbatim item"),
            _ => self.unsupported(item.span(), "unknown syn item"),
        }
        self.depth += 1;
        visit::visit_item(self, item);
        self.depth -= 1;
    }
    fn visit_impl_item(&mut self, item: &'ast syn::ImplItem) {
        match item {
            syn::ImplItem::Fn(n) => {
                self.function(&n.attrs, &n.vis, &n.sig, n.span(), Some(&n.block));
            }
            syn::ImplItem::Const(n) => {
                self.record(
                    ItemKind::Const,
                    n.ident.to_string(),
                    n.span(),
                    &n.attrs,
                    &n.vis,
                    None,
                );
            }
            syn::ImplItem::Type(n) => {
                self.record(
                    ItemKind::TypeAlias,
                    n.ident.to_string(),
                    n.span(),
                    &n.attrs,
                    &n.vis,
                    None,
                );
            }
            syn::ImplItem::Macro(node) => self.macro_member(&node.attrs, &node.mac, node.span()),
            _ => self.unsupported(item.span(), "unsupported impl member"),
        }
        self.depth += 1;
        visit::visit_impl_item(self, item);
        self.depth -= 1;
    }
    fn visit_trait_item(&mut self, item: &'ast syn::TraitItem) {
        let private = syn::Visibility::Inherited;
        match item {
            syn::TraitItem::Fn(n) => {
                self.function(&n.attrs, &private, &n.sig, n.span(), n.default.as_ref());
            }
            syn::TraitItem::Const(n) => {
                self.record(
                    ItemKind::Const,
                    n.ident.to_string(),
                    n.span(),
                    &n.attrs,
                    &private,
                    None,
                );
            }
            syn::TraitItem::Type(n) => {
                self.record(
                    ItemKind::TypeAlias,
                    n.ident.to_string(),
                    n.span(),
                    &n.attrs,
                    &private,
                    None,
                );
            }
            syn::TraitItem::Macro(node) => self.macro_member(&node.attrs, &node.mac, node.span()),
            _ => self.unsupported(item.span(), "unsupported trait member"),
        }
        self.depth += 1;
        visit::visit_trait_item(self, item);
        self.depth -= 1;
    }
    fn visit_foreign_item(&mut self, item: &'ast syn::ForeignItem) {
        match item {
            syn::ForeignItem::Fn(n) => self.function(&n.attrs, &n.vis, &n.sig, n.span(), None),
            syn::ForeignItem::Static(n) => {
                self.record(
                    ItemKind::Static,
                    n.ident.to_string(),
                    n.span(),
                    &n.attrs,
                    &n.vis,
                    None,
                );
            }
            syn::ForeignItem::Type(n) => {
                self.record(
                    ItemKind::TypeAlias,
                    n.ident.to_string(),
                    n.span(),
                    &n.attrs,
                    &n.vis,
                    None,
                );
            }
            syn::ForeignItem::Macro(node) => self.macro_member(&node.attrs, &node.mac, node.span()),
            _ => self.unsupported(item.span(), "unsupported extern member"),
        }
        visit::visit_foreign_item(self, item);
    }
}
fn type_head(ty: &syn::Type) -> Option<String> {
    match ty {
        syn::Type::Path(p) => p.path.segments.last().map(|s| s.ident.to_string()),
        syn::Type::Reference(r) => type_head(&r.elem),
        _ => None,
    }
}
