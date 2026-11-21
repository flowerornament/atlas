//! Developer questions over captured source: lexical and selected native navigation,
//! source review and documentation audits. Their populations and wire meanings stay
//! distinct; a query never redefines physical membership or source grammar.
use crate::source::{ItemKind, Visibility};
use crate::{
    Result, discovery,
    model::{Declaration, Diagnostic, Relationship, Resolution, SourceFile, SourceGraph},
};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write,
};
pub mod callers;
pub mod documentation;
pub mod navigation;
pub mod orient;
pub mod review;
pub(crate) mod selector;

const BASIS: &str = "Lexical declaration evidence; candidate identity is approximate, not runtime use or ownership.";

#[derive(Debug, Serialize)]
#[non_exhaustive]
pub struct Census {
    pub schema: u32,
    pub scope: String,
    pub files: usize,
    pub test_only_files: usize,
    pub declarations: usize,
    pub kinds: BTreeMap<String, usize>,
    pub crates: BTreeMap<String, usize>,
    pub relationships: usize,
    pub ambiguous: usize,
    pub unresolved: usize,
    pub diagnostics: usize,
    pub basis: &'static str,
}
/// Count the physical population and production declarations, including private items.
pub fn census(graph: &SourceGraph, scope: Option<&str>) -> Result<Census> {
    let files = files(graph, scope)?;
    let paths: BTreeSet<_> = files.iter().map(|f| &f.path).collect();
    let production_paths: BTreeSet<_> = files
        .iter()
        .filter(|file| !file.test_only)
        .map(|file| &file.path)
        .collect();
    let source_ids: BTreeSet<_> = files
        .iter()
        .flat_map(|f| f.declarations.iter().map(|d| &d.id))
        .collect();
    let mut result = Census {
        schema: 1,
        scope: scope.unwrap_or("workspace").into(),
        files: paths.len(),
        test_only_files: paths.difference(&production_paths).count(),
        declarations: 0,
        kinds: BTreeMap::new(),
        crates: BTreeMap::new(),
        relationships: 0,
        ambiguous: 0,
        unresolved: 0,
        diagnostics: diagnostics_of(graph, &files).count(),
        basis: if files.len() == paths.len() {
            BASIS
        } else {
            "Physical file counts; declarations and lexical relationships per package context; approximate identity, not runtime use or ownership."
        },
    };
    for file in files {
        *result.crates.entry(file.crate_name.clone()).or_default() += 1;
        for declaration in file.declarations.iter().filter(|d| !d.test_only) {
            result.declarations += 1;
            *result.kinds.entry(declaration.kind.clone()).or_default() += 1;
        }
    }
    for relation in graph
        .relationships
        .iter()
        .filter(|r| source_ids.contains(&r.source))
    {
        result.relationships += 1;
        match relation.target {
            Resolution::Ambiguous(_) => result.ambiguous += 1,
            Resolution::Unresolved(_) => result.unresolved += 1,
            Resolution::Unique(_) => {}
        }
    }
    Ok(result)
}
#[derive(Debug, Serialize)]
#[non_exhaustive]
pub struct NameCard<'a> {
    pub file: &'a str,
    pub crate_name: &'a str,
    pub declaration: &'a Declaration,
    pub outgoing: Vec<&'a Relationship>,
    pub incoming: Vec<&'a Relationship>,
    pub possible_incoming: Vec<&'a Relationship>,
}
#[derive(Debug, Serialize)]
#[non_exhaustive]
pub struct Names<'a> {
    pub schema: u32,
    /// The selector as given.
    pub query: String,
    pub ambiguous: bool,
    pub matches: Vec<NameCard<'a>>,
    pub basis: &'static str,
}
/// A `NAME`, `MODULE::NAME`, `Type::method` or `FILE:LINE[:COLUMN]`
/// selector, or an exact declaration ID; multiple sites stay explicit. The modules match the end of
/// the declaration's module path: its file's directories and name, then
/// enclosing inline `mod` blocks. This is lexical, not Rust path resolution.
pub fn name<'a>(graph: &'a SourceGraph, name: &str) -> Result<Names<'a>> {
    let selector = selector::Selector::parse(name)?;
    let mut matches = Vec::new();
    for file in &graph.files {
        for declaration in file.declarations.iter().filter(|d| {
            !d.test_only
                && (d.id == name
                    || selector.admits(&d.name, &file.path, d.line, None, || {
                        d.modules.iter().map(String::as_str).collect()
                    })
                    || selector.owns(&d.name, d.self_type.as_deref()))
        }) {
            let mut card = NameCard {
                file: &file.path,
                crate_name: &file.crate_name,
                declaration,
                outgoing: Vec::new(),
                incoming: Vec::new(),
                possible_incoming: Vec::new(),
            };
            for relation in &graph.relationships {
                if relation.source == declaration.id {
                    card.outgoing.push(relation);
                }
                match &relation.target {
                    Resolution::Unique(id) if *id == declaration.id => card.incoming.push(relation),
                    Resolution::Ambiguous(ids) if ids.contains(&declaration.id) => {
                        card.possible_incoming.push(relation);
                    }
                    _ => {}
                }
            }
            matches.push(card);
        }
    }
    if matches.is_empty() {
        return Err(format!("no production declaration matching {name:?}; give NAME, MODULE::NAME (the end of its module path, inline modules included), Type::method, FILE:LINE or a declaration ID").into());
    }
    Ok(Names {
        schema: 1,
        query: name.into(),
        ambiguous: matches.len() > 1,
        matches,
        basis: BASIS,
    })
}
#[derive(Debug, Serialize)]
#[non_exhaustive]
pub struct ModuleCard<'a> {
    pub schema: u32,
    pub scope: String,
    pub files: Vec<&'a SourceFile>,
    pub relationships: Vec<&'a Relationship>,
    pub diagnostics: Vec<&'a Diagnostic>,
    pub basis: &'static str,
}
/// Physical path/module basename or Cargo package; no architecture is inferred.
pub fn module<'a>(graph: &'a SourceGraph, scope: &str) -> Result<ModuleCard<'a>> {
    let files = files(graph, Some(scope))?;
    let ids: BTreeSet<_> = files
        .iter()
        .flat_map(|f| f.declarations.iter().map(|d| &d.id))
        .collect();
    Ok(ModuleCard {
        schema: 1,
        scope: scope.into(),
        relationships: graph
            .relationships
            .iter()
            .filter(|r| {
                ids.contains(&r.source)
                    || match &r.target {
                        Resolution::Unique(id) => ids.contains(id),
                        Resolution::Ambiguous(candidates) => {
                            candidates.iter().any(|id| ids.contains(id))
                        }
                        Resolution::Unresolved(_) => false,
                    }
            })
            .collect(),
        diagnostics: diagnostics_of(graph, &files).collect(),
        files,
        basis: BASIS,
    })
}
/// The diagnostics about `files`: on one of their paths, and either about the
/// physical file or from one of their packages. Under a package scope, a path
/// another package also reads keeps only this package's observations.
pub(crate) fn diagnostics_of<'a>(
    graph: &'a SourceGraph,
    files: &[&SourceFile],
) -> impl Iterator<Item = &'a Diagnostic> {
    let paths: BTreeSet<&str> = files.iter().map(|f| f.path.as_str()).collect();
    let packages: BTreeSet<&str> = files.iter().map(|f| f.crate_name.as_str()).collect();
    graph.diagnostics.iter().filter(move |d| {
        paths.contains(d.file.as_str())
            && d.package
                .as_deref()
                .is_none_or(|package| packages.contains(package))
    })
}
fn files<'a>(graph: &'a SourceGraph, scope: Option<&str>) -> Result<Vec<&'a SourceFile>> {
    if let Some(name) = scope.filter(|s| graph.files.iter().any(|f| f.crate_name == *s)) {
        return Ok(graph
            .files
            .iter()
            .filter(|f| f.crate_name == name)
            .collect());
    }
    let path = discovery::resolve_scope(graph, scope)?;
    let files: Vec<_> = graph
        .files
        .iter()
        .filter(|f| crate::architecture::contains(&path, &f.path))
        .collect();
    if files.is_empty() {
        return Err(format!("scope {path:?} contains no discovered Rust files").into());
    }
    Ok(files)
}
impl Census {
    pub fn text(&self) -> String {
        let mut out = format!(
            "{}: {} files ({} test-only), {} production declarations\n",
            self.scope, self.files, self.test_only_files, self.declarations
        );
        for (kind, count) in &self.kinds {
            let _ = writeln!(out, "  {kind}: {count}");
        }
        let _ = writeln!(
            out,
            "{} lexical relationships; {} ambiguous, {} unresolved; {} diagnostics\n{}",
            self.relationships, self.ambiguous, self.unresolved, self.diagnostics, self.basis
        );
        out
    }
}
/// The last line of a list that left `hidden` rows out: `… 12 more; COMMAND`.
pub(crate) fn cut_line(indent: &str, hidden: usize, command: &str) -> String {
    format!("{indent}… {hidden} more; {command}")
}
/// Where a bounded text list sends the reader for the rows it left out: the
/// command for the whole list (`None`), or for one entry (`Some(id or path)`).
pub type More<'a> = &'a dyn Fn(Option<&str>) -> String;
impl Names<'_> {
    pub fn text(&self, related: bool) -> String {
        self.render(related, usize::MAX, 30, &|_, hidden| {
            format!("  ... {hidden} more; --json retains all relationships")
        })
    }
    /// As `text`, with at most `cards` declarations and `relations`
    /// relationships under each; a cut list ends with `… N more; ` and the
    /// command `more` names.
    pub fn text_bounded(
        &self,
        related: bool,
        cards: usize,
        relations: usize,
        more: More<'_>,
    ) -> String {
        let mut out = self.render(related, cards, relations, &|id, hidden| {
            cut_line(if id.is_some() { "  " } else { "" }, hidden, &more(id))
        });
        // A co-mention is not a call: say where calls are answered.
        if related {
            let at = out.rfind(&format!("\n{}", self.basis)).unwrap_or(out.len());
            out.insert_str(
                at,
                &format!(
                    "\nfor calls, {}\n",
                    crate::suggestion::Command::new("callers").arg(self.query.as_str())
                ),
            );
        }
        out
    }
    fn render(
        &self,
        related: bool,
        cards: usize,
        relations: usize,
        tail: &dyn Fn(Option<&str>, usize) -> String,
    ) -> String {
        // An impl block is not another thing with this name: count it under
        // the declarations, so one type and its impl are not "ambiguous".
        let impls = self
            .matches
            .iter()
            .filter(|card| card.declaration.item_kind() == Some(ItemKind::Impl))
            .count();
        let sites = self.matches.len() - impls;
        let mut out = format!(
            "{}: {sites} declaration site{}{}{}\n",
            self.query,
            if sites == 1 { "" } else { "s" },
            if sites > 1 { " (ambiguous)" } else { "" },
            match impls {
                0 => String::new(),
                1 => ", and 1 impl block".to_owned(),
                n => format!(", and {n} impl blocks"),
            }
        );
        for card in self.matches.iter().take(cards) {
            card.write_text(&mut out, related, relations, tail);
        }
        if self.matches.len() > cards {
            let _ = writeln!(out, "\n{}", tail(None, self.matches.len() - cards));
        }
        let _ = writeln!(out, "\n{}", self.basis);
        out
    }
}
impl NameCard<'_> {
    /// One card: its id and label, summary and relationship counts, then
    /// with `related` up to `relations` relationships, else the first line.
    fn write_text(
        &self,
        out: &mut String,
        related: bool,
        relations: usize,
        tail: &dyn Fn(Option<&str>, usize) -> String,
    ) {
        let declaration = self.declaration;
        let _ = writeln!(out, "\n{} [{}]", declaration.id, label(declaration));
        if let Some(summary) = &declaration.summary {
            let _ = writeln!(out, "  {summary}");
        }
        let _ = writeln!(
            out,
            "  {} outgoing, {} incoming candidate-unique, {} incoming ambiguous",
            self.outgoing.len(),
            self.incoming.len(),
            self.possible_incoming.len()
        );
        if !related {
            let _ = writeln!(out, "  {}", declaration.block.lines().next().unwrap_or(""));
            return;
        }
        let all = self
            .outgoing
            .iter()
            .map(|r| ("outgoing", *r))
            .chain(self.incoming.iter().map(|r| ("incoming", *r)))
            .chain(self.possible_incoming.iter().map(|r| ("possible", *r)))
            .collect::<Vec<_>>();
        for (direction, relation) in all.iter().take(relations) {
            let _ = writeln!(
                out,
                "  {direction}: {} --{}--> {} [{}]",
                relation.source,
                relation.evidence.label(),
                relation.spelling,
                relation.target
            );
        }
        if all.len() > relations {
            let _ = writeln!(
                out,
                "{}",
                tail(Some(&declaration.id), all.len() - relations)
            );
        }
    }
}
/// A card's bracketed label: an impl's keyword, or visibility and kind
/// keywords; a tag Atlas does not know is printed as written.
fn label(declaration: &Declaration) -> String {
    let kind: &str = match declaration.item_kind() {
        Some(kind) => kind.keyword(),
        None => &declaration.kind,
    };
    if declaration.item_kind() == Some(ItemKind::Impl) {
        kind.to_owned()
    } else {
        let visibility: &str = match Visibility::from_tag(&declaration.visibility) {
            Some(visibility) => visibility.keyword(),
            None => &declaration.visibility,
        };
        format!("{visibility} {kind}")
    }
}
impl ModuleCard<'_> {
    pub fn text(&self) -> String {
        self.render(usize::MAX, 15, &|_, hidden| {
            format!("  ... {hidden} more; --json retains all sites")
        })
    }
    /// As `text`, with at most `files` files and `declarations` declarations
    /// under each; a cut list ends with `… N more; ` and the command `more`
    /// names (`Some(path)` for one file's declarations).
    pub fn text_bounded(&self, files: usize, declarations: usize, more: More<'_>) -> String {
        self.render(files, declarations, &|path, hidden| {
            cut_line(if path.is_some() { "  " } else { "" }, hidden, &more(path))
        })
    }
    fn render(
        &self,
        files: usize,
        declarations: usize,
        tail: &dyn Fn(Option<&str>, usize) -> String,
    ) -> String {
        let mut out = format!(
            "{}: {} physical files, {} incident lexical relationships\n",
            self.scope,
            self.files.len(),
            self.relationships.len()
        );
        for file in self.files.iter().take(files) {
            let shown: Vec<_> = file.declarations.iter().filter(|d| !d.test_only).collect();
            let _ = writeln!(
                out,
                "\n{} ({} production declarations)",
                file.path,
                shown.len()
            );
            for declaration in shown.iter().take(declarations) {
                let _ = writeln!(
                    out,
                    "  {}:{} {} {}",
                    file.path, declaration.line, declaration.kind, declaration.name
                );
            }
            if shown.len() > declarations {
                let _ = writeln!(
                    out,
                    "{}",
                    tail(Some(&file.path), shown.len() - declarations)
                );
            }
        }
        if self.files.len() > files {
            let _ = writeln!(out, "\n{}", tail(None, self.files.len() - files));
        }
        let _ = writeln!(out, "\n{}", self.basis);
        out
    }
}
