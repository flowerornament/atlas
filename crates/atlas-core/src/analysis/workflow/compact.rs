//! Contract only builder-identified mechanics; source-control paths keep their endpoints.
use crate::diagram::{Diagram, Edge};
use std::collections::BTreeSet;

#[derive(Default)]
pub(super) struct Folds {
    pub(super) linear: Vec<String>,
    pub(super) residuals: Vec<(String, Option<String>)>,
    pub(super) calls: BTreeSet<String>,
    pub(super) call_sites: BTreeSet<super::Site>,
}
impl Folds {
    pub(super) fn apply(self, mut diagram: Diagram) -> Diagram {
        for id in self.linear {
            let incoming: Vec<_> = diagram
                .edges
                .iter()
                .filter(|e| e.to == id)
                .cloned()
                .collect();
            let outgoing: Vec<_> = diagram
                .edges
                .iter()
                .filter(|e| e.from == id)
                .cloned()
                .collect();
            let ([before], [after]) = (incoming.as_slice(), outgoing.as_slice()) else {
                continue;
            };
            if before.from == id || after.to == id {
                continue;
            }
            let mut joined = before.clone();
            joined.to.clone_from(&after.to);
            joined.label = labels(before, after);
            diagram.edges.retain(|e| e.to != id && e.from != id);
            diagram.edges.push(joined);
            summarize(&mut diagram, &id);
        }
        for (id, binding) in self.residuals {
            let incoming: Vec<_> = diagram.edges.iter().filter(|e| e.to == id).collect();
            let [edge] = incoming.as_slice() else {
                continue;
            };
            let from = edge.from.clone();
            if !self.calls.contains(&from)
                || diagram.edges.iter().filter(|e| e.from == from).count() != 1
            {
                continue;
            }
            let Some(call) = diagram.nodes.iter().position(|n| n.id == from) else {
                continue;
            };
            let Some(decision) = diagram.nodes.iter().find(|n| n.id == id) else {
                continue;
            };
            if diagram.nodes[call].group != decision.group {
                continue;
            }
            let binding = binding.map_or_else(String::new, |name| format!("{name} = "));
            diagram.nodes[call].label = format!("{binding}{} ?", diagram.nodes[call].label);
            diagram.edges.retain(|e| e.to != id);
            for edge in &mut diagram.edges {
                if edge.from == id {
                    edge.from.clone_from(&from);
                }
            }
            summarize(&mut diagram, &id);
        }
        diagram.notes.push(format!(
            "Control detail: {} borrow/bridge/decision nodes folded; exact labels and source sites retained in coverage.summarized_nodes. Branches, loops, opaque steps and matched call returns remain.",
            diagram.coverage.summarized_nodes.len()
        ));
        diagram
    }
}
fn labels(before: &Edge, after: &Edge) -> Option<String> {
    if before.label.as_deref() == Some("residual")
        && after.label.as_deref() == Some("return residual")
    {
        return after.label.clone();
    }
    let labels: Vec<_> = before
        .label
        .iter()
        .chain(after.label.iter())
        .cloned()
        .collect();
    if labels.is_empty() {
        None
    } else {
        Some(labels.join(" · "))
    }
}
fn summarize(diagram: &mut Diagram, id: &str) {
    if let Some(index) = diagram.nodes.iter().position(|n| n.id == id) {
        diagram
            .coverage
            .summarized_nodes
            .push(diagram.nodes.remove(index));
    }
}
