//! Mermaid emission and a self-contained pinned Pele bundle, evaluated by Node.
use super::{Artifact, Document, Renderer, Theme, theme::Palette, visible};
use crate::{Result, diagram::Diagram};
use std::{collections::BTreeMap, fmt::Write as _};
mod process;

/// SVG (or an HTML page around it) drawn by the embedded Pele bundle.
#[derive(Clone, Copy)]
pub struct Pele {
    /// Wrap the SVG in an HTML page with the diagram's notes and evidence links.
    pub html: bool,
    pub theme: Theme,
    /// Draw only the diagram: leave out the title, notes and panel headings
    /// Atlas writes around the `types`, `compare` and `overview` figures.
    /// The evidence keeps everything; graph diagrams have none to leave out.
    pub bare: bool,
}
impl Default for Pele {
    fn default() -> Self {
        Self {
            html: true,
            theme: Theme::default(),
            bare: false,
        }
    }
}
pub(crate) fn label(text: &str) -> String {
    let visible = visible(text);
    let mut out = String::with_capacity(visible.len());
    for c in visible.chars() {
        match c {
            '"' => out.push_str("#quot;"),
            '&' => out.push_str("#38;"),
            '|' => out.push_str("#124;"),
            '<' => out.push_str("#60;"),
            '>' => out.push_str("#62;"),
            _ => out.push(c),
        }
    }
    out
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
            super::View::Types(v, p) => crate::render::types::pele(v, p, self)?,
            super::View::Comparison(c) => c.pele(self)?,
            super::View::Overview(o) => crate::render::overview::pele(o, self)?,
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
                bytes: html_document(
                    "Atlas source view",
                    "Native source-derived view",
                    &svg,
                    "",
                    self.theme.palette(),
                )
                .into_bytes(),
            },
            companions,
        })
    }

    fn render(&self, diagram: &Diagram) -> Result<Artifact> {
        let source = mermaid(diagram)?;
        let svg = render_source(&source, self.theme.palette())?;
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
                html_document(
                    &diagram.title,
                    &diagram.summary(),
                    &svg,
                    &details,
                    self.theme.palette(),
                )
                .into_bytes(),
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
pub(crate) fn render_source(source: &str, palette: &Palette) -> Result<String> {
    render_source_named(source, "atlas", palette)
}

/// Render with a distinct ID prefix, so composed panels keep unique IDs. The
/// result carries the palette's concrete colours and an opaque background.
pub(crate) fn render_source_named(source: &str, prefix: &str, palette: &Palette) -> Result<String> {
    Ok(super::theme::resolve(
        &process::render(source, prefix)?,
        palette,
    ))
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

fn html_document(
    title: &str,
    summary: &str,
    svg: &str,
    details: &str,
    palette: &Palette,
) -> String {
    format!(
        "<!doctype html><html lang=\"en\"><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>{0}</title><style>body{{font:15px system-ui;margin:24px;color:{4};background:{5}}}a{{color:{6}}}.diagram{{overflow:auto;border:1px solid {7};padding:16px}}svg{{max-width:none!important}}li{{margin:8px 0}}</style><h1>{0}</h1><p>{1}</p><div class=\"diagram\">{2}</div>{3}</html>",
        html_text(title),
        html_text(summary),
        svg,
        details,
        palette.text,
        palette.background,
        palette.link,
        palette.border,
    )
}
