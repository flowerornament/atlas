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
    let mut subjects = vec![(
        "size.core_token_lines".to_owned(),
        Some(before.size.core_token_lines),
        Some(after.size.core_token_lines),
        Some(core),
    )];
    for (prefix, old, new, measured) in [
        ("size.crate", &before.size.crates, &after.size.crates, size),
        ("surface", &before.surface, &after.surface, surface),
    ] {
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

pub(super) fn ceiling_violations(
    policy: &Policy,
    size: &BTreeMap<String, usize>,
    surface: &BTreeMap<String, usize>,
) -> Vec<String> {
    let mut violations = Vec::new();
    for (name, ceiling) in &policy.size.crates {
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
    if core > policy.size.core_token_lines {
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
