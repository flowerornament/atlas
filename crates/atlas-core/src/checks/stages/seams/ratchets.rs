//! The seam ratchets `atlas delta` applies: each refusal names its fix.
//!
//! Counts are held against the candidate policy's rows and against the
//! published policy (a recorded count only falls). New names are held
//! against the same reading of the landing's base tree, whatever the counts.

use std::collections::BTreeSet;

use super::super::StageReport;
use super::{SeamReading, UnownedRead};
use crate::checks::policy;

impl SeamReading {
    /// Every breach of the seam ratchets. `base` is the same reading of the
    /// landing's base tree, when the crate has one; `published` is the base's
    /// authored policy.
    pub fn refusals(
        &self,
        base: Option<&Self>,
        report: &StageReport,
        published: &policy::PublishedStages,
    ) -> Vec<String> {
        let published_seams = published.seams.get(&self.crate_name);
        let mut refusals = self.debt_refusals(base, report, published.reads.get(&self.crate_name));
        refusals.extend(self.export_refusals(base, published_seams));
        refusals.extend(self.writer_refusals(base, published_seams));
        refusals.extend(self.policy_refusals(published));
        refusals.extend(self.test_refusals(base, published_seams));
        refusals
    }

    /// Debt rows: an input's interior read with no row, a row above its
    /// count, a name its (reader, read) row did not read in the base, and a
    /// recorded count above the published one.
    fn debt_refusals(
        &self,
        base: Option<&Self>,
        report: &StageReport,
        published: Option<&std::collections::BTreeMap<String, policy::StageReads>>,
    ) -> Vec<String> {
        let name = &self.crate_name;
        let mut refusals: Vec<String> = self
            .unowned
            .iter()
            .map(|UnownedRead { reader, read, item, site }| {
                format!(
                    "stage seam `{name}`: {}:{} {reader} reads `{item}` (`{}`) inside its input {read}, past {read}'s boundary; read a boundary item, or list `{item}` in {read}'s //! Boundary paragraph if it is part of the FM object",
                    site.file, site.line, site.path
                )
            })
            .collect();
        let rows = report.interfaces.as_ref();
        for (reader, reads) in &self.debt {
            for (read, items) in reads {
                let Some(row) = rows
                    .and_then(|rows| rows.get(reader))
                    .and_then(|declared| declared.debt.row(read))
                else {
                    continue;
                };
                if items.len() > row.items {
                    refusals.push(format!(
                        "stage seam `{name}`: {reader} reads {} items of {read} past its boundary, above the row's {}; read {read} through its boundary, or retire a read (`atlas seams {name}` lists them)",
                        items.len(),
                        row.items,
                    ));
                }
                let Some(base) = base.filter(|base| base.present.contains(reader)) else {
                    continue;
                };
                let known: BTreeSet<&String> = base
                    .debt
                    .get(reader)
                    .and_then(|reads| reads.get(read))
                    .into_iter()
                    .flat_map(|items| items.keys())
                    .collect();
                for (item, site) in items.iter().filter(|(item, _)| !known.contains(item)) {
                    refusals.push(format!(
                        "stage seam `{name}`: {}:{} {reader} newly reads `{item}` of {read} (`{}`) past its boundary; a debt row only shrinks: read {read}'s boundary instead",
                        site.file, site.line, site.path
                    ));
                }
            }
        }
        for (reader, declared) in rows.into_iter().flatten() {
            let policy::StageDebt::Rows(debt) = &declared.debt else {
                continue;
            };
            for (read, row) in debt {
                let was = published
                    .and_then(|rows| rows.get(reader))
                    .and_then(|row| row.debt.row(read));
                if let Some(was) = was
                    && row.items > was.items
                {
                    refusals.push(format!(
                        "stage seam `{name}`: the debt row {reader} → {read} rose from {} to {} items; a debt row only goes down",
                        was.items, row.items
                    ));
                }
            }
        }
        refusals
    }

    /// Root exports: a wildcard re-export, an item-level macro invocation, a
    /// citation that does not resolve, unlisted exports above the row, an
    /// unlisted export the base root did not export, and a recorded count
    /// above the published one.
    fn export_refusals(
        &self,
        base: Option<&Self>,
        published: Option<&policy::StageSeams>,
    ) -> Vec<String> {
        let name = &self.crate_name;
        let mut refusals = Vec::new();
        for (stage, boundary) in &self.boundaries {
            let file = &boundary.file;
            refusals.extend(boundary.wildcards.iter().map(|line| {
                format!(
                    "stage boundary `{name}`: {file}:{line} {stage}'s root re-exports a wildcard; export each boundary item by name and list it in the //! Boundary paragraph"
                )
            }));
            refusals.extend(boundary.foreign.iter().map(|(line, listed, home)| {
                format!(
                    "stage boundary `{name}`: {file}:{line} {stage}'s //! Boundary paragraph lists `{listed}`, whose item is `{home}` in another stage; a stage lists only its own items"
                )
            }));
            refusals.extend(boundary.macro_items.iter().map(|(line, invoked)| {
                format!(
                    "stage boundary `{name}`: {file}:{line} {stage}'s root invokes `{invoked}!` at item level, and what it generates escapes the export census; invoke it in a child module and re-export its items by name"
                )
            }));
            refusals.extend(boundary.citation_problems.iter().map(|problem| {
                format!(
                    "stage boundary `{name}`: {file} {stage}'s //! Boundary paragraph does not resolve its citation: {problem}; resolve the citation with the project's boundary authority"
                )
            }));
            let held = self.held.exports.get(stage).map_or(0, |row| row.items);
            if boundary.unlisted.len() > held {
                refusals.push(format!(
                    "stage boundary `{name}`: {stage}'s root exports {} items its //! Boundary paragraph does not list, above the recorded {held}; list an FM object (with its citation) or make the export private",
                    boundary.unlisted.len()
                ));
            }
            // A base root that invoked a macro at item level exported what
            // the scan could not see, so its names cannot be judged new.
            if let Some(based) = base
                .and_then(|base| base.boundaries.get(stage))
                .filter(|based| based.macro_items.is_empty())
            {
                for (item, line) in boundary
                    .unlisted
                    .iter()
                    .filter(|(item, _)| !based.exported.contains(*item))
                {
                    refusals.push(format!(
                        "stage boundary `{name}`: {file}:{line} {stage}'s root newly exports `{item}`, which its //! Boundary paragraph does not list; an export enters the boundary only with its FM citation in that list"
                    ));
                }
            }
        }
        for (stage, row) in &self.held.exports {
            if let Some(was) = published.and_then(|seams| seams.exports.get(stage))
                && row.items > was.items
            {
                refusals.push(format!(
                    "stage boundary `{name}`: {stage}'s recorded unlisted exports rose from {} to {}; the count only goes down",
                    was.items, row.items
                ));
            }
        }
        refusals
    }

    /// Writers: a type its stage does not declare (or, outside the floor,
    /// does not list), calls above an owner's recorded sites, named by the
    /// calls the base lacks, and recorded sites above the published ones.
    fn writer_refusals(
        &self,
        base: Option<&Self>,
        published: Option<&policy::StageSeams>,
    ) -> Vec<String> {
        let name = &self.crate_name;
        let mut refusals = Vec::new();
        for (ty, row) in &self.held.writers {
            let census = self.writer_census.get(ty);
            if !census.is_some_and(|census| census.declared) {
                refusals.push(format!(
                    "stage writers `{name}`: the writers row for `{ty}` names {}, which declares no `{ty}`; name the stage that declares it",
                    row.stage
                ));
            }
            if !self.held.floor.contains(&row.stage)
                && !self
                    .boundaries
                    .get(&row.stage)
                    .is_some_and(|boundary| boundary.listed.contains(ty))
            {
                refusals.push(format!(
                    "stage writers `{name}`: the writers row for `{ty}` names {}, whose //! Boundary paragraph does not list it; list it there or drop the row",
                    row.stage
                ));
            }
            let based = base.and_then(|base| base.writer_census.get(ty));
            for (owner, calls) in census.into_iter().flat_map(|census| &census.calls) {
                let held = row.sites.get(owner).copied().unwrap_or(0);
                if calls.len() <= held {
                    continue;
                }
                let known: BTreeSet<(&str, &str)> = based
                    .and_then(|based| based.calls.get(owner))
                    .into_iter()
                    .flatten()
                    .map(|call| (call.site.file.as_str(), call.code.as_str()))
                    .collect();
                let fresh: Vec<String> = calls
                    .iter()
                    .filter(|call| !known.contains(&(call.site.file.as_str(), call.code.as_str())))
                    .map(|call| {
                        format!("{}:{} `{}`", call.site.file, call.site.line, call.site.path)
                    })
                    .collect();
                let shown = if fresh.is_empty() {
                    format!("`atlas seams {name} --items` lists them")
                } else {
                    format!("new: {}", fresh.join(", "))
                };
                refusals.push(format!(
                    "stage writers `{name}`: {owner} builds or changes `{ty}` at {} sites, above the recorded {held} ({shown}); only {} write it: {}",
                    calls.len(),
                    row.producers.join(", "),
                    row.reason
                ));
            }
            let was = published.and_then(|seams| seams.writers.get(ty));
            for (owner, held) in &row.sites {
                if let Some(was) = was.and_then(|was| was.sites.get(owner))
                    && held > was
                {
                    refusals.push(format!(
                        "stage writers `{name}`: {owner}'s recorded `{ty}` writer sites rose from {was} to {held}; the count only goes down"
                    ));
                }
            }
        }
        refusals
    }

    /// The seam policy against the published one: an existing stage moving
    /// into the floor, and a writers row removed, moved to another stage,
    /// narrowed (a method or function dropped) or widened (a producer added).
    /// A new floor stage, a new row, an added method and a dropped producer
    /// tighten and pass.
    fn policy_refusals(&self, published: &policy::PublishedStages) -> Vec<String> {
        let name = &self.crate_name;
        let Some(was) = published.seams.get(name) else {
            return Vec::new();
        };
        let mut refusals = Vec::new();
        let staged = published
            .orders
            .iter()
            .find(|order| &order.crate_name == name)
            .map(|order| order.stages.as_slice())
            .unwrap_or_default();
        for stage in self
            .held
            .floor
            .iter()
            .filter(|stage| !was.floor.contains(stage) && staged.contains(stage))
        {
            let listed = self
                .floor_boundaries
                .get(stage)
                .map(|boundary| {
                    boundary
                        .listed
                        .iter()
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_default();
            refusals.push(format!(
                "stage seams `{name}`: {stage} moves into the floor, so every stage would read it whole (its boundary lists: {listed}); the floor grows only by a new stage, never by an existing one"
            ));
        }
        for (ty, before) in &was.writers {
            let Some(now) = self.held.writers.get(ty) else {
                refusals.push(format!(
                    "stage writers `{name}`: the published writers row for `{ty}` is gone; a row leaves only with the listing it guards"
                ));
                continue;
            };
            let mut changes = Vec::new();
            if now.stage != before.stage {
                changes.push(format!(
                    "its stage moved from {} to {}",
                    before.stage, now.stage
                ));
            }
            let dropped: Vec<&String> = before
                .methods
                .iter()
                .chain(&before.functions)
                .filter(|method| !now.methods.contains(method) && !now.functions.contains(method))
                .collect();
            if !dropped.is_empty() {
                changes.push(format!(
                    "it no longer counts {}",
                    dropped
                        .iter()
                        .map(|method| method.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            let added: Vec<&String> = now
                .producers
                .iter()
                .filter(|producer| !before.producers.contains(producer))
                .collect();
            if !added.is_empty() {
                changes.push(format!(
                    "it adds producers {}",
                    added
                        .iter()
                        .map(|producer| producer.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            if !changes.is_empty() {
                refusals.push(format!(
                    "stage writers `{name}`: the writers row for `{ty}` loosens: {}; a writers row only tightens",
                    changes.join("; ")
                ));
            }
        }
        refusals
    }

    /// Test coupling: an owner above its recorded pairs, an item the owner's
    /// tests did not name in the base (keyed by item, so a file move within
    /// the owner passes), and a recorded count above the published one.
    fn test_refusals(
        &self,
        base: Option<&Self>,
        published: Option<&policy::StageSeams>,
    ) -> Vec<String> {
        let name = &self.crate_name;
        let mut refusals = Vec::new();
        for (owner, pairs) in &self.tests {
            let held = self
                .held
                .test_coupling
                .get(owner)
                .map_or(0, |row| row.pairs);
            if pairs.len() > held {
                refusals.push(format!(
                    "test coupling `{name}`: {owner}'s tests name {} (test file, item) pairs inside other stages, above the recorded {held}; test through the stages' boundary types, or retire a coupled test (`atlas seams {name} --items` lists them)",
                    pairs.len(),
                ));
            }
            let Some(base) = base.filter(|base| base.present.contains(owner)) else {
                continue;
            };
            let known: BTreeSet<&String> = base
                .tests
                .get(owner)
                .into_iter()
                .flat_map(|pairs| pairs.keys().map(|(_, item)| item))
                .collect();
            let mut fresh = BTreeSet::new();
            for ((_, item), site) in pairs {
                if !known.contains(item) && fresh.insert(item) {
                    refusals.push(format!(
                        "test coupling `{name}`: {}:{} {owner}'s tests newly name `{item}` (`{}`) inside another stage; test through its boundary types",
                        site.file, site.line, site.path
                    ));
                }
            }
        }
        for (owner, row) in &self.held.test_coupling {
            if let Some(was) = published
                .and_then(|seams| seams.test_coupling.get(owner))
                .map(|row| row.pairs)
                && row.pairs > was
            {
                refusals.push(format!(
                    "test coupling `{name}`: {owner}'s recorded count rose from {was} to {}; the count only goes down",
                    row.pairs
                ));
            }
        }
        refusals
    }
}
