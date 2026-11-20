//! The arrival report: measurement lives in `queries::orient`; this command
//! chooses the scope and row count and prints text or JSON.
use super::json;
use cargo_atlas::{Result, discovery, queries::orient};
use clap::Args;
use std::path::Path;

#[derive(Args)]
pub(crate) struct Options {
    /// Package name or physical source path; defaults to the whole workspace.
    #[arg(long)]
    scope: Option<String>,
    /// Rows shown in each list; every list reports what it left out.
    #[arg(long, default_value_t = 5)]
    rows: usize,
    /// Print the same answer as JSON, for scripts and agents.
    #[arg(long)]
    json: bool,
}

/// `root` is the project directory as the caller gave it, so printed commands
/// carry the same `--root` and run as shown.
pub(crate) fn run(
    metadata: &cargo_metadata::Metadata,
    root: &Path,
    options: &Options,
) -> Result<()> {
    let snapshot = discovery::snapshot_metadata(metadata)?;
    let report = orient::orient(&snapshot, options.scope.as_deref(), options.rows)?;
    if options.json {
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
