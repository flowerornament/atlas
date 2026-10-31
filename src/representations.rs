//! Representation contracts, direct field retention and preparation calls.
//!
//! Extraction is independent of a saved question. These are written-source facts,
//! not value identity, complete ownership, runtime order or a duplication verdict.
mod project;
mod render;
pub use atlas_core::representations::{
    Constant, Constructor, Facts, Function, Literal, Reference, ResolvedCall, Role, Type,
    UnknownCall, UnknownType, extract,
};
pub use project::{Contract, Preparation, Producers, Views, project};
pub(crate) use render::vertical;
pub use render::{Panel, ascii, pele};
use serde::{Deserialize, Serialize};

/// One symbol selector. A name must resolve uniquely; qualify with its physical file.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Selector {
    pub name: String,
    #[serde(default)]
    pub file: Option<String>,
}
/// A saved question contains selections, never authored edges.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Query {
    pub title: String,
    pub types: Vec<Selector>,
    /// Optional exact API roots. Empty selects public producers and trait implementation methods.
    #[serde(default)]
    pub apis: Vec<Selector>,
    /// Files whose producer-returning functions become preparation rows.
    #[serde(default)]
    pub preparation_files: Vec<String>,
    /// Exact preparation functions; optional file selection further narrows them.
    #[serde(default)]
    pub preparation_functions: Vec<Selector>,
}

impl Query {
    pub(crate) fn prepares(&self, function: &Function) -> bool {
        if self.preparation_functions.is_empty() {
            self.preparation_files.contains(&function.file)
        } else {
            self.preparation_functions.iter().any(|selector| {
                selector.name == function.name
                    && selector
                        .file
                        .as_deref()
                        .is_none_or(|file| file == function.file)
            }) && (self.preparation_files.is_empty()
                || self.preparation_files.contains(&function.file))
        }
    }
}
