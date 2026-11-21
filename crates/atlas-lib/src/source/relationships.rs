//! Atlas-derived declaration co-mentions, deliberately approximate by name.
//! `Self` counts as the name of the impl's self type, or of the struct or enum
//! it is written in; a `Self` with neither is a diagnostic.
use crate::{
    model::{Declaration, Diagnostic, Evidence, Relationship, Resolution, SourceFile, SourceGraph},
    scan::{self, ItemKind},
};
use std::collections::{BTreeMap, BTreeSet};

/// Derive mentions without claiming ownership, call order or Rust name resolution.
pub fn derive(graph: &mut SourceGraph) {
    let mut imports = BTreeMap::new();
    for file in &graph.files {
        let mut bindings = Vec::new();
        for statement in &file.imports {
            match scan::import_bindings(statement) {
                Ok(found) => bindings.extend(found),
                Err(error) => graph.diagnostics.push(Diagnostic {
                    code: crate::model::DiagnosticCode::UseUnreadable,
                    package: Some(file.crate_name.clone()),
                    file: file.path.clone(),
                    line: None,
                    message: format!("use syntax could not be read: {error}"),
                }),
            }
        }
        imports.insert(file.path.clone(), bindings);
    }
    derive_with_bindings(graph, &imports, None, MentionScope::Classified);
}

pub(crate) fn derive_captured(
    graph: &mut SourceGraph,
    sources: &BTreeMap<String, std::sync::Arc<crate::source::Source>>,
) {
    let imports = captured_imports(sources, crate::population::TestSelection::Production);
    derive_with_bindings(graph, &imports, None, MentionScope::Classified);
}

pub(crate) fn derive_navigation(
    graph: &mut SourceGraph,
    sources: &BTreeMap<String, std::sync::Arc<crate::source::Source>>,
    registry: &BTreeSet<String>,
) -> BTreeSet<String> {
    let imports = captured_imports(
        sources,
        crate::population::TestSelection::OutsideTestModules,
    );
    derive_with_bindings(graph, &imports, Some(registry), MentionScope::Declarations)
}

fn captured_imports(
    sources: &BTreeMap<String, std::sync::Arc<crate::source::Source>>,
    tests: crate::population::TestSelection,
) -> BTreeMap<String, Vec<scan::ImportBinding>> {
    sources
        .iter()
        .map(|(path, source)| {
            let bindings = source
                .imports()
                .filter(|import| tests.admits_offset(source, import.bytes.start))
                .filter_map(|import| import.bindings.as_ref().ok())
                .flatten()
                .cloned()
                .collect();
            (path.clone(), bindings)
        })
        .collect()
}

#[derive(Clone, Copy)]
enum MentionScope {
    Classified,
    Declarations,
}

fn derive_with_bindings(
    graph: &mut SourceGraph,
    imports: &BTreeMap<String, Vec<scan::ImportBinding>>,
    registry_ids: Option<&BTreeSet<String>>,
    mentions: MentionScope,
) -> BTreeSet<String> {
    let registry = registry(graph, registry_ids);
    let mut derived = Derived::default();
    for file in graph.files.iter().filter(|f| !f.test_only) {
        let shadows = Shadows::of(
            imports.get(&file.path),
            graph.import_roots.get(&file.crate_name),
        );
        if shadows.glob {
            derived.diagnostics.push(Diagnostic {
                code: crate::model::DiagnosticCode::ExternalGlob,
                package: Some(file.crate_name.clone()),
                file: file.path.clone(),
                line: None,
                message:
                    "external glob: name-only relationship resolution suppressed for this file"
                        .into(),
            });
        }
        for item in file.declarations.iter().filter(|d| !d.test_only) {
            derived.item(file, item, &registry, &shadows, mentions);
        }
    }
    let Derived {
        edges,
        mentioned,
        diagnostics,
    } = derived;
    graph.relationships = edges;
    graph.diagnostics.extend(diagnostics);
    graph.diagnostics.sort_by(|a, b| {
        (&a.package, &a.file, a.line, &a.message).cmp(&(&b.package, &b.file, b.line, &b.message))
    });
    graph.diagnostics.dedup_by(|a, b| {
        a.package == b.package && a.file == b.file && a.line == b.line && a.message == b.message
    });
    mentioned
}

/// What a bare name may mean: each production struct, enum or trait (only
/// those in `registry_ids` when given) by name, with its package and id.
type Registry<'a> = BTreeMap<&'a str, Vec<(&'a str, &'a str)>>;

fn registry<'a>(graph: &'a SourceGraph, registry_ids: Option<&BTreeSet<String>>) -> Registry<'a> {
    let mut registry = Registry::new();
    for file in &graph.files {
        for item in file.declarations.iter().filter(|d| {
            !d.test_only
                && matches!(
                    d.item_kind(),
                    Some(ItemKind::Struct | ItemKind::Enum | ItemKind::Trait)
                )
                && registry_ids.is_none_or(|ids| ids.contains(&d.id))
        }) {
            registry
                .entry(&item.name)
                .or_default()
                .push((&file.crate_name, &item.id));
        }
    }
    registry
}

/// The spellings an external import may bind in one file. An import rooted
/// at `crate`, `self`, `super` or a workspace import root is not external.
struct Shadows {
    external: BTreeSet<String>,
    /// An external glob may bind any name.
    glob: bool,
}
impl Shadows {
    fn of(bindings: Option<&Vec<scan::ImportBinding>>, roots: Option<&Vec<String>>) -> Self {
        let mut external = BTreeSet::new();
        let mut glob = false;
        for binding in bindings.into_iter().flatten() {
            let root = scan::identifiers(&binding.source_path).next().unwrap_or("");
            if matches!(root, "crate" | "self" | "super")
                || roots.is_some_and(|names| names.iter().any(|name| name == root))
            {
                continue;
            }
            if binding.local_name == "*" {
                glob = true;
            } else {
                external.insert(binding.local_name.clone());
            }
        }
        Self { external, glob }
    }
    fn may_shadow(&self, name: &str) -> bool {
        self.glob || self.external.contains(name)
    }
}

/// Mentions, mentioned registry names and diagnostics, in derivation order.
#[derive(Default)]
struct Derived {
    edges: Vec<Relationship>,
    mentioned: BTreeSet<String>,
    diagnostics: Vec<Diagnostic>,
}
impl Derived {
    /// Read one production declaration's mentions of registry names. Only a
    /// classified item (fn, struct or enum) records relationships; any other
    /// declaration is read only for the names it mentions.
    fn item(
        &mut self,
        file: &SourceFile,
        item: &Declaration,
        registry: &Registry<'_>,
        shadows: &Shadows,
        mentions: MentionScope,
    ) {
        let classified = matches!(
            item.item_kind(),
            Some(ItemKind::Fn | ItemKind::Struct | ItemKind::Enum)
        );
        if item.item_kind() == Some(ItemKind::PubUse)
            || (!classified && matches!(mentions, MentionScope::Classified))
        {
            return;
        }
        let projection = scan::RustSourceProjection::of(item.block.to_string());
        let code = projection.code_lines().collect::<Vec<_>>().join("\n");
        let self_means = self_meaning(item);
        let mut seen = BTreeSet::new();
        for (text, evidence) in regions(item, &code) {
            if classified && self_means.is_none() && scan::identifiers(text).any(|x| x == "Self") {
                self.diagnostics.push(Diagnostic {
                    code: crate::model::DiagnosticCode::SelfUnresolved,
                    package: Some(file.crate_name.clone()),
                    file: file.path.clone(),
                    line: Some(item.line),
                    message: "Self is not resolved to an enclosing impl type".into(),
                });
            }
            for token in scan::identifiers(text) {
                let name = match (token, self_means) {
                    ("Self", Some(means)) => means,
                    ("Self", None) => continue,
                    _ => token,
                };
                if name == item.name || !seen.insert((name, evidence.label())) {
                    continue;
                }
                let Some(candidates) = registry.get(name) else {
                    continue;
                };
                let shadowed = shadows.may_shadow(name);
                if matches!(mentions, MentionScope::Declarations) && !shadowed {
                    self.mentioned.insert(name.to_owned());
                }
                if !classified {
                    continue;
                }
                let target = if shadowed {
                    Resolution::Unresolved(
                        "external import shadows or may shadow this spelling".into(),
                    )
                } else {
                    candidate_target(candidates, &file.crate_name)
                };
                self.edges.push(Relationship {
                    id: format!("edge-{}", self.edges.len()),
                    source: item.id.clone(),
                    spelling: token.into(),
                    evidence,
                    target,
                });
            }
        }
    }
}

/// The parts of a declaration's code a mention is read from: a fn's
/// parameter and return sides, or a type's whole body.
fn regions<'c>(item: &Declaration, code: &'c str) -> Vec<(&'c str, Evidence)> {
    if item.item_kind() == Some(ItemKind::Fn) {
        let (a, b) = split_signature(code);
        vec![
            (a, Evidence::ParameterMention),
            (b, Evidence::ReturnMention),
        ]
    } else {
        vec![(code, Evidence::TypeBodyMention)]
    }
}

/// `Self` in a struct or enum is that type; in an impl item, the impl's self
/// type, read as if its name were written.
fn self_meaning(item: &Declaration) -> Option<&str> {
    if matches!(item.item_kind(), Some(ItemKind::Struct | ItemKind::Enum)) {
        Some(item.name.as_str())
    } else {
        item.self_type.as_deref()
    }
}

/// The registry candidates for a name, preferring the mentioning package's
/// own: one is unique, several stay ambiguous.
fn candidate_target(candidates: &[(&str, &str)], package: &str) -> Resolution {
    let preferred = prefer_local(candidates.iter(), |(candidate, _)| *candidate == package);
    match preferred.as_slice() {
        [(_, id)] => Resolution::Unique((*id).to_owned()),
        _ => Resolution::Ambiguous(preferred.iter().map(|(_, id)| (*id).to_owned()).collect()),
    }
}

/// Split a fn signature block at the depth-0 `->` into (parameter side,
/// return side). Arrows inside generic bounds or nested fn types
/// (`F: Fn() -> T`) sit at bracket depth > 0 and are skipped.
fn split_signature(block: &str) -> (&str, &str) {
    let bytes = block.as_bytes();
    let mut paren = 0usize;
    let mut angle = 0usize;
    let mut previous = b' ';
    for (index, &byte) in bytes.iter().enumerate() {
        match byte {
            b'(' => paren += 1,
            b')' => paren = paren.saturating_sub(1),
            b'<' => angle += 1,
            b'>' if previous != b'-' => angle = angle.saturating_sub(1),
            b'-' if paren == 0 && angle == 0 && bytes.get(index + 1) == Some(&b'>') => {
                return (&block[..index], &block[index + 2..]);
            }
            _ => {}
        }
        previous = byte;
    }
    (block, "")
}

/// One lexical preference shared by ordinary mentions and normalized export bindings.
pub(crate) fn prefer_local<'a, T: 'a>(
    candidates: impl Iterator<Item = &'a T> + Clone,
    is_local: impl Fn(&T) -> bool,
) -> Vec<&'a T> {
    let local: Vec<_> = candidates
        .clone()
        .filter(|candidate| is_local(candidate))
        .collect();
    if local.is_empty() {
        candidates.collect()
    } else {
        local
    }
}
