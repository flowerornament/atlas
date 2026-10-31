//! Pinned CLI judgment and the wire fields the native landing consumer admits.
#[path = "support/native_delta.rs"]
mod native_delta;
mod support;
use std::process::{Command, Output};
use support::Fixture;
const POLICY: &str = "schema=1\nprovenance='authored'\nsurface={fixture=1}\nauthority=[{fact='home',home='src/lib.rs::Home'}]\n[population]\ncore=['fixture']\n[size]\ncore_token_lines=100\ncrate={fixture=100}\n[crate_graph]\nrule=[{name='fixture',allowed=[],law='leaf'}]\n[concentration]\npopulation=['fixture']\nentry_lines=300\nfile_lines=1000\nconventional_names=[]\n";
fn git(fixture: &Fixture, args: &[&str]) -> String {
    let result = Command::new("git")
        .current_dir(fixture.path())
        .args(args)
        .output()
        .expect("git fixture");
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    String::from_utf8(result.stdout)
        .expect("Git text")
        .trim()
        .into()
}
fn commit(fixture: &Fixture) -> String {
    git(fixture, &["add", "."]);
    git(
        fixture,
        &[
            "-c",
            "user.name=Atlas Test",
            "-c",
            "user.email=atlas@example.invalid",
            "commit",
            "-qm",
            "fixture",
        ],
    );
    git(fixture, &["rev-parse", "HEAD"])
}
fn fixture() -> Fixture {
    let fixture = Fixture::new(&[
        ("src/lib.rs", "//! fixture: meaning\npub struct Home;\n"),
        (".config/architecture.toml", POLICY),
    ]);
    git(&fixture, &["init", "-q"]);
    commit(&fixture);
    fixture
}
fn run(fixture: &Fixture, before: &str, candidate: &str, markers: bool) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_atlas"));
    command
        .current_dir("/")
        .args(["delta", before, "--root"])
        .arg(fixture.path())
        .args([
            "--candidate",
            candidate,
            "--policy",
            ".config/architecture.toml",
            "--json",
        ]);
    if markers {
        command.args(["--citation-markers", "fixture:"]);
    }
    command.output().expect("Atlas process")
}
#[test]
fn json_verdict_retains_exact_identity_raises_and_a_reviewable_display() {
    let fixture = fixture();
    let before = git(&fixture, &["rev-parse", "HEAD"]);
    let output = run(&fixture, &before, &before, true);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["verb"], "delta");
    assert_eq!(result["verdict"], "PASS");
    assert_eq!(result["basis"]["baseline"], before);
    assert_eq!(result["basis"]["candidate"], before);
    assert_eq!(result["policy"]["raises"], serde_json::json!([]));
    assert!(native_delta::admit_delta(&result, &before, &before, false).is_ok());
    assert!(
        result["display"]
            .as_str()
            .unwrap()
            .contains("verdict   PASS")
    );
    assert_eq!(result["policy"]["sha256"].as_str().unwrap().len(), 64);
    assert_eq!(result["size"]["before"], 1);
    assert_eq!(result["size"]["after"], 1);

    fixture.write(
        "src/lib.rs",
        "//! fixture: meaning\npub struct Home;\npub struct Extra;\n",
    );
    let candidate = commit(&fixture);
    let output = run(&fixture, &before, &candidate, true);
    assert!(
        output.status.success(),
        "JSON REFUSE is data for the consumer"
    );
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["verdict"], "REFUSE");
    assert!(matches!(
        native_delta::admit_delta(&result, &before, &candidate, true),
        Err(native_delta::LandingOutcome::Refused(_))
    ));
    fixture.write(
        ".config/architecture.toml",
        &POLICY.replace("surface={fixture=1}", "surface={fixture=2}"),
    );
    let candidate = commit(&fixture);
    let output = run(&fixture, &before, &candidate, true);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["verdict"], "PASS");
    assert_eq!(result["policy"]["raises"][0]["subject"], "surface.fixture");
    assert_eq!(result["policy"]["raises"][0]["measured"], 2);
    assert!(matches!(
        native_delta::admit_delta(&result, &before, &candidate, false),
        Err(native_delta::LandingOutcome::Refused(_))
    ));
    assert!(native_delta::admit_delta(&result, &before, &candidate, true).is_ok());
}
#[test]
fn wrong_candidate_dirty_source_and_missing_authority_cannot_emit_pass() {
    let fixture = fixture();
    let before = git(&fixture, &["rev-parse", "HEAD"]);
    let output = run(&fixture, &before, &before, false);
    assert!(!output.status.success());
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["verdict"], "UNAVAILABLE");
    assert!(matches!(
        native_delta::admit_delta(&result, &before, &before, true),
        Err(native_delta::LandingOutcome::CouldNotDetermine(_))
    ));
    assert!(result["concentration"].is_null());
    fixture.write(
        "src/lib.rs",
        "//! fixture: meaning\npub struct Home;\nstruct Extra;\n",
    );
    let candidate = commit(&fixture);
    let output = run(&fixture, &before, &before, true);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let output = run(&fixture, &before, &candidate, true);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    fixture.write("src/lib.rs", "//! fixture: meaning\npub struct Changed;\n");
    let status = git(&fixture, &["status", "--porcelain"]);
    let output = run(&fixture, &before, &candidate, true);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert_eq!(status, git(&fixture, &["status", "--porcelain"]));
}
#[test]
fn forbidden_path_observations_read_the_real_project_root() {
    let fixture = fixture();
    fixture.write(".gitignore", "legacy/\n");
    fixture.write(
        ".config/architecture.toml",
        &POLICY.replace(
            "home='src/lib.rs::Home'",
            "home='src/lib.rs::Home',forbid_path=['legacy']",
        ),
    );
    let candidate = commit(&fixture);
    fixture.write("legacy/old.txt", "ignored but forbidden");
    let output = run(&fixture, &candidate, &candidate, true);
    assert!(
        output.status.success(),
        "JSON refusal should remain a result: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["verdict"], "REFUSE");
    assert!(
        result["violations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|finding| finding
                .as_str()
                .unwrap()
                .contains("forbidden path `legacy` exists"))
    );
    assert_eq!(
        std::fs::read_to_string(fixture.path().join("legacy/old.txt")).unwrap(),
        "ignored but forbidden"
    );
}
