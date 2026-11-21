//! Read-only analysis of a Rust workspace's source: the library behind
//! `cargo atlas`. It reads every file once into a [`discovery::Snapshot`] and
//! answers questions of it, never building or running the code and never
//! writing to it.
//!
//! Answers keep their evidence apart: *lexical* ones match written names,
//! *qualified* ones follow `use` statements, modules and Cargo targets, and
//! *declared* ones come from a policy you wrote. What Atlas cannot resolve is
//! kept as unknown, never dropped, and an ambiguous name keeps every
//! candidate: unknown is not absent.
//!
//! # Where to start
//!
//! - [`discovery::snapshot_workspace`] (or [`discovery::snapshot_metadata`])
//!   captures a workspace.
//! - [`queries::census`], [`queries::name`] and [`queries::module`] answer
//!   lexical questions of its [`discovery::Snapshot::graph`];
//!   [`queries::orient`] is the one-page arrival report.
//! - [`analysis::Analysis`] adds qualified resolution:
//!   [`analysis::Analysis::representations`] returns the
//!   [`representations::Facts`] that [`queries::callers`] and the
//!   dependency views read.
//! - [`relations::Relations`] flattens both into tables, and [`query`] runs
//!   Datalog over them.
//! - [`review`] and [`revision`] compare versions; [`checks`] runs
//!   architecture policies.
//!
//! [`guide`] is the longer tour, the same text as `cargo atlas help library`.
//!
//! # Example
//!
//! Capture this crate's workspace, count it, and ask the same count as a
//! query over its lexical facts:
//!
//! ```
//! use atlas_lib::{discovery, queries, query, relations::{Cell, Family, Relations}};
//! use std::path::Path;
//!
//! let snapshot = discovery::snapshot_workspace(Path::new(env!("CARGO_MANIFEST_DIR")))?;
//! let census = queries::census(snapshot.graph(), None)?;
//! assert!(census.files > 0);
//!
//! let program = query::parse(r#"?- census{metric: "files", n: N}."#)?;
//! let checked = query::check(&program, &Family::ALL, &query::Options::default())?;
//! let relations = Relations::capture(snapshot.graph(), None);
//! let answer = query::run(&checked, &relations, query::Limits::default())?;
//! assert_eq!(answer.tables[0].rows, [[Cell::Integer(census.files as u64)]]);
//! # Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
//! ```
//!
//! Public reexports keep the library's stable entry paths; each folder groups
//! a representation with the operations that own its invariants.
pub mod source;
pub use source::{discovery, model, population, relationships};
pub(crate) use source::{scan, text as source_text, workspace};
pub mod analysis;
pub use analysis::{index as source_index, representations, workflow};
pub mod queries;
pub use queries::{documentation, review};
pub mod architecture;
pub mod checks;
pub mod diagram;
pub mod query;
pub mod relations;
pub mod revision;
pub mod suggestion;

/// The library guide: snapshots, qualified analysis, navigation, relations,
/// queries, comparing versions and architecture checks, with examples.
pub mod guide;

/// Errors from source, revision and analysis boundaries.
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[cfg(test)]
#[path = "../tests/support/mod.rs"]
mod test_support;
