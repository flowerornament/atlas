//! Backends present source-linked native view models as artifacts.
//! Model families retain their own meanings; rendering never reopens source.
mod ascii;
pub(crate) mod comparison;
pub(crate) mod overview;
pub(crate) mod pele;
pub(crate) mod representations;
use crate::{Result, diagram::Diagram};
pub use ascii::Ascii;
pub use pele::{Pele, mermaid};

pub struct Document {
    pub suffix: &'static str,
    pub bytes: Vec<u8>,
}
pub struct Artifact {
    pub primary: Document,
    pub companions: Vec<Document>,
}
/// Distinct native models. A matrix or comparison is not an edgeless graph.
#[derive(Clone, Copy)]
pub enum View<'a> {
    Graph(&'a Diagram),
    Representations(
        &'a crate::representations::Views,
        crate::representations::Panel,
    ),
    Comparison(&'a crate::comparison::Comparison),
    Overview(&'a crate::overview::Overview),
}
impl View<'_> {
    pub fn family(self) -> &'static str {
        match self {
            Self::Graph(_) => "graph",
            Self::Representations(..) => "representations",
            Self::Comparison(_) => "comparison",
            Self::Overview(_) => "overview",
        }
    }
}
#[derive(Debug)]
pub struct UnsupportedView(pub &'static str);
impl std::fmt::Display for UnsupportedView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "renderer does not support {} views", self.0)
    }
}
impl std::error::Error for UnsupportedView {}
pub trait Renderer {
    /// Existing graph-only clients and renderers remain valid.
    fn render(&self, diagram: &Diagram) -> Result<Artifact>;
    /// The default refuses other families rather than silently converting them.
    fn render_view(&self, view: View<'_>) -> Result<Artifact> {
        match view {
            View::Graph(g) => self.render(g),
            other => Err(UnsupportedView(other.family()).into()),
        }
    }
}

pub(crate) fn visible(text: &str) -> String {
    text.chars()
        .flat_map(|c| {
            if c.is_control() {
                c.escape_default().collect::<Vec<_>>()
            } else {
                vec![c]
            }
        })
        .collect()
}

/// Display aggregation changes layout only; exact evidence edges stay in Diagram.
pub(crate) struct DisplayEdge {
    pub from: String,
    pub to: String,
    pub label: String,
}
pub(crate) fn display_edges(diagram: &Diagram) -> Vec<DisplayEdge> {
    if diagram.view == crate::diagram::View::Types
        && matches!(
            diagram.level,
            crate::diagram::Level::Stages | crate::diagram::Level::Modules
        )
    {
        let pairs: std::collections::BTreeSet<_> = diagram
            .edges
            .iter()
            .map(|edge| (edge.from.clone(), edge.to.clone()))
            .collect();
        pairs
            .into_iter()
            .map(|(from, to)| DisplayEdge {
                from,
                to,
                label: "uses types".into(),
            })
            .collect()
    } else {
        diagram
            .edges
            .iter()
            .map(|e| DisplayEdge {
                from: e.from.clone(),
                to: e.to.clone(),
                label: e.label.clone().unwrap_or_else(|| {
                    if diagram.view == crate::diagram::View::Workflow {
                        String::new()
                    } else if matches!(e.evidence, crate::diagram::EdgeEvidence::Source(_)) {
                        format!("{} x{}", e.evidence.label(), e.count())
                    } else {
                        e.evidence.label().into()
                    }
                }),
            })
            .collect()
    }
}

/// Coarse views preserve declaration order; detailed views sort physical folders.
pub(crate) fn groups(diagram: &Diagram) -> Vec<(&str, Vec<&crate::diagram::Node>)> {
    let mut result: Vec<(&str, Vec<&crate::diagram::Node>)> = Vec::new();
    for node in &diagram.nodes {
        if let Some((_, nodes)) = result.iter_mut().find(|(name, _)| *name == node.group) {
            nodes.push(node);
        } else {
            result.push((&node.group, vec![node]));
        }
    }
    if matches!(
        diagram.level,
        crate::diagram::Level::Files | crate::diagram::Level::Declarations
    ) {
        result.sort_by_key(|(name, _)| *name);
    }
    result
}
