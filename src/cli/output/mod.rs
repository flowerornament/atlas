//! Keep source payloads in ignored evidence, publish only bounded diagram artifacts.
pub(super) mod artifact;
mod files;
pub(super) mod form;
pub(super) mod notice;
pub(super) mod rows;

use self::{
    files::{absolute, preflight, publish},
    form::{As, formats},
};
use cargo_atlas::{
    Result,
    diagram::{Diagram, Query},
    model::SourceGraph,
    render::{Artifact, Ascii, Pele, Renderer, Theme, mermaid},
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
        let path = options.out.clone().or_else(|| {
            (options.backend() == Backend::Pele).then(|| {
                root.join("target/atlas").join(format!(
                    "{base}.{}",
                    if options.format() == Format::Html {
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
            .clone()
            .unwrap_or_else(|| root.join("target/atlas"));
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
            pele: options.backend() == Backend::Pele,
            svg_companion: options.backend() == Backend::Pele && options.format() == Format::Html,
        }
    }
    pub fn prepare(
        &self,
        graph: Option<&SourceGraph>,
        diagram: &mut Diagram,
        profile: Option<&str>,
        details: Option<(&str, &str)>,
    ) -> Result<Pending> {
        self.prepare_with(graph, diagram, profile, details, None)
    }
    /// As `prepare`, plus one more evidence file: its file-name kind, its
    /// link label and its JSON.
    pub fn prepare_with(
        &self,
        graph: Option<&SourceGraph>,
        diagram: &mut Diagram,
        profile: Option<&str>,
        details: Option<(&str, &str)>,
        extra: Option<(&str, &str, &str)>,
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
        if let Some((kind, label, json)) = extra {
            let path = self.evidence.join(format!("{}.{kind}.json", self.base));
            diagram
                .evidence_links
                .push((label.to_owned(), relative_url(&self.reference, &path)?));
            files.push((path, json.as_bytes().to_vec()));
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
    pub(crate) fn render_view(
        self,
        view: cargo_atlas::render::View<'_>,
        theme: Theme,
        bare: bool,
    ) -> Result<Artifact> {
        match self {
            Self::Ascii => Ascii.render_view(view),
            Self::Pele => Pele {
                html: false,
                theme,
                bare,
            }
            .render_view(view),
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
    /// `text` draws in the terminal; `svg` draws an image and `html` a page
    /// with the image and its evidence (both need Node.js 20 or newer).
    #[arg(
        long = "as",
        id = "as",
        value_name = "FORMAT",
        value_parser = formats(&[As::Text, As::Svg, As::Html])
    )]
    form: Option<As>,
    /// Superseded by --as: `ascii` is --as text; `pele` is --as html, or
    /// --as svg with --format svg.
    #[arg(long, value_enum, default_value = "ascii")]
    backend: Backend,
    /// Superseded by --as: for --backend pele, an HTML page with the
    /// evidence, or the SVG alone.
    #[arg(long, value_enum, default_value = "html")]
    format: Format,
    /// Colours for Pele images and pages.
    #[arg(long, value_enum, default_value_t)]
    pub(super) theme: Theme,
    /// Relative output paths are resolved against the Cargo workspace root.
    #[arg(long, value_parser = crate::cli::typed::file)]
    pub(super) out: Option<PathBuf>,
    /// Where to write the JSON evidence files; defaults to `target/atlas`.
    /// Relative paths are resolved against the Cargo workspace root.
    #[arg(long, value_parser = crate::cli::typed::file)]
    pub(super) evidence_dir: Option<PathBuf>,
}

impl RenderOptions {
    pub(super) fn backend(&self) -> Backend {
        match self.form {
            Some(As::Text) => Backend::Ascii,
            Some(As::Svg | As::Html) => Backend::Pele,
            Some(As::Json | As::Jsonl) | None => self.backend,
        }
    }
    pub(super) fn format(&self) -> Format {
        match self.form {
            Some(As::Svg) => Format::Svg,
            Some(As::Html) => Format::Html,
            Some(As::Text | As::Json | As::Jsonl) | None => self.format,
        }
    }
    /// `--as json`, where the command offers it (program).
    pub(super) fn json(&self) -> bool {
        self.form == Some(As::Json)
    }
}

pub(super) fn render(options: &RenderOptions, diagram: &Diagram) -> Result<Artifact> {
    match options.backend() {
        Backend::Ascii => Ascii.render(diagram),
        Backend::Pele => Pele {
            html: options.format() == Format::Html,
            theme: options.theme,
            bare: false,
        }
        .render(diagram),
    }
}
/// Write one file to `path`, refusing a destination that is a protected input.
pub(super) fn write_protected(path: &Path, bytes: Vec<u8>, protected: &[PathBuf]) -> Result<()> {
    publish(vec![(path.to_owned(), bytes)], protected)
}
pub(super) fn protected_inputs(
    metadata: &cargo_metadata::Metadata,
    graph: &cargo_atlas::model::SourceGraph,
    extra: impl IntoIterator<Item = PathBuf>,
) -> Vec<PathBuf> {
    protected_paths(metadata, graph_sources(metadata, graph), extra)
}
/// The captured graph's source files, as paths under the workspace root.
pub(super) fn graph_sources<'a>(
    metadata: &'a cargo_metadata::Metadata,
    graph: &'a cargo_atlas::model::SourceGraph,
) -> impl Iterator<Item = PathBuf> + 'a {
    graph
        .files
        .iter()
        .map(|file| metadata.workspace_root.join(&file.path).into_std_path_buf())
}
/// Source files with the fingerprint of the bytes a command read.
pub(super) type Captured = Vec<(PathBuf, String)>;
/// Each captured source file with the fingerprint of the bytes the capture
/// read, so a step that reads the live tree later (the indexer) can tell
/// that a file changed since.
pub(super) fn graph_captures<'a>(
    metadata: &'a cargo_metadata::Metadata,
    graph: &'a cargo_atlas::model::SourceGraph,
) -> impl Iterator<Item = (PathBuf, String)> + 'a {
    graph.files.iter().map(|file| {
        (
            metadata.workspace_root.join(&file.path).into_std_path_buf(),
            file.content_fingerprint.clone(),
        )
    })
}
/// The manifests and configuration every command protects, plus `sources`
/// and `extra`.
pub(super) fn protected_paths(
    metadata: &cargo_metadata::Metadata,
    sources: impl IntoIterator<Item = PathBuf>,
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
        .chain(sources)
        .chain(extra)
        .collect()
}
