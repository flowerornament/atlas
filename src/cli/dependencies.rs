//! Qualified file relationships with exact source witnesses.
use crate::cli::output::artifact as artifact_output;
use cargo_atlas::{
    Result,
    dependencies::{self, Relation},
    discovery,
    render::View,
};
use clap::Args;
use std::path::Path;
/// The evidence document, serialized from borrows rather than copied into a
/// JSON value first.
#[derive(serde::Serialize)]
struct Evidence<'a, V> {
    view: &'a V,
    facts: &'a atlas_lib::representations::Facts,
}
#[derive(Args)]
pub(crate) struct Options {
    #[command(flatten)]
    pub(super) scope: crate::cli::subject::Scope,
    #[arg(long, value_enum, default_value = "calls")]
    relation: Relation,
    /// Put the whole workspace's facts in the JSON evidence, not only those
    /// incident to the scope.
    #[arg(long)]
    all_facts: bool,
    #[command(flatten)]
    evidence: crate::cli::evidence::Evidence,
    #[command(flatten)]
    output: artifact_output::Options,
}
pub(crate) fn run(
    root: &Path,
    metadata: &cargo_metadata::Metadata,
    options: &Options,
) -> Result<()> {
    options.evidence.check(
        "dependencies",
        "qualified relations",
        "for lexical file mentions use cargo atlas map --level files",
        "dump --json and flow accept --scip FILE",
    )?;
    let snapshot = discovery::snapshot_metadata(metadata)?;
    let graph = snapshot.graph();
    let chosen = options.scope.choose(graph, metadata)?;
    let requested = chosen.query();
    let scope = discovery::resolve_scope(graph, Some(&chosen.path(metadata, "dependencies")?))?;
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
    if requested.is_none() && !options.output.json() {
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
            "atlas: drew all {} packages; to draw one: cargo atlas dependencies {}",
            packages.len(),
            src.display()
        );
    }
}
