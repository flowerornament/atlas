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
    #[command(flatten)]
    pub(super) words: crate::cli::subject::Types,
    /// Saved selection: title, types, optional apis and preparation_files; no edges.
    #[arg(long, conflicts_with = "types")]
    pub(super) query: Option<PathBuf>,
    /// Superseded by the bare TYPES; still accepted (repeat or comma separate).
    #[arg(long = "type", value_delimiter = ',')]
    pub(super) types: Vec<String>,
    /// Keep the types declared under this path or unique file or directory
    /// name, to choose between types that share a name.
    #[arg(long = "in", value_name = "PATH", conflicts_with = "query")]
    in_path: Option<String>,
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
    let names = if options.types.is_empty() {
        options.words.names()
    } else {
        options.types.clone()
    };
    if query_path.is_none() && names.is_empty() {
        let example = most_mentioned_type(metadata).map_or_else(String::new, |name| {
            format!(
                "; run: {}",
                cargo_atlas::suggestion::Command::new("types").arg(name)
            )
        });
        return Err(format!(
            "name the types ({}) or give --query FILE; use overview for source-declared defaults{example}",
            cargo_atlas::suggestion::Command::new("types").placeholder("NAME[,NAME]")
        )
        .into());
    }
    let snapshot = discovery::snapshot_metadata(metadata)?;
    let graph = snapshot.graph();
    let facts = atlas_lib::analysis::Analysis::new(&snapshot)?.representations()?;
    let query: Query = if let Some(p) = &query_path {
        serde_json::from_str(&fs::read_to_string(p)?)?
    } else {
        let within = options
            .in_path
            .as_deref()
            .map(|path| discovery::resolve_scope(graph, Some(path)))
            .transpose()?;
        Query {
            title: format!("Types: {}", names.join(", ")),
            types: names
                .iter()
                .map(|word| selector(graph, &facts, word, within.as_deref()))
                .collect::<Result<_>>()?,
            apis: vec![],
            preparation_files: vec![],
            preparation_functions: vec![],
        }
    };
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

/// The one type `word` names: `Name`, `module::Name` (the end of its module
/// path, as `name` reads it) or `FILE:Name`, kept to `within` when given.
/// Several matches are refused with each one's `FILE:Name` spelling, which
/// this command reads back.
fn selector(
    graph: &atlas_lib::model::SourceGraph,
    facts: &atlas_lib::representations::Facts,
    word: &str,
    within: Option<&str>,
) -> Result<types::Selector> {
    let inside =
        |file: &str| within.is_none_or(|path| atlas_lib::architecture::contains(path, file));
    let (name, candidates): (&str, Vec<&atlas_lib::representations::Type>) =
        if let Some((file, name)) = word.rsplit_once(':').filter(|(file, _)| {
            std::path::Path::new(file)
                .extension()
                .is_some_and(|e| e == "rs")
        }) {
            let file = file.trim_end_matches(':');
            (
                name,
                facts
                    .types
                    .iter()
                    .filter(|t| t.file == file && t.name == name)
                    .collect(),
            )
        } else if let Some((_, name)) = word.rsplit_once("::") {
            // The lexical declarations `name` selects, read back as types.
            let declared: std::collections::BTreeSet<(&str, usize)> =
                cargo_atlas::queries::name(graph, word)
                    .map(|names| {
                        names
                            .matches
                            .iter()
                            .map(|card| (card.file, card.declaration.line))
                            .collect()
                    })
                    .unwrap_or_default();
            (
                name,
                facts
                    .types
                    .iter()
                    .filter(|t| t.name == name && declared.contains(&(t.file.as_str(), t.line)))
                    .collect(),
            )
        } else {
            (
                word,
                facts.types.iter().filter(|t| t.name == word).collect(),
            )
        };
    let candidates: Vec<_> = candidates.into_iter().filter(|t| inside(&t.file)).collect();
    match candidates.as_slice() {
        [one] => Ok(types::Selector {
            name: one.name.clone(),
            file: Some(one.file.clone()),
        }),
        [] => Err(format!(
            "no type matches {word:?}{}; {} lists what is declared with that name",
            within.map_or_else(String::new, |path| format!(" in {path}")),
            cargo_atlas::suggestion::Command::new("name").arg(name)
        )
        .into()),
        several => {
            let spellings: Vec<String> = several
                .iter()
                .map(|t| respelled(word, &format!("{}:{}", t.file, t.name)))
                .collect();
            Err(format!(
                "{word:?} names {} types; choose one: {}",
                several.len(),
                spellings.join(", ")
            )
            .into())
        }
    }
}
/// This run's command line with the first comma-separated element equal to
/// `word` replaced by `with`: the other types and every flag stay as given.
fn respelled(word: &str, with: &str) -> String {
    let mut words = crate::cli::output::rows::this_run();
    for arg in &mut words {
        let mut parts: Vec<&str> = arg.split(',').collect();
        if let Some(part) = parts.iter_mut().find(|part| **part == word) {
            *part = with;
            *arg = parts.join(",");
            return crate::cli::output::rows::rerun(&words);
        }
    }
    cargo_atlas::suggestion::Command::new("types")
        .arg(with)
        .to_string()
}
