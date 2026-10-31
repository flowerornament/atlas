//! Revision review over one immutable-before/current-after snapshot pair.
use super::json;
use atlas::{Result, discovery, revision::Revision};
use clap::Args;
use std::path::Path;

#[derive(Args)]
pub(crate) struct Options {
    /// Published revision to compare against current source.
    reference: String,
    /// Physical source scope, including removed files or folders.
    #[arg(long, default_value = ".")]
    scope: String,
    #[arg(long)]
    json: bool,
}
#[derive(Args)]
pub(crate) struct DiffOptions {
    #[command(flatten)]
    comparison: Options,
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
    let (commit, before_snapshot, after_snapshot) = captures(root, metadata, &options.reference)?;
    match question {
        Question::Shape => {
            let report =
                atlas_core::review::diff(&before_snapshot, &after_snapshot, &options.scope)?;
            if options.json {
                print_json(&commit, question, &report)?;
            } else {
                println!("Module shape against {commit} (current source)");
                for change in &report.changes {
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
            }
        }
        Question::Visibility => {
            let report = atlas_core::review::moves::visibility(
                &before_snapshot,
                &after_snapshot,
                Some(&options.scope),
            )?;
            if options.json {
                print_json(&commit, question, &report)?;
            } else {
                println!(
                    "Written visibility against {}: {} widened; {} rescoped; {} narrowed; {} ambiguous keys",
                    commit,
                    report.widened.len(),
                    report.rescoped.len(),
                    report.narrowed.len(),
                    report.ambiguous_keys
                );
                for (label, rows) in [
                    ("widened", &report.widened),
                    ("rescoped", &report.rescoped),
                    ("narrowed", &report.narrowed),
                ] {
                    for row in rows {
                        pair(label, row);
                    }
                }
                for limit in &report.limits {
                    println!("limit: {limit}");
                }
            }
        }
        Question::Moved => {
            let report = atlas_core::review::moves::moved(
                &before_snapshot,
                &after_snapshot,
                Some(&options.scope),
            )?;
            if options.json {
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
                for row in &report.rerooted {
                    pair("rerooted (review)", row);
                }
                for row in &report.changed {
                    pair("changed", row);
                }
                for (label, rows) in [
                    ("only before", &report.only_before),
                    ("only after", &report.only_after),
                    ("unreadable", &report.unreadable),
                ] {
                    for row in rows {
                        println!(
                            "{label}: {} {} {} at {}:{}",
                            row.crate_name, row.class, row.name, row.site.file, row.site.line
                        );
                    }
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
fn pair(label: &str, row: &atlas_core::review::moves::Pair) {
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

fn captures(
    root: &Path,
    metadata: &cargo_metadata::Metadata,
    reference: &str,
) -> Result<(
    String,
    atlas_core::discovery::Snapshot,
    atlas_core::discovery::Snapshot,
)> {
    let (commit, before) = {
        let revision = Revision::read(root, reference)?;
        let snapshot = atlas_core::discovery::snapshot_workspace(revision.path())?;
        (revision.commit_id().to_owned(), snapshot)
    };
    let after = discovery::snapshot_metadata(metadata)?;
    Ok((commit, before, after))
}
#[derive(serde::Serialize)]
struct DeclarationReview {
    shapes: atlas_core::review::ShapeDiff,
    items: atlas_core::queries::navigation::DeclarationDiff,
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
    let (commit, before, after) = captures(root, metadata, &args.reference)?;
    let population = options
        .population
        .selected()
        .unwrap_or(atlas_core::population::VisibilitySelection::All);
    let prior = atlas_core::queries::navigation::Navigation::new(&before, population)?;
    let current = atlas_core::queries::navigation::Navigation::new(&after, population)?;
    let report = DeclarationReview {
        shapes: atlas_core::review::diff(&before, &after, &args.scope)?,
        items: current.declaration_diff(&prior, &args.scope, options.graph)?,
    };
    if args.json {
        return print_json(&commit, Question::Shape, &report);
    }
    println!(
        "Declaration review against {commit} · {} subjects",
        population.label()
    );
    for module in &report.shapes.changes {
        println!("module shape: {}", module.file);
    }
    for change in &report.items.declarations {
        use atlas_core::queries::navigation::ChangedDeclaration;
        let (label, version) = match change {
            ChangedDeclaration::Added { after } => ("added", after),
            ChangedDeclaration::Removed { before } => ("removed", before),
            ChangedDeclaration::Modified { after, .. } => ("modified", after.as_ref()),
        };
        println!(
            "{label}: {}:{} {} [{}]",
            version.site.file, version.site.line, version.site.name, version.owner
        );
        println!("  {}", version.declaration_block);
    }
    if let Some(relationships) = &report.items.relationships {
        for incident in relationships {
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
        }
    }
    println!("{}", report.items.limits);
    for (side, basis) in [
        ("prior", &report.items.before),
        ("current", &report.items.after),
    ] {
        for diagnostic in &basis.diagnostics {
            println!(
                "{side} source limit: {}: {}",
                diagnostic.file, diagnostic.message
            );
        }
    }
    Ok(())
}

#[derive(Args)]
pub(super) struct SourceReviewArgs {
    /// Compatibility positional source scope; defaults to the whole workspace.
    path: Option<String>,
    /// Physical source file or directory; no matches is an error.
    #[arg(long, conflicts_with = "path")]
    scope: Option<String>,
    #[arg(long)]
    json: bool,
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
    let snapshot = atlas_core::discovery::snapshot_metadata(metadata)?;
    let report = atlas_core::review::outline(
        &snapshot,
        args.scope
            .as_deref()
            .or(args.path.as_deref())
            .unwrap_or("."),
    )?;
    if args.json {
        println!(
            "{}",
            json(&serde_json::json!({
                "schema": 1,
                "question": question.name(),
                "report": report,
            }))?
        );
    } else {
        for file in &report.files {
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
                for item in &file.items {
                    println!(
                        "  {}:{}-{} {} {}",
                        item.declaration, item.first, item.last, item.kind, item.name
                    );
                }
                if !file.uncovered_code.is_empty() {
                    println!("  uncovered code: {:?}", file.uncovered_code);
                }
            }
        }
    }
    Ok(())
}
