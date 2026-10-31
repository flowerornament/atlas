//! Contract single-callee wrappers; keep roots, selected anchors and branching coordinators.
use super::{Connection, Overview, Program};
use crate::{
    Result,
    diagram::{Coverage, Diagram, Edge, EdgeEvidence, Level, View},
};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
pub(super) fn overview(program: &Program) -> Overview {
    let mut nodes: BTreeMap<_, _> = program
        .nodes
        .iter()
        .map(|n| (n.id.clone(), n.clone()))
        .collect();
    let mut edges: BTreeMap<_, _> = program
        .connections
        .iter()
        .map(|e| ((e.from.clone(), e.to.clone()), e.chains.clone()))
        .collect();
    let protected: BTreeSet<_> = program
        .roots
        .iter()
        .chain(&program.anchors)
        .cloned()
        .collect();
    let mut collapsed = BTreeMap::new();
    loop {
        let candidate = nodes
            .values()
            .filter(|n| n.surface.is_none() && !n.context && !protected.contains(&n.id))
            .find_map(|n| {
                let out: Vec<_> = edges.keys().filter(|(from, _)| from == &n.id).collect();
                if out.is_empty() {
                    Some((n.id.clone(), None))
                } else if out.len() == 1 && out[0].1 != n.id {
                    Some((n.id.clone(), Some(out[0].1.clone())))
                } else {
                    None
                }
            });
        let Some((id, target)) = candidate else { break };
        let incoming: Vec<_> = edges
            .iter()
            .filter(|((_, to), _)| to == &id)
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        if let Some(target) = &target {
            let outgoing = edges
                .get(&(id.clone(), target.clone()))
                .cloned()
                .unwrap_or_default();
            for ((from, _), prefixes) in incoming {
                let chains = edges.entry((from, target.clone())).or_default();
                for prefix in prefixes {
                    for suffix in &outgoing {
                        let mut chain = prefix.clone();
                        chain.extend(suffix.clone());
                        chains.push(chain);
                    }
                }
            }
        }
        edges.retain(|(from, to), _| from != &id && to != &id);
        nodes.remove(&id);
        collapsed.insert(id, target.unwrap_or_else(|| "summarized leaf".into()));
    }
    let mut nodes: Vec<_> = nodes.into_values().collect();
    nodes.sort_by_key(|n| {
        (
            !program.roots.contains(&n.id),
            n.context,
            n.group.clone(),
            n.label.clone(),
        )
    });
    Overview {
        nodes,
        connections: edges
            .into_iter()
            .map(|((from, to), chains)| Connection { from, to, chains })
            .collect(),
        collapsed,
    }
}
fn label(edge: &Connection) -> String {
    let values: BTreeSet<_> = edge
        .chains
        .iter()
        .filter_map(|chain| {
            chain.first().map(|hop| {
                let via = if chain.len() > 1 {
                    format!(
                        " via {}",
                        chain[..chain.len() - 1]
                            .iter()
                            .map(|h| h.call.spelling.as_str())
                            .collect::<Vec<_>>()
                            .join(" → ")
                    )
                } else {
                    String::new()
                };
                format!("{}: {}{}", hop.ordinal, hop.call.arguments.join(", "), via)
            })
        })
        .collect();
    let omitted = values.len().saturating_sub(3);
    let text = values.into_iter().take(3).collect::<Vec<_>>().join("; ");
    let mut label: String = text.chars().take(100).collect();
    if text.chars().count() > 100 {
        label.push('…');
    }
    if omitted > 0 {
        write!(label, " (+{omitted} sites)").expect("formatting into a String cannot fail");
    }
    label
}
pub(super) fn diagram(program: &Program, view: &Overview) -> Result<Diagram> {
    let edges = view
        .connections
        .iter()
        .enumerate()
        .map(|(i, edge)| Edge {
            label: Some(label(edge)),
            from: edge.from.clone(),
            to: edge.to.clone(),
            evidence: EdgeEvidence::ProgramCall,
            relationships: vec![],
            declarations: vec![format!("program-overview:{i}")],
        })
        .collect();
    let diagram=Diagram{title:"Whole program — calls and stages".into(),level:Level::Stages,view:View::Program,nodes:view.nodes.clone(),edges,inventory:program.inventory.clone(),coverage:Coverage{represented_files:program.represented_files.clone(),unassigned_files:program.outside_view_files.clone(),..Default::default()},notes:vec![
        "Arrows mean calls, not execution sequence or typed data flow. Numbers give source traversal order within each caller; arguments are source spellings.".into(),
        "Single-callee wrappers and unsupported leaves are folded; complete call chains, branch/loop context, and every opaque/unresolved observation remain in program evidence.".into(),
        "Selected workflow coordinators stay visible as drilldowns. Use flow to read their within-body sequence.".into(),
        "Context nodes are declared stages not surfaced by supported syntax. Calls inside macros or deferred bodies are not guessed.".into(),
        format!("{} represented source files / {} inventoried; {} summarized or opaque observations. Outside-view files are not a dead-code claim.",program.represented_files.len(),program.inventory.len(),program.observations.len()),
    ],evidence_links:vec![]};
    diagram.validate()?;
    Ok(diagram)
}
pub(super) fn ascii(program: &Program, view: &Overview) -> String {
    let mut out = format!("Whole program (calls): {}\n\n", program.roots.join(", "));
    let names: BTreeMap<_, _> = view.nodes.iter().map(|n| (&n.id, &n.label)).collect();
    for node in &view.nodes {
        writeln!(
            out,
            "{}{} [{}]",
            node.label,
            if node.context { " (not surfaced)" } else { "" },
            node.source_sites.join(", ")
        )
        .expect("formatting into a String cannot fail");
        let mut edges: Vec<_> = view
            .connections
            .iter()
            .filter(|e| e.from == node.id)
            .collect();
        edges.sort_by_key(|e| {
            e.chains
                .iter()
                .filter_map(|c| c.first().map(|h| h.ordinal))
                .min()
        });
        for edge in edges {
            writeln!(out, "  -> {} [{}]", names[&edge.to], label(edge))
                .expect("formatting into a String cannot fail");
        }
    }
    write!(out, "\n{} coordinators folded; {} represented / {} inventoried files; {} observations retained.\n",view.collapsed.len(),program.represented_files.len(),program.inventory.len(),program.observations.len()).expect("formatting into a String cannot fail");
    out.push_str("Arrows are calls. Numbers order source call sites within a caller, not runtime paths. Use flow for sequence; program evidence retains composed chains and unresolved dispatch.\n");
    out
}
