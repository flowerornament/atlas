//! `cargo atlas help`: every guide is in the binary and prints its file
//! exactly; a command's help is the same either way it is asked for; and no
//! help text sends a reader to a file they may not have.
use std::process::Command;

const TOPICS: &[&str] = &[
    "agent",
    "getting-started",
    "concepts",
    "query",
    "facts",
    "diagrams",
    "checks",
    "extensions",
    "library",
];

fn atlas(args: &[&str]) -> (bool, String, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_cargo-atlas"))
        // Help reads no project: run it where there is none.
        .current_dir(std::env::temp_dir())
        .args(args)
        .output()
        .expect("run Atlas");
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}
fn file(topic: &str) -> String {
    std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("docs/{topic}.md")),
    )
    .expect("topic file")
}

#[test]
fn every_topic_prints_its_file_exactly() {
    let (ok, listing, _) = atlas(&["help", "topics"]);
    assert!(ok);
    for topic in TOPICS {
        assert!(
            listing.contains(&format!("  {topic} ")),
            "{topic} not listed:\n{listing}"
        );
        let (ok, text, stderr) = atlas(&["help", "--topic", topic]);
        assert!(ok, "{topic}: {stderr}");
        assert_eq!(text, file(topic), "{topic}");
        if !["query", "facts"].contains(topic) {
            assert_eq!(atlas(&["help", topic]).1, text, "{topic}");
        }
    }
    // Every guide in docs/ is a topic.
    for entry in std::fs::read_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("docs"))
        .expect("docs")
    {
        let path = entry.expect("entry").path();
        if path.extension().is_some_and(|e| e == "md") {
            let stem = path.file_stem().and_then(|s| s.to_str()).expect("stem");
            assert!(TOPICS.contains(&stem), "docs/{stem}.md is not a help topic");
        }
    }
}

#[test]
fn help_command_is_command_help_even_when_a_topic_shares_the_name() {
    // A name that is both prints the command; --topic reaches the guide.
    for command in ["census", "callers", "orient", "query", "facts"] {
        let (ok, help, _) = atlas(&["help", command]);
        assert!(ok);
        assert_eq!(help, atlas(&[command, "--help"]).1, "{command}");
    }
    assert_eq!(atlas(&["help"]).1, atlas(&["--help"]).1);
    let (ok, _, stderr) = atlas(&["help", "nowhere"]);
    assert!(!ok);
    assert!(
        stderr.contains("cargo atlas help topics lists the topics"),
        "{stderr}"
    );
}

#[test]
fn help_points_at_topics_not_files() {
    let (_, main, _) = atlas(&["--help"]);
    let commands: Vec<String> = main
        .lines()
        .skip_while(|line| !line.starts_with("Commands:"))
        .skip(1)
        .take_while(|line| line.starts_with("  "))
        .filter_map(|line| line.split_whitespace().next().map(str::to_owned))
        .collect();
    assert!(commands.len() > 20, "{main}");
    for (name, text) in std::iter::once(("--help".to_owned(), main.clone())).chain(
        commands
            .iter()
            .map(|command| (command.clone(), atlas(&[command, "--help"]).1)),
    ) {
        for word in text.split_whitespace() {
            let path = std::path::Path::new(word.trim_end_matches(['.', ',', ')']));
            assert!(
                !(word.starts_with("docs/") && path.extension().is_some_and(|e| e == "md")),
                "{name} help points at {word}"
            );
        }
        if text.contains("https://") {
            assert!(
                text.contains("cargo atlas help"),
                "{name} has only a web link"
            );
        }
    }
    assert!(main.contains("cargo atlas help topics"));
}

/// The cookbook in `help agent`: every `datalog` block is a program that runs
/// on the tutorial project and answers with rows.
#[test]
fn every_agent_recipe_runs_on_the_tutorial() {
    let briefing = file("agent");
    assert!(
        briefing.lines().count() <= 160,
        "the briefing grew past a page"
    );
    let project = tempfile::tempdir().expect("tutorial copy");
    copy(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/tutorial"),
        project.path(),
    );
    let mut recipes = 0;
    for block in briefing.split("```datalog\n").skip(1) {
        let program = block.split("```").next().expect("a closed block");
        let mut child = Command::new(env!("CARGO_BIN_EXE_cargo-atlas"))
            .current_dir(project.path())
            .args(["query", "-f", "-", "--as", "json"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("run Atlas");
        std::io::Write::write_all(&mut child.stdin.take().expect("stdin"), program.as_bytes())
            .expect("write the recipe");
        let output = child.wait_with_output().expect("Atlas output");
        assert!(
            output.status.success(),
            "{program}\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let answer: serde_json::Value = serde_json::from_slice(&output.stdout).expect("JSON");
        assert!(
            answer["tables"][0]["total"].as_u64().is_some_and(|n| n > 0),
            "{program} answered no rows"
        );
        recipes += 1;
    }
    assert!(recipes >= 5, "{recipes} recipes");
}
fn copy(from: &std::path::Path, to: &std::path::Path) {
    std::fs::create_dir_all(to).expect("directory");
    for entry in std::fs::read_dir(from).expect("source") {
        let entry = entry.expect("entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("type").is_dir() {
            copy(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).expect("copy");
        }
    }
}

/// `query --help` names every relation the prelude defines: the list is
/// read from the prelude, so a new relation cannot be left out.
#[test]
fn query_help_lists_every_prelude_relation() {
    let (_, prelude, _) = atlas(&["query", "--prelude", "--root", env!("CARGO_MANIFEST_DIR")]);
    let defined: std::collections::BTreeSet<&str> = prelude
        .lines()
        .filter_map(|line| line.split_once('{').map(|(head, _)| head))
        .filter(|head| !head.is_empty() && head.chars().all(|c| c.is_ascii_lowercase() || c == '_'))
        .collect();
    assert!(defined.len() >= 8, "{defined:?}");
    let (_, help, _) = atlas(&["query", "--help"]);
    let listed = help
        .lines()
        .find(|line| line.starts_with("The prelude (--prelude) restates commands as relations: "))
        .expect("the prelude line");
    for relation in &defined {
        assert!(
            listed.contains(&format!("{relation},")) || listed.contains(&format!("{relation} (")),
            "{relation} missing: {listed}"
        );
    }
}

/// A guide's link to another guide also names the topic that prints it, so
/// it means something in a terminal.
#[test]
fn every_guide_cross_link_names_its_topic() {
    for topic in TOPICS {
        let text = file(topic);
        for (at, _) in text.match_indices("](") {
            let rest = &text[at + 2..];
            let Some(end) = rest.find(')') else { continue };
            let target = rest[..end].split('#').next().unwrap_or_default();
            let Some(linked) = target.strip_suffix(".md") else {
                continue;
            };
            if !TOPICS.contains(&linked) || target.contains('/') {
                continue;
            }
            let spelling = if ["query", "facts"].contains(&linked) {
                format!("--topic {linked}")
            } else {
                linked.to_owned()
            };
            assert!(
                rest[end + 1..].starts_with(&format!(" (`cargo atlas help {spelling}`)")),
                "docs/{topic}.md links {target} without naming its topic"
            );
        }
    }
}

/// atlas-lib's rustdoc guide is the library topic, word for word: the crate
/// is published on its own, so it carries a copy.
#[test]
fn the_library_crate_carries_the_library_guide() {
    let copy = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("crates/atlas-lib/src/guide.md"),
    )
    .expect("the crate's copy");
    assert_eq!(
        copy,
        file("library"),
        "copy docs/library.md to crates/atlas-lib/src/guide.md"
    );
}

/// The main help opens with the briefing, for a reader with only the binary.
#[test]
fn main_help_starts_with_the_agent_briefing() {
    for args in [&["--help"][..], &["help"]] {
        let (_, help, _) = atlas(args);
        assert!(
            help.starts_with("New here, or an agent? Run `cargo atlas help agent` first"),
            "{args:?}: {help}"
        );
    }
}
