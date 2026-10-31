//! Saved questions feed native projections; output is explicit and source-safe.
use atlas::{
    Result, discovery,
    render::View,
    representations::{self, Panel, Query},
};
use clap::Args;
use std::{
    fs,
    path::{Path, PathBuf},
};
#[derive(Args)]
pub(crate) struct Options {
    /// Compatibility positional project root; prefer --root.
    pub root: Option<PathBuf>,
    /// Saved selection: title, types, optional apis and preparation_files; no edges.
    #[arg(long, conflicts_with = "types")]
    query: Option<PathBuf>,
    /// Select types without a query file (repeat or comma separate).
    #[arg(long = "type", value_delimiter = ',')]
    types: Vec<String>,
    #[arg(long, value_enum, default_value = "all")]
    panel: Panel,
    #[command(flatten)]
    output: crate::cli::output::artifact::Options,
}
pub(crate) fn run(
    root: &Path,
    metadata: &cargo_metadata::Metadata,
    options: &Options,
) -> Result<()> {
    let query_path = options
        .query
        .as_ref()
        .map(|p| root.join(p).canonicalize())
        .transpose()?;
    let query: Query = if let Some(p) = &query_path {
        serde_json::from_str(&fs::read_to_string(p)?)?
    } else {
        if options.types.is_empty() {
            return Err(
                "choose --query FILE or --type NAME; use overview for source-declared defaults"
                    .into(),
            );
        }
        Query {
            title: format!("Representations: {}", options.types.join(", ")),
            types: options
                .types
                .iter()
                .map(|n| representations::Selector {
                    name: n.clone(),
                    file: None,
                })
                .collect(),
            apis: vec![],
            preparation_files: vec![],
            preparation_functions: vec![],
        }
    };
    let snapshot = discovery::snapshot_metadata(metadata)?;
    let graph = snapshot.graph();
    let facts = atlas_core::analysis::Analysis::new(&snapshot)?.representations()?;
    let views = representations::project(&facts, &query)?;
    options.output.emit(
        root,
        "representations",
        &serde_json::json!({"facts":facts,"views":views}),
        &crate::cli::output::protected_inputs(metadata, graph, query_path),
        || {
            options
                .output
                .backend
                .render_view(View::Representations(&views, options.panel))
        },
    )
}
