//! Keep source payloads in ignored evidence, publish only bounded diagram artifacts.
pub(super) mod artifact;
mod files;

use self::files::{absolute, preflight, publish};
use atlas::{
    Result,
    diagram::{Diagram, Query},
    model::SourceGraph,
    render::{Artifact, Ascii, Pele, Renderer, mermaid},
};
use clap::{Args, ValueEnum};
use std::{
    io::Write,
    path::{Path, PathBuf},
};

pub(crate) struct Pending {
    files: Vec<(PathBuf, Vec<u8>)>,
    protected: Vec<PathBuf>,
    failure_diagram: Option<PathBuf>,
}

pub(crate) struct Destination {
    pub path: Option<PathBuf>,
    evidence: PathBuf,
    base: String,
    reference: PathBuf,
    protected: Vec<PathBuf>,
    pele: bool,
    svg_companion: bool,
}
impl Destination {
    pub fn new(
        root: &Path,
        options: &RenderOptions,
        query: &Query,
        protected: Vec<PathBuf>,
    ) -> Self {
        let focus = query.focus.as_deref().unwrap_or("overview");
        let name = Path::new(focus)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("overview");
        let slug: String = name
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '-'
                }
            })
            .collect();
        let base = format!(
            "{}-{}-{}",
            if slug.is_empty() { "overview" } else { &slug },
            query.level.label(),
            query.view.label()
        );
        let path = options.out.as_ref().map(|p| root.join(p)).or_else(|| {
            (options.backend == Backend::Pele).then(|| {
                root.join("target/atlas").join(format!(
                    "{base}.{}",
                    if options.format == Format::Html {
                        "html"
                    } else {
                        "svg"
                    }
                ))
            })
        });
        let base = path
            .as_ref()
            .and_then(|p| p.file_stem())
            .and_then(|s| s.to_str())
            .unwrap_or(&base)
            .to_owned();
        let evidence = options
            .evidence_dir
            .as_ref()
            .map_or_else(|| root.join("target/atlas"), |p| root.join(p));
        let reference = path
            .as_ref()
            .and_then(|p| p.parent())
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(root)
            .to_path_buf();
        Self {
            path,
            evidence,
            base,
            reference,
            protected,
            pele: options.backend == Backend::Pele,
            svg_companion: options.backend == Backend::Pele && options.format == Format::Html,
        }
    }
    pub fn prepare(
        &self,
        graph: Option<&SourceGraph>,
        diagram: &mut Diagram,
        profile: Option<&str>,
        details: Option<(&str, &str)>,
    ) -> Result<Pending> {
        let mut files = Vec::new();
        if let Some(graph) = graph {
            let path = self.evidence.join(format!("{}.source.json", self.base));
            diagram
                .evidence_links
                .push(("source graph".into(), relative_url(&self.reference, &path)?));
            files.push((path, serde_json::to_vec_pretty(graph)?));
        }
        if let Some((kind, details)) = details {
            let path = self.evidence.join(format!("{}.{kind}.json", self.base));
            diagram.evidence_links.push((
                format!("source {kind} evidence"),
                relative_url(&self.reference, &path)?,
            ));
            files.push((path, details.as_bytes().to_vec()));
        }
        let diagram_path = self.evidence.join(format!("{}.diagram.json", self.base));
        diagram.evidence_links.push((
            "diagram evidence".into(),
            relative_url(&self.reference, &diagram_path)?,
        ));
        if let Some(text) = profile {
            let path = self
                .evidence
                .join(format!("{}.architecture.json", self.base));
            diagram.evidence_links.push((
                "architecture profile".into(),
                relative_url(&self.reference, &path)?,
            ));
            files.push((path, text.as_bytes().to_vec()));
        }
        let failure_diagram = self.pele.then(|| diagram_path.clone());
        files.push((diagram_path, serde_json::to_vec_pretty(diagram)?));
        let protected = self.protected.clone();
        Ok(Pending {
            files,
            protected,
            failure_diagram,
        })
    }
    pub fn write(&self, artifact: Result<Artifact>, mut pending: Pending) -> Result<()> {
        let artifact = match artifact {
            Ok(artifact) => artifact,
            Err(error) => {
                // Keep the existing renderer-failure evidence contract, after the
                // same source and destination preflight as successful artifacts.
                if let Some(diagram_path) = pending.failure_diagram.take() {
                    let bytes = &pending
                        .files
                        .iter()
                        .find(|(path, _)| *path == diagram_path)
                        .ok_or("diagram evidence missing")?
                        .1;
                    let diagram: Diagram = serde_json::from_slice(bytes)?;
                    let path = self
                        .path
                        .as_ref()
                        .ok_or("Pele output destination missing")?;
                    pending
                        .files
                        .push((path.with_extension("mmd"), mermaid(&diagram)?.into_bytes()));
                }
                let mut paths: Vec<_> =
                    pending.files.iter().map(|(path, _)| path.clone()).collect();
                if let Some(path) = &self.path {
                    paths.push(path.clone());
                    if self.svg_companion {
                        paths.push(path.with_extension("svg"));
                    }
                }
                preflight(&paths, &pending.protected)?;
                publish(pending.files, &pending.protected)?;
                return Err(error);
            }
        };
        if let Some(path) = &self.path {
            pending.files.extend(
                artifact
                    .companions
                    .into_iter()
                    .map(|document| (path.with_extension(document.suffix), document.bytes)),
            );
            pending.files.push((path.clone(), artifact.primary.bytes));
            publish(pending.files, &pending.protected)?;
        } else {
            publish(pending.files, &pending.protected)?;
            std::io::stdout()
                .lock()
                .write_all(&artifact.primary.bytes)?;
        }
        Ok(())
    }
}
fn relative_url(from: &Path, to: &Path) -> Result<String> {
    let from = absolute(from)?;
    let to = absolute(to)?;
    let a: Vec<_> = from.components().collect();
    let b: Vec<_> = to.components().collect();
    let common = a.iter().zip(&b).take_while(|(a, b)| a == b).count();
    let mut result = vec!["..".to_owned(); a.len() - common];
    result.extend(
        b[common..]
            .iter()
            .map(|part| part.as_os_str().to_string_lossy().to_string()),
    );
    let path = result.join("/");
    let mut encoded = String::new();
    for byte in path.bytes() {
        if byte.is_ascii_alphanumeric() || b"/-_.~".contains(&byte) {
            encoded.push(char::from(byte));
        } else {
            use std::fmt::Write;
            write!(encoded, "%{byte:02X}")?;
        }
    }
    Ok(encoded)
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(super) enum Backend {
    Ascii,
    Pele,
}
impl Backend {
    pub(crate) fn render_view(self, view: atlas::render::View<'_>) -> Result<Artifact> {
        match self {
            Self::Ascii => Ascii.render_view(view),
            Self::Pele => Pele { html: false }.render_view(view),
        }
    }
}
#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(super) enum Format {
    Html,
    Svg,
}
#[derive(Args)]
pub(super) struct RenderOptions {
    #[arg(long, value_enum, default_value = "ascii")]
    pub(super) backend: Backend,
    #[arg(long, value_enum, default_value = "html")]
    pub(super) format: Format,
    /// Relative output paths are resolved against the Cargo workspace root.
    #[arg(long)]
    pub(super) out: Option<PathBuf>,
    #[arg(long)]
    pub(super) evidence_dir: Option<PathBuf>,
}

pub(super) fn render(options: &RenderOptions, diagram: &Diagram) -> Result<Artifact> {
    match options.backend {
        Backend::Ascii => Ascii.render(diagram),
        Backend::Pele => Pele {
            html: options.format == Format::Html,
        }
        .render(diagram),
    }
}
pub(super) fn protected_inputs(
    metadata: &cargo_metadata::Metadata,
    graph: &atlas::model::SourceGraph,
    extra: impl IntoIterator<Item = PathBuf>,
) -> Vec<PathBuf> {
    metadata
        .workspace_packages()
        .into_iter()
        .map(|package| package.manifest_path.as_std_path().to_owned())
        .chain([
            metadata
                .workspace_root
                .join(".cargo/config.toml")
                .into_std_path_buf(),
            metadata
                .workspace_root
                .join(".cargo/config")
                .into_std_path_buf(),
            metadata
                .workspace_root
                .join("Cargo.toml")
                .into_std_path_buf(),
            metadata
                .workspace_root
                .join("Cargo.lock")
                .into_std_path_buf(),
            metadata
                .workspace_root
                .join("rust-toolchain.toml")
                .into_std_path_buf(),
        ])
        .chain(
            graph
                .files
                .iter()
                .map(|file| metadata.workspace_root.join(&file.path).into_std_path_buf()),
        )
        .chain(extra)
        .collect()
}
