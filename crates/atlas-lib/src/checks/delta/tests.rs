//! Native guard-retention, exact-purchase, growth and graph-law assertions.
use super::super::policy::*;
use super::ceilings::{ceiling_changes, ceiling_violations};
use std::collections::{BTreeMap, BTreeSet};
fn fixture_policy() -> Policy {
    Policy {
            schema: 1,
            provenance: "authored".into(),
            population: CorePopulation {
                core: vec!["core".into()],
                amended_by: None,
            },
            size: SizeCeilings {
                core_token_lines: 2,
                crates: BTreeMap::from([("core".into(), 2)]),
                approve_growth: false,
            },
            surface: BTreeMap::from([("core".into(), 1)]),
            crate_graph: CrateGraphPolicy {
                vendored: vec![],
                terminal: vec![],
                transparent: vec![],
                forbidden_members: vec![],
                rule: vec![CrateRule {
                    name: "core".into(),
                    allowed: Some(Allowed::Crates(vec![])),
                    law: Some("core is a leaf".into()),
                }],
            },
            authority: vec![AuthorityRule {
                fact: "one home".into(),
                home: "crates/core/src/lib.rs::Home".into(),
                forbid_item: vec![],
                forbid_path: vec![],
                forbid_pattern: vec![],
                allowed_readers: None,
                forbid_new_pattern: vec![],
                forbid_new_observed: vec![],
                allowed_in: vec![],
            }],
            stage_order: vec![],
            stage_reads: BTreeMap::new(),
            stage_seams: BTreeMap::new(),
            typed_errors: vec![],
            concentration: Some(
                toml::from_str(
                    "population = ['core']\nentry_lines = 300\nfile_lines = 2000\nconventional_names = []\n",
                )
                .expect("fixture concentration section"),
            ),
        }
}

#[test]
fn a_present_zero_subject_cannot_drop_its_guard() {
    let before = fixture_policy();
    let mut after = fixture_policy();
    after.surface.clear();
    let size = BTreeMap::from([("core".into(), 1)]);
    let surface = BTreeMap::from([("core".into(), 0)]);
    assert!(
        ceiling_changes(&before, &after, &size, &BTreeMap::new())
            .1
            .is_empty()
    );
    let (raises, refusals) = ceiling_changes(&before, &after, &size, &surface);
    assert!(raises.is_empty());
    assert_eq!(
        refusals.len(),
        1,
        "an existing crate remains the guard subject even at zero"
    );
    assert!(refusals[0].contains("surface.core"));
}

#[test]
fn a_present_crate_cannot_drop_its_typed_error_guard() {
    let mut before = fixture_policy();
    before.typed_errors.push("core".into());
    let after = fixture_policy();
    let size = BTreeMap::from([("core".into(), 1)]);
    let (_, refused) = ceiling_changes(&before, &after, &size, &BTreeMap::new());
    assert_eq!(refused.len(), 1);
    assert!(refused[0].contains("typed-error guard"));
    assert!(
        ceiling_changes(&before, &after, &BTreeMap::new(), &BTreeMap::new())
            .1
            .is_empty(),
        "retiring the crate retires its guard"
    );
}

#[test]
fn ceiling_raises_buy_exact_measurements_without_headroom() {
    let before = fixture_policy();
    let mut after = fixture_policy();
    let size = BTreeMap::from([("core".into(), 3)]);
    let surface = BTreeMap::from([("core".into(), 1)]);
    after.size.core_token_lines = 3;
    after.size.crates.insert("core".into(), 3);
    let (raises, refusals) = ceiling_changes(&before, &after, &size, &surface);
    assert_eq!(raises.len(), 2, "aggregate and crate are separate guards");
    assert!(refusals.is_empty(), "{refusals:?}");
    after.size.core_token_lines = 4;
    after.size.crates.insert("core".into(), 4);
    let (_, refusals) = ceiling_changes(&before, &after, &size, &surface);
    assert_eq!(refusals.len(), 2, "neither guard buys headroom");

    let mut after = fixture_policy();
    after.surface.insert("core".into(), 77_479);
    let (_, refusals) = ceiling_changes(&before, &after, &size, &surface);
    assert_eq!(refusals.len(), 1);
    assert!(refusals[0].contains("surface.core"));
    assert!(refusals[0].contains("measured 1"));
}

#[test]
fn a_transfer_still_reports_the_raised_guard() {
    let mut before = fixture_policy();
    before.population.core.push("other".into());
    before.size.core_token_lines = 4;
    before.size.crates.insert("other".into(), 2);
    let mut after = fixture_policy();
    after.population.core.push("other".into());
    after.size.core_token_lines = 4;
    after.size.crates.insert("core".into(), 3);
    after.size.crates.insert("other".into(), 1);
    let size = BTreeMap::from([("core".into(), 3), ("other".into(), 1)]);
    let surface = BTreeMap::from([("core".into(), 1)]);
    let (raises, refusals) = ceiling_changes(&before, &after, &size, &surface);
    assert!(refusals.is_empty());
    assert_eq!(raises.len(), 1);
    assert_eq!(raises[0].subject, "size.crate.core");
}

#[test]
fn lowering_is_free_and_guard_removal_requires_subject_removal() {
    let before = fixture_policy();
    let mut after = fixture_policy();
    after.size.core_token_lines = 1;
    after.size.crates.insert("core".into(), 1);
    let size = BTreeMap::from([("core".into(), 1)]);
    let surface = BTreeMap::from([("core".into(), 1)]);
    assert_eq!(
        ceiling_changes(&before, &after, &size, &surface),
        (vec![], vec![])
    );
    after.size.crates.clear();
    let (raises, refusals) = ceiling_changes(&before, &after, &size, &surface);
    assert!(raises.is_empty());
    assert_eq!(refusals.len(), 1);
    assert!(refusals[0].contains("retain its guard"));
    after.population.core.clear();
    after.size.core_token_lines = 0;
    after.surface.clear();
    assert_eq!(
        ceiling_changes(&before, &after, &BTreeMap::new(), &BTreeMap::new()),
        (vec![], vec![])
    );
}

#[test]
fn size_growth_refuses() {
    let policy = fixture_policy();
    let violations = ceiling_violations(
        &policy,
        &BTreeMap::from([("core".into(), 3)]),
        &BTreeMap::from([("core".into(), 1)]),
    );
    assert!(violations.iter().any(|row| row.starts_with("size `core`")));
}

#[test]
fn surface_growth_refuses() {
    let policy = fixture_policy();
    let violations = ceiling_violations(
        &policy,
        &BTreeMap::from([("core".into(), 2)]),
        &BTreeMap::from([("core".into(), 2)]),
    );
    assert!(
        violations
            .iter()
            .any(|row| row.starts_with("surface `core`"))
    );
}

#[test]
fn illegal_edge_refuses() {
    let policy = fixture_policy();
    let members = ["core", "other"]
        .into_iter()
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
    let dag = BTreeMap::from([("core".into(), BTreeSet::from(["other".into()]))]);
    let violations = policy.crate_graph.violations(&members, &dag);
    assert!(violations.iter().any(|row| row.starts_with("ILLEGAL EDGE")));
}

fn names(members: &[&str]) -> BTreeSet<String> {
    members.iter().map(|name| (*name).to_owned()).collect()
}
fn rule(text: &str) -> CrateRule {
    toml::from_str(text).expect("fixture crate rule")
}

/// An allowed crate the rule's crate does not depend on is unused slack:
/// a later edge would spend it without anyone deciding to.
#[test]
fn an_unused_allowance_refuses() {
    let mut policy = fixture_policy();
    policy
        .crate_graph
        .rule
        .push(rule("name = 'other'\nallowed = ['core']"));
    let violations = policy
        .crate_graph
        .violations(&names(&["core", "other"]), &BTreeMap::new());
    assert!(
        violations
            .iter()
            .any(|row| row.starts_with("UNUSED ALLOWANCE: `other` allows `core`")),
        "{violations:?}"
    );
    // Used, it is no violation; "any" lists nothing to leave unused.
    let used = BTreeMap::from([("other".into(), BTreeSet::from(["core".into()]))]);
    assert!(
        policy
            .crate_graph
            .violations(&names(&["core", "other"]), &used)
            .is_empty()
    );
    policy.crate_graph.rule[1] = rule("name = 'other'\nallowed = 'any'");
    assert!(
        policy
            .crate_graph
            .violations(&names(&["core", "other"]), &BTreeMap::new())
            .is_empty()
    );
}

/// Code copied into the workspace from elsewhere was written against no
/// member of it.
#[test]
fn a_vendored_crate_with_a_member_edge_refuses() {
    let mut policy = fixture_policy();
    policy.crate_graph.vendored = vec!["upstream".into()];
    let dag = BTreeMap::from([("upstream".into(), BTreeSet::from(["core".into()]))]);
    let violations = policy
        .crate_graph
        .violations(&names(&["core", "upstream"]), &dag);
    assert!(
        violations
            .iter()
            .any(|row| row
                .starts_with("VENDORED EDGE: vendored `upstream` depends on member `core`")),
        "{violations:?}"
    );
}

/// A rule must say what it allows: absent is refused with both spellings.
#[test]
fn an_absent_allowed_set_is_refused_with_its_two_spellings() {
    let mut policy = fixture_policy();
    policy.crate_graph.rule[0] = rule("name = 'core'");
    let error = policy
        .validate_configuration(&BTreeSet::from(["core"]))
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("crate rule `core` has no `allowed`"),
        "{error}"
    );
    assert!(
        error.contains("`allowed = []`") && error.contains(r#"`allowed = "any"`"#),
        "{error}"
    );
}

#[test]
fn dependency_cycle_refuses() {
    let policy = fixture_policy();
    let members = ["core", "other"]
        .into_iter()
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
    let dag = BTreeMap::from([
        ("core".into(), BTreeSet::from(["other".into()])),
        ("other".into(), BTreeSet::from(["core".into()])),
    ]);
    let violations = policy.crate_graph.violations(&members, &dag);
    assert!(violations.iter().any(|row| row.starts_with("CYCLE")));
}

/// A crate leaving `population.core` is a loosening: it needs a cited
/// `population.amended_by` covering `population`, as concentration's.
#[test]
fn leaving_the_core_needs_a_cited_amendment() {
    use crate::checks::concentration::Citations;
    let mut before = fixture_policy();
    before.population.core.push("other".into());
    let after = fixture_policy();
    let cited = |description: &str, beads: &[&str]| Citations {
        description_fingerprint: crate::discovery::fingerprint(description.as_bytes()),
        beads: beads.iter().map(|bead| (*bead).to_owned()).collect(),
    };
    let check = |after: &Policy, description: &str, citations: &Citations| {
        let mut unavailable = Vec::new();
        let refusals = super::ceilings::population_changes(
            &before,
            after,
            description,
            Some(citations),
            &mut unavailable,
        );
        assert!(unavailable.is_empty());
        refusals
    };
    let refused = check(&after, "", &cited("", &[]));
    assert_eq!(refused.len(), 1, "{refused:?}");
    assert!(
        refused[0].starts_with("`other` leaves population.core"),
        "{refused:?}"
    );
    let mut amended = after.clone();
    amended.population.amended_by =
        Some(toml::from_str("bead = 'ruling-1'\ncovers = ['population']").expect("amendment"));
    assert!(check(&amended, "(ruling-1)", &cited("(ruling-1)", &["ruling-1"])).is_empty());
    let uncited = check(&amended, "", &cited("", &[]));
    assert!(
        uncited.iter().any(|text| text.contains("does not cite")),
        "{uncited:?}"
    );
    // An amendment with nothing to cover is refused.
    let mut idle = before.clone();
    idle.population.amended_by = amended.population.amended_by.clone();
    let mut unavailable = Vec::new();
    let refused = super::ceilings::population_changes(
        &before,
        &idle,
        "(ruling-1)",
        Some(&cited("(ruling-1)", &["ruling-1"])),
        &mut unavailable,
    );
    assert!(
        refused
            .iter()
            .any(|text| text.contains("no crate leaves `core`")),
        "{refused:?}"
    );
}
