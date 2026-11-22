//! Ask an explicit declaration adapter to interpret source constants, then project them.
use crate::cli::output::artifact as artifact_output;
use cargo_atlas::{
    Result,
    adapter::{self, Request, Snapshot},
    discovery, overview,
    render::View,
};
use clap::Args;
use std::path::{Path, PathBuf};
#[derive(Args)]
pub(crate) struct Options {
    #[command(flatten)]
    pub(super) words: crate::cli::subject::Root,
    /// The declaration source file supplied to the adapter; read from the
    /// current directory, and src/lib.rs of the workspace root by default.
    #[arg(long)]
    declarations: Option<String>,
    /// Declaration adapter executable that interprets the project's declarations.
    #[arg(long)]
    adapter: PathBuf,
    #[command(flatten)]
    output: artifact_output::Options,
}
pub(crate) fn run(
    root: &Path,
    metadata: &cargo_metadata::Metadata,
    options: &Options,
) -> Result<()> {
    let snapshot = discovery::snapshot_metadata(metadata)?;
    let graph = snapshot.graph();
    let facts = atlas_lib::analysis::Analysis::new(&snapshot)?.representations()?;
    let declarations = match &options.declarations {
        Some(path) => super::typed::key(path, root)?,
        None => "src/lib.rs".to_owned(),
    };
    let input = Request::new(
        Snapshot::from_facts(graph, &facts, &declarations)?,
        declarations,
    )?;
    let program = options.adapter.canonicalize()?;
    let invocation = adapter::invoke(&program, root, &input)?;
    if !invocation.diagnostics.is_empty() {
        eprint!("{}", invocation.diagnostics);
    }
    let reply = &invocation.reply;
    let architecture = reply.architecture()?.clone().bind(root)?;
    let overview = overview::project(&facts, &architecture)?;
    options.output.emit(
        root,
        "overview",
        &serde_json::json!({
            "overview": overview,
            "facts": facts,
            "adapter": reply,
            "adapter_diagnostics": invocation.diagnostics,
            "snapshot": input.snapshot,
        }),
        &crate::cli::output::protected_inputs(metadata, graph, [program]),
        || options.output.render_view(View::Overview(&overview)),
    )
}
