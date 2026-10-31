//! Caller-selected documentation auditing; citation markers are data, never inferred project identity.
use atlas::Result;
use atlas_core::{
    documentation::{self, HeaderPopulation, HeaderScope, Policy},
    population::{Population, TestSelection, VisibilitySelection},
    source::ItemKind,
};
use clap::Args;
#[derive(Args)]
pub(crate) struct Options {
    #[arg(long, default_value = ".")]
    scope: String,
    /// Select the documentation audit; without it the command refuses.
    #[arg(long = "audit", required = true)]
    _audit: bool,
    /// Item visibility: all, pub or pub-ish. Test modules remain excluded.
    #[arg(long, default_value = "pub-ish")]
    population: VisibilitySelection,
    #[arg(long, conflicts_with = "population")]
    pub_only: bool,
    /// Header scope: file or any-module (the native coverage metric).
    #[arg(long, default_value = "file")]
    header_scope: HeaderScope,
    /// Citation substrings; omission leaves citation checking explicitly unrequested.
    #[arg(long, value_delimiter = ',')]
    citation_markers: Option<Vec<String>>,
    #[arg(long)]
    json: bool,
}
pub(crate) fn run(metadata: &cargo_metadata::Metadata, options: &Options) -> Result<()> {
    let snapshot = atlas_core::discovery::snapshot_metadata(metadata)?;
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
    let report = documentation::audit(&snapshot, &options.scope, &policy)?;
    if options.json {
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
        for finding in &report.findings {
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
            }
        }
    }
    Ok(())
}
