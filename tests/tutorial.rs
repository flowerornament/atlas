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
    assert!(name.contains("src/check.rs:10:check [pub fn]"));
    assert!(name.contains("src/lib.rs:2:check [pub mod]"));
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
    assert!(text.contains(
        "refusal   authority `the syntax tree`: src/emit.rs:3 reads the home `src/parse.rs::Tree` through `crate::parse::Tree`; src/emit.rs is not an allowed reader"
    ), "{text}");
    assert!(
        text.contains("surface   +1 public declarations · tutorial 11/20 · 2 authorities"),
        "{text}"
    );
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

#[test]
fn delta_refuses_a_second_writer_to_standard_error() {
    let project = tutorial();
    let root = project.path();
    git(root, &["init", "-q"]);
    git(root, &["add", "-A"]);
    git(root, &["commit", "-qm", "start"]);
    let check = fs::read_to_string(root.join("src/check.rs")).expect("check.rs");
    let check = check.replace(
        "        return Err(\"empty program\".into());",
        "        eprintln!(\"warning: nothing to check\");\n        return Err(\"empty program\".into());",
    );
    fs::write(root.join("src/check.rs"), check).expect("edit check.rs");
    git(root, &["commit", "-qam", "check reports to stderr"]);
    let refused = atlas(
        root,
        &[
            "delta",
            "HEAD~1",
            "--policy",
            ".config/architecture.toml",
            "--citation-markers",
            "docs/",
        ],
    );
    assert!(!refused.status.success());
    let text = String::from_utf8_lossy(&refused.stderr);
    for quoted in [
        "spelling  error output: debt 0 -> 1 outside the home · 0 in tests or literals left out · files with macros a pattern cannot see into: 2",
        "refusal   authority `error output`: `src/check.rs::check` newly holds 1 `stderr` outside its home at src/check.rs:12; spell it through the home, or give the item an `allowed_in` row",
    ] {
        assert!(text.contains(quoted), "{quoted}\n{text}");
    }
}

/// The policy docs/checks.md shows is the tutorial's, as written.
#[test]
fn the_checks_guide_shows_the_tutorial_policy() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let guide = fs::read_to_string(manifest.join("docs/checks.md")).expect("checks.md");
    let policy = fs::read_to_string(manifest.join("examples/tutorial/.config/architecture.toml"))
        .expect("tutorial policy");
    let shown = policy
        .split_once('\n')
        .map_or("", |(_, rest)| rest)
        .trim_end();
    assert!(
        guide.contains(&format!("```toml\n{shown}\n```")),
        "docs/checks.md must quote examples/tutorial/.config/architecture.toml below its first line"
    );
}
