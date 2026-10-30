//! Typed project declarations: boundaries, allowed reads and source workflow anchors.
use crate::Result;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    path::{Component, Path},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    Support,
    Vocabulary,
    SharedLaw,
    Stage,
    Instrument,
    Edge,
}
impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Support => "support",
            Self::Vocabulary => "vocabulary",
            Self::SharedLaw => "shared-law",
            Self::Stage => "stage",
            Self::Instrument => "instrument",
            Self::Edge => "edge",
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Module {
    pub name: String,
    pub kind: Kind,
    /// Project terminology; operational role remains kind.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub path: String,
    pub entry: String,
    pub types: Vec<String>,
    pub dependencies: Vec<String>,
    #[serde(skip)]
    pub directory: bool,
}
impl Module {
    pub fn label(&self) -> &str {
        self.label.as_deref().unwrap_or_else(|| self.kind.label())
    }
}
/// An existing coordinator selected for source analysis; contains no authored edges.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkflowAnchor {
    pub name: String,
    pub path: String,
    pub function: String,
}
/// JSON protocol v1. Derived filesystem properties are never protocol inputs.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Architecture {
    pub schema: u32,
    #[serde(rename = "source")]
    pub provenance: Vec<String>,
    pub modules: Vec<Module>,
    pub workflows: Vec<WorkflowAnchor>,
}
impl Architecture {
    /// Parse JSON and bind disjoint boundaries and anchors to the requested workspace.
    pub fn parse(text: &str, root: &Path) -> Result<Self> {
        let profile: Self =
            serde_json::from_str(text).map_err(|e| format!("architecture JSON: {e}"))?;
        profile.bind(root)
    }
    /// Bind and validate already parsed declarations without serializing them again.
    pub fn bind(self, root: &Path) -> Result<Self> {
        let mut profile = self;
        if profile.schema != 1 {
            return Err(format!(
                "unsupported architecture schema {}; expected 1",
                profile.schema
            )
            .into());
        }
        if profile.modules.is_empty() && profile.workflows.is_empty() {
            return Err("architecture has neither modules nor workflow anchors".into());
        }
        if profile
            .provenance
            .iter()
            .any(|s| s.starts_with('/') || s.contains('\\'))
        {
            return Err("architecture source references must be workspace relative".into());
        }
        let root = root.canonicalize()?;
        let mut names = BTreeSet::new();
        let mut paths: Vec<String> = Vec::new();
        for module in &mut profile.modules {
            if module.name.is_empty() || !names.insert(module.name.clone()) {
                return Err(
                    format!("duplicate or empty architecture module {:?}", module.name).into(),
                );
            }
            module.directory = bind_path(&root, &module.path)?.is_dir();
            for prior in &paths {
                if contains(prior, &module.path) || contains(&module.path, prior) {
                    return Err(format!(
                        "overlapping architecture paths {prior} and {}",
                        module.path
                    )
                    .into());
                }
            }
            unique_values(&module.types, "surface type")?;
            unique_values(&module.dependencies, "dependency")?;
            paths.push(module.path.clone());
        }
        for module in &profile.modules {
            for dependency in &module.dependencies {
                if !names.contains(dependency) {
                    return Err(
                        format!("{} has unknown dependency {dependency}", module.name).into(),
                    );
                }
            }
        }
        names.clear();
        for anchor in &profile.workflows {
            if anchor.name.is_empty() || !names.insert(anchor.name.clone()) {
                return Err(format!("duplicate or empty workflow name {:?}", anchor.name).into());
            }
            if anchor.function.trim().is_empty() {
                return Err(format!("workflow {} has no function", anchor.name).into());
            }
            if !bind_path(&root, &anchor.path)?.is_file() {
                return Err(format!("workflow {} must name a source file", anchor.name).into());
            }
        }
        Ok(profile)
    }
    pub fn owner(&self, file: &str) -> Option<&Module> {
        self.modules.iter().find(|m| {
            if m.directory {
                contains(&m.path, file)
            } else {
                m.path == file
            }
        })
    }
    /// A shared physical directory, independent of any semantic interpretation.
    pub fn common_directory(&self) -> String {
        let Some(first) = self.modules.first() else {
            return String::new();
        };
        let mut components: Vec<_> = first.path.split('/').collect();
        if !first.directory {
            components.pop();
        }
        for module in &self.modules[1..] {
            let mut parts: Vec<_> = module.path.split('/').collect();
            if !module.directory {
                parts.pop();
            }
            let count = components
                .iter()
                .zip(parts)
                .take_while(|(a, b)| **a == *b)
                .count();
            components.truncate(count);
        }
        components.join("/")
    }
}
fn unique_values(values: &[String], kind: &str) -> Result<()> {
    let mut seen = BTreeSet::new();
    for value in values {
        if value.trim() != value || value.is_empty() || !seen.insert(value) {
            return Err(format!("empty, padded or duplicate {kind} {value:?}").into());
        }
    }
    Ok(())
}
/// Validate a normalized workspace-relative spelling without requiring existence.
pub(crate) fn relative_path(path: &str) -> Result<()> {
    if path.is_empty()
        || !Path::new(path)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
    {
        return Err(format!("architecture path {path:?} must be normalized and relative").into());
    }
    let normalized = Path::new(path)
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/");
    if normalized != path {
        return Err(format!("architecture path {path:?} is not normalized").into());
    }
    Ok(())
}
pub(crate) fn bind_path(root: &Path, path: &str) -> Result<std::path::PathBuf> {
    relative_path(path)?;
    let mut boundary = root.to_path_buf();
    for component in Path::new(path).components() {
        boundary.push(component.as_os_str());
        if std::fs::symlink_metadata(&boundary)
            .map_err(|e| format!("architecture path {path}: {e}"))?
            .file_type()
            .is_symlink()
        {
            return Err(format!(
                "architecture path {path} crosses a symlink excluded by discovery"
            )
            .into());
        }
    }
    if !boundary.canonicalize()?.starts_with(root) {
        return Err(format!("architecture path {path} escapes workspace").into());
    }
    Ok(boundary)
}
pub fn contains(prefix: &str, path: &str) -> bool {
    prefix.is_empty()
        || prefix == path
        || path
            .strip_prefix(prefix)
            .is_some_and(|tail| tail.starts_with('/'))
}
