//! Navigation owns legacy lexical and explicit-population command routing.
//! The two report schemas are intentional compatibility contracts. Both read the
//! shared capture/grammar; selected native output is formatted only after selection.
use super::output::rows::{Rows, narrower, rerun_all, tail};
use atlas_lib::{
    Result, discovery,
    population::VisibilitySelection,
    queries::navigation::{self, Navigation},
};
use cargo_atlas::suggestion::{Command, PROGRAM};
use clap::Args;
use std::{fmt::Write as _, path::PathBuf};

#[derive(Args)]
pub(crate) struct PopulationArgs {
    /// Native source-tree subjects: all, pub-ish or pub. LOC remains outside test modules.
    #[arg(long = "population", conflicts_with = "pub_only")]
    selection: Option<VisibilitySelection>,
    /// Deprecated spelling of --population pub, still accepted.
    #[arg(long)]
    pub_only: bool,
}
impl PopulationArgs {
    pub(crate) fn selected(&self) -> Option<VisibilitySelection> {
        self.selection
            .or(self.pub_only.then_some(VisibilitySelection::Public))
    }
}
#[derive(Clone, Copy)]
pub(crate) enum Question<'a> {
    Dump,
    Census(&'a super::subject::Scope),
    Name(&'a str),
    Module {
        path: &'a str,
        items: bool,
        graph: bool,
    },
}
fn selected(
    metadata: &cargo_metadata::Metadata,
    population: VisibilitySelection,
    question: Question<'_>,
    json: bool,
    rows: Rows,
) -> Result<()> {
    let snapshot = discovery::snapshot_metadata(metadata)?;
    let navigation = Navigation::new(&snapshot, population)?;
    let text = match question {
        Question::Dump => report_json("dump", &navigation.dump()?)?,
        Question::Census(scope) => {
            let report = navigation.census(scope.choose(snapshot.graph(), metadata)?.query())?;
            if json {
                report_json("census", &report)?
            } else {
                census(&report, rows)?
            }
        }
        Question::Name(query) => {
            let report = navigation.name(query)?;
            if json {
                report_json("name", &report)?
            } else {
                name(&report, rows)?
            }
        }
        Question::Module { path, items, graph } => {
            let report = navigation.module(path)?;
            if json {
                report_json("module", &report)?
            } else {
                module(&report, items, graph, rows)?
            }
        }
    };
    println!("{text}");
    Ok(())
}
fn report_json<T: serde::Serialize>(question: &str, report: &T) -> Result<String> {
    #[derive(serde::Serialize)]
    struct Report<'a, T> {
        question: &'a str,
        #[serde(flatten)]
        facts: &'a T,
    }
    crate::cli::json(&Report {
        question,
        facts: report,
    })
}

fn census(report: &navigation::Census, rows: Rows) -> Result<String> {
    let navigation::Census::Rows(report) = report else {
        let navigation::Census::Module(report) = report else {
            return Err("unsupported census rung".into());
        };
        return module(report, false, false, rows);
    };
    let mut text = format!(
        "{PROGRAM} census — {} · {} subjects; native source-tree lines\n",
        report.scope,
        report.population.label()
    );
    let limit = rows.limit(20);
    for row in report.rows.iter().take(limit) {
        tally(&mut text, &row.key, row.tally)?;
    }
    more(&mut text, "", report.rows.len(), limit)?;
    tally(&mut text, "TOTAL", report.total)?;
    diagnostics(&mut text, &report.basis, rows)?;
    Ok(text)
}
fn tally(text: &mut String, key: &str, tally: navigation::Tally) -> Result<()> {
    let docs = tally
        .doc_pct()
        .map_or_else(|| "NA".into(), |percent| format!("{percent}%"));
    writeln!(
        text,
        "{key}: {} types, {} fns, {} aliases, {} macros; {} token-lines, {} non-test lines; docs {docs}",
        tally.types, tally.fns, tally.aliases, tally.macros, tally.token_lines, tally.nontest_loc
    )?;
    Ok(())
}
fn name(report: &navigation::Name, rows: Rows) -> Result<String> {
    let mut text = format!(
        "{PROGRAM} name — {} · {} {} in population {}, {} hidden; ~{} textual references\n",
        report.query,
        report.matches.len(),
        if report.matches.len() == 1 {
            "match"
        } else {
            "matches"
        },
        report.population.label(),
        report.hidden,
        report.refs_approx
    );
    let limit = rows.limit(10);
    for site in report.matches.iter().take(limit) {
        writeln!(
            text,
            "{}:{} ({}) {} {} {}",
            site.file,
            site.line,
            site.package,
            site.kind.keyword(),
            site.visibility_restriction.as_ref().map_or_else(
                || site.visibility.label().to_owned(),
                |scope| {
                    if site.visibility == atlas_lib::source::Visibility::PubIn {
                        format!("pub(in {scope})")
                    } else {
                        site.visibility.label().to_owned()
                    }
                }
            ),
            site.name
        )?;
        writeln!(
            text,
            "  {}",
            site.doc.as_deref().unwrap_or("MISSING doc comment")
        )?;
    }
    more(&mut text, "", report.matches.len(), limit)?;
    for (label, sites) in [("reexport", &report.reexports), ("impl", &report.impls)] {
        for site in sites.iter().take(limit) {
            writeln!(
                text,
                "{label}: {}:{} ({})",
                site.file, site.line, site.package
            )?;
        }
        more(&mut text, "", sites.len(), limit)?;
    }
    edges(&mut text, &report.edges, rows)?;
    diagnostics(&mut text, &report.basis, rows)?;
    Ok(text)
}
fn module(report: &navigation::Module, items: bool, graph: bool, rows: Rows) -> Result<String> {
    let mut text = format!(
        "{PROGRAM} module — {} · {} subjects\n",
        report.path,
        report.population.label()
    );
    for context in &report.contexts {
        writeln!(text, "package: {}", context.package)?;
        if context.header.is_empty() {
            writeln!(text, "MISSING module header")?;
        }
        let limit = rows.limit(5);
        for line in context.header.iter().take(limit) {
            writeln!(text, "//! {line}")?;
        }
        more(&mut text, "", context.header.len(), limit)?;
        tally(&mut text, &report.path, context.tally)?;
        writeln!(
            text,
            "{} undocumented subjects; {} imports ({} internal)",
            context.undocumented, context.imports, context.internal_imports
        )?;
        match &context.fan_in_files {
            Some(files) => writeln!(
                text,
                "fan-in: {} files ({}) [source-use heuristic]",
                files.len(),
                files.join(", ")
            )?,
            None => writeln!(text, "fan-in: NA at crate root")?,
        }
        writeln!(
            text,
            "unmentioned names: {} [workspace-wide spelling heuristic]",
            context.unmentioned_names.join(", ")
        )?;
        if graph {
            edges(&mut text, &context.edges, rows)?;
        }
        if items {
            let limit = rows.limit(30);
            for item in context.items.iter().take(limit) {
                writeln!(
                    text,
                    "  {} {} {} {} · {}",
                    item.line,
                    item.kind.keyword(),
                    item.visibility.label(),
                    item.name,
                    item.doc.as_deref().unwrap_or("MISSING doc comment")
                )?;
            }
            more(&mut text, "  ", context.items.len(), limit)?;
        }
    }
    diagnostics(&mut text, &report.basis, rows)?;
    Ok(text)
}
fn edges(text: &mut String, edges: &[navigation::Edge], rows: Rows) -> Result<()> {
    let limit = rows.limit(8);
    writeln!(text, "{} declaration co-mentions:", edges.len())?;
    for edge in edges.iter().take(limit) {
        writeln!(
            text,
            "  {}:{} {} -{}-> {} [{}]",
            edge.source.file,
            edge.source.line,
            edge.source.name,
            edge.evidence.label(),
            edge.target,
            edge.resolution
        )?;
    }
    more(text, "  ", edges.len(), limit)
}
/// The tail of a text list of `total` rows that showed `limit`.
fn more(text: &mut String, indent: &str, total: usize, limit: usize) -> Result<()> {
    if total > limit {
        writeln!(text, "{}", tail(indent, total - limit, &rerun_all()))?;
    }
    Ok(())
}
fn diagnostics(text: &mut String, basis: &navigation::Basis, rows: Rows) -> Result<()> {
    let limit = rows.limit(8);
    if !basis.diagnostics.is_empty() {
        writeln!(text, "{} source limits:", basis.diagnostics.len())?;
    }
    for diagnostic in basis.diagnostics.iter().take(limit) {
        let location = diagnostic.line.map_or_else(
            || diagnostic.file.clone(),
            |line| format!("{}:{line}", diagnostic.file),
        );
        writeln!(text, "source limit: {location}: {}", diagnostic.message)?;
    }
    more(text, "", basis.diagnostics.len(), limit)
}

#[derive(Args)]
pub(super) struct ScopeArgs {
    #[command(flatten)]
    pub(super) scope: super::subject::Scope,
    #[command(flatten)]
    output: crate::cli::output::form::Report,
    #[command(flatten)]
    population: PopulationArgs,
    #[command(flatten)]
    rows: Rows,
}
#[derive(Args)]
pub(super) struct RelatedNameArgs {
    #[command(flatten)]
    pub(super) name: super::subject::Name,
    /// Include incoming/outgoing approximate lexical co-mentions, not qualified uses.
    #[arg(long)]
    relations: bool,
    #[command(flatten)]
    output: crate::cli::output::form::Report,
    #[command(flatten)]
    population: PopulationArgs,
    #[command(flatten)]
    rows: Rows,
}
#[derive(Args)]
pub(super) struct ModuleArgs {
    #[command(flatten)]
    pub(super) name: super::subject::Name,
    #[command(flatten)]
    output: crate::cli::output::form::Report,
    #[command(flatten)]
    population: PopulationArgs,
    /// Show every captured production item instead of the module summary alone.
    #[arg(long)]
    items: bool,
    /// Show declaration co-mentions; lexical, not a resolved call graph.
    #[arg(long)]
    graph: bool,
    #[command(flatten)]
    rows: Rows,
}
#[derive(Args)]
pub(super) struct NameArgs {
    #[command(flatten)]
    pub(super) name: super::subject::Name,
    #[command(flatten)]
    output: crate::cli::output::form::Report,
    #[command(flatten)]
    population: PopulationArgs,
    #[command(flatten)]
    rows: Rows,
}
#[derive(Args)]
pub(super) struct DumpArgs {
    #[command(flatten)]
    pub(super) root: super::subject::Root,
    #[command(flatten)]
    output: crate::cli::output::form::Report,
    #[command(flatten)]
    population: PopulationArgs,
    /// Add a `semantic` field: every production call read against this SCIP
    /// index (for example from `rust-analyzer scip`), beside its qualified
    /// target. A relative path is read from the current directory. Atlas
    /// runs no indexer.
    #[arg(long, value_name = "FILE", value_parser = crate::cli::typed::file)]
    scip: Option<PathBuf>,
    #[command(flatten)]
    indexer: super::semantic::IndexerArgs,
}

// Legacy lexical output and native selected populations intentionally have different schemas.
pub(super) fn run_census(metadata: &cargo_metadata::Metadata, args: &ScopeArgs) -> Result<()> {
    dispatch(
        metadata,
        args.population.selected(),
        Question::Census(&args.scope),
        args.output.json(),
        false,
        args.rows,
    )
}
/// A name question's selector, its `FILE:LINE` read from the current
/// directory.
fn typed_name(metadata: &cargo_metadata::Metadata, name: &str) -> Result<String> {
    let root = metadata.workspace_root.as_std_path().canonicalize()?;
    super::typed::selector(name, &root)
}
pub(super) fn run_name(metadata: &cargo_metadata::Metadata, args: &RelatedNameArgs) -> Result<()> {
    let name = typed_name(metadata, args.name.subject())?;
    dispatch(
        metadata,
        args.population.selected(),
        Question::Name(&name),
        args.output.json(),
        args.relations,
        args.rows,
    )
}
pub(super) fn run_related(metadata: &cargo_metadata::Metadata, args: &NameArgs) -> Result<()> {
    let name = typed_name(metadata, args.name.subject())?;
    dispatch(
        metadata,
        args.population.selected(),
        Question::Name(&name),
        args.output.json(),
        true,
        args.rows,
    )
}
pub(super) fn run_module(metadata: &cargo_metadata::Metadata, args: &ModuleArgs) -> Result<()> {
    let population = args
        .population
        .selected()
        .or_else(|| (args.items || args.graph).then_some(VisibilitySelection::RestrictedOrPublic));
    // A path is read from the current directory; a package name as written.
    let subject = args.name.subject();
    let root = metadata.workspace_root.as_std_path().canonicalize()?;
    let path = super::typed::path(subject, &root)?.unwrap_or_else(|| subject.to_owned());
    dispatch(
        metadata,
        population,
        Question::Module {
            path: &path,
            items: args.items,
            graph: args.graph,
        },
        args.output.json(),
        false,
        args.rows,
    )
}
pub(super) fn run_dump(
    root: &std::path::Path,
    metadata: &cargo_metadata::Metadata,
    args: &DumpArgs,
    cache: &super::semantic::IndexCache,
) -> Result<()> {
    if args.scip.is_some() || args.indexer.semantic {
        let snapshot = discovery::snapshot_metadata(metadata)?;
        let (index, indexing) = super::semantic::obtain(
            root,
            metadata,
            args.scip.as_deref(),
            &args.indexer,
            cache,
            || super::output::graph_captures(metadata, snapshot.graph()).collect(),
        )?
        .ok_or("no semantic index was asked for")?;
        let semantic = atlas_lib::analysis::Analysis::new(&snapshot)?.semantic(&index)?;
        let text = if let Some(population) = args.population.selected() {
            let dump = Navigation::new(&snapshot, population)?.dump()?;
            report_json(
                "dump",
                &super::semantic::WithSemantic {
                    report: &dump,
                    semantic: &semantic,
                    semantic_indexing: indexing.as_ref(),
                },
            )?
        } else {
            super::json(&super::semantic::WithSemantic {
                // The graph the semantic reading was made from.
                report: snapshot.graph(),
                semantic: &semantic,
                semantic_indexing: indexing.as_ref(),
            })?
        };
        print!("{text}");
        return Ok(());
    }
    dispatch(
        metadata,
        args.population.selected(),
        Question::Dump,
        true,
        false,
        Rows::default(),
    )
}
fn dispatch(
    metadata: &cargo_metadata::Metadata,
    population: Option<VisibilitySelection>,
    question: Question<'_>,
    json: bool,
    relations: bool,
    rows: Rows,
) -> Result<()> {
    if let Some(population) = population {
        selected(metadata, population, question, json, rows)
    } else {
        legacy(metadata, question, json, relations, rows)
    }
}
fn legacy(
    metadata: &cargo_metadata::Metadata,
    question: Question<'_>,
    json_output: bool,
    relations: bool,
    rows: Rows,
) -> Result<()> {
    let graph = cargo_atlas::discovery::scan_metadata(metadata)?;
    let text = match question {
        Question::Census(scope) => {
            let chosen = scope.choose(&graph, metadata)?;
            let result = cargo_atlas::queries::census(&graph, chosen.query())?;
            if json_output {
                super::json(&result)?
            } else {
                result.text()
            }
        }
        Question::Name(name) => {
            let result = cargo_atlas::queries::name(&graph, name)?;
            if json_output {
                report_json("name", &result)?
            } else {
                // With --relations each declaration carries a list, so fewer
                // declarations share the page.
                let cards = rows.limit(if relations { 4 } else { 10 });
                let shown = cards.min(result.matches.len());
                result.text_bounded(relations, cards, rows.nested(shown, 40), &|id| {
                    id.map_or_else(rerun_all, |id| {
                        narrower(Command::new("name").arg(id).arg("--relations"))
                    })
                })
            }
        }
        Question::Module { path, .. } => {
            let result = cargo_atlas::queries::module(&graph, path)?;
            if json_output {
                super::json(&result)?
            } else {
                let files = rows.limit(8);
                let shown = files.min(result.files.len());
                // 8 files of 3 declarations, or one file of 24, fill a page.
                result.text_bounded(files, rows.nested(shown, 24), &|file| {
                    file.map_or_else(rerun_all, |file| {
                        narrower(Command::new("module").path(file))
                    })
                })
            }
        }
        Question::Dump => super::json(&graph)?,
    };
    print!("{text}");
    Ok(())
}
