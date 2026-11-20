//! Read-only Rust analysis and evidence-bearing views with interchangeable renderers.
//! Core owns capture and analysis; this facade preserves existing public paths.
//! Application views select observations and renderers present them without reading source.
pub(crate) use atlas_lib::source_index;
pub use atlas_lib::{
    analysis, architecture, diagram, discovery, model, queries, relationships, revision, workflow,
};
pub mod adapter;
mod process;
pub mod render;
mod views;
/// The `types` views under their name before 0.18.0.
#[doc(hidden)]
pub use views::types as representations;
pub use views::{comparison, dependencies, overview, program, types};

/// Errors at I/O, source discovery and rendering boundaries.
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
