//! Qualified file relationships with exact source witnesses.
use crate::{Backend, artifact_output};
use atlas::{
    Result,
    dependencies::{self, Relation},
    discovery,
    render::{Ascii, Pele, Renderer},
};
use clap::Args;
use std::path::{Path, PathBuf};
#[derive(Args)]
pub(crate) struct Options {
    /// Compatibility positional project root; prefer --root.
    pub root: Option<PathBuf>,
    /// Physical file/directory; defaults to production src.
    scope: Option<String>,
    #[arg(long = "scope", conflicts_with = "scope")]
    named_scope: Option<String>,
    #[arg(long, value_enum, default_value = "calls")]
    relation: Relation,
    #[command(flatten)]
    output: artifact_output::Options,
}
pub(crate) fn run(
    root: &Path,
    metadata: &cargo_metadata::Metadata,
    options: &Options,
) -> Result<()> {
    let snapshot = discovery::snapshot_metadata(metadata)?;
    let graph = snapshot.graph();
    let scope = discovery::resolve_scope(
        graph,
        Some(
            options
                .named_scope
                .as_deref()
                .or(options.scope.as_deref())
                .unwrap_or("src"),
        ),
    )?;
    let facts = atlas_core::analysis::Analysis::new(&snapshot)?.representations()?;
    let view = dependencies::project(&facts, &scope, options.relation)?;
    options.output.emit(
        root,
        "dependencies",
        &serde_json::json!({"view":view,"facts":facts}),
        &crate::protected_inputs(metadata, graph, []),
        || match options.output.backend {
            Backend::Ascii => Ascii.render(&view.diagram),
            Backend::Pele => Pele { html: false }.render(&view.diagram),
        },
    )
}
