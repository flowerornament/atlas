//! Atlas-derived declaration co-mentions, deliberately approximate by name.
use crate::{
    model::{Diagnostic, Evidence, Relationship, Resolution, SourceGraph},
    scan::{self, ItemKind},
};
use std::collections::{BTreeMap, BTreeSet};

/// Derive mentions without claiming ownership, call order or Rust name resolution.
pub fn derive(graph: &mut SourceGraph) {
    let imports = graph
        .files
        .iter()
        .map(|file| {
            let bindings = file
                .imports
                .iter()
                .flat_map(|import| scan::import_bindings(import))
                .collect();
            (file.path.clone(), bindings)
        })
        .collect();
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
                .filter(|import| tests.admits_line(source, import.line))
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
    let mut registry: BTreeMap<&str, Vec<(&str, &str)>> = BTreeMap::new();
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
    let mut edges = Vec::new();
    let mut mentioned = BTreeSet::new();
    let mut diagnostics = Vec::new();
    for file in graph.files.iter().filter(|f| !f.test_only) {
        let mut external = BTreeSet::new();
        let mut glob = false;
        let roots = graph.import_roots.get(&file.crate_name);
        for binding in imports.get(&file.path).into_iter().flatten() {
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
        if glob {
            diagnostics.push(Diagnostic {
                package: Some(file.crate_name.clone()),
                file: file.path.clone(),
                line: None,
                message:
                    "external glob: name-only relationship resolution suppressed for this file"
                        .into(),
            });
        }
        for item in file.declarations.iter().filter(|d| !d.test_only) {
            let classified = matches!(
                item.item_kind(),
                Some(ItemKind::Fn | ItemKind::Struct | ItemKind::Enum)
            );
            if item.item_kind() == Some(ItemKind::PubUse)
                || (!classified && matches!(mentions, MentionScope::Classified))
            {
                continue;
            }
            let projection = scan::RustSourceProjection::of(item.block.to_string());
            let code = projection.code_lines().collect::<Vec<_>>().join("\n");
            let regions = if item.item_kind() == Some(ItemKind::Fn) {
                let (a, b) = split_signature(&code);
                vec![
                    (a, Evidence::ParameterMention),
                    (b, Evidence::ReturnMention),
                ]
            } else {
                vec![(code.as_str(), Evidence::TypeBodyMention)]
            };
            let mut seen = BTreeSet::new();
            for (text, evidence) in regions {
                if classified && scan::identifiers(text).any(|x| x == "Self") {
                    diagnostics.push(Diagnostic {
                        package: Some(file.crate_name.clone()),
                        file: file.path.clone(),
                        line: Some(item.line),
                        message: "Self is not resolved to an enclosing impl type".into(),
                    });
                }
                for token in scan::identifiers(text) {
                    if token == item.name || !seen.insert((token, evidence.label())) {
                        continue;
                    }
                    let Some(candidates) = registry.get(token) else {
                        continue;
                    };
                    if matches!(mentions, MentionScope::Declarations)
                        && !glob
                        && !external.contains(token)
                    {
                        mentioned.insert(token.to_owned());
                    }
                    if !classified {
                        continue;
                    }
                    let target = if glob || external.contains(token) {
                        Resolution::Unresolved(
                            "external import shadows or may shadow this spelling".into(),
                        )
                    } else {
                        let preferred = prefer_local(candidates.iter(), |(package, _)| {
                            *package == file.crate_name
                        });
                        match preferred.as_slice() {
                            [(_, id)] => Resolution::Unique((*id).to_owned()),
                            _ => Resolution::Ambiguous(
                                preferred.iter().map(|(_, id)| (*id).to_owned()).collect(),
                            ),
                        }
                    };
                    edges.push(Relationship {
                        id: format!("edge-{}", edges.len()),
                        source: item.id.clone(),
                        spelling: token.into(),
                        evidence,
                        target,
                    });
                }
            }
        }
    }
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
