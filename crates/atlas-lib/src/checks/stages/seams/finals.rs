//! `stage_seams.final_stages`: stages no stage reads but the composition,
//! the last stage of the crate's `stage_order`.
//!
//! A read of a final stage is a stage's `stage_reads` row naming it, or an
//! observed production path into it (crate-root re-exports and test code
//! are not stages' reads). Once published, the list and the composition are
//! the policy author's: a landing that adds, drops or removes the list, or
//! moves the composition, refuses unless the candidate's `amended_by`
//! covers `final_stages` and its description cites the bead.
use std::collections::BTreeSet;

use super::super::StageReport;
use super::SeamReading;
use crate::checks::policy::{self, Knob, MissingInput, Policy, StageOrder};

impl SeamReading {
    /// One refusal per (reader, final stage): the first observed site, or
    /// the declaring row when no path is observed.
    pub(super) fn final_read_refusals(&self, report: &StageReport) -> Vec<String> {
        let name = &self.crate_name;
        let (Some(finals), Some(composition)) = (&self.held.final_stages, self.stages.last())
        else {
            return Vec::new();
        };
        let mut refusals = Vec::new();
        for stage in finals {
            for reader in self
                .stages
                .iter()
                .filter(|reader| *reader != stage && *reader != composition)
            {
                let observed = report
                    .crossing
                    .iter()
                    .filter(|edge| &edge.from == reader && &edge.to == stage)
                    .min_by(|a, b| (&a.file, a.line).cmp(&(&b.file, b.line)));
                let declared = report
                    .interfaces
                    .as_ref()
                    .and_then(|rows| rows.get(reader))
                    .is_some_and(|row| row.admits(stage, &[]));
                if let Some(edge) = observed {
                    refusals.push(format!(
                        "stage final `{name}`: {}:{} {reader} reads {stage} (`{}`), a final stage only {composition} reads; move what {reader} needs to a stage before {stage}",
                        edge.file, edge.line, edge.path
                    ));
                } else if declared {
                    refusals.push(format!(
                        "stage final `{name}`: {reader}'s stage_reads row names {stage}, a final stage only {composition} reads; drop it from the row, or move what {reader} needs to a stage before {stage}"
                    ));
                }
            }
        }
        refusals
    }
}

/// The shape of one crate's `final_stages` and `amended_by`: each final
/// stage is a stage, not in the floor, not the composition, listed once;
/// an amendment covers only `final_stages`.
pub(in crate::checks) fn final_stage_shape(
    order: &StageOrder,
    seams: &policy::StageSeams,
) -> Vec<String> {
    let name = &order.crate_name;
    let mut violations = Vec::new();
    let composition = order.stages.last();
    let mut once = BTreeSet::new();
    for stage in seams.final_stages.iter().flatten() {
        let problem = if !order.stages.contains(stage) {
            "is not a stage in its stage_order row"
        } else if seams.floor.contains(stage) {
            "is a floor stage, which every stage reads whole"
        } else if Some(stage) == composition {
            "is the composition (the last stage), which runs the final stages"
        } else if !once.insert(stage) {
            "is listed twice"
        } else {
            continue;
        };
        violations.push(format!(
            "stage final `{name}`: final_stages names `{stage}`, which {problem}"
        ));
    }
    if let Some(amendment) = &seams.amended_by
        && (amendment.covers.is_empty()
            || amendment
                .covers
                .iter()
                .any(|knob| *knob != Knob::FinalStages))
    {
        violations.push(format!(
            "stage final `{name}`: stage_seams amended_by covers only `final_stages`; write covers = [\"final_stages\"]"
        ));
    }
    violations
}

/// The published-list law over a landing, per crate.
pub(in crate::checks) fn final_stage_list_refusals(
    published: &Policy,
    policy: &Policy,
    description: &str,
    citations: Option<&crate::checks::concentration::Citations>,
    unavailable: &mut Vec<MissingInput>,
) -> Vec<String> {
    let crates: BTreeSet<&String> = published
        .stage_seams
        .keys()
        .chain(policy.stage_seams.keys())
        .collect();
    let mut refusals = Vec::new();
    for name in crates {
        let was = published.stage_seams.get(name);
        let now = policy.stage_seams.get(name);
        let changes = list_changes(name, published, policy);
        let amendment = now
            .and_then(|seams| seams.amended_by.as_ref())
            .filter(|amendment| {
                was.and_then(|seams| seams.amended_by.as_ref()) != Some(*amendment)
            });
        if let Some(amendment) = amendment {
            let section = format!("stage_seams.{name}");
            if crate::checks::concentration::known_uncited(
                &section,
                &amendment.bead,
                description,
                citations,
                unavailable,
            ) {
                refusals.push(format!(
                    "stage final `{name}`: amended_by names `{}`, which this landing's description does not cite; cite it as ({})",
                    amendment.bead, amendment.bead
                ));
            }
            if changes.is_empty() {
                refusals.push(format!(
                    "stage final `{name}`: amended_by covers `final_stages`, but this landing does not change the published list or its composition; remove it"
                ));
            }
        }
        let covered =
            amendment.is_some_and(|amendment| amendment.covers.contains(&Knob::FinalStages));
        if !covered {
            refusals.extend(changes.into_iter().map(|change| {
                format!(
                    "stage final `{name}`: {change}; only the policy author changes it: name the ruling with `amended_by = {{ bead = \"<reference>\", covers = [\"final_stages\"] }}` in [stage_seams.{name}] and cite it"
                )
            }));
        }
    }
    refusals
}
/// How the candidate changes a crate's published `final_stages` list and
/// its composition; nothing when the published policy wrote no list.
fn list_changes(name: &str, published: &Policy, policy: &Policy) -> Vec<String> {
    let Some(was) = published
        .stage_seams
        .get(name)
        .and_then(|seams| seams.final_stages.as_ref())
    else {
        return Vec::new();
    };
    let last = |policy: &Policy| {
        policy
            .stage_order
            .iter()
            .find(|order| order.crate_name == name)
            .and_then(|order| order.stages.last().cloned())
    };
    let mut changes = Vec::new();
    match policy
        .stage_seams
        .get(name)
        .and_then(|seams| seams.final_stages.as_ref())
    {
        None => changes.push(
            "the published final_stages list is gone; once written it stays: restore it".to_owned(),
        ),
        Some(now) => {
            for added in now.iter().filter(|stage| !was.contains(stage)) {
                changes.push(format!("{added} is not a published final stage; declaring one is the policy author's decision: drop it"));
            }
            for dropped in was.iter().filter(|stage| !now.contains(stage)) {
                changes.push(format!("the published final stage {dropped} is gone; only the policy author undeclares one: restore it"));
            }
        }
    }
    let (before, after) = (last(published), last(policy));
    if before != after {
        changes.push(format!(
            "the composition moves from {} to {} while final_stages is published, so a reader could become the composition; keep {} last",
            before.as_deref().unwrap_or("none"),
            after.as_deref().unwrap_or("none"),
            before.as_deref().unwrap_or("none"),
        ));
    }
    changes
}
