//! The adapter sees source identities, module mounts and selected constant syntax.
use crate::{
    Result,
    discovery::fingerprint,
    model::SourceGraph,
    representations::{Constant, Facts},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    path::{Component, Path},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputFile {
    pub path: String,
    pub fingerprint: String,
    pub modules: Vec<Vec<String>>,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub id: String,
    pub files: Vec<InputFile>,
    pub constants: Vec<Constant>,
}
fn relative(path: &str) -> bool {
    !path.is_empty()
        && !path.contains('\\')
        && !path.chars().any(char::is_control)
        && Path::new(path)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
}
impl Snapshot {
    /// Select already extracted constants. No source walk or Rust parse happens here.
    pub fn from_facts(graph: &SourceGraph, facts: &Facts, source: &str) -> Result<Self> {
        if graph.files.len() != graph.physical_files().count() {
            return Err("declaration adapter cannot represent a physical file shared by Cargo packages; select an unambiguous package input".into());
        }
        let files = graph
            .files
            .iter()
            .filter(|f| !f.test_only)
            .map(|f| InputFile {
                path: f.path.clone(),
                fingerprint: f.content_fingerprint.clone(),
                modules: f
                    .mounts
                    .iter()
                    .filter(|mount| !mount.test_only)
                    .map(|m| m.modules.clone())
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect(),
            })
            .collect::<Vec<_>>();
        let recorded = facts
            .fingerprints
            .iter()
            .map(|(p, h)| (p.as_str(), h.as_str()))
            .collect::<BTreeSet<_>>();
        if recorded.len() != facts.fingerprints.len()
            || files.len() != recorded.len()
            || files
                .iter()
                .any(|f| !recorded.contains(&(f.path.as_str(), f.fingerprint.as_str())))
        {
            return Err("source facts and discovery have different file snapshots".into());
        }
        let constants = facts
            .constants
            .iter()
            .filter(|c| c.site.path == source)
            .cloned()
            .collect();
        let mut input = Self {
            id: String::new(),
            files,
            constants,
        };
        input.id = input.identity()?;
        input.validate_structure()?;
        Ok(input)
    }
    fn identity(&self) -> Result<String> {
        Ok(fingerprint(&serde_json::to_vec(&(
            &self.files,
            &self.constants,
        ))?))
    }
    pub fn validate(&self) -> Result<()> {
        if self.id != self.identity()? {
            return Err("adapter input snapshot identity does not match its contents".into());
        }
        self.validate_structure()
    }
    fn validate_structure(&self) -> Result<()> {
        let mut files = BTreeSet::new();
        for f in &self.files {
            if !relative(&f.path) || !files.insert(f.path.as_str()) || f.fingerprint.is_empty() {
                return Err(
                    "snapshot file paths must be unique, normalized and fingerprinted".into(),
                );
            }
        }
        for c in &self.constants {
            if !files.contains(c.site.path.as_str())
                || c.site.line == 0
                || c.site.end_line < c.site.line
            {
                return Err("constant witness is outside the source snapshot".into());
            }
        }
        Ok(())
    }
    /// Recheck bytes, without reparsing. Fingerprints are custody checks, not security hashes.
    pub fn check_current(&self, root: &Path) -> Result<()> {
        self.validate()?;
        self.check_bytes(root)
    }
    pub(super) fn check_bytes(&self, root: &Path) -> Result<()> {
        let root = root.canonicalize()?;
        for f in &self.files {
            let path = root.join(&f.path).canonicalize()?;
            if !path.starts_with(&root) || fingerprint(&std::fs::read(path)?) != f.fingerprint {
                return Err(format!("source changed during adapter invocation: {}", f.path).into());
            }
        }
        Ok(())
    }
    pub fn constant(&self, source: &str, name: &str) -> Result<&Constant> {
        let found = self
            .constants
            .iter()
            .filter(|c| c.site.path == source && c.module.is_empty() && c.name == name)
            .collect::<Vec<_>>();
        let [one] = found.as_slice() else {
            return Err(format!(
                "constant {source}::{name}: expected one top-level declaration, found {}",
                found.len()
            )
            .into());
        };
        Ok(one)
    }
    /// Resolve a qualified module path, or a globally unique leaf selector.
    /// Girard names stages by unique leaf (e.g. backend::admit as admit).
    /// A leaf is not a claim of top-level Rust scope; ambiguity always refuses.
    pub fn module_root(&self, name: &str) -> Result<String> {
        let roots = self
            .files
            .iter()
            .filter(|f| {
                f.modules.iter().any(|m| {
                    if name.contains("::") {
                        m.iter().map(String::as_str).eq(name.split("::"))
                    } else {
                        m.last().is_some_and(|s| s == name)
                    }
                })
            })
            .map(|f| f.path.strip_suffix("/mod.rs").unwrap_or(&f.path).to_owned())
            .collect::<BTreeSet<_>>();
        if roots.len() != 1 {
            return Err(format!(
                "declared module {name}: expected one source root, found {}",
                roots.len()
            )
            .into());
        }
        roots
            .into_iter()
            .next()
            .ok_or_else(|| "module root unavailable".into())
    }
}
