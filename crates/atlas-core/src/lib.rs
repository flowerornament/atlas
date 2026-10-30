//! Shared read-only Rust source facts and developer queries.
//! Lexical evidence preserves ambiguity; it does not establish runtime behavior.
pub mod architecture;
pub mod discovery;
pub mod documentation;
pub mod model;
pub mod population;
pub mod queries;
pub mod relationships;
pub mod review;
pub mod revision;
mod scan;
pub mod source;
/// Errors from source, revision and analysis boundaries.
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// Renderer-neutral diagram data and source-backed quotient projections.
pub mod diagram;
/// Qualified written-source forms and construction observations.
pub mod representations;
/// Conservative name/call lookup shared by qualified views.
pub mod source_index;
mod source_text;
/// Ordered source-control syntax and body observations.
pub mod workflow;

/// Reusable qualified queries over one captured source and Cargo context.
pub mod analysis;

/// Fixed architecture checks over captured source and explicit authored policy.
pub mod checks;

mod workspace;
