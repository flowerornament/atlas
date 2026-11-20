//! Qualified relations and project declarations cannot erase source contradictions.
mod support;
use cargo_atlas::{
    adapter, comparison,
    dependencies::{self, Relation},
    discovery, overview,
    types::{self, Facts, Role},
};
use cargo_metadata::MetadataCommand;
use support::Fixture;
fn scan(
    f: &Fixture,
) -> (
    cargo_metadata::Metadata,
    cargo_atlas::model::SourceGraph,
    Facts,
) {
    let m = MetadataCommand::new()
        .current_dir(f.path())
        .no_deps()
        .exec()
        .expect("test fixture prerequisite must succeed");
    let g = discovery::scan_metadata(&m).expect("test fixture prerequisite must succeed");
    let facts = types::extract(f.path(), &m, &g).expect("test fixture prerequisite must succeed");
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
        assert!(f.path().join(&w.file).is_file());
        assert!(
            std::fs::read_to_string(f.path().join(&w.file))
                .unwrap()
                .lines()
                .nth(w.line - 1)
                .is_some()
        );
    }
    v.diagram.validate().unwrap();
}
const DECLARATIONS: &str = "pub mod build;\npub mod run;\npub mod observe;\nenum Layer{Stage,Instrument}\nconst LAYERS:[(&str,&str,Layer);3]=[(\"build\",\"build\",Layer::Stage),(\"run\",\"Runtime\",Layer::Stage),(\"observe\",\"\",Layer::Instrument)];\n";
/// Reads `LAYERS` rows of (module, entry, layer); a test-local declaration convention.
struct Layers;
impl adapter::ProjectAdapter for Layers {
    fn name(&self) -> &'static str {
        "test-layers"
    }
    fn version(&self) -> &'static str {
        "0"
    }
    fn declarations(
        &self,
        input: &adapter::Request,
    ) -> cargo_atlas::Result<cargo_atlas::architecture::Architecture> {
        use cargo_atlas::architecture::{Architecture, Kind, Module};
        let layers = input.snapshot.constant(&input.source, "LAYERS")?;
        let mut modules = Vec::new();
        for row in layers.value.array().ok_or("LAYERS must be an array")? {
            let [name, entry, layer] = row.tuple().ok_or("row must be a tuple")? else {
                return Err("row expects module, entry, layer".into());
            };
            let name = name.string().ok_or("module must be a string")?;
            let kind = match layer.path().and_then(|p| p.last()).map(String::as_str) {
                Some("Stage") => Kind::Stage,
                Some("Instrument") => Kind::Instrument,
                _ => return Err("unsupported layer".into()),
            };
            modules.push(Module {
                name: name.into(),
                kind,
                label: None,
                path: input.snapshot.module_root(name)?,
                entry: entry.string().ok_or("entry must be a string")?.into(),
                types: vec![],
                dependencies: vec![],
                directory: false,
            });
        }
        Ok(Architecture {
            schema: 1,
            provenance: vec![layers.id()],
            modules,
            workflows: vec![],
        })
    }
}
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
    let query = types::Query {
        title: "Admission".into(),
        types: ["Graph", "Checked"]
            .into_iter()
            .map(|name| types::Selector {
                name: name.into(),
                file: None,
            })
            .collect(),
        apis: vec![],
        preparation_files: vec![],
        preparation_functions: vec![],
    };
    let v = types::project(&facts, &query).unwrap();
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
        types::pele(
            &v,
            types::Panel::All,
            &cargo_atlas::render::Pele {
                html: false,
                theme: cargo_atlas::render::Theme::Light,
                bare: false,
            },
        )
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
    let artifact = c
        .pele(&cargo_atlas::render::Pele {
            html: false,
            theme: cargo_atlas::render::Theme::Light,
            bare: false,
        })
        .unwrap();
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
    let reply = adapter::adapt(&Layers, &input);
    adapter::validate_reply(&input, &reply).unwrap();
    let a = reply
        .architecture()
        .unwrap()
        .clone()
        .bind(f.path())
        .unwrap();
    let v = overview::project(&facts, &a).unwrap();
    assert_eq!(v.panels.len(), 1);
    let views = &v.panels[0].views;
    assert!(views.types.iter().any(|t| t.name == "Graph"));
    assert!(views.types.iter().any(|t| t.name == "Runtime"));
    assert!(
        views
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
/// A workspace of stages `s0`, `s1`, ... where stage `i` declares `sizes[i]`
/// types and its entry `run` returns all of them. Each later stage's entry also
/// takes the previous stage's first type, a cross-stage form. Each stage also
/// has `extra` private functions that build its first type literally; each is
/// one more API contract row.
fn staged(
    sizes: &[usize],
    extra: usize,
) -> (Fixture, Facts, cargo_atlas::architecture::Architecture) {
    use std::fmt::Write;
    let rows: Vec<_> = (0..sizes.len())
        .map(|i| format!("(\"s{i}\",\"run\",Layer::Stage)"))
        .collect();
    let mut lib = String::new();
    for i in 0..sizes.len() {
        writeln!(lib, "pub mod s{i};").expect("formatting into a String cannot fail");
    }
    writeln!(
        lib,
        "enum Layer{{Stage,Instrument}}\nconst LAYERS:[(&str,&str,Layer);{}]=[{}];",
        sizes.len(),
        rows.join(",")
    )
    .expect("formatting into a String cannot fail");
    let f = Fixture::new(&[("src/lib.rs", lib.as_str())]);
    for (i, &size) in sizes.iter().enumerate() {
        let names: Vec<_> = (0..size).map(|t| format!("S{i}T{t}")).collect();
        let mut text = String::new();
        for name in &names {
            writeln!(text, "pub struct {name} {{}}").expect("formatting into a String cannot fail");
        }
        let input = if i == 0 {
            String::new()
        } else {
            format!("_: crate::s{}::S{}T0", i - 1, i - 1)
        };
        for k in 0..extra {
            writeln!(text, "fn build{k}() {{ let _ = S{i}T0 {{}}; }}")
                .expect("formatting into a String cannot fail");
        }
        let values = names
            .iter()
            .map(|n| format!("{n} {{}}"))
            .collect::<Vec<_>>()
            .join(",");
        let names = names.join(",");
        writeln!(text, "pub fn run({input})->({names},){{({values},)}}")
            .expect("formatting into a String cannot fail");
        f.write(&format!("src/s{i}.rs"), &text);
    }
    let (_, g, facts) = scan(&f);
    let input = adapter::Request::new(
        adapter::Snapshot::from_facts(&g, &facts, "src/lib.rs")
            .expect("test fixture prerequisite must succeed"),
        "src/lib.rs".into(),
    )
    .expect("test fixture prerequisite must succeed");
    let reply = adapter::adapt(&Layers, &input);
    let architecture = reply
        .architecture()
        .expect("test fixture prerequisite must succeed")
        .clone()
        .bind(f.path())
        .expect("test fixture prerequisite must succeed");
    (f, facts, architecture)
}

#[test]
fn an_overview_above_one_figure_is_split_into_module_panels_without_dropping_types() {
    let (_f, facts, architecture) = staged(&[5, 5, 5], 0);
    let v = overview::project(&facts, &architecture).unwrap();
    // Whole modules, packed in declared order into panels of at most 12.
    let modules: Vec<_> = v.panels.iter().map(|p| p.modules.clone()).collect();
    assert_eq!(modules, [vec!["s0", "s1"], vec!["s2"]]);
    let split: Vec<_> = v
        .split
        .iter()
        .map(|g| (g.module.as_deref().unwrap(), g.types, g.panel))
        .collect();
    assert_eq!(split, [("s0", 5, 0), ("s1", 5, 0), ("s2", 5, 1)]);
    // Every selected type appears exactly once.
    let mut drawn: Vec<_> = v
        .panels
        .iter()
        .flat_map(|p| p.views.types.iter().map(|t| t.name.clone()))
        .collect();
    drawn.sort();
    let mut expected: Vec<_> = (0..3)
        .flat_map(|i| (0..5).map(move |t| format!("S{i}T{t}")))
        .collect();
    expected.sort();
    assert_eq!(drawn, expected);
    // s2's entry takes S1T0, so it is also drawn in the panel holding s1.
    let first = &v.panels[0].views;
    let s1t0 = first.types.iter().find(|t| t.name == "S1T0").unwrap();
    assert!(
        first
            .contracts
            .iter()
            .any(|c| c.function.file == "src/s2.rs" && c.inputs.iter().any(|r| r.to == s1t0.id))
    );
    let json = serde_json::to_value(&v).unwrap();
    assert_eq!(json["split"].as_array().unwrap().len(), 3);
    let text = overview::ascii(&v);
    assert!(text.contains("PANEL 1 OF 2: s0, s1") && text.contains("PANEL 2 OF 2: s2"));
    assert!(expected.iter().all(|name| text.contains(name.as_str())));
    let svg = overview::pele(
        &v,
        &cargo_atlas::render::Pele {
            html: false,
            theme: cargo_atlas::render::Theme::Light,
            bare: false,
        },
    )
    .unwrap();
    let svg = String::from_utf8(svg.primary.bytes).unwrap();
    assert!(svg.contains("panel 1 of 2: s0, s1") && svg.contains("panel 2 of 2: s2"));
    assert!(expected.iter().all(|name| svg.contains(name.as_str())));
}

#[test]
fn one_module_above_a_figure_refuses_by_name_and_user_queries_keep_their_cap() {
    let (_f, facts, architecture) = staged(&[3, 13], 0);
    let error = overview::project(&facts, &architecture)
        .err()
        .unwrap()
        .to_string();
    assert!(
        error.contains("module `s1` alone selects 13 types"),
        "{error}"
    );
    let query = types::Query {
        title: "thirteen".into(),
        types: (0..13)
            .map(|t| types::Selector {
                name: format!("S1T{t}"),
                file: Some("src/s1.rs".into()),
            })
            .collect(),
        apis: vec![],
        preparation_files: vec![],
        preparation_functions: vec![],
    };
    let error = types::project(&facts, &query).err().unwrap().to_string();
    assert!(error.contains("select 1–12 types per figure"), "{error}");
}

#[test]
fn overview_panels_also_split_on_api_contracts_and_keep_every_contract() {
    // Six types fit one figure, but each stage brings 16 contract rows.
    let (_f, facts, architecture) = staged(&[2, 2, 2], 15);
    let v = overview::project(&facts, &architecture).unwrap();
    let modules: Vec<_> = v.panels.iter().map(|p| p.modules.clone()).collect();
    assert_eq!(modules, [vec!["s0", "s1"], vec!["s2"]]);
    assert!(v.panels.iter().all(|p| p.views.contracts.len() <= 40));
    // Every builder and every entry is drawn in some panel.
    let drawn: std::collections::BTreeSet<_> = v
        .panels
        .iter()
        .flat_map(|p| p.views.contracts.iter())
        .map(|c| (c.function.file.clone(), c.function.name.clone()))
        .collect();
    for i in 0..3 {
        let file = format!("src/s{i}.rs");
        assert!(drawn.contains(&(file.clone(), "run".to_owned())));
        for k in 0..15 {
            assert!(
                drawn.contains(&(file.clone(), format!("build{k}"))),
                "s{i} build{k}"
            );
        }
    }
}

#[test]
fn one_module_above_forty_contracts_refuses_by_name_and_user_queries_keep_the_row_limit() {
    let (_f, facts, architecture) = staged(&[1, 2], 41);
    let error = overview::project(&facts, &architecture)
        .err()
        .unwrap()
        .to_string();
    assert!(
        error.contains("module `s0` alone selects 1 types and 4"),
        "{error}"
    );
    assert!(error.contains("API contracts"), "{error}");
    let query = types::Query {
        title: "one".into(),
        types: vec![types::Selector {
            name: "S0T0".into(),
            file: Some("src/s0.rs".into()),
        }],
        apis: vec![],
        preparation_files: vec![],
        preparation_functions: vec![],
    };
    let error = types::project(&facts, &query).err().unwrap().to_string();
    assert!(error.contains("view exceeds 40 API contracts"), "{error}");
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
    let query = cargo_atlas::types::Query {
        title: "Graph".into(),
        types: vec![cargo_atlas::types::Selector {
            name: "Graph".into(),
            file: None,
        }],
        apis: vec![],
        preparation_files: vec![],
        preparation_functions: vec![],
    };
    let v = types::project(&b, &query).unwrap();
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
            .current_dir(f.path())
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
    let snapshot = cargo_atlas::revision::Revision::read(f.path(), &id).unwrap();
    assert_eq!(
        std::fs::read_to_string(snapshot.root.join("src/lib.rs")).unwrap(),
        "pub struct Before;\n"
    );
    assert_eq!(
        std::fs::read_to_string(f.path().join("src/lib.rs")).unwrap(),
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
    let query = types::Query {
        title: "Error routes".into(),
        types: vec![types::Selector {
            name: "Refused".into(),
            file: None,
        }],
        apis: vec![],
        preparation_files: vec![],
        preparation_functions: vec![],
    };
    let v = types::project(&facts, &query).unwrap();
    let artifact = types::pele(
        &v,
        types::Panel::Producers,
        &cargo_atlas::render::Pele {
            html: false,
            theme: cargo_atlas::render::Theme::Light,
            bare: false,
        },
    )
    .unwrap();
    let svg = String::from_utf8(artifact.primary.bytes).unwrap();
    assert!(svg.contains("return · Err") && svg.contains("reject"));
}
