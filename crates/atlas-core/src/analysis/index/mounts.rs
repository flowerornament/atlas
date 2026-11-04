//! Cargo target and module contexts for each captured physical source.
//! A file may have several mounts; declared boundaries require exactly one owner.
use super::Boundary;
use crate::{
    Result,
    architecture::Architecture,
    source::{discovery, model::SourceGraph, population::TestSelection},
};
use cargo_metadata::{Metadata, TargetKind};
use std::collections::{BTreeMap, BTreeSet};

pub(super) struct Mounts {
    pub(super) crates: BTreeSet<String>,
    pub(super) files: BTreeMap<String, Vec<(Vec<String>, bool)>>,
}
impl Mounts {
    pub(super) fn read(
        metadata: &Metadata,
        graph: &SourceGraph,
        tests: TestSelection,
    ) -> Result<Self> {
        let root = metadata.workspace_root.as_std_path();
        let mut crates = BTreeSet::new();
        let mut files = BTreeMap::new();
        for package in metadata.workspace_packages() {
            let library = package
                .targets
                .iter()
                .find(|t| {
                    t.kind.iter().any(|k| {
                        matches!(
                            k,
                            TargetKind::Lib | TargetKind::RLib | TargetKind::ProcMacro
                        )
                    })
                })
                .map_or_else(
                    || package.name.replace('-', "_"),
                    |t| t.name.replace('-', "_"),
                );
            if !crates.insert(library.clone()) {
                return Err(format!("Cargo library namespace {library} belongs to multiple packages; qualified lookup cannot choose one").into());
            }
            let mut targets = BTreeMap::<String, Vec<(String, bool, String)>>::new();
            for target in &package.targets {
                let path = discovery::target_key(root, target.src_path.as_std_path());
                let binary = target
                    .kind
                    .iter()
                    .any(|kind| matches!(kind, TargetKind::Bin));
                let label = discovery::target_label(target);
                let namespace = if target.kind.iter().any(|kind| {
                    matches!(
                        kind,
                        TargetKind::Lib | TargetKind::RLib | TargetKind::ProcMacro
                    )
                }) {
                    library.clone()
                } else {
                    format!("{library}#{label}")
                };
                targets
                    .entry(path)
                    .or_default()
                    .push((namespace, binary, label));
            }
            for file in graph
                .files
                .iter()
                .filter(|f| f.crate_name == package.name.as_str() && tests.admits_file(f.test_only))
            {
                let mounts: Vec<_> = file
                    .mounts
                    .iter()
                    .filter(|mount| tests.admits_file(mount.test_only))
                    .flat_map(|mount| {
                        targets
                            .get(&mount.root)
                            .into_iter()
                            .flatten()
                            .filter(|(_, _, label)| {
                                mount.target.as_ref().is_none_or(|target| target == label)
                            })
                            .map(|(namespace, binary, _)| {
                                let mut scope = vec![namespace.clone()];
                                scope.extend(mount.modules.clone());
                                (scope, *binary && mount.modules.is_empty())
                            })
                    })
                    .collect();
                files
                    .entry(file.path.clone())
                    .or_insert_with(Vec::new)
                    .extend(mounts);
            }
        }
        for mounts in files.values_mut() {
            mounts.sort();
            mounts.dedup();
        }
        Ok(Self { crates, files })
    }
    pub(super) fn boundaries(&self, architecture: &Architecture) -> Result<Vec<Boundary>> {
        let mut boundaries = Vec::new();
        for module in &architecture.modules {
            let root_path = if module.directory {
                format!("{}/mod.rs", module.path)
            } else {
                module.path.clone()
            };
            let mounts = self.files.get(&root_path).ok_or_else(|| {
                format!(
                    "program boundary {} needs a mounted Rust root at {root_path}",
                    module.name
                )
            })?;
            let [(scope, _)] = mounts.as_slice() else {
                return Err(format!(
                    "program boundary {} has {} Cargo module mounts; cannot choose an owner",
                    module.name,
                    mounts.len()
                )
                .into());
            };
            boundaries.push(Boundary {
                module: module.clone(),
                path: scope.clone(),
            });
        }
        Ok(boundaries)
    }
}
