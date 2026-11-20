//! The flat relations export: its schema contract, its rows' agreement with
//! the reports that read the same observations, and that ambiguity and
//! unknowns stay rows instead of disappearing.
mod support;
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    process::{Command, Output},
};
use support::Fixture;

fn fixture() -> Fixture {
    Fixture::new(&[
        (
            "src/lib.rs",
            "//! root\npub mod a;\npub mod b;\nmod nested {\n    pub mod inner {\n        pub struct Deep;\n        pub fn make() -> Deep { Deep }\n    }\n}\npub use std::collections::*;\npub fn entry(token: a::Token) -> usize {\n    helper();\n    let deep = nested::inner::make();\n    drop(deep);\n    token.len()\n}\nfn helper() {}\npub const NAMES: [&str; 2] = [\"x\", \"y\"];\n",
        ),
        (
            "src/a.rs",
            "pub struct Token(pub u8);\nimpl Token {\n    pub fn len(&self) -> usize { 1 }\n}\npub fn uses(t: Token) -> HashMap { let _ = t; todo() }\n",
        ),
        (
            "src/b.rs",
            "pub struct Token;\npub type Alias = Token;\npub struct Holder { pub token: Token, pub count: Vec<u8> }\n",
        ),
        ("src/orphan.rs", "pub struct Lost;\n"),
    ])
}
fn run(fixture: &Fixture, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_cargo-atlas"))
        .current_dir("/")
        .arg("--root")
        .arg(fixture.path())
        .args(args)
        .output()
        .expect("Atlas process")
}
fn stdout(fixture: &Fixture, args: &[&str]) -> Vec<u8> {
    let output = run(fixture, args);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}
fn json(fixture: &Fixture, args: &[&str]) -> Value {
    serde_json::from_slice(&stdout(fixture, args)).expect("Atlas JSON")
}
/// The rows of `relation` as maps from column name to value.
fn records(facts: &Value, relation: &str) -> Vec<BTreeMap<String, Value>> {
    let table = &facts["relations"][relation];
    let columns: Vec<&str> = table["columns"]
        .as_array()
        .expect("columns")
        .iter()
        .map(|c| c.as_str().expect("column name"))
        .collect();
    table["rows"]
        .as_array()
        .expect("rows")
        .iter()
        .map(|row| {
            columns
                .iter()
                .map(|c| (*c).to_owned())
                .zip(row.as_array().expect("row").iter().cloned())
                .collect()
        })
        .collect()
}
fn text<'a>(row: &'a BTreeMap<String, Value>, column: &str) -> &'a str {
    row[column].as_str().expect("text column")
}

#[test]
fn schema_matches_the_committed_contract() {
    let fixture = fixture();
    let printed = String::from_utf8(stdout(&fixture, &["facts", "--schema"])).expect("UTF-8");
    let committed = include_str!("snapshots/facts-schema.json");
    assert_eq!(
        printed, committed,
        "the facts schema changed. Within schema v1 a change may only add relations or append \
         columns; if this one does, update tests/snapshots/facts-schema.json and docs/facts.md \
         (cargo run -- facts --schema > tests/snapshots/facts-schema.json)"
    );
}

#[test]
fn docs_describe_every_relation_and_column() {
    let docs = include_str!("../docs/facts.md");
    let schema: Value = serde_json::from_str(include_str!("snapshots/facts-schema.json")).unwrap();
    for relation in schema["relations"].as_array().expect("relations") {
        let name = relation["name"].as_str().expect("name");
        assert!(docs.contains(&format!("### `{name}`")), "{name}");
        for column in relation["columns"].as_array().expect("columns") {
            let column = column["name"].as_str().expect("column");
            assert!(
                docs.contains(&format!("`{column}`")),
                "docs/facts.md does not describe {name}.{column}"
            );
        }
    }
}

#[test]
fn every_row_has_its_schema_columns_and_kinds() {
    let fixture = fixture();
    let facts = json(&fixture, &["facts"]);
    let schema = json(&fixture, &["facts", "--schema"]);
    assert_eq!(facts["schema"], schema["schema"]);
    for relation in schema["relations"].as_array().expect("relations") {
        let name = relation["name"].as_str().expect("name");
        let table = &facts["relations"][name];
        let columns = relation["columns"].as_array().expect("columns");
        let names: Vec<_> = columns.iter().map(|c| c["name"].clone()).collect();
        assert_eq!(table["columns"].as_array().expect(name), &names, "{name}");
        assert_eq!(table["family"], relation["family"], "{name}");
        assert_eq!(table["basis"], relation["basis"], "{name}");
        assert_eq!(table["unknowns"], relation["unknowns"], "{name}");
        for row in table["rows"].as_array().expect(name) {
            let row = row.as_array().expect("row");
            assert_eq!(row.len(), columns.len(), "{name}: {row:?}");
            for (cell, column) in row.iter().zip(columns) {
                let fits = match column["kind"].as_str().expect("kind") {
                    "text" => cell.is_string(),
                    "integer" => cell.is_u64(),
                    "boolean" => cell.is_boolean(),
                    other => panic!("unknown kind {other}"),
                };
                assert!(fits, "{name}.{}: {cell}", column["name"]);
            }
        }
    }
}

#[test]
fn ambiguity_and_unresolved_mentions_stay_rows_with_their_basis() {
    let fixture = fixture();
    let facts = json(&fixture, &["facts", "--families", "lexical"]);
    assert!(facts["relations"]["call"].is_null(), "lexical only");
    let mentions = records(&facts, "mention");
    let targets = records(&facts, "mention_target");
    let unresolved: BTreeSet<_> = records(&facts, "mention_unresolved")
        .iter()
        .map(|row| text(row, "mention").to_owned())
        .collect();
    let mut by_mention = BTreeMap::<&str, Vec<&str>>::new();
    for row in &targets {
        by_mention
            .entry(text(row, "mention"))
            .or_default()
            .push(text(row, "basis"));
    }
    for mention in &mentions {
        let id = text(mention, "id");
        match (by_mention.get(id), unresolved.contains(id)) {
            (Some(bases), false) if bases == &["unique"] => {}
            (Some(bases), false) => {
                assert!(bases.len() > 1, "{id}: {bases:?}");
                assert!(bases.iter().all(|b| *b == "candidate"), "{id}: {bases:?}");
            }
            (None, true) => {}
            other => panic!("{id} is neither resolved nor unresolved once: {other:?}"),
        }
    }
    // `Token` is declared in a.rs and b.rs; `uses(t: Token)` in a.rs prefers
    // its own package's declarations, both of them, so it stays ambiguous.
    let token_candidates: BTreeSet<_> = targets
        .iter()
        .filter(|row| text(row, "basis") == "candidate")
        .map(|row| text(row, "target"))
        .collect();
    assert!(
        token_candidates.contains("src/a.rs:1:Token"),
        "{token_candidates:?}"
    );
    assert!(
        token_candidates.contains("src/b.rs:1:Token"),
        "{token_candidates:?}"
    );
    // The glob import of an external module makes `HashMap` unresolved, not absent.
    assert!(!unresolved.is_empty());
}

#[test]
fn inline_modules_and_disconnected_files_are_recorded() {
    let fixture = fixture();
    let facts = json(&fixture, &["facts", "--families", "lexical"]);
    let declarations = records(&facts, "declaration");
    let deep = declarations
        .iter()
        .find(|row| text(row, "name") == "Deep")
        .expect("Deep declaration");
    assert_eq!(text(deep, "inline_modules"), "nested::inner");
    let mounted: BTreeSet<_> = records(&facts, "mount")
        .iter()
        .map(|row| text(row, "path").to_owned())
        .collect();
    let files: BTreeSet<_> = records(&facts, "file")
        .iter()
        .map(|row| text(row, "path").to_owned())
        .collect();
    assert!(files.contains("src/orphan.rs"), "{files:?}");
    assert!(!mounted.contains("src/orphan.rs"), "{mounted:?}");
    assert!(mounted.contains("src/a.rs"));
}

#[test]
fn qualified_rows_bridge_to_exactly_one_lexical_declaration() {
    let fixture = fixture();
    let facts = json(&fixture, &["facts"]);
    let declarations: BTreeMap<_, _> = records(&facts, "declaration")
        .into_iter()
        .map(|row| (text(&row, "id").to_owned(), row))
        .collect();
    for (relation, kinds) in [
        ("function", &["Fn"][..]),
        ("type", &["Struct", "Enum", "Union", "TypeAlias"][..]),
    ] {
        let rows = records(&facts, relation);
        assert!(!rows.is_empty(), "{relation}");
        let mut seen = BTreeSet::new();
        for row in &rows {
            let declaration = text(row, "declaration");
            let lexical = declarations
                .get(declaration)
                .unwrap_or_else(|| panic!("{relation} {} has no declaration", text(row, "id")));
            assert_eq!(text(lexical, "path"), text(row, "path"));
            assert_eq!(lexical["line"], row["line"]);
            assert!(kinds.contains(&text(lexical, "kind")), "{lexical:?}");
            assert!(seen.insert(declaration), "{declaration} bridged twice");
        }
    }
}

#[test]
fn counts_agree_with_census_and_dependencies() {
    let fixture = fixture();
    let facts = json(&fixture, &["facts"]);
    let census = json(&fixture, &["census", "--json"]);
    let files = records(&facts, "file");
    let paths: BTreeSet<_> = files.iter().map(|row| text(row, "path")).collect();
    let production: BTreeSet<_> = files
        .iter()
        .filter(|row| row["test_only"] == false)
        .map(|row| text(row, "path"))
        .collect();
    let declarations: Vec<_> = records(&facts, "declaration")
        .into_iter()
        .filter(|row| row["test_only"] == false)
        .collect();
    let mut kinds = BTreeMap::<String, usize>::new();
    for row in &declarations {
        *kinds.entry(text(row, "kind").to_owned()).or_default() += 1;
    }
    let candidates: BTreeSet<_> = records(&facts, "mention_target")
        .iter()
        .filter(|row| text(row, "basis") == "candidate")
        .map(|row| text(row, "mention").to_owned())
        .collect();
    assert_eq!(census["files"], paths.len());
    assert_eq!(
        census["test_only_files"],
        paths.difference(&production).count()
    );
    assert_eq!(census["declarations"], declarations.len());
    assert_eq!(census["kinds"], serde_json::to_value(&kinds).unwrap());
    assert_eq!(census["relationships"], records(&facts, "mention").len());
    assert_eq!(census["ambiguous"], candidates.len());
    assert_eq!(
        census["unresolved"],
        records(&facts, "mention_unresolved").len()
    );
    assert_eq!(census["diagnostics"], records(&facts, "diagnostic").len());

    let dependencies = json(&fixture, &["dependencies", "--all-facts", "--json"]);
    let evidence = &dependencies["facts"];
    for (relation, key) in [
        ("function", "functions"),
        ("type", "types"),
        ("call", "calls"),
        ("unknown_call", "unknown_calls"),
        ("reference", "references"),
        ("unknown_type", "unknown_types"),
        ("construction", "constructors"),
        ("default_call", "default_calls"),
        ("constant", "constants"),
    ] {
        assert_eq!(
            records(&facts, relation).len(),
            evidence[key].as_array().expect(key).len(),
            "{relation}"
        );
    }
    let calls: BTreeSet<_> = records(&facts, "call")
        .iter()
        .map(|row| (text(row, "from").to_owned(), text(row, "to").to_owned()))
        .collect();
    let reported: BTreeSet<_> = evidence["calls"]
        .as_array()
        .expect("calls")
        .iter()
        .map(|c| {
            (
                c["from"].as_str().unwrap().to_owned(),
                c["to"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    assert_eq!(calls, reported);
    assert!(!calls.is_empty(), "the fixture has resolved calls");
    assert!(
        !records(&facts, "unknown_call").is_empty(),
        "and unresolved ones"
    );
}

#[test]
fn json_lines_hold_the_same_rows_with_named_columns() {
    let fixture = fixture();
    let facts = json(&fixture, &["facts"]);
    let lines = String::from_utf8(stdout(&fixture, &["facts", "--format", "jsonl"])).unwrap();
    let mut lines = lines
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap());
    let header = lines.next().expect("header");
    assert_eq!(header["schema"], facts["schema"]);
    assert_eq!(header["families"], facts["families"]);
    let mut counts = BTreeMap::<String, usize>::new();
    for line in lines {
        let relation = line["relation"].as_str().expect("relation").to_owned();
        let table = &facts["relations"][&relation];
        assert_eq!(
            line.as_object().unwrap().len(),
            table["columns"].as_array().unwrap().len() + 1,
            "{relation}"
        );
        *counts.entry(relation).or_default() += 1;
    }
    for (name, table) in facts["relations"].as_object().unwrap() {
        let rows = table["rows"].as_array().unwrap().len();
        assert_eq!(counts.get(name).copied().unwrap_or(0), rows, "{name}");
    }
}

#[test]
fn out_writes_a_file_and_never_overwrites_an_input() {
    let fixture = fixture();
    let destination = fixture.path().join("target/facts.json");
    let output = run(
        &fixture,
        &["facts", "--out", destination.to_str().expect("UTF-8 path")],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty());
    let written: Value = serde_json::from_slice(&std::fs::read(&destination).unwrap()).unwrap();
    assert_eq!(written["schema"], 1);
    for input in ["src/lib.rs", "Cargo.toml"] {
        let path = fixture.path().join(input);
        let before = std::fs::read(&path).unwrap();
        let output = run(&fixture, &["facts", "--out", path.to_str().unwrap()]);
        assert!(!output.status.success(), "{input} was overwritten");
        assert_eq!(std::fs::read(&path).unwrap(), before, "{input}");
    }
}
