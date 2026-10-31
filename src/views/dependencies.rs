//! Qualified source relationships between files, never declared permission or runtime order.
use crate::{
    Result,
    diagram::{Coverage, Diagram, Edge, EdgeEvidence, Level, Node, View},
    representations::{Facts, Role},
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum Relation {
    #[default]
    Calls,
    Signatures,
    Fields,
    All,
}
// Preserve the historical alphabetic relation order in diagram/evidence edges.
impl Ord for Relation {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        fn order(relation: Relation) -> u8 {
            match relation {
                Relation::All => 0,
                Relation::Calls => 1,
                Relation::Fields => 2,
                Relation::Signatures => 3,
            }
        }
        order(*self).cmp(&order(*other))
    }
}
impl PartialOrd for Relation {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Witness {
    pub from: String,
    pub to: String,
    pub relation: Relation,
    pub file: String,
    pub line: usize,
    pub description: String,
    pub source: String,
    pub target: String,
}
#[derive(Debug, Serialize)]
pub struct Dependencies {
    pub diagram: Diagram,
    pub witnesses: Vec<Witness>,
    pub outside: Vec<Witness>,
    pub unresolved_calls: usize,
}
/// Every edge has a positive source witness; unresolved names create no guessed edge.
/// The physical scope chooses both endpoints. Outside relationships remain in evidence.
pub fn project(facts: &Facts, scope: &str, relation: Relation) -> Result<Dependencies> {
    let files: BTreeSet<_> = facts
        .fingerprints
        .iter()
        .filter(|(f, _)| crate::architecture::contains(scope, f))
        .map(|(f, _)| f.clone())
        .collect();
    if files.is_empty() {
        return Err(format!("no production files in {scope}").into());
    }
    let functions: BTreeMap<_, _> = facts.functions.iter().map(|f| (f.id.as_str(), f)).collect();
    let types: BTreeMap<_, _> = facts.types.iter().map(|t| (t.id.as_str(), t)).collect();
    let mut raw = Vec::new();
    if matches!(relation, Relation::All | Relation::Calls) {
        for c in &facts.calls {
            let (Some(a), Some(b)) = (functions.get(c.from.as_str()), functions.get(c.to.as_str()))
            else {
                continue;
            };
            raw.push(Witness {
                from: a.file.clone(),
                to: b.file.clone(),
                relation: Relation::Calls,
                file: c.site.path.clone(),
                line: c.site.line,
                description: format!("{} calls {}", a.name, b.name),
                source: a.id.clone(),
                target: b.id.clone(),
            });
        }
    }
    for r in &facts.references {
        let is_field = r.role == Role::Field;
        if !(matches!(relation, Relation::All)
            || (is_field && relation == Relation::Fields)
            || (!is_field && relation == Relation::Signatures))
        {
            continue;
        }
        let (file, name) = if let Some(f) = functions.get(r.from.as_str()) {
            (&f.file, &f.name)
        } else if let Some(t) = types.get(r.from.as_str()) {
            (&t.file, &t.name)
        } else {
            continue;
        };
        let Some(t) = types.get(r.to.as_str()) else {
            continue;
        };
        raw.push(Witness {
            from: file.clone(),
            to: t.file.clone(),
            relation: if is_field {
                Relation::Fields
            } else {
                Relation::Signatures
            },
            file: file.clone(),
            line: r.line,
            description: format!("{name}.{} : {}", r.slot, r.expression),
            source: r.from.clone(),
            target: r.to.clone(),
        });
    }
    raw.sort_by(|a, b| {
        (&a.from, &a.to, a.line, &a.description).cmp(&(&b.from, &b.to, b.line, &b.description))
    });
    let (witnesses, outside): (Vec<_>, Vec<_>) = raw
        .into_iter()
        .filter(|w| w.from != w.to && files.contains(&w.from))
        .partition(|w| files.contains(&w.to));
    let mut grouped = BTreeMap::<(String, String, Relation), Vec<usize>>::new();
    for (i, w) in witnesses.iter().enumerate() {
        grouped
            .entry((w.from.clone(), w.to.clone(), w.relation))
            .or_default()
            .push(i);
    }
    let edges = grouped
        .into_iter()
        .map(|((from, to, _), indices)| {
            let first = &witnesses[indices[0]];
            let evidence = match first.relation {
                Relation::Calls => EdgeEvidence::ProgramCall,
                Relation::Signatures => EdgeEvidence::SignatureType,
                Relation::Fields => EdgeEvidence::FieldType,
                Relation::All => unreachable!(),
            };
            let names: BTreeSet<_> = indices
                .iter()
                .map(|&i| witnesses[i].description.clone())
                .collect();
            Edge {
                from,
                to,
                label: Some(format!(
                    "{}: {}",
                    evidence.label(),
                    names.into_iter().collect::<Vec<_>>().join("; ")
                )),
                evidence,
                relationships: indices.iter().map(|i| format!("qualified:{i}")).collect(),
                declarations: vec![],
            }
        })
        .collect();
    let unresolved_calls = facts
        .unknown_calls
        .iter()
        .filter(|u| {
            functions
                .get(u.from.as_str())
                .is_some_and(|f| files.contains(&f.file))
        })
        .count();
    let mut diagram = Diagram {
        title: format!(
            "Source {:?}: {}",
            relation,
            if scope.is_empty() { "workspace" } else { scope }
        ),
        level: Level::Files,
        view: View::Types,
        nodes: files
            .iter()
            .map(|file| Node {
                id: file.clone(),
                label: file.rsplit('/').next().unwrap_or(file).into(),
                group: file.rsplit_once('/').map_or(".", |(parent, _)| parent).into(),
                source_sites: vec![file.clone()],
                context: false,
                internal_relationships: vec![],
                surface: None,
            })
            .collect(),
        edges,
        inventory: files.iter().cloned().collect(),
        coverage: Coverage {
            source_relationships: witnesses.len(),
            represented_files: files.into_iter().collect(),
            ..Coverage::default()
        },
        notes: vec![
            "Calls, signature types and direct field types are separate positive source observations; none denotes runtime order.".into(),
            format!(
                "{} outside-scope observations retained in evidence; {unresolved_calls} unresolved/external call observations are not proved absent.",
                outside.len()
            ),
        ],
        evidence_links: vec![],
    };
    diagram.notes.extend(facts.limitations.clone());
    diagram.validate()?;
    Ok(Dependencies {
        diagram,
        witnesses,
        outside,
        unresolved_calls,
    })
}
