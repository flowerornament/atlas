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
    let mut command = Command::new(env!("CARGO_BIN_EXE_cargo-atlas"));
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
#[test]
fn forbid_new_pattern_counts_both_sides_with_their_own_policies() {
    // The command line observes forbid_new_pattern with ast-grep, which
    // contributors need not install; CI installs it so this runs there.
    if Command::new("ast-grep").arg("--version").output().is_err() {
        return;
    }
    let guarded = POLICY.replace(
        "home='src/lib.rs::Home'",
        "home='src/lib.rs::Home',forbid_new_pattern=[{name='narrow',pattern='$X as u8'}]",
    );
    let fixture = fixture();
    fixture.write(".config/architecture.toml", &guarded);
    let one =
        "//! fixture: meaning\npub struct Home;\npub fn render(x: u32) -> u8 {\n    x as u8\n}\n";
    fixture.write("src/lib.rs", one);
    let base = commit(&fixture);
    fixture.write("src/lib.rs", &format!("{one}\n"));
    let same = commit(&fixture);
    let held = run(&fixture, &base, &same, true);
    let result: serde_json::Value = serde_json::from_slice(&held.stdout).unwrap();
    let violations = result["violations"].to_string();
    assert!(!violations.contains("narrow"), "{violations}");
    assert!(
        result["unavailable"]
            .to_string()
            .matches("forbid_new")
            .count()
            == 0,
        "{}",
        result["unavailable"]
    );
    assert_eq!(result["spellings"]["home"]["debt_after"], 1, "{result}");
    assert!(
        result["display"]
            .as_str()
            .unwrap()
            .contains("spelling  home: debt 1 -> 1 outside the home"),
        "{}",
        result["display"]
    );
    fixture.write(
        "src/lib.rs",
        &one.replace("    x as u8\n", "    (x as u8).wrapping_add(x as u8)\n"),
    );
    let grown = commit(&fixture);
    let output = run(&fixture, &same, &grown, true);
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["verdict"], "REFUSE", "{result}");
    assert!(
        result["violations"]
            .to_string()
            .contains("`src/lib.rs::render` holds 2 `narrow` outside its home (base 1)"),
        "{}",
        result["violations"]
    );
}

#[test]
fn delta_prints_the_policy_hints_in_text() {
    let fixture = fixture();
    fixture.write(
        "src/lib.rs",
        "//! fixture: meaning\npub struct Home;\npub mod reader;\n",
    );
    fixture.write(
        "src/reader.rs",
        "//! fixture: meaning\nuse crate::Home;\npub fn read(_: &Home) {}\n",
    );
    fixture.write(
        ".config/architecture.toml",
        &POLICY.replace(
            "home='src/lib.rs::Home'",
            "home='src/lib.rs::Home',allowed_readers=[{file='src/reader.rs',reads=2,reason='reads it'}]",
        ).replace("surface={fixture=1}", "surface={fixture=2}"),
    );
    let candidate = commit(&fixture);
    let output = Command::new(env!("CARGO_BIN_EXE_cargo-atlas"))
        .current_dir("/")
        .args(["delta", &candidate, "--root"])
        .arg(fixture.path())
        .args([
            "--candidate",
            &candidate,
            "--policy",
            ".config/architecture.toml",
            "--citation-markers",
            "fixture:",
        ])
        .output()
        .expect("Atlas process");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        text.contains(
            "hint      authority `home`: the row for src/reader.rs can ratchet to reads = 1 (row 2)"
        ),
        "{text}"
    );
}

#[test]
fn a_policy_missing_at_the_baseline_names_the_baseline() {
    let fixture = Fixture::new(&[("src/lib.rs", "//! fixture: meaning\npub struct Home;\n")]);
    git(&fixture, &["init", "-q"]);
    let base = commit(&fixture);
    fixture.write(".config/architecture.toml", POLICY);
    let candidate = commit(&fixture);
    let output = run(&fixture, &base, &candidate, true);
    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(
        error.contains(&format!(
            "the policy .config/architecture.toml is not in the baseline {base} ({})",
            &base[..12]
        )) && error.contains("run `cargo atlas policy` instead"),
        "{error}"
    );
}
