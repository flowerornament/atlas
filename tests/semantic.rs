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
