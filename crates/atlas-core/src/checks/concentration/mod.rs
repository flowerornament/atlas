//! Measured architecture debt over captured source; project citation rules stay outside core.
mod measure;
mod ratchets;
use super::{
    Workspace,
    policy::{Alias, ConcentrationPolicy, LineRow, MissingInput, Status},
};
use crate::{
    Result,
    discovery::Snapshot,
    documentation,
    population::{Population, TestSelection, VisibilitySelection},
};
pub use measure::Measure;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

/// An interpreted header citation tied to one captured file.
#[derive(Clone, Debug, Serialize)]
pub struct HeaderCitation {
    pub content_fingerprint: String,
    pub cited: bool,
}
/// A project's parsed commit citations; these are not inferred from bead-like strings.
#[derive(Clone, Debug, Serialize)]
pub struct Citations {
    pub description_fingerprint: String,
    pub beads: BTreeSet<String>,
}
#[derive(Default, Serialize)]
pub struct Inputs {
    pub before_headers: BTreeMap<String, HeaderCitation>,
    pub after_headers: BTreeMap<String, HeaderCitation>,
    pub citations: Option<Citations>,
    /// Project-declared support features; no feature name is inferred from its spelling.
    pub test_support_features: Vec<String>,
}
#[derive(Debug, Serialize)]
pub struct NameHomes {
    pub name: String,
    pub homes: BTreeSet<String>,
}
#[derive(Debug, Serialize)]
pub struct Rows {
    pub entry: Vec<LineRow>,
    pub file: Vec<LineRow>,
    pub name: Vec<NameHomes>,
    pub alias: BTreeSet<Alias>,
    pub header_uncited: BTreeMap<String, BTreeSet<String>>,
}
#[derive(Serialize)]
pub struct DebtSummary {
    pub limit: usize,
    pub rows: usize,
    pub over_before: usize,
    pub over_after: usize,
}
#[derive(Serialize)]
pub struct Summary {
    pub population: Vec<String>,
    pub entry: DebtSummary,
    pub file: DebtSummary,
    pub names: usize,
    pub aliases: usize,
    pub header_uncited: usize,
    pub hints: Vec<String>,
    pub measured: Rows,
}
#[derive(Serialize)]
pub struct Verdict {
    pub violations: Vec<String>,
    pub unavailable: Vec<MissingInput>,
    /// None when required measurement input is unavailable; never a fabricated zero.
    pub report: Option<Summary>,
}
impl Verdict {
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
/// Observe the generic substring citation rule with caller-supplied marker values.
/// Native wrappers may supply their own interpreted facts instead.
pub fn observe_headers(
    snapshot: &Snapshot,
    markers: &[&str],
) -> Result<BTreeMap<String, HeaderCitation>> {
    let report = documentation::audit(
        snapshot,
        ".",
        &documentation::Policy {
            items: Population {
                visibility: VisibilitySelection::All,
                tests: TestSelection::All,
            },
            headers: documentation::HeaderPopulation::AllFiles,
            header_scope: documentation::HeaderScope::AnyModule,
            item_kinds: &[],
            citation_markers: Some(markers),
        },
    )?;
    let uncited: BTreeSet<_> = report
        .findings
        .iter()
        .filter_map(|finding| match finding {
            documentation::Finding::MissingHeader { file }
            | documentation::Finding::UncitedHeader { file, .. } => Some(file.as_str()),
            documentation::Finding::MissingOuterLineDoc { .. } => None,
        })
        .collect();
    Ok(report
        .sources
        .iter()
        .map(|(path, fingerprint)| {
            (
                path.clone(),
                HeaderCitation {
                    content_fingerprint: fingerprint.clone(),
                    cited: !uncited.contains(path.as_str()),
                },
            )
        })
        .collect())
}
/// Measure native body-line, module-home and alias units without rereading source.
pub fn measure(
    snapshot: &Snapshot,
    policy: &ConcentrationPolicy,
    headers: &BTreeMap<String, HeaderCitation>,
) -> Result<Measure> {
    let members = snapshot
        .metadata()
        .workspace_packages()
        .into_iter()
        .map(|package| package.name.as_str())
        .collect();
    policy.validate_configuration(&members)?;
    read(snapshot, policy, headers)
}
fn read(
    snapshot: &Snapshot,
    policy: &ConcentrationPolicy,
    headers: &BTreeMap<String, HeaderCitation>,
) -> Result<Measure> {
    Ok(read_workspace(
        &Workspace::of(snapshot)?,
        policy,
        headers,
        &[],
    ))
}
fn read_workspace(
    workspace: &Workspace,
    policy: &ConcentrationPolicy,
    headers: &BTreeMap<String, HeaderCitation>,
    features: &[&str],
) -> Measure {
    let mut measured = Measure::of(workspace, policy, headers, features);
    for package in workspace
        .crates
        .iter()
        .filter(|package| policy.population.contains(&package.name))
    {
        if package.files.is_empty() {
            measured.unavailable.push(crate::model::Diagnostic {
                package: Some(package.name.clone()),
                file: package.src_rel.clone(),
                line: None,
                message: "selected package has no captured source-tree population".into(),
            });
        }
        measured.unavailable.extend(
            workspace
                .unavailable
                .iter()
                .filter(|issue| crate::architecture::contains(&package.src_rel, &issue.file))
                .cloned(),
        );
    }
    measured
}
/// Judge path-fixed debt against paired captures, with project citation observations.
pub fn landing(
    policy: &ConcentrationPolicy,
    published: Option<&ConcentrationPolicy>,
    before: &Snapshot,
    after: &Snapshot,
    inputs: &Inputs,
    description: &str,
) -> Result<Verdict> {
    let members = after
        .metadata()
        .workspace_packages()
        .into_iter()
        .map(|package| package.name.as_str())
        .collect();
    policy.validate_configuration(&members)?;
    Ok(landing_workspaces(
        policy,
        published,
        &Workspace::of(before)?,
        &Workspace::of(after)?,
        inputs,
        description,
    ))
}
pub(in crate::checks) fn landing_workspaces(
    policy: &ConcentrationPolicy,
    published: Option<&ConcentrationPolicy>,
    before: &Workspace,
    after: &Workspace,
    inputs: &Inputs,
    description: &str,
) -> Verdict {
    let features = inputs
        .test_support_features
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    let base = read_workspace(before, policy, &inputs.before_headers, &features);
    let published_reading = published
        .filter(|published| {
            published.population != policy.population
                || published.conventional_names != policy.conventional_names
        })
        .map(|published| read_workspace(before, published, &inputs.before_headers, &features));
    let reported = published_reading.as_ref().unwrap_or(&base);
    let new = read_workspace(after, policy, &inputs.after_headers, &features);
    let mut seen = BTreeSet::new();
    let unavailable = [("before", reported), ("before", &base), ("after", &new)]
        .into_iter()
        .flat_map(|(phase, measure)| measure.unavailable.iter().map(move |issue| (phase, issue)))
        .filter_map(|(phase, issue)| {
            let field = format!("concentration.{phase}.source.{}", issue.file);
            seen.insert((field.clone(), issue.message.clone()))
                .then(|| MissingInput {
                    field,
                    reason: issue.message.clone(),
                })
        })
        .collect::<Vec<_>>();
    if !unavailable.is_empty() {
        return Verdict {
            violations: Vec::new(),
            unavailable,
            report: None,
        };
    }
    ratchets::landing(
        policy,
        published,
        reported,
        &base,
        &new,
        description,
        inputs.citations.as_ref(),
    )
}

#[cfg(test)]
mod tests;
