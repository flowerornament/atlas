//! Explicit population queries: capture, select native facts, then format.
use atlas_core::{
    Result, discovery,
    population::VisibilitySelection,
    queries::navigation::{self, Navigation},
};
use clap::Args;
use std::fmt::Write as _;

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
pub(crate) fn run(
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
    crate::json(&Report {
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
