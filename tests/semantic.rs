//! `--scip <FILE>` adds semantic call evidence beside a command's own output
//! and changes nothing else.
mod support;
use serde_json::Value;
use std::{fs, process::Command};
use support::Fixture;

const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/crates/atlas-lib/tests/fixtures/semantic"
);

fn fixture() -> Fixture {
    let file = |path: &str| fs::read_to_string(format!("{FIXTURE}/{path}")).expect(path);
    let f = Fixture::new(&[
        ("Cargo.toml", &file("Cargo.toml")),
        ("src/lib.rs", &file("src/lib.rs")),
    ]);
    fs::copy(format!("{FIXTURE}/index.scip"), f.path().join("index.scip")).expect("copy index");
    f
}
fn atlas(f: &Fixture, args: &[&str]) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_cargo-atlas"))
        .current_dir(f.path())
        .args(args)
        .arg("--root")
        .arg(f.path())
        .output()
        .expect("run cargo-atlas");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("JSON output")
}

#[test]
fn dump_adds_a_semantic_field_and_leaves_the_rest_unchanged() {
    let f = fixture();
    for population in [None, Some("all")] {
        let mut base = vec!["dump", "--json"];
        base.extend(population.iter().flat_map(|p| ["--population", p]));
        let plain = atlas(&f, &base);
        assert!(plain.get("semantic").is_none());
        let mut args = base.clone();
        args.extend(["--scip", "index.scip"]);
        let mut with = atlas(&f, &args);
        let semantic = with
            .as_object_mut()
            .expect("dump object")
            .remove("semantic")
            .expect("semantic field");
        assert_eq!(with, plain, "--scip only adds a field");
        assert_eq!(semantic["evidence"], "semantic");
        assert_eq!(semantic["status"], "available");
        assert_eq!(semantic["agreement"]["agrees"], 2);
        assert!(semantic["calls"][0]["qualified"]["target"].is_string());
    }
}

#[test]
fn an_unreadable_index_is_unavailable_and_the_command_still_answers() {
    let f = fixture();
    let dump = atlas(&f, &["dump", "--json", "--scip", "absent.scip"]);
    assert_eq!(dump["semantic"]["status"], "unavailable");
    assert!(
        dump["semantic"]["unavailable"]
            .as_str()
            .is_some_and(|reason| reason.contains("cannot read"))
    );
    assert!(dump["files"].is_array());
}

#[test]
fn flow_writes_semantic_evidence_beside_its_workflow_evidence() {
    let f = fixture();
    let run = |extra: &[&str]| {
        let output = Command::new(env!("CARGO_BIN_EXE_cargo-atlas"))
            .current_dir(f.path())
            .args(["flow", "--path", "src/lib.rs", "--function", "generic"])
            .args(extra)
            .arg("--root")
            .arg(f.path())
            .output()
            .expect("run flow");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).expect("UTF-8 text")
    };
    let evidence = f
        .path()
        .join("target/atlas/generic-declarations-workflow.semantic.json");
    let plain = run(&[]);
    assert!(!evidence.exists());
    let with = run(&["--scip", "index.scip"]);
    assert_eq!(
        with.replace(
            "semantic call evidence (supplied SCIP index, host cfg): target/atlas/generic-declarations-workflow.semantic.json\n",
            ""
        ),
        plain,
        "--scip only adds the evidence link"
    );
    let reading: Value =
        serde_json::from_slice(&fs::read(evidence).expect("semantic evidence")).expect("JSON");
    assert_eq!(reading["status"], "available");
    let call = &reading["calls"][0];
    assert_eq!(call["spelling"], "run");
    assert_eq!(call["semantic"]["reading"], "trait_method");
    assert!(
        call.get("agreement").is_none(),
        "flow reads without a qualified comparison"
    );
    assert_eq!(
        fs::read(f.path().join("index.scip")).expect("index"),
        fs::read(format!("{FIXTURE}/index.scip")).expect("committed index"),
        "the index is an input and stays unchanged"
    );
}

/// The rows of a query's first table, run with `extra` flags.
fn rows(f: &Fixture, program: &str, extra: &[&str]) -> Vec<Vec<Value>> {
    let answer = atlas(f, &[&["query", "--json", program][..], extra].concat());
    answer["tables"][0]["rows"]
        .as_array()
        .expect("rows")
        .iter()
        .map(|row| row.as_array().expect("row").clone())
        .collect()
}
fn site(row: &[Value], at: usize) -> (String, u64, u64) {
    (
        row[at].as_str().expect("path").to_owned(),
        row[at + 1].as_u64().expect("line"),
        row[at + 2].as_u64().expect("column"),
    )
}

/// The semantic family is the reading `dump --scip` gives, one row per
/// recorded call: `semantic_call` when the index names a captured function,
/// `semantic_unknown` otherwise.
#[test]
fn the_semantic_family_flattens_the_reading_one_row_per_call() {
    let f = fixture();
    let scip = ["--scip", "index.scip"];
    let dump = atlas(&f, &["dump", "--json", "--scip", "index.scip"]);
    let read: std::collections::BTreeSet<(String, u64, u64)> = dump["semantic"]["calls"]
        .as_array()
        .expect("calls")
        .iter()
        .map(|call| {
            let site = &call["site"];
            (
                site["path"].as_str().expect("path").to_owned(),
                site["line"].as_u64().expect("line"),
                site["column"].as_u64().expect("column"),
            )
        })
        .collect();
    let called = rows(
        &f,
        "?- semantic_call{from: F, to: G, path: P, line: L, column: C, agreement: A}.",
        &scip,
    );
    let unknown = rows(
        &f,
        "?- semantic_unknown{path: P, line: L, column: C, reading: R}.",
        &scip,
    );
    let sites: std::collections::BTreeSet<_> = called
        .iter()
        .map(|row| site(row, 2))
        .chain(unknown.iter().map(|row| site(row, 0)))
        .collect();
    assert_eq!(sites, read);
    assert_eq!(called.len() + unknown.len(), read.len());
    // `direct` and `run` call `base`, both readings agreeing.
    let agrees = called.iter().filter(|row| row[5] == "agrees").count();
    assert_eq!(agrees, 2, "{called:?}");
    // any_call unions the readings and names which one said so.
    let any = rows(&f, "?- any_call{from: F, to: G, reading: R}.", &scip);
    assert!(any.iter().any(|row| row[2] == "semantic"));
    assert!(any.iter().any(|row| row[2] == "qualified"));
    let qualified = rows(&f, "?- call{from: F, to: G}.", &[]);
    assert_eq!(
        any.iter().filter(|row| row[2] == "qualified").count(),
        qualified.len()
    );
}

#[test]
fn the_semantic_family_needs_an_index() {
    let f = fixture();
    let output = Command::new(env!("CARGO_BIN_EXE_cargo-atlas"))
        .current_dir(f.path())
        .args([
            "query",
            "?- any_call{from: F, to: G, reading: R}.",
            "--root",
        ])
        .arg(f.path())
        .output()
        .expect("run Atlas");
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("needs an index: give --scip FILE, or --semantic"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// A stale index: the source was edited after indexing so that a call now
/// names another function at the same place and with the same length. The
/// index's symbol no longer names what is written, so it is a mismatch, not
/// an edge.
#[test]
fn a_stale_index_reads_as_mismatch_not_as_an_edge() {
    let f = fixture();
    let source = std::fs::read_to_string(f.path().join("src/lib.rs")).expect("source");
    let edited = source.replace(
        "pub fn direct() -> u8 {\n    base()\n}",
        "pub fn direct() -> u8 {\n    bass()\n}",
    );
    assert_ne!(edited, source);
    std::fs::write(f.path().join("src/lib.rs"), edited).expect("edit after indexing");
    let scip = ["--scip", "index.scip"];
    let at_28 = |row: &Vec<Value>, at: usize| row[at] == "src/lib.rs" && row[at + 1] == 28;
    let called = rows(
        &f,
        "?- semantic_call{from: F, to: G, path: P, line: L}.",
        &scip,
    );
    assert!(!called.iter().any(|row| at_28(row, 2)), "{called:?}");
    let unknown = rows(
        &f,
        "?- semantic_unknown{path: P, line: L, spelling: S, reading: R, detail: D}.",
        &scip,
    );
    let row = unknown
        .iter()
        .find(|row| at_28(row, 0))
        .expect("the edited call");
    assert_eq!(row[2], "bass");
    assert_eq!(row[3], "mismatch");
    assert!(
        row[4]
            .as_str()
            .is_some_and(|detail| detail.starts_with("index names ") && detail.contains("base()")),
        "{row:?}"
    );
}

fn answer(f: &Fixture, args: &[&str]) -> Value {
    let report = atlas(f, args);
    report["answers"][0].clone()
}

#[test]
fn semantic_callers_add_rows_by_reading_and_list_only_open_possible_calls() {
    let f = fixture();
    let semantic = ["--json", "--scip", "index.scip"];
    // Qualified: Parse::run has no resolved caller and two possible ones.
    let qualified = answer(&f, &["callers", "Parse::run", "--json"]);
    assert_eq!(qualified["resolved"].as_array().map(Vec::len), Some(0));
    assert_eq!(qualified["possibly"].as_array().map(Vec::len), Some(2));
    assert!(qualified.get("semantic").is_none());
    // Semantic: the index names Parse::run at `parse.run()` (a semantic row)
    // and leaves `stage.run()`, open trait dispatch, possible.
    let report = atlas(&f, &[&["callers", "Parse::run"][..], &semantic].concat());
    assert!(
        report["semantic"][0]
            .as_str()
            .is_some_and(|line| line.starts_with("Semantic reading (supplied): "))
    );
    let answer = &report["answers"][0];
    let resolved = answer["resolved"].as_array().expect("resolved");
    assert_eq!(resolved.len(), 1, "{answer}");
    assert_eq!(resolved[0]["reading"], "semantic");
    assert_eq!(resolved[0]["site"]["line"], 32);
    let possibly = answer["possibly"].as_array().expect("possibly");
    assert_eq!(possibly.len(), 1, "{answer}");
    assert_eq!(possibly[0]["site"]["line"], 35);
    assert_eq!(possibly[0]["semantic"], "trait-method");
    assert_eq!(answer["semantic"]["confirmed"], 1);
    assert_eq!(answer["semantic"]["open"]["trait-method"], 1);
    // A call written `label()` that the index names another function for
    // is ruled out, never a possible caller of `label`.
    let label = callers_of(&f, "label", &semantic);
    assert_eq!(label["semantic"]["other_function"], 1, "{label}");
    assert!(
        label["possibly"]
            .as_array()
            .expect("possibly")
            .iter()
            .all(|row| row["site"]["line"] != 53)
    );
    // A qualified row stays qualified; an agreeing index adds no second row.
    let base = callers_of(&f, "base", &semantic);
    let readings: Vec<_> = base["resolved"]
        .as_array()
        .expect("resolved")
        .iter()
        .map(|row| (row["site"]["line"].clone(), row["reading"].clone()))
        .collect();
    assert_eq!(
        readings,
        [
            (17.into(), "qualified".into()),
            (28.into(), "qualified".into()),
            (53.into(), "semantic".into())
        ]
    );
}
fn callers_of(f: &Fixture, selector: &str, semantic: &[&str]) -> Value {
    answer(f, &[&["callers", selector][..], semantic].concat())
}

#[test]
fn semantic_callees_resolve_what_the_index_names_and_count_the_rest() {
    let f = fixture();
    let semantic = ["--json", "--evidence", "semantic", "--scip", "index.scip"];
    let answer = answer(
        &f,
        &[&["callees", "method_on_local"][..], &semantic].concat(),
    );
    assert_eq!(answer["resolved"][0]["to_name"], "Parse::run");
    assert_eq!(answer["resolved"][0]["reading"], "semantic");
    assert_eq!(answer["unresolved"].as_array().map(Vec::len), Some(0));
    // A call to an item of another package leaves the list, counted.
    let external = callees_of(&f, "external", &semantic);
    assert!(
        external["semantic"]["elsewhere"]["external"].as_u64() >= Some(1),
        "{external}"
    );
    assert!(
        external["unresolved"]
            .as_array()
            .expect("unresolved")
            .iter()
            .all(|row| row["semantic"] != "external")
    );
}
fn callees_of(f: &Fixture, selector: &str, semantic: &[&str]) -> Value {
    answer(f, &[&["callees", selector][..], semantic].concat())
}

#[test]
fn an_index_selects_the_semantic_reading_and_the_evidence_needs_one() {
    let f = fixture();
    let run = |args: &[&str]| {
        let output = Command::new(env!("CARGO_BIN_EXE_cargo-atlas"))
            .current_dir(f.path())
            .args(args)
            .arg("--root")
            .arg(f.path())
            .output()
            .expect("run cargo-atlas");
        (
            output.status.success(),
            String::from_utf8_lossy(&output.stderr).into_owned(),
        )
    };
    let (ok, stderr) = run(&["callers", "base", "--evidence", "semantic"]);
    assert!(!ok);
    assert!(
        stderr.contains("needs an index: give --scip FILE, or --semantic"),
        "{stderr}"
    );
    let (ok, stderr) = run(&[
        "callees",
        "base",
        "--evidence",
        "qualified",
        "--scip",
        "index.scip",
    ]);
    assert!(!ok);
    assert!(
        stderr.contains("--evidence qualified conflicts with --scip and --semantic"),
        "{stderr}"
    );
    // The index alone selects the semantic reading.
    let (ok, stderr) = run(&["callers", "base", "--scip", "index.scip"]);
    assert!(ok, "{stderr}");
    let (ok, stderr) = run(&[
        "callers",
        "base",
        "--evidence",
        "semantic",
        "--scip",
        "missing.scip",
    ]);
    assert!(!ok);
    assert!(
        stderr.contains("the semantic index is unavailable: cannot read"),
        "{stderr}"
    );
}

#[test]
fn orient_with_an_index_splits_the_unresolved_calls_and_counts_its_callers() {
    let f = fixture();
    let report = atlas(&f, &["orient", "--json", "--scip", "index.scip"]);
    let resolution = &report["resolution"];
    let semantic = &resolution["semantic"];
    let open: u64 = semantic["open"]
        .as_object()
        .expect("open")
        .values()
        .filter_map(Value::as_u64)
        .sum();
    let parts = ["workspace", "external", "other_items"]
        .iter()
        .map(|key| semantic[key].as_u64().expect(key))
        .sum::<u64>()
        + open;
    // Every unknown call is in exactly one part.
    assert_eq!(
        Some(parts),
        resolution["calls_unknown"].as_u64(),
        "{semantic}"
    );
    assert!(semantic["workspace"].as_u64() >= Some(1), "{semantic}");
    assert!(
        semantic["reading"][0]
            .as_str()
            .is_some_and(|line| line.starts_with("Semantic reading (supplied): "))
    );
    // The most-called rows carry what the index adds.
    let called = report["sections"]
        .as_array()
        .expect("sections")
        .iter()
        .find(|section| section["id"] == "called")
        .expect("called");
    assert!(
        called["rows"]
            .as_array()
            .expect("rows")
            .iter()
            .all(|row| row["by_index"].is_u64())
    );
    // Without an index the JSON has neither.
    let plain = atlas(&f, &["orient", "--json"]);
    assert!(plain["resolution"].get("semantic").is_none());
}
