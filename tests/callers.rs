//! `callers` and `callees` answer from resolved calls, and say what they could
//! not resolve: an unresolved call written with the name is a possible
//! caller, never a caller, and no answer reads as "unused".
mod support;
use std::process::Command;
use support::Fixture;

const LIB: &str = "pub mod a;\npub mod b;\npub struct S;\nimpl S {\n    pub fn method(&self) {}\n}\npub fn two() {}\npub fn one() {}\npub fn none() {}\npub fn caller_x() {\n    two();\n}\npub fn caller_y() {\n    two();\n    one();\n}\npub fn guess() {\n    let value = make();\n    value.one();\n}\npub fn mixed() {\n    two();\n    let _items: Vec<u8> = Vec::new();\n    make().go();\n}\npub fn both() {\n    crate::a::run();\n    crate::b::run();\n}\npub fn chain_a() {\n    chain_b();\n}\npub fn chain_b() {\n    chain_c();\n}\npub fn chain_c() {\n    chain_d();\n}\npub fn chain_d() {}\npub fn hidden() {\n    make().chain_c();\n}\n";

fn fixture() -> Fixture {
    Fixture::new(&[
        ("src/lib.rs", LIB),
        ("src/a.rs", "pub fn run() {}\n"),
        ("src/b.rs", "pub fn run() {}\n"),
    ])
}
fn atlas(f: &Fixture, args: &[&str]) -> (bool, String, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_cargo-atlas"))
        .current_dir(f.path())
        .env("ATLAS_CACHE", "off")
        .env_remove("ATLAS_NOTICES")
        .args(args)
        .output()
        .expect("run Atlas");
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}
fn text(f: &Fixture, args: &[&str]) -> String {
    let (ok, stdout, stderr) = atlas(f, args);
    assert!(ok, "{args:?}: {stderr}");
    assert!(stderr.is_empty(), "{args:?}: {stderr}");
    stdout
}
fn json(f: &Fixture, args: &[&str]) -> serde_json::Value {
    serde_json::from_str(&text(f, &[args, &["--as", "json"]].concat())).expect("JSON")
}

#[test]
fn callers_lists_resolved_calls_and_possible_ones_apart() {
    let f = fixture();
    // AT7: resolved callers, one per site.
    let two = text(&f, &["callers", "two"]);
    assert!(
        two.starts_with(
            "callers of two  src/lib.rs:7\n  resolved: 3 calling functions, 3 call sites\n"
        ),
        "{two}"
    );
    assert!(two.contains("      src/lib.rs:11  caller_x\n"), "{two}");
    // A resolved caller and an unresolved call written "one", kept apart.
    let one = text(&f, &["callers", "one"]);
    assert!(
        one.contains(
            "  resolved: 1 calling function, 1 call site\n      src/lib.rs:15  caller_y\n"
        ),
        "{one}"
    );
    assert!(
        one.contains("  possibly: 1 unresolved call is written as \"one\"\n      1  receiver-initializer-unknown  e.g. value.one() at src/lib.rs:19\n"),
        "{one}"
    );
    let report = json(&f, &["callers", "one"]);
    assert_eq!(report["schema"], "atlas.callers/1");
    let answer = &report["answers"][0];
    assert_eq!(answer["resolved"].as_array().map(Vec::len), Some(1));
    assert_eq!(answer["resolved"][0]["reading"], "qualified");
    assert_eq!(answer["resolved"][0]["depth"], 1);
    assert_eq!(answer["possibly"][0]["spelling"], "one");
    assert_eq!(
        answer["possibly_by_kind"]["receiver-initializer-unknown"],
        1
    );
    // R19: neither resolved nor possible callers is never "unused".
    let none = text(&f, &["callers", "none"]);
    assert!(
        none.contains("  no resolved callers, and no unresolved call is written as \"none\"; calls Atlas does not record (inside macros, or through a function passed elsewhere as a value) are not covered\n"),
        "{none}"
    );
    assert!(!none.contains("unused"));
    // R16: two functions named `run` are answered on their own.
    let run = text(&f, &["callers", "run"]);
    assert!(
        run.starts_with("run matches 2 functions; each is answered on its own\n"),
        "{run}"
    );
    assert!(
        run.contains("callers of a::run  src/a.rs:1\n")
            && run.contains("callers of b::run  src/b.rs:1\n"),
        "{run}"
    );
    assert_eq!(
        json(&f, &["callers", "run"])["answers"]
            .as_array()
            .map(Vec::len),
        Some(2)
    );
    // The other selector forms reach the same function.
    for selector in ["a::run", "src/a.rs:1", "src/a.rs:1:7"] {
        let answer = json(&f, &["callers", selector]);
        assert_eq!(
            answer["answers"].as_array().map(Vec::len),
            Some(1),
            "{selector}"
        );
        assert_eq!(
            answer["answers"][0]["function"]["path"], "src/a.rs",
            "{selector}"
        );
    }
    assert_eq!(
        json(&f, &["callers", "S::method"])["answers"][0]["function"]["name"],
        "S::method"
    );
    assert_eq!(
        json(&f, &["callers", "--path", "src/b.rs", "--function", "run"])["answers"][0]["function"]
            ["path"],
        "src/b.rs"
    );
    // R15.
    let (ok, stdout, stderr) = atlas(&f, &["callers", "nowhere"]);
    assert!(!ok && stdout.is_empty());
    assert_eq!(
        stderr,
        "atlas: no function matches \"nowhere\"; try cargo atlas name nowhere\n"
    );
    // --in keeps callers in part of the project, never which function is asked.
    let kept = json(&f, &["callers", "two", "--in", "src/a.rs"]);
    assert_eq!(
        kept["answers"][0]["resolved"].as_array().map(Vec::len),
        Some(0)
    );
}

#[test]
fn callees_lists_resolved_targets_and_counts_the_rest() {
    let f = fixture();
    // AT9: one workspace function, Vec::new and an unknown receiver.
    let mixed = text(&f, &["callees", "mixed"]);
    assert!(
        mixed.contains("  resolved: 1 call to 1 function\n      src/lib.rs:22  two\n"),
        "{mixed}"
    );
    assert!(
        mixed.contains("  unresolved: 3 calls; the rest are external or unknown, not absent\n      2  unresolved-path        e.g. Vec::new() at src/lib.rs:23\n      1  receiver-type-unknown  e.g. make().go() at src/lib.rs:24\n"),
        "{mixed}"
    );
    let report = json(&f, &["callees", "mixed"]);
    assert_eq!(report["schema"], "atlas.callees/1");
    assert_eq!(
        report["answers"][0]["unresolved"].as_array().map(Vec::len),
        Some(3)
    );
    assert_eq!(
        report["answers"][0]["unresolved_by_kind"]["unresolved-path"],
        2
    );
}

#[test]
fn name_relations_says_where_calls_are_answered() {
    let f = fixture();
    // AT13: co-mentions are not calls.
    let relations = text(&f, &["name", "two", "--relations"]);
    assert!(
        relations.contains("\nfor calls, cargo atlas callers two\n"),
        "{relations}"
    );
    assert!(!text(&f, &["name", "two"]).contains("for calls"));
    // `name` reads `Type::method`, the spelling `orient` and `callers` print.
    assert!(text(&f, &["name", "S::method"]).starts_with("S::method: 1 declaration site\n"));
}

#[test]
fn depth_follows_resolved_calls_and_counts_what_it_cannot() {
    let f = fixture();
    // AT8: a -> b -> c -> d. Depth 2 from d lists c at 1 and b at 2, not a.
    let report = json(&f, &["callers", "chain_d", "--depth", "2"]);
    assert_eq!(report["depth"], 2);
    let rows: Vec<(String, u64)> = report["answers"][0]["resolved"]
        .as_array()
        .expect("rows")
        .iter()
        .map(|row| {
            (
                row["from_name"].as_str().expect("name").to_owned(),
                row["depth"].as_u64().expect("depth"),
            )
        })
        .collect();
    assert_eq!(rows, [("chain_c".to_owned(), 1), ("chain_b".to_owned(), 2)]);
    assert_eq!(report["answers"][0]["resolved"][1]["via_name"], "chain_c");
    // An unresolved call written as a depth-1 caller's name is counted at depth 2.
    assert_eq!(report["answers"][0]["possibly_by_depth"]["2"], 1);
    assert_eq!(report["answers"][0]["possibly_beyond_depth_1"], 1);
    let text = text(&f, &["callers", "chain_d", "--depth", "2"]);
    assert!(
        text.contains(
            "       1  src/lib.rs:37  chain_c\n       2  src/lib.rs:34  chain_b → chain_c\n"
        ),
        "{text}"
    );
    assert!(text.contains("  possibly, beyond depth 1: 1 unresolved call written as the name of a caller (depth 2: 1)\n"), "{text}");
    // Forward from a: b, c, d at depths 1, 2, 3.
    let callees = json(&f, &["callees", "chain_a", "--depth", "16"]);
    let depths: Vec<u64> = callees["answers"][0]["resolved"]
        .as_array()
        .expect("rows")
        .iter()
        .map(|row| row["depth"].as_u64().expect("depth"))
        .collect();
    assert_eq!(depths, [1, 2, 3]);
    // Depth above 16 is refused.
    let (ok, _, stderr) = atlas(&f, &["callers", "chain_d", "--depth", "17"]);
    assert!(!ok && stderr.contains("17 is not in 1..=16"), "{stderr}");
}

#[test]
fn evidence_is_reserved_and_names_what_to_run() {
    let f = fixture();
    // §3.9: auto and qualified are the command's reading.
    let auto = text(&f, &["callers", "two"]);
    assert_eq!(
        auto,
        text(&f, &["callers", "two", "--evidence", "qualified"])
    );
    assert_eq!(auto, text(&f, &["callers", "two", "--evidence", "auto"]));
    for (args, message) in [
        (
            &["callers", "two", "--evidence", "lexical"][..],
            "atlas: callers needs resolved calls; for lexical co-mentions use cargo atlas name two --relations\n",
        ),
        (
            &["callees", "two", "--evidence", "semantic"],
            "atlas: callees --evidence semantic needs an index: give --scip FILE, or --semantic, which runs rust-analyzer and with it the project's build scripts\n",
        ),
        (
            &["dependencies", "--evidence", "lexical"],
            "atlas: dependencies needs qualified relations; for lexical file mentions use cargo atlas map --level files\n",
        ),
        (
            &[
                "flow",
                "--path",
                "src/lib.rs",
                "--function",
                "two",
                "--evidence",
                "semantic",
            ],
            "atlas: flow does not answer from the semantic reading; flow --scip FILE writes one beside the drawing as separate evidence\n",
        ),
    ] {
        let (ok, stdout, stderr) = atlas(&f, args);
        assert!(!ok && stdout.is_empty(), "{args:?}");
        assert_eq!(stderr, message, "{args:?}");
    }
    // AT14: the question map names callers and callees, and the output flag.
    let (_, help, _) = atlas(&f, &["--help"]);
    assert!(
        help.contains("who calls X, what X calls: callers, callees"),
        "{help}"
    );
    assert!(help.contains("Add --as json"), "{help}");
}
