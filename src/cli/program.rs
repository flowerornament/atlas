//! Source call-boundary view: capture, select, project and publish without changing evidence.
use super::{
    json,
    output::{self, Backend, RenderOptions, protected_inputs, render},
    profile::architecture,
};
use atlas::{
    Result,
    diagram::{Level, Query, View},
    discovery, program,
    render::{Artifact, Document},
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
    #[arg(long)]
    architecture: PathBuf,
    #[arg(long, requires = "function")]
    path: Option<String>,
    #[arg(long, requires = "path")]
    function: Option<String>,
    #[arg(long)]
    json: bool,
    #[command(flatten)]
    render: RenderOptions,
}

pub(super) fn run(
    root: &Path,
    metadata: &cargo_metadata::Metadata,
    options: &Options,
) -> Result<()> {
    let (text, architecture) = architecture(root, Some(&options.architecture))?;
    let architecture = architecture.ok_or("program requires architecture boundaries")?;
    let snapshot = discovery::snapshot_metadata(metadata)?;
    let graph = snapshot.graph();
    let analysis = atlas_core::analysis::Analysis::new(&snapshot)?;
    let entry = options.path.as_deref().zip(options.function.as_deref());
    let program = program::from_analysis(&analysis, &architecture, entry)?;
    let overview = program.overview();
    if options.json {
        print!(
            "{}",
            json(&serde_json::json!({"program": &program, "overview": &overview}))?
        );
        return Ok(());
    }
    let mut diagram = program.diagram(&overview)?;
    let query = Query {
        focus: Some("program".into()),
        level: Level::Stages,
        view: View::Program,
    };
    let destination = output::Destination::new(
        root,
        &options.render,
        &query,
        protected_inputs(metadata, graph, [root.join(&options.architecture)]),
    );
    let pending = destination.prepare(
        Some(graph),
        &mut diagram,
        text.as_deref(),
        Some((
            "program",
            &json(&serde_json::json!({"program": &program, "overview": &overview}))?,
        )),
    )?;
    let artifact = if options.render.backend == Backend::Ascii {
        let mut text = program.ascii(&overview);
        for (label, url) in &diagram.evidence_links {
            write!(text, "\n{label}: {url}").expect("formatting into a String cannot fail");
        }
        Ok(Artifact {
            primary: Document {
                suffix: "txt",
                bytes: text.into_bytes(),
            },
            companions: vec![],
        })
    } else {
        render(&options.render, &diagram)
    };
    destination.write(artifact, pending)
}
