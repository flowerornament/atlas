//! Source-body control and bounded expansion; raw workflow evidence stays distinct from presentation.
use super::{
    json,
    output::{self, Backend, RenderOptions, protected_inputs, render},
    profile::architecture,
};
use atlas::{
    Result,
    architecture::WorkflowAnchor,
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
    /// Compatibility positional project root; prefer --root.
    pub(super) root: Option<PathBuf>,
    /// Expand matched source calls; recursion/unknown bodies remain explicit (max 6).
    #[arg(long,default_value_t=0,value_parser=clap::value_parser!(u8).range(0..=i64::from(workflow::MAX_DEPTH)),requires="path")]
    depth: u8,
    /// Fold borrow operands and generated ? bridges; raw steps remain in evidence.
    #[arg(long, value_enum, default_value = "expressions")]
    detail: workflow::Detail,
    /// Profile workflow name. Omit to show independent source bodies together.
    anchor: Option<String>,
    /// Select a declared workflow without a positional ROOT.
    #[arg(long = "anchor", conflicts_with = "anchor")]
    named_anchor: Option<String>,
    #[arg(long)]
    architecture: Option<PathBuf>,
    #[arg(long, requires = "function")]
    path: Option<String>,
    #[arg(long, requires = "path")]
    function: Option<String>,
    #[command(flatten)]
    render: RenderOptions,
}

pub(super) fn run(
    root: &Path,
    metadata: &cargo_metadata::Metadata,
    options: &Options,
) -> Result<()> {
    let (profile_text, profile) = architecture(root, options.architecture.as_deref())?;
    let selected_anchor = options.named_anchor.as_ref().or(options.anchor.as_ref());
    let anchors = if let (Some(path), Some(function)) = (&options.path, &options.function) {
        if selected_anchor.is_some() {
            return Err("choose a named workflow or --path/--function, not both".into());
        }
        vec![WorkflowAnchor {
            name: function.clone(),
            path: path.clone(),
            function: function.clone(),
        }]
    } else {
        let profile = profile
            .as_ref()
            .ok_or("flow needs --architecture or both --path and --function")?;
        let anchors: Vec<_> = profile
            .workflows
            .iter()
            .filter(|a| selected_anchor.is_none_or(|name| name == &a.name))
            .cloned()
            .collect();
        if anchors.is_empty() {
            return Err("no matching workflow anchors; inspect the architecture profile".into());
        }
        anchors
    };
    let snapshot = discovery::snapshot_metadata(metadata)?;
    let graph = snapshot.graph();
    let (flows, mut diagram) = if options.depth > 0 {
        let path = options
            .path
            .as_deref()
            .ok_or("--depth needs --path and --function")?;
        let function = options
            .function
            .as_deref()
            .ok_or("--depth needs --function")?;
        let analysis = atlas_core::analysis::Analysis::new(&snapshot)?;
        let (flow, diagram) =
            analysis.expanded_with_detail(path, function, options.depth, options.detail)?;
        (vec![(function.to_owned(), flow)], diagram)
    } else {
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
        (flows, diagram)
    };
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
        protected_inputs(
            metadata,
            graph,
            options
                .architecture
                .iter()
                .map(|path| root.join(path))
                .chain(anchors.iter().map(|anchor| root.join(&anchor.path))),
        ),
    );
    let pending = destination.prepare(
        None,
        &mut diagram,
        profile_text.as_deref(),
        Some(("workflow", &json(&flows)?)),
    )?;
    let artifact = if options.render.backend == Backend::Ascii
        && (options.depth > 0 || options.detail == workflow::Detail::Control)
    {
        Ascii.render(&diagram)
    } else if options.render.backend == Backend::Ascii {
        let mut text = flows
            .iter()
            .map(|(name, flow)| format!("{name}\n{}", flow.ascii()))
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
        render(&options.render, &diagram)
    };
    destination.write(artifact, pending)
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
        coverage: atlas::diagram::Coverage::default(),
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
