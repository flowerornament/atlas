//! A source change must survive selection; syntactic uncertainty must remain visible.
mod support;
use atlas::{
    discovery,
    representations::{self, Panel, Query, Selector},
};
use cargo_metadata::MetadataCommand;
use support::Fixture;
fn query() -> Query {
    Query {
        title: "Representation test".into(),
        types: ["Graph", "Checked", "Engine"]
            .into_iter()
            .map(|name| Selector {
                name: name.into(),
                file: None,
            })
            .collect(),
        apis: vec![Selector {
            name: "check".into(),
            file: None,
        }],
        preparation_files: vec!["src/lib.rs".into()],
        preparation_functions: vec![],
    }
}
fn inspect(source: &str) -> (Fixture, representations::Facts, representations::Views) {
    let f = Fixture::new(&[("src/lib.rs", source)]);
    let m = MetadataCommand::new()
        .current_dir(f.path())
        .no_deps()
        .exec()
        .expect("test fixture prerequisite must succeed");
    let g = discovery::scan_metadata(&m).expect("test fixture prerequisite must succeed");
    let facts =
        representations::extract(f.path(), &m, &g).expect("test fixture prerequisite must succeed");
    let views =
        representations::project(&facts, &query()).expect("test fixture prerequisite must succeed");
    (f, facts, views)
}
const BASE: &str = "
pub struct Graph { pub value: u32 }
pub struct Checked { graph: Graph }
pub struct Engine<'a> { checked: &'a Checked }
pub fn check(graph: Graph) -> Checked { Checked { graph } }
pub fn prepare(graph: Graph) -> Checked { check(graph) }
fn generic<Graph>(g: Graph) -> Graph { g }
";
#[test]
fn changed_source_exposes_retention_bypass_and_duplicated_preparation() {
    let (_f, _, base) = inspect(BASE);
    let changed = BASE.replace(
        "checked: &'a Checked }",
        "checked: &'a Checked, accidental: Graph }",
    ) + "\nfn unchecked_copy(graph: Graph)->Checked { Checked { graph } }\npub fn prepare_again(graph:Graph)->Checked {check(graph)}";
    let (changed_fixture, _, v) = inspect(&changed);
    let compile = std::process::Command::new("cargo")
        .args(["check", "--offline", "--quiet"])
        .current_dir(changed_fixture.path())
        .output()
        .unwrap();
    assert!(
        compile.status.success(),
        "{}",
        String::from_utf8_lossy(&compile.stderr)
    );
    assert_eq!(v.retention.len(), base.retention.len() + 1);
    assert!(
        v.contracts
            .iter()
            .any(|c| c.function.name == "unchecked_copy" && c.additional_constructor)
    );
    let rows: Vec<_> = v
        .preparation
        .iter()
        .filter(|r| r.function.name.starts_with("prepare"))
        .collect();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].calls[0].1.name, rows[1].calls[0].1.name);
    assert!(!v.contracts.iter().any(|c| c.function.name == "generic"));
    assert!(!v.preparation.iter().any(|c| c.function.name == "generic"));
    assert_eq!(
        v.types.iter().map(|t| &t.name).collect::<Vec<_>>(),
        base.types.iter().map(|t| &t.name).collect::<Vec<_>>()
    );
}
#[test]
fn field_expressions_keep_containers_tuples_and_borrows_separate() {
    let source = BASE.replace(
        "checked: &'a Checked }",
        "checked: &'a Checked, graph: std::sync::Arc<Graph>, pair: (Graph, Vec<&'a Checked>) }",
    );
    let (_f, _, v) = inspect(&source);
    let arc = v
        .retention
        .iter()
        .find(|r| r.slot == "graph" && r.expression.contains("Arc"))
        .unwrap();
    assert_eq!(arc.wrappers, ["Arc[0]"]);
    assert!(!arc.borrow);
    assert!(arc.expression.contains("std::sync::Arc<Graph>"));
    let nested = v
        .retention
        .iter()
        .find(|r| r.slot == "pair" && r.borrow)
        .unwrap();
    assert_eq!(nested.wrappers, ["tuple[1]", "Vec[0]"]);
}
#[test]
fn initializer_without_selected_return_is_not_hidden_and_deferred_is_not_attributed() {
    let source = BASE.to_owned()
        + "\nfn hidden() -> usize { let _ = Graph { value: 1 }; 0 }\nfn deferred() { let _ = || Graph {value:2}; }";
    let (_f, _, v) = inspect(&source);
    let hidden = v
        .contracts
        .iter()
        .find(|c| c.function.name == "hidden")
        .unwrap();
    assert!(hidden.outputs.is_empty());
    assert_eq!(hidden.literals.len(), 1);
    assert!(!v.contracts.iter().any(|c| c.function.name == "deferred"));
}
#[test]
#[expect(
    clippy::many_single_char_names,
    reason = "short fixture and graph-vertex bindings mirror the source relationships under test"
)]
fn type_only_files_and_cross_file_impl_owner_use_the_shared_index() {
    let f = Fixture::new(&[
        (
            "src/lib.rs",
            "pub mod data;\nmod make;\npub fn prepare(g:data::Graph)->data::Checked {data::Checked::new(g)}",
        ),
        (
            "src/data.rs",
            "pub struct Graph { pub value: u32 } pub struct Checked { pub graph: Graph }",
        ),
        (
            "src/make.rs",
            "use crate::data::{Checked,Graph}; impl Checked { pub fn new(graph:Graph)->Self { Self {graph} } }",
        ),
    ]);
    let m = MetadataCommand::new()
        .current_dir(f.path())
        .no_deps()
        .exec()
        .unwrap();
    let g = discovery::scan_metadata(&m).unwrap();
    let facts = representations::extract(f.path(), &m, &g).unwrap();
    let q = Query {
        title: "cross file".into(),
        types: ["Graph", "Checked"]
            .into_iter()
            .map(|s| Selector {
                name: s.into(),
                file: None,
            })
            .collect(),
        apis: vec![],
        preparation_files: vec!["src/lib.rs".into()],
        preparation_functions: vec![],
    };
    let v = representations::project(&facts, &q).unwrap();
    assert_eq!(v.retention.len(), 1);
    assert_eq!(v.preparation[0].calls[0].1.name, "Checked::new");
}
#[test]
fn selection_does_not_change_extraction_and_ambiguity_refuses() {
    let f = Fixture::new(&[(
        "src/lib.rs",
        &(BASE.to_owned() + "\nmod other {pub struct Graph;}"),
    )]);
    let m = MetadataCommand::new()
        .current_dir(f.path())
        .no_deps()
        .exec()
        .unwrap();
    let g = discovery::scan_metadata(&m).unwrap();
    let facts = representations::extract(f.path(), &m, &g).unwrap();
    assert_eq!(facts.types.iter().filter(|t| t.name == "Graph").count(), 2);
    assert!(representations::project(&facts, &query()).is_err());
}
#[test]
#[expect(
    clippy::many_single_char_names,
    reason = "short fixture and graph-vertex bindings mirror the source relationships under test"
)]
fn unknowns_and_no_preparation_remain_explicit_in_both_backends() {
    let mut q = query();
    q.apis.push(Selector {
        name: "opaque".into(),
        file: None,
    });
    let f = Fixture::new(&[(
        "src/lib.rs",
        &(BASE.to_owned() + "\nfn opaque()->Graph { external::make() }"),
    )]);
    let m = MetadataCommand::new()
        .current_dir(f.path())
        .no_deps()
        .exec()
        .unwrap();
    let g = discovery::scan_metadata(&m).unwrap();
    let facts = representations::extract(f.path(), &m, &g).unwrap();
    let v = representations::project(&facts, &q).unwrap();
    assert!(
        v.unknown_calls
            .iter()
            .any(|u| u.call.spelling.contains("external"))
    );
    let ascii = representations::ascii(&v, Panel::All);
    assert!(ascii.contains("Unknown observations"));
    let artifact = representations::pele(&v, Panel::All).unwrap();
    let svg = String::from_utf8(artifact.primary.bytes).unwrap();
    assert!(svg.contains("viewBox"));
    assert!(svg.contains("Graph"));
}

#[test]
fn expanding_reexport_cycles_terminate_as_unknown() {
    let source = "pub use inner::Graph;\nmod inner { pub use super::Graph; }\npub fn make(g: Graph)->Graph {g}";
    let f = Fixture::new(&[("src/lib.rs", source)]);
    let m = MetadataCommand::new()
        .current_dir(f.path())
        .no_deps()
        .exec()
        .unwrap();
    let g = discovery::scan_metadata(&m).unwrap();
    let facts = representations::extract(f.path(), &m, &g).unwrap();
    assert!(facts.references.is_empty());
    assert!(!facts.unknown_types.is_empty());
}

#[test]
fn matrix_renders_nested_borrows_and_contract_titles_keep_all_literal_sites() {
    let source = BASE.replace(
        "checked: &'a Checked }",
        "checked: &'a Checked, graphs: Vec<&'a Graph> }",
    ) + "\nfn extra()->Graph { let _=Graph {value:1}; Graph {value:2} }";
    let (_f, _, v) = inspect(&source);
    let artifact = representations::pele(&v, Panel::All).unwrap();
    let matrix = representations::pele(&v, Panel::Retention).unwrap();
    let native = String::from_utf8(matrix.companions[0].bytes.clone()).unwrap();
    assert!(native.contains("#38;graphs*"));
    let svg = String::from_utf8(artifact.primary.bytes).unwrap();
    // The type-level return arrow has one relation, with both construction sites in its title.
    assert!(svg.contains("literal Graph at line"));
    assert!(svg.contains("fn extra()-&gt;Graph"));
}

#[test]
#[expect(
    clippy::many_single_char_names,
    reason = "short fixture and graph-vertex bindings mirror the source relationships under test"
)]
fn default_api_selection_includes_trait_implementation_methods() {
    let f = Fixture::new(&[(
        "src/lib.rs",
        "pub struct Graph {value:u32}\npub struct Builder;\ntrait Build { fn build()->Graph; }\nimpl Build for Builder {fn build()->Graph {Graph {value:1}}}",
    )]);
    let m = MetadataCommand::new()
        .current_dir(f.path())
        .no_deps()
        .exec()
        .unwrap();
    let g = discovery::scan_metadata(&m).unwrap();
    let facts = representations::extract(f.path(), &m, &g).unwrap();
    let q = Query {
        title: "trait impl".into(),
        types: vec![Selector {
            name: "Graph".into(),
            file: None,
        }],
        apis: vec![],
        preparation_files: vec![],
        preparation_functions: vec![],
    };
    let v = representations::project(&facts, &q).unwrap();
    let c = v
        .contracts
        .iter()
        .find(|c| c.function.name == "Builder::build")
        .unwrap();
    assert!(c.function.trait_impl);
    assert!(!c.additional_constructor);
}

#[test]
fn changed_source_cannot_be_reported_under_the_old_fingerprint() {
    let f = Fixture::new(&[("src/lib.rs", BASE)]);
    let m = MetadataCommand::new()
        .current_dir(f.path())
        .no_deps()
        .exec()
        .unwrap();
    let g = discovery::scan_metadata(&m).unwrap();
    f.write("src/lib.rs", &(BASE.to_owned() + "\npub fn new_route() {}"));
    assert!(
        representations::extract(f.path(), &m, &g)
            .unwrap_err()
            .to_string()
            .contains("source changed after discovery")
    );
}
