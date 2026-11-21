//! The arrival report: measurement lives in `queries::orient`; this command
//! chooses the scope and row count and prints text or JSON.
use super::json;
use cargo_atlas::{Result, discovery, queries::orient};
use clap::Args;

#[derive(Args)]
pub(crate) struct Options {
    #[command(flatten)]
    pub(super) scope: crate::cli::subject::Scope,
    #[command(flatten)]
    rows: crate::cli::output::rows::Rows,
    #[command(flatten)]
    output: crate::cli::output::form::Report,
    /// Read this SCIP index to split unresolved calls into other packages,
    /// workspace functions and open ones (Atlas runs no indexer); --semantic
    /// makes one instead.
    #[arg(long, value_name = "FILE")]
    scip: Option<std::path::PathBuf>,
    #[command(flatten)]
    indexer: crate::cli::semantic::IndexerArgs,
}

pub(crate) fn run(
    metadata: &cargo_metadata::Metadata,
    options: &Options,
    cache: &crate::cli::semantic::IndexCache,
) -> Result<()> {
    let snapshot = discovery::snapshot_metadata(metadata)?;
    let graph = snapshot.graph();
    let chosen = options.scope.choose(graph, metadata)?;
    // The row budget is the text's: JSON holds every row of every section.
    let rows = if options.output.json() {
        usize::MAX
    } else {
        options.rows.limit(5)
    };
    let index = crate::cli::semantic::obtain(
        metadata.workspace_root.as_std_path(),
        metadata,
        options.scip.as_deref(),
        &options.indexer,
        cache,
        || crate::cli::output::graph_captures(metadata, graph).collect(),
    )?;
    let report = match &index {
        Some((index, _)) => orient::semantic_orient(&snapshot, chosen.query(), rows, index)?,
        None => orient::orient(&snapshot, chosen.query(), rows)?,
    };
    if options.output.json() {
        print!("{}", json(&report)?);
    } else {
        print!("{}", report.text());
    }
    Ok(())
}
