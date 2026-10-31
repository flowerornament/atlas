//! Source evidence, independent of any diagram layout.
use serde::{Deserialize, Serialize};

/// A module path established by declarations from a Cargo target root.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Mount {
    pub root: String,
    pub modules: Vec<String>,
    /// Disambiguates Cargo targets sharing a root path; absent for a unique root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    /// A target or module ancestry requiring a test build, independent of file identity.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub test_only: bool,
}

/// One package's observation of a physical file, including disconnected source.
/// A path shared by packages has multiple observations, never a chosen owner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceFile {
    pub path: String,
    pub crate_name: String,
    /// Stable FNV-1a 64-bit content fingerprint, not a security digest.
    pub content_fingerprint: String,
    pub test_only: bool,
    #[serde(default)]
    pub mounts: Vec<Mount>,
    pub declarations: Vec<Declaration>,
    pub(crate) imports: Vec<std::sync::Arc<str>>,
}

/// A declaration site, not a globally resolved Rust symbol identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Declaration {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub visibility: String,
    pub line: usize,
    pub end_line: usize,
    pub summary: Option<String>,
    pub test_only: bool,
    pub(crate) block: std::sync::Arc<str>,
}

/// The syntactic observation behind a relationship.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Evidence {
    ParameterMention,
    ReturnMention,
    TypeBodyMention,
}
impl Evidence {
    pub fn label(self) -> &'static str {
        match self {
            Self::ParameterMention => "consumes",
            Self::ReturnMention => "produces",
            Self::TypeBodyMention => "composes",
        }
    }
}

/// Textual candidate selection; uniqueness does not establish Rust resolution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", content = "value", rename_all = "kebab-case")]
pub enum Resolution {
    Unique(String),
    Ambiguous(Vec<String>),
    Unresolved(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relationship {
    pub id: String,
    pub source: String,
    pub spelling: String,
    pub evidence: Evidence,
    pub target: Resolution,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnostic {
    pub file: String,
    pub line: Option<usize>,
    /// None for physical-file evidence; Some for a package-specific observation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package: Option<String>,
    pub message: String,
}

/// All source observations for one scan, serialized with a schema version.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceGraph {
    pub schema: u32,
    pub root: String,
    /// Names Cargo exposes for internal imports, per consuming package.
    pub import_roots: std::collections::BTreeMap<String, Vec<String>>,
    pub files: Vec<SourceFile>,
    pub relationships: Vec<Relationship>,
    pub diagnostics: Vec<Diagnostic>,
}
impl SourceGraph {
    /// Physical inventory, counting a shared path once. A production observation
    /// takes precedence for physical reports; package queries use `files` directly.
    pub fn physical_files(&self) -> impl Iterator<Item = &SourceFile> {
        let mut paths = std::collections::BTreeMap::<&str, &SourceFile>::new();
        for file in &self.files {
            paths
                .entry(&file.path)
                .and_modify(|previous| {
                    if previous.test_only && !file.test_only {
                        *previous = file;
                    }
                })
                .or_insert(file);
        }
        paths.into_values()
    }
}
