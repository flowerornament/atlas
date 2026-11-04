//! Match across whole owning crates, then restrict reported sites to the chosen scope.
use super::inventory::{Key, Site};
use crate::{Result, source::discovery::Snapshot};
use std::collections::BTreeSet;

pub(super) enum Scope {
    Workspace,
    Crate(String),
    Path(String),
}
impl Scope {
    pub(super) fn of(before: &Snapshot, after: &Snapshot, argument: Option<&str>) -> Result<Self> {
        let Some(argument) = argument else {
            return Ok(Self::Workspace);
        };
        if argument.is_empty() {
            return Err("an empty scope names nothing".into());
        }
        if argument == "." {
            return Ok(Self::Workspace);
        }
        let path = argument.trim_start_matches("./").trim_end_matches('/');
        if before
            .graph()
            .files
            .iter()
            .chain(&after.graph().files)
            .any(|file| file.crate_name == path)
        {
            return Ok(Self::Crate(path.to_owned()));
        }
        let path = super::super::comparison_scope(before, after, path)?;
        if path.is_empty() {
            return Ok(Self::Workspace);
        }
        Ok(Self::Path(path))
    }
    pub(super) fn admits_file(&self, path: &str) -> bool {
        match self {
            Self::Path(scope) => crate::architecture::contains(scope, path),
            _ => true,
        }
    }
    pub(super) fn crates(&self, before: &Snapshot, after: &Snapshot) -> BTreeSet<String> {
        before
            .graph()
            .files
            .iter()
            .chain(&after.graph().files)
            .filter(|file| match self {
                Self::Workspace => true,
                Self::Crate(name) => file.crate_name == *name,
                Self::Path(_) => self.admits_file(&file.path),
            })
            .map(|file| file.crate_name.clone())
            .collect()
    }
    pub(super) fn admits(&self, key: &Key, site: &Site) -> bool {
        match self {
            Self::Workspace => true,
            Self::Crate(name) => key.0 == *name,
            Self::Path(_) => self.admits_file(&site.file),
        }
    }
    pub(super) fn selection(&self) -> Option<&str> {
        match self {
            Self::Workspace => None,
            Self::Crate(name) | Self::Path(name) => Some(name),
        }
    }
}
