//! Source-backed Rust relationship diagrams with interchangeable renderers.
//! Calls, signature types, fields, literals and lexical co-mentions retain distinct evidence.
//! None establishes runtime value identity, complete execution or ownership.
pub mod adapter;
pub use atlas_core::analysis;
pub mod architecture;
pub mod comparison;
pub mod dependencies;
pub mod diagram;
pub mod discovery;
pub mod model;
pub mod overview;
pub mod program;
pub mod queries;
pub mod relationships;
pub mod render;
pub mod representations;
pub mod revision;
mod source_index;
pub mod workflow;

/// Errors at I/O, source discovery and rendering boundaries.
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
