//! Saved questions feed native projections; output is explicit and source-safe.
use cargo_atlas::{
    Result, discovery,
    render::View,
    types::{self, Panel, Query},
};
use clap::Args;
use std::{
    fs,
    path::{Path, PathBuf},
};
#[derive(Args)]
pub(crate) struct Options {
    /// Compatibility positional project root; prefer --root.
    pub root: Option<PathBuf>,
    /// Saved selection: title, types, optional apis and preparation_files; no edges.
    #[arg(long, conflicts_with = "types")]
    query: Option<PathBuf>,
    /// Select types without a query file (repeat or comma separate).
    #[arg(long = "type", value_delimiter = ',')]
    types: Vec<String>,
    #[arg(long, value_enum, default_value = "all")]
    panel: Panel,
    #[command(flatten)]
    output: crate::cli::output::artifact::Options,
}
pub(crate) fn run(
    root: &Path,
    metadata: &cargo_metadata::Metadata,
    options: &Options,
) -> Result<()> {
    let query_path = options
        .query
        .as_ref()
        .map(|p| root.join(p).canonicalize())
        .transpose()?;
    let query: Query = if let Some(p) = &query_path {
        serde_json::from_str(&fs::read_to_string(p)?)?
    } else {
        if options.types.is_empty() {
            let example = most_mentioned_type(metadata).map_or_else(String::new, |name| {
                format!("; run: cargo atlas types --type {name}")
            });
            return Err(format!(
                "choose --query FILE or --type NAME; use overview for source-declared defaults{example}"
            )
            .into());
        }
        Query {
            title: format!("Types: {}", options.types.join(", ")),
            types: options
                .types
                .iter()
                .map(|n| types::Selector {
                    name: n.clone(),
                    file: None,
                })
                .collect(),
            apis: vec![],
            preparation_files: vec![],
            preparation_functions: vec![],
        }
    };
    let snapshot = discovery::snapshot_metadata(metadata)?;
    let graph = snapshot.graph();
    let facts = atlas_lib::analysis::Analysis::new(&snapshot)?.representations()?;
    let views = types::project(&facts, &query)?;
    options.output.emit(
        root,
        "types",
        &serde_json::json!({"facts":facts,"views":views}),
        &crate::cli::output::protected_inputs(metadata, graph, query_path),
        || {
            options
                .output
                .render_view(View::Types(&views, options.panel))
        },
    )
}

/// The struct or enum the most declarations mention, for an example that
/// names a real type: ties go to the first name.
fn most_mentioned_type(metadata: &cargo_metadata::Metadata) -> Option<String> {
    use atlas_lib::{model::Resolution, source::ItemKind};
    let snapshot = discovery::snapshot_metadata(metadata).ok()?;
    let graph = snapshot.graph();
    let types: std::collections::BTreeMap<&str, &str> = graph
        .files
        .iter()
        .flat_map(|file| &file.declarations)
        .filter(|d| matches!(d.item_kind(), Some(ItemKind::Struct | ItemKind::Enum)))
        .map(|d| (d.id.as_str(), d.name.as_str()))
        .collect();
    let mut mentions = std::collections::BTreeMap::<&str, std::collections::BTreeSet<&str>>::new();
    for relation in &graph.relationships {
        if let Resolution::Unique(id) = &relation.target
            && let Some(name) = types.get(id.as_str())
        {
            mentions.entry(name).or_default().insert(&relation.source);
        }
    }
    mentions
        .into_iter()
        .max_by(|(a, x), (b, y)| x.len().cmp(&y.len()).then_with(|| b.cmp(a)))
        .map(|(name, _)| name.to_owned())
}
