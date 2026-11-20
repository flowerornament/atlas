//! What an agent with only the binary stumbled on in its first session, each
//! now answered by the binary itself: where a count and `order by` go, which
//! file pairs are busiest, how to narrow a name that matches many functions,
//! and source limits read by kind rather than one line each.
mod support;
use std::{fmt::Write as _, process::Command};
use support::Fixture;

fn atlas(f: &Fixture, args: &[&str]) -> (bool, String, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_cargo-atlas"))
        .current_dir(f.path())
        .env("ATLAS_CACHE", "off")
        .env("ATLAS_NOTICES", "off")
        .args(args)
        .output()
        .expect("run Atlas");
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}
/// Five modules that each define `run`, and `lib.rs` calling every one, plus
/// a test target whose `mod support;` has no file (a source limit).
fn fixture() -> Fixture {
    let mut lib = String::new();
    let mut files = Vec::new();
    for n in 0..5 {
        writeln!(lib, "pub mod m{n};").expect("write to a String");
        files.push((
            format!("src/m{n}.rs"),
            "pub fn run() {}
"
            .to_owned(),
        ));
    }
    lib.push_str("pub fn all() {\n");
    for n in 0..5 {
        for _ in 0..=n {
            writeln!(lib, "    m{n}::run();").expect("write to a String");
        }
    }
    lib.push_str("}\n");
    let mut all: Vec<(&str, &str)> = files
        .iter()
        .map(|(p, t)| (p.as_str(), t.as_str()))
        .collect();
    all.push(("src/lib.rs", &lib));
    all.push(("tests/it.rs", "mod support;\n#[test]\nfn it() {}\n"));
    all.push(("tests/other.rs", "mod support;\n#[test]\nfn other() {}\n"));
    Fixture::new(&all)
}

#[test]
fn query_mistakes_say_where_a_count_and_order_by_go() {
    let f = fixture();
    for (program, hint) in [
        (
            "?- function{id: F}, n = count(F).",
            "a count goes in a rule head, not a body: write `n{key: K, n: count(C)} :- r{key: K, id: C}.` and then query it, `?- n{key: K, n: N}.` (cargo atlas help --topic query)",
        ),
        (
            "?- function{id: F}, N = count(F).",
            "a count goes in a rule head, not a body",
        ),
        (
            "?- function{id: F}, order by F.",
            "`order by` ends a query after its last literal, with no comma before it: `?- r{a: A, n: N} order by N desc limit 10.` (cargo atlas help --topic query)",
        ),
    ] {
        let (ok, _, stderr) = atlas(&f, &["query", program]);
        assert!(!ok, "{program}");
        assert!(stderr.contains(hint), "{program}: {stderr}");
    }
    // The absence check names the relation that holds possible calls.
    let (ok, _, stderr) = atlas(
        &f,
        &[
            "query",
            "c{f: F} :- call{to: F}. ?- function{id: F}, !c{f: F}.",
        ],
    );
    assert!(!ok);
    assert!(
        stderr.contains("for calls, the prelude's possible_call{to: F, call: C}"),
        "{stderr}"
    );
}

#[test]
fn dependencies_ranks_the_busiest_file_pairs() {
    let f = fixture();
    let (ok, text, stderr) = atlas(&f, &["dependencies", "src"]);
    assert!(ok, "{stderr}");
    let ranked: Vec<&str> = text
        .lines()
        .skip_while(|line| !line.starts_with("BUSIEST FILE PAIRS"))
        .skip(1)
        .take_while(|line| !line.is_empty())
        .collect();
    assert_eq!(
        ranked.first(),
        Some(&"  5  src/lib.rs -> src/m4.rs"),
        "{text}"
    );
    assert_eq!(
        ranked.last(),
        Some(&"  1  src/lib.rs -> src/m0.rs"),
        "{text}"
    );
    assert!(text.find("BUSIEST FILE PAIRS") < text.find("EVIDENCE / LIMITS"));
    // JSON edges carry the same count as a number.
    let (_, json, _) = atlas(&f, &["dependencies", "src", "--as", "json"]);
    let json: serde_json::Value = serde_json::from_str(&json).expect("JSON");
    let edge = json["view"]["diagram"]["edges"]
        .as_array()
        .expect("edges")
        .iter()
        .find(|edge| edge["to"] == "src/m4.rs")
        .expect("the m4 edge");
    assert_eq!(edge["count"], 5);
    assert_eq!(edge["relationships"].as_array().map(Vec::len), Some(5));
}

#[test]
fn a_name_with_many_matches_says_how_to_name_one() {
    let f = fixture();
    let (ok, text, _) = atlas(&f, &["callers", "run"]);
    assert!(ok);
    let mut lines = text.lines();
    assert_eq!(
        lines.next(),
        Some("run matches 5 functions; each is answered on its own")
    );
    assert_eq!(
        lines.next(),
        Some(
            "  to name one: Type::method or module::name (cargo atlas callers m0::run), FILE:LINE (src/m0.rs:1), or --in PATH (--in src)"
        )
    );
    // Three or fewer need no hint.
    let (_, text, _) = atlas(&f, &["callers", "m0::run"]);
    assert!(!text.contains("to name one"), "{text}");
}

#[test]
fn diff_items_counts_source_limits_by_kind() {
    let f = fixture();
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
    let (ok, text, stderr) = atlas(&f, &["diff", "HEAD", "--items"]);
    assert!(ok, "{stderr}");
    let limits: Vec<&str> = text
        .lines()
        .filter(|line| line.contains("source limit"))
        .collect();
    // One summary line per side, not one line per file.
    assert_eq!(limits.len(), 2, "{text}");
    assert!(limits[1].starts_with("current source limits: "), "{text}");
    assert!(limits[1].ends_with("(--json lists each)"), "{text}");
    assert!(text.contains("  mount-missing  e.g. tests/"), "{text}");
}

#[test]
fn the_schema_reads_as_text_and_as_data() {
    let f = fixture();
    let (ok, text, _) = atlas(&f, &["facts", "--schema"]);
    assert!(ok);
    assert!(text.starts_with("Atlas facts, schema 1: "), "{text}");
    assert!(text.contains("\nfunction (qualified): "), "{text}");
    assert!(text.contains("  public "), "{text}");
    assert!(
        text.contains("unresolved observations: unknown_call"),
        "{text}"
    );
    let (_, json, _) = atlas(&f, &["facts", "--schema", "--as", "json"]);
    let json: serde_json::Value = serde_json::from_str(&json).expect("schema JSON");
    for relation in json["relations"].as_array().expect("relations") {
        let name = relation["name"].as_str().expect("name");
        assert!(
            text.contains(&format!("\n{name} (")),
            "{name} missing from the text"
        );
    }
    let (ok, _, stderr) = atlas(&f, &["facts", "--as", "text"]);
    assert!(!ok);
    assert!(stderr.contains("--as text is for --schema"), "{stderr}");
}

#[test]
fn query_text_folds_the_standing_limits_into_one_note() {
    let f = fixture();
    let program = "?- calls_function{from: F, to: G}.";
    let (ok, text, _) = atlas(&f, &["query", program]);
    assert!(ok);
    let notes: Vec<&str> = text
        .lines()
        .filter(|line| line.starts_with("note: "))
        .collect();
    assert_eq!(notes.len(), 2, "{text}");
    assert!(
        notes[0].contains("`call` has an unknowns relation"),
        "{text}"
    );
    assert!(
        notes[1].contains("standing limits") && notes[1].contains("cargo atlas help concepts"),
        "{text}"
    );
    // JSON keeps every one.
    let (_, json, _) = atlas(&f, &["query", program, "--as", "json"]);
    let json: serde_json::Value = serde_json::from_str(&json).expect("JSON");
    assert!(json["notes"].as_array().expect("notes").len() >= 5);
}
