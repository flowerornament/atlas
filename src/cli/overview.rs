//! Read source declarations directly; no compiled project exporter or maintained profile.
use crate::cli::output::artifact as artifact_output;
use atlas::{
    Result,
    adapter::{self, Request, Snapshot},
    discovery, overview,
    render::View,
};
use clap::Args;
use std::path::{Path, PathBuf};
#[derive(Args)]
pub(crate) struct Options {
    /// Compatibility positional project root; prefer --root.
    pub root: Option<PathBuf>,
    /// Normalized declaration source path supplied to the adapter.
    #[arg(long, default_value = "src/lib.rs")]
    declarations: String,
    /// Explicit declaration adapter executable. Defaults to the co-installed Girard adapter.
    #[arg(long)]
    adapter: Option<PathBuf>,
    #[command(flatten)]
    output: artifact_output::Options,
}
pub(crate) fn run(
    root: &Path,
    metadata: &cargo_metadata::Metadata,
    options: Options,
) -> Result<()> {
    let snapshot = discovery::snapshot_metadata(metadata)?;
    let graph = snapshot.graph();
    let facts = atlas_core::analysis::Analysis::new(&snapshot)?.representations()?;
    let input = Request::new(
        Snapshot::from_facts(graph, &facts, &options.declarations)?,
        options.declarations,
    )?;
    let program = options.adapter.unwrap_or(
        std::env::current_exe()?
            .with_file_name(format!("atlas-girard{}", std::env::consts::EXE_SUFFIX)),
    );
    let program = program.canonicalize()?;
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
        || {
            options
                .output
                .backend
                .render_view(View::Overview(&overview))
        },
    )
}
