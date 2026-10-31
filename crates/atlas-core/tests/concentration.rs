//! Debt judgment at the captured-source boundary, including unavailable project evidence.
mod support;
use atlas_core::{
    checks::{
        concentration::{self, Citations, Inputs},
        policy::{ConcentrationPolicy, Status},
    },
    discovery::snapshot_workspace,
};
use std::{collections::BTreeSet, fs};
use support::Fixture;
fn fixture(source: &str) -> Fixture {
    let root = Fixture::new(&[]);
    fs::create_dir(root.path().join("src")).expect("fixture source root");
    fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname='fix'\nversion='0.0.0'\nedition='2024'\n[workspace]\n",
    )
    .expect("manifest");
    fs::write(root.path().join("src/lib.rs"), source).expect("source");
    root
}
fn policy(extra: &str) -> ConcentrationPolicy {
    toml::from_str(&format!(
        "population=['fix']\nentry_lines=3\nfile_lines=5\nconventional_names=[]\n{extra}"
    ))
    .expect("policy fixture")
}
fn inputs(snapshot: &atlas_core::discovery::Snapshot) -> Inputs {
    Inputs {
        before_headers: concentration::observe_headers(snapshot, &["fixture:"])
            .expect("header reading"),
        after_headers: concentration::observe_headers(snapshot, &["fixture:"])
            .expect("header reading"),
        citations: None,
        test_support_features: Vec::new(),
    }
}
#[test]
fn absent_and_stale_header_observations_never_become_zero_debt() {
    let root = fixture("//! fixture: meaning\npub fn run() {}\n");
    let snapshot = snapshot_workspace(root.path()).unwrap();
    let p = policy("");
    let report =
        concentration::landing(&p, Some(&p), &snapshot, &snapshot, &Inputs::default(), "").unwrap();
    assert_eq!(report.status(), Status::Unavailable);
    assert!(report.report.is_none());
    let mut evidence = inputs(&snapshot);
    assert_eq!(
        concentration::landing(&p, Some(&p), &snapshot, &snapshot, &evidence, "")
            .unwrap()
            .status(),
        Status::Pass
    );
    evidence
        .after_headers
        .get_mut("src/lib.rs")
        .unwrap()
        .content_fingerprint = "stale".into();
    let report = concentration::landing(&p, Some(&p), &snapshot, &snapshot, &evidence, "").unwrap();
    assert_eq!(report.status(), Status::Unavailable);
    assert!(report.report.is_none());
}
#[test]
fn ruling_requires_the_project_interpretation_of_this_exact_commit_description() {
    let root = fixture("//! fixture: meaning\npub fn run() {}\n");
    let snapshot = snapshot_workspace(root.path()).unwrap();
    let published = policy("");
    let mut candidate = policy("amended_by={bead='project-ruling',covers=['file_lines']}");
    candidate.file_lines = 6;
    let description = "approved change";
    let mut evidence = inputs(&snapshot);
    assert_eq!(
        concentration::landing(
            &candidate,
            Some(&published),
            &snapshot,
            &snapshot,
            &evidence,
            description
        )
        .unwrap()
        .status(),
        Status::Unavailable
    );
    evidence.citations = Some(Citations {
        description_fingerprint: atlas_core::discovery::fingerprint(description.as_bytes()),
        beads: BTreeSet::new(),
    });
    assert_eq!(
        concentration::landing(
            &candidate,
            Some(&published),
            &snapshot,
            &snapshot,
            &evidence,
            description
        )
        .unwrap()
        .status(),
        Status::Refuse
    );
    evidence
        .citations
        .as_mut()
        .unwrap()
        .beads
        .insert("project-ruling".into());
    assert_eq!(
        concentration::landing(
            &candidate,
            Some(&published),
            &snapshot,
            &snapshot,
            &evidence,
            description
        )
        .unwrap()
        .status(),
        Status::Pass
    );
    assert_eq!(
        concentration::landing(
            &candidate,
            Some(&published),
            &snapshot,
            &snapshot,
            &evidence,
            "different description"
        )
        .unwrap()
        .status(),
        Status::Unavailable
    );
}
#[test]
fn alias_syntax_gaps_are_unavailable_and_measurements_never_reopen_source() {
    let root = fixture("//! fixture: meaning\npub use crate::{;\n");
    let snapshot = snapshot_workspace(root.path()).unwrap();
    let p = policy("");
    let evidence = inputs(&snapshot);
    let reading = concentration::measure(&snapshot, &p, &evidence.after_headers).unwrap();
    assert!(!reading.unavailable.is_empty());
    let report = concentration::landing(&p, Some(&p), &snapshot, &snapshot, &evidence, "").unwrap();
    assert_eq!(report.status(), Status::Unavailable);
    assert!(report.report.is_none());

    let root = fixture("//! fixture: meaning\npub fn run() {}\n");
    let snapshot = snapshot_workspace(root.path()).unwrap();
    let evidence = inputs(&snapshot);
    fs::remove_file(root.path().join("src/lib.rs")).unwrap();
    let reading = concentration::measure(&snapshot, &p, &evidence.after_headers).unwrap();
    assert!(reading.unavailable.is_empty());
    assert_eq!(reading.file["src/lib.rs"], 1);
}
#[test]
fn native_accounting_keeps_directly_gated_helpers_and_excludes_test_modules() {
    let root = fixture(
        "//! fixture: meaning\n#[cfg(test)]\npub fn helper() {}\n#[cfg(test)]\nmod tests {pub fn hidden() {}}\n",
    );
    let snapshot = snapshot_workspace(root.path()).unwrap();
    let p = policy("");
    let evidence = inputs(&snapshot);
    let reading = concentration::measure(&snapshot, &p, &evidence.after_headers).unwrap();
    assert!(reading.unavailable.is_empty());
    assert_eq!(reading.file["src/lib.rs"], 2);
}

#[test]
fn a_joining_crate_cannot_claim_existing_shared_path_debt() {
    let root = fixture("//! fixture: meaning\npub struct Home;\n");
    fs::write(root.path().join("Cargo.toml"),"[package]\nname='fix'\nversion='0.0.0'\nedition='2024'\n[workspace]\nmembers=['src/zextra']\n").unwrap();
    fs::create_dir_all(root.path().join("src/zextra/src")).unwrap();
    fs::write(
        root.path().join("src/zextra/Cargo.toml"),
        "[package]\nname='zextra'\nversion='0.0.0'\nedition='2024'\n",
    )
    .unwrap();
    fs::write(
        root.path().join("src/zextra/src/lib.rs"),
        "//! fixture: meaning\nfn a() {}\nfn b() {}\nfn c() {}\nfn d() {}\nfn e() {}\nfn f() {}\n",
    )
    .unwrap();
    let snapshot = snapshot_workspace(root.path()).unwrap();
    let mut published = policy("");
    published.file_lines = 7;
    let mut candidate = policy("file=[{path='src/zextra/src/lib.rs',lines=6}]");
    candidate.population.push("zextra".into());
    let evidence = inputs(&snapshot);
    let report = concentration::landing(
        &candidate,
        Some(&published),
        &snapshot,
        &snapshot,
        &evidence,
        "",
    )
    .unwrap();
    assert_eq!(
        report.status(),
        Status::Refuse,
        "the shared path already belonged to fix; a joining context grants it no new row"
    );
}

#[test]
fn a_production_cfg_containing_test_does_not_exempt_a_path_remap() {
    for gate in ["#[cfg(not(test))]", "#[cfg(feature = \"contest\")]"] {
        let root = fixture(&format!(
            "//! fixture: meaning\n{gate}\n#[path=\"mapped.rs\"]\nmod hidden;\n"
        ));
        fs::write(root.path().join("src/mapped.rs"), "struct Item;\n").unwrap();
        let snapshot = snapshot_workspace(root.path()).unwrap();
        let p = policy("");
        let evidence = inputs(&snapshot);
        let report =
            concentration::landing(&p, Some(&p), &snapshot, &snapshot, &evidence, "").unwrap();
        assert_eq!(report.status(), Status::Refuse, "{gate}");
        assert!(
            report
                .violations
                .iter()
                .any(|finding| finding.contains("#[path]"))
        );
    }
}

#[test]
fn test_support_remaps_need_an_explicit_feature_and_markers_cannot_be_empty() {
    let root = fixture(
        "//! fixture: meaning\n#[cfg(any(test, feature = \"fixture-support\"))]\n#[path=\"mapped.rs\"]\nmod hidden;\n",
    );
    fs::write(root.path().join("src/mapped.rs"), "struct Item;\n").unwrap();
    let snapshot = snapshot_workspace(root.path()).unwrap();
    let p = policy("");
    let mut evidence = inputs(&snapshot);
    assert_eq!(
        concentration::landing(&p, Some(&p), &snapshot, &snapshot, &evidence, "")
            .unwrap()
            .status(),
        Status::Refuse
    );
    evidence
        .test_support_features
        .push("fixture-support".into());
    assert_eq!(
        concentration::landing(&p, Some(&p), &snapshot, &snapshot, &evidence, "")
            .unwrap()
            .status(),
        Status::Pass
    );
    assert!(concentration::observe_headers(&snapshot, &[]).is_err());
    assert!(concentration::observe_headers(&snapshot, &[""]).is_err());
}
