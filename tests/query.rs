//! `query`: the prelude's relations agree with the commands they restate,
//! on a fixture and on this workspace; saved facts answer as a capture
//! does; and refusals, acceptances and empty answers exit as documented.
mod support;
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    process::{Command, Output},
};
use support::Fixture;

fn fixture() -> Fixture {
    Fixture::new(&[
        (
            "src/lib.rs",
            "pub mod a;\npub mod b;\npub mod c;\npub fn entry(token: a::Token) -> usize {\n    helper();\n    a::make();\n    b::build();\n    token.len()\n}\nfn helper() {}\n",
        ),
        (
            "src/a.rs",
            "pub struct Token(pub u8);\nimpl Token {\n    pub fn len(&self) -> usize { 1 }\n}\npub fn make() -> Token { crate::b::build(); Token(1) }\npub fn unused() {}\n",
        ),
        (
            "src/b.rs",
            "pub struct Token;\nimpl Token {\n    pub fn go(&self) {}\n}\npub fn build() {}\npub struct Holder { pub token: Token }\n",
        ),
        (
            "src/c.rs",
            "pub fn lonely(t: Token) { t.go(); mystery(); }\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn t() { super::lonely(todo!()); }\n}\n",
        ),
        (
            "tests/it.rs",
            "#[test]\nfn it() { fixture::entry(todo!()); }\n",
        ),
    ])
}

fn run(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_cargo-atlas"))
        .current_dir("/")
        .arg("--root")
        .arg(root)
        .args(args)
        .output()
        .expect("Atlas process")
}
fn json(root: &Path, args: &[&str]) -> Value {
    let output = run(root, args);
    assert!(
        output.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("Atlas JSON")
}
/// The rows of a query answer's first table.
fn rows(root: &Path, program: &str) -> Vec<Vec<Value>> {
    let answer = json(root, &["query", "--json", program]);
    answer["tables"][0]["rows"]
        .as_array()
        .expect("rows")
        .iter()
        .map(|r| r.as_array().expect("row").clone())
        .collect()
}
fn s(v: &Value) -> String {
    v.as_str().expect("text").to_owned()
}

/// `census` and the prelude's `census`: every metric, kind and package.
/// The prelude has no row for a metric with nothing to count.
fn census_agrees(root: &Path) {
    let command = json(root, &["census", "--json"]);
    let mut expected = BTreeMap::new();
    for (metric, value) in command.as_object().expect("census") {
        match value {
            Value::Number(n) => {
                expected.insert((metric.clone(), String::new()), n.as_u64().expect("count"));
            }
            Value::Object(keys) => {
                for (key, n) in keys {
                    expected.insert((metric.clone(), key.clone()), n.as_u64().expect("count"));
                }
            }
            _ => {}
        }
    }
    expected.remove(&("schema".to_owned(), String::new()));
    let mut got: BTreeMap<(String, String), u64> =
        rows(root, "?- census{metric: M, key: K, n: N}.")
            .iter()
            .map(|r| ((s(&r[0]), s(&r[1])), r[2].as_u64().expect("n")))
            .collect();
    for (key, n) in &expected {
        if *n == 0 && key.1.is_empty() {
            got.entry(key.clone()).or_insert(0);
        }
    }
    assert_eq!(got, expected, "{}", root.display());
}

/// `dependencies --relation calls` edges and the prelude's `file_calls`
/// read through the same scope: every package's production files when
/// `scope` is `None`, else both ends `under` it.
fn file_calls_agree(root: &Path, scope: Option<&str>) {
    let mut args = vec!["dependencies", "--relation", "calls", "--json"];
    let mut program = "?- file_calls{from: A, to: B, calls: N}".to_owned();
    if let Some(scope) = scope {
        args.extend(["--scope", scope]);
        program = format!("{program}, under(A, {scope:?}), under(B, {scope:?})");
    }
    program.push('.');
    let command = json(root, &args);
    let expected: BTreeSet<(String, String, u64)> = command["view"]["diagram"]["edges"]
        .as_array()
        .expect("edges")
        .iter()
        .map(|e| {
            (
                s(&e["from"]),
                s(&e["to"]),
                u64::try_from(e["relationships"].as_array().expect("witnesses").len())
                    .expect("count"),
            )
        })
        .collect();
    assert!(!expected.is_empty(), "{}", root.display());
    let got: BTreeSet<(String, String, u64)> = rows(root, &program)
        .iter()
        .map(|r| (s(&r[0]), s(&r[1]), r[2].as_u64().expect("n")))
        .collect();
    assert_eq!(got, expected, "{}", root.display());
}

/// `name NAME --relations` cards and the prelude's `name_match` and
/// `name_card`.
fn name_cards_agree(root: &Path, name: &str) {
    type Card = BTreeMap<&'static str, BTreeSet<String>>;
    let command = json(root, &["name", name, "--relations", "--json"]);
    let mut expected: BTreeMap<String, Card> = BTreeMap::new();
    for m in command["matches"].as_array().expect("matches") {
        let card = expected.entry(s(&m["declaration"]["id"])).or_default();
        for (key, relation) in [
            ("outgoing", "outgoing"),
            ("incoming", "incoming"),
            ("possible_incoming", "possible"),
        ] {
            card.entry(relation).or_default().extend(
                m[key]
                    .as_array()
                    .expect("mentions")
                    .iter()
                    .map(|e| s(&e["id"])),
            );
        }
    }
    let answer = json(
        root,
        &[
            "query",
            "--json",
            &format!(
                "?- name_match{{name: {name:?}, declaration: D}}.\n?- name_card{{name: {name:?}, declaration: D, relation: R, mention: E}}."
            ),
        ],
    );
    let mut got: BTreeMap<String, Card> = BTreeMap::new();
    for r in answer["tables"][0]["rows"].as_array().expect("rows") {
        let card = got.entry(s(&r[0])).or_default();
        for relation in ["outgoing", "incoming", "possible"] {
            card.entry(relation).or_default();
        }
    }
    for r in answer["tables"][1]["rows"].as_array().expect("rows") {
        let relation = match r[1].as_str() {
            Some("outgoing") => "outgoing",
            Some("incoming") => "incoming",
            _ => "possible",
        };
        got.entry(s(&r[0]))
            .or_default()
            .entry(relation)
            .or_default()
            .insert(s(&r[2]));
    }
    assert!(!expected.is_empty(), "{name}");
    assert_eq!(got, expected, "{name} in {}", root.display());
}

#[test]
fn the_prelude_restates_its_commands_on_a_fixture() {
    let fixture = fixture();
    census_agrees(fixture.path());
    file_calls_agree(fixture.path(), None);
    file_calls_agree(fixture.path(), Some("src"));
    name_cards_agree(fixture.path(), "Token");
    name_cards_agree(fixture.path(), "lonely");
}

#[test]
fn the_prelude_restates_its_commands_on_this_workspace() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    census_agrees(root);
    file_calls_agree(root, None);
    file_calls_agree(root, Some("src"));
    file_calls_agree(root, Some("crates/atlas-lib/src/query"));
    name_cards_agree(root, "Relation");
    name_cards_agree(root, "Snapshot");
}

#[test]
fn saved_facts_answer_as_a_capture_and_say_when_the_tree_moved() {
    let fixture = fixture();
    let program = "?- file_calls{from: A, to: B, calls: N}.";
    let captured = json(fixture.path(), &["query", "--json", program]);
    for format in ["json", "jsonl"] {
        let out = fixture.path().join(format!("target/facts.{format}"));
        let output = run(
            fixture.path(),
            &[
                "facts",
                "--format",
                format,
                "--out",
                out.to_str().expect("path"),
            ],
        );
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let saved = json(
            fixture.path(),
            &[
                "query",
                "--json",
                "--facts",
                out.to_str().expect("path"),
                program,
            ],
        );
        assert_eq!(saved["tables"], captured["tables"], "{format}");
        assert!(saved["notes"].as_array().expect("notes").iter().all(|n| {
            !n.as_str()
                .unwrap_or_default()
                .contains("differ from the tree")
        }));
    }
    fixture.write("src/b.rs", "pub struct Token;\npub fn build() { }\n");
    let out = fixture.path().join("target/facts.json");
    let saved = json(
        fixture.path(),
        &[
            "query",
            "--json",
            "--facts",
            out.to_str().expect("path"),
            program,
        ],
    );
    let notes: Vec<&str> = saved["notes"]
        .as_array()
        .expect("notes")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert!(
        notes
            .iter()
            .any(|n| n.contains("1 of the export's") && n.contains("src/b.rs")),
        "{notes:?}"
    );
}

#[test]
fn a_question_about_use_splits_into_three_outcomes() {
    let fixture = fixture();
    let program = r#"
        called{f: F} :- call{to: F}.
        maybe{name: S} :- unknown_call{spelling: W}, last_segment(W, S).
        use{name: N, status: "called"} :- function{id: F, name: N, public: true}, called{f: F}.
        use{name: N, status: "candidate"} :-
            function{id: F, name: N, public: true}, !called{f: F}, last_segment(N, S), maybe{name: S}.
        use{name: N, status: "unobserved"} :-
            function{id: F, name: N, public: true}, !called{f: F}, last_segment(N, S), !maybe{name: S}.
        ?- use{name: N, status: S}.
    "#;
    let got: BTreeMap<String, String> = rows(fixture.path(), program)
        .iter()
        .map(|r| (s(&r[0]), s(&r[1])))
        .collect();
    assert_eq!(got.get("build").map(String::as_str), Some("called"));
    // `len` is called through a typed parameter; `go` only through a
    // receiver Atlas could not type, so it may be the target.
    assert_eq!(got.get("Token::len").map(String::as_str), Some("called"));
    assert_eq!(got.get("Token::go").map(String::as_str), Some("candidate"));
    assert_eq!(got.get("unused").map(String::as_str), Some("unobserved"));
    // The same question without the unknowns is refused, and runs only when
    // accepted, with the acceptance printed.
    let naive =
        "called{f: F} :- call{to: F}.\n?- function{id: F, name: N, public: true}, !called{f: F}.";
    let refused = run(fixture.path(), &["query", naive]);
    assert_eq!(refused.status.code(), Some(1));
    let error = String::from_utf8_lossy(&refused.stderr);
    assert!(
        error.contains("absence: this query negates `called`"),
        "{error}"
    );
    assert!(error.contains("--accept absence"), "{error}");
    let accepted = run(fixture.path(), &["query", "--accept", "absence", naive]);
    assert!(accepted.status.success());
    let text = String::from_utf8_lossy(&accepted.stdout);
    assert!(text.contains("accepted absence: query:2:1:"), "{text}");
    assert!(text.contains("unused"), "{text}");
}

#[test]
fn exits_say_whether_the_program_ran_not_what_it_found() {
    let fixture = fixture();
    let empty = run(
        fixture.path(),
        &["query", r#"?- function{name: "nothing"}."#],
    );
    assert!(empty.status.success());
    assert!(String::from_utf8_lossy(&empty.stdout).contains("(no rows)"));
    let typo = run(fixture.path(), &["query", "?- functon{name: N}."]);
    assert_eq!(typo.status.code(), Some(1));
    let error = String::from_utf8_lossy(&typo.stderr);
    assert!(
        error.contains("query:1:4: no relation `functon`; closest: `function`"),
        "{error}"
    );
    let prelude = run(fixture.path(), &["query", "--prelude"]);
    assert!(
        String::from_utf8_lossy(&prelude.stdout)
            .contains("file_calls{from: A, to: B, calls: count(C)}")
    );
    let file = fixture.path().join("q.dl");
    std::fs::write(&file, "?- census{metric: \"files\", n: N}.").expect("program");
    let answer = json(
        fixture.path(),
        &["query", "--json", "-f", file.to_str().expect("path")],
    );
    assert_eq!(answer["tables"][0]["columns"], serde_json::json!(["N"]));
    assert_eq!(answer["tables"][0]["kinds"], serde_json::json!(["integer"]));
    assert!(answer["facts"].as_str().expect("facts").contains("lexical"));
}
