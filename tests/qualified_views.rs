//! Qualified relations and project declarations cannot erase source contradictions.
mod support;
use atlas::{
    adapter, comparison,
    dependencies::{self, Relation},
    discovery, overview,
    representations::{self, Facts, Role},
};
use cargo_metadata::MetadataCommand;
use support::Fixture;
fn scan(f: &Fixture) -> (cargo_metadata::Metadata, atlas::model::SourceGraph, Facts) {
    let m = MetadataCommand::new()
        .current_dir(&f.0)
        .no_deps()
        .exec()
        .expect("test fixture prerequisite must succeed");
    let g = discovery::scan_metadata(&m).expect("test fixture prerequisite must succeed");
    let facts =
        representations::extract(&f.0, &m, &g).expect("test fixture prerequisite must succeed");
    (m, g, facts)
}
#[test]
fn body_call_is_seen_but_same_spelled_variant_and_generic_parameter_are_not_types() {
    let f = Fixture::new(&[
        (
            "src/lib.rs",
            "mod laws;\nmod bindings;\nmod writers;\nmod witness;\n",
        ),
        (
            "src/laws.rs",
            "use crate::bindings::Bindings;\nfn judge(){let _=Bindings::new();}\n",
        ),
        (
            "src/bindings.rs",
            "pub struct Bindings;\nimpl Bindings {pub fn new()->Self {Self}}\n",
        ),
        (
            "src/writers.rs",
            "enum Writer { Instruction(u32) }\nfn out(){let _=Writer::Instruction(1);}\nfn generic<Instruction>(){Instruction::new();}\n",
        ),
        (
            "src/witness.rs",
            "pub struct Instruction;\nimpl Instruction {pub fn new()->Self{Self}}\n",
        ),
    ]);
    let (_, _, facts) = scan(&f);
    let v = dependencies::project(&facts, "src", Relation::All).unwrap();
    assert!(v.witnesses.iter().any(|w| w.from == "src/laws.rs"
        && w.to == "src/bindings.rs"
        && w.relation == Relation::Calls
        && w.line == 2));
    assert!(
        !v.witnesses
            .iter()
            .any(|w| w.from == "src/writers.rs" && w.to == "src/witness.rs")
    );
    assert!(
        facts
            .unknown_calls
            .iter()
            .any(|u| u.call.spelling == "Instruction::new")
    );
    for w in &v.witnesses {
        assert!(f.0.join(&w.file).is_file());
        assert!(
            std::fs::read_to_string(f.0.join(&w.file))
                .unwrap()
                .lines()
                .nth(w.line - 1)
                .is_some()
        );
    }
    v.diagram.validate().unwrap();
}
const DECLARATIONS: &str = "pub mod build;\npub mod run;\npub mod observe;\nenum Layer{Stage,Instrument}\nconst MODULES:[(&str,Option<()>,Layer,&[&str]);3]=[(\"build\",None,Layer::Stage,&[]),(\"run\",None,Layer::Stage,&[]),(\"observe\",None,Layer::Instrument,&[])];\nconst SURFACES:[(&str,&str,&[&str]);2]=[(\"build\",\"build\",&[]),(\"run\",\"Runtime\",&[])];\n";
#[test]
fn creation_calls_and_refusal_contracts_survive_a_success_focused_question() {
    let f = Fixture::new(&[(
        "src/lib.rs",
        r"
#[derive(Default)] pub struct Graph;
pub struct Checked;
pub struct Refused { original: Graph }
pub fn admit(g:Graph)->Result<Checked,Box<Refused>> {Err(Box::new(Refused{original:g}))}
pub fn prepare()->Graph {Graph::default()}
fn shadow<Graph:Default>()->Graph {Graph::default()}
fn deferred() {let _=||Graph::default();}
fn inferred()->Graph {Default::default()}
",
    )]);
    let (_, _, facts) = scan(&f);
    let query = representations::Query {
        title: "Admission".into(),
        types: ["Graph", "Checked"]
            .into_iter()
            .map(|name| representations::Selector {
                name: name.into(),
                file: None,
            })
            .collect(),
        apis: vec![],
        preparation_files: vec![],
    };
    let v = representations::project(&facts, &query).unwrap();
    assert_eq!(
        facts.default_calls.len(),
        1,
        "no generic, inferred or deferred default call is attributed"
    );
    let graph = v
        .producers
        .iter()
        .find(|p| p.representation.name == "Graph")
        .unwrap();
    assert!(graph.constructed.is_empty());
    assert_eq!(graph.default_calls[0].0.name, "prepare");
    let admit = v
        .contracts
        .iter()
        .find(|c| c.function.name == "admit")
        .unwrap();
    assert_eq!(admit.outputs.len(), 1);
    assert_eq!(admit.failures[0].0.name, "Refused");
    assert_eq!(admit.failures[0].1.wrappers, ["Result[1]", "Box[0]"]);
    let svg = String::from_utf8(
        representations::pele(&v, representations::Panel::All)
            .unwrap()
            .primary
            .bytes,
    )
    .unwrap();
    assert!(
        svg.contains("Refused") && svg.contains("Err") && svg.contains("Explicit Type::default()")
    );
    assert!(
        !svg.contains("Result[1]"),
        "human labels, original syntax remains in JSON"
    );
}

#[test]
fn comparison_has_aligned_values_revisions_and_no_invented_flow_edges() {
    let f = Fixture::new(&[(
        "src/lib.rs",
        "pub struct Graph;\npub fn make()->Graph{Graph}\n",
    )]);
    let (_, _, a) = scan(&f);
    f.write(
        "src/lib.rs",
        "pub struct Graph;\npub fn make()->Graph{Graph}\nfn extra()->Graph{Graph::default()}\n",
    );
    let (_, _, b) = scan(&f);
    let c = comparison::compare(
        &a,
        &b,
        None,
        ("baseline-pin".into(), "candidate-pin".into()),
    )
    .unwrap();
    assert!(
        c.changes
            .iter()
            .any(|c| c.subject.contains("explicitly calls"))
    );
    let artifact = c.pele().unwrap();
    let svg = String::from_utf8(artifact.primary.bytes).unwrap();
    assert!(svg.contains("baseline-pin") && svg.contains("candidate-pin"));
    assert!(svg.contains("ADDED") && svg.contains("Before") && svg.contains("After"));
    assert!(
        !String::from_utf8(artifact.companions[0].bytes.clone())
            .unwrap()
            .contains("-->")
    );
}
#[test]
fn source_declarations_choose_apis_but_not_edges_and_instruments_survive() {
    let f = Fixture::new(&[
        ("src/lib.rs", DECLARATIONS),
        (
            "src/build.rs",
            "pub struct Graph { pub n:u32 }\npub fn build()->Graph{Graph {n:0}}\n",
        ),
        (
            "src/run.rs",
            "use crate::build::Graph;\npub struct Runtime;\nimpl crate::run::Runtime {pub fn new(_:Graph)->Self{Self}}\nfn bypass()->Graph{Graph {n:1}}\n",
        ),
        (
            "src/observe.rs",
            "pub fn inspect(){let _=crate::build::build();}\n",
        ),
    ]);
    let (_, g, facts) = scan(&f);
    let input = adapter::Request::new(
        adapter::Snapshot::from_facts(&g, &facts, "src/lib.rs").unwrap(),
        "src/lib.rs".into(),
    )
    .unwrap();
    let reply = adapter::invoke(
        std::path::Path::new(env!("CARGO_BIN_EXE_atlas-girard")),
        &f.0,
        &input,
    )
    .unwrap();
    let a = reply
        .reply
        .architecture()
        .unwrap()
        .clone()
        .bind(&f.0)
        .unwrap();
    let v = overview::project(&facts, &a).unwrap();
    assert!(v.views.types.iter().any(|t| t.name == "Graph"));
    assert!(v.views.types.iter().any(|t| t.name == "Runtime"));
    assert!(
        v.views
            .contracts
            .iter()
            .any(|c| c.function.name == "bypass" && c.additional_constructor)
    );
    assert_eq!(v.instruments.len(), 1);
    assert!(
        v.instruments[0]
            .calls
            .iter()
            .any(|c| c.to.contains("build"))
    );
    let deps = dependencies::project(&facts, "src", Relation::Calls).unwrap();
    assert!(
        deps.witnesses
            .iter()
            .any(|w| w.from == "src/observe.rs" && w.to == "src/build.rs")
    );
    assert!(a.modules.iter().all(|m| m.dependencies.is_empty()));
}
#[test]
#[expect(
    clippy::many_single_char_names,
    reason = "short fixture and graph-vertex bindings mirror the source relationships under test"
)]
fn comparison_ignores_line_shifts_and_exposes_new_construction_and_preparation() {
    let base = "pub struct Graph {n:u32}\npub fn make()->Graph {Graph {n:0}}\npub fn prepare()->Graph{make()}\n";
    let f = Fixture::new(&[("src/lib.rs", base)]);
    let (_, _, a) = scan(&f);
    f.write("src/lib.rs", &format!("\n\n{base}"));
    let (_, _, same) = scan(&f);
    assert!(
        comparison::compare(&a, &same, None, ("a".into(), "b".into()))
            .unwrap()
            .changes
            .is_empty()
    );
    f.write(
        "src/lib.rs",
        &(base.to_owned()
            + "pub fn prepare_again()->Graph{make()}\nfn bypass()->Graph{Graph {n:2}}\n"),
    );
    let (_, _, b) = scan(&f);
    let c = comparison::compare(&a, &b, None, ("a".into(), "b".into())).unwrap();
    assert!(
        c.changes
            .iter()
            .any(|c| c.category == "constructor" && c.subject.contains("bypass"))
    );
    assert!(
        c.changes
            .iter()
            .any(|c| c.category == "preparation" && c.subject.contains("prepare_again"))
    );
    assert!(
        c.changes
            .iter()
            .any(|c| c.category == "contract" && c.subject.contains("prepare_again"))
    );
    let query = atlas::representations::Query {
        title: "Graph".into(),
        types: vec![atlas::representations::Selector {
            name: "Graph".into(),
            file: None,
        }],
        apis: vec![],
        preparation_files: vec![],
    };
    let v = representations::project(&b, &query).unwrap();
    let p = &v.producers[0];
    assert_eq!(p.returned.len(), 4);
    assert_eq!(p.constructed.len(), 2);
    assert!(
        b.references
            .iter()
            .filter(|r| r.role == Role::Output)
            .all(|r| r.line > 0)
    );
}
#[test]
fn comparison_reads_exact_git_source_without_switching_working_tree() {
    use std::process::Command;
    let f = Fixture::new(&[("src/lib.rs", "pub struct Before;\n")]);
    let git = |args: &[&str]| {
        let o = Command::new("git")
            .current_dir(&f.0)
            .args(args)
            .output()
            .unwrap();
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        String::from_utf8(o.stdout).unwrap().trim().to_owned()
    };
    git(&["init", "-q"]);
    git(&["add", "Cargo.toml", "src/lib.rs"]);
    git(&[
        "-c",
        "user.name=Atlas test",
        "-c",
        "user.email=atlas@example.invalid",
        "commit",
        "-qm",
        "before",
    ]);
    let id = git(&["rev-parse", "HEAD"]);
    f.write("src/lib.rs", "pub struct After;\n");
    let snapshot = atlas::revision::Revision::read(&f.0, &id).unwrap();
    assert_eq!(
        std::fs::read_to_string(snapshot.root.join("src/lib.rs")).unwrap(),
        "pub struct Before;\n"
    );
    assert_eq!(
        std::fs::read_to_string(f.0.join("src/lib.rs")).unwrap(),
        "pub struct After;\n"
    );
}

#[test]
fn comparison_keeps_distinct_tuple_positions_and_module_symbols() {
    let base = "pub struct Graph;\npub fn pair()->(Graph,Graph){(Graph,Graph)}\nmod a {pub struct Local;}\nmod b {pub struct Local;}\n";
    let f = Fixture::new(&[("src/lib.rs", base)]);
    let (_, _, a) = scan(&f);
    f.write("src/lib.rs", &base.replace("(Graph,Graph)", "(Graph,)"));
    let (_, _, b) = scan(&f);
    let c = comparison::compare(&a, &b, None, ("a".into(), "b".into())).unwrap();
    assert!(
        c.changes
            .iter()
            .any(|c| c.category == "contract" && c.subject.contains("tuple[1]"))
    );
}

#[test]
fn error_type_returners_keep_their_err_position_in_the_producer_view() {
    let f = Fixture::new(&[(
        "src/lib.rs",
        "pub struct Refused; pub fn reject()->Result<(),Refused>{Err(Refused)}",
    )]);
    let (_, _, facts) = scan(&f);
    let query = representations::Query {
        title: "Error routes".into(),
        types: vec![representations::Selector {
            name: "Refused".into(),
            file: None,
        }],
        apis: vec![],
        preparation_files: vec![],
    };
    let v = representations::project(&facts, &query).unwrap();
    let artifact = representations::pele(&v, representations::Panel::Producers).unwrap();
    let svg = String::from_utf8(artifact.primary.bytes).unwrap();
    assert!(svg.contains("return · Err") && svg.contains("reject"));
}
