//! Explicit population selection does not replace the unflagged graph-query contract.
mod support;
use serde_json::Value;
use std::process::{Command, Output};
use support::Fixture;

fn fixture() -> Fixture {
    Fixture::new(&[
        (
            "src/lib.rs",
            "//! root\npub mod items;\npub use crate::items::Public;\n",
        ),
        (
            "src/items.rs",
            "//! subjects\n/// public\npub struct Public;\npub(crate) struct Internal;\nstruct Private;\n#[cfg(test)]\nfn helper(item: Public) -> Public { item }\n",
        ),
    ])
}
fn run(fixture: &Fixture, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_atlas"))
        .current_dir("/")
        .args(["--root"])
        .arg(fixture.path())
        .args(args)
        .output()
        .expect("Atlas query process")
}
fn report(fixture: &Fixture, args: &[&str]) -> Value {
    let output = run(fixture, args);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("Atlas JSON report")
}
#[test]
fn population_queries_and_legacy_graph_queries_keep_distinct_explicit_contracts() {
    let fixture = fixture();
    let legacy = report(&fixture, &["census", "--json"]);
    assert!(legacy["declarations"].is_number());
    let selected = report(&fixture, &["census", "--population", "pub-ish", "--json"]);
    assert_eq!(selected["kind"], "rows");
    assert_eq!(selected["question"], "census");
    assert_eq!(selected["total"]["types"], 2);
    let public = report(&fixture, &["census", "--pub-only", "--json"]);
    assert_eq!(public["total"]["types"], 1);
    assert_eq!(
        public["total"]["token_lines"],
        selected["total"]["token_lines"]
    );
    let named = report(
        &fixture,
        &["name", "items::Public", "--population", "pub", "--json"],
    );
    assert_eq!(named["matches"][0]["file"], "src/items.rs");
    let bare = report(
        &fixture,
        &["name", "Public", "--population", "pub", "--json"],
    );
    assert_eq!(bare["reexports"][0]["file"], "src/lib.rs");
    assert!(named["reexports"].as_array().expect("reexports").is_empty());
    assert!(
        named["edges"]
            .as_array()
            .expect("edges")
            .iter()
            .any(|edge| edge["source"]["name"] == "helper")
    );
    let old_name = report(&fixture, &["name", "Public", "--json"]);
    assert_eq!(old_name["matches"][0]["declaration"]["name"], "Public");
    let module = report(
        &fixture,
        &["module", "src/items.rs", "--population", "pub", "--json"],
    );
    assert_eq!(module["contexts"][0]["tally"]["types"], 1);
    assert!(module["basis"]["sources"]["src/items.rs"].is_string());
}
#[test]
fn invalid_or_conflicting_populations_refuse_and_text_states_its_evidence() {
    let fixture = fixture();
    assert!(
        !run(&fixture, &["census", "--population", "invented"])
            .status
            .success()
    );
    assert!(
        !run(&fixture, &["census", "--population", "all", "--pub-only"])
            .status
            .success()
    );
    let output = run(&fixture, &["name", "Public", "--pub-only"]);
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).expect("text report");
    assert!(text.contains("textual references"));
    assert!(text.contains("declaration co-mentions"));
}

#[test]
fn explicit_population_dump_exposes_exports_without_changing_graph_dump() {
    let fixture = fixture();
    let legacy = report(&fixture, &["dump", "--json"]);
    assert!(legacy["files"].is_array());
    let native = report(&fixture, &["dump", "--population", "pub", "--json"]);
    assert!(native["declarations"].is_array());
    assert_eq!(native["question"], "dump");
    assert_eq!(native["schema"], 1);
    let export = native["exports"]
        .as_array()
        .expect("exports")
        .iter()
        .find(|e| e["local_name"] == "Public")
        .expect("public facade");
    assert_eq!(export["kind"], "facade");
    assert_eq!(export["evidence"], "qualified-suffix");
    assert_eq!(export["target"]["status"], "unique");
    let id = export["target"]["value"].as_str().expect("candidate id");
    assert!(
        native["declarations"]
            .as_array()
            .expect("declarations")
            .iter()
            .any(|d| d["id"] == id)
    );
}

#[test]
fn module_items_and_graph_are_explicit_native_presentation_controls() {
    let fixture = fixture();
    let output = run(&fixture, &["module", "src/items.rs", "--items", "--graph"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).expect("module text");
    assert!(text.contains("Private"));
    assert!(text.contains("declaration co-mentions"));
    let limited = run(&fixture, &["module", "src/items.rs", "--population", "pub"]);
    assert!(limited.status.success());
    assert!(
        !String::from_utf8(limited.stdout)
            .expect("text")
            .contains("declaration co-mentions")
    );
}
#[test]
fn diff_items_and_incidents_keep_shape_only_compatibility() {
    let fixture = fixture();
    let git = |args: &[&str]| {
        let output = Command::new("git")
            .current_dir(fixture.path())
            .args(args)
            .output()
            .expect("Git fixture");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout)
            .expect("Git text")
            .trim()
            .to_owned()
    };
    git(&["init", "-q"]);
    git(&["add", "."]);
    git(&[
        "-c",
        "user.name=Atlas Test",
        "-c",
        "user.email=atlas@example.invalid",
        "commit",
        "-qm",
        "before",
    ]);
    let before = git(&["rev-parse", "HEAD"]);
    fixture.write("src/items.rs", "//! subjects\npub struct Candidate;\n");
    let legacy = report(&fixture, &["diff", &before, "--json"]);
    assert!(legacy["report"]["changes"].is_array());
    let changed = report(&fixture, &["diff", &before, "--items", "--graph", "--json"]);
    assert!(
        changed["report"]["items"]["declarations"]
            .as_array()
            .expect("declarations")
            .iter()
            .any(|change| change["change"] == "added"
                && change["after"]["site"]["name"] == "Candidate")
    );
    assert!(changed["report"]["items"]["relationships"].is_array());
    let selected = report(
        &fixture,
        &["diff", &before, "--items", "--pub-only", "--json"],
    );
    assert!(selected["report"]["items"]["relationships"].is_null());
    assert_eq!(selected["report"]["items"]["population"], "public");
    assert!(
        !run(&fixture, &["diff", &before, "--graph"])
            .status
            .success()
    );
    assert!(
        !run(&fixture, &["diff", &before, "--pub-only"])
            .status
            .success()
    );
}

#[test]
fn compatibility_name_alias_warns_with_both_population_contracts() {
    let fixture = fixture();
    for selection in [&[][..], &["--population", "pub"][..]] {
        let mut alias_args = vec!["related", "Public", "--json"];
        alias_args.extend_from_slice(selection);
        let alias = run(&fixture, &alias_args);
        assert!(alias.status.success());
        assert!(String::from_utf8_lossy(&alias.stderr).contains("compatibility alias"));
        let mut name_args = vec!["name", "Public", "--relations", "--json"];
        name_args.extend_from_slice(selection);
        let canonical = run(&fixture, &name_args);
        assert!(canonical.status.success());
        assert_eq!(alias.stdout, canonical.stdout);
        assert!(!String::from_utf8_lossy(&canonical.stderr).contains("compatibility alias"));
    }
}
