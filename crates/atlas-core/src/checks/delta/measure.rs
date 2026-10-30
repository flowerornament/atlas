//! Native accounting units and declared normal dependencies from captured Cargo metadata.
use super::super::Workspace;
use crate::{
    Result,
    discovery::Snapshot,
    population::TestSelection,
    source::{ItemKind, Visibility},
};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Serialize)]
pub struct CrateMeasure {
    pub token_lines: usize,
    pub tokens: usize,
    pub surface: usize,
}
#[derive(Serialize)]
pub struct Measures {
    pub crates: BTreeMap<String, CrateMeasure>,
    pub dependencies: BTreeMap<String, BTreeSet<String>>,
    pub sources: BTreeMap<String, String>,
}
/// Read each native package-file context. Shared physical paths retain each package's count.
pub fn read(snapshot: &Snapshot) -> Result<Measures> {
    read_workspace(snapshot, &Workspace::of(snapshot)?)
}
pub(in crate::checks) fn read_workspace(
    snapshot: &Snapshot,
    workspace: &Workspace,
) -> Result<Measures> {
    let mut crates = BTreeMap::new();
    let mut sources = BTreeMap::new();
    for package in &workspace.crates {
        let mut size = 0;
        let mut tokens = 0;
        let mut surface = 0;
        if package.files.is_empty() {
            return Err(format!("{} has no captured source-tree population", package.name).into());
        }
        if let Some(issue) = workspace
            .unavailable
            .iter()
            .find(|issue| crate::architecture::contains(&package.src_rel, &issue.file))
        {
            return Err(format!(
                "required source unavailable: {}: {}",
                issue.file, issue.message
            )
            .into());
        }
        for file in &package.files {
            sources.insert(
                file.rel.clone(),
                crate::discovery::fingerprint(file.source.text().as_bytes()),
            );
            size += file.token_lines();
            tokens += file.token_count()?;
            for item in file
                .items(TestSelection::OutsideTestModules)
                .filter(|item| item.visibility == Visibility::Pub && item.kind != ItemKind::Impl)
            {
                surface += if item.kind == ItemKind::PubUse {
                    file.source
                        .import_at(item.line)
                        .ok_or_else(|| {
                            format!(
                                "public surface lacks captured import {}:{}",
                                file.rel, item.line
                            )
                        })?
                        .bindings
                        .as_ref()
                        .map_err(|why| {
                            format!(
                                "public surface syntax unavailable {}:{}: {why}",
                                file.rel, item.line
                            )
                        })?
                        .len()
                } else {
                    1
                };
            }
        }
        crates.insert(
            package.name.clone(),
            CrateMeasure {
                token_lines: size,
                tokens,
                surface,
            },
        );
    }
    let members: BTreeSet<_> = snapshot
        .metadata()
        .workspace_packages()
        .into_iter()
        .map(|package| package.name.as_str())
        .collect();
    let dependencies = snapshot
        .metadata()
        .workspace_packages()
        .into_iter()
        .map(|package| {
            let targets = package
                .dependencies
                .iter()
                .filter(|dependency| {
                    dependency.kind == cargo_metadata::DependencyKind::Normal
                        && members.contains(dependency.name.as_str())
                })
                .map(|dependency| dependency.name.clone())
                .collect();
            (package.name.to_string(), targets)
        })
        .collect();
    Ok(Measures {
        crates,
        dependencies,
        sources,
    })
}
