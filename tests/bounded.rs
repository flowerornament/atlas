//! Bounded text (spec §3.6): a list longer than its default ends with one
//! tail line whose command, run, shows the rest; `--all` shows every row; JSON
//! is never bounded.
mod support;
use std::{
    fmt::Write as _,
    path::Path,
    process::{Command, Output},
};
use support::Fixture;

/// 30 files with four undocumented functions each, all calling `run`, a name
/// declared in every file, then a second commit that changes every file and
/// narrows one function in each.
fn project() -> Fixture {
    let mut lib = String::new();
    let mut files = Vec::new();
    for n in 0..30 {
        writeln!(lib, "pub mod m{n:02};").expect("write to a String");
        files.push((
            format!("src/m{n:02}.rs"),
            format!(
                "pub fn run() {{}}\npub fn a{n}() {{ run() }}\npub fn b{n}() {{ run() }}\npub fn c{n}() {{ run() }}\n"
            ),
        ));
    }
    let mut all: Vec<(&str, &str)> = files
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect();
    all.push(("src/lib.rs", &lib));
    let f = Fixture::new(&all);
    let git = |args: &[&str]| {
        let status = Command::new("git")
            .current_dir(f.path())
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
    for (path, text) in &files {
        let narrowed = text.replace("pub fn a", "pub(crate) fn a");
        f.write(path, &format!("{narrowed}pub(crate) fn d() {{}}\n"));
    }
    git(&["commit", "-qam", "two"]);
    f
}
fn atlas(dir: &Path, args: &[&str]) -> String {
    let output: Output = Command::new(env!("CARGO_BIN_EXE_cargo-atlas"))
        .current_dir(dir)
        .env("ATLAS_NOTICES", "off")
        .env("ATLAS_CACHE", "off")
        .args(args)
        .output()
        .expect("run Atlas");
    assert!(
        output.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}
/// The tail lines of `text`: (rows left out, the command after `; `).
fn tails(text: &str) -> Vec<(usize, String)> {
    text.lines()
        .filter_map(|line| {
            let rest = line.trim_start().strip_prefix("… ")?;
            let (count, command) = rest.split_once(" more; ")?;
            Some((count.parse().ok()?, command.to_owned()))
        })
        .collect()
}

#[test]
fn every_cut_list_ends_with_a_command_that_shows_the_rest() {
    let f = project();
    let dir = f.path();
    for args in [
        &["spans"][..],
        &["concentration"],
        &["nontest-loc"],
        &["comments"],
        &["name", "run"],
        &["name", "run", "--relations"],
        &["module", "src"],
        &["diff", "HEAD~1"],
        &["diff", "HEAD~1", "--items"],
        &["moved", "HEAD~1"],
        &["visibility", "HEAD~1"],
    ] {
        let text = atlas(dir, args);
        assert!(
            text.lines().count() <= 60,
            "{args:?}: {} lines",
            text.lines().count()
        );
        let found = tails(&text);
        assert!(!found.is_empty(), "{args:?} cut nothing:\n{text}");
        for (hidden, command) in found {
            let words: Vec<&str> = command
                .strip_prefix("cargo atlas ")
                .expect("a runnable command")
                .split(' ')
                .collect();
            assert_eq!(words.last(), Some(&"--all"), "{command}");
            let rest = atlas(dir, &words);
            assert!(tails(&rest).is_empty(), "{command} still cuts:\n{rest}");
            assert!(
                rest.lines().count() >= hidden,
                "{command} shows fewer than the {hidden} rows left out"
            );
        }
    }
}

#[test]
fn rows_and_all_choose_how_many_and_json_holds_every_row() {
    let f = project();
    let dir = f.path();
    let files = |text: &str| text.lines().filter(|line| line.starts_with("src/")).count();
    let loc = atlas(dir, &["nontest-loc", "--rows", "3"]);
    assert_eq!(
        loc.lines().filter(|line| line.contains(" src/")).count(),
        3,
        "{loc}"
    );
    assert_eq!(files(&atlas(dir, &["spans", "--all"])), 31);
    assert_eq!(files(&atlas(dir, &["spans", "--rows", "4"])), 4);
    // AT6: JSON is never bounded.
    let json: serde_json::Value =
        serde_json::from_str(&atlas(dir, &["spans", "--as", "json"])).expect("JSON");
    assert_eq!(json["report"]["files"].as_array().expect("files").len(), 31);
    let refused = Command::new(env!("CARGO_BIN_EXE_cargo-atlas"))
        .current_dir(dir)
        .args(["spans", "--rows", "2", "--all"])
        .output()
        .expect("run Atlas");
    assert!(!refused.status.success());
    assert!(String::from_utf8_lossy(&refused.stderr).contains("cannot be used with"));
    // A nested list's tail names the narrower subject.
    let spans = atlas(dir, &["spans", "--rows", "1"]);
    assert!(
        spans.contains("  … 29 more; cargo atlas spans src/lib.rs --all\n"),
        "{spans}"
    );
}
