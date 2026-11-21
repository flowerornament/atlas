//! Source-body control and bounded expansion; raw workflow evidence stays distinct from presentation.
use super::{
    json,
    output::{self, Backend, RenderOptions, protected_paths, render},
    profile::architecture,
};
use cargo_atlas::{
    Result,
    architecture::{Architecture, WorkflowAnchor},
    diagram::{Diagram, Level, Query, View},
    discovery,
    render::{Artifact, Ascii, Document, Renderer},
    workflow,
};
use clap::Args;
use std::{
    fmt::Write as _,
    path::{Path, PathBuf},
};
#[derive(Args)]
pub(super) struct Options {
    #[command(flatten)]
    pub(super) words: super::subject::Root,
    /// Expand matched source calls; recursion/unknown bodies remain explicit (max 6).
    #[arg(long,default_value_t=0,value_parser=clap::value_parser!(u8).range(0..=i64::from(workflow::MAX_DEPTH)),requires="path")]
    depth: u8,
    /// Fold borrow operands and generated ? bridges; raw steps remain in evidence.
    #[arg(long, value_enum, default_value = "expressions")]
    detail: workflow::Detail,
    /// A workflow the --architecture profile declares; omit to draw them all.
    #[arg(long = "anchor")]
    pub(super) named_anchor: Option<String>,
    /// An architecture profile (JSON) whose declared workflows to draw,
    /// instead of one function named with --path and --function.
    #[arg(long)]
    pub(super) architecture: Option<PathBuf>,
    /// The file that holds the function to draw, relative to the project
    /// root, such as `src/main.rs`; needs --function.
    #[arg(long, requires = "function")]
    path: Option<String>,
    /// The function in --path: its name, a qualified name (module::name,
    /// Type::method, Trait::method for a default body, <Type as Trait>::method),
    /// or the LINE (or LINE:COLUMN) where its name is written.
    #[arg(long, requires = "path")]
    function: Option<String>,
    /// Write the selected bodies' calls, read against this SCIP index (for
    /// example from `rust-analyzer scip`), as a separate semantic evidence
    /// file. A relative path is resolved against the workspace root. Atlas
    /// runs no indexer.
    #[arg(long, value_name = "FILE")]
    scip: Option<PathBuf>,
    #[command(flatten)]
    indexer: super::semantic::IndexerArgs,
    #[command(flatten)]
    evidence: super::evidence::Evidence,
    #[command(flatten)]
    render: RenderOptions,
}

pub(super) fn run(
    root: &Path,
    metadata: &cargo_metadata::Metadata,
    options: &Options,
    cache: &super::semantic::IndexCache,
) -> Result<()> {
    options.evidence.check(
        "flow",
        "source bodies and resolved calls",
        "it has no lexical reading",
        "flow --scip FILE writes one beside the drawing as separate evidence",
    )?;
    let (profile_text, profile) = architecture(root, options.architecture.as_deref())?;
    let selected_anchor = options
        .named_anchor
        .as_deref()
        .or(options.words.subject())
        .map(str::to_owned);
    let selected_anchor = selected_anchor.as_ref();
    let anchors = anchors(metadata, options, profile.as_ref(), selected_anchor)?;
    let (flows, mut diagram, sources) = read_flows(root, metadata, options, &anchors)?;
    let query = Query {
        focus: selected_anchor
            .cloned()
            .or(options.function.clone())
            .or(Some("workflows".into())),
        level: Level::Declarations,
        view: View::Workflow,
    };
    let destination = output::Destination::new(
        root,
        &options.render,
        &query,
        protected_paths(
            metadata,
            sources.iter().map(|(path, _)| path.clone()),
            options
                .architecture
                .iter()
                .map(|path| root.join(path))
                .chain(anchors.iter().map(|anchor| root.join(&anchor.path)))
                .chain(options.scip.iter().map(|path| root.join(path))),
        ),
    );
    let semantic = semantic_evidence(root, metadata, options, cache, sources, &flows)?;
    let pending = destination.prepare_with(
        None,
        &mut diagram,
        profile_text.as_deref(),
        Some(("workflow", &json(&flows)?)),
        semantic
            .as_ref()
            .map(|(source, json)| ("semantic", *source, json.as_str())),
    )?;
    let artifact = artifact(options, &flows, &diagram);
    destination.write(artifact, pending)
}

/// The bodies to draw: the one named by --path and --function, or the
/// profile's workflows (only the selected one, when a name is given).
fn anchors(
    metadata: &cargo_metadata::Metadata,
    options: &Options,
    profile: Option<&Architecture>,
    selected_anchor: Option<&String>,
) -> Result<Vec<WorkflowAnchor>> {
    if let (Some(path), Some(function)) = (&options.path, &options.function) {
        if selected_anchor.is_some() {
            return Err("choose a named workflow or --path/--function, not both".into());
        }
        return Ok(vec![WorkflowAnchor {
            name: function.clone(),
            path: path.clone(),
            function: function.clone(),
        }]);
    }
    let Some(profile) = profile else {
        let example = entry_function(metadata).map_or_else(String::new, |(path, function)| {
            format!("; run: cargo atlas flow --path {path} --function {function}")
        });
        return Err(
            format!("flow needs --architecture or both --path and --function{example}").into(),
        );
    };
    let anchors: Vec<_> = profile
        .workflows
        .iter()
        .filter(|a| selected_anchor.is_none_or(|name| name == &a.name))
        .cloned()
        .collect();
    if anchors.is_empty() {
        return Err("no matching workflow anchors; inspect the architecture profile".into());
    }
    Ok(anchors)
}

/// Each read body under the anchor name it was selected by.
type NamedFlows = Vec<(String, workflow::Workflow)>;

/// The named workflows, their diagram and the source paths to protect.
/// Depth zero reads only the selected bodies; its inputs are protected
/// from a path walk rather than a capture of the whole workspace.
fn read_flows(
    root: &Path,
    metadata: &cargo_metadata::Metadata,
    options: &Options,
    anchors: &[WorkflowAnchor],
) -> Result<(NamedFlows, Diagram, output::Captured)> {
    if options.depth > 0 {
        let path = options
            .path
            .as_deref()
            .ok_or("--depth needs --path and --function")?;
        let function = options
            .function
            .as_deref()
            .ok_or("--depth needs --function")?;
        let snapshot = discovery::snapshot_metadata(metadata)?;
        let sources: Vec<_> = super::output::graph_captures(metadata, snapshot.graph()).collect();
        let analysis = atlas_lib::analysis::Analysis::new(&snapshot)?;
        let (flow, diagram) =
            analysis.expanded_with_detail(path, function, options.depth, options.detail)?;
        return Ok((vec![(function.to_owned(), flow)], diagram, sources));
    }
    let sources = discovery::source_paths(metadata)?;
    let selections: Vec<_> = anchors
        .iter()
        .map(|a| (a.path.as_str(), a.function.as_str()))
        .collect();
    let flows: Vec<_> = anchors
        .iter()
        .zip(workflow::read_many(root, &selections)?)
        .map(|(a, f)| (a.name.clone(), f))
        .collect();
    let diagram = workflow_diagram(&flows, options.detail)?;
    // The anchors read their files directly: what they read is what is on
    // disk now, fingerprinted for the indexer to hold the tree to.
    let sources = sources
        .into_iter()
        .map(|path| {
            let fingerprint = std::fs::read(&path)
                .map(|bytes| discovery::fingerprint(&bytes))
                .unwrap_or_default();
            (path, fingerprint)
        })
        .collect();
    Ok((flows, diagram, sources))
}

/// The selected bodies' calls read against the supplied or produced SCIP
/// index, with the label that says which; `None` when no index was asked for.
fn semantic_evidence(
    root: &Path,
    metadata: &cargo_metadata::Metadata,
    options: &Options,
    cache: &super::semantic::IndexCache,
    sources: output::Captured,
    flows: &[(String, workflow::Workflow)],
) -> Result<Option<(&'static str, String)>> {
    super::semantic::obtain(
        root,
        metadata,
        options.scip.as_deref(),
        &options.indexer,
        cache,
        || sources,
    )?
    .map(|(index, indexing)| {
        let source = if indexing.is_some() {
            "semantic call evidence (rust-analyzer run by Atlas, host cfg)"
        } else {
            "semantic call evidence (supplied SCIP index, host cfg)"
        };
        json(&super::semantic::Evidence {
            reading: &super::semantic::read_workflows(root, &index, flows),
            indexing: indexing.as_ref(),
        })
        .map(|text| (source, text))
    })
    .transpose()
}

/// ASCII draws an expanded or control-detail diagram as a diagram, and
/// otherwise prints each body's own text with the evidence links; any other
/// backend renders the diagram.
fn artifact(
    options: &Options,
    flows: &[(String, workflow::Workflow)],
    diagram: &Diagram,
) -> Result<Artifact> {
    if options.render.backend() == Backend::Ascii
        && (options.depth > 0 || options.detail == workflow::Detail::Control)
    {
        Ascii.render(diagram)
    } else if options.render.backend() == Backend::Ascii {
        let mut text = flows
            .iter()
            // A body's own heading names it; an anchor's name is shown only
            // when it differs, as for a profile workflow.
            .map(|(name, flow)| {
                if *name == flow.function {
                    flow.ascii()
                } else {
                    format!("{name}\n{}", flow.ascii())
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        for (label, url) in &diagram.evidence_links {
            write!(text, "\n{label}: {url}").expect("formatting into a String cannot fail");
        }
        text.push('\n');
        Ok(Artifact {
            primary: Document {
                suffix: "txt",
                bytes: text.into_bytes(),
            },
            companions: Vec::new(),
        })
    } else {
        render(&options.render, diagram)
    }
}

fn workflow_diagram(
    flows: &[(String, workflow::Workflow)],
    detail: workflow::Detail,
) -> Result<Diagram> {
    let mut combined = Diagram {
        title: "Source workflows (independent bodies)".into(),
        level: Level::Declarations,
        view: View::Workflow,
        nodes: vec![],
        edges: vec![],
        inventory: vec![],
        coverage: cargo_atlas::diagram::Coverage::default(),
        notes: vec![],
        evidence_links: vec![],
    };
    for (index, (name, flow)) in flows.iter().enumerate() {
        let mut diagram = flow.diagram_with_detail(detail)?;
        for node in &mut diagram.nodes {
            node.id = format!("w{index}_{}", node.id);
            node.group = format!("{} · {}", node.group, name);
        }
        for edge in &mut diagram.edges {
            edge.from = format!("w{index}_{}", edge.from);
            edge.to = format!("w{index}_{}", edge.to);
        }
        for node in &mut diagram.coverage.summarized_nodes {
            node.id = format!("w{index}_{}", node.id);
            node.group = format!("{} · {}", node.group, name);
        }
        combined
            .coverage
            .summarized_nodes
            .extend(diagram.coverage.summarized_nodes);
        combined
            .coverage
            .represented_files
            .extend(diagram.coverage.represented_files);
        combined.nodes.extend(diagram.nodes);
        combined.edges.extend(diagram.edges);
        combined.inventory.extend(diagram.inventory);
        combined.notes.extend(diagram.notes);
    }
    combined.notes.sort();
    combined.notes.dedup();
    combined.inventory.sort();
    combined.inventory.dedup();
    combined.validate()?;
    Ok(combined)
}

/// A function to suggest: a binary's `main`, else the first function written
/// in a library root. Names come from Cargo targets and captured source.
fn entry_function(metadata: &cargo_metadata::Metadata) -> Option<(String, String)> {
    let root = metadata.workspace_root.as_std_path();
    let relative = |target: &cargo_metadata::Target| {
        target
            .src_path
            .as_std_path()
            .strip_prefix(root)
            .ok()
            .map(cargo_atlas::discovery::path_key)
    };
    let targets: Vec<_> = metadata
        .workspace_packages()
        .into_iter()
        .flat_map(|package| &package.targets)
        .collect();
    if let Some(path) = targets
        .iter()
        .filter(|target| target.is_bin())
        .filter_map(|target| relative(target))
        .min()
    {
        return Some((path, "main".to_owned()));
    }
    let lib = targets
        .iter()
        .filter(|target| target.is_lib())
        .filter_map(|target| relative(target))
        .min()?;
    let snapshot = discovery::snapshot_metadata(metadata).ok()?;
    let function = snapshot
        .graph()
        .files
        .iter()
        .filter(|file| file.path == lib)
        .flat_map(|file| &file.declarations)
        .filter(|d| !d.test_only && d.item_kind() == Some(atlas_lib::source::ItemKind::Fn))
        .min_by_key(|d| d.line)?
        .name
        .clone();
    Some((lib, function))
}
