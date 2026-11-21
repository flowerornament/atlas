//! Export inventory from captured facts and explicitly interpreted project boundaries.
use super::super::super::{CrateMap, FileMap, module_path_of};
use super::super::{PathForm, RootTable, inline_paths, read_path};
use super::ItemRef;
use crate::scan;
use crate::source::{ItemKind, Visibility};
use std::collections::{BTreeMap, BTreeSet};
/// A project wrapper's interpreted boundary, bound to one captured stage root.
#[derive(Clone, Debug, serde::Serialize)]
#[non_exhaustive]
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
        let reader = StageRoot {
            crate_map,
            file,
            module: &module,
            root,
        };
        match reader.declared(declarations) {
            Ok(declared) => {
                let (boundary, unjudged) = reader.boundary(declared);
                unavailable.extend(unjudged);
                boundaries.insert(module, boundary);
            }
            Err(missing) => unavailable.push(missing),
        }
    }
    if unavailable.is_empty() {
        Ok(boundaries)
    } else {
        Err(unavailable)
    }
}

/// One stage's root file, being read for its boundary.
struct StageRoot<'a> {
    crate_map: &'a CrateMap,
    file: &'a FileMap,
    module: &'a str,
    root: &'a RootTable,
}
impl StageRoot<'_> {
    /// The project's boundary declaration for these exact bytes.
    fn declared<'d>(
        &self,
        declarations: &'d BTreeMap<String, BoundaryDeclaration>,
    ) -> Result<&'d BoundaryDeclaration, crate::model::Diagnostic> {
        let (file, module) = (self.file, self.module);
        let maybe = declarations.get(module);
        if let Some(declared) = maybe.filter(|declared| {
            declared.file == file.rel
                && declared.content_fingerprint
                    == crate::discovery::fingerprint(file.source.text().as_bytes())
        }) {
            return Ok(declared);
        }
        let (code, message) = if maybe.is_some() {
            (
                crate::model::DiagnosticCode::BoundaryDeclarationMismatch,
                format!("project boundary declaration for {module} does not match captured source"),
            )
        } else {
            (
                crate::model::DiagnosticCode::BoundaryDeclarationUnavailable,
                format!("project boundary declaration unavailable for {module}"),
            )
        };
        Err(crate::model::Diagnostic {
            package: Some(self.crate_map.name.clone()),
            file: file.rel.clone(),
            line: None,
            code,
            message,
        })
    }
    /// The boundary the root states and exports, and its paths that could
    /// not be judged.
    fn boundary(
        &self,
        declared: &BoundaryDeclaration,
    ) -> (Boundary, Vec<crate::model::Diagnostic>) {
        let file = self.file;
        let mut boundary = Boundary {
            file: file.rel.clone(),
            ..Boundary::default()
        };
        boundary
            .citation_problems
            .clone_from(&declared.citation_problems);
        boundary.listed.clone_from(&declared.listed);
        let mut exports: Vec<(String, usize)> = Vec::new();
        let mut unjudged = Vec::new();
        for item in file.production_items().filter(|item| item.top_level) {
            match item.kind {
                ItemKind::Mod | ItemKind::Impl | ItemKind::MacroRules => {}
                ItemKind::PubUse => {
                    self.re_export(item, &mut boundary, &mut exports, &mut unjudged);
                }
                _ if item.visibility != Visibility::Private => {
                    exports.push((item.name.clone(), item.line));
                    if item.kind == ItemKind::TypeAlias && boundary.listed.contains(&item.name) {
                        self.listed_alias(item, &mut boundary, &mut unjudged);
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
        (boundary, unjudged)
    }
    /// A `pub use`: each binding is an export (a glob is a wildcard), and
    /// a listed one records its home.
    fn re_export(
        &self,
        item: &crate::source::Item,
        boundary: &mut Boundary,
        exports: &mut Vec<(String, usize)>,
        unjudged: &mut Vec<crate::model::Diagnostic>,
    ) {
        let Some(Ok(bindings)) = self
            .file
            .source
            .import_for(item)
            .map(|import| &import.bindings)
        else {
            return;
        };
        for binding in bindings {
            if binding.local_name == "*" {
                boundary.wildcards.push(item.line);
                continue;
            }
            exports.push((binding.local_name.clone(), item.line));
            if boundary.listed.contains(&binding.local_name)
                && let Err(why) = listed_home(boundary, self.module, binding, item.line, self.root)
            {
                unjudged.push(self.unjudged(
                    item.line,
                    format!(
                        "listed boundary path {} is unjudged: {why}",
                        binding.source_path
                    ),
                ));
            }
        }
    }
    /// A listed alias of another stage's item is that item: foreign.
    fn listed_alias(
        &self,
        item: &crate::source::Item,
        boundary: &mut Boundary,
        unjudged: &mut Vec<crate::model::Diagnostic>,
    ) {
        for path in inline_paths(&item.declaration_block, &BTreeSet::new()) {
            match read_path(
                &[self.module],
                path,
                PathForm::Inline,
                self.root,
                &BTreeSet::new(),
            ) {
                Ok(targets) => boundary.foreign.extend(
                    targets
                        .into_iter()
                        .filter(|target| target[0] != self.module)
                        .map(|target| (item.line, item.name.clone(), target.join("::"))),
                ),
                Err(why) => unjudged.push(self.unjudged(
                    item.line,
                    format!("listed boundary path {path} is unjudged: {why}"),
                )),
            }
        }
    }
    fn unjudged(&self, line: usize, message: String) -> crate::model::Diagnostic {
        crate::model::Diagnostic {
            code: crate::model::DiagnosticCode::BoundaryUnjudged,
            package: Some(self.crate_map.name.clone()),
            file: self.file.rel.clone(),
            line: Some(line),
            message,
        }
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
