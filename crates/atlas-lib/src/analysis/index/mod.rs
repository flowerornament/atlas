//! Conservative source names: Cargo roots, explicit imports, and inspected signatures.
//! Construction binds Cargo mounts, boundaries, functions, names and method owners.
//! Resolution never chooses among ambiguous names; Analysis owns the syntax cache.
mod build;
mod mounts;
mod resolve;

use super::{
    sources::{Sources, read_sources},
    workflow::Workflow,
};
use crate::{
    Result,
    architecture::{Architecture, Module},
    source::model::SourceGraph,
};
use cargo_metadata::Metadata;
use mounts::Mounts;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

#[non_exhaustive]
pub struct Function {
    pub id: String,
    pub scope: Vec<String>,
    pub flow: Workflow,
}
#[non_exhaustive]
pub struct Boundary {
    pub module: Module,
    pub path: Vec<String>,
}
/// Immutable correlated name tables. Function and boundary positions are local
/// to this index; borrowing accessors cannot reorder or replace their contents.
///
/// ```compile_fail
/// fn reorder(index: &mut atlas_lib::source_index::Index) {
///     index.functions.reverse();
/// }
/// ```
///
/// ```compile_fail
/// fn reorder(index: &mut atlas_lib::source_index::Index) {
///     index.functions().reverse();
/// }
/// ```
#[non_exhaustive]
pub struct Index {
    functions: Vec<Function>,
    boundaries: Vec<Boundary>,
    roots: Vec<usize>,
    inventory: Vec<String>,
    scopes: BTreeMap<String, Vec<Vec<String>>>,
    names: BTreeMap<Vec<String>, Vec<usize>>,
    crates: BTreeSet<String>,
    exports: BTreeMap<Vec<String>, Vec<Vec<String>>>,
    /// Qualified paths of the structs, enums, unions and traits the workspace
    /// source declares.
    types: BTreeSet<Vec<String>>,
    /// The last segment of every path in `types`.
    type_names: BTreeSet<String>,
    /// Type aliases by qualified path: the module scope they are written in,
    /// the aliased path and the imports there.
    aliases: BTreeMap<Vec<String>, (Vec<String>, super::workflow::Alias)>,
    /// Trait-impl owners the workspace declares no type for: a foreign type
    /// (or one a macro declares). Their methods are never call targets.
    foreign: BTreeSet<Vec<String>>,
}
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum Target {
    Function(usize),
    Boundary(usize, Option<String>),
    Summarized(Summary),
    /// The caller supplied a position outside this index, not an unresolved
    /// source call. Consumers must refuse rather than publish a partial view.
    InvalidFunction(usize),
}
/// Why a call was left unresolved: a stable kind, the name it concerns where
/// there is one, and a reading for people whose wording may change.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct Summary {
    pub kind: SummaryKind,
    /// The path, name, receiver, type or module the reason concerns.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
    pub message: String,
}
impl Summary {
    pub(crate) fn new(kind: SummaryKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            subject: None,
            message: message.into(),
        }
    }
    pub(crate) fn about(kind: SummaryKind, subject: &str, message: impl Into<String>) -> Self {
        Self {
            kind,
            subject: Some(subject.to_owned()),
            message: message.into(),
        }
    }
}
impl std::fmt::Display for Summary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
/// The kind of a [`Summary`], as a stable kebab-case code.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum SummaryKind {
    /// A public re-export chain that is ambiguous or cyclic.
    ReexportAmbiguous,
    /// A stage's declared entry re-exported ambiguously.
    StageEntryAmbiguous,
    /// A stage's support API, outside its declared entry (subject: stage).
    SupportApi,
    /// More than one workspace function has the path.
    StaticCallAmbiguous,
    /// A trait impl for a type the workspace does not declare (subject: type).
    ForeignImplOwner,
    /// A path no workspace function has: external, or not followed (subject: path).
    UnresolvedPath,
    /// A call through an expression rather than a path.
    IndirectCall,
    /// A call of a local value (subject: name).
    ValueCall,
    /// A body-local item or import shadows the module's names (subject: name).
    LocalShadow,
    /// A local item and an import share the name (subject: name).
    ImportNamespaceClash,
    /// The import the path starts with is ambiguous.
    ImportAmbiguous,
    /// `self`'s type is not established.
    SelfOwnerUnknown,
    /// The receiver has no unique written type (subject: receiver).
    ReceiverTypeUnknown,
    /// The receiver's initializer result is not established (subject: receiver).
    ReceiverInitializerUnknown,
    /// A boundary call with no resolved entry.
    BoundaryUnresolved,
    /// The call resolved to a body the type facts did not select.
    BodyUnselected,
    /// The file's Cargo or module contexts resolve the call to different bodies.
    ContextsDisagree,
    /// Read from JSON with a kind this build does not know.
    #[serde(other)]
    Unclassified,
}
impl Index {
    /// Functions in the stable order used by this index's positions.
    pub fn functions(&self) -> &[Function] {
        &self.functions
    }
    /// Authored boundaries in the stable order used by Target::Boundary.
    pub fn boundaries(&self) -> &[Boundary] {
        &self.boundaries
    }
    /// Cargo binary entry positions in functions().
    pub fn roots(&self) -> &[usize] {
        &self.roots
    }
    /// Captured file paths represented by this index.
    pub fn inventory(&self) -> &[String] {
        &self.inventory
    }

    pub fn new(
        root: &Path,
        metadata: &Metadata,
        graph: &SourceGraph,
        architecture: &Architecture,
    ) -> Result<Self> {
        let sources = read_sources(&root.canonicalize()?, graph)?;
        Self::from_sources(&sources, metadata, graph, architecture)
    }
    pub(in crate::analysis) fn from_sources(
        sources: &Sources,
        metadata: &Metadata,
        graph: &SourceGraph,
        architecture: &Architecture,
    ) -> Result<Self> {
        Self::from_selected_sources(
            sources,
            metadata,
            graph,
            architecture,
            crate::population::TestSelection::Production,
        )
    }
    pub(in crate::analysis) fn from_selected_sources(
        sources: &Sources,
        metadata: &Metadata,
        graph: &SourceGraph,
        architecture: &Architecture,
        tests: crate::population::TestSelection,
    ) -> Result<Self> {
        let mounts = Mounts::read(metadata, graph, tests)?;
        let boundaries = mounts.boundaries(architecture)?;
        let mut index = Self {
            functions: Vec::new(),
            boundaries,
            roots: Vec::new(),
            inventory: graph
                .physical_files()
                .map(|file| file.path.clone())
                .collect(),
            scopes: mounts
                .files
                .iter()
                .map(|(path, mounts)| {
                    (
                        path.clone(),
                        mounts.iter().map(|(scope, _)| scope.clone()).collect(),
                    )
                })
                .collect(),
            names: BTreeMap::new(),
            crates: mounts.crates.clone(),
            exports: BTreeMap::new(),
            types: BTreeSet::new(),
            type_names: BTreeSet::new(),
            aliases: BTreeMap::new(),
            foreign: BTreeSet::new(),
        };
        index.read_functions(sources, graph, &mounts, tests)?;
        index.name_functions();
        index.bind_method_owners()?;
        Ok(index)
    }
    pub fn select(&self, path: &str, function: &str) -> Result<usize> {
        let found: Vec<_> = self
            .functions
            .iter()
            .enumerate()
            .filter(|(_, f)| {
                f.flow.site.path == path
                    && (f.flow.function == function
                        || (!function.contains("::")
                            && f.flow.function.rsplit("::").next() == Some(function)))
            })
            .map(|(i, _)| i)
            .collect();
        match found.as_slice() {
            [i] => Ok(*i),
            _ => Err(format!(
                "program entry {path}::{function}: expected one function, found {}",
                found.len()
            )
            .into()),
        }
    }
}
