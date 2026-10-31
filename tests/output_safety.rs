//! Every output plan refuses to replace its source or query inputs before writing.
mod support;
use std::{fs, process::Command};
use support::Fixture;
fn fixture() -> Fixture {
    let f = Fixture::new(&[("src/lib.rs", "pub fn run() { let x = 1; }\n")]);
    f.write("profile.json",r#"{"schema":1,"source":["src/lib.rs"],"modules":[{"name":"body","kind":"stage","path":"src/lib.rs","entry":"run","types":[],"dependencies":[]}],"workflows":[{"name":"run","path":"src/lib.rs","function":"run"}]}"#);
    f
}
fn refuses(f: &Fixture, args: &[&str], protected: &str) {
    let before = fs::read(f.path().join(protected)).expect("output fixture operation");
    let result = Command::new(env!("CARGO_BIN_EXE_atlas"))
        .args(args)
        .arg("--root")
        .arg(f.path())
        .output()
        .expect("output fixture operation");
    assert!(!result.status.success(), "unexpected success: {args:?}");
    assert!(
        String::from_utf8_lossy(&result.stderr).contains("collision"),
        "wrong refusal: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        fs::read(f.path().join(protected)).expect("output fixture operation"),
        before,
        "input was changed"
    );
    assert!(
        !f.path().join("target/atlas").exists(),
        "refusal wrote evidence"
    );
}
#[test]
fn map_and_workflow_refuse_primary_source_and_profile_destinations() {
    for (args, protected) in [
        (
            vec![
                "program",
                "--architecture",
                "profile.json",
                "--path",
                "src/lib.rs",
                "--function",
                "run",
                "--out",
                "src/lib.rs",
            ],
            "src/lib.rs",
        ),
        (
            vec![
                "flow",
                "--path",
                "src/lib.rs",
                "--function",
                "run",
                "--depth",
                "1",
                "--out",
                "src/lib.rs",
            ],
            "src/lib.rs",
        ),
        (
            vec!["map", "--backend", "ascii", "--out", "src/lib.rs"],
            "src/lib.rs",
        ),
        (
            vec![
                "flow",
                "--path",
                "src/lib.rs",
                "--function",
                "run",
                "--out",
                "src/lib.rs",
            ],
            "src/lib.rs",
        ),
        (
            vec![
                "map",
                "--architecture",
                "profile.json",
                "--out",
                "profile.json",
            ],
            "profile.json",
        ),
        (
            vec![
                "flow",
                "--architecture",
                "profile.json",
                "--out",
                "profile.json",
            ],
            "profile.json",
        ),
    ] {
        let f = fixture();
        refuses(&f, &args, protected);
    }
}
#[test]
fn evidence_cannot_replace_an_architecture_input() {
    let f = fixture();
    let profile =
        fs::read_to_string(f.path().join("profile.json")).expect("output fixture operation");
    f.write("view.diagram.json", &profile);
    refuses(
        &f,
        &[
            "map",
            "--architecture",
            "view.diagram.json",
            "--evidence-dir",
            ".",
            "--out",
            "view.txt",
        ],
        "view.diagram.json",
    );
    assert!(!f.path().join("view.txt").exists());
    assert!(!f.path().join("view.source.json").exists());
}
#[cfg(unix)]
#[test]
fn output_alias_to_source_is_refused() {
    let f = fixture();
    std::os::unix::fs::symlink(f.path().join("src/lib.rs"), f.path().join("alias.txt"))
        .expect("output fixture operation");
    refuses(&f, &["map", "--out", "alias.txt"], "src/lib.rs");
}
#[test]
fn ordinary_artifacts_still_write_with_their_evidence() {
    let f = fixture();
    let result = Command::new(env!("CARGO_BIN_EXE_atlas"))
        .args(["map", "--out", "docs/view.txt", "--root"])
        .arg(f.path())
        .output()
        .expect("output fixture operation");
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(f.path().join("docs/view.txt").is_file());
    assert!(f.path().join("target/atlas/view.diagram.json").is_file());
}

#[test]
fn nonexistent_parent_traversal_cannot_alias_source() {
    let f = fixture();
    refuses(&f, &["map", "--out", "missing/../src/lib.rs"], "src/lib.rs");
    assert!(!f.path().join("missing").exists());
}
#[test]
fn cargo_manifest_is_an_input_not_an_output() {
    let f = fixture();
    refuses(&f, &["map", "--out", "Cargo.toml"], "Cargo.toml");
}
#[test]
fn writing_an_uninspected_hardlink_alias_cannot_change_source_bytes() {
    let f = fixture();
    let before = fs::read(f.path().join("src/lib.rs")).expect("source");
    fs::hard_link(f.path().join("src/lib.rs"), f.path().join("alias.txt")).expect("hardlink");
    let result = Command::new(env!("CARGO_BIN_EXE_atlas"))
        .args(["map", "--out", "alias.txt", "--root"])
        .arg(f.path())
        .output()
        .expect("Atlas");
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        fs::read(f.path().join("src/lib.rs")).expect("source"),
        before
    );
}

#[test]
fn expanded_workflow_protects_captured_callee_source() {
    let f = fixture();
    f.write(
        "src/lib.rs",
        "mod helper;\npub fn run(){helper::inner();}\n",
    );
    f.write("src/helper.rs", "pub fn inner() {}\n");
    refuses(
        &f,
        &[
            "flow",
            "--path",
            "src/lib.rs",
            "--function",
            "run",
            "--depth",
            "1",
            "--out",
            "src/helper.rs",
        ],
        "src/helper.rs",
    );
}
#[test]
fn output_and_evidence_directory_cannot_be_the_same_path() {
    let f = fixture();
    let result = Command::new(env!("CARGO_BIN_EXE_atlas"))
        .args([
            "map",
            "--out",
            "artifact.txt",
            "--evidence-dir",
            "artifact.txt",
            "--root",
        ])
        .arg(f.path())
        .output()
        .expect("Atlas");
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("collision"));
    assert!(!f.path().join("artifact.txt").exists());
}
#[test]
fn existing_directory_destination_refuses_before_evidence() {
    let f = fixture();
    fs::create_dir(f.path().join("view.txt")).expect("directory");
    let result = Command::new(env!("CARGO_BIN_EXE_atlas"))
        .args(["map", "--out", "view.txt", "--root"])
        .arg(f.path())
        .output()
        .expect("Atlas");
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("not a regular file"));
    assert!(!f.path().join("target/atlas").exists());
}
#[cfg(unix)]
#[test]
fn artifact_permissions_follow_create_defaults_and_preserve_existing_mode() {
    use std::os::unix::fs::PermissionsExt;
    let f = fixture();
    f.write("control.txt", "control");
    let invoke = || {
        Command::new(env!("CARGO_BIN_EXE_atlas"))
            .args(["map", "--out", "view.txt", "--root"])
            .arg(f.path())
            .output()
            .expect("Atlas")
    };
    assert!(invoke().status.success());
    let mode = |path: &str| {
        fs::metadata(f.path().join(path))
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777
    };
    assert_eq!(mode("view.txt"), mode("control.txt"));
    fs::set_permissions(f.path().join("view.txt"), fs::Permissions::from_mode(0o640))
        .expect("mode");
    assert!(invoke().status.success());
    assert_eq!(mode("view.txt"), 0o640);
}

#[test]
fn unexpanded_workflow_protects_other_project_sources_without_parsing_their_ast() {
    let f = fixture();
    f.write("src/transformer.rs", "pub fn transformer() {}\n");
    f.write("src/unrelated.rs", "pub fn {\n");
    refuses(
        &f,
        &[
            "flow",
            "--path",
            "src/lib.rs",
            "--function",
            "run",
            "--out",
            "src/transformer.rs",
        ],
        "src/transformer.rs",
    );
    let output = Command::new(env!("CARGO_BIN_EXE_atlas"))
        .args([
            "flow",
            "--path",
            "src/lib.rs",
            "--function",
            "run",
            "--root",
        ])
        .arg(f.path())
        .output()
        .expect("Atlas");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
