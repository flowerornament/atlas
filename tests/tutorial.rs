//! The tutorial project behind docs/getting-started.md and docs/checks.md.
//!
//! These tests run the documented commands on a copy of `examples/tutorial`
//! and check the lines the guides quote, so the guides fail loudly when the
//! output they show changes.
#[path = "support/adapter.rs"]
mod example_adapter;
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

/// Copy the tutorial into a fresh directory, as the guide tells readers to.
fn tutorial() -> tempfile::TempDir {
    fn copy(from: &Path, to: &Path) {
        fs::create_dir_all(to).expect("tutorial directory");
        for entry in fs::read_dir(from).expect("tutorial source") {
            let entry = entry.expect("tutorial entry");
            let target = to.join(entry.file_name());
            if entry.file_type().expect("tutorial entry type").is_dir() {
                if entry.file_name() != "target" {
                    copy(&entry.path(), &target);
                }
            } else {
                fs::copy(entry.path(), target).expect("copy tutorial file");
            }
        }
    }
    let directory = tempfile::tempdir().expect("tutorial copy");
    copy(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/tutorial"),
        directory.path(),
    );
    directory
}

fn atlas(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_cargo-atlas"))
        .current_dir(root)
        .args(args)
        .output()
        .expect("run atlas")
}

fn stdout(output: &Output) -> String {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout.clone()).expect("UTF-8 output")
}

fn git(root: &Path, args: &[&str]) {
    let output = Command::new("git")
        .current_dir(root)
        .args([
            "-c",
            "user.name=Atlas Test",
            "-c",
            "user.email=atlas@example.invalid",
        ])
        .args(args)
        .output()
        .expect("git");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn navigation_commands_match_the_getting_started_guide() {
    let project = tutorial();
    let root = project.path();
    let census = stdout(&atlas(root, &["census"]));
    assert!(census.starts_with("workspace: 5 files (0 test-only), 11 production declarations\n"));
    let name = stdout(&atlas(root, &["name", "check"]));
    assert!(name.starts_with("check: 2 declaration sites (ambiguous)\n"));
    assert!(name.contains("src/check.rs:10:check [Pub Fn]"));
    assert!(name.contains("src/lib.rs:2:check [Pub Mod]"));
    let calls = stdout(&atlas(root, &["dependencies", "--relation", "calls"]));
    assert!(calls.contains("[main.rs] --calls: main calls check (1 site)--> [check.rs]"));
    let contracts = stdout(&atlas(
        root,
        &["types", "--query", "pipeline.json", "--panel", "contracts"],
    ));
    assert!(contracts.contains("(Tree [tree]) -> (Checked [return · Ok])"));
}

#[test]
fn the_example_policy_passes_and_delta_refuses_a_skipped_stage() {
    let project = tutorial();
    let root = project.path();
    let policy = stdout(&atlas(
        root,
        &["policy", "--policy", ".config/architecture.toml"],
    ));
    assert!(policy.starts_with("policy inputs: PASS"));

    git(root, &["init", "-q"]);
    git(root, &["add", "-A"]);
    git(root, &["commit", "-qm", "start"]);
    let emit = fs::read_to_string(root.join("src/emit.rs")).expect("emit.rs");
    let emit = emit.replace(
        "use crate::check::Checked;\n",
        "use crate::check::Checked;\nuse crate::parse::Tree;\n",
    ) + "\n/// Count the words in a tree.\npub fn count(tree: &Tree) -> usize {\n    tree.words.len()\n}\n";
    fs::write(root.join("src/emit.rs"), emit).expect("edit emit.rs");
    git(root, &["commit", "-qam", "emit reads the tree directly"]);

    let delta = |markers: bool| {
        let mut args = vec!["delta", "HEAD~1", "--policy", ".config/architecture.toml"];
        if markers {
            args.extend(["--citation-markers", "docs/"]);
        }
        atlas(root, &args)
    };
    // A failing verdict is printed on stderr, with the process exiting unsuccessfully.
    let refused = delta(true);
    assert!(!refused.status.success());
    let text = String::from_utf8_lossy(&refused.stderr);
    assert!(text.contains("verdict   REFUSE"), "{text}");
    assert!(text.contains(
        "refusal   stage interface `tutorial`: src/emit.rs:3 emit reads parse (`crate::parse::Tree`)"
    ));
    let unavailable = delta(false);
    assert!(!unavailable.status.success());
    assert!(String::from_utf8_lossy(&unavailable.stderr).contains("verdict   UNAVAILABLE"));
}

#[test]
fn the_example_adapter_reads_the_tutorial_pipeline() {
    let project = tutorial();
    let adapter = example_adapter::example();
    let overview = stdout(&atlas(
        project.path(),
        &[
            "overview",
            "--adapter",
            adapter.to_str().expect("UTF-8 path"),
        ],
    ));
    assert!(overview.contains("src/emit.rs:5  emit"));
    assert!(overview.contains("(Checked [& program]) -> ()"));
}
