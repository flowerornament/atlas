//! Whole paired-capture judgment, without host-specific Git or bead interpretation.
mod support;
use atlas_core::{
    checks::{
        concentration,
        delta::{self, Basis, Inputs},
        policy::{Policy, Status},
    },
    discovery::{Snapshot, snapshot_workspace},
};
use std::{collections::BTreeMap, fs};
use support::Fixture;
const POLICY: &str = "schema=1\nprovenance='authored'\nsurface={fix=1}\nauthority=[{fact='home',home='src/lib.rs::Home'}]\ntyped_errors=['fix']\n[population]\ncore=['fix']\n[size]\ncore_token_lines=100\ncrate={fix=100}\n[crate_graph]\nrule=[{name='fix',allowed=[],law='leaf'}]\n[concentration]\npopulation=['fix']\nentry_lines=300\nfile_lines=1000\nconventional_names=[]\n";
fn fixture() -> Fixture {
    let root = Fixture::new(&[]);
    fs::create_dir(root.path().join("src")).expect("source directory");
    fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname='fix'\nversion='0.0.0'\nedition='2024'\n[workspace]\n",
    )
    .expect("manifest");
    write(
        root.path(),
        "src/lib.rs",
        "//! fixture: meaning\npub struct Home;\n",
    );
    root
}
fn write(root: &std::path::Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().expect("parent")).expect("directory");
    fs::write(path, text).expect("source");
}
fn inputs(before: &Snapshot, after: &Snapshot) -> Inputs {
    let mut inputs = Inputs::new(
        after,
        Basis {
            baseline: "host-resolved-before".into(),
            candidate: "host-resolved-after".into(),
        },
        "candidate description".into(),
    );
    inputs.concentration = concentration::Inputs {
        before_headers: concentration::observe_headers(before, &["fixture:"])
            .expect("before headers"),
        after_headers: concentration::observe_headers(after, &["fixture:"]).expect("after headers"),
        citations: None,
        test_support_features: Vec::new(),
    };
    inputs
}
#[test]
fn identical_capture_passes_and_measured_surface_growth_refuses_until_its_exact_raise() {
    let root = fixture();
    let before = snapshot_workspace(root.path()).unwrap();
    let policy = Policy::parse(POLICY).unwrap();
    let report = delta::check(
        &before,
        &before,
        &policy,
        &policy,
        &inputs(&before, &before),
    )
    .unwrap();
    assert_eq!(report.status(), Status::Pass);
    assert_eq!(
        report.measurements.as_ref().unwrap().after.crates["fix"].surface,
        1
    );
    write(
        root.path(),
        "src/lib.rs",
        "//! fixture: meaning\npub struct Home;\npub struct Extra;\n",
    );
    let after = snapshot_workspace(root.path()).unwrap();
    let evidence = inputs(&before, &after);
    let refused = delta::check(&before, &after, &policy, &policy, &evidence).unwrap();
    assert_eq!(refused.status(), Status::Refuse);
    assert!(
        refused
            .violations
            .iter()
            .any(|finding| finding.contains("surface `fix`"))
    );
    let mut raised = policy.clone();
    raised.surface.insert("fix".into(), 2);
    let report = delta::check(&before, &after, &policy, &raised, &evidence).unwrap();
    assert_eq!(report.status(), Status::Pass);
    assert_eq!(report.raises.len(), 1);
    assert_eq!(report.raises[0].subject, "surface.fix");
    assert_eq!(report.raises[0].measured, 2);
    raised.surface.insert("fix".into(), 3);
    let report = delta::check(&before, &after, &policy, &raised, &evidence).unwrap();
    assert_eq!(report.status(), Status::Refuse);
    assert!(
        report
            .violations
            .iter()
            .any(|finding| finding.contains("measured 2"))
    );
}
#[test]
fn size_and_typed_error_guards_participate_in_the_complete_judgment() {
    let root = fixture();
    let before = snapshot_workspace(root.path()).unwrap();
    let mut policy = Policy::parse(POLICY).unwrap();
    policy.size.crates = BTreeMap::from([("fix".into(), 1)]);
    policy.size.core_token_lines = 1;
    write(
        root.path(),
        "src/lib.rs",
        "//! fixture: meaning\npub struct Home;\nfn load()->Result<(),String> {todo!()}\n",
    );
    let after = snapshot_workspace(root.path()).unwrap();
    let report = delta::check(&before, &after, &policy, &policy, &inputs(&before, &after)).unwrap();
    assert_eq!(report.status(), Status::Refuse);
    assert!(
        report
            .violations
            .iter()
            .any(|finding| finding.contains("size `fix`"))
    );
    assert!(
        report
            .violations
            .iter()
            .any(|finding| finding.contains("production Result<_, String>"))
    );
    policy.typed_errors.clear();
    let report = delta::check(
        &before,
        &after,
        &Policy::parse(POLICY).unwrap(),
        &policy,
        &inputs(&before, &after),
    )
    .unwrap();
    assert!(
        report
            .violations
            .iter()
            .any(|finding| finding.contains("typed-error guard") && finding.contains("removed"))
    );
}
#[test]
fn forward_stage_paths_are_judged_alongside_accounting_and_missing_headers_are_unavailable() {
    let root = fixture();
    write(
        root.path(),
        "src/lib.rs",
        "//! fixture: meaning\nmod a;\nmod b;\npub struct Home;\n",
    );
    write(root.path(), "src/a.rs", "pub(crate) struct A;\n");
    write(root.path(), "src/b.rs", "pub(crate) struct B;\n");
    let policy =
        Policy::parse(&(POLICY.to_owned() + "\n[[stage_order]]\ncrate='fix'\nstages=['a','b']\n"))
            .unwrap();
    let before = snapshot_workspace(root.path()).unwrap();
    write(
        root.path(),
        "src/a.rs",
        "use crate::b::B;\npub(crate) struct A;\n",
    );
    let after = snapshot_workspace(root.path()).unwrap();
    let mut evidence = inputs(&before, &after);
    let report = delta::check(&before, &after, &policy, &policy, &evidence).unwrap();
    assert_eq!(report.status(), Status::Refuse);
    assert!(
        report
            .violations
            .iter()
            .any(|finding| finding.contains("later stage"))
    );
    evidence.concentration.after_headers.clear();
    let report = delta::check(&before, &after, &policy, &policy, &evidence).unwrap();
    assert_eq!(report.status(), Status::Unavailable);
    assert!(report.concentration.is_none());
    assert!(
        report
            .violations
            .iter()
            .any(|finding| finding.contains("later stage")),
        "known refusals remain visible"
    );
}
#[test]
fn captured_metadata_normal_dependencies_feed_the_terminal_leak_rule() {
    let root = fixture();
    let before = snapshot_workspace(root.path()).unwrap();
    let published = Policy::parse(POLICY).unwrap();
    write(
        root.path(),
        "Cargo.toml",
        "[package]\nname='fix'\nversion='0.0.0'\nedition='2024'\n[workspace]\nmembers=['extra']\n[dependencies]\nextra={path='extra'}\n",
    );
    write(
        root.path(),
        "extra/Cargo.toml",
        "[package]\nname='extra'\nversion='0.0.0'\nedition='2024'\n",
    );
    write(root.path(), "extra/src/lib.rs", "pub struct External;\n");
    let after = snapshot_workspace(root.path()).unwrap();
    let mut policy = published.clone();
    policy.crate_graph.terminal.push("extra".into());
    let report = delta::check(
        &before,
        &after,
        &published,
        &policy,
        &inputs(&before, &after),
    )
    .unwrap();
    assert_eq!(report.status(), Status::Refuse);
    assert!(
        report
            .violations
            .iter()
            .any(|finding| finding.contains("TERMINAL LEAK"))
    );
    assert!(report.measurements.as_ref().unwrap().after.dependencies["fix"].contains("extra"));
}

#[test]
fn strict_public_surface_excludes_test_modules_and_impl_blocks_but_expands_aliases() {
    let root = fixture();
    write(
        root.path(),
        "src/lib.rs",
        "pub struct Home;\nimpl Home {\n    pub fn method() {}\n}\n#[cfg(test)]\nmod tests {\n    pub type Probe = super::Home;\n}\npub use crate::{Home as First, Home as Second};\n",
    );
    let snapshot = snapshot_workspace(root.path()).unwrap();
    let measures = delta::measure::read(&snapshot).unwrap();
    assert_eq!(measures.crates["fix"].surface, 4);
}
#[test]
fn paired_capture_uses_each_revisions_package_members_without_reopening_the_baseline() {
    let root = fixture();
    write(
        root.path(),
        "Cargo.toml",
        "[package]\nname='fix'\nversion='0.0.0'\nedition='2024'\n[workspace]\nmembers=['retired']\n[dependencies]\nretired={path='retired'}\n",
    );
    write(
        root.path(),
        "retired/Cargo.toml",
        "[package]\nname='retired'\nversion='0.0.0'\nedition='2024'\n",
    );
    write(root.path(), "retired/src/lib.rs", "pub struct Old;\n");
    let before = snapshot_workspace(root.path()).unwrap();
    let mut published = Policy::parse(POLICY).unwrap();
    published.crate_graph.rule[0].allowed = Some(vec!["retired".into()]);
    published
        .crate_graph
        .rule
        .push(atlas_core::checks::policy::CrateRule {
            name: "retired".into(),
            allowed: Some(Vec::new()),
            law: Some("leaf".into()),
        });
    write(
        root.path(),
        "Cargo.toml",
        "[package]\nname='fix'\nversion='0.0.0'\nedition='2024'\n[workspace]\nmembers=['fresh']\n[dependencies]\nfresh={path='fresh'}\n",
    );
    write(
        root.path(),
        "fresh/Cargo.toml",
        "[package]\nname='fresh'\nversion='0.0.0'\nedition='2024'\n",
    );
    write(root.path(), "fresh/src/lib.rs", "pub struct New;\n");
    fs::remove_dir_all(root.path().join("retired")).unwrap();
    let after = snapshot_workspace(root.path()).unwrap();
    let mut policy = published.clone();
    policy.crate_graph.rule[0].allowed = Some(vec!["fresh".into()]);
    policy.crate_graph.rule[1].name = "fresh".into();
    let result = delta::check(
        &before,
        &after,
        &published,
        &policy,
        &inputs(&before, &after),
    )
    .unwrap();
    assert_eq!(result.status(), Status::Pass);
    let measurements = result.measurements.unwrap();
    assert!(measurements.before.crates.contains_key("retired"));
    assert!(!measurements.before.crates.contains_key("fresh"));
    assert!(measurements.after.crates.contains_key("fresh"));
    assert!(!measurements.after.crates.contains_key("retired"));
    assert!(measurements.before.dependencies["fix"].contains("retired"));
    assert!(measurements.after.dependencies["fix"].contains("fresh"));
}
