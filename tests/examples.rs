//! Every example Atlas shows a reader runs: each `Example:` line in the
//! commands' `--help`, and each `cargo atlas …` line in the agent briefing and
//! the query guide, on a copy of the tutorial project with two commits. An
//! example that needs what the tutorial has not (a Rust adapter, another
//! project's files, a jj workspace) must still parse: it may fail, but never
//! with a usage error.
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

/// Examples that cannot run on the tutorial, and why.
const PARSE_ONLY: &[(&str, &str)] = &[
    ("--adapter", "needs a declaration adapter executable"),
    ("architecture.json", "needs an architecture profile"),
    ("@-", "a jj revision; the copy is a git repository"),
];

fn copy(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("directory");
    for entry in fs::read_dir(from).expect("source") {
        let entry = entry.expect("entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("type").is_dir() {
            if entry.file_name() != "target" {
                copy(&entry.path(), &target);
            }
        } else {
            fs::copy(entry.path(), target).expect("copy");
        }
    }
}
/// The tutorial with two commits, and the files the guides' examples read.
fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("project");
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    copy(&root.join("examples/tutorial"), dir.path());
    fs::write(
        dir.path().join("questions.dl"),
        "?- file_calls{from: A, to: B, calls: N}.\n",
    )
    .expect("questions");
    let git = |args: &[&str]| {
        let status = Command::new("git")
            .current_dir(dir.path())
            .args([
                "-c",
                "user.name=Atlas Test",
                "-c",
                "user.email=atlas@example.invalid",
            ])
            .args(args)
            .status()
            .expect("git");
        assert!(status.success(), "git {args:?}");
    };
    git(&["init", "-q"]);
    git(&["add", "."]);
    git(&["commit", "-qm", "one"]);
    fs::write(
        dir.path().join("src/emit.rs"),
        fs::read_to_string(dir.path().join("src/emit.rs")).expect("emit")
            + "\n/// More.\npub fn more() {}\n",
    )
    .expect("edit");
    git(&["commit", "-qam", "two"]);
    dir
}
fn atlas_help(args: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_cargo-atlas"))
        .args(args)
        .output()
        .expect("help");
    String::from_utf8_lossy(&output.stdout).into_owned()
}
/// `Example:` lines from every command's help.
fn help_examples() -> Vec<(String, String)> {
    let main = atlas_help(&["--help"]);
    main.lines()
        .skip_while(|line| !line.starts_with("Commands:"))
        .skip(1)
        .take_while(|line| line.starts_with("  "))
        .filter_map(|line| line.split_whitespace().next())
        .flat_map(|command| {
            atlas_help(&[command, "--help"])
                .lines()
                .filter_map(|line| line.strip_prefix("Example: "))
                .map(|example| (format!("{command} --help"), example.to_owned()))
                .collect::<Vec<_>>()
        })
        .collect()
}
/// `cargo atlas …` lines in a guide's `sh` and `console` blocks, and inline
/// spans with no placeholder (an upper-case word such as FILE or X, or `...`).
fn guide_examples(topic: &str) -> Vec<(String, String)> {
    let text =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("docs/{topic}.md")))
            .expect("guide");
    let mut found = Vec::new();
    let mut block = None;
    for line in text.lines() {
        if let Some(fence) = line.strip_prefix("```") {
            block = if block.is_some() {
                None
            } else {
                Some(fence.to_owned())
            };
            continue;
        }
        match block.as_deref() {
            Some("sh" | "console") => {
                let line = line.strip_prefix("$ ").unwrap_or(line);
                if line.starts_with("cargo atlas ") {
                    found.push((format!("docs/{topic}.md"), line.to_owned()));
                }
            }
            Some(_) => {}
            None => {
                for span in line.split('`').skip(1).step_by(2) {
                    let placeholder = span.contains("...")
                        || span.split_whitespace().any(|word| {
                            let word = word.trim_matches(|c: char| !c.is_ascii_alphanumeric());
                            !word.is_empty() && word.chars().all(|c| c.is_ascii_uppercase())
                        });
                    if span.starts_with("cargo atlas ") && !placeholder {
                        found.push((format!("docs/{topic}.md"), span.to_owned()));
                    }
                }
            }
        }
    }
    found
}

#[test]
fn every_shown_example_runs() {
    let project = project();
    let bin: PathBuf = Path::new(env!("CARGO_BIN_EXE_cargo-atlas"))
        .parent()
        .expect("binary directory")
        .to_owned();
    let path = std::env::join_paths(std::iter::once(bin).chain(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    )))
    .expect("PATH");
    let examples: Vec<(String, String)> = help_examples()
        .into_iter()
        .chain(guide_examples("agent"))
        .chain(guide_examples("query"))
        .collect();
    assert!(examples.len() > 30, "{} examples", examples.len());
    let mut failures = Vec::new();
    for (source, example) in &examples {
        let output = Command::new("sh")
            .current_dir(project.path())
            .env("PATH", &path)
            .env("ATLAS_NOTICES", "off")
            .env("ATLAS_CACHE", "off")
            .args(["-c", example])
            .output()
            .expect("sh");
        // A guide naming a command in prose ("`cargo atlas query` runs …")
        // shows no example to run.
        let named = example == "cargo atlas query";
        let parse_only = PARSE_ONLY.iter().find(|(word, _)| example.contains(word));
        let parse_only = parse_only.is_some() || named;
        let code = output.status.code();
        // A usage error from the parser exits 2.
        let ok = if parse_only {
            code != Some(2)
        } else {
            code == Some(0)
        };
        if !ok {
            failures.push(format!(
                "{source}: {example}\n  exit {code:?}: {}",
                String::from_utf8_lossy(&output.stderr)
                    .lines()
                    .next()
                    .unwrap_or_default()
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} examples failed:\n{}",
        failures.len(),
        examples.len(),
        failures.join("\n")
    );
}
