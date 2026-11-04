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

pub struct Function {
    pub id: String,
    pub scope: Vec<String>,
    pub flow: Workflow,
}
pub struct Boundary {
    pub module: Module,
    pub path: Vec<String>,
}
/// Immutable correlated name tables. Function and boundary positions are local
/// to this index; borrowing accessors cannot reorder or replace their contents.
///
/// ```compile_fail
/// fn reorder(index: &mut atlas_core::source_index::Index) {
///     index.functions.reverse();
/// }
/// ```
///
/// ```compile_fail
/// fn reorder(index: &mut atlas_core::source_index::Index) {
///     index.functions().reverse();
/// }
/// ```
pub struct Index {
    functions: Vec<Function>,
    boundaries: Vec<Boundary>,
    roots: Vec<usize>,
    inventory: Vec<String>,
    scopes: BTreeMap<String, Vec<Vec<String>>>,
    names: BTreeMap<Vec<String>, Vec<usize>>,
    crates: BTreeSet<String>,
    exports: BTreeMap<Vec<String>, Vec<Vec<String>>>,
}
#[derive(Clone, Debug)]
pub enum Target {
    Function(usize),
    Boundary(usize, Option<String>),
    Summarized(String),
    /// The caller supplied a position outside this index, not an unresolved
    /// source call. Consumers must refuse rather than publish a partial view.
    InvalidFunction(usize),
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
