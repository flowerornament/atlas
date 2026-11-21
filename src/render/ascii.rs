//! Folder boxes and labelled adjacency rows keep terminal views navigable.
use super::{Artifact, Document, Renderer, visible};
use crate::{
    Result,
    diagram::{Diagram, Node, View},
};
use std::{collections::BTreeMap, fmt::Write};

pub struct Ascii;
impl Renderer for Ascii {
    fn render_view(&self, view: super::View<'_>) -> Result<Artifact> {
        let text = match view {
            super::View::Graph(g) => return self.render(g),
            super::View::Types(v, p) => crate::render::types::ascii(v, p),
            super::View::Comparison(c) => c.text(),
            super::View::Overview(o) => crate::render::overview::ascii(o),
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
        write_node_groups(&mut out, diagram)?;
        write_relationships(&mut out, diagram)?;
        let coverage = coverage_section(diagram)?;
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

/// Folder boxes listing each node under a short `nN` reference.
fn write_node_groups(out: &mut String, diagram: &Diagram) -> Result<()> {
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
    Ok(())
}

/// One row per edge, named by each end's unique label.
fn write_relationships(out: &mut String, diagram: &Diagram) -> Result<()> {
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
    Ok(())
}

/// What the diagram hid, collapsed or left unassigned, and where its
/// evidence lives; empty when there is nothing to account for.
fn coverage_section(diagram: &Diagram) -> Result<String> {
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
    Ok(coverage)
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
    let edges = super::display_edges(diagram);
    let mut out = format!(
        "{}\n{} files -> {}\n",
        visible(&diagram.title),
        diagram.inventory.len(),
        coarse_headline(diagram, edges.len())
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
            write_coarse_node(&mut out, diagram.view, node, &outgoing, &labels)?;
        }
        writeln!(out, "+")?;
    }
    write_coarse_limits(&mut out, diagram)?;
    Ok(Artifact {
        primary: Document {
            suffix: "txt",
            bytes: out.into_bytes(),
        },
        companions: Vec::new(),
    })
}

/// The node and edge counts, named by what a coarse edge means in this view.
fn coarse_headline(diagram: &Diagram, edges: usize) -> String {
    let meaning = if diagram.view == View::Dependencies {
        "declared dependencies"
    } else {
        "type connections"
    };
    format!(
        "{} {} / {edges} {meaning}",
        diagram.nodes.len(),
        diagram.level.label()
    )
}

/// A dependency node lists what it reads on its own line; a type node lists
/// each connection's label beneath it.
fn write_coarse_node(
    out: &mut String,
    view: View,
    node: &Node,
    outgoing: &[&super::DisplayEdge],
    labels: &BTreeMap<&String, &String>,
) -> Result<()> {
    let context = if node.context { " (context)" } else { "" };
    if view == View::Dependencies {
        let reads = if outgoing.is_empty() {
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
        };
        writeln!(out, "|  {}{context}{reads}", visible(&node.label))?;
    } else {
        writeln!(out, "|  {}{context}", visible(&node.label))?;
        for edge in outgoing {
            writeln!(
                out,
                "|    {} -> {}",
                visible(&edge.label),
                visible(labels[&edge.to])
            )?;
        }
    }
    Ok(())
}

/// What a coarse edge does not claim, the architecture source and the
/// evidence link.
fn write_coarse_limits(out: &mut String, diagram: &Diagram) -> Result<()> {
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
    Ok(())
}
