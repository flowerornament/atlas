//! One paired-capture architecture judgment; Git identity and local authority belong to the host.
mod ceilings;
mod coupling;
pub mod measure;
mod spellings;
use super::{
    concentration,
    policy::{self, MissingInput, Policy, Status},
    stages::seams::BoundaryDeclaration,
};
use crate::{Result, analysis::Analysis, discovery::Snapshot};
use serde::Serialize;
use std::collections::BTreeMap;

/// Exact identities resolved by the host, never inferred from source similarity.
#[derive(Clone, Debug, Serialize)]
pub struct Basis {
    pub baseline: String,
    pub candidate: String,
}
/// A raise is the policy's: re-exported here, where it was first defined.
pub use super::policy::Raise;
#[derive(Default, Serialize)]
#[non_exhaustive]
pub struct Boundaries {
    pub before: BTreeMap<String, BTreeMap<String, BoundaryDeclaration>>,
    pub after: BTreeMap<String, BTreeMap<String, BoundaryDeclaration>>,
}
/// Project observations accompany the actual candidate capture and description.
#[derive(Serialize)]
#[non_exhaustive]
pub struct Inputs {
    pub basis: Basis,
    pub description: String,
    pub policy: policy::Inputs,
    /// The base capture's observations, for the laws that count both sides
    /// (`forbid_new_*`): fill them from the base snapshot under the
    /// published policy.
    pub before_policy: policy::Inputs,
    pub concentration: concentration::Inputs,
    pub boundaries: Boundaries,
}
impl Inputs {
    pub fn new(after: &Snapshot, basis: Basis, description: String) -> Self {
        Self {
            basis,
            description,
            policy: policy::Inputs::new(after),
            before_policy: policy::Inputs::new(after),
            concentration: concentration::Inputs::default(),
            boundaries: Boundaries::default(),
        }
    }
}
#[derive(Serialize)]
#[non_exhaustive]
pub struct Measurements {
    pub before: measure::Measures,
    pub after: measure::Measures,
    pub shape: crate::review::ShapeDiff,
}
#[derive(Serialize)]
#[non_exhaustive]
pub struct SeamDelta {
    pub crate_name: String,
    pub totals: super::stages::seams::Totals,
    pub listed: Vec<String>,
    pub unlisted: Vec<String>,
    pub floor_added: Vec<String>,
}
/// One authority's held spelling debt across the change.
#[derive(Debug, Default, Serialize)]
#[non_exhaustive]
pub struct SpellingSummary {
    pub debt_before: usize,
    pub debt_after: usize,
    /// Occurrences left out on the candidate side (test code, literals).
    pub dropped: usize,
    /// Candidate files with macro invocations: a pattern cannot see a
    /// spelling inside a macro's input, so these may hold debt it misses
    /// (a wrapper supplies such spellings as an observed class).
    pub files_with_macros: usize,
}
/// One core crate's size across the change, with `[size] approve_growth`:
/// token lines in the base (none when the base has no such crate) and in
/// the candidate.
#[derive(Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct SizeChange {
    #[serde(rename = "crate")]
    pub crate_name: String,
    pub before: Option<usize>,
    pub after: usize,
}
#[derive(Serialize)]
#[non_exhaustive]
pub struct Report {
    pub basis: Basis,
    pub measurements: Option<Measurements>,
    pub policy: policy::Report,
    /// Each raise for the host to admit: a ceiling raised, or with
    /// `approve_growth`, a core crate that grew.
    pub raises: Vec<Raise>,
    /// With `approve_growth`: every core crate's size before and after,
    /// grown or not, so the host can check what it approves.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub size_changes: Vec<SizeChange>,
    /// Per authority with `forbid_new_*`: its held debt on each side, and
    /// the assumption it rests on.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub spellings: BTreeMap<String, SpellingSummary>,
    pub coupling_violations: Vec<String>,
    pub seams: Vec<SeamDelta>,
    pub concentration: Option<concentration::Summary>,
    pub violations: Vec<String>,
    pub unavailable: Vec<MissingInput>,
}
impl Report {
    pub fn status(&self) -> Status {
        if !self.unavailable.is_empty() {
            Status::Unavailable
        } else if !self.violations.is_empty() {
            Status::Refuse
        } else {
            Status::Pass
        }
    }
}
/// Capture owners supply source and identity; this function measures, judges, then returns data.
/// A caller must not project PASS until this complete result is available.
pub fn check(
    before: &Snapshot,
    after: &Snapshot,
    published: &Policy,
    policy: &Policy,
    inputs: &Inputs,
) -> Result<Report> {
    let analysis = Analysis::new(after)?;
    let policy_reading = policy::validate(&analysis, policy, &inputs.policy)?;
    let mut out = Report {
        basis: inputs.basis.clone(),
        measurements: None,
        violations: policy_reading.violations.clone(),
        unavailable: policy_reading
            .unavailable
            .iter()
            .map(|issue| MissingInput {
                field: issue.field.clone(),
                reason: issue.reason.clone(),
            })
            .collect(),
        policy: policy_reading,
        raises: Vec::new(),
        size_changes: Vec::new(),
        spellings: BTreeMap::new(),
        coupling_violations: Vec::new(),
        seams: Vec::new(),
        concentration: None,
    };
    let members = after
        .metadata()
        .workspace_packages()
        .into_iter()
        .map(|package| package.name.as_str())
        .collect();
    if policy.validate_configuration(&members).is_err() {
        return Ok(out);
    }

    let before_workspace = super::Workspace::of(before)?;
    let after_workspace = super::Workspace::of(after)?;
    match paired_measures(before, after, &before_workspace, &after_workspace) {
        Ok(measured) => judge_measures(published, policy, measured, &mut out),
        Err(error) => out.unavailable.push(MissingInput {
            field: "delta.measurements".into(),
            reason: error.to_string(),
        }),
    }
    match stage_judgment(
        before,
        after,
        published,
        policy,
        &inputs.boundaries,
        &before_workspace,
        &after_workspace,
    ) {
        Ok((violations, unavailable, seams)) => {
            out.violations.extend(violations);
            out.unavailable.extend(unavailable);
            out.seams = seams;
        }
        Err(error) => out.unavailable.push(MissingInput {
            field: "delta.stages".into(),
            reason: error.to_string(),
        }),
    }
    judge_landing(
        (published, policy),
        (before, after),
        (&before_workspace, &after_workspace),
        inputs,
        &mut out,
    );
    Ok(out)
}
/// The laws over the paired measurements: ceilings and their raises, size
/// growth for the host to approve, and the crate graph.
fn judge_measures(published: &Policy, policy: &Policy, measured: Measurements, out: &mut Report) {
    let size = token_lines(&measured.after);
    let surface = measured
        .after
        .crates
        .iter()
        .map(|(name, count)| (name.clone(), count.surface))
        .collect();
    out.violations
        .extend(ceilings::ceiling_violations(policy, &size, &surface));
    let (raises, refusals) = ceilings::ceiling_changes(published, policy, &size, &surface);
    out.raises = raises;
    if policy.size.approve_growth {
        let (changes, raises) =
            ceilings::size_growth(policy, &token_lines(&measured.before), &size);
        out.size_changes = changes;
        out.raises.extend(raises);
    }
    out.violations.extend(refusals);
    let members = measured.after.crates.keys().cloned().collect();
    out.coupling_violations = policy
        .crate_graph
        .violations(&members, &measured.after.dependencies);
    // `policy` judged the same graph; each row is reported once.
    for row in &out.coupling_violations {
        if !out.violations.contains(row) {
            out.violations.push(row.clone());
        }
    }
    out.measurements = Some(measured);
}
fn token_lines(measures: &measure::Measures) -> BTreeMap<String, usize> {
    measures
        .crates
        .iter()
        .map(|(name, count)| (name.clone(), count.token_lines))
        .collect()
}
/// The laws over the landing that read both trees as code: the population
/// and concentration laws, and the held spelling debt of `forbid_new_*`.
fn judge_landing(
    (published, policy): (&Policy, &Policy),
    (before, after): (&Snapshot, &Snapshot),
    workspaces: (&super::Workspace, &super::Workspace),
    inputs: &Inputs,
    out: &mut Report,
) {
    judge_concentration(published, policy, workspaces, inputs, out);
    let side = |name, snapshot, inputs| spellings::Side {
        name,
        snapshot,
        inputs,
    };
    spellings::judge(
        published,
        policy,
        &side("before", before, &inputs.before_policy),
        &side("after", after, &inputs.policy),
        out,
    );
}
/// The core population's loosening law, and the concentration section's
/// landing over the two trees, its function raises included.
fn judge_concentration(
    published: &Policy,
    policy: &Policy,
    (before, after): (&super::Workspace, &super::Workspace),
    inputs: &Inputs,
    out: &mut Report,
) {
    out.violations.extend(ceilings::population_changes(
        published,
        policy,
        &inputs.description,
        inputs.concentration.citations.as_ref(),
        &mut out.unavailable,
    ));
    if let Some(concentration) = &policy.concentration {
        let held = concentration::landing_workspaces(
            concentration,
            published.concentration.as_ref(),
            before,
            after,
            &inputs.concentration,
            &inputs.description,
        );
        out.violations.extend(held.violations);
        out.unavailable.extend(held.unavailable);
        out.raises.extend(held.raises);
        out.concentration = held.report;
    }
}
fn paired_measures(
    before: &Snapshot,
    after: &Snapshot,
    before_workspace: &super::Workspace,
    after_workspace: &super::Workspace,
) -> Result<Measurements> {
    let old = measure::read_workspace(before, before_workspace)?;
    let new = measure::read_workspace(after, after_workspace)?;
    let mut shape = crate::review::diff(before, after, ".")?;
    shape.changes.retain(|change| {
        old.sources.contains_key(&change.file) || new.sources.contains_key(&change.file)
    });
    Ok(Measurements {
        before: old,
        after: new,
        shape,
    })
}
#[expect(
    clippy::too_many_lines,
    reason = "longer than 70 lines; to be split along its decisions"
)]
fn stage_judgment(
    before: &Snapshot,
    after: &Snapshot,
    published: &Policy,
    policy: &Policy,
    inputs: &Boundaries,
    before_workspace: &super::Workspace,
    after_workspace: &super::Workspace,
) -> Result<(Vec<String>, Vec<MissingInput>, Vec<SeamDelta>)> {
    let (stages, roots) = super::read_stages(after_workspace, policy, None)?;
    let (crates, unavailable) = super::seam_readings(
        after,
        after_workspace,
        policy,
        &inputs.after,
        None,
        &stages,
        &roots,
    )?;
    let now = super::SeamReports {
        stages,
        crates,
        unavailable,
    };
    let mut violations = now.stages.policy_violations;
    for (name, report) in &now.stages.crates {
        violations.extend(report.refusals(name));
    }
    let mut unavailable = now
        .unavailable
        .iter()
        .chain(&now.stages.unavailable)
        .map(|issue| MissingInput {
            field: format!("delta.stages.{}", issue.file),
            reason: issue.message.clone(),
        })
        .collect::<Vec<_>>();
    let mut base_policy = published.clone();
    base_policy.stage_seams = policy
        .stage_seams
        .iter()
        .filter(|(name, _)| {
            published
                .stage_order
                .iter()
                .any(|order| &order.crate_name == *name)
        })
        .map(|(name, seams)| (name.clone(), seams.clone()))
        .collect();
    let (base_stages, base_roots) = super::read_stages(before_workspace, &base_policy, None)?;
    let (base_crates, base_unavailable) = super::seam_readings(
        before,
        before_workspace,
        &base_policy,
        &inputs.before,
        None,
        &base_stages,
        &base_roots,
    )?;
    let base = super::SeamReports {
        stages: base_stages,
        crates: base_crates,
        unavailable: base_unavailable,
    };
    unavailable.extend(
        base.unavailable
            .iter()
            .chain(&base.stages.unavailable)
            .map(|issue| MissingInput {
                field: format!("delta.base_stages.{}", issue.file),
                reason: issue.message.clone(),
            }),
    );
    let published = policy::PublishedStages::from(published);
    let mut summary = Vec::new();
    for (name, reading) in now.crates {
        let based = base.crates.get(&name);
        let report = now
            .stages
            .crates
            .get(&name)
            .ok_or("seam reading lacks its stage report")?;
        violations.extend(reading.refusals(based, report, &published));
        let (listed, unlisted) = reading.listing_changes(based);
        let was = published
            .seams
            .get(&name)
            .map(|row| row.floor.as_slice())
            .unwrap_or_default();
        let floor_added = policy.stage_seams[&name]
            .floor
            .iter()
            .filter(|stage| !was.contains(stage))
            .cloned()
            .collect();
        summary.push(SeamDelta {
            crate_name: name,
            totals: reading.counts(),
            listed,
            unlisted,
            floor_added,
        });
    }
    Ok((violations, unavailable, summary))
}

#[cfg(test)]
mod tests;
