//! Complete source -> policy -> stage report -> process-boundary claims.
mod support;
use std::process::{Command, Output};
use support::Fixture;

const POLICY: &str = "schema=1\nprovenance='authored'\nsurface={}\nauthority=[]\nstage_order=[{crate='fixture',stages=['load','engine']}]\n[population]\ncore=['fixture']\n[size]\ncore_token_lines=100\ncrate={fixture=100}\n[crate_graph]\nrule=[]\n";
fn fixture() -> Fixture {
    Fixture::new(&[
        ("src/lib.rs", "pub mod load;\npub mod engine;\n"),
        ("src/load.rs", "use crate::engine::Token;\n"),
        ("src/engine.rs", "pub struct Token;\n"),
        ("architecture.toml", POLICY),
    ])
}
fn run(fixture: &Fixture, args: &[&str]) -> Output {
    run_question(fixture, "stages", args)
}
fn run_question(fixture: &Fixture, question: &str, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_atlas"))
        .current_dir("/")
        .arg(question)
        .arg("--root")
        .arg(&fixture.0)
        .args(args)
        .output()
        .expect("run stage command")
}

#[test]
fn stage_command_reports_forward_paths_and_keeps_their_witnesses() {
    let fixture = fixture();
    let output = run(&fixture, &["--policy", "architecture.toml", "--json"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["schema"], 1);
    assert_eq!(report["question"], "stages");
    assert_eq!(report["status"], "report-only");
    assert!(
        report.get("verdict").is_none(),
        "an observation report must not claim complete policy PASS"
    );
    let edge = &report["observations"]["crates"]["fixture"]["forward"][0];
    assert_eq!(edge["from"], "load");
    assert_eq!(edge["to"], "engine");
    assert_eq!(edge["file"], "src/load.rs");
    assert_eq!(edge["line"], 1);
    assert!(
        report["violations"][0]
            .as_str()
            .unwrap()
            .contains("later stage")
    );
    assert!(
        report["basis"]["files"]
            .as_array()
            .unwrap()
            .iter()
            .any(|file| file[1] == "src/load.rs")
    );
    assert!(
        !run(&fixture, &["--json"]).status.success(),
        "policy is explicit"
    );
    assert!(
        !run(
            &fixture,
            &["--policy", "architecture.toml", "--scope", "absent"]
        )
        .status
        .success()
    );
}

#[test]
fn written_authority_is_not_treated_as_resolved_authority() {
    let fixture = fixture();
    fixture.write(
        "architecture.toml",
        &(POLICY.to_owned()
            + "[stage_reads.fixture.engine]\ndebt={load={items=1,owner='not-a-validated-bead'}}\n"),
    );
    let output = run(&fixture, &["--policy", "architecture.toml", "--json"]);
    assert!(!output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["status"], "unavailable");
    assert_eq!(
        report["unresolved_authority"][0]["written"],
        "not-a-validated-bead"
    );
    assert_eq!(
        report["unresolved_authority"][0]["field"],
        serde_json::json!(["stage_reads", "fixture", "engine", "debt", "load", "owner"])
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("native Atlas wrapper"));
}

#[test]
fn malformed_uses_refuse_while_unrelated_test_mounts_stay_diagnostic() {
    let fixture = fixture();
    fixture.write(
        "src/lib.rs",
        "pub mod load;\npub mod engine;\n#[cfg(test)]\n#[path=\"../missing.rs\"]\nmod support;\n",
    );
    let production = run(&fixture, &["--policy", "architecture.toml", "--json"]);
    assert!(
        production.status.success(),
        "an unrelated test population does not disable production observations"
    );
    fixture.write(
        "architecture.toml",
        &(POLICY.to_owned() + "[stage_seams.fixture]\nfloor=[]\n"),
    );
    let missing = run(&fixture, &["--policy", "architecture.toml", "--json"]);
    assert!(
        missing.status.success(),
        "stage paths do not judge the seam-test population"
    );
    let report: serde_json::Value = serde_json::from_slice(&missing.stdout).unwrap();
    assert_eq!(report["status"], "report-only");
    assert!(
        report["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|diagnostic| diagnostic["message"]
                .as_str()
                .is_some_and(|message| message.contains("support")))
    );
    fixture.write("architecture.toml", POLICY);
    fixture.write("src/load.rs", "use crate::engine::{;\n");
    let malformed = run(&fixture, &["--policy", "architecture.toml", "--json"]);
    assert!(!malformed.status.success());
    let report: serde_json::Value = serde_json::from_slice(&malformed.stdout).unwrap();
    assert_eq!(report["status"], "unavailable");
    assert!(
        report["observations"]["unavailable"][0]["message"]
            .as_str()
            .unwrap()
            .contains("syntax")
    );
}

#[test]
fn policy_decoder_refuses_unknown_fields_and_preserves_source_families() {
    let fixture = fixture();
    fixture.write(
        "architecture.toml",
        &(POLICY.to_owned() + "[unrecognized]\nthing=1\n"),
    );
    let output = run(&fixture, &["--policy", "architecture.toml", "--json"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("unknown field"));
    let loaded = atlas_core::checks::policy::Policy::parse(POLICY).unwrap();
    assert_eq!(loaded.population.core, ["fixture"]);
    assert_eq!(loaded.stage_order[0].stages, ["load", "engine"]);
    assert!(loaded.external_references().is_empty());
}

#[test]
fn production_root_names_exclude_test_only_aliases() {
    let fixture = fixture();
    fixture.write("src/lib.rs", "pub mod load;\npub mod engine;\n#[cfg(not(test))]\npub type Alias = crate::load::Input;\n#[cfg(test)]\npub type Alias = crate::engine::Token;\n");
    fixture.write(
        "src/load.rs",
        "pub struct Input;\npub fn run(_: crate::Alias) {}\n",
    );
    let output = run(&fixture, &["--policy", "architecture.toml", "--json"]);
    assert!(output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let reading = &report["observations"]["crates"]["fixture"];
    assert_eq!(reading["forward"], serde_json::json!([]));
    assert_eq!(reading["unjudged"], serde_json::json!([]));
}

#[test]
fn scoped_observations_policy_and_authority_select_the_same_package() {
    let fixture = fixture();
    fixture.write("architecture.toml", &POLICY.replace("stage_order=[{crate='fixture',stages=['load','engine']}]", "stage_order=[{crate='fixture',stages=['load','engine']},{crate='other',stages=['load','engine']}]").replace("[population]", "[stage_reads.other.unknown]\ndebt={engine={items=1,owner='outside-selection'}}\n[population]"));
    let output = run(
        &fixture,
        &[
            "--policy",
            "architecture.toml",
            "--scope",
            "fixture",
            "--json",
        ],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["status"], "report-only");
    assert_eq!(report["unresolved_authority"], serde_json::json!([]));
    assert_eq!(
        report["observations"]["policy_violations"],
        serde_json::json!([])
    );
    assert!(report["observations"]["crates"].get("other").is_none());
    let all = run(&fixture, &["--policy", "architecture.toml", "--json"]);
    assert!(!all.status.success());
    assert!(String::from_utf8_lossy(&all.stderr).contains("not a workspace package"));
}

#[test]
fn competing_root_imports_remain_unjudged_without_cfg_resolution() {
    let fixture = fixture();
    fixture.write("src/lib.rs", "pub mod load;\npub mod engine;\n#[cfg(feature=\"a\")]\npub use engine::Token as Alias;\n#[cfg(not(feature=\"a\"))]\npub use load::Input as Alias;\n");
    fixture.write(
        "src/load.rs",
        "pub struct Input;\npub fn run(_: crate::Alias) {}\n",
    );
    let output = run(&fixture, &["--policy", "architecture.toml", "--json"]);
    assert!(output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(
        report["observations"]["crates"]["fixture"]["unjudged"]
            .as_array()
            .unwrap()
            .iter()
            .any(|issue| issue["why"] == "ambiguous_root_name")
    );
}

#[test]
fn imports_keep_absolute_paths_and_are_retained_with_their_source_witnesses() {
    let source = atlas_core::source::Source::of(
        std::path::Path::new("src/lib.rs"),
        "use ::external::{Thing as Local, other};\n".into(),
    );
    let import = source.imports().next().unwrap();
    assert_eq!(import.line, 1);
    assert_eq!(
        import.bindings.as_ref().unwrap()[0].source_path,
        "::external::Thing"
    );
    assert_eq!(import.bindings.as_ref().unwrap()[0].local_name, "Local");
}

#[test]
fn standalone_seams_preserves_missing_boundary_evidence_as_unavailable() {
    let fixture = fixture();
    fixture.write(
        "architecture.toml",
        &(POLICY.to_owned() + "[stage_seams.fixture]\nfloor=[]\n"),
    );
    let output = run_question(
        &fixture,
        "seams",
        &["--policy", "architecture.toml", "--json"],
    );
    assert!(!output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["question"], "seams");
    assert_eq!(report["status"], "unavailable");
    assert_eq!(report["observations"]["crates"], serde_json::json!([]));
    assert!(
        report["observations"]["unavailable"]
            .as_array()
            .unwrap()
            .iter()
            .any(|issue| issue["message"]
                .as_str()
                .unwrap()
                .contains("boundary declaration unavailable"))
    );
    assert!(report.get("verdict").is_none());
    let detailed = run_question(
        &fixture,
        "seams",
        &["--policy", "architecture.toml", "--items", "--json"],
    );
    assert!(!detailed.status.success());
    let detailed: serde_json::Value =
        serde_json::from_slice(&detailed.stdout).expect("detailed observations");
    assert_eq!(
        detailed, report,
        "presentation cannot supply missing project authority"
    );
}

#[test]
fn absent_seam_rules_do_not_require_unrelated_stage_authority() {
    let fixture = fixture();
    fixture.write(
        "architecture.toml",
        &(POLICY.to_owned()
            + "[stage_reads.fixture.engine]\ndebt={load={items=1,owner='stage-only'}}\n"),
    );
    let output = run_question(
        &fixture,
        "seams",
        &["--policy", "architecture.toml", "--json"],
    );
    assert!(output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["status"], "not-applicable");
    assert_eq!(report["unresolved_authority"], serde_json::json!([]));
    assert!(
        report["observations"]["stages"]["crates"]
            .as_object()
            .unwrap()
            .is_empty()
    );
}
