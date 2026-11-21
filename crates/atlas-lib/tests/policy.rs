//! Complete generic policy validation, helper custody and failure outcomes.
mod support;
use atlas_lib::{
    analysis::Analysis,
    checks::policy::{
        self, Inputs, Match, PathState, PatternState, Policy, ReferenceResolution, Resolution,
        Status,
    },
    discovery::snapshot_workspace,
};
use std::{collections::BTreeMap, fs};
use support::Fixture;
const POLICY: &str = "schema=1\nprovenance='authored'\nsurface={fixture=10}\nauthority=[{fact='one home',home='src/lib.rs::Home'}]\ntyped_errors=['fixture']\n[population]\ncore=['fixture']\n[size]\ncore_token_lines=100\ncrate={fixture=100}\n[crate_graph]\nrule=[{name='fixture',allowed=[],law='leaf'}]\n[concentration]\npopulation=['fixture']\nentry_lines=300\nfile_lines=2000\nconventional_names=[]\n";
fn fixture(source: &str) -> Fixture {
    let root = Fixture::new(&[]);
    fs::create_dir(root.path().join("src")).expect("source directory");
    fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname='fixture'\nversion='0.0.0'\nedition='2024'\n[workspace]\n",
    )
    .expect("manifest");
    fs::write(root.path().join("src/lib.rs"), source).expect("fixture source");
    root
}
#[test]
fn source_authorities_refuse_ambiguous_or_test_only_homes_and_forbidden_items() {
    for (source, needle) in [
        ("pub struct Home;\n", ""),
        ("pub struct Home;\npub struct Home;\n", "resolves 2 times"),
        ("#[cfg(test)]\npub struct Home;\n", "resolves 0 times"),
        (
            "pub struct Home;\npub fn load()->Result<(),String> {todo!()}\n",
            "production Result<_, String>",
        ),
    ] {
        let root = fixture(source);
        let snapshot = snapshot_workspace(root.path()).unwrap();
        let analysis = Analysis::new(&snapshot).unwrap();
        let policy = Policy::parse(POLICY).unwrap();
        let report = policy::validate(&analysis, &policy, &Inputs::new(&snapshot)).unwrap();
        if needle.is_empty() {
            assert_eq!(report.status(), Status::Pass);
            assert_eq!(report.authority_homes.len(), 1);
        } else {
            assert_eq!(report.status(), Status::Refuse);
            assert!(
                report.violations.iter().any(|text| text.contains(needle)),
                "{:?}",
                report.violations
            );
        }
    }
    for (source, status) in [
        ("pub struct Home;\npub struct Old;\n", Status::Refuse),
        (
            "pub struct Home;\n#[cfg(test)]\npub struct Old;\n",
            Status::Pass,
        ),
    ] {
        let root = fixture(source);
        let snapshot = snapshot_workspace(root.path()).unwrap();
        let analysis = Analysis::new(&snapshot).unwrap();
        let mut policy = Policy::parse(POLICY).unwrap();
        policy.authority[0].forbid_item.push("src::Old".into());
        assert_eq!(
            policy::validate(&analysis, &policy, &Inputs::new(&snapshot))
                .unwrap()
                .status(),
            status
        );
    }
}
#[test]
fn missing_paths_patterns_and_project_authority_never_mean_pass() {
    let root = fixture("pub struct Home;\n");
    let snapshot = snapshot_workspace(root.path()).unwrap();
    let analysis = Analysis::new(&snapshot).unwrap();
    let mut policy = Policy::parse(POLICY).unwrap();
    policy.authority[0].forbid_path.push("old.rs".into());
    let mut inputs = Inputs::new(&snapshot);
    assert_eq!(
        policy::validate(&analysis, &policy, &inputs)
            .unwrap()
            .status(),
        Status::Unavailable
    );
    inputs.paths.insert("old.rs".into(), PathState::Absent);
    assert_eq!(
        policy::validate(&analysis, &policy, &inputs)
            .unwrap()
            .status(),
        Status::Pass
    );
    inputs.paths.insert("old.rs".into(), PathState::Present);
    assert_eq!(
        policy::validate(&analysis, &policy, &inputs)
            .unwrap()
            .status(),
        Status::Refuse
    );
    inputs.paths.insert("old.rs".into(), PathState::Absent);
    policy.authority[0]
        .forbid_pattern
        .push("src::struct Old;".into());
    assert_eq!(
        policy::validate(&analysis, &policy, &inputs)
            .unwrap()
            .status(),
        Status::Unavailable
    );
    let sources = BTreeMap::from([(
        "src/lib.rs".into(),
        snapshot
            .physical_file("src/lib.rs")
            .unwrap()
            .content_fingerprint
            .clone(),
    )]);
    inputs.patterns.insert(
        "src::struct Old;".into(),
        PatternState::Read {
            sources: sources.clone(),
            matches: Vec::new(),
        },
    );
    assert_eq!(
        policy::validate(&analysis, &policy, &inputs)
            .unwrap()
            .status(),
        Status::Pass
    );
    inputs.patterns.insert(
        "src::struct Old;".into(),
        PatternState::Read {
            sources,
            matches: vec![Match {
                file: "src/lib.rs".into(),
                line: 1,
            }],
        },
    );
    assert_eq!(
        policy::validate(&analysis, &policy, &inputs)
            .unwrap()
            .status(),
        Status::Refuse
    );
}
#[test]
fn stale_and_escaped_pattern_observations_are_unavailable() {
    let root = fixture("pub struct Home;\n");
    let snapshot = snapshot_workspace(root.path()).unwrap();
    let analysis = Analysis::new(&snapshot).unwrap();
    let mut policy = Policy::parse(POLICY).unwrap();
    policy.authority[0]
        .forbid_pattern
        .push("src::struct Old;".into());
    let mut inputs = Inputs::new(&snapshot);
    inputs.patterns.insert(
        "src::struct Old;".into(),
        PatternState::Read {
            sources: BTreeMap::new(),
            matches: Vec::new(),
        },
    );
    assert_eq!(
        policy::validate(&analysis, &policy, &inputs)
            .unwrap()
            .status(),
        Status::Unavailable
    );
    let sources = BTreeMap::from([(
        "src/lib.rs".into(),
        snapshot
            .physical_file("src/lib.rs")
            .unwrap()
            .content_fingerprint
            .clone(),
    )]);
    inputs.patterns.insert(
        "src::struct Old;".into(),
        PatternState::Read {
            sources,
            matches: vec![Match {
                file: "elsewhere.rs".into(),
                line: 1,
            }],
        },
    );
    assert_eq!(
        policy::validate(&analysis, &policy, &inputs)
            .unwrap()
            .status(),
        Status::Unavailable
    );
    let other = fixture("pub struct Home;\n");
    let other = snapshot_workspace(other.path()).unwrap();
    assert_eq!(
        policy::validate(&analysis, &policy, &Inputs::new(&other))
            .unwrap()
            .status(),
        Status::Unavailable
    );
}
#[test]
fn project_resolutions_are_bound_to_the_exact_written_reference() {
    let root = fixture("pub struct Home;\n");
    let snapshot = snapshot_workspace(root.path()).unwrap();
    let analysis = Analysis::new(&snapshot).unwrap();
    let mut policy = Policy::parse(POLICY).unwrap();
    policy.concentration.as_mut().unwrap().amended_by = Some(
        toml::from_str("bead = 'project-ruling'\ncovers = ['entry_lines']\n").expect("amendment"),
    );
    let needed = policy.external_references().remove(0);
    let mut inputs = Inputs::new(&snapshot);
    assert_eq!(
        policy::validate(&analysis, &policy, &inputs)
            .unwrap()
            .status(),
        Status::Unavailable
    );
    inputs.references.push(ReferenceResolution {
        reference: needed.clone(),
        result: Resolution::Valid,
    });
    assert_eq!(
        policy::validate(&analysis, &policy, &inputs)
            .unwrap()
            .status(),
        Status::Pass
    );
    inputs.references[0].reference.written = "some-other-ruling".into();
    assert_eq!(
        policy::validate(&analysis, &policy, &inputs)
            .unwrap()
            .status(),
        Status::Unavailable
    );
    inputs.references.push(ReferenceResolution {
        reference: needed.clone(),
        result: Resolution::Invalid("bad ruling".into()),
    });
    assert_eq!(
        policy::validate(&analysis, &policy, &inputs)
            .unwrap()
            .status(),
        Status::Refuse
    );
    inputs.references.push(ReferenceResolution {
        reference: needed,
        result: Resolution::Valid,
    });
    assert_eq!(
        policy::validate(&analysis, &policy, &inputs)
            .unwrap()
            .status(),
        Status::Unavailable
    );
}
#[cfg(unix)]
fn helper(root: &std::path::Path, name: &str, body: &str) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = root.join(name);
    fs::write(&path, format!("#!/bin/sh\n{body}\n")).expect("helper fixture");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).expect("executable helper");
    path
}
#[cfg(unix)]
#[test]
fn helpers_observe_private_captured_bytes_and_failures_stay_unavailable() {
    let root = fixture("pub struct Home; // captured_marker\n");
    let snapshot = snapshot_workspace(root.path()).unwrap();
    let analysis = Analysis::new(&snapshot).unwrap();
    let mut policy = Policy::parse(POLICY).unwrap();
    policy.authority[0]
        .forbid_pattern
        .push("src::struct Old;".into());
    fs::write(root.path().join("src/lib.rs"), "changed original source").unwrap();
    let good = helper(
        root.path(),
        "good.sh",
        "for last do :; done\ncase \"$last\" in */atlas-policy-*/*) ;; *) exit 2;; esac\ngrep -q captured_marker \"$last\" || exit 3\nprintf '[]'",
    );
    let inputs = policy::observe_with_helper(&snapshot, &policy, &good);
    assert_eq!(
        policy::validate(&analysis, &policy, &inputs)
            .unwrap()
            .status(),
        Status::Pass
    );
    let cwd = std::env::current_dir().unwrap();
    let relative_home = tempfile::tempdir_in(&cwd).unwrap();
    let relative_tool = helper(relative_home.path(), "relative.sh", "printf '[]'");
    let inputs = policy::observe_with_helper(
        &snapshot,
        &policy,
        relative_tool.strip_prefix(&cwd).unwrap(),
    );
    assert_eq!(
        policy::validate(&analysis, &policy, &inputs)
            .unwrap()
            .status(),
        Status::Pass
    );
    for (name, body) in [
        ("bad-json.sh", "printf broken"),
        ("warning.sh", "printf '[]'; printf warning >&2"),
        ("exit.sh", "exit 2"),
    ] {
        let tool = helper(root.path(), name, body);
        let inputs = policy::observe_with_helper(&snapshot, &policy, &tool);
        assert!(
            serde_json::to_value(&inputs).is_ok(),
            "helper failure must remain serializable"
        );
        assert_eq!(
            policy::validate(&analysis, &policy, &inputs)
                .unwrap()
                .status(),
            Status::Unavailable,
            "{name}"
        );
    }
    let inputs =
        policy::observe_with_helper(&snapshot, &policy, &root.path().join("missing-helper"));
    assert_eq!(
        policy::validate(&analysis, &policy, &inputs)
            .unwrap()
            .status(),
        Status::Unavailable
    );
}

#[cfg(unix)]
#[test]
fn excluded_authority_and_pattern_populations_remain_unavailable() {
    let root = fixture("pub struct Home;\n");
    let outside = root.path().join("outside");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("hidden.rs"), "pub struct Home;").unwrap();
    std::os::unix::fs::symlink(&outside, root.path().join("src/hidden")).unwrap();
    let snapshot = snapshot_workspace(root.path()).unwrap();
    let analysis = Analysis::new(&snapshot).unwrap();
    let mut policy = Policy::parse(POLICY).unwrap();
    policy.typed_errors.clear();
    policy.authority[0].home = "src/hidden/hidden.rs::Home".into();
    assert_eq!(
        policy::validate(&analysis, &policy, &Inputs::new(&snapshot))
            .unwrap()
            .status(),
        Status::Unavailable
    );
    policy.authority[0].home = "src/lib.rs::Home".into();
    policy.authority[0]
        .forbid_item
        .push("src/hidden::Old".into());
    assert_eq!(
        policy::validate(&analysis, &policy, &Inputs::new(&snapshot))
            .unwrap()
            .status(),
        Status::Unavailable
    );
    policy.authority[0].forbid_item.clear();
    policy.authority[0]
        .forbid_pattern
        .push("src/hidden::struct Old;".into());
    let inputs = policy::observe_with_helper(&snapshot, &policy, &root.path().join("must-not-run"));
    assert_eq!(
        policy::validate(&analysis, &policy, &inputs)
            .unwrap()
            .status(),
        Status::Unavailable
    );
}

#[test]
fn forbidden_paths_must_be_normalized_workspace_relative_names() {
    let root = fixture("pub struct Home;\n");
    let snapshot = snapshot_workspace(root.path()).unwrap();
    let analysis = Analysis::new(&snapshot).unwrap();
    for path in [
        "",
        "../outside",
        "/outside",
        "src/../outside",
        "src//old",
        "./old",
    ] {
        let mut policy = Policy::parse(POLICY).unwrap();
        policy.authority[0].forbid_path.push(path.into());
        let inputs =
            policy::observe_with_helper(&snapshot, &policy, &root.path().join("must-not-run"));
        assert!(
            inputs.paths.is_empty(),
            "invalid path must not be probed: {path}"
        );
        assert_eq!(
            policy::validate(&analysis, &policy, &inputs)
                .unwrap()
                .status(),
            Status::Refuse,
            "{path}"
        );
    }
}

#[cfg(unix)]
#[test]
fn source_scopes_exclude_siblings_and_forbidden_paths_stop_at_intermediate_symlinks() {
    let root = fixture("pub struct Home;\n");
    fs::create_dir(root.path().join("src/old")).unwrap();
    fs::write(root.path().join("src/old/kept.rs"), "pub struct Kept;\n").unwrap();
    fs::write(root.path().join("src/older.rs"), "pub struct Old;\n").unwrap();
    let outside = Fixture::new(&[]);
    fs::write(outside.path().join("hidden.rs"), "pub struct Hidden;\n").unwrap();
    std::os::unix::fs::symlink(outside.path(), root.path().join("src/older")).unwrap();
    let snapshot = snapshot_workspace(root.path()).unwrap();
    let analysis = Analysis::new(&snapshot).unwrap();
    let mut policy = Policy::parse(POLICY).unwrap();
    policy.typed_errors.clear(); // Isolate the scoped authority from the unrelated source gap.
    policy.authority[0].forbid_item.push("src/old::Old".into());
    let query = "src/old::struct Old;";
    policy.authority[0].forbid_pattern.push(query.into());
    let good = helper(
        root.path(),
        "scope.sh",
        "[ -f src/old/kept.rs ] || exit 2\n[ ! -e src/older.rs ] || exit 3\nprintf '[]'",
    );
    let inputs = policy::observe_with_helper(&snapshot, &policy, &good);
    let PatternState::Read { sources, .. } = &inputs.patterns[query] else {
        panic!("expected captured query");
    };
    assert_eq!(
        sources.keys().map(String::as_str).collect::<Vec<_>>(),
        ["src/old/kept.rs"]
    );
    let report = policy::validate(&analysis, &policy, &inputs).unwrap();
    assert_eq!(
        report.status(),
        Status::Pass,
        "{:?}",
        serde_json::to_value(&report).unwrap()
    );

    policy.authority[0]
        .forbid_path
        .push("src/older/absent".into());
    let inputs = policy::observe_with_helper(&snapshot, &policy, &good);
    assert!(matches!(
        &inputs.paths["src/older/absent"],
        PathState::Unavailable(_)
    ));
    assert_eq!(
        policy::validate(&analysis, &policy, &inputs)
            .unwrap()
            .status(),
        Status::Unavailable
    );
    policy.authority[0].forbid_path = vec!["src/older".into()];
    let inputs = policy::observe_with_helper(&snapshot, &policy, &good);
    assert!(matches!(&inputs.paths["src/older"], PathState::Present));
    assert_eq!(
        policy::validate(&analysis, &policy, &inputs)
            .unwrap()
            .status(),
        Status::Refuse
    );
}

/// Every field 0.21's policy extensions add, written once: the policy
/// parses, and each field this build reads but does not judge yet is
/// reported unavailable by its address, so the check is never a complete
/// PASS. Without them the same policy passes as before. The owners and
/// citations judged since wait for the host's resolution instead.
const EXTENDED: &str = r#"
schema = 1
provenance = "authored"
surface = { fixture = 10 }
typed_errors = []
authority = [
  { fact = "one home", home = "src/lib.rs::Home",
    allowed_readers = [{ file = "src/reader.rs", reads = 2, owner = "ticket-1", reason = "compares against it" }],
    forbid_new_pattern = [{ name = "narrow-cast", pattern = "$X as u8" }],
    forbid_new_observed = ["byte-width"],
    allowed_in = [{ file = "src/lib.rs", item = "Home", counts = { "" = { narrow-cast = 1 } }, owner = "ticket-2", reason = "the one conversion" }] },
]
stage_order = [{ crate = "fixture", stages = ["parse", "check"],
                 pipeline = { file = "src/lib.rs", function = "run", calls = ["parse::parse", "check::check"], loops = false } }]

[population]
core = ["fixture"]
amended_by = { bead = "ticket-3", covers = ["population"] }

[size]
approve_growth = true

[crate_graph]
rule = [{ name = "fixture", allowed = "any", law = "leaf" }]

[stage_seams.fixture]
floor = []
final_stages = ["check"]

[concentration]
population = ["fixture"]
entry_lines = 300
file_lines = 2000
conventional_names = []
function_lines = 70
function_nesting = 5
generated = [{ path = "src/generated", reason = "written by a build script" }]
function = [{ file = "src/lib.rs", function = "Home::long", lines = 90, nesting = 6, owner = "ticket-4" }]
entry = [{ path = "src/lib.rs", lines = 400, owner = "ticket-5" }]
"#;

#[test]
fn every_extension_field_parses_and_is_unavailable_until_judged() {
    let policy = Policy::parse(EXTENDED).expect("every new field parses");
    let root = fixture("pub struct Home;\n");
    let snapshot = snapshot_workspace(root.path()).unwrap();
    let analysis = Analysis::new(&snapshot).unwrap();
    let report = policy::validate(&analysis, &policy, &Inputs::new(&snapshot)).unwrap();
    assert_eq!(
        report.status(),
        Status::Unavailable,
        "{:?}",
        report.violations
    );
    let fields: Vec<_> = report
        .unavailable
        .iter()
        .map(|m| m.field.as_str())
        .collect();
    for field in [
        "size.approve_growth",
        "concentration.function_lines",
        "concentration.function_nesting",
        "concentration.generated",
        "concentration.function",
        "authority.0.allowed_readers",
        "authority.0.forbid_new_pattern",
        "authority.0.forbid_new_observed",
        "authority.0.allowed_in",
        "stage_order.fixture.pipeline",
        "stage_seams.fixture.final_stages",
    ] {
        assert!(fields.contains(&field), "{field} missing from {fields:?}");
    }
    // Judged fields that name project authority wait for the host to
    // resolve it, like every other owner.
    for (field, written) in [
        ("population.amended_by.bead", "ticket-3"),
        ("concentration.entry.0.owner", "ticket-5"),
    ] {
        let missing = report.unavailable.iter().find(|m| m.field == field);
        assert!(
            missing.is_some_and(|m| m
                .reason
                .contains(&format!("`{written}` has not been resolved"))),
            "{field}: {:?}",
            report
                .unavailable
                .iter()
                .map(|m| (&m.field, &m.reason))
                .collect::<Vec<_>>()
        );
    }
    // `allowed = "any"` is today's meaning of an absent `allowed`: judged.
    assert!(policy.crate_graph.rule[0].allowed.is_none());
    assert!(!fields.iter().any(|f| f.starts_with("crate_graph")));
}

#[test]
fn approve_growth_stores_no_ceilings_and_allowed_names_crates_or_any() {
    let both = EXTENDED.replace(
        "approve_growth = true",
        "approve_growth = true\ncrate = { fixture = 10 }",
    );
    let policy = Policy::parse(&both).expect("parses");
    let members = std::collections::BTreeSet::from(["fixture"]);
    let error = policy
        .validate_configuration(&members)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("approve_growth stores no ceilings"),
        "{error}"
    );
    let word = EXTENDED.replace(r#"allowed = "any""#, r#"allowed = "every""#);
    let error = Policy::parse(&word).unwrap_err().to_string();
    assert!(error.contains(r#"a list of crates or "any""#), "{error}");
}
