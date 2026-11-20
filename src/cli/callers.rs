//! Who calls a function and what it calls, one level, from the qualified
//! calls `queries::callers` reads. Unresolved calls are reported as possible
//! callers or as calls out Atlas could not follow, never dropped.
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
pub(crate) fn run(
    metadata: &cargo_metadata::Metadata,
    options: &Options,
    direction: Direction,
) -> Result<()> {
    let (selector, file) = options.function.selector();
    let command = match direction {
        Direction::Callers => "callers",
        Direction::Callees => "callees",
    };
    options.evidence.check(
        command,
        "resolved calls",
        &format!("for lexical co-mentions use cargo atlas name {selector} --relations"),
        "dump --json and flow accept --scip FILE",
    )?;
    let snapshot = discovery::snapshot_metadata(metadata)?;
    let graph = snapshot.graph();
    let facts = atlas_lib::analysis::Analysis::new(&snapshot)?.representations()?;
    let within = options.within.filter(graph, metadata)?;
    let more = |_: Option<&str>| rerun_all(None);
    match direction {
        Direction::Callers => {
            let report = callers::callers(graph, &facts, selector, file, options.depth, &within)?;
            if options.output.json() {
                print!("{}", json(&report)?);
            } else {
                let answers = options.rows.limit(3);
                let rows = options.rows.nested(answers.min(report.answers.len()), 20);
                print!("{}", report.text_bounded(answers, rows, &more));
            }
        }
        Direction::Callees => {
            let report = callers::callees(graph, &facts, selector, file, options.depth, &within)?;
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
