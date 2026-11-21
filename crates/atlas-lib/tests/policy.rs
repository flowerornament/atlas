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
fn a_missing_forbidden_path_observation_never_means_pass() {
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
}
#[test]
fn a_missing_forbidden_pattern_observation_never_means_pass() {
    let root = fixture("pub struct Home;\n");
    let snapshot = snapshot_workspace(root.path()).unwrap();
    let analysis = Analysis::new(&snapshot).unwrap();
    let mut policy = Policy::parse(POLICY).unwrap();
    policy.authority[0].forbid_path.push("old.rs".into());
    let mut inputs = Inputs::new(&snapshot);
    // The forbidden path is observed absent, so only the pattern decides.
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
                 pipeline = [{ file = "src/lib.rs", function = "run", calls = ["parse::parse", "check::check"], loops = false }] }]

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
writers = { Model = { stage = "parse", reason = "read whole", producers = ["parse"], owner = "ticket-6", production_sites = {} } }

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
    assert!(
        !fields.contains(&"stage_order.fixture.pipeline.0"),
        "pipeline is judged: {fields:?}"
    );
    // `run` does not exist in the fixture: the judged rule says so.
    assert!(
        report.violations.iter().any(|v| v
            .starts_with("stage_order.fixture.pipeline.0 (src/lib.rs::run): ")
            && v.contains("expected one function, found 0")),
        "{:?}",
        report.violations
    );
    for field in [
        "stage_seams.fixture.final_stages",
        "stage_seams.fixture.writers.Model.production_sites",
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
    // `allowed = "any"` is judged: it allows every dependency.
    assert_eq!(
        policy.crate_graph.rule[0].allowed,
        Some(policy::Allowed::Any)
    );
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

/// `policy` judges the crate graph as `delta` does: an allowance the crate
/// does not use refuses without any change to compare.
#[test]
fn policy_judges_the_crate_graph() {
    let root = Fixture::new(&[
        (
            "Cargo.toml",
            "[workspace]\nmembers = ['core', 'app']\nresolver = '3'\n",
        ),
        (
            "core/Cargo.toml",
            "[package]\nname = 'core'\nversion = '0.0.0'\nedition = '2024'\n",
        ),
        ("core/src/lib.rs", "pub struct Home;\n"),
        (
            "app/Cargo.toml",
            "[package]\nname = 'app'\nversion = '0.0.0'\nedition = '2024'\n",
        ),
        ("app/src/lib.rs", "pub fn run() {}\n"),
    ]);
    let text = "schema=1\nprovenance='authored'\nsurface={core=10}\nauthority=[{fact='one home',home='core/src/lib.rs::Home'}]\n[population]\ncore=['core']\n[size]\ncore_token_lines=100\ncrate={core=100}\n[crate_graph]\nrule=[{name='core',allowed=[]},{name='app',allowed=['core']}]\n[concentration]\npopulation=['core']\nentry_lines=300\nfile_lines=2000\nconventional_names=[]\n";
    let snapshot = snapshot_workspace(root.path()).unwrap();
    let analysis = Analysis::new(&snapshot).unwrap();
    let policy = Policy::parse(text).unwrap();
    let report = policy::validate(&analysis, &policy, &Inputs::new(&snapshot)).unwrap();
    assert_eq!(report.status(), Status::Refuse, "{:?}", report.violations);
    assert!(
        report
            .violations
            .iter()
            .any(|row| row.starts_with("UNUSED ALLOWANCE: `app` allows `core`")),
        "{:?}",
        report.violations
    );
}

/// One stage_order per crate, as `stages` already requires: `policy`
/// refuses a second.
#[test]
fn a_duplicate_stage_order_is_refused() {
    let twice = EXTENDED.replace(
        "stage_order = [{ crate = \"fixture\"",
        "stage_order = [{ crate = \"fixture\", stages = [\"parse\"] }, { crate = \"fixture\"",
    );
    assert_ne!(twice, EXTENDED);
    let policy = Policy::parse(&twice).expect("parses");
    let error = policy
        .validate_configuration(&std::collections::BTreeSet::from(["fixture"]))
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("duplicate stage_order for fixture"),
        "{error}"
    );
}

/// An explicitly empty claim ("no final stages", "no production writers")
/// is a claim to judge, unlike a field left out.
#[test]
fn an_explicitly_empty_field_is_a_claim_not_an_absence() {
    let empty = EXTENDED.replace("final_stages = [\"check\"]", "final_stages = []");
    let policy = Policy::parse(&empty).expect("parses");
    assert!(
        policy
            .unjudged()
            .contains(&"stage_seams.fixture.final_stages".to_owned())
    );
    assert!(
        policy
            .unjudged()
            .contains(&"stage_seams.fixture.writers.Model.production_sites".to_owned())
    );
    let absent = EXTENDED.replace("final_stages = [\"check\"]\n", "");
    let policy = Policy::parse(&absent).expect("parses");
    assert!(
        !policy
            .unjudged()
            .iter()
            .any(|field| field.ends_with("final_stages"))
    );
}

/// Which functions `function_lines` reads: a crate's production source and
/// its build script with the modules it declares; not test modules, test
/// functions, test or example targets, nor `generated` paths.
#[test]
fn function_limits_read_production_and_build_scripts_only() {
    let long = "{\n    let a = 1;\n    let b = 2;\n    let c = 3;\n}\n";
    let file = |name: &str| format!("pub fn {name}() {long}");
    let root = Fixture::new(&[
        (
            "Cargo.toml",
            "[package]\nname='fixture'\nversion='0.0.0'\nedition='2024'\n[workspace]\n",
        ),
        (
            "src/lib.rs",
            &format!(
                "pub mod generated;\npub struct Home;\n{}#[cfg(test)]\nmod tests {{\n    fn in_test_module() {long}}}\n#[test]\nfn a_test() {long}",
                file("production")
            ),
        ),
        ("src/generated/mod.rs", &file("written_by_a_tool")),
        ("build.rs", &format!("mod helper;\nfn main() {long}")),
        ("helper.rs", &file("build_helper")),
        ("tests/it.rs", &file("integration")),
        ("examples/demo.rs", &format!("fn main() {long}")),
    ]);
    let text = POLICY.replace(
        "conventional_names=[]\n",
        "conventional_names=[]\nfunction_lines=2\ngenerated=[{path='src/generated',reason='written by a tool'}]\n",
    );
    let snapshot = snapshot_workspace(root.path()).unwrap();
    let analysis = Analysis::new(&snapshot).unwrap();
    let policy = Policy::parse(&text).unwrap();
    let report = policy::validate(&analysis, &policy, &Inputs::new(&snapshot)).unwrap();
    let mut over: Vec<_> = report
        .violations
        .iter()
        .filter_map(|text| text.strip_prefix("function `")?.split('`').next())
        .collect();
    over.sort_unstable();
    assert_eq!(
        over,
        ["build_helper", "main", "production"],
        "{:?}",
        report.violations
    );
}

/// A crate with a home module `src/front/parse.rs`, read from `user.rs`
/// three times (an import, a qualified path, the door), plus test code and
/// a feature-gated read.
fn readers_fixture(user: &str) -> Fixture {
    Fixture::new(&[
        (
            "Cargo.toml",
            "[package]\nname='tutorial'\nversion='0.0.0'\nedition='2024'\n[features]\nextra=[]\n[workspace]\n",
        ),
        (
            "src/lib.rs",
            "pub mod front;\npub mod user;\npub mod other;\npub struct Home;\n",
        ),
        (
            "src/front/mod.rs",
            "pub mod parse;\npub use parse::Tree as Door;\n",
        ),
        (
            "src/front/parse.rs",
            "pub struct Tree;\nimpl Tree { pub fn new() -> Self { Tree } }\npub fn helper() {}\n",
        ),
        ("src/user.rs", user),
        ("src/other.rs", "pub fn quiet() {}\n"),
    ])
}
const USER: &str = "use crate::front::parse::Tree;\npub fn run() {\n    Tree::new();\n    Tree::new();\n    crate::front::parse::helper();\n    crate::front::Door::new();\n}\n#[cfg(test)]\nmod tests { fn t() { crate::front::parse::helper(); } }\n#[test]\nfn a_test() { crate::front::parse::helper(); }\n";
fn readers_policy(rows: &str) -> String {
    POLICY
        .replace("typed_errors=['fixture']\n", "typed_errors=[]\n")
        .replace("'fixture'", "'tutorial'")
        .replace("fixture=", "tutorial=")
        .replace(
            "authority=[{fact='one home',home='src/lib.rs::Home'}]",
            &format!("authority=[{{fact='the parser',home='src/front/parse.rs',allowed_readers=[{rows}]}}]"),
        )
}
fn readers_report(user: &str, rows: &str) -> policy::Report {
    let root = readers_fixture(user);
    let snapshot = snapshot_workspace(root.path()).expect("snapshot");
    let analysis = Analysis::new(&snapshot).expect("analysis");
    let policy = Policy::parse(&readers_policy(rows)).expect("policy parses");
    policy::validate(&analysis, &policy, &Inputs::new(&snapshot)).expect("validation")
}

#[test]
fn allowed_readers_count_exact_production_sites() {
    // The import, the qualified path and the door: 3; the door file is the
    // home's parent and reads it once (its re-export).
    let exact = "{file='src/user.rs',reads=3,reason='runs it'},{file='src/front/mod.rs',reads=1,reason='the door'}";
    let report = readers_report(USER, exact);
    assert_eq!(report.status(), Status::Pass, "{:?}", report.violations);
    let reading = &report.authority_readers["the parser"];
    assert_eq!(reading.home, "tutorial::front::parse");
    assert_eq!(reading.reads["src/user.rs"].len(), 3, "{:?}", reading.reads);
    // Unlisted, the reader refuses with its first site and the row to add.
    let report = readers_report(USER, "{file='src/front/mod.rs',reads=1,reason='the door'}");
    assert!(
        report.violations.iter().any(|v| v.contains(
            "src/user.rs:1 reads the home `src/front/parse.rs` through `crate::front::parse::Tree`"
        ) && v
            .contains(r#"{ file = "src/user.rs", reads = 3, reason = "..." }"#)),
        "{:?}",
        report.violations
    );
    // `[]`: no production reader at all.
    let report = readers_report(USER, "");
    assert_eq!(report.violations.len(), 2, "{:?}", report.violations);
}

#[test]
fn a_row_is_exact_when_owned_and_an_unused_row_refuses() {
    let door = "{file='src/front/mod.rs',reads=1,reason='the door'}";
    let more = readers_report(
        USER,
        &format!("{door},{{file='src/user.rs',reads=2,reason='runs it'}}"),
    );
    assert!(
        more.violations
            .iter()
            .any(|v| v.contains("at 3 sites (1, 5, 6); its row allows 2")),
        "{:?}",
        more.violations
    );
    let fewer = readers_report(
        USER,
        &format!("{door},{{file='src/user.rs',reads=4,reason='runs it'}}"),
    );
    assert!(fewer.violations.is_empty(), "{:?}", fewer.violations);
    assert!(
        fewer
            .hints
            .iter()
            .any(|h| h.contains("can ratchet to reads = 3 (row 4)")),
        "{:?}",
        fewer.hints
    );
    let owned = readers_report(
        USER,
        &format!("{door},{{file='src/user.rs',reads=4,reason='runs it',owner='ticket-1'}}"),
    );
    assert!(
        owned
            .violations
            .iter()
            .any(|v| v.contains("owned by `ticket-1`")),
        "{:?}",
        owned.violations
    );
    // An owner is project authority: unresolved, the check is unavailable.
    assert!(
        owned
            .unavailable
            .iter()
            .any(|m| m.field == "authority.0.allowed_readers.1.owner")
    );
}

#[test]
fn an_unused_row_and_a_row_for_the_home_itself_refuse() {
    let door = "{file='src/front/mod.rs',reads=1,reason='the door'}";
    let unused = readers_report(
        USER,
        &format!(
            "{door},{{file='src/user.rs',reads=3,reason='runs it'}},{{file='src/other.rs',reads=1,reason='once'}}"
        ),
    );
    assert!(
        unused
            .violations
            .iter()
            .any(|v| v.contains("listed reader src/other.rs no longer reads the home")),
        "{:?}",
        unused.violations
    );
    let home_file = readers_report(
        USER,
        &format!(
            "{door},{{file='src/user.rs',reads=3,reason='runs it'}},{{file='src/front/parse.rs',reads=1,reason='itself'}}"
        ),
    );
    assert!(
        home_file
            .violations
            .iter()
            .any(|v| v.contains("is a file of the home")),
        "{:?}",
        home_file.violations
    );
}

#[test]
fn feature_gated_code_reads_and_unreadable_sites_are_unavailable() {
    let door = "{file='src/front/mod.rs',reads=1,reason='the door'}";
    let gated = "#[cfg(feature = \"extra\")]\npub fn extra() { crate::front::parse::helper(); }\n";
    let report = readers_report(
        gated,
        &format!("{door},{{file='src/user.rs',reads=1,reason='the extra feature'}}"),
    );
    assert_eq!(
        report.status(),
        Status::Pass,
        "{:?} {:?}",
        report.violations,
        report
            .unavailable
            .iter()
            .map(|m| &m.reason)
            .collect::<Vec<_>>()
    );
    let spelled = "pub fn run() { custom! { go crate::front::parse::helper } }\n";
    let report = readers_report(spelled, door);
    assert_eq!(report.status(), Status::Unavailable);
    assert!(
        report.unavailable.iter().any(|m| m
            .reason
            .contains("spells `crate::front::parse::helper` in a macro's input")),
        "{:?}",
        report
            .unavailable
            .iter()
            .map(|m| &m.reason)
            .collect::<Vec<_>>()
    );
    let twins = "#[cfg(unix)]\nuse crate::front::parse as p;\n#[cfg(windows)]\nuse crate::other as p;\npub fn run() { p::helper(); }\n";
    let report = readers_report(
        twins,
        &format!("{door},{{file='src/user.rs',reads=1,reason='unix'}}"),
    );
    assert!(
        report
            .unavailable
            .iter()
            .any(|m| m.reason.contains("`p::helper` may read the home")),
        "{:?}",
        report
            .unavailable
            .iter()
            .map(|m| &m.reason)
            .collect::<Vec<_>>()
    );
}

#[test]
fn an_item_home_is_read_through_its_item() {
    let text = readers_policy("{file='src/user.rs',reads=3,reason='runs it'},{file='src/front/mod.rs',reads=1,reason='the door'}")
        .replace("home='src/front/parse.rs'", "home='src/front/parse.rs::Tree'");
    let root = readers_fixture(USER);
    let snapshot = snapshot_workspace(root.path()).unwrap();
    let analysis = Analysis::new(&snapshot).unwrap();
    let report = policy::validate(
        &analysis,
        &Policy::parse(&text).unwrap(),
        &Inputs::new(&snapshot),
    )
    .unwrap();
    // `helper` is not `Tree`: user.rs reads the item through the import
    // and the door only, so its row of 3 can fall to 2.
    assert!(report.violations.is_empty(), "{:?}", report.violations);
    assert_eq!(
        report.authority_readers["the parser"].home,
        "tutorial::front::parse::Tree"
    );
    assert!(
        report
            .hints
            .iter()
            .any(|h| h.contains("src/user.rs can ratchet to reads = 2")),
        "{:?}",
        report.hints
    );
}

/// A file outside `src/` mounted with `#[path]` behind a feature gate is
/// production code, so its reads count, under its own path.
#[test]
fn a_feature_gated_path_mount_outside_src_is_a_reader() {
    let root = readers_fixture("pub fn run() {}\n");
    std::fs::write(
        root.path().join("src/lib.rs"),
        "pub mod front;\npub mod user;\npub mod other;\npub struct Home;\n#[cfg(feature = \"extra\")]\n#[path = \"../support/extra.rs\"]\nmod extra;\n",
    )
    .unwrap();
    std::fs::create_dir_all(root.path().join("support")).unwrap();
    std::fs::write(
        root.path().join("support/extra.rs"),
        "pub fn go() { crate::front::parse::helper(); }\n",
    )
    .unwrap();
    let text = readers_policy(
        "{file='src/front/mod.rs',reads=1,reason='the door'},{file='support/extra.rs',reads=1,reason='the extra feature'}",
    );
    let snapshot = snapshot_workspace(root.path()).unwrap();
    let analysis = Analysis::new(&snapshot).unwrap();
    let report = policy::validate(
        &analysis,
        &Policy::parse(&text).unwrap(),
        &Inputs::new(&snapshot),
    )
    .unwrap();
    assert_eq!(report.status(), Status::Pass, "{:?}", report.violations);
    assert_eq!(
        report.authority_readers["the parser"].reads["support/extra.rs"].len(),
        1
    );
}
