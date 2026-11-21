//! Every path written in production code, as a site, resolved through the
//! qualified index: where it leads in each production scope of its file,
//! and what binds its first segment. A reader of an item or module counts
//! the sites that lead into it.
//!
//! A site is a `use` tree leaf (`use a::{b, c}` is two), or a whole written
//! path in an expression, type, pattern, bound, impl header or macro name
//! (`crate::parse::Tree::new` is one; a path in its generic arguments is
//! its own). `Self`, method calls and field access are not paths, and
//! attributes are not read. A macro's input is read as code when it parses
//! as comma-separated expressions (form `macro-argument`, a syntax
//! assumption); otherwise each `a::b` it spells is a `macro-text` site,
//! which a reader may see but cannot trust. Expansions are not seen.
use super::Index;
use crate::{
    analysis::sources::Sources, population::SyntaxSelection, source::Source, workflow::facts,
};
use proc_macro2::TokenTree;
use std::collections::BTreeMap;
use syn::{
    punctuated::Punctuated,
    spanned::Spanned,
    visit::{self, Visit},
};

/// Every production path site, with each production file's module scopes
/// (`crate::module` paths), from one index.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct PathReading {
    pub sites: Vec<PathSite>,
    pub scopes: BTreeMap<String, Vec<Vec<String>>>,
}

/// How a site is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum SiteForm {
    Use,
    Path,
    MacroArgument,
    MacroText,
}

/// One written path at one production site.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct PathSite {
    pub file: String,
    pub line: usize,
    pub column: usize,
    /// The site's first byte.
    pub start: usize,
    pub written: String,
    pub form: SiteForm,
    /// Where the path leads in each production scope of its file (`None`
    /// where it is ambiguous), after re-exports, glob re-exports and type
    /// aliases.
    pub targets: Vec<Option<Vec<String>>>,
    /// Where the import or glob that binds the path's first segment leads,
    /// in the first scope, when one does: a later use of an imported name
    /// is the import's reading, not a new one.
    pub through: Option<Vec<String>>,
    /// For a path ambiguous in some scope: what each competing binding of
    /// its first segment would make it.
    pub candidates: Vec<Vec<String>>,
}

impl Index {
    /// Each production file's module scopes.
    pub(in crate::analysis) fn file_scopes(&self) -> BTreeMap<String, Vec<Vec<String>>> {
        self.scopes.clone()
    }
    /// The production path sites of every file this index mounts.
    pub(in crate::analysis) fn path_sites(
        &self,
        sources: &Sources,
    ) -> crate::Result<Vec<PathSite>> {
        let mut out = Vec::new();
        for (file, scopes) in &self.scopes {
            let Some(cached) = sources.get(file) else {
                continue;
            };
            let ast = cached.ast(file)?;
            let mut reader = Reader {
                index: self,
                file,
                source: cached.source(),
                scopes,
                module: Vec::new(),
                imports: facts::imports(&ast.items, Self::production(cached.source())),
                out: &mut out,
            };
            reader.visit_file(ast);
        }
        Ok(out)
    }
    fn production(source: &Source) -> SyntaxSelection<'_> {
        SyntaxSelection::Selected {
            source,
            tests: crate::population::TestSelection::Production,
        }
    }
    /// One written path resolved in every scope.
    fn site(
        &self,
        scopes: &[Vec<String>],
        imports: &BTreeMap<String, Vec<Vec<String>>>,
        path: &[String],
    ) -> Reading {
        let resolve = |scope: &[String], path: &[String]| {
            self.in_scope(scope, imports, path)
                .and_then(|p| self.through_doors(p))
        };
        let targets: Vec<_> = scopes.iter().map(|scope| resolve(scope, path)).collect();
        let through = scopes
            .first()
            .filter(|scope| self.binds_first(scope, imports, path))
            .and_then(|scope| resolve(scope, &path[..1]));
        let candidates = if targets.iter().all(Option::is_some) {
            Vec::new()
        } else {
            self.candidates(scopes, imports, path)
        };
        Reading {
            targets,
            through,
            candidates,
        }
    }
    /// Whether a path's first segment is bound by something that is itself
    /// a site (an import, a glob, or a type alias declared here), so a
    /// path through it repeats that site's reading. A declared module or
    /// item is not such a binding.
    fn binds_first(
        &self,
        scope: &[String],
        imports: &BTreeMap<String, Vec<Vec<String>>>,
        path: &[String],
    ) -> bool {
        let Some(first) = path.first().filter(|first| !keyword(first)) else {
            return false;
        };
        let mut alias = scope.to_vec();
        alias.push(first.clone());
        imports.contains_key(first)
            || self.aliases.contains_key(&alias)
            || (imports.contains_key("*") && !self.declares_name(scope, first))
    }
    /// Each binding the first segment of an ambiguous path could take.
    fn candidates(
        &self,
        scopes: &[Vec<String>],
        imports: &BTreeMap<String, Vec<Vec<String>>>,
        path: &[String],
    ) -> Vec<Vec<String>> {
        let Some(first) = path.first() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for scope in scopes {
            let bindings = imports.get(first).into_iter().flatten().cloned();
            let globs = imports.get("*").into_iter().flatten().map(|glob| {
                let mut module = glob.clone();
                module.push(first.clone());
                module
            });
            for binding in bindings.chain(globs) {
                let mut candidate = self.relative(scope, &binding);
                candidate.extend_from_slice(&path[1..]);
                if !out.contains(&candidate) {
                    out.push(candidate);
                }
            }
        }
        out
    }
}

/// One path's resolution: per scope, the binding of its first segment,
/// and its candidates when ambiguous (as on [`PathSite`]).
struct Reading {
    targets: Vec<Option<Vec<String>>>,
    through: Option<Vec<String>>,
    candidates: Vec<Vec<String>>,
}

fn keyword(segment: &str) -> bool {
    matches!(segment, "crate" | "self" | "super" | "Self")
}

struct Reader<'a> {
    index: &'a Index,
    file: &'a str,
    source: &'a Source,
    /// The file's production scopes.
    scopes: &'a [Vec<String>],
    /// The inline modules around the current item.
    module: Vec<String>,
    /// The bindings in force here: the module's, then each enclosing
    /// block's own `use` items.
    imports: BTreeMap<String, Vec<Vec<String>>>,
    out: &'a mut Vec<PathSite>,
}

impl Reader<'_> {
    fn selection(&self) -> SyntaxSelection<'_> {
        Index::production(self.source)
    }
    fn record(&mut self, span: proc_macro2::Span, form: SiteForm, path: &[String]) {
        if path.is_empty() || path[0] == "Self" || !self.selection().admits(span) {
            return;
        }
        let scopes: Vec<Vec<String>> = self
            .scopes
            .iter()
            .map(|scope| scope.iter().chain(&self.module).cloned().collect())
            .collect();
        let Reading {
            targets,
            through,
            candidates,
        } = self.index.site(&scopes, &self.imports, path);
        let at = span.start();
        self.out.push(PathSite {
            file: self.file.to_owned(),
            line: at.line,
            column: at.column,
            start: span.byte_range().start,
            written: path.join("::"),
            form,
            targets,
            through,
            candidates,
        });
    }
    /// Read `items` as a nested scope: `module` (an inline module) or a
    /// block, whose own `use` items bind names inside it.
    fn nested(
        &mut self,
        module: Option<String>,
        items: &[syn::Item],
        read: impl FnOnce(&mut Self),
    ) {
        let outer = self.imports.clone();
        let local = facts::imports(items, self.selection());
        if module.is_some() {
            self.imports = local;
        } else {
            self.imports.extend(local);
        }
        let pushed = module.map(|name| self.module.push(name)).is_some();
        read(self);
        if pushed {
            self.module.pop();
        }
        self.imports = outer;
    }
    /// A macro's input: comma-separated expressions are read as code;
    /// anything else yields one site per `a::b` it spells.
    fn macro_input(&mut self, mac: &syn::Macro) {
        let parser = Punctuated::<syn::Expr, syn::Token![,]>::parse_terminated;
        if let Ok(arguments) = mac.parse_body_with(parser) {
            for argument in &arguments {
                let start = self.out.len();
                self.visit_expr(argument);
                for site in &mut self.out[start..] {
                    site.form = SiteForm::MacroArgument;
                }
            }
            return;
        }
        let tokens: Vec<TokenTree> = mac.tokens.clone().into_iter().collect();
        for (span, path) in spelled_paths(&tokens) {
            self.record(span, SiteForm::MacroText, &path);
        }
    }
}

/// Each `a::b::c` (and `$crate::a`) spelled in a token stream, through its
/// groups.
fn spelled_paths(tokens: &[TokenTree]) -> Vec<(proc_macro2::Span, Vec<String>)> {
    let mut out = Vec::new();
    let mut index = 0;
    while index < tokens.len() {
        if let TokenTree::Group(group) = &tokens[index] {
            out.extend(spelled_paths(
                &group.stream().into_iter().collect::<Vec<_>>(),
            ));
            index += 1;
            continue;
        }
        let (path, next) = path_at(tokens, index);
        if path.len() > 1 {
            out.push((tokens[index].span(), path));
            index = next;
        } else {
            index += 1;
        }
    }
    out
}
/// The `ident (:: ident)*` path starting at `index`, `$crate` read as
/// `crate`, and the index after it.
fn path_at(tokens: &[TokenTree], mut index: usize) -> (Vec<String>, usize) {
    let mut path = Vec::new();
    if matches!(&tokens[index], TokenTree::Punct(p) if p.as_char() == '$') {
        index += 1;
    }
    while let Some(TokenTree::Ident(ident)) = tokens.get(index) {
        path.push(ident.to_string());
        let colons = matches!(
            (tokens.get(index + 1), tokens.get(index + 2)),
            (Some(TokenTree::Punct(a)), Some(TokenTree::Punct(b))) if a.as_char() == ':' && b.as_char() == ':'
        );
        index += 1;
        if !colons {
            break;
        }
        index += 2;
    }
    (path, index)
}

impl<'ast> Visit<'ast> for Reader<'_> {
    fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
        if let Some((_, items)) = &item.content {
            self.nested(Some(item.ident.to_string()), items, |reader| {
                for item in items {
                    reader.visit_item(item);
                }
            });
        }
    }
    fn visit_block(&mut self, block: &'ast syn::Block) {
        let items: Vec<syn::Item> = block
            .stmts
            .iter()
            .filter_map(|stmt| match stmt {
                syn::Stmt::Item(item) => Some(item.clone()),
                _ => None,
            })
            .collect();
        self.nested(None, &items, |reader| visit::visit_block(reader, block));
    }
    fn visit_item_use(&mut self, item: &'ast syn::ItemUse) {
        let mut leaves = Vec::new();
        use_leaves(&item.tree, &mut Vec::new(), &mut leaves);
        for (span, path) in leaves {
            self.record(span, SiteForm::Use, &path);
        }
    }
    fn visit_path(&mut self, path: &'ast syn::Path) {
        let written: Vec<String> = path.segments.iter().map(|s| s.ident.to_string()).collect();
        self.record(path.span(), SiteForm::Path, &written);
        visit::visit_path(self, path);
    }
    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        self.visit_path(&mac.path);
        self.macro_input(mac);
    }
    fn visit_attribute(&mut self, _: &'ast syn::Attribute) {}
}

/// Each leaf of a `use` tree as a full path: a name, a rename (its source
/// path), `self` (the module), or a glob (the module it reads).
fn use_leaves(
    tree: &syn::UseTree,
    prefix: &mut Vec<String>,
    out: &mut Vec<(proc_macro2::Span, Vec<String>)>,
) {
    match tree {
        syn::UseTree::Path(path) => {
            prefix.push(path.ident.to_string());
            use_leaves(&path.tree, prefix, out);
            prefix.pop();
        }
        syn::UseTree::Name(name) if name.ident == "self" => out.push((name.span(), prefix.clone())),
        syn::UseTree::Name(name) => out.push((name.span(), with(prefix, &name.ident))),
        syn::UseTree::Rename(rename) => out.push((rename.span(), with(prefix, &rename.ident))),
        syn::UseTree::Glob(glob) => out.push((glob.span(), prefix.clone())),
        syn::UseTree::Group(group) => {
            for tree in &group.items {
                use_leaves(tree, prefix, out);
            }
        }
    }
}
fn with(prefix: &[String], ident: &syn::Ident) -> Vec<String> {
    prefix.iter().cloned().chain([ident.to_string()]).collect()
}
