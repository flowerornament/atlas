//! Shared read-only Rust source analysis: capture, query, check and project.
//! Public reexports preserve the library's stable entry paths; implementation
//! folders group each representation with the operations that own its invariants.
//! Source observations and project judgments retain different evidence meanings.
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
pub mod revision;

/// Errors from source, revision and analysis boundaries.
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
