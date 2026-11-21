//! Native ceiling judgments and the exact measured raises a purchase can authorize.
use super::{super::policy::Policy, Raise};
use std::collections::{BTreeMap, BTreeSet};
const POLICY_PATH: &str = "authored architecture policy";
pub(super) fn ceiling_changes(
    before: &Policy,
    after: &Policy,
    size: &BTreeMap<String, usize>,
    surface: &BTreeMap<String, usize>,
) -> (Vec<Raise>, Vec<String>) {
    let core = after
        .population
        .core
        .iter()
        .map(|name| size.get(name).copied().unwrap_or(0))
        .sum();
    // Growth the host approves has no ceiling to raise (reported unjudged).
    let ceilings = !(before.size.approve_growth || after.size.approve_growth);
    let mut subjects = Vec::new();
    if ceilings {
        subjects.push((
            "size.core_token_lines".to_owned(),
            Some(before.size.core_token_lines),
            Some(after.size.core_token_lines),
            Some(core),
        ));
    }
    let sizes = ceilings.then_some(("size.crate", &before.size.crates, &after.size.crates, size));
    for (prefix, old, new, measured) in
        sizes
            .into_iter()
            .chain([("surface", &before.surface, &after.surface, surface)])
    {
        let names = old.keys().chain(new.keys()).collect::<BTreeSet<_>>();
        subjects.extend(names.into_iter().map(|name| {
            (
                format!("{prefix}.{name}"),
                old.get(name).copied(),
                new.get(name).copied(),
                measured.get(name).copied(),
            )
        }));
    }
    let mut raises = Vec::new();
    let mut violations = Vec::new();
    for name in &before.typed_errors {
        if !after.typed_errors.contains(name) && size.contains_key(name) {
            violations.push(format!(
                "typed-error guard for `{name}` removed while the crate exists; retain its guard"
            ));
        }
    }
    for (subject, old, new, measured) in subjects {
        let quantity = measured.unwrap_or(0);
        match new {
            None if measured.is_some() => violations.push(format!(
                "ceiling `{subject}` removed while its measured subject is {quantity}; retain its guard"
            )),
            Some(ceiling) if old.is_some_and(|old| ceiling > old) => {
                raises.push(Raise {
                    subject: subject.clone(),
                    before: old,
                    after: ceiling,
                    measured: quantity,
                    owner: None,
                });
                if ceiling != quantity {
                    violations.push(format!(
                        "ceiling `{subject}` raised to {ceiling}, measured {quantity}; set the raise exactly to its measured value, with a resolving Purchase trailer"
                    ));
                }
            }
            _ => {}
        }
    }
    (raises, violations)
}

/// The core population's loosening law: a crate leaving `population.core`
/// needs `population.amended_by` naming the ruling, covering `population`,
/// and cited by the candidate's description, as a concentration loosening
/// does. An amendment that loosens nothing is refused.
pub(super) fn population_changes(
    before: &Policy,
    after: &Policy,
    description: &str,
    citations: Option<&crate::checks::concentration::Citations>,
    unavailable: &mut Vec<crate::checks::policy::MissingInput>,
) -> Vec<String> {
    use crate::checks::policy::Knob;
    let mut refusals = Vec::new();
    let amendment = after
        .population
        .amended_by
        .as_ref()
        .filter(|amendment| before.population.amended_by.as_ref() != Some(*amendment));
    let left: Vec<&String> = before
        .population
        .core
        .iter()
        .filter(|name| !after.population.core.contains(name))
        .collect();
    if let Some(amendment) = amendment {
        if crate::checks::concentration::known_uncited(
            "population",
            &amendment.bead,
            description,
            citations,
            unavailable,
        ) {
            refusals.push(format!(
                "population amended_by names `{}`, which this landing's description does not cite; cite it as ({}) so the citation check can confirm it",
                amendment.bead, amendment.bead
            ));
        }
        if amendment
            .covers
            .iter()
            .any(|knob| *knob != Knob::Population)
        {
            refusals.push(
                "population amended_by covers only `population`, the crates that leave `core`"
                    .into(),
            );
        }
        if left.is_empty() {
            refusals.push(
                "population amended_by covers `population`, but no crate leaves `core` in this landing; remove it".into(),
            );
        }
    }
    let covered = amendment.is_some_and(|amendment| amendment.covers.contains(&Knob::Population));
    for name in left.into_iter().filter(|_| !covered) {
        refusals.push(format!(
            "`{name}` leaves population.core; name the ruling with `amended_by = {{ bead = \"<reference>\", covers = [\"population\"] }}` in [population] and cite it, or keep the crate in core"
        ));
    }
    refusals
}

/// `[size] approve_growth`: no stored ceiling. Every core crate's size
/// before (`None` when the base has no such crate) and after, and a raise
/// for each that grew, for the host to approve; a crate with no baseline
/// raises its whole size. Shrinking needs nothing.
pub(super) fn size_growth(
    policy: &Policy,
    before: &BTreeMap<String, usize>,
    after: &BTreeMap<String, usize>,
) -> (Vec<super::SizeChange>, Vec<Raise>) {
    let mut changes = Vec::new();
    let mut raises = Vec::new();
    for name in &policy.population.core {
        let old = before.get(name).copied();
        let new = after.get(name).copied().unwrap_or(0);
        if old.is_none_or(|old| new > old) && new > 0 {
            raises.push(Raise {
                subject: format!("size.crate.{name}"),
                before: old,
                after: new,
                measured: new,
                owner: None,
            });
        }
        changes.push(super::SizeChange {
            crate_name: name.clone(),
            before: old,
            after: new,
        });
    }
    (changes, raises)
}

pub(super) fn ceiling_violations(
    policy: &Policy,
    size: &BTreeMap<String, usize>,
    surface: &BTreeMap<String, usize>,
) -> Vec<String> {
    let mut violations = Vec::new();
    // Growth approved by the host is judged elsewhere (reported unjudged).
    let ceilings = !policy.size.approve_growth;
    for (name, ceiling) in policy.size.crates.iter().filter(|_| ceilings) {
        let actual = size.get(name).copied().unwrap_or(0);
        if actual > *ceiling {
            violations.push(format!(
                "size `{name}` is {actual} token lines, above {ceiling}; shrink it or edit {POLICY_PATH} naming what the growth buys"
            ));
        }
    }
    let core = policy
        .population
        .core
        .iter()
        .map(|name| size.get(name).copied().unwrap_or(0))
        .sum::<usize>();
    if ceilings && core > policy.size.core_token_lines {
        violations.push(format!(
            "core size is {core} token lines, above {}; shrink it or edit {POLICY_PATH} naming what the growth buys",
            policy.size.core_token_lines
        ));
    }
    for (name, ceiling) in &policy.surface {
        let actual = surface.get(name).copied().unwrap_or(0);
        if actual > *ceiling {
            violations.push(format!(
                "surface `{name}` has {actual} public declarations, above {ceiling}; remove an interface or edit {POLICY_PATH} naming what it buys"
            ));
        }
    }
    violations
}
