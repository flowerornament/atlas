//! Mermaid emission and a self-contained pinned Pele bundle, evaluated by Node.
use super::{Artifact, Document, Renderer, visible};
use crate::{Result, diagram::Diagram};
use std::{collections::BTreeMap, fmt::Write as _};
mod process;

pub struct Pele {
    pub html: bool,
}
impl Default for Pele {
    fn default() -> Self {
        Self { html: true }
    }
}
pub(crate) fn label(text: &str) -> String {
    visible(text)
        .chars()
        .map(|c| match c {
            '"' => "#quot;".into(),
            '&' => "#38;".into(),
            '|' => "#124;".into(),
            '<' => "#60;".into(),
            '>' => "#62;".into(),
            _ => c.to_string(),
        })
        .collect()
}
/// Emit inspectable Mermaid with opaque IDs and escaped source-derived labels.
pub fn mermaid(diagram: &Diagram) -> Result<String> {
    diagram.validate()?;
    let mut out = if matches!(
        diagram.view,
        crate::diagram::View::Workflow | crate::diagram::View::Program
    ) {
        "flowchart TB\n"
    } else {
        "flowchart LR\n"
    }
    .to_owned();
    let ids: BTreeMap<_, _> = diagram
        .nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (&n.id, format!("n{i}")))
        .collect();
    for (i, (group, nodes)) in super::groups(diagram).into_iter().enumerate() {
        writeln!(out, "  subgraph g{i}[\"{}\"]", label(group))?;
        for node in nodes {
            writeln!(
                out,
                "    {}[\"{}{}\"]",
                ids[&node.id],
                label(&node.label),
                if node.context { " (context)" } else { "" }
            )?;
        }
        writeln!(out, "  end")?;
    }
    for edge in super::display_edges(diagram) {
        if edge.label.is_empty() {
            writeln!(out, "  {} --> {}", ids[&edge.from], ids[&edge.to])?;
        } else {
            writeln!(
                out,
                "  {} -->|\"{}\"| {}",
                ids[&edge.from],
                label(&edge.label),
                ids[&edge.to]
            )?;
        }
    }
    Ok(out)
}
pub(crate) fn html_text(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
impl Renderer for Pele {
    fn render_view(&self, view: super::View<'_>) -> Result<Artifact> {
        let artifact = match view {
            super::View::Graph(g) => return self.render(g),
            super::View::Representations(v, p) => crate::representations::pele(v, p)?,
            super::View::Comparison(c) => c.pele()?,
            super::View::Overview(o) => crate::overview::pele(o)?,
        };
        if !self.html {
            return Ok(artifact);
        }
        let svg = String::from_utf8(artifact.primary.bytes)?;
        let mut companions = artifact.companions;
        companions.push(Document {
            suffix: "svg",
            bytes: svg.as_bytes().to_vec(),
        });
        Ok(Artifact {
            primary: Document {
                suffix: "html",
                bytes: html_document("Atlas source view", "Native source-derived view", &svg, "")
                    .into_bytes(),
            },
            companions,
        })
    }

    fn render(&self, diagram: &Diagram) -> Result<Artifact> {
        let source = mermaid(diagram)?;
        let svg = render_source(&source)?;
        let (suffix, bytes) = if self.html {
            let mut notes = String::new();
            for note in &diagram.notes {
                write!(notes, "<li>{}</li>", html_text(note))
                    .expect("formatting into a String cannot fail");
            }
            let mut links = String::new();
            for (name, link) in &diagram.evidence_links {
                write!(
                    links,
                    "<li><a href=\"{}\">{}</a></li>",
                    html_text(link),
                    html_text(name)
                )
                .expect("formatting into a String cannot fail");
            }
            let mut surfaces = String::new();
            for node in &diagram.nodes {
                if let Some(surface) = &node.surface {
                    write!(surfaces, "<li><strong>{}</strong>: entry <code>{}</code>; public types <code>{}</code></li>", html_text(&node.label), html_text(&surface.entry), html_text(&surface.types.join(", "))).expect("formatting into a String cannot fail");
                }
            }
            let mut sites = String::new();
            for node in &diagram.nodes {
                if !node.source_sites.is_empty() {
                    write!(
                        sites,
                        "<li><strong>{}</strong>: <code>{}</code></li>",
                        html_text(&node.label),
                        html_text(&node.source_sites.join(", "))
                    )
                    .expect("formatting into a String cannot fail");
                }
            }
            let details = format!(
                "<details><summary>Source sites, evidence and declared surfaces</summary><ul>{notes}{links}</ul><ul>{surfaces}</ul><ul>{sites}</ul></details>"
            );
            (
                "html",
                html_document(&diagram.title, &diagram.summary(), &svg, &details).into_bytes(),
            )
        } else {
            ("svg", svg.as_bytes().to_vec())
        };
        let mut companions = vec![Document {
            suffix: "mmd",
            bytes: source.into_bytes(),
        }];
        if self.html {
            companions.push(Document {
                suffix: "svg",
                bytes: svg.into_bytes(),
            });
        }
        Ok(Artifact {
            primary: Document { suffix, bytes },
            companions,
        })
    }
}

/// Render native Pele input with the same embedded bundle as diagram rendering.
pub(crate) fn render_source(source: &str) -> Result<String> {
    render_source_named(source, "atlas")
}

pub(crate) fn render_source_named(source: &str, prefix: &str) -> Result<String> {
    process::render(source, prefix)
}

/// Read native Pele geometry before composing independent panels.
pub(crate) fn dimensions(svg: &str) -> Result<(f64, f64)> {
    let values = svg
        .split("viewBox=\"")
        .nth(1)
        .and_then(|s| s.split('"').next())
        .ok_or("Pele omitted viewBox")?;
    let v = values
        .split_whitespace()
        .map(str::parse::<f64>)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    if v.len() != 4 || !v.iter().all(|n| n.is_finite()) || v[2] <= 0. || v[3] <= 0. {
        return Err("invalid Pele viewBox".into());
    }
    Ok((v[2], v[3]))
}

fn html_document(title: &str, summary: &str, svg: &str, details: &str) -> String {
    format!(
        "<!doctype html><html lang=\"en\"><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>{0}</title><style>body{{font:15px system-ui;margin:24px;color:#222;background:#fff}}.diagram{{overflow:auto;border:1px solid #ddd;padding:16px}}svg{{max-width:none!important}}li{{margin:8px 0}}</style><h1>{0}</h1><p>{1}</p><div class=\"diagram\">{2}</div>{3}</html>",
        html_text(title),
        html_text(summary),
        svg,
        details
    )
}
