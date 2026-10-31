//! Read-only Rust analysis and evidence-bearing views with interchangeable renderers.
//! Core owns capture and analysis; this facade preserves existing public paths.
//! Application views select observations and renderers present them without reading source.
pub(crate) use atlas_core::source_index;
pub use atlas_core::{
    analysis, architecture, diagram, discovery, model, queries, relationships, revision, workflow,
};
pub mod adapter;
pub mod render;
mod views;
pub use views::{comparison, dependencies, overview, program, representations};

/// Errors at I/O, source discovery and rendering boundaries.
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
