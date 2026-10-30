//! What `atlas seams` prints and what `atlas delta` reports of a reading.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value, json};

use super::SeamReading;

/// Four native seam units; each counts distinct subjects in its own population.
#[derive(serde::Serialize)]
pub struct Totals {
    pub debt_items: usize,
    pub unlisted_exports: usize,
    pub writer_sites: usize,
    pub test_pairs: usize,
}

impl SeamReading {
    /// Boundary names listed and unlisted since `base`, as `stage::name`:
    /// how the boundary grew or shrank, for the landing record.
    pub fn listing_changes(&self, base: Option<&Self>) -> (Vec<String>, Vec<String>) {
        let empty = BTreeSet::new();
        let mut added = Vec::new();
        let mut removed = Vec::new();
        let stages: BTreeSet<&String> = self
            .boundaries
            .keys()
            .chain(base.into_iter().flat_map(|base| base.boundaries.keys()))
            .collect();
        for stage in stages {
            let now = self.boundaries.get(stage).map_or(&empty, |b| &b.listed);
            let was = base
                .and_then(|base| base.boundaries.get(stage))
                .map_or(&empty, |b| &b.listed);
            added.extend(now.difference(was).map(|item| format!("{stage}::{item}")));
            removed.extend(was.difference(now).map(|item| format!("{stage}::{item}")));
        }
        (added, removed)
    }

    /// The totals delta's `seams` line prints.
    pub fn totals(&self) -> Value {
        json!(self.counts())
    }
    /// Typed totals used by architecture judgments before presentation.
    pub fn counts(&self) -> Totals {
        Totals {
            debt_items: self
                .debt
                .values()
                .flat_map(BTreeMap::values)
                .map(BTreeMap::len)
                .sum(),
            unlisted_exports: self
                .boundaries
                .values()
                .map(|boundary| boundary.unlisted.len())
                .sum(),
            writer_sites: self
                .writer_census
                .values()
                .flat_map(|census| census.calls.values())
                .map(Vec::len)
                .sum(),
            test_pairs: self.tests.values().map(BTreeMap::len).sum(),
        }
    }

    pub fn json(&self) -> Value {
        let debt: BTreeMap<&String, BTreeMap<&String, Vec<&String>>> = self
            .debt
            .iter()
            .map(|(reader, reads)| {
                (
                    reader,
                    reads
                        .iter()
                        .map(|(read, items)| (read, items.keys().collect()))
                        .collect(),
                )
            })
            .collect();
        let tests: BTreeMap<&String, usize> = self
            .tests
            .iter()
            .map(|(owner, pairs)| (owner, pairs.len()))
            .collect();
        let boundaries: BTreeMap<&String, Value> = self
            .boundaries
            .iter()
            .map(|(stage, boundary)| {
                (
                    stage,
                    json!({
                        "listed": boundary.listed,
                        "unlisted": boundary.unlisted.keys().collect::<Vec<_>>(),
                    }),
                )
            })
            .collect();
        let writers: BTreeMap<&String, Value> = self
            .writer_census
            .iter()
            .map(|(ty, census)| {
                let sites: BTreeMap<&String, usize> = census
                    .calls
                    .iter()
                    .map(|(owner, calls)| (owner, calls.len()))
                    .collect();
                (
                    ty,
                    json!({ "methods": census.methods, "aliases": census.aliases, "sites": sites }),
                )
            })
            .collect();
        json!({
            "crate": self.crate_name,
            "debt": debt,
            "boundaries": boundaries,
            "writers": writers,
            "test_coupling": tests,
        })
    }
}
