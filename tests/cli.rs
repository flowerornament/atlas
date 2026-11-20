//! Installed-command contracts: shared workspace root, evidence, and explicit syntax limits.
mod support;
use std::{
    fs,
    process::{Command, Output},
};
use support::Fixture;
fn run(f: &Fixture, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_cargo-atlas"))
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
    let version = Command::new(env!("CARGO_BIN_EXE_cargo-atlas"))
        .arg("--version")
        .output()
        .unwrap();
    assert!(version.status.success());
    assert!(String::from_utf8_lossy(&version.stdout).starts_with("cargo-atlas "));
    // `cargo atlas census` reaches the binary as `cargo-atlas atlas census`.
    let census = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_cargo-atlas"))
            .args(args)
            .arg("--root")
            .arg(f.path())
            .args(["census", "--json"])
            .output()
            .unwrap()
    };
    let direct = census(&[]);
    let through_cargo = census(&["atlas"]);
    assert!(direct.status.success() && through_cargo.status.success());
    assert_eq!(direct.stdout, through_cargo.stdout);
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
    // An anchor named like its function is headed once.
    assert!(
        text.contains("first (src/lib.rs:2)")
            && text.contains("second (src/lib.rs:3)")
            && !text.contains("first\nfirst")
    );
    assert!(
        text.contains("second(x)")
            && text.contains("propagate ?")
            && text.contains("not a resolved call graph")
    );
    let diagram: cargo_atlas::diagram::Diagram = serde_json::from_str(
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
    let output = Command::new(env!("CARGO_BIN_EXE_cargo-atlas"))
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
    let exe = env!("CARGO_BIN_EXE_cargo-atlas");
    let run = |args: &[&str]| {
        std::process::Command::new(exe)
            .arg("types")
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
        let new = Command::new(env!("CARGO_BIN_EXE_cargo-atlas"))
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
        let positional = Command::new(env!("CARGO_BIN_EXE_cargo-atlas"))
            .current_dir(f.path())
            .args([command, "src", "--json"])
            .output()
            .expect("positional scope");
        let named = Command::new(env!("CARGO_BIN_EXE_cargo-atlas"))
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
    let default_root = Command::new(env!("CARGO_BIN_EXE_cargo-atlas"))
        .current_dir(f.path())
        .args(["census", "--json"])
        .output()
        .expect("current directory invocation");
    assert!(default_root.status.success());
    assert_eq!(default_root.stdout, run(&f, &["census", "--json"]).stdout);
    let conflict = Command::new(env!("CARGO_BIN_EXE_cargo-atlas"))
        .current_dir("/")
        .args(["census", "/", "--root"])
        .arg(f.path())
        .output()
        .expect("root conflict");
    assert!(!conflict.status.success());
    assert!(String::from_utf8_lossy(&conflict.stderr).contains("conflicts with --root"));
    assert!(conflict.stdout.is_empty());
    let same = Command::new(env!("CARGO_BIN_EXE_cargo-atlas"))
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
    let named = Command::new(env!("CARGO_BIN_EXE_cargo-atlas"))
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

#[test]
fn scoped_dependency_evidence_keeps_only_incident_facts_unless_asked() {
    let f = Fixture::new(&[
        ("src/lib.rs", "pub mod a;\npub mod b;\npub mod c;\n"),
        (
            "src/a.rs",
            "pub fn call_b() { crate::b::work(); missing::gone(); }\n",
        ),
        ("src/b.rs", "pub fn work() {}\n"),
        (
            "src/c.rs",
            "pub fn alone() { crate::c::other(); }\npub fn other() {}\n",
        ),
    ]);
    let facts = |extra: &[&str]| -> serde_json::Value {
        let output = Command::new(env!("CARGO_BIN_EXE_cargo-atlas"))
            .current_dir("/")
            .arg("--root")
            .arg(f.path())
            .args(["dependencies", "--scope", "src/a.rs", "--json"])
            .args(extra)
            .output()
            .expect("dependencies");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let report: serde_json::Value = serde_json::from_slice(&output.stdout).expect("JSON");
        report["facts"].clone()
    };
    let names = |facts: &serde_json::Value| -> Vec<String> {
        facts["functions"]
            .as_array()
            .expect("functions")
            .iter()
            .map(|f| f["name"].as_str().expect("name").to_owned())
            .collect()
    };
    let scoped = facts(&[]);
    // call_b in scope, work at the far end of its call; c.rs is not incident.
    let mut found = names(&scoped);
    found.sort();
    assert_eq!(found, ["call_b", "work"]);
    assert_eq!(scoped["calls"].as_array().expect("calls").len(), 1);
    assert!(
        scoped["unknown_calls"]
            .as_array()
            .expect("unknown calls")
            .iter()
            .all(|u| u["from"].as_str().expect("from").starts_with("src/a.rs:"))
    );
    assert!(
        !scoped["unknown_calls"]
            .as_array()
            .expect("unknown calls")
            .is_empty()
    );
    assert!(
        scoped["limitations"]
            .as_array()
            .expect("limitations")
            .iter()
            .any(|l| l.as_str().expect("text").contains("incident to `src/a.rs`"))
    );
    let all = facts(&["--all-facts"]);
    assert!(names(&all).contains(&"alone".to_owned()));
}

#[test]
fn representations_remains_an_alias_of_types() {
    let f = fixture();
    let types = run(&f, &["types", "--type", "Value", "--json"]);
    let alias = run(&f, &["representations", "--type", "Value", "--json"]);
    assert!(
        types.status.success(),
        "{}",
        String::from_utf8_lossy(&types.stderr)
    );
    assert_eq!(types.stdout, alias.stdout);
}

#[test]
fn census_reads_file_modules_inside_inline_modules() {
    // syn's lib.rs declares `mod gen { pub mod fold; }`.
    let f = Fixture::new(&[
        ("src/lib.rs", "pub mod tree {\n    pub mod fold;\n}\n"),
        ("src/tree/fold.rs", "pub trait Fold {}\n"),
    ]);
    let output = run(&f, &["census", "--population", "pub", "--json"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let census = String::from_utf8_lossy(&output.stdout);
    assert!(census.contains("src/tree/fold.rs"), "{census}");
}

#[test]
fn a_positional_root_that_is_a_subdirectory_says_to_use_scope() {
    // Run inside a workspace, `dependencies crates` read `crates` as the
    // project root. It is a directory of this project, not a project: say so
    // and name the --scope spelling. A real positional root still works,
    // with a warning.
    let f = Fixture::new(&[("crates/core/src/lib.rs", "pub fn run() {}\n")]);
    f.write(
        "Cargo.toml",
        "[workspace]\nmembers=['crates/core']\nresolver='3'\n",
    );
    f.write(
        "crates/core/Cargo.toml",
        "[package]\nname='core'\nversion='0.0.0'\nedition='2024'\n",
    );
    let run_in = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_cargo-atlas"))
            .current_dir(f.path())
            .args(args)
            .output()
            .unwrap()
    };
    let refused = run_in(&["atlas", "dependencies", "crates"]);
    assert!(!refused.status.success());
    let message = String::from_utf8_lossy(&refused.stderr);
    assert!(
        message.contains("cargo atlas dependencies --scope crates"),
        "{message}"
    );
    let positional = run_in(&["atlas", "census", "."]);
    assert!(positional.status.success());
    let warning = String::from_utf8_lossy(&positional.stderr);
    assert!(warning.contains("--root ."), "{warning}");
}

#[test]
fn dependencies_draws_every_package_by_default() {
    // A virtual workspace has no root `src`, and a root package's `src` is not
    // the whole project: by default every package's files are drawn.
    let f = Fixture::new(&[]);
    f.write(
        "Cargo.toml",
        "[workspace]\nmembers=['crates/one','crates/two']\nresolver='3'\n",
    );
    for name in ["one", "two"] {
        f.write(
            &format!("crates/{name}/Cargo.toml"),
            &format!("[package]\nname='{name}'\nversion='0.0.0'\nedition='2024'\n"),
        );
        f.write(&format!("crates/{name}/src/lib.rs"), "pub fn run() {}\n");
    }
    let output = Command::new(env!("CARGO_BIN_EXE_cargo-atlas"))
        .args(["atlas", "dependencies", "--json", "--root"])
        .arg(f.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let nodes: Vec<_> = json["view"]["diagram"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["id"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(nodes, ["crates/one/src/lib.rs", "crates/two/src/lib.rs"]);
}

#[test]
fn every_json_flag_is_documented_and_comments_needs_no_audit_flag() {
    let help = |command: &str| {
        let output = Command::new(env!("CARGO_BIN_EXE_cargo-atlas"))
            .args(["atlas", command, "--help"])
            .output()
            .unwrap();
        String::from_utf8_lossy(&output.stdout).into_owned()
    };
    // `flow` and `map` write their JSON beside the drawing; `facts` has --format.
    for command in [
        "orient",
        "census",
        "name",
        "module",
        "dump",
        "diff",
        "moved",
        "visibility",
        "spans",
        "concentration",
        "nontest-loc",
        "comments",
        "dependencies",
        "types",
        "compare",
        "overview",
        "program",
        "delta",
        "stages",
        "policy",
        "seams",
    ] {
        let text = help(command);
        let line = text
            .lines()
            .skip_while(|line| !line.trim_start().starts_with("--json"))
            .take(2)
            .collect::<String>();
        assert!(line.contains("as JSON"), "{command}: {line:?}");
    }
    let f = fixture();
    for audit in [None, Some("--audit")] {
        let output = Command::new(env!("CARGO_BIN_EXE_cargo-atlas"))
            .args(["atlas", "comments", "--json", "--root"])
            .arg(f.path())
            .args(audit)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{audit:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn dependencies_text_names_files_unambiguously_and_says_what_follows() {
    // Two `mod.rs` files: an edge between `[mod.rs]` and `[mod.rs]` cannot be
    // read. Each label is the shortest path suffix no other node shares.
    let f = Fixture::new(&[
        ("src/lib.rs", "pub mod a;\npub mod b;\n"),
        ("src/a/mod.rs", "pub fn go() { crate::b::run(); }\n"),
        ("src/b/mod.rs", "pub fn run() {}\n"),
    ]);
    let output = Command::new(env!("CARGO_BIN_EXE_cargo-atlas"))
        .args(["atlas", "dependencies", "--root"])
        .arg(f.path())
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("[a/mod.rs] --calls"), "{text}");
    assert!(text.contains("--> [b/mod.rs]"), "{text}");
    let header = text.lines().nth(1).unwrap_or_default();
    assert_eq!(header, "3 files; 1 link between files", "{text}");
    assert!(
        !text.contains("COVERAGE"),
        "an empty section is left out: {text}"
    );
}

#[test]
fn flow_text_names_the_function_once_and_an_if_let_once() {
    let f = Fixture::new(&[(
        "src/main.rs",
        "fn run() -> Result<(), String> { Ok(()) }\nfn main() {\n    if let Err(error) = run() {\n        eprintln!(\"{error}\");\n    }\n}\n",
    )]);
    let output = Command::new(env!("CARGO_BIN_EXE_cargo-atlas"))
        .args([
            "atlas",
            "flow",
            "--path",
            "src/main.rs",
            "--function",
            "main",
            "--root",
        ])
        .arg(f.path())
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.starts_with("main (src/main.rs:2)\n"), "{text}");
    let steps: Vec<_> = text.lines().filter(|l| l.starts_with("-> ")).collect();
    assert_eq!(
        steps,
        [
            "-> run() [src/main.rs:3]",
            "-> if let Err(error) = run() [src/main.rs:3]"
        ],
        "{text}"
    );
}
