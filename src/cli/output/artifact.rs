//! Explicit artifacts and evidence share one collision check before any writes.
use super::{Backend, files::publish};
use cargo_atlas::{
    Result,
    render::{Artifact, Theme, View},
};
use clap::Args;
use std::{
    io::Write,
    path::{Path, PathBuf},
};
#[derive(Args)]
#[group(id = "artifact_output")]
pub(crate) struct Options {
    /// `ascii` draws in the terminal; `pele` draws an SVG image (needs
    /// Node.js 20 or newer).
    #[arg(long, value_enum, default_value = "ascii")]
    pub backend: Backend,
    /// Colours for Pele images.
    #[arg(long, value_enum, default_value_t)]
    pub theme: Theme,
    /// Draw only the diagram in Pele images, without the title, notes and
    /// panel headings around it. The JSON evidence keeps all of them.
    /// A `dependencies` drawing has nothing around it, so it is always bare.
    #[arg(long)]
    pub bare: bool,
    /// Output path; Pele defaults to target/atlas/NAME.svg. ASCII defaults to stdout.
    #[arg(long)]
    pub out: Option<PathBuf>,
    /// Print the same answer as JSON, for scripts and agents.
    #[arg(long)]
    pub json: bool,
}
impl Options {
    pub fn render_view(&self, view: View<'_>) -> Result<Artifact> {
        self.backend.render_view(view, self.theme, self.bare)
    }
    pub fn emit(
        &self,
        root: &Path,
        name: &str,
        evidence: &impl serde::Serialize,
        protected: &[PathBuf],
        render: impl FnOnce() -> Result<Artifact>,
    ) -> Result<()> {
        if self.json {
            if self.out.is_some() {
                return Err("--json writes stdout; omit --out".into());
            }
            // Stream: a large evidence document is never held twice.
            let mut stdout = std::io::BufWriter::new(std::io::stdout().lock());
            serde_json::to_writer_pretty(&mut stdout, evidence)?;
            writeln!(stdout)?;
            stdout.flush()?;
            return Ok(());
        }
        let artifact = render()?;
        let output = self.out.as_ref().map(|p| root.join(p)).or_else(|| {
            matches!(self.backend, Backend::Pele)
                .then(|| root.join("target/atlas").join(format!("{name}.svg")))
        });
        let Some(output) = output else {
            std::io::stdout()
                .lock()
                .write_all(&artifact.primary.bytes)?;
            return Ok(());
        };
        if output.extension().and_then(|s| s.to_str()) != Some(artifact.primary.suffix) {
            return Err(format!("output extension must be .{}", artifact.primary.suffix).into());
        }
        let evidence = serde_json::to_vec_pretty(evidence)?;
        let mut files = vec![
            (output.clone(), artifact.primary.bytes),
            (output.with_extension("json"), evidence),
        ];
        files.extend(
            artifact
                .companions
                .into_iter()
                .map(|d| (output.with_extension(d.suffix), d.bytes)),
        );
        publish(files, protected)?;
        eprintln!("{}", output.display());
        Ok(())
    }
}
