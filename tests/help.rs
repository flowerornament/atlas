//! `cargo atlas help`: every guide is in the binary and prints its file
//! exactly; a command's help is the same either way it is asked for; and no
//! help text sends a reader to a file they may not have.
use std::process::Command;

const TOPICS: &[&str] = &[
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
