//! Who calls a function and what it calls, to `--depth` levels, from the qualified
//! calls `queries::callers` reads, and with `--evidence semantic` also from a
//! SCIP index's reading. Unresolved calls are reported as possible callers or
//! as calls out Atlas could not follow, never dropped; the semantic reading
//! settles some, and says how.
use super::{
    json,
    output::rows::{Rows, rerun_all},
    subject::{Function, Within},
};
use cargo_atlas::{Result, discovery, queries::callers};
use clap::Args;

#[derive(Args)]
pub(crate) struct Options {
    #[command(flatten)]
    function: Function,
    #[command(flatten)]
    within: Within,
    /// Follow resolved calls this many levels (1 to 16); each row carries
    /// its depth.
    #[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u32).range(1..=i64::from(callers::MAX_DEPTH)))]
    depth: u32,
    #[command(flatten)]
    evidence: super::evidence::Evidence,
    /// Answer from this SCIP index's reading beside the qualified one
    /// (Atlas runs no indexer); --semantic makes one instead.
    #[arg(long, value_name = "FILE")]
    scip: Option<std::path::PathBuf>,
    #[command(flatten)]
    indexer: super::semantic::IndexerArgs,
    #[command(flatten)]
    tests: super::tests::Tests,
    #[command(flatten)]
    rows: Rows,
    #[command(flatten)]
    output: super::output::form::Report,
}
#[derive(Clone, Copy)]
pub(crate) enum Direction {
    Callers,
    Callees,
}
/// Whether the semantic index is asked for, once the evidence flags agree.
fn asks_semantic(options: &Options, command: &str, selector: &str) -> Result<bool> {
    options.evidence.semantic(
        command,
        "resolved calls",
        &format!(
            "for lexical co-mentions use {}",
            cargo_atlas::suggestion::Command::new("name")
                .arg(selector)
                .arg("--relations")
        ),
        options.scip.is_some() || options.indexer.semantic,
    )
}
pub(crate) fn run(
    metadata: &cargo_metadata::Metadata,
    options: &Options,
    direction: Direction,
    cache: &super::semantic::IndexCache,
) -> Result<()> {
    let (selector, file) = options.function.selector();
    let command = match direction {
        Direction::Callers => "callers",
        Direction::Callees => "callees",
    };
    let semantic = asks_semantic(options, command, selector)?;
    let snapshot = discovery::snapshot_metadata(metadata)?;
    let graph = snapshot.graph();
    let analysis =
        atlas_lib::analysis::Analysis::new(&snapshot)?.with_tests(options.tests.selection());
    let facts = analysis.representations()?;
    let reading = if semantic {
        let (index, _) = super::semantic::obtain(
            metadata.workspace_root.as_std_path(),
            metadata,
            options.scip.as_deref(),
            &options.indexer,
            cache,
            || super::output::graph_captures(metadata, graph).collect(),
        )?
        .ok_or("no semantic index was asked for")?;
        Some(analysis.semantic(&index)?)
    } else {
        None
    };
    let within = options.within.filter(graph, metadata)?;
    let more = |_: Option<&str>| rerun_all();
    let depth = options.depth;
    match direction {
        Direction::Callers => {
            let report = match &reading {
                Some(reading) => callers::semantic_callers(
                    graph, &facts, selector, file, depth, &within, reading,
                )?,
                None => callers::callers(graph, &facts, selector, file, depth, &within)?,
            };
            if options.output.json() {
                print!("{}", json(&report)?);
            } else {
                let answers = options.rows.limit(3);
                let rows = options.rows.nested(answers.min(report.answers.len()), 20);
                print!("{}", report.text_bounded(answers, rows, &more));
            }
        }
        Direction::Callees => {
            let report = match &reading {
                Some(reading) => callers::semantic_callees(
                    graph, &facts, selector, file, depth, &within, reading,
                )?,
                None => callers::callees(graph, &facts, selector, file, depth, &within)?,
            };
            if options.output.json() {
                print!("{}", json(&report)?);
            } else {
                let answers = options.rows.limit(3);
                let rows = options.rows.nested(answers.min(report.answers.len()), 20);
                print!("{}", report.text_bounded(answers, rows, &more));
            }
        }
    }
    Ok(())
}
