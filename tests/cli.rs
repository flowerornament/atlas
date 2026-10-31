//! Installed-command contracts: shared workspace root, evidence, and explicit syntax limits.
mod support;
use std::{
    fs,
    process::{Command, Output},
};
use support::Fixture;
fn run(f: &Fixture, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_atlas"))
        .current_dir("/")
        .arg(args[0])
        .arg(f.path())
        .args(&args[1..])
        .output()
        .expect("test fixture prerequisite must succeed")
}
fn fixture() -> Fixture {
    Fixture::new(&[(
        "src/lib.rs",
        "pub struct Value;\npub fn first(x: Value) -> Value { let y = second(x)?; y }\npub fn second(x: Value) -> Value { x }\n",
    )])
}
#[test]
fn commands_expose_source_data_and_refuse_missing_names() {
    let f = fixture();
    for command in ["census", "dump"] {
        let output = run(&f, &[command, "--json"]);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert!(value.is_object());
    }
    for args in [
        ["name", "Value", "--json"],
        ["related", "Value", "--json"],
        ["module", "src", "--json"],
    ] {
        let output = run(&f, &args);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert!(value["basis"].as_str().unwrap().contains("Lexical"));
    }
    assert!(!run(&f, &["name", "absent"]).status.success());
    let legacy = run(&f, &["related", "Value"]);
    let canonical = run(&f, &["name", "Value", "--relations"]);
    assert_eq!(legacy.stdout, canonical.stdout);
    assert!(String::from_utf8_lossy(&legacy.stderr).contains("compatibility alias"));
    let version = Command::new(env!("CARGO_BIN_EXE_atlas"))
        .arg("--version")
        .output()
        .unwrap();
    assert!(version.status.success());
    assert!(String::from_utf8_lossy(&version.stdout).starts_with("atlas "));
}
#[test]
fn workflows_preserve_independent_bodies_and_raw_source_evidence() {
    let f = fixture();
    f.write("profile.json",r#"{"schema":1,"source":["src/lib.rs"],"modules":[],"workflows":[{"name":"first","path":"src/lib.rs","function":"first"},{"name":"second","path":"src/lib.rs","function":"second"}]}"#);
    let output = run(
        &f,
        &[
            "flow",
            "--architecture",
            "profile.json",
            "--out",
            "docs/flows.txt",
        ],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = fs::read_to_string(f.path().join("docs/flows.txt")).unwrap();
    assert!(
        text.contains("first\nfirst (src/lib.rs:2)")
            && text.contains("second\nsecond (src/lib.rs:3)")
    );
    assert!(
        text.contains("second(x)")
            && text.contains("propagate ?")
            && text.contains("not a resolved call graph")
    );
    let diagram: atlas::diagram::Diagram = serde_json::from_str(
        &fs::read_to_string(f.path().join("target/atlas/flows.diagram.json")).unwrap(),
    )
    .unwrap();
    diagram.validate().unwrap();
    assert!(
        diagram
            .edges
            .iter()
            .all(|e| e.from.split('_').next() == e.to.split('_').next())
    );
    let raw = fs::read_to_string(f.path().join("target/atlas/flows.workflow.json")).unwrap();
    assert!(raw.contains("fallible") && raw.contains("src/lib.rs"));
    assert!(!raw.contains(f.path().to_str().unwrap()));
    let direct = run(&f, &["flow", "--path", "src/lib.rs", "--function", "first"]);
    assert!(direct.status.success());
    assert!(!String::from_utf8_lossy(&direct.stdout).contains("second\nsecond ("));
    assert!(
        !run(&f, &["flow", "absent", "--architecture", "profile.json"])
            .status
            .success()
    );
    assert!(
        !run(&f, &["map", "--architecture", "profile.json"])
            .status
            .success()
    );
}
#[test]
fn member_invocations_resolve_workflows_and_outputs_at_workspace_root() {
    let f = fixture();
    f.write("Cargo.toml","[package]\nname='fixture'\nversion='0.1.0'\nedition='2024'\n[workspace]\nmembers=['member']\n");
    f.write(
        "member/Cargo.toml",
        "[package]\nname='member'\nversion='0.1.0'\nedition='2024'\n",
    );
    f.write("member/src/lib.rs", "pub struct Member;\n");
    let output = Command::new(env!("CARGO_BIN_EXE_atlas"))
        .current_dir("/")
        .arg("flow")
        .arg(f.path().join("member"))
        .args([
            "--path",
            "src/lib.rs",
            "--function",
            "first",
            "--out",
            "docs/root.txt",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(f.path().join("docs/root.txt").is_file());
    assert!(!f.path().join("member/docs/root.txt").exists());
}

#[test]
fn representation_query_outputs_are_explicit_and_cannot_overwrite_inputs() {
    let f = Fixture::new(&[(
        "src/lib.rs",
        "pub struct Input;\npub struct Output;\npub fn make(_: Input)->Output {Output}\n",
    )]);
    let query = r#"{"title":"CLI contracts","types":[{"name":"Input"},{"name":"Output"}]}"#;
    f.write("query.json", query);
    let exe = env!("CARGO_BIN_EXE_atlas");
    let run = |args: &[&str]| {
        std::process::Command::new(exe)
            .arg("representations")
            .arg(f.path())
            .args(args)
            .output()
            .unwrap()
    };
    let ascii = run(&["--query", "query.json"]);
    assert!(ascii.status.success());
    assert!(String::from_utf8_lossy(&ascii.stdout).contains("make"));
    let denied = run(&["--query", "query.json", "--out", "query.txt"]);
    assert!(!denied.status.success());
    assert_eq!(
        std::fs::read_to_string(f.path().join("query.json")).unwrap(),
        query
    );
    assert!(!f.path().join("query.txt").exists());
    let missing = run(&["--query", "query.json", "--backend", "pele"]);
    assert!(missing.status.success());
    let pele = run(&[
        "--query",
        "query.json",
        "--backend",
        "pele",
        "--out",
        "target/views/figure.svg",
    ]);
    assert!(
        pele.status.success(),
        "{}",
        String::from_utf8_lossy(&pele.stderr)
    );
    for name in ["figure.svg", "figure.mmd", "figure.json"] {
        assert!(f.path().join("target/views").join(name).exists());
    }
    assert_eq!(
        std::fs::read_to_string(f.path().join("src/lib.rs")).unwrap(),
        "pub struct Input;\npub struct Output;\npub fn make(_: Input)->Output {Output}\n"
    );
}

#[test]
fn canonical_root_and_current_directory_preserve_existing_queries() {
    let f = fixture();
    for (legacy, named) in [
        (
            vec!["census", "src", "--json"],
            vec!["census", "--scope", "src", "--json"],
        ),
        (
            vec!["name", "Value", "--json"],
            vec!["name", "Value", "--json"],
        ),
        (
            vec!["module", "src", "--json"],
            vec!["module", "src", "--json"],
        ),
        (
            vec!["map", "src", "--level", "files"],
            vec!["map", "--scope", "src", "--level", "files"],
        ),
        (
            vec!["dependencies", "src", "--relation", "signatures", "--json"],
            vec![
                "dependencies",
                "--scope",
                "src",
                "--relation",
                "signatures",
                "--json",
            ],
        ),
    ] {
        let old = run(&f, &legacy);
        let new = Command::new(env!("CARGO_BIN_EXE_atlas"))
            .current_dir("/")
            .arg("--root")
            .arg(f.path())
            .args(&named)
            .output()
            .expect("canonical invocation");
        assert!(
            old.status.success(),
            "legacy {:?}: {}",
            legacy,
            String::from_utf8_lossy(&old.stderr)
        );
        assert!(
            new.status.success(),
            "canonical {:?}: {}",
            named,
            String::from_utf8_lossy(&new.stderr)
        );
        assert_eq!(old.stdout, new.stdout, "{named:?}");
    }
    for command in ["spans", "nontest-loc", "concentration"] {
        let positional = Command::new(env!("CARGO_BIN_EXE_atlas"))
            .current_dir(f.path())
            .args([command, "src", "--json"])
            .output()
            .expect("positional scope");
        let named = Command::new(env!("CARGO_BIN_EXE_atlas"))
            .current_dir("/")
            .arg(command)
            .arg("--root")
            .arg(f.path())
            .args(["--scope", "src", "--json"])
            .output()
            .expect("named scope");
        assert!(
            positional.status.success(),
            "{}",
            String::from_utf8_lossy(&positional.stderr)
        );
        assert!(
            named.status.success(),
            "{}",
            String::from_utf8_lossy(&named.stderr)
        );
        assert_eq!(positional.stdout, named.stdout, "{command}");
    }
    let default_root = Command::new(env!("CARGO_BIN_EXE_atlas"))
        .current_dir(f.path())
        .args(["census", "--json"])
        .output()
        .expect("current directory invocation");
    assert!(default_root.status.success());
    assert_eq!(default_root.stdout, run(&f, &["census", "--json"]).stdout);
    let conflict = Command::new(env!("CARGO_BIN_EXE_atlas"))
        .current_dir("/")
        .args(["census", "/", "--root"])
        .arg(f.path())
        .output()
        .expect("root conflict");
    assert!(!conflict.status.success());
    assert!(String::from_utf8_lossy(&conflict.stderr).contains("conflicts with --root"));
    assert!(conflict.stdout.is_empty());
    let same = Command::new(env!("CARGO_BIN_EXE_atlas"))
        .current_dir("/")
        .arg("census")
        .arg(f.path())
        .arg("--root")
        .arg(f.path())
        .arg("--json")
        .output()
        .expect("same root aliases");
    assert!(same.status.success());
}

#[test]
fn named_and_positional_workflows_use_the_same_output_identity() {
    let f = fixture();
    f.write("profile.json", r#"{"schema":1,"source":["src/lib.rs"],"modules":[],"workflows":[{"name":"selected","path":"src/lib.rs","function":"first"}]}"#);
    let legacy = run(&f, &["flow", "selected", "--architecture", "profile.json"]);
    assert!(
        legacy.status.success(),
        "{}",
        String::from_utf8_lossy(&legacy.stderr)
    );
    let files = || {
        let mut names: Vec<_> = fs::read_dir(f.path().join("target/atlas"))
            .expect("evidence directory")
            .map(|entry| entry.expect("evidence file").file_name())
            .collect();
        names.sort();
        names
    };
    let original_files = files();
    assert!(original_files.iter().any(|name| {
        name.to_str()
            .expect("evidence filename")
            .starts_with("selected-")
    }));
    fs::remove_dir_all(f.path().join("target/atlas")).expect("clear owned fixture artifacts");
    let named = Command::new(env!("CARGO_BIN_EXE_atlas"))
        .current_dir("/")
        .arg("flow")
        .arg("--root")
        .arg(f.path())
        .args(["--anchor", "selected", "--architecture", "profile.json"])
        .output()
        .expect("canonical workflow");
    assert!(
        named.status.success(),
        "{}",
        String::from_utf8_lossy(&named.stderr)
    );
    assert_eq!(legacy.stdout, named.stdout);
    assert_eq!(original_files, files());
}
