//! Keep source payloads in ignored evidence, publish only bounded diagram artifacts.
use crate::{
    Backend, Format, RenderOptions,
    output_files::{absolute, write},
};
use atlas::{
    Result,
    diagram::{Diagram, Query},
    model::SourceGraph,
    render::{Artifact, mermaid},
};
use std::{
    io::Write,
    path::{Path, PathBuf},
};

pub(crate) struct Destination {
    pub path: Option<PathBuf>,
    evidence: PathBuf,
    base: String,
    reference: PathBuf,
    svg_companion: bool,
}
impl Destination {
    pub fn new(root: &Path, options: &RenderOptions, query: &Query) -> Self {
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
            svg_companion: options.backend == Backend::Pele && options.format == Format::Html,
        }
    }
    pub fn prepare(
        &self,
        graph: Option<&SourceGraph>,
        diagram: &mut Diagram,
        profile: Option<&str>,
        details: Option<(&str, &str)>,
        pele: bool,
    ) -> Result<()> {
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
        files.push((diagram_path, serde_json::to_vec_pretty(diagram)?));
        if pele {
            let path = self
                .path
                .as_ref()
                .ok_or("Pele output destination missing")?;
            files.push((path.with_extension("mmd"), mermaid(diagram)?.into_bytes()));
        }
        const COLLISION: &str = "output path collides with a companion; choose the primary .html/.svg/.txt extension and matching --format";
        let mut destinations = std::collections::BTreeSet::new();
        if let Some(path) = &self.path {
            destinations.insert(absolute(path)?);
            if self.svg_companion && !destinations.insert(absolute(&path.with_extension("svg"))?) {
                return Err(COLLISION.into());
            }
        }
        for (path, _) in &files {
            if !destinations.insert(absolute(path)?) {
                return Err(COLLISION.into());
            }
        }
        for (path, bytes) in files {
            write(&path, bytes)?;
        }
        Ok(())
    }
    pub fn write(&self, artifact: Artifact) -> Result<()> {
        if let Some(path) = &self.path {
            for document in artifact.companions {
                write(&path.with_extension(document.suffix), document.bytes)?;
            }
            write(path, artifact.primary.bytes)?;
        } else {
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
