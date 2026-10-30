//! Export inventory from captured facts and explicitly interpreted project boundaries.
use super::super::super::{CrateMap, module_path_of};
use super::super::{PathForm, RootTable, inline_paths, read_path};
use super::ItemRef;
use crate::scan;
use crate::source::{ItemKind, Visibility};
use std::collections::{BTreeMap, BTreeSet};
/// A project wrapper's interpreted boundary, bound to one captured stage root.
#[derive(Clone, Debug, serde::Serialize)]
pub struct BoundaryDeclaration {
    pub file: String,
    pub content_fingerprint: String,
    pub listed: BTreeSet<String>,
    /// Available citation findings, not unavailable authority disguised as no findings.
    pub citation_problems: Vec<String>,
}
#[derive(Default)]
pub(super) struct Boundary {
    /// The names the `Boundary` paragraph lists.
    pub(super) listed: BTreeSet<String>,
    /// For each listed name the root re-exports, the item at its home, so a
    /// child-path read of that same item is a boundary read too.
    pub(super) listed_homes: BTreeSet<(Vec<String>, String)>,
    /// Root exports the paragraph does not list, at their lines.
    pub(super) unlisted: BTreeMap<String, usize>,
    /// Every name the scanned root declares or re-exports.
    pub(super) exported: BTreeSet<String>,
    /// Lines of wildcard re-exports (`pub use x::*`) in the root.
    pub(super) wildcards: Vec<usize>,
    /// Item-level macro invocations in the root, as (line, macro).
    pub(super) macro_items: Vec<(usize, String)>,
    /// Listed re-exports whose item lives in another stage, as (line, name,
    /// path): a root may list only its own stage's items.
    pub(super) foreign: Vec<(usize, String, String)>,
    /// Why the paragraph's citations do not resolve.
    pub(super) citation_problems: Vec<String>,
    /// The root file.
    pub(super) file: String,
}

impl Boundary {
    /// Whether the item `name`, held by `module`, is a listed boundary item.
    pub(super) fn names(&self, module: &[String], name: &str) -> bool {
        if module.len() == 1 {
            self.listed.contains(name)
        } else {
            self.listed_homes
                .contains(&(module.to_vec(), name.to_owned()))
        }
    }
}

/// Each stage's boundary, read from its root module's header and items.
pub(super) fn boundaries(
    crate_map: &CrateMap,
    stages: &[String],
    root: &RootTable,
    declarations: &BTreeMap<String, BoundaryDeclaration>,
) -> Result<BTreeMap<String, Boundary>, Vec<crate::model::Diagnostic>> {
    let mut boundaries = BTreeMap::new();
    let mut unavailable = Vec::new();
    for file in crate_map.files.iter().filter(|file| !file.test_file) {
        let Some(module) = module_path_of(&crate_map.src_rel, &file.rel) else {
            continue;
        };
        if !stages.contains(&module) {
            continue;
        }
        let mut boundary = Boundary {
            file: file.rel.clone(),
            ..Boundary::default()
        };
        let declared = match declarations.get(&module) {
            Some(declared)
                if declared.file == file.rel
                    && declared.content_fingerprint
                        == crate::discovery::fingerprint(file.source.text().as_bytes()) =>
            {
                declared
            }
            maybe => {
                let message = if maybe.is_some() {
                    format!(
                        "project boundary declaration for {module} does not match captured source"
                    )
                } else {
                    format!("project boundary declaration unavailable for {module}")
                };
                unavailable.push(crate::model::Diagnostic {
                    package: Some(crate_map.name.clone()),
                    file: file.rel.clone(),
                    line: None,
                    message,
                });
                continue;
            }
        };
        boundary
            .citation_problems
            .clone_from(&declared.citation_problems);
        boundary.listed.clone_from(&declared.listed);
        let mut exports: Vec<(String, usize)> = Vec::new();
        for item in file.production_items().filter(|item| item.top_level) {
            match item.kind {
                ItemKind::Mod | ItemKind::Impl | ItemKind::MacroRules => {}
                ItemKind::PubUse => {
                    let Some(import) = file.source.import_at(item.line) else {
                        continue;
                    };
                    let Ok(bindings) = &import.bindings else {
                        continue;
                    };
                    for binding in bindings {
                        if binding.local_name == "*" {
                            boundary.wildcards.push(item.line);
                            continue;
                        }
                        exports.push((binding.local_name.clone(), item.line));
                        if boundary.listed.contains(&binding.local_name)
                            && let Err(why) =
                                listed_home(&mut boundary, &module, binding, item.line, root)
                        {
                            unavailable.push(crate::model::Diagnostic {
                                package: Some(crate_map.name.clone()),
                                file: file.rel.clone(),
                                line: Some(item.line),
                                message: format!(
                                    "listed boundary path {} is unjudged: {why}",
                                    binding.source_path
                                ),
                            });
                        }
                    }
                }
                _ if item.visibility != Visibility::Private => {
                    exports.push((item.name.clone(), item.line));
                    // A listed alias of another stage's item is that item.
                    if item.kind == ItemKind::TypeAlias && boundary.listed.contains(&item.name) {
                        for path in inline_paths(&item.declaration_block, &BTreeSet::new()) {
                            let targets = match read_path(
                                &[module.as_str()],
                                path,
                                PathForm::Inline,
                                root,
                                &BTreeSet::new(),
                            ) {
                                Ok(targets) => targets,
                                Err(why) => {
                                    unavailable.push(crate::model::Diagnostic {
                                        package: Some(crate_map.name.clone()),
                                        file: file.rel.clone(),
                                        line: Some(item.line),
                                        message: format!(
                                            "listed boundary path {path} is unjudged: {why}"
                                        ),
                                    });
                                    continue;
                                }
                            };
                            boundary.foreign.extend(
                                targets
                                    .into_iter()
                                    .filter(|target| target[0] != module)
                                    .map(|target| {
                                        (item.line, item.name.clone(), target.join("::"))
                                    }),
                            );
                        }
                    }
                }
                _ => {}
            }
        }
        boundary.macro_items = file
            .source
            .outline()
            .iter()
            .filter(|item| {
                item.kind == ItemKind::MacroCall
                    && item.top_level
                    && crate::population::TestSelection::Production
                        .admits_line(&file.source, item.line)
            })
            .map(|item| (item.line, item.name.clone()))
            .collect();
        for (name, line) in exports {
            if !boundary.listed.contains(&name) {
                boundary.unlisted.entry(name.clone()).or_insert(line);
            }
            boundary.exported.insert(name);
        }
        boundaries.insert(module, boundary);
    }
    if unavailable.is_empty() {
        Ok(boundaries)
    } else {
        Err(unavailable)
    }
}

/// Record the home of a listed re-export. A uniform path
/// (`pub(crate) use check::X;`) starts at a child of the root.
/// A listed re-export from another stage is recorded as foreign.
fn listed_home(
    boundary: &mut Boundary,
    module: &str,
    binding: &scan::ImportBinding,
    line: usize,
    root: &RootTable,
) -> Result<(), super::super::Unjudged> {
    let source = match binding.source_path.split("::").next() {
        Some("crate" | "super" | "self") => binding.source_path.clone(),
        _ => format!("self::{}", binding.source_path),
    };
    let targets = read_path(&[module], &source, PathForm::Use, root, &BTreeSet::new())?;
    for target in targets {
        if target[0] != module {
            boundary
                .foreign
                .push((line, binding.local_name.clone(), target.join("::")));
            continue;
        }
        let item = ItemRef::of(&target, &root.module_paths);
        if let Some(name) = item.name {
            boundary.listed_homes.insert((item.module, name));
        }
    }
    Ok(())
}
