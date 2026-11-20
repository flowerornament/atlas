//! Qualified file relationships with exact source witnesses.
use crate::cli::output::artifact as artifact_output;
use cargo_atlas::{
    Result,
    dependencies::{self, Relation},
    discovery,
    render::View,
};
use clap::Args;
use std::path::{Path, PathBuf};
/// The evidence document, serialized from borrows rather than copied into a
/// JSON value first.
#[derive(serde::Serialize)]
struct Evidence<'a, V> {
    view: &'a V,
    facts: &'a atlas_lib::representations::Facts,
}
#[derive(Args)]
pub(crate) struct Options {
    /// Compatibility positional project root; prefer --root.
    pub root: Option<PathBuf>,
    /// Physical file/directory; defaults to every package's production files.
    scope: Option<String>,
    /// File or directory to draw, such as `crates/core/src`; defaults to every
    /// package's production files.
    #[arg(long = "scope", conflicts_with = "scope")]
    named_scope: Option<String>,
    #[arg(long, value_enum, default_value = "calls")]
    relation: Relation,
    /// Put the whole workspace's facts in the JSON evidence, not only those
    /// incident to the scope.
    #[arg(long)]
    all_facts: bool,
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
    let requested = options.named_scope.as_deref().or(options.scope.as_deref());
    let scope = discovery::resolve_scope(graph, requested)?;
    let facts = atlas_lib::analysis::Analysis::new(&snapshot)?.representations()?;
    let view = dependencies::project(&facts, &scope, options.relation)?;
    let facts = if scope.is_empty() || options.all_facts {
        facts
    } else {
        facts.incident(&scope, |path| {
            atlas_lib::architecture::contains(&scope, path)
        })
    };
    options.output.emit(
        root,
        "dependencies",
        &Evidence {
            view: &view,
            facts: &facts,
        },
        &crate::cli::output::protected_inputs(metadata, graph, []),
        || options.output.render_view(View::Graph(&view.diagram)),
    )?;
    if requested.is_none() && !options.output.json {
        narrowing_hint(metadata, graph);
    }
    Ok(())
}

/// After an unscoped drawing of several packages, name one to narrow to: the
/// package with the most source files, as a runnable command.
fn narrowing_hint(metadata: &cargo_metadata::Metadata, graph: &cargo_atlas::model::SourceGraph) {
    let packages = metadata.workspace_packages();
    if packages.len() < 2 {
        return;
    }
    let root = metadata.workspace_root.as_std_path();
    let largest = packages
        .iter()
        .filter_map(|package| {
            let src = package.manifest_path.parent()?.as_std_path().join("src");
            let src = src.strip_prefix(root).ok()?.to_owned();
            let files = graph
                .files
                .iter()
                .filter(|file| file.crate_name == package.name.as_str() && !file.test_only)
                .count();
            Some((files, src))
        })
        .max_by_key(|(files, _)| *files);
    if let Some((_, src)) = largest {
        eprintln!(
            "atlas: drew all {} packages; to draw one: cargo atlas dependencies --scope {}",
            packages.len(),
            src.display()
        );
    }
}
