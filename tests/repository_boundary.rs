//! A caller's Git environment must not redefine the requested source repository.
mod support;
use std::process::Command;
use support::Fixture;

fn commit(fixture: &Fixture) -> String {
    let git = |args: &[&str]| {
        let output = Command::new("git")
            .current_dir(fixture.path())
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_INDEX_FILE")
            .env_remove("GIT_COMMON_DIR")
            .args(args)
            .output()
            .expect("Git fixture");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout)
            .expect("Git text")
            .trim()
            .to_owned()
    };
    git(&["init", "-q"]);
    git(&["add", "."]);
    git(&[
        "-c",
        "user.name=Atlas Test",
        "-c",
        "user.email=atlas@example.invalid",
        "commit",
        "-qm",
        "fixture",
    ]);
    git(&["rev-parse", "HEAD"])
}

#[test]
fn inherited_git_environment_cannot_redirect_revision_identity() {
    let intended = Fixture::new(&[("src/lib.rs", "pub struct Intended;\n")]);
    let foreign = Fixture::new(&[("src/lib.rs", "pub struct Foreign;\n")]);
    let expected = commit(&intended);
    let other = commit(&foreign);
    assert_ne!(expected, other);
    let output = Command::new(env!("CARGO_BIN_EXE_cargo-atlas"))
        .current_dir("/")
        .args(["diff", "HEAD", "--root"])
        .arg(intended.path())
        .arg("--json")
        .env("GIT_DIR", foreign.path().join(".git"))
        .env("GIT_WORK_TREE", foreign.path())
        .env("GIT_COMMON_DIR", foreign.path().join(".git"))
        .env("GIT_INDEX_FILE", foreign.path().join(".git/index"))
        .output()
        .expect("Atlas process");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).expect("JSON report");
    assert_eq!(report["basis"]["before"], expected);
    assert!(
        report["report"]["changes"]
            .as_array()
            .expect("changes")
            .is_empty()
    );
}

#[test]
fn a_revision_extracted_inside_another_workspace_is_refused_by_name() {
    // A standalone package; its revision is extracted under a TMPDIR that an
    // unrelated workspace encloses, so Cargo would claim it.
    let project = Fixture::new(&[("src/lib.rs", "pub fn value() {}\n")]);
    project.write(
        "Cargo.toml",
        "[package]\nname = \"standalone\"\nversion = \"0.0.0\"\nedition = \"2024\"\n",
    );
    commit(&project);
    let outer = Fixture::new(&[]);
    outer.write("Cargo.toml", "[workspace]\nmembers = []\n");
    let temp = outer.path().join("tmp");
    std::fs::create_dir_all(&temp).expect("temp directory");
    let output = Command::new(env!("CARGO_BIN_EXE_cargo-atlas"))
        .current_dir(project.path())
        .args(["diff", "HEAD", "--root"])
        .arg(project.path())
        .env("TMPDIR", &temp)
        .output()
        .expect("Atlas");
    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains("inside the Cargo workspace"), "{error}");
    assert!(error.contains("atlas-revision-"), "{error}");
    assert!(error.contains("TMPDIR"), "{error}");
}

#[test]
fn a_revision_with_its_own_workspace_is_reviewed_inside_another() {
    // The fixture's manifest declares its own `[workspace]`.
    let project = Fixture::new(&[("src/lib.rs", "pub fn value() {}\n")]);
    commit(&project);
    let outer = Fixture::new(&[]);
    outer.write("Cargo.toml", "[workspace]\nmembers = []\n");
    let temp = outer.path().join("tmp");
    std::fs::create_dir_all(&temp).expect("temp directory");
    let output = Command::new(env!("CARGO_BIN_EXE_cargo-atlas"))
        .current_dir(project.path())
        .args(["diff", "HEAD", "--root"])
        .arg(project.path())
        .env("TMPDIR", &temp)
        .output()
        .expect("Atlas");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
