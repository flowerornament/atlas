//! Native item-level stage seams over captured source and supplied boundary declarations.
use std::collections::{BTreeMap, BTreeSet};

use super::super::policy::{self, StageOrder};
use super::super::{CrateMap, module_path_of};
use super::{Edge, RootTable, StageReport, test_source_lines};
use crate::population::TestSelection;

mod boundary;
mod ratchets;
mod view;
pub use view::Totals;
mod writers;

pub use boundary::BoundaryDeclaration;
use boundary::{Boundary, boundaries};

/// One read past a boundary: the item named, at its first site.
#[derive(Clone)]
struct Site {
    file: String,
    line: usize,
    path: String,
}

impl Site {
    fn of(edge: &Edge) -> Self {
        Self {
            file: edge.file.clone(),
            line: edge.line,
            path: edge.path.clone(),
        }
    }
}

/// A path's item within its top-level module: the module path that holds it
/// and the item's name, `None` when the path names a module itself. A glob
/// is named by its module (`formation::*`), so each globbed module counts.
struct ItemRef {
    module: Vec<String>,
    name: Option<String>,
}

impl ItemRef {
    /// Split a resolved path at the longest prefix that is a file module.
    fn of(target: &[String], modules: &BTreeSet<Vec<String>>) -> Self {
        let held = (1..=target.len())
            .rev()
            .find(|&len| modules.contains(&target[..len]))
            .unwrap_or(1);
        let module = target[..held].to_vec();
        let name = target.get(held).map(|name| {
            if name == "*" {
                module[1..]
                    .iter()
                    .map(String::as_str)
                    .chain(["*"])
                    .collect::<Vec<_>>()
                    .join("::")
            } else {
                name.clone()
            }
        });
        Self { module, name }
    }
}

/// An interior item read from an input that owes no debt row: a coupling
/// the policy never declared.
struct UnownedRead {
    reader: String,
    read: String,
    item: String,
    site: Site,
}

/// One seamed crate's item-level reading.
pub struct SeamReading {
    crate_name: String,
    /// Reader stage, then read stage, then item name: the first site.
    debt: BTreeMap<String, BTreeMap<String, BTreeMap<String, Site>>>,
    /// Test owner, then (test file, `stage::item`): the first site.
    tests: BTreeMap<String, BTreeMap<(String, String), Site>>,
    /// Interior items read from an input stage that has no debt row.
    unowned: Vec<UnownedRead>,
    /// The crate's top-level modules in the tree read.
    present: BTreeSet<String>,
    /// Each non-floor stage's boundary as its root states and exports it.
    boundaries: BTreeMap<String, Boundary>,
    /// The floor stages' roots, read whole: kept to name what a stage moving
    /// into the floor would stop guarding.
    floor_boundaries: BTreeMap<String, Boundary>,
    /// Per writers row: its census over the tree read.
    writer_census: BTreeMap<String, writers::WriterCensus>,
    /// The seam rows the candidate policy records.
    held: policy::StageSeams,
}

impl SeamReading {
    pub(in crate::checks) fn of(
        crate_map: &CrateMap,
        order: &StageOrder,
        report: &StageReport,
        seams: &policy::StageSeams,
        declarations: &BTreeMap<String, BoundaryDeclaration>,
        root: &RootTable,
    ) -> Result<Self, Vec<crate::model::Diagnostic>> {
        let (floor_boundaries, boundaries): (BTreeMap<_, _>, BTreeMap<_, _>) =
            boundaries(crate_map, &order.stages, root, declarations)?
                .into_iter()
                .partition(|(stage, _)| seams.floor.contains(stage));
        let mut reading = Self {
            crate_name: order.crate_name.clone(),
            debt: BTreeMap::new(),
            tests: BTreeMap::new(),
            unowned: Vec::new(),
            present: root
                .modules
                .iter()
                .cloned()
                .chain(crate_map.mounted.iter().map(|mount| {
                    mount
                        .module
                        .split("::")
                        .next()
                        .unwrap_or_default()
                        .to_owned()
                }))
                .collect(),
            boundaries,
            floor_boundaries,
            writer_census: writers::census(crate_map, &seams.writers),
            held: policy::StageSeams {
                floor: seams.floor.clone(),
                exports: seams.exports.clone(),
                writers: seams.writers.clone(),
                test_coupling: seams.test_coupling.clone(),
            },
        };
        reading.read_production(report, order, &root.module_paths);
        let test_root = RootTable::of(crate_map, TestSelection::All);
        let unavailable = reading.read_tests(crate_map, &test_root, &order.stages);
        if unavailable.is_empty() {
            Ok(reading)
        } else {
            Err(unavailable)
        }
    }

    /// The interior item an edge from `reader` names in `edge.to`, or `None`
    /// for a module path (what is read through it is read on its own lines)
    /// and for a free read: a listed boundary item of a stage `admits`, or a
    /// writer function of a type `reader` produces.
    fn interior(
        &self,
        reader: &str,
        edge: &Edge,
        modules: &BTreeSet<Vec<String>>,
        admits: impl Fn(&str) -> bool,
    ) -> Option<String> {
        let item = ItemRef::of(&edge.target, modules);
        let name = item.name?;
        let listed = self
            .boundaries
            .get(&edge.to)
            .is_some_and(|boundary| boundary.names(&item.module, &name));
        let writes = self.held.writers.values().any(|writers| {
            writers.stage == edge.to
                && writers.producers.iter().any(|producer| producer == reader)
                && writers.functions.contains(&name)
        });
        let free = (listed && admits(&edge.to)) || writes;
        (!free).then_some(name)
    }

    /// Each stage's production reads past its inputs' boundaries.
    fn read_production(
        &mut self,
        report: &StageReport,
        order: &StageOrder,
        modules: &BTreeSet<Vec<String>>,
    ) {
        let rank = |stage: &str| order.stages.iter().position(|named| named == stage);
        let interfaces = report.interfaces.as_ref();
        for edge in &report.crossing {
            let (Some(from_rank), Some(to_rank)) = (rank(&edge.from), rank(&edge.to)) else {
                continue;
            };
            if to_rank >= from_rank || self.held.floor.contains(&edge.to) {
                continue;
            }
            let declared = interfaces.and_then(|rows| rows.get(&edge.from));
            if declared.is_some_and(|row| row.reads.contains(&edge.to)) {
                continue;
            }
            let is_input = |stage: &str| {
                declared.is_some_and(|row| row.input.iter().any(|input| input == stage))
            };
            let Some(name) = self.interior(&edge.from, edge, modules, is_input) else {
                continue;
            };
            // An undeclared stage is the stage reading's refusal, not a seam's.
            match declared {
                Some(row) if row.debt.row(&edge.to).is_some() => {}
                Some(row) if row.input.contains(&edge.to) => self.unowned.push(UnownedRead {
                    reader: edge.from.clone(),
                    read: edge.to.clone(),
                    item: name.clone(),
                    site: Site::of(edge),
                }),
                _ => continue,
            }
            self.debt
                .entry(edge.from.clone())
                .or_default()
                .entry(edge.to.clone())
                .or_default()
                .entry(name)
                .or_insert_with(|| Site::of(edge));
        }
    }

    /// Each test owner's (test file, item) pairs inside other stages.
    /// Files mounted from outside `src` (`tests/support/mod.rs` as
    /// `test_support`) are test code, owned by the module they mount at.
    fn read_tests(
        &mut self,
        crate_map: &CrateMap,
        root: &RootTable,
        stages: &[String],
    ) -> Vec<crate::model::Diagnostic> {
        let mut unavailable = Vec::new();
        let files = crate_map
            .files
            .iter()
            .filter_map(|file| {
                module_path_of(&crate_map.src_rel, &file.rel).map(|module| (module, file))
            })
            .chain(
                crate_map
                    .mounted
                    .iter()
                    .map(|mount| (mount.module.clone(), &mount.file)),
            );
        for (module, file) in files {
            let module: Vec<&str> = module.split("::").collect();
            for (path, lines) in test_source_lines(file, &module) {
                let path: Vec<&str> = path.iter().map(String::as_str).collect();
                let owner = path[0];
                let (edges, unjudged) = root.edges(&file.rel, &path, &lines);
                unavailable.extend(unjudged.into_iter().map(|issue| crate::model::Diagnostic {
                    package: Some(crate_map.name.clone()),
                    file: issue.file,
                    line: Some(issue.line),
                    message: format!(
                        "required test path {} is unjudged: {}",
                        issue.path, issue.why
                    ),
                }));
                for edge in edges {
                    if edge.to == owner
                        || !stages.contains(&edge.to)
                        || self.held.floor.contains(&edge.to)
                    {
                        continue;
                    }
                    let Some(name) = self.interior(owner, &edge, &root.module_paths, |_| true)
                    else {
                        continue;
                    };
                    self.tests
                        .entry(owner.to_owned())
                        .or_default()
                        .entry((file.rel.clone(), format!("{}::{name}", edge.to)))
                        .or_insert_with(|| Site::of(&edge));
                }
            }
        }
        unavailable
    }
}

#[cfg(test)]
mod tests;
