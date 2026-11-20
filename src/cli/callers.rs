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
    let snapshot = discovery::snapshot_metadata(metadata)?;
    let graph = snapshot.graph();
    let facts = atlas_lib::analysis::Analysis::new(&snapshot)?.representations()?;
    let within = options.within.filter(graph, metadata)?;
    let (selector, file) = options.function.selector();
    let more = |_: Option<&str>| rerun_all(None);
    match direction {
        Direction::Callers => {
            let report = callers::callers(graph, &facts, selector, file, &within)?;
            if options.output.json() {
                print!("{}", json(&report)?);
            } else {
                let answers = options.rows.limit(3);
                let rows = options.rows.nested(answers.min(report.answers.len()), 20);
                print!("{}", report.text_bounded(answers, rows, &more));
            }
        }
        Direction::Callees => {
            let report = callers::callees(graph, &facts, selector, file, &within)?;
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
