//! The arrival report: measurement lives in `queries::orient`; this command
//! chooses the scope and row count and prints text or JSON.
use super::json;
use cargo_atlas::{Result, discovery, queries::orient};
use clap::Args;
use std::path::Path;

#[derive(Args)]
pub(crate) struct Options {
    #[command(flatten)]
    pub(super) scope: crate::cli::subject::Scope,
    #[command(flatten)]
    rows: crate::cli::output::rows::Rows,
    #[command(flatten)]
    output: crate::cli::output::form::Report,
}

/// `root` is the project directory as the caller gave it, so printed commands
/// carry the same `--root` and run as shown.
pub(crate) fn run(
    metadata: &cargo_metadata::Metadata,
    root: &Path,
    options: &Options,
) -> Result<()> {
    let snapshot = discovery::snapshot_metadata(metadata)?;
    let chosen = options.scope.choose(snapshot.graph(), metadata)?;
    let report = orient::orient(&snapshot, chosen.query(), options.rows.limit(5))?;
    if options.output.json() {
        print!("{}", json(&report)?);
    } else {
        let suffix = if root == Path::new(".") {
            Vec::new()
        } else {
            vec!["--root".to_owned(), root.display().to_string()]
        };
        print!("{}", report.text(&suffix));
    }
    Ok(())
}
