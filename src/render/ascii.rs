//! Folder boxes and labelled adjacency rows keep terminal views navigable.
use super::{Artifact, Document, Renderer, visible};
use crate::{Result, diagram::Diagram};
use std::{collections::BTreeMap, fmt::Write};

pub struct Ascii;
impl Renderer for Ascii {
    fn render_view(&self, view: super::View<'_>) -> Result<Artifact> {
        let text = match view {
            super::View::Graph(g) => return self.render(g),
            super::View::Types(v, p) => crate::types::ascii(v, p),
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
        let labels = unique_labels(diagram);
        for edge in &diagram.edges {
            writeln!(
                out,
                "  [{}] --{} ({} {})--> [{}]",
                visible(&labels[edge.from.as_str()]),
                edge.label
                    .as_deref()
                    .unwrap_or_else(|| edge.evidence.label()),
                edge.count(),
                if edge.count() == 1 { "site" } else { "sites" },
                visible(&labels[edge.to.as_str()])
            )?;
        }
        let mut coverage = String::new();
        for (reason, ids) in &diagram.coverage.hidden_relationships {
            writeln!(
                coverage,
                "  {}: {} source relationships",
                visible(reason),
                ids.len()
            )?;
        }
        for (kind, files) in &diagram.coverage.collapsed_files {
            writeln!(
                coverage,
                "  collapsed {}: {} files",
                visible(kind),
                files.len()
            )?;
        }
        if !diagram.coverage.unassigned_files.is_empty() {
            writeln!(
                coverage,
                "  unassigned: {}",
                diagram.coverage.unassigned_files.join(", ")
            )?;
        }
        for (name, link) in &diagram.evidence_links {
            writeln!(coverage, "  {}: {}", visible(name), visible(link))?;
        }
        // An empty section says nothing; leave it out.
        if !coverage.is_empty() {
            write!(out, "\nCOVERAGE\n{coverage}")?;
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

/// Each node's label, or, where labels repeat (several `mod.rs`), the
/// shortest suffix of its path that no other node's path ends in.
fn unique_labels(diagram: &Diagram) -> BTreeMap<&str, String> {
    let mut counts = BTreeMap::<&str, usize>::new();
    for node in &diagram.nodes {
        *counts.entry(node.label.as_str()).or_default() += 1;
    }
    diagram
        .nodes
        .iter()
        .map(|node| {
            if counts[node.label.as_str()] == 1 {
                return (node.id.as_str(), node.label.clone());
            }
            let parts: Vec<&str> = node.id.split('/').collect();
            let suffix = (1..=parts.len())
                .map(|n| parts[parts.len() - n..].join("/"))
                .find(|suffix| {
                    diagram.nodes.iter().all(|other| {
                        other.id == node.id
                            || !(other.id == *suffix || other.id.ends_with(&format!("/{suffix}")))
                    })
                })
                .unwrap_or_else(|| node.id.clone());
            (node.id.as_str(), suffix)
        })
        .collect()
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
