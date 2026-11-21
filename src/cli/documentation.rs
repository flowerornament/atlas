//! Caller-selected documentation auditing; citation markers are data, never inferred project identity.
use atlas_lib::{
    documentation::{self, HeaderPopulation, HeaderScope, Policy},
    population::{Population, TestSelection, VisibilitySelection},
    source::ItemKind,
};
use cargo_atlas::Result;
use clap::Args;
#[derive(Args)]
pub(crate) struct Options {
    #[command(flatten)]
    pub(super) scope: crate::cli::subject::Scope,
    /// Accepted for compatibility: the audit is all this command does.
    #[arg(long = "audit")]
    _audit: bool,
    /// Item visibility: all, pub or pub-ish. Test modules remain excluded.
    #[arg(long, default_value = "pub-ish")]
    population: VisibilitySelection,
    /// Deprecated spelling of --population pub, still accepted.
    #[arg(long, conflicts_with = "population")]
    pub_only: bool,
    /// Header scope: file or any-module (the native coverage metric).
    #[arg(long, default_value = "file")]
    header_scope: HeaderScope,
    /// Citation substrings; omission leaves citation checking explicitly unrequested.
    #[arg(long, value_delimiter = ',')]
    citation_markers: Option<Vec<String>>,
    #[command(flatten)]
    output: crate::cli::output::form::Report,
    #[command(flatten)]
    rows: crate::cli::output::rows::Rows,
}
#[expect(
    clippy::too_many_lines,
    reason = "longer than 70 lines; to be split along its decisions"
)]
pub(crate) fn run(metadata: &cargo_metadata::Metadata, options: &Options) -> Result<()> {
    let snapshot = atlas_lib::discovery::snapshot_metadata(metadata)?;
    let markers = options
        .citation_markers
        .as_ref()
        .map(|markers| markers.iter().map(String::as_str).collect::<Vec<_>>());
    let policy = Policy {
        items: Population {
            visibility: if options.pub_only {
                VisibilitySelection::Public
            } else {
                options.population
            },
            tests: TestSelection::OutsideTestModules,
        },
        headers: HeaderPopulation::AllFiles,
        header_scope: options.header_scope,
        item_kinds: &[
            ItemKind::Struct,
            ItemKind::Enum,
            ItemKind::Union,
            ItemKind::Trait,
            ItemKind::TypeAlias,
            ItemKind::Fn,
        ],
        citation_markers: markers.as_deref(),
    };
    let scope = options
        .scope
        .choose(snapshot.graph(), metadata)?
        .path(metadata, "comments")?;
    let report = documentation::audit(&snapshot, &scope, &policy)?;
    if options.output.json() {
        println!(
            "{}",
            serde_json::to_string_pretty(
                &serde_json::json!({"schema":1,"question":"comments","report":report})
            )?
        );
    } else {
        println!(
            "Documentation audit: {} files, {} selected items",
            report.checked_files, report.checked_items
        );
        if report.citation_markers.is_none() {
            println!("Citation checking not requested; pass --citation-markers to apply it.");
        }
        let limit = options.rows.limit(40);
        for finding in report.findings.iter().take(limit) {
            match finding {
                documentation::Finding::MissingHeader { file } => {
                    println!("{file}: no //! line documentation in the selected header scope");
                }
                documentation::Finding::UncitedHeader { file, .. } => {
                    println!(
                        "{file}: selected //! line documentation has no requested citation marker"
                    );
                }
                documentation::Finding::MissingOuterLineDoc { file, name, line } => {
                    println!("{file}:{line}: {name} has no outer /// comment");
                }
                // A finding this text output does not know is still shown.
                other => println!("{other:?}"),
            }
        }
        if report.findings.len() > limit {
            println!(
                "{}",
                crate::cli::output::rows::tail(
                    "",
                    report.findings.len() - limit,
                    &crate::cli::output::rows::rerun_all()
                )
            );
        }
    }
    Ok(())
}
