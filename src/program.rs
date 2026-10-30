//! Whole-program source calls, folded at declared interfaces and simple wrappers.
mod calls;
use crate::source_index as index;
mod projection;
use crate::{
    Result,
    architecture::Architecture,
    diagram::{Diagram, Node},
    model::SourceGraph,
    workflow::Call,
};
use cargo_metadata::Metadata;
use serde::Serialize;
use std::{collections::BTreeMap, path::Path};

/// One source call in a composed wrapper chain; ordinal is AST evaluation traversal order.
#[derive(Debug, Clone, Serialize)]
pub struct Hop {
    pub function: String,
    pub ordinal: usize,
    pub call: Call,
    pub context: Vec<String>,
}
#[derive(Debug, Clone, Serialize)]
pub struct Connection {
    pub from: String,
    pub to: String,
    pub chains: Vec<Vec<Hop>>,
}
#[derive(Debug, Clone, Serialize)]
pub struct Observation {
    pub function: String,
    pub call: Option<Call>,
    pub reason: String,
}
#[derive(Debug, Serialize)]
pub struct Program {
    pub schema: u32,
    pub roots: Vec<String>,
    pub anchors: Vec<String>,
    pub connections: Vec<Connection>,
    pub observations: Vec<Observation>,
    pub represented_files: Vec<String>,
    pub inventory: Vec<String>,
    pub outside_view_files: Vec<String>,
    pub nodes: Vec<Node>,
}
#[derive(Debug, Serialize)]
pub struct Overview {
    pub nodes: Vec<Node>,
    pub connections: Vec<Connection>,
    pub collapsed: BTreeMap<String, String>,
}
impl Program {
    pub fn overview(&self) -> Overview {
        projection::overview(self)
    }
    pub fn diagram(&self, overview: &Overview) -> Result<Diagram> {
        projection::diagram(self, overview)
    }
    pub fn ascii(&self, overview: &Overview) -> String {
        projection::ascii(self, overview)
    }
}
/// Follow Cargo binary entries, or one selected function; no project code is executed.
pub fn build(
    root: &Path,
    metadata: &Metadata,
    graph: &SourceGraph,
    architecture: &Architecture,
    entry: Option<(&str, &str)>,
) -> Result<Program> {
    let root = root.canonicalize()?;
    let index = index::Index::new(&root, metadata, graph, architecture)?;
    calls::build(&index, architecture, entry)
}

/// Derive the same view from captured bytes; no source read or metadata reload.
pub fn from_analysis(
    analysis: &atlas_core::analysis::Analysis<'_>,
    architecture: &Architecture,
    entry: Option<(&str, &str)>,
) -> Result<Program> {
    let index = analysis.index(architecture)?;
    calls::build(&index, architecture, entry)
}
