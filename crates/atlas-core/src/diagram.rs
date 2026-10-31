//! Quotient views retain every source relation while collapsing internal structure.
mod projection;
use crate::{Result, model::Evidence};
pub use projection::project;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
pub enum Level {
    Stages,
    Modules,
    Files,
    Declarations,
}
impl Level {
    pub fn label(self) -> &'static str {
        match self {
            Self::Stages => "stages",
            Self::Modules => "modules",
            Self::Files => "files",
            Self::Declarations => "declarations",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
pub enum View {
    Dependencies,
    Types,
    #[cfg_attr(feature = "clap", value(skip))]
    Workflow,
    #[cfg_attr(feature = "clap", value(skip))]
    Program,
}
impl View {
    pub fn label(self) -> &'static str {
        match self {
            Self::Dependencies => "dependencies",
            Self::Types => "types",
            Self::Workflow => "workflow",
            Self::Program => "program",
        }
    }
}
#[derive(Debug, Clone)]
pub struct Query {
    pub focus: Option<String>,
    pub level: Level,
    pub view: View,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "observation", rename_all = "kebab-case")]
pub enum EdgeEvidence {
    Source(Evidence),
    DeclaredRead,
    SourceOrder,
    ProgramCall,
    SignatureType,
    FieldType,
}
impl EdgeEvidence {
    pub fn label(self) -> &'static str {
        match self {
            Self::Source(e) => e.label(),
            Self::DeclaredRead => "reads",
            Self::SourceOrder => "source-order",
            Self::ProgramCall => "calls",
            Self::SignatureType => "signature names",
            Self::FieldType => "field contains",
        }
    }
}
impl From<Evidence> for EdgeEvidence {
    fn from(value: Evidence) -> Self {
        Self::Source(value)
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Surface {
    pub entry: String,
    pub types: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: String,
    pub label: String,
    pub group: String,
    pub source_sites: Vec<String>,
    pub context: bool,
    pub internal_relationships: Vec<String>,
    pub surface: Option<Surface>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub from: String,
    pub to: String,
    pub evidence: EdgeEvidence,
    pub relationships: Vec<String>,
    pub declarations: Vec<String>,
}
impl Edge {
    pub fn count(&self) -> usize {
        self.relationships.len() + self.declarations.len()
    }
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Coverage {
    /// Source-control nodes contracted for display; IDs, labels and witnesses remain auditable.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub summarized_nodes: Vec<Node>,
    pub source_relationships: usize,
    pub represented_files: Vec<String>,
    pub hidden_relationships: BTreeMap<String, Vec<String>>,
    pub collapsed_files: BTreeMap<String, Vec<String>>,
    pub unassigned_files: Vec<String>,
    pub outside_profile_files: Vec<String>,
    pub hidden_dependencies: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagram {
    pub title: String,
    pub level: Level,
    pub view: View,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub inventory: Vec<String>,
    pub coverage: Coverage,
    pub notes: Vec<String>,
    pub evidence_links: Vec<(String, String)>,
}
impl Diagram {
    pub fn validate(&self) -> Result<()> {
        for (_, link) in &self.evidence_links {
            if link.is_empty()
                || link.trim() != link
                || link.starts_with('/')
                || link.contains(':')
                || link.contains('\\')
                || link.chars().any(char::is_control)
            {
                return Err("evidence links must be local relative URLs".into());
            }
        }
        let ids: BTreeSet<_> = self.nodes.iter().map(|n| &n.id).collect();
        if ids.len() != self.nodes.len() {
            return Err("diagram has duplicate node IDs".into());
        }
        if self
            .edges
            .iter()
            .any(|e| !ids.contains(&e.from) || !ids.contains(&e.to))
        {
            return Err("diagram edge refers to an absent node".into());
        }
        let carried: Vec<_> = self
            .edges
            .iter()
            .flat_map(|e| e.relationships.iter())
            .chain(
                self.nodes
                    .iter()
                    .flat_map(|n| n.internal_relationships.iter()),
            )
            .chain(self.coverage.hidden_relationships.values().flatten())
            .collect();
        let unique: BTreeSet<_> = carried.iter().collect();
        if carried.len() != unique.len() || carried.len() != self.coverage.source_relationships {
            return Err("diagram source relationship conservation failed".into());
        }
        Ok(())
    }
    pub fn summary(&self) -> String {
        if self.view == View::Program {
            return format!(
                "{} surfaced boundaries / entries; {} connections; {} represented source files",
                self.nodes.len(),
                self.edges.len(),
                self.coverage.represented_files.len()
            );
        }
        if self.view == View::Workflow {
            return format!(
                "{} source files; {} syntax nodes; {} source-structure connections",
                self.inventory.len(),
                self.nodes.len(),
                self.edges.len()
            );
        }
        let internal: usize = self
            .nodes
            .iter()
            .map(|n| n.internal_relationships.len())
            .sum();
        let hidden: usize = self
            .coverage
            .hidden_relationships
            .values()
            .map(Vec::len)
            .sum();
        let collapsed: usize = self.coverage.collapsed_files.values().map(Vec::len).sum();
        format!(
            "{} selected files; {} nodes; {} crossing groups; {internal} internal / {hidden} summarized source relations; {collapsed} collapsed files; {} unassigned / {} outside profile files",
            self.inventory.len(),
            self.nodes.len(),
            self.edges.len(),
            self.coverage.unassigned_files.len(),
            self.coverage.outside_profile_files.len()
        )
    }
}
