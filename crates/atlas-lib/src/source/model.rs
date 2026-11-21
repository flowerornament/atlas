//! Source evidence, independent of any diagram layout.
use serde::{Deserialize, Serialize};

/// A module path established by declarations from a Cargo target root.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[non_exhaustive]
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
#[non_exhaustive]
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
#[non_exhaustive]
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
    /// Enclosing inline `mod` blocks, outermost first. Omitted from JSON when
    /// empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub modules: Vec<String>,
    /// For an item written in an impl block, the name the impl's self type
    /// ends in, which `Self` means there. Omitted when there is none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub self_type: Option<String>,
}

impl Declaration {
    /// Recognized kind at the legacy wire boundary. Unknown tags stay available
    /// verbatim in `kind`; consumers must not infer a supported item kind.
    pub fn item_kind(&self) -> Option<crate::source::ItemKind> {
        crate::source::ItemKind::from_tag(&self.kind)
    }
    /// Recognized written visibility, distinct from Rust effective visibility.
    /// Unknown serialized tags are retained verbatim in `visibility`.
    pub fn item_visibility(&self) -> Option<crate::source::Visibility> {
        crate::source::Visibility::from_tag(&self.visibility)
    }
}

/// The syntactic observation behind a relationship.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
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
#[non_exhaustive]
pub enum Resolution {
    Unique(String),
    Ambiguous(Vec<String>),
    Unresolved(String),
}
/// For text: the declaration ID, `one of ...`, or `unresolved: reason`.
impl std::fmt::Display for Resolution {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unique(id) => f.write_str(id),
            Self::Ambiguous(ids) => write!(f, "one of {}", ids.join(", ")),
            Self::Unresolved(reason) => write!(f, "unresolved: {reason}"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Relationship {
    pub id: String,
    pub source: String,
    pub spelling: String,
    pub evidence: Evidence,
    pub target: Resolution,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Diagnostic {
    pub file: String,
    pub line: Option<usize>,
    /// None for physical-file evidence; Some for a package-specific observation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package: Option<String>,
    /// What kind of limit this is: stable across releases, unlike `message`.
    #[serde(default)]
    pub code: DiagnosticCode,
    /// A reading for people; its wording may change.
    pub message: String,
}

/// The kind of a [`Diagnostic`], as a stable kebab-case code. Group and
/// filter by code; `message` explains the instance.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum DiagnosticCode {
    /// A symbolic link met while walking source directories was not followed.
    SymlinkExcluded,
    /// A source path outside the workspace, or reached through a symlink.
    SourceExcluded,
    /// A target root outside `src`; the module files it declares are
    /// followed from it, other files beside it are not read.
    TargetOutsideSrc,
    /// A `mod` declaration whose file is not among the discovered files.
    MountMissing,
    /// A module file outside `src` that could not be read.
    MountUnavailable,
    /// The file does not parse as Rust.
    SyntaxError,
    /// Written Rust that line-based native readings cannot account for.
    NativeLimit,
    /// A `use` declaration whose bindings could not be read.
    UseUnreadable,
    /// An external glob import; name-only resolution is suppressed there.
    ExternalGlob,
    /// `Self` outside any impl or type it could name.
    SelfUnresolved,
    /// A boundary path a seam check could not judge.
    BoundaryUnjudged,
    /// A project boundary declaration that is missing.
    BoundaryDeclarationUnavailable,
    /// A project boundary declaration that differs from the captured source.
    BoundaryDeclarationMismatch,
    /// A required test path a seam check could not judge.
    TestPathUnjudged,
    /// A required test module file that could not be read.
    TestMountUnavailable,
    /// A header-citation observation taken from other source bytes.
    HeaderCitationStale,
    /// A required header-citation observation that was not supplied.
    HeaderCitationUnavailable,
    /// A required measurement that could not be made.
    MeasureUnavailable,
    /// A selected package without a captured source-tree population.
    PopulationUnavailable,
    /// Stage `use` syntax that could not be read.
    StageUseUnreadable,
    /// Stage seams without their required `stage_order`.
    StageOrderMissing,
    /// Required seam import syntax that could not be read.
    SeamImportUnavailable,
    /// A production file mounted from outside `src`, which the stage
    /// reading does not read.
    ProductionMountUnread,
    /// Read from JSON written before codes existed, or with a code this
    /// build does not know.
    #[default]
    #[serde(other)]
    Unclassified,
}

/// All source observations for one scan, serialized with a schema version.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
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

#[cfg(test)]
mod tests {
    use super::Declaration;
    #[test]
    fn unknown_declaration_tags_round_trip_without_acquiring_meaning() {
        let wire = serde_json::json!({
            "id": "source:1:Future", "name": "Future", "kind": "FutureKind",
            "visibility": "FutureVisibility", "line": 1, "end_line": 1,
            "summary": null, "test_only": false, "block": "future Future;"
        });
        let declaration: Declaration = serde_json::from_value(wire.clone()).unwrap();
        assert_eq!(declaration.item_kind(), None);
        assert_eq!(declaration.item_visibility(), None);
        assert_eq!(serde_json::to_value(declaration).unwrap(), wire);
    }
}
