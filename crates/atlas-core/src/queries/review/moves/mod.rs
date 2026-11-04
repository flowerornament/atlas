//! Source evidence for moves: native lexical keys, written visibility and normalized tokens.
//! Matching spans whole crates before report selection. No report proves equivalent behavior.
mod compare;
mod inventory;
mod scope;
#[cfg(test)]
mod tests;
mod tokens;

use crate::{Result, source::discovery::Snapshot};
use compare::{BodyReport, Change, Direction, ambiguities, visibility_changes};
use inventory::{Key, Site, inventory};
use scope::Scope;
use serde::Serialize;
use std::collections::BTreeMap;

/// Written visibility changes, matched by crate/class/name; repeated names pair approximately.
/// This does not calculate Rust's effective visibility or prove a move safe.
pub fn visibility(
    before: &Snapshot,
    after: &Snapshot,
    scope: Option<&str>,
) -> Result<VisibilityReport> {
    let scope = Scope::of(before, after, scope)?;
    let crates = scope.crates(before, after);
    let matched = |name: &str| crates.contains(name);
    let old = inventory(before, &matched, false)?;
    let new = inventory(after, &matched, false)?;
    let changes = visibility_changes(&old, &new);
    let rows = |direction| {
        changes
            .iter()
            .filter(|change| {
                scope.admits(change.key, change.after)
                    && Direction::of(change.before, change.after) == direction
            })
            .map(Pair::from)
            .collect()
    };
    let ambiguous = ambiguities(&old, &new, &scope);
    Ok(VisibilityReport {
        schema: 1,
        scope: scope.selection().map(str::to_owned),
        population: crate::population::TestSelection::OutsideTestModules,
        basis: Basis::of(before, after, &matched),
        widened: rows(Direction::Widened),
        rescoped: rows(Direction::Rescoped),
        narrowed: rows(Direction::Narrowed),
        ambiguous_keys: ambiguous.len(),
        ambiguous,
        limits: vec![
            "Written visibility ladder, not effective Rust visibility; module-relative scopes are not resolved.",
            "Repeated keys pair approximately; equal visibility occurrences cancel before remaining pairs.",
            "Native field inventory covers structs, not unions; union field visibility is not measured.",
        ],
    })
}
/// Compare native normalized token readings. Scope narrows reported sites after whole-crate matching.
/// Exact ignores docs, written visibility and layout. Rerooted also forgives selected path qualifiers.
/// Neither reading establishes semantic equivalence; unreadable spans remain explicit.
pub fn moved(before: &Snapshot, after: &Snapshot, scope: Option<&str>) -> Result<MoveReport> {
    let scope = Scope::of(before, after, scope)?;
    let crates = scope.crates(before, after);
    let matched = |name: &str| crates.contains(name);
    let old = inventory(before, &matched, true)?;
    let new = inventory(after, &matched, true)?;
    let report = BodyReport::between(&old, &new, &scope);
    Ok(MoveReport {
        schema: 1,
        scope: scope.selection().map(str::to_owned),
        population: crate::population::TestSelection::OutsideTestModules,
        basis: Basis::of(before, after, &matched),
        exact: report.exact,
        relocated: report.relocated,
        rerooted: report.rerooted.iter().map(Pair::from).collect(),
        changed: report.changed.iter().map(Pair::from).collect(),
        only_before: report.only_before.into_iter().map(Single::from).collect(),
        only_after: report.only_after.into_iter().map(Single::from).collect(),
        unreadable: report.unreadable.into_iter().map(Single::from).collect(),
        limits: vec![
            "Native lexical keys are crate/class/name, including inline-module and impl-owner names; duplicate keys remain approximate.",
            "Exact forgives docs, visibility and layout, including closure-brace/trailing-comma normalization outside parentheses and macro input.",
            "Rerooting forgives crate/super/self and declared or imported module paths; different destinations can reroot equal, so review these rows.",
            "No macro expansion, type resolution or behavior-equivalence claim; macro input tokens are preserved without normalization.",
            "Modules and fields are not separate body comparisons; use rows report named-binding retargeting only; glob retargets are not measured.",
        ],
    })
}

/// Fingerprints and discovery limits for every file in the matching population.
#[derive(Debug, Serialize)]
pub struct SnapshotBasis {
    pub sources: BTreeMap<String, String>,
    /// Workspace-wide discovery diagnostics, including exclusions without a captured file.
    pub diagnostics: Vec<crate::model::Diagnostic>,
}
#[derive(Debug, Serialize)]
pub struct Basis {
    pub before: SnapshotBasis,
    pub after: SnapshotBasis,
}
impl Basis {
    fn of(before: &Snapshot, after: &Snapshot, matched: &dyn Fn(&str) -> bool) -> Self {
        let side = |snapshot: &Snapshot| {
            let sources: BTreeMap<_, _> = snapshot
                .graph()
                .files
                .iter()
                .filter(|file| matched(&file.crate_name))
                .map(|file| (file.path.clone(), file.content_fingerprint.clone()))
                .collect();
            // Exclusions can have no captured file (for example a symlink).
            // Retain workspace diagnostics rather than silently dropping that evidence.
            let diagnostics = snapshot.graph().diagnostics.clone();
            SnapshotBasis {
                sources,
                diagnostics,
            }
        };
        Self {
            before: side(before),
            after: side(after),
        }
    }
}
#[derive(Debug, Serialize)]
pub struct Location {
    pub file: String,
    pub line: usize,
    pub visibility: String,
}
impl From<&Site> for Location {
    fn from(site: &Site) -> Self {
        Self {
            file: site.file.clone(),
            line: site.line,
            visibility: site.visibility_text(),
        }
    }
}
#[derive(Debug, Serialize)]
pub struct Pair {
    pub crate_name: String,
    pub class: &'static str,
    pub name: String,
    pub before: Location,
    pub after: Location,
    pub ambiguous: bool,
}
impl From<&Change<'_>> for Pair {
    fn from(change: &Change<'_>) -> Self {
        Self {
            crate_name: change.key.0.clone(),
            class: change.key.1,
            name: change.key.2.clone(),
            before: change.before.into(),
            after: change.after.into(),
            ambiguous: change.ambiguous,
        }
    }
}
#[derive(Debug, Serialize)]
pub struct Single {
    pub crate_name: String,
    pub class: &'static str,
    pub name: String,
    pub site: Location,
}
impl From<(&Key, &Site)> for Single {
    fn from((key, site): (&Key, &Site)) -> Self {
        Self {
            crate_name: key.0.clone(),
            class: key.1,
            name: key.2.clone(),
            site: site.into(),
        }
    }
}
/// Every candidate site for a repeated native key; pairing is not a unique resolution.
#[derive(Debug, Serialize)]
pub struct Ambiguity {
    pub crate_name: String,
    pub class: &'static str,
    pub name: String,
    pub before: Vec<Location>,
    pub after: Vec<Location>,
}
#[derive(Debug, Serialize)]
pub struct VisibilityReport {
    pub schema: u32,
    pub scope: Option<String>,
    pub population: crate::population::TestSelection,
    pub basis: Basis,
    pub widened: Vec<Pair>,
    pub rescoped: Vec<Pair>,
    pub narrowed: Vec<Pair>,
    pub ambiguous_keys: usize,
    pub ambiguous: Vec<Ambiguity>,
    pub limits: Vec<&'static str>,
}
#[derive(Debug, Serialize)]
pub struct MoveReport {
    pub schema: u32,
    pub scope: Option<String>,
    pub population: crate::population::TestSelection,
    pub basis: Basis,
    pub exact: usize,
    pub relocated: usize,
    pub rerooted: Vec<Pair>,
    pub changed: Vec<Pair>,
    pub only_before: Vec<Single>,
    pub only_after: Vec<Single>,
    pub unreadable: Vec<Single>,
    pub limits: Vec<&'static str>,
}
