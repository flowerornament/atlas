//! Named aliases remain written declarations, not expanded structures or ownership claims.
mod support;
use atlas::{
    comparison, discovery,
    representations::{self, Panel, Query, Role, Selector},
};
use support::Fixture;

fn facts(source: &str) -> (Fixture, representations::Facts) {
    let fixture = Fixture::new(&[("src/lib.rs", source)]);
    let metadata = cargo_metadata::MetadataCommand::new()
        .current_dir(&fixture.0)
        .no_deps()
        .exec()
        .expect("alias fixture metadata");
    let graph = discovery::scan_metadata(&metadata).expect("alias fixture capture");
    let facts =
        representations::extract(&fixture.0, &metadata, &graph).expect("alias fixture facts");
    (fixture, facts)
}
fn query(names: &[&str]) -> Query {
    Query {
        title: "Named handoff".into(),
        types: names
            .iter()
            .map(|name| Selector {
                name: (*name).into(),
                file: None,
            })
            .collect(),
        apis: vec![],
        preparation_files: vec!["src/lib.rs".into()],
        preparation_functions: vec![],
    }
}
const SOURCE: &str = "pub struct Form;\npub type Forms = Vec<Form>;\npub fn infer() -> Forms { Vec::new() }\npub fn prepare() -> Forms { infer() }\npub fn build(forms: Forms) -> Form { Form }\n";

#[test]
fn alias_is_selectable_in_public_contracts_producers_preparation_and_comparison() {
    let (fixture, before) = facts(SOURCE);
    let q = query(&["Forms", "Form"]);
    let view = representations::project(&before, &q).unwrap();
    let alias = view.types.iter().find(|t| t.name == "Forms").unwrap();
    assert_eq!(alias.alias.as_deref(), Some("Vec<Form>"));
    assert!(alias.fields.is_empty());
    assert!(view.retention.is_empty());
    let infer = view
        .contracts
        .iter()
        .find(|c| c.function.name == "infer")
        .unwrap();
    assert_eq!(infer.outputs[0].to, alias.id);
    assert_eq!(infer.outputs.len(), 1, "no expansion into Form");
    let producer = view
        .producers
        .iter()
        .find(|p| p.representation.name == "Forms")
        .unwrap();
    assert_eq!(producer.returned.len(), 2);
    let preparation = view
        .preparation
        .iter()
        .find(|p| p.function.name == "prepare")
        .unwrap();
    assert_eq!(preparation.calls[0].1.name, "infer");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_atlas"))
        .args(["representations", "--root"])
        .arg(&fixture.0)
        .args(["--type", "Forms", "--panel", "contracts", "--json"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(representations::ascii(&view, Panel::Contracts).contains("type Forms = Vec<Form>"));
    let (_, after) = facts(&SOURCE.replace("Vec<Form>", "Option<Form>"));
    let comparison =
        comparison::compare(&before, &after, Some(&q), ("before".into(), "after".into())).unwrap();
    assert!(comparison.changes.iter().any(|c| c.category == "type"
        && c.before.as_deref() == Some("type Forms = Vec<Form>")
        && c.after.as_deref() == Some("type Forms = Option<Form>")));
}

#[test]
fn alias_rhs_keeps_written_shape_unknowns_and_cycles_without_expansion() {
    let (_, facts) = facts(
        "pub struct Form;\npub type Pair<'a, T> = (&'a Form, Vec<T>);\npub type Missing = external::Other;\npub type A = B;\npub type B = A;\n",
    );
    let pair = facts.types.iter().find(|t| t.name == "Pair").unwrap();
    assert_eq!(pair.alias.as_deref(), Some("(&'a Form, Vec<T>)"));
    let reference = facts.references.iter().find(|r| r.from == pair.id).unwrap();
    assert_eq!(reference.role, Role::AliasTarget);
    assert!(reference.borrow);
    assert_eq!(reference.wrappers, ["tuple[0]"]);
    assert_eq!(reference.line, 2);
    assert!(!facts.unknown_types.iter().any(|u| u.spelling == "T"));
    assert!(
        facts
            .unknown_types
            .iter()
            .any(|u| u.spelling == "external::Other" && u.role == Role::AliasTarget)
    );
    for (from, to) in [("A", "B"), ("B", "A")] {
        let from = facts.types.iter().find(|t| t.name == from).unwrap();
        let to = facts.types.iter().find(|t| t.name == to).unwrap();
        assert!(
            facts
                .references
                .iter()
                .any(|r| r.from == from.id && r.to == to.id && r.role == Role::AliasTarget)
        );
    }
    assert_eq!(
        facts.references.len(),
        3,
        "each written RHS visited once; cycles stay finite"
    );
}

#[test]
fn preparation_picture_shares_callee_identity_without_merging_callers() {
    let (_, facts) = facts(
        &(SOURCE.to_owned()
            + "pub fn prepare_again()->Forms { infer() }\npub fn wrapped()->Vec<Forms>{vec![]}\n"),
    );
    let view = representations::project(&facts, &query(&["Forms"])).unwrap();
    let artifact = representations::pele(&view, Panel::Preparation).unwrap();
    let source = String::from_utf8(artifact.companions[0].bytes.clone()).unwrap();
    assert_eq!(source.matches("infer()").count(), 1);
    assert!(source.contains("prepare_again()"));
    assert!(source.contains("returns Vec#60;Forms#62;"));
    assert!(source.contains("prepare()"));
    assert_eq!(source.matches("calls @").count(), 2);
    assert!(source.contains("subgraph pf0[\"src/lib.rs\"]"));
}

#[test]
fn preparation_function_selection_is_exact_and_keeps_other_routes_in_facts() {
    let (_, facts) = facts(SOURCE);
    let mut question = query(&["Forms"]);
    question.preparation_files.clear();
    question.preparation_functions.push(Selector {
        name: "prepare".into(),
        file: None,
    });
    let view = representations::project(&facts, &question).unwrap();
    assert_eq!(view.preparation.len(), 1);
    assert_eq!(view.preparation[0].function.name, "prepare");
    assert_eq!(view.preparation[0].calls[0].1.name, "infer");
    assert!(facts.functions.iter().any(|f| f.name == "infer"));
    question.preparation_functions[0].name = "not_present".into();
    assert!(
        representations::project(&facts, &question)
            .unwrap_err()
            .to_string()
            .contains("found 0")
    );
    question.preparation_functions[0].name = "build".into();
    assert!(
        representations::project(&facts, &question)
            .unwrap_err()
            .to_string()
            .contains("return a selected type")
    );
}
