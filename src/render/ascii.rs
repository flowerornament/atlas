//! Folder boxes and labelled adjacency rows keep terminal views navigable.
use super::{Artifact, Document, Renderer, visible};
use crate::{Result, diagram::Diagram};
use std::{collections::BTreeMap, fmt::Write};

pub struct Ascii;
impl Renderer for Ascii {
    fn render_view(&self, view: super::View<'_>) -> Result<Artifact> {
        let text = match view {
            super::View::Graph(g) => return self.render(g),
            super::View::Representations(v, p) => crate::representations::ascii(v, p),
            super::View::Comparison(c) => c.text(),
            super::View::Overview(o) => crate::overview::ascii(o),
        };
        Ok(Artifact {
            primary: Document {
                suffix: "txt",
                bytes: text.into_bytes(),
            },
            companions: vec![],
        })
    }

    fn render(&self, diagram: &Diagram) -> Result<Artifact> {
        diagram.validate()?;
        if matches!(
            diagram.level,
            crate::diagram::Level::Stages | crate::diagram::Level::Modules
        ) {
            return coarse(diagram);
        }
        let mut out = format!("{}\n{}\n\n", visible(&diagram.title), diagram.summary());
        let ids: BTreeMap<_, _> = diagram
            .nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (&n.id, format!("n{}", i + 1)))
            .collect();
        for (group, nodes) in super::groups(diagram) {
            writeln!(out, "+-- {}", visible(group))?;
            for node in nodes {
                writeln!(
                    out,
                    "|  [{}] {}{}",
                    ids[&node.id],
                    visible(&node.label),
                    if node.context { " (context)" } else { "" }
                )?;
            }
            writeln!(out, "+")?;
        }
        writeln!(out, "\nRELATIONSHIPS")?;
        let labels: BTreeMap<_, _> = diagram.nodes.iter().map(|n| (&n.id, &n.label)).collect();
        for edge in &diagram.edges {
            writeln!(
                out,
                "  [{}] --{} ({} sites)--> [{}]",
                visible(labels[&edge.from]),
                edge.label
                    .as_deref()
                    .unwrap_or_else(|| edge.evidence.label()),
                edge.count(),
                visible(labels[&edge.to])
            )?;
        }
        writeln!(out, "\nCOVERAGE")?;
        for (reason, ids) in &diagram.coverage.hidden_relationships {
            writeln!(
                out,
                "  {}: {} source relationships",
                visible(reason),
                ids.len()
            )?;
        }
        for (kind, files) in &diagram.coverage.collapsed_files {
            writeln!(out, "  collapsed {}: {} files", visible(kind), files.len())?;
        }
        if !diagram.coverage.unassigned_files.is_empty() {
            writeln!(
                out,
                "  unassigned: {}",
                diagram.coverage.unassigned_files.join(", ")
            )?;
        }
        for (name, link) in &diagram.evidence_links {
            writeln!(out, "  {}: {}", visible(name), visible(link))?;
        }
        writeln!(out, "\nEVIDENCE / LIMITS")?;
        for note in &diagram.notes {
            writeln!(out, "  - {}", visible(note))?;
        }
        Ok(Artifact {
            primary: Document {
                suffix: "txt",
                bytes: out.into_bytes(),
            },
            companions: Vec::new(),
        })
    }
}

fn coarse(diagram: &Diagram) -> Result<Artifact> {
    use crate::diagram::View;
    let edges = super::display_edges(diagram);
    let headline = if diagram.view == View::Dependencies {
        format!(
            "{} {} / {} declared dependencies",
            diagram.nodes.len(),
            diagram.level.label(),
            edges.len()
        )
    } else {
        format!(
            "{} {} / {} type connections",
            diagram.nodes.len(),
            diagram.level.label(),
            edges.len()
        )
    };
    let mut out = format!(
        "{}\n{} files -> {}\n",
        visible(&diagram.title),
        diagram.inventory.len(),
        headline
    );
    let collapsed = diagram
        .coverage
        .collapsed_files
        .iter()
        .map(|(kind, files)| format!("{} {kind}", files.len()))
        .collect::<Vec<_>>()
        .join(", ");
    if !collapsed.is_empty() {
        writeln!(
            out,
            "Collapsed files: {collapsed}; {} unassigned files.",
            diagram.coverage.unassigned_files.len()
        )?;
    }
    let labels: BTreeMap<_, _> = diagram.nodes.iter().map(|n| (&n.id, &n.label)).collect();
    for (group, nodes) in super::groups(diagram) {
        writeln!(out, "\n+-- {}", visible(group))?;
        for node in nodes {
            let outgoing: Vec<_> = edges.iter().filter(|e| e.from == node.id).collect();
            if diagram.view == View::Dependencies {
                writeln!(
                    out,
                    "|  {}{}{}",
                    visible(&node.label),
                    if node.context { " (context)" } else { "" },
                    if outgoing.is_empty() {
                        String::new()
                    } else {
                        format!(
                            "  reads -> {}",
                            outgoing
                                .iter()
                                .map(|e| visible(labels[&e.to]))
                                .collect::<Vec<_>>()
                                .join(", ")
                        )
                    }
                )?;
            } else {
                writeln!(
                    out,
                    "|  {}{}",
                    visible(&node.label),
                    if node.context { " (context)" } else { "" }
                )?;
                for edge in outgoing {
                    writeln!(
                        out,
                        "|    {} -> {}",
                        visible(&edge.label),
                        visible(labels[&edge.to])
                    )?;
                }
            }
        }
        writeln!(out, "+")?;
    }
    writeln!(
        out,
        "\n{}",
        if diagram.view == View::Dependencies {
            "reads = declared allowed dependency, not execution order."
        } else {
            "Type mentions, not runtime flow. Internal/ambiguous relationships remain in evidence."
        }
    )?;
    for note in diagram
        .notes
        .iter()
        .filter(|n| n.starts_with("Architecture source:"))
    {
        writeln!(out, "{}", visible(note))?;
    }
    if let Some((_, link)) = diagram
        .evidence_links
        .iter()
        .find(|(name, _)| name == "diagram evidence")
    {
        writeln!(out, "Evidence: {}", visible(link))?;
    }
    Ok(Artifact {
        primary: Document {
            suffix: "txt",
            bytes: out.into_bytes(),
        },
        companions: Vec::new(),
    })
}
