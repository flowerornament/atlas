//! Navigation owns legacy lexical and explicit-population command routing.
//! The two report schemas are intentional compatibility contracts. Both read the
//! shared capture/grammar; selected native output is formatted only after selection.
use atlas_core::{
    Result, discovery,
    population::VisibilitySelection,
    queries::navigation::{self, Navigation},
};
use clap::Args;
use std::{fmt::Write as _, path::PathBuf};

#[derive(Args)]
pub(crate) struct PopulationArgs {
    /// Native source-tree subjects: all, pub-ish or pub. LOC remains outside test modules.
    #[arg(long = "population", conflicts_with = "pub_only")]
    selection: Option<VisibilitySelection>,
    /// Compatibility spelling of --population pub; enables the native-style report.
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
    Census(Option<&'a str>),
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
) -> Result<()> {
    let snapshot = discovery::snapshot_metadata(metadata)?;
    let navigation = Navigation::new(&snapshot, population)?;
    let text = match question {
        Question::Dump => report_json("dump", &navigation.dump()?)?,
        Question::Census(scope) => {
            let report = navigation.census(scope)?;
            if json {
                report_json("census", &report)?
            } else {
                census(&report)?
            }
        }
        Question::Name(query) => {
            let report = navigation.name(query)?;
            if json {
                report_json("name", &report)?
            } else {
                name(&report)?
            }
        }
        Question::Module { path, items, graph } => {
            let report = navigation.module(path)?;
            if json {
                report_json("module", &report)?
            } else {
                module(&report, items, graph)?
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

fn census(report: &navigation::Census) -> Result<String> {
    let navigation::Census::Rows(report) = report else {
        let navigation::Census::Module(report) = report else {
            return Err("unsupported census rung".into());
        };
        return module(report, false, false);
    };
    let mut text = format!(
        "atlas census — {} · {} subjects; native source-tree lines\n",
        report.scope,
        report.population.label()
    );
    for row in &report.rows {
        tally(&mut text, &row.key, row.tally)?;
    }
    tally(&mut text, "TOTAL", report.total)?;
    diagnostics(&mut text, &report.basis)?;
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
fn name(report: &navigation::Name) -> Result<String> {
    let mut text = format!(
        "atlas name — {} · {} {} matches, {} hidden; ~{} textual references\n",
        report.query,
        report.matches.len(),
        report.population.label(),
        report.hidden,
        report.refs_approx
    );
    for site in report.matches.iter().take(30) {
        writeln!(
            text,
            "{}:{} ({}) {:?} {} {}",
            site.file,
            site.line,
            site.package,
            site.kind,
            site.visibility_restriction.as_ref().map_or_else(
                || site.visibility.label().to_owned(),
                |scope| {
                    if site.visibility == atlas_core::source::Visibility::PubIn {
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
    if report.matches.len() > 30 {
        writeln!(
            text,
            "{} more matches; narrow with path::Name or use --json",
            report.matches.len() - 30
        )?;
    }
    for (label, sites) in [("reexport", &report.reexports), ("impl", &report.impls)] {
        for site in sites {
            writeln!(
                text,
                "{label}: {}:{} ({})",
                site.file, site.line, site.package
            )?;
        }
    }
    edges(&mut text, &report.edges)?;
    diagnostics(&mut text, &report.basis)?;
    Ok(text)
}
fn module(report: &navigation::Module, items: bool, graph: bool) -> Result<String> {
    let mut text = format!(
        "atlas module — {} · {} subjects\n",
        report.path,
        report.population.label()
    );
    for context in &report.contexts {
        writeln!(text, "package: {}", context.package)?;
        if context.header.is_empty() {
            writeln!(text, "MISSING module header")?;
        }
        for line in &context.header {
            writeln!(text, "//! {line}")?;
        }
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
            edges(&mut text, &context.edges)?;
        }
        if items {
            for item in &context.items {
                writeln!(
                    text,
                    "  {} {:?} {} {} · {}",
                    item.line,
                    item.kind,
                    item.visibility.label(),
                    item.name,
                    item.doc.as_deref().unwrap_or("MISSING doc comment")
                )?;
            }
        }
    }
    diagnostics(&mut text, &report.basis)?;
    Ok(text)
}
fn edges(text: &mut String, edges: &[navigation::Edge]) -> Result<()> {
    writeln!(
        text,
        "{} declaration co-mentions (first 8 shown):",
        edges.len()
    )?;
    for edge in edges.iter().take(8) {
        writeln!(
            text,
            "  {}:{} {} -{}-> {} [{:?}]",
            edge.source.file,
            edge.source.line,
            edge.source.name,
            edge.evidence.label(),
            edge.target,
            edge.resolution
        )?;
    }
    if edges.len() > 8 {
        writeln!(
            text,
            "  {} more; use --json for all observations",
            edges.len() - 8
        )?;
    }
    Ok(())
}
fn diagnostics(text: &mut String, basis: &navigation::Basis) -> Result<()> {
    if !basis.diagnostics.is_empty() {
        writeln!(
            text,
            "{} source limits (first 8 shown; --json retains all):",
            basis.diagnostics.len()
        )?;
    }
    for diagnostic in basis.diagnostics.iter().take(8) {
        let location = diagnostic.line.map_or_else(
            || diagnostic.file.clone(),
            |line| format!("{}:{line}", diagnostic.file),
        );
        writeln!(text, "source limit: {location}: {}", diagnostic.message)?;
    }
    Ok(())
}

#[derive(Args)]
pub(super) struct ScopeArgs {
    /// Compatibility positional project root; prefer --root.
    pub(super) root: Option<PathBuf>,
    scope: Option<String>,
    #[arg(long = "scope", conflicts_with = "scope")]
    named_scope: Option<String>,
    #[arg(long)]
    json: bool,
    #[command(flatten)]
    population: PopulationArgs,
}
#[derive(Args)]
#[command(allow_missing_positional = true)]
pub(super) struct RelatedNameArgs {
    /// Compatibility positional project root; prefer --root.
    pub(super) root: Option<PathBuf>,
    pub(super) name: String,
    /// Include incoming/outgoing approximate lexical co-mentions, not qualified uses.
    #[arg(long)]
    relations: bool,
    #[arg(long)]
    json: bool,
    #[command(flatten)]
    population: PopulationArgs,
}
#[derive(Args)]
#[command(allow_missing_positional = true)]
pub(super) struct ModuleArgs {
    /// Compatibility positional project root; prefer --root.
    pub(super) root: Option<PathBuf>,
    pub(super) name: String,
    #[arg(long)]
    json: bool,
    #[command(flatten)]
    population: PopulationArgs,
    /// Show every captured production item instead of the module summary alone.
    #[arg(long)]
    items: bool,
    /// Show declaration co-mentions; lexical, not a resolved call graph.
    #[arg(long)]
    graph: bool,
}
#[derive(Args)]
#[command(allow_missing_positional = true)]
pub(super) struct NameArgs {
    /// Compatibility positional project root; prefer --root.
    pub(super) root: Option<PathBuf>,
    pub(super) name: String,
    #[arg(long)]
    json: bool,
    #[command(flatten)]
    population: PopulationArgs,
}
#[derive(Args)]
pub(super) struct DumpArgs {
    /// Compatibility positional project root; prefer --root.
    pub(super) root: Option<PathBuf>,
    #[arg(long)]
    json: bool,
    #[command(flatten)]
    population: PopulationArgs,
}

// Legacy lexical output and native selected populations intentionally have different schemas.
pub(super) fn run_census(metadata: &cargo_metadata::Metadata, args: &ScopeArgs) -> Result<()> {
    let question = Question::Census(args.named_scope.as_deref().or(args.scope.as_deref()));
    dispatch(
        metadata,
        args.population.selected(),
        question,
        args.json,
        false,
    )
}
pub(super) fn run_name(metadata: &cargo_metadata::Metadata, args: &RelatedNameArgs) -> Result<()> {
    dispatch(
        metadata,
        args.population.selected(),
        Question::Name(&args.name),
        args.json,
        args.relations,
    )
}
pub(super) fn run_related(metadata: &cargo_metadata::Metadata, args: &NameArgs) -> Result<()> {
    dispatch(
        metadata,
        args.population.selected(),
        Question::Name(&args.name),
        args.json,
        true,
    )
}
pub(super) fn run_module(metadata: &cargo_metadata::Metadata, args: &ModuleArgs) -> Result<()> {
    let population = args
        .population
        .selected()
        .or_else(|| (args.items || args.graph).then_some(VisibilitySelection::RestrictedOrPublic));
    dispatch(
        metadata,
        population,
        Question::Module {
            path: &args.name,
            items: args.items,
            graph: args.graph,
        },
        args.json,
        false,
    )
}
pub(super) fn run_dump(metadata: &cargo_metadata::Metadata, args: &DumpArgs) -> Result<()> {
    dispatch(
        metadata,
        args.population.selected(),
        Question::Dump,
        true,
        false,
    )
}
fn dispatch(
    metadata: &cargo_metadata::Metadata,
    population: Option<VisibilitySelection>,
    question: Question<'_>,
    json: bool,
    relations: bool,
) -> Result<()> {
    if let Some(population) = population {
        selected(metadata, population, question, json)
    } else {
        legacy(metadata, question, json, relations)
    }
}
fn legacy(
    metadata: &cargo_metadata::Metadata,
    question: Question<'_>,
    json_output: bool,
    relations: bool,
) -> Result<()> {
    let graph = atlas::discovery::scan_metadata(metadata)?;
    let text = match question {
        Question::Census(scope) => {
            let result = atlas::queries::census(&graph, scope)?;
            if json_output {
                super::json(&result)?
            } else {
                result.text()
            }
        }
        Question::Name(name) => {
            let result = atlas::queries::name(&graph, name)?;
            if json_output {
                super::json(&result)?
            } else {
                result.text(relations)
            }
        }
        Question::Module { path, .. } => {
            let result = atlas::queries::module(&graph, path)?;
            if json_output {
                super::json(&result)?
            } else {
                result.text()
            }
        }
        Question::Dump => super::json(&graph)?,
    };
    print!("{text}");
    Ok(())
}
