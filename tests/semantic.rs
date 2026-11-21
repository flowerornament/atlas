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
        .current_dir("/")
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
            .current_dir("/")
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
        .current_dir("/")
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
