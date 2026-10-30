//! The same question applied to an immutable source revision and current/exact source.
use crate::artifact_output;
use atlas::{Result, comparison, discovery, render::View, representations, revision::Revision};
use clap::Args;
use std::{
    fs,
    path::{Path, PathBuf},
};
#[derive(Args)]
pub(crate) struct Options {
    /// Compatibility positional project root; prefer --root.
    pub root: Option<PathBuf>,
    #[arg(long)]
    before: String,
    /// Defaults to current working source, without snapshotting or changing it.
    #[arg(long)]
    after: Option<String>,
    #[arg(long)]
    query: Option<PathBuf>,
    #[command(flatten)]
    output: artifact_output::Options,
}
fn read(root: &Path) -> Result<representations::Facts> {
    let snapshot = discovery::snapshot_workspace(root)?;
    atlas_core::analysis::Analysis::new(&snapshot)?.representations()
}
pub(crate) fn run(
    root: &Path,
    metadata: &cargo_metadata::Metadata,
    options: &Options,
) -> Result<()> {
    let before = Revision::read(root, &options.before)?;
    let before_facts = read(&before.root)?;
    let after = options
        .after
        .as_deref()
        .map(|r| Revision::read(root, r))
        .transpose()?;
    let snapshot = discovery::snapshot_metadata(metadata)?;
    let graph = snapshot.graph();
    let after_facts = if let Some(after) = &after {
        read(&after.root)?
    } else {
        atlas_core::analysis::Analysis::new(&snapshot)?.representations()?
    };
    let query = options
        .query
        .as_ref()
        .map(|p| fs::read_to_string(root.join(p)))
        .transpose()?
        .map(|s| serde_json::from_str::<representations::Query>(&s))
        .transpose()?;
    let comparison = comparison::compare(
        &before_facts,
        &after_facts,
        query.as_ref(),
        (
            before.commit.clone(),
            after
                .as_ref()
                .map_or("working source".into(), |r| r.commit.clone()),
        ),
    )?;
    options.output.emit(
        root,
        graph,
        "comparison",
        &comparison,
        &options
            .query
            .iter()
            .map(|p| root.join(p))
            .collect::<Vec<_>>(),
        || {
            options
                .output
                .backend
                .render_view(View::Comparison(&comparison))
        },
    )
}
