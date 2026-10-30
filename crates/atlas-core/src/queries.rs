//! Read-only developer questions over the same lexical SourceGraph used by maps.
use crate::{
    Result, discovery,
    model::{Declaration, Diagnostic, Relationship, Resolution, SourceFile, SourceGraph},
};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write,
};
pub mod navigation;

const BASIS: &str = "Lexical declaration evidence; candidate identity is approximate, not runtime use or ownership.";

#[derive(Debug, Serialize)]
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
    let packages: BTreeSet<_> = files.iter().map(|file| file.crate_name.as_str()).collect();
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
        diagnostics: graph
            .diagnostics
            .iter()
            .filter(|d| {
                paths.contains(&d.file)
                    && d.package
                        .as_deref()
                        .is_none_or(|package| packages.contains(package))
            })
            .count(),
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
pub struct NameCard<'a> {
    pub file: &'a str,
    pub crate_name: &'a str,
    pub declaration: &'a Declaration,
    pub outgoing: Vec<&'a Relationship>,
    pub incoming: Vec<&'a Relationship>,
    pub possible_incoming: Vec<&'a Relationship>,
}
#[derive(Debug, Serialize)]
pub struct Names<'a> {
    pub schema: u32,
    pub name: String,
    pub ambiguous: bool,
    pub matches: Vec<NameCard<'a>>,
    pub basis: &'static str,
}
/// Exact bare name or exact source-site ID; multiple sites stay explicit.
pub fn name<'a>(graph: &'a SourceGraph, name: &str) -> Result<Names<'a>> {
    let mut matches = Vec::new();
    for file in &graph.files {
        for declaration in file
            .declarations
            .iter()
            .filter(|d| !d.test_only && (d.name == name || d.id == name))
        {
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
        return Err(format!("no production declaration matching {name:?}; use census/module or an exact source-site ID").into());
    }
    Ok(Names {
        schema: 1,
        name: name.into(),
        ambiguous: matches.len() > 1,
        matches,
        basis: BASIS,
    })
}
#[derive(Debug, Serialize)]
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
    let paths: BTreeSet<_> = files.iter().map(|f| &f.path).collect();
    let packages: BTreeSet<_> = files.iter().map(|file| file.crate_name.as_str()).collect();
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
        diagnostics: graph
            .diagnostics
            .iter()
            .filter(|d| {
                paths.contains(&d.file)
                    && d.package
                        .as_deref()
                        .is_none_or(|package| packages.contains(package))
            })
            .collect(),
        files,
        basis: BASIS,
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
impl Names<'_> {
    pub fn text(&self, related: bool) -> String {
        let mut out = format!(
            "{}: {} declaration site{}{}\n",
            self.name,
            self.matches.len(),
            if self.matches.len() == 1 { "" } else { "s" },
            if self.ambiguous { " (ambiguous)" } else { "" }
        );
        for card in &self.matches {
            let _ = writeln!(
                out,
                "\n{} [{} {}]",
                card.declaration.id, card.declaration.visibility, card.declaration.kind
            );
            if let Some(summary) = &card.declaration.summary {
                let _ = writeln!(out, "  {summary}");
            }
            let _ = writeln!(
                out,
                "  {} outgoing, {} incoming candidate-unique, {} incoming ambiguous",
                card.outgoing.len(),
                card.incoming.len(),
                card.possible_incoming.len()
            );
            if related {
                let all = card
                    .outgoing
                    .iter()
                    .map(|r| ("outgoing", *r))
                    .chain(card.incoming.iter().map(|r| ("incoming", *r)))
                    .chain(card.possible_incoming.iter().map(|r| ("possible", *r)))
                    .collect::<Vec<_>>();
                for (direction, relation) in all.iter().take(30) {
                    let _ = writeln!(
                        out,
                        "  {direction}: {} --{}--> {} [{:?}]",
                        relation.source,
                        relation.evidence.label(),
                        relation.spelling,
                        relation.target
                    );
                }
                if all.len() > 30 {
                    let _ = writeln!(
                        out,
                        "  ... {} more; --json retains all relationships",
                        all.len() - 30
                    );
                }
            } else {
                let _ = writeln!(
                    out,
                    "  {}",
                    card.declaration.block.lines().next().unwrap_or("")
                );
            }
        }
        let _ = writeln!(out, "\n{}", self.basis);
        out
    }
}
impl ModuleCard<'_> {
    pub fn text(&self) -> String {
        let mut out = format!(
            "{}: {} physical files, {} incident lexical relationships\n",
            self.scope,
            self.files.len(),
            self.relationships.len()
        );
        for file in &self.files {
            let declarations: Vec<_> = file.declarations.iter().filter(|d| !d.test_only).collect();
            let _ = writeln!(
                out,
                "\n{} ({} production declarations)",
                file.path,
                declarations.len()
            );
            for declaration in declarations.iter().take(15) {
                let _ = writeln!(
                    out,
                    "  {}:{} {} {}",
                    file.path, declaration.line, declaration.kind, declaration.name
                );
            }
            if declarations.len() > 15 {
                let _ = writeln!(
                    out,
                    "  ... {} more; --json retains all sites",
                    declarations.len() - 15
                );
            }
        }
        let _ = writeln!(out, "\n{}", self.basis);
        out
    }
}
