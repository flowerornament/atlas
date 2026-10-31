//! Qualified written-source signatures, direct fields, literal construction and calls.
//! Source names remain conservative; unknowns do not establish absence or ownership.
mod collect;
pub(crate) mod extract;
mod types;
pub use types::{
    Constant, Constructor, Facts, Function, Literal, Reference, ResolvedCall, Role, Type,
    UnknownCall, UnknownType,
};

/// Extract mounted production forms and calls from the supplied source graph.
/// Other cfg alternatives remain written source; macros are not expanded.
pub fn extract(
    root: &std::path::Path,
    metadata: &cargo_metadata::Metadata,
    graph: &crate::model::SourceGraph,
) -> crate::Result<Facts> {
    extract::extract(root, metadata, graph)
}
