//! Revision review over one immutable-before/current-after snapshot pair.
use super::json;
use super::output::rows::{Rows, narrower, print_bounded, rerun_all, tail};
use cargo_atlas::{Result, discovery, revision::Revision};
use clap::Args;
use std::path::Path;

#[derive(Args)]
pub(crate) struct Options {
    #[command(flatten)]
    pub(super) revision: crate::cli::subject::Revision,
    /// Physical source scope, including removed files or folders.
    #[arg(
        long = "in",
        value_name = "PATH",
        default_value = ".",
        conflicts_with = "scope"
    )]
    in_path: String,
    /// Superseded by --in.
    #[arg(long, value_name = "PATH")]
    scope: Option<String>,
    #[command(flatten)]
    output: crate::cli::output::form::Report,
    #[command(flatten)]
    rows: Rows,
}
impl Options {
    fn scope(&self) -> &str {
        self.scope.as_deref().unwrap_or(&self.in_path)
    }
}
#[derive(Args)]
pub(crate) struct DiffOptions {
    #[command(flatten)]
    pub(super) comparison: Options,
    /// Review changed captured declarations, including private subjects by default.
    #[arg(long)]
    items: bool,
    /// Include prior/current known incident co-mentions; requires --items.
    #[arg(long, requires = "items")]
    graph: bool,
    #[command(flatten)]
    population: crate::cli::navigation::PopulationArgs,
}
#[derive(Clone, Copy)]
pub(crate) enum Question {
    Shape,
    Visibility,
    Moved,
}
impl Question {
    fn label(self) -> &'static str {
        match self {
            Self::Shape => "diff",
            Self::Visibility => "visibility",
            Self::Moved => "moved",
        }
    }
}
pub(crate) fn run(
    root: &Path,
    metadata: &cargo_metadata::Metadata,
    options: &Options,
    question: Question,
) -> Result<()> {
    let (commit, before_snapshot, after_snapshot) =
        captures(root, metadata, options.revision.reference())?;
    match question {
        Question::Shape => {
            let report =
                atlas_lib::review::diff(&before_snapshot, &after_snapshot, options.scope())?;
            if options.output.json() {
                print_json(&commit, question, &report)?;
            } else {
                println!("Module shape against {commit} (current source)");
                let content_only = report.changes.iter().filter(|c| !c.shape_changed).count();
                let changed: Vec<_> = report.changes.iter().filter(|c| c.shape_changed).collect();
                let limit = options.rows.limit(15);
                for change in changed.iter().take(limit) {
                    println!("{}", change.file);
                    for (side, shape) in [("before", &change.before), ("after", &change.after)] {
                        if let Some(shape) = shape {
                            println!(
                                "  {side}: {} non-test lines; {} top-level items; largest block {}",
                                shape.non_test_lines,
                                shape.top_level_items,
                                shape.largest_block.as_ref().map_or(0, |block| block.lines)
                            );
                        } else {
                            println!("  {side}: absent");
                        }
                    }
                }
                if changed.len() > limit {
                    println!("{}", tail("", changed.len() - limit, &rerun_all(None)));
                }
                same_shape_note(content_only);
            }
        }
        Question::Visibility => {
            let report = atlas_lib::review::moves::visibility(
                &before_snapshot,
                &after_snapshot,
                Some(options.scope()),
            )?;
            if options.output.json() {
                print_json(&commit, question, &report)?;
            } else {
                // Repeated keys matter only as a caveat: say how many, once.
                let approximate = match report.ambiguous_keys {
                    0 => String::new(),
                    1 => " (1 item shares a name with another and was paired approximately; --json lists it)".to_owned(),
                    n => format!(
                        " ({n} items share a name and were paired approximately; --json lists them)"
                    ),
                };
                let changes = report.widened.len() + report.rescoped.len() + report.narrowed.len();
                if changes == 0 {
                    println!("No visibility changes against {commit}{approximate}.");
                } else {
                    println!(
                        "Written visibility against {commit}: {} widened; {} rescoped; {} narrowed{approximate}",
                        report.widened.len(),
                        report.rescoped.len(),
                        report.narrowed.len(),
                    );
                }
                for (label, rows) in [
                    ("widened", &report.widened),
                    ("rescoped", &report.rescoped),
                    ("narrowed", &report.narrowed),
                ] {
                    print_bounded(
                        rows,
                        options.rows.limit(15),
                        "",
                        || rerun_all(None),
                        |row| pair(label, row),
                    );
                }
                for limit in &report.limits {
                    println!("limit: {limit}");
                }
            }
        }
        Question::Moved => {
            let report = atlas_lib::review::moves::moved(
                &before_snapshot,
                &after_snapshot,
                Some(options.scope()),
            )?;
            if options.output.json() {
                print_json(&commit, question, &report)?;
            } else {
                println!(
                    "Token bodies against {}: {} exact; {} rerooted; {} changed; {} only before; {} only after; {} unreadable ({} exact/rerooted relocated)",
                    commit,
                    report.exact,
                    report.rerooted.len(),
                    report.changed.len(),
                    report.only_before.len(),
                    report.only_after.len(),
                    report.unreadable.len(),
                    report.relocated
                );
                let limit = options.rows.limit(8);
                for (label, rows) in [
                    ("relocated", &report.exact_relocated),
                    ("rerooted (review)", &report.rerooted),
                    ("changed", &report.changed),
                ] {
                    print_bounded(rows, limit, "", || rerun_all(None), |row| pair(label, row));
                }
                for (label, rows) in [
                    ("only before", &report.only_before),
                    ("only after", &report.only_after),
                    ("unreadable", &report.unreadable),
                ] {
                    print_bounded(
                        rows,
                        limit,
                        "",
                        || rerun_all(None),
                        |row| {
                            println!(
                                "{label}: {} {} {} at {}:{}",
                                row.crate_name, row.class, row.name, row.site.file, row.site.line
                            );
                        },
                    );
                }
                for limit in &report.limits {
                    println!("limit: {limit}");
                }
            }
        }
    }
    Ok(())
}
fn print_json(commit: &str, question: Question, report: &impl serde::Serialize) -> Result<()> {
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "schema": 1,
            "question": question.label(),
            "basis": {
                "before": commit,
                "after": null,
            },
            "report": report,
        }))?
    );
    Ok(())
}
fn pair(label: &str, row: &atlas_lib::review::moves::Pair) {
    println!(
        "{label}: {} {} {} · {}:{} ({}) -> {}:{} ({}){}",
        row.crate_name,
        row.class,
        row.name,
        row.before.file,
        row.before.line,
        row.before.visibility,
        row.after.file,
        row.after.line,
        row.after.visibility,
        if row.ambiguous {
            " [ambiguous pairing]"
        } else {
            ""
        }
    );
}

/// One line for the files whose text changed but whose shape did not.
fn same_shape_note(count: usize) {
    if count > 0 {
        let (noun, pronoun) = if count == 1 {
            ("file", "it")
        } else {
            ("files", "them")
        };
        println!("{count} more {noun} changed text with the same shape; --json lists {pronoun}");
    }
}

fn captures(
    root: &Path,
    metadata: &cargo_metadata::Metadata,
    reference: &str,
) -> Result<(
    String,
    atlas_lib::discovery::Snapshot,
    atlas_lib::discovery::Snapshot,
)> {
    let (commit, before) = {
        let revision = Revision::read(root, reference)?;
        let snapshot = atlas_lib::discovery::snapshot_workspace(revision.path())?;
        (revision.commit_id().to_owned(), snapshot)
    };
    let after = discovery::snapshot_metadata(metadata)?;
    Ok((commit, before, after))
}
#[derive(serde::Serialize)]
struct DeclarationReview {
    shapes: atlas_lib::review::ShapeDiff,
    items: atlas_lib::queries::navigation::DeclarationDiff,
}
pub(crate) fn diff(
    root: &Path,
    metadata: &cargo_metadata::Metadata,
    options: &DiffOptions,
) -> Result<()> {
    if !options.items {
        if options.population.selected().is_some() {
            return Err("diff population controls select declaration subjects; add --items".into());
        }
        return run(root, metadata, &options.comparison, Question::Shape);
    }
    let args = &options.comparison;
    let (commit, before, after) = captures(root, metadata, args.revision.reference())?;
    let population = options
        .population
        .selected()
        .unwrap_or(atlas_lib::population::VisibilitySelection::All);
    let prior = atlas_lib::queries::navigation::Navigation::new(&before, population)?;
    let current = atlas_lib::queries::navigation::Navigation::new(&after, population)?;
    let report = DeclarationReview {
        shapes: atlas_lib::review::diff(&before, &after, args.scope())?,
        items: current.declaration_diff(&prior, args.scope(), options.graph)?,
    };
    if args.output.json() {
        return print_json(&commit, Question::Shape, &report);
    }
    println!(
        "Declaration review against {commit} · {} subjects",
        population.label()
    );
    let rows = args.rows;
    let shaped: Vec<_> = report
        .shapes
        .changes
        .iter()
        .filter(|c| c.shape_changed)
        .collect();
    print_bounded(
        &shaped,
        rows.limit(10),
        "",
        || rerun_all(None),
        |module| {
            println!("module shape: {}", module.file);
        },
    );
    let content_only = report
        .shapes
        .changes
        .iter()
        .filter(|c| !c.shape_changed)
        .count();
    same_shape_note(content_only);
    let limit = rows.limit(12);
    for change in report.items.declarations.iter().take(limit) {
        use atlas_lib::queries::navigation::ChangedDeclaration;
        let (label, version) = match change {
            ChangedDeclaration::Added { after } => ("added", after),
            ChangedDeclaration::Removed { before } => ("removed", before),
            ChangedDeclaration::Modified { after, .. } => ("modified", after.as_ref()),
            // A change this text output does not know is named, not dropped;
            // the JSON report carries it in full.
            _ => {
                println!(
                    "changed: a declaration change of a kind this text output does not show; see --json"
                );
                continue;
            }
        };
        println!(
            "{label}: {}:{} {} [{}]",
            version.site.file, version.site.line, version.site.name, version.owner
        );
        // The first line names the item; --json holds the whole block.
        println!(
            "  {}",
            version.declaration_block.lines().next().unwrap_or_default()
        );
    }
    if report.items.declarations.len() > limit {
        println!(
            "{}",
            tail(
                "",
                report.items.declarations.len() - limit,
                &rerun_all(None)
            )
        );
    }
    if let Some(relationships) = &report.items.relationships {
        print_bounded(
            relationships,
            rows.limit(5),
            "",
            || rerun_all(None),
            |incident| {
                println!(
                    "{:?}: {}:{} {} -{}-> {} [{:?}]",
                    incident.side,
                    incident.edge.source.file,
                    incident.edge.source.line,
                    incident.edge.source.name,
                    incident.edge.evidence.label(),
                    incident.edge.target,
                    incident.edge.resolution
                );
            },
        );
    }
    println!("{}", report.items.limits);
    for (side, basis) in [
        ("prior", &report.items.before),
        ("current", &report.items.after),
    ] {
        print_bounded(
            &basis.diagnostics,
            rows.limit(5),
            "",
            || rerun_all(None),
            |diagnostic| {
                println!(
                    "{side} source limit: {}: {}",
                    diagnostic.file, diagnostic.message
                );
            },
        );
    }
    Ok(())
}

#[derive(Args)]
pub(super) struct SourceReviewArgs {
    #[command(flatten)]
    pub(super) scope: crate::cli::subject::Scope,
    #[command(flatten)]
    output: crate::cli::output::form::Report,
    #[command(flatten)]
    rows: Rows,
}
#[derive(Clone, Copy)]
pub(super) enum SourceQuestion {
    Spans,
    Loc,
    Concentration,
}
impl SourceQuestion {
    fn name(&self) -> &'static str {
        match self {
            Self::Spans => "spans",
            Self::Loc => "nontest-loc",
            Self::Concentration => "concentration",
        }
    }
}
pub(super) fn source_review(
    metadata: &cargo_metadata::Metadata,
    args: &SourceReviewArgs,
    question: SourceQuestion,
) -> Result<()> {
    let snapshot = atlas_lib::discovery::snapshot_metadata(metadata)?;
    let chosen = args.scope.choose(snapshot.graph(), metadata)?;
    let report = atlas_lib::review::outline(&snapshot, &chosen.path(metadata, question.name())?)?;
    if args.output.json() {
        println!(
            "{}",
            json(&serde_json::json!({
                "schema": 1,
                "question": question.name(),
                "report": report,
            }))?
        );
    } else {
        // spans lists each file's items, so it shows fewer files and shares
        // the page between their item lists.
        let limit = args
            .rows
            .limit(if matches!(question, SourceQuestion::Spans) {
                8
            } else {
                20
            });
        let shown = limit.min(report.files.len());
        for file in &report.files[..shown] {
            if matches!(question, SourceQuestion::Loc) {
                println!("{} {}", file.non_test_lines, file.file);
            } else if matches!(question, SourceQuestion::Concentration) {
                let depth = file.largest_block.as_ref().map_or(0, |b| b.lines);
                println!(
                    "{}: {} non-test lines, {} top-level items, largest block {}",
                    file.file, file.non_test_lines, file.top_level_items, depth
                );
            } else {
                println!("{}", file.file);
                let items = args.rows.nested(shown, 45);
                print_bounded(
                    &file.items,
                    items,
                    "  ",
                    || narrower("spans", &file.file, &[]),
                    |item| {
                        println!(
                            "  {}:{}-{} {} {}",
                            item.declaration, item.first, item.last, item.kind, item.name
                        );
                    },
                );
                if !file.uncovered_code.is_empty() {
                    println!("  uncovered code: {:?}", file.uncovered_code);
                }
            }
        }
        if report.files.len() > shown {
            println!("{}", tail("", report.files.len() - shown, &rerun_all(None)));
        }
    }
    Ok(())
}
