//! Compose native textual stage readings over captured source and declared order.
//! Root binding, path syntax, interfaces and item seams have separate owners;
//! reports retain unjudged evidence rather than asserting a complete policy PASS.
mod interfaces;
mod paths;
mod roots;
pub mod seams;
#[cfg(test)]
mod tests;
use super::{CrateMap, FileMap, ItemKind, policy};
#[cfg(test)]
use crate::population::TestSelection;
pub(super) use interfaces::interface_policy_violations;
use paths::{PathForm, inline_paths, read_path, source_lines, test_source_lines};
pub(in crate::checks) use roots::RootTable;
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fmt,
};
/// One crate-internal path from one module into another, at its site.
#[derive(Debug, Clone, Serialize)]
pub struct Edge {
    pub file: String,
    pub line: usize,
    pub from: String,
    pub to: String,
    pub path: String,
    /// The path resolved from the crate root, `to` first
    /// (`["cover", "timing", "RegionWork"]`): what the item-level seam reads.
    pub target: Vec<String>,
}

/// Why the textual reading cannot rank a path.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Unjudged {
    /// A glob over the crate root (`use crate::*`).
    RootGlob,
    /// A root name lib.rs neither defines nor binds by name, while a lib.rs
    /// glob re-exports a crate module that may supply it.
    GlobbedRootName,
    /// A root name lib.rs neither defines nor binds, and no glob supplies.
    UnknownRootName,
    /// A chain of root aliases beyond the reader's fixed depth limit.
    AliasChain,
    /// Competing conditional root bindings cannot select one arbitrary target.
    AmbiguousRootName,
    /// The shared source grammar could not read this use statement.
    UnreadableImport,
}

impl Unjudged {
    /// The fix a refusal names.
    fn remedy(self) -> &'static str {
        match self {
            Self::AmbiguousRootName => {
                "name the defining stage directly; competing root bindings are unresolved"
            }
            Self::UnreadableImport => {
                "write a complete use statement in the supported Rust source form"
            }
            Self::RootGlob => "import each name by its true path, `crate::<stage>::…`",
            Self::GlobbedRootName => {
                "name the item by its true path, `crate::<stage>::…`, or replace the lib.rs glob with named re-exports"
            }
            Self::UnknownRootName | Self::AliasChain => {
                "name the item by its true path, `crate::<stage>::…`"
            }
        }
    }
}

impl fmt::Display for Unjudged {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::AmbiguousRootName => "competing crate-root bindings with the same name",
            Self::UnreadableImport => "a use statement whose syntax could not be read",
            Self::RootGlob => "a glob over the crate root names nothing the reading can rank",
            Self::GlobbedRootName => "a crate-root name a lib.rs glob re-export may supply",
            Self::UnknownRootName => "a crate-root name lib.rs neither defines nor re-exports",
            Self::AliasChain => "a chain of crate-root aliases too long to follow",
        })
    }
}

/// One path the reading could not rank, at its site.
#[derive(Debug, Serialize)]
pub struct UnjudgedPath {
    pub file: String,
    pub line: usize,
    pub path: String,
    pub why: Unjudged,
}

/// A forward edge through unplaced modules: a placed stage reads an
/// unplaced module that, directly or through further unplaced modules,
/// reads a later stage.
#[derive(Debug, Serialize)]
pub struct TransitiveEdge {
    pub reader: Edge,
    pub via: Vec<String>,
    pub onward: Edge,
}

/// One crate's stage reading over its production files: forward edges, the
/// judged edge count, the paths and modules it could not judge, and the
/// modules and stages the order and the tree do not share.
#[derive(Debug, Serialize)]
pub struct StageReport {
    /// The declared stage interfaces, when the crate declares them.
    pub interfaces: Option<BTreeMap<String, policy::StageReads>>,
    /// The stages every stage reads whole, when the crate declares seams.
    pub floor: Vec<String>,
    pub forward: Vec<Edge>,
    /// Each path from a stage into an earlier one: what the stage reads.
    pub backward: Vec<Edge>,
    /// Every production path from one top-level module into another, each
    /// resolved target its own edge: the item-level seam reads these.
    pub crossing: Vec<Edge>,
    pub through_unplaced: Vec<TransitiveEdge>,
    pub unjudged: Vec<UnjudgedPath>,
    pub judged: usize,
    pub unplaced: BTreeSet<String>,
    pub inline_modules: BTreeSet<String>,
    pub absent: Vec<String>,
    /// Modules that share their name with a lib.rs re-export.
    pub shared_names: BTreeSet<String>,
}

impl StageReport {
    #[cfg(test)]
    pub(super) fn of(crate_map: &CrateMap, stages: &[String]) -> Self {
        let root = RootTable::of(crate_map, TestSelection::Production);
        Self::from_root(crate_map, stages, &root)
    }
    pub(in crate::checks) fn from_root(
        crate_map: &CrateMap,
        stages: &[String],
        root: &RootTable,
    ) -> Self {
        let rank = |module: &str| stages.iter().position(|stage| stage == module);
        let mut report = Self {
            interfaces: None,
            floor: Vec::new(),
            forward: Vec::new(),
            backward: Vec::new(),
            crossing: Vec::new(),
            through_unplaced: Vec::new(),
            unjudged: root
                .unreadable_imports
                .iter()
                .map(|(line, text)| UnjudgedPath {
                    file: format!("{}/lib.rs", crate_map.src_rel),
                    line: *line,
                    path: text.clone(),
                    why: Unjudged::UnreadableImport,
                })
                .collect(),
            judged: 0,
            unplaced: BTreeSet::new(),
            inline_modules: root.inline_modules.clone(),
            absent: Vec::new(),
            shared_names: root
                .modules
                .iter()
                .filter(|module| root.bindings.contains_key(*module))
                .cloned()
                .collect(),
        };
        let mut present = BTreeSet::new();
        // Every cross-module read, placed or not, for the transitive judgment.
        let mut reads: BTreeMap<String, Vec<Edge>> = BTreeMap::new();
        for (module, file) in crate_map.production_modules() {
            let Some(module) = module else {
                continue;
            };
            let module: Vec<&str> = module.split("::").collect();
            let from = module[0].to_owned();
            present.insert(from.clone());
            let from_rank = rank(&from);
            if from_rank.is_none() {
                report.unplaced.insert(from.clone());
            }
            let lines = source_lines(file);
            let (edges, unjudged) = root.edges(&file.rel, &module, &lines);
            report.unjudged.extend(unjudged);
            let mut seen = BTreeSet::new();
            for edge in edges {
                if edge.to == from {
                    continue;
                }
                reads.entry(from.clone()).or_default().push(edge.clone());
                report.crossing.push(edge.clone());
                let (Some(from_rank), Some(to_rank)) = (from_rank, rank(&edge.to)) else {
                    continue;
                };
                if !seen.insert((edge.line, edge.to.clone())) {
                    continue;
                }
                report.judged += 1;
                if to_rank > from_rank {
                    report.forward.push(edge);
                } else {
                    report.backward.push(edge);
                }
            }
        }
        for via in &report.unplaced {
            let Some((chain, onward)) = latest_reach(via, &reads, &report.unplaced, &rank) else {
                continue;
            };
            let onward_rank = rank(&onward.to).expect("reach ends at a ranked stage");
            for edges in reads.values() {
                for reader in edges.iter().filter(|edge| &edge.to == via) {
                    if rank(&reader.from).is_some_and(|reader_rank| reader_rank < onward_rank) {
                        report.through_unplaced.push(TransitiveEdge {
                            reader: reader.clone(),
                            via: chain.clone(),
                            onward: onward.clone(),
                        });
                    }
                }
            }
        }
        report.absent = stages
            .iter()
            .filter(|stage| !present.contains(*stage))
            .cloned()
            .collect();
        report
    }
}

/// The latest stage an unplaced module reaches through unplaced modules
/// only: the chain of unplaced modules walked (starting at `start`) and the
/// read that reaches the stage.
fn latest_reach(
    start: &str,
    reads: &BTreeMap<String, Vec<Edge>>,
    unplaced: &BTreeSet<String>,
    rank: &impl Fn(&str) -> Option<usize>,
) -> Option<(Vec<String>, Edge)> {
    let mut best: Option<(usize, Vec<String>, Edge)> = None;
    let mut visited = BTreeSet::from([start.to_owned()]);
    let mut queue = VecDeque::from([vec![start.to_owned()]]);
    while let Some(chain) = queue.pop_front() {
        let module = chain.last().expect("a chain names its module");
        for edge in reads.get(module).into_iter().flatten() {
            if let Some(to_rank) = rank(&edge.to) {
                if best.as_ref().is_none_or(|(rank, _, _)| to_rank > *rank) {
                    best = Some((to_rank, chain.clone(), edge.clone()));
                }
            } else if unplaced.contains(&edge.to) && visited.insert(edge.to.clone()) {
                let mut next = chain.clone();
                next.push(edge.to.clone());
                queue.push_back(next);
            }
        }
    }
    best.map(|(_, chain, edge)| (chain, edge))
}
