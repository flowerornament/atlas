//! Selected declarations and normalized exports with lexical lookup evidence.
use super::{Basis, Edge, Navigation, Site};
use crate::{
    Result,
    model::Resolution,
    population::{TestSelection, VisibilitySelection},
    source::{ItemKind, Visibility},
};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Serialize)]
pub struct Dump {
    pub schema: u32,
    pub population: VisibilitySelection,
    pub declarations: Vec<Definition>,
    pub exports: Vec<Export>,
    pub edges: Vec<Edge>,
    pub basis: Basis,
}
#[derive(Serialize)]
pub struct Definition {
    pub id: String,
    pub site: Site,
    /// The donor's file-derived home, preserving conventional lib/main root names.
    pub home: String,
}
/// A written pub use at a conventional lib/main root is the native facade surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExportKind {
    Facade,
    Reexport,
}
/// Which written-source heuristic selected the candidates, never a Rust resolution proof.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum BindingEvidence {
    QualifiedSuffix,
    PackageName,
    WorkspaceName,
}
#[derive(Serialize)]
pub struct Export {
    pub id: String,
    pub site: Site,
    pub local_name: String,
    pub source_name: String,
    pub source_path: String,
    pub kind: ExportKind,
    pub evidence: BindingEvidence,
    pub target: Resolution,
}
impl Navigation<'_> {
    pub fn dump(&self) -> Result<Dump> {
        let relations = self.relations()?;
        let packages: BTreeMap<_, _> = self
            .workspace
            .crates
            .iter()
            .map(|p| (p.name.as_str(), p))
            .collect();
        let files: BTreeSet<_> = self
            .workspace
            .crates
            .iter()
            .flat_map(|p| {
                p.files
                    .iter()
                    .filter(|f| !f.test_file)
                    .map(|f| (p.name.as_str(), f.rel.as_str()))
            })
            .collect();
        let mut declarations = Vec::new();
        let mut pending = Vec::new();
        for (id, site) in &relations.sites {
            if !files.contains(&(site.package.as_str(), site.file.as_str()))
                || !self.visibility.admits(site.visibility)
            {
                continue;
            }
            let source = self
                .snapshot
                .source_facts(&site.file)
                .ok_or("captured source absent")?;
            if !TestSelection::OutsideTestModules.admits_line(source, site.line) {
                continue;
            }
            let package = packages
                .get(site.package.as_str())
                .ok_or("source package absent")?;
            if site.kind == ItemKind::PubUse {
                let root = crate::discovery::module_path_of(&package.src_rel, &site.file).is_none();
                pending.push((id, site, root));
            } else if site.kind != ItemKind::Impl {
                declarations.push(Definition {
                    id: id.clone(),
                    site: site.clone(),
                    home: home(&package.name, &package.src_rel, &site.file),
                });
            }
        }
        let mut index = BTreeMap::<&str, Vec<&Definition>>::new();
        for definition in &declarations {
            index
                .entry(&definition.site.name)
                .or_default()
                .push(definition);
        }
        let mut exports = Vec::new();
        for (id, site, root) in pending {
            let source = self
                .snapshot
                .source_facts(&site.file)
                .ok_or("captured source absent")?;
            let bindings = source
                .import_at(site.line)
                .ok_or("captured export absent")?
                .bindings
                .as_ref()
                .map_err(|why| {
                    format!("export unavailable at {}:{}: {why}", site.file, site.line)
                })?;
            for (ordinal, binding) in bindings.iter().enumerate() {
                let (target, evidence) =
                    resolve(&index, &site.package, &binding.source, &binding.source_path);
                exports.push(Export {
                    id: format!("{id}:binding:{ordinal}"),
                    site: site.clone(),
                    local_name: binding.local_name.clone(),
                    source_name: binding.source.clone(),
                    source_path: binding.source_path.clone(),
                    kind: if site.visibility == Visibility::Pub && root {
                        ExportKind::Facade
                    } else {
                        ExportKind::Reexport
                    },
                    evidence,
                    target,
                });
            }
        }
        let selected: BTreeSet<_> = declarations
            .iter()
            .map(|d| {
                (
                    d.site.package.as_str(),
                    d.site.file.as_str(),
                    d.site.line,
                    d.site.name.as_str(),
                )
            })
            .collect();
        let edges = Self::edges(relations, |site, _| {
            selected.contains(&(
                site.package.as_str(),
                site.file.as_str(),
                site.line,
                site.name.as_str(),
            ))
        })?;
        let basis = self.site_basis(
            relations,
            declarations.iter().map(|declaration| &declaration.site),
        );
        declarations.sort_by(|a, b| {
            (&a.site.file, a.site.line, &a.site.name, &a.site.package).cmp(&(
                &b.site.file,
                b.site.line,
                &b.site.name,
                &b.site.package,
            ))
        });
        exports.sort_by(|a, b| {
            (
                &a.site.file,
                a.site.line,
                &a.local_name,
                &a.site.package,
                &a.id,
            )
                .cmp(&(
                    &b.site.file,
                    b.site.line,
                    &b.local_name,
                    &b.site.package,
                    &b.id,
                ))
        });
        Ok(Dump {
            schema: 1,
            population: self.visibility,
            declarations,
            exports,
            edges,
            basis,
        })
    }
}
fn home(package: &str, source_directory: &str, file: &str) -> String {
    let path = file
        .strip_prefix(source_directory)
        .and_then(|rest| rest.strip_prefix('/'))
        .unwrap_or(file);
    let stem = path.strip_suffix(".rs").unwrap_or(path);
    let module = stem.strip_suffix("/mod").unwrap_or(stem).replace('/', "::");
    format!("{package}::{module}")
}
fn qualified_hint(definition: &Definition) -> String {
    let home = definition
        .home
        .strip_suffix("::lib")
        .or_else(|| definition.home.strip_suffix("::main"))
        .unwrap_or(&definition.home)
        .replace('-', "_");
    format!("{home}::{}", definition.site.name)
}
fn resolve(
    index: &BTreeMap<&str, Vec<&Definition>>,
    package: &str,
    name: &str,
    source_path: &str,
) -> (Resolution, BindingEvidence) {
    let Some(candidates) = index.get(name) else {
        return (
            Resolution::Unresolved(format!("no selected declaration named `{name}`")),
            BindingEvidence::WorkspaceName,
        );
    };
    if source_path.contains("::") {
        let suffix = source_path
            .trim_start_matches("::")
            .trim_start_matches("crate::")
            .trim_start_matches("self::")
            .trim_start_matches("super::")
            .replace('-', "_");
        let qualified: Vec<_> = candidates
            .iter()
            .copied()
            .filter(|candidate| qualified_hint(candidate).ends_with(&suffix))
            .collect();
        if !qualified.is_empty() {
            return (resolution(&qualified), BindingEvidence::QualifiedSuffix);
        }
    }
    let preferred = crate::relationships::prefer_local(candidates.iter().copied(), |candidate| {
        candidate.site.package == package
    });
    let evidence = if preferred
        .first()
        .is_some_and(|candidate| candidate.site.package == package)
    {
        BindingEvidence::PackageName
    } else {
        BindingEvidence::WorkspaceName
    };
    (resolution(&preferred), evidence)
}
fn resolution(candidates: &[&Definition]) -> Resolution {
    match candidates {
        [only] => Resolution::Unique(only.id.clone()),
        [] => Resolution::Unresolved("no selected declaration candidate".into()),
        _ => Resolution::Ambiguous(
            candidates
                .iter()
                .map(|candidate| candidate.id.clone())
                .collect(),
        ),
    }
}
