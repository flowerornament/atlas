//! A caller's Git environment must not redefine the requested source repository.
mod support;
use std::process::Command;
use support::Fixture;

fn commit(fixture: &Fixture) -> String {
    let git = |args: &[&str]| {
        let output = Command::new("git")
            .current_dir(&fixture.0)
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
    let output = Command::new(env!("CARGO_BIN_EXE_atlas"))
        .current_dir("/")
        .args(["diff", "HEAD", "--root"])
        .arg(&intended.0)
        .arg("--json")
        .env("GIT_DIR", foreign.0.join(".git"))
        .env("GIT_WORK_TREE", &foreign.0)
        .env("GIT_COMMON_DIR", foreign.0.join(".git"))
        .env("GIT_INDEX_FILE", foreign.0.join(".git/index"))
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
