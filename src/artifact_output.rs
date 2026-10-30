//! Explicit artifacts and evidence share one collision check before any writes.
use crate::{Backend, output_files::publish};
use atlas::{Result, render::Artifact};
use clap::Args;
use std::{
    io::Write,
    path::{Path, PathBuf},
};
#[derive(Args)]
#[group(id = "artifact_output")]
pub(crate) struct Options {
    #[arg(long, value_enum, default_value = "ascii")]
    pub backend: Backend,
    /// Output path; Pele defaults to target/atlas/NAME.svg. ASCII defaults to stdout.
    #[arg(long)]
    pub out: Option<PathBuf>,
    #[arg(long)]
    pub json: bool,
}
impl Options {
    pub fn emit(
        &self,
        root: &Path,
        name: &str,
        evidence: &impl serde::Serialize,
        protected: &[PathBuf],
        render: impl FnOnce() -> Result<Artifact>,
    ) -> Result<()> {
        let evidence = serde_json::to_vec_pretty(evidence)?;
        if self.json {
            if self.out.is_some() {
                return Err("--json writes stdout; omit --out".into());
            }
            std::io::stdout().lock().write_all(&evidence)?;
            println!();
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
