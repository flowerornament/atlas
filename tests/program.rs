//! Program orientation preserves actual call sites; it does not invent stage order.
mod support;
use atlas::{architecture::Architecture, discovery, program};
use cargo_metadata::MetadataCommand;
use support::Fixture;
fn profile(f: &Fixture, modules: serde_json::Value) -> Architecture {
    let mut declaration = serde_json::json!({"schema":1,"source":[],"workflows":[]});
    declaration["modules"] = modules;
    Architecture::parse(&declaration.to_string(), &f.0)
        .expect("test fixture prerequisite must succeed")
}
fn stage(name: &str, path: &str, entry: &str) -> serde_json::Value {
    serde_json::json!({"name":name,"kind":"stage","path":path,"entry":entry,"types":[],"dependencies":[]})
}
fn build(f: &Fixture, profile: &Architecture) -> atlas::Result<program::Program> {
    let metadata = MetadataCommand::new().current_dir(&f.0).no_deps().exec()?;
    let graph = discovery::scan_metadata(&metadata)?;
    program::build(&f.0, &metadata, &graph, profile, None)
}
#[test]
fn calls_and_contracted_wrappers_retain_order_arguments_and_branches() {
    let f = Fixture::new(&[
        ("src/lib.rs", "pub mod a;\npub mod b;\n"),
        ("src/a.rs", "pub fn run(x: u32) {}\n"),
        ("src/b.rs", "pub fn run() {}\n"),
        (
            "src/main.rs",
            "use fixture::a as renamed;\nfn main(){ if gate { wrapper(7); } else { fixture::b::run(); } }\nfn wrapper(x:u32){renamed::run(x);}\n",
        ),
    ]);
    let p = build(
        &f,
        &profile(
            &f,
            serde_json::json!([stage("a", "src/a.rs", "run"), stage("b", "src/b.rs", "run")]),
        ),
    )
    .unwrap();
    let overview = p.overview();
    p.diagram(&overview).unwrap().validate().unwrap();
    assert!(
        overview
            .collapsed
            .keys()
            .any(|id| id.contains("::wrapper "))
    );
    let edge = overview
        .connections
        .iter()
        .find(|edge| edge.to == "a")
        .unwrap();
    assert_eq!(edge.chains[0].len(), 2);
    assert_eq!(edge.chains[0][0].call.arguments, ["7"]);
    assert_eq!(edge.chains[0][1].call.arguments, ["x"]);
    assert!(
        edge.chains[0][0]
            .context
            .iter()
            .any(|c| c.contains("if gate: then"))
    );
    assert!(
        !overview
            .connections
            .iter()
            .any(|e| e.from == "a" && e.to == "b")
    );
}
#[test]
fn public_reexports_and_source_established_receiver_types_are_followed() {
    let f = Fixture::new(&[
        ("src/lib.rs", "pub mod engine;\n"),
        ("src/engine/mod.rs", "mod body;\npub use body::Runner;\n"),
        (
            "src/engine/body.rs",
            "pub struct Runner;\nimpl Runner {\n pub fn new()->Self {Self}\n pub fn run(&mut self){}\n}\n",
        ),
        (
            "src/main.rs",
            "use fixture::engine::Runner;\nfn main(){let mut r=Runner::new(); r.run();}\n",
        ),
    ]);
    let p = build(
        &f,
        &profile(
            &f,
            serde_json::json!([stage("engine", "src/engine", "Runner")]),
        ),
    )
    .unwrap();
    assert!(p.nodes.iter().any(|n| n.id == "engine::new"));
    assert!(p.nodes.iter().any(|n| n.id == "engine::run"));
    assert!(!p.observations.iter().any(|o| {
        o.call
            .as_ref()
            .is_some_and(|c| c.receiver.as_deref() == Some("r"))
    }));
}
#[test]
fn a_path_mount_establishes_names_but_a_disconnected_file_does_not() {
    let f = Fixture::new(&[
        (
            "src/lib.rs",
            "#[path=\"unusual/body.rs\"]\npub mod stage;\n",
        ),
        ("src/unusual/body.rs", "pub fn run(){}\n"),
        ("src/ghost.rs", "pub fn run(){}\n"),
        (
            "src/main.rs",
            "fn main(){fixture::stage::run(); fixture::ghost::run();}\n",
        ),
    ]);
    let p = build(
        &f,
        &profile(
            &f,
            serde_json::json!([stage("stage", "src/unusual/body.rs", "run")]),
        ),
    )
    .unwrap();
    assert!(p.nodes.iter().any(|n| n.id == "stage"));
    assert!(p.observations.iter().any(|o| o.reason.contains("ghost")));
    assert!(p.outside_view_files.contains(&"src/ghost.rs".into()));
    assert!(
        build(
            &f,
            &profile(
                &f,
                serde_json::json!([stage("ghost", "src/ghost.rs", "run")])
            )
        )
        .is_err()
    );
}
#[test]
fn macro_deferred_and_untyped_receiver_calls_stay_explicitly_unresolved() {
    let f = Fixture::new(&[
        ("src/lib.rs", "pub mod stage;\n"),
        ("src/stage.rs", "pub fn run(){}\n"),
        (
            "src/main.rs",
            "fn main(){let later=||fixture::stage::run(); println!(\"{}\",fixture::stage::run()); unknown.run();}\n",
        ),
    ]);
    let p = build(
        &f,
        &profile(
            &f,
            serde_json::json!([stage("stage", "src/stage.rs", "run")]),
        ),
    )
    .unwrap();
    assert!(!p.connections.iter().any(|e| e.to == "stage"));
    assert!(p.nodes.iter().any(|n| n.id == "stage" && n.context));
    assert!(p.observations.iter().any(|o| o.reason.contains("opaque")));
    assert!(
        p.observations
            .iter()
            .any(|o| o.reason.contains("receiver unknown"))
    );
}
#[test]
fn recursion_is_a_finite_graph_instead_of_an_expansion() {
    let f = Fixture::new(&[
        ("src/lib.rs", "pub mod stage;\n"),
        ("src/stage.rs", "pub fn run(){}\n"),
        (
            "src/main.rs",
            "fn main(){a();}\nfn a(){b();}\nfn b(){a();}\n",
        ),
    ]);
    let p = build(
        &f,
        &profile(
            &f,
            serde_json::json!([stage("stage", "src/stage.rs", "run")]),
        ),
    )
    .unwrap();
    assert_eq!(p.connections.len(), 3);
    let overview = p.overview();
    assert!(overview.connections.iter().any(|e| e.from == e.to));
    p.diagram(&overview).unwrap().validate().unwrap();
}
#[test]
fn a_repeated_module_mount_does_not_silently_choose_a_boundary_owner() {
    let f = Fixture::new(&[
        (
            "src/lib.rs",
            "#[path=\"stage.rs\"]\npub mod first;\n#[path=\"stage.rs\"]\npub mod second;\n",
        ),
        ("src/stage.rs", "pub fn run(){}\n"),
        ("src/main.rs", "fn main(){fixture::first::run();}\n"),
    ]);
    let result = build(
        &f,
        &profile(
            &f,
            serde_json::json!([stage("stage", "src/stage.rs", "run")]),
        ),
    );
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("2 Cargo module mounts")
    );
}
#[test]
fn shadowed_receiver_bindings_are_not_guessed() {
    let f = Fixture::new(&[
        ("src/lib.rs", "pub mod engine;\n"),
        (
            "src/engine.rs",
            "pub struct Runner;\nimpl Runner {\n pub fn run(&self){}\n}\n",
        ),
        (
            "src/main.rs",
            "use fixture::engine::Runner;\nfn main(){let r:Runner=make(); {let r:Other=other(); r.run();} r.run();}\n",
        ),
    ]);
    let p = build(
        &f,
        &profile(
            &f,
            serde_json::json!([stage("engine", "src/engine.rs", "Runner")]),
        ),
    )
    .unwrap();
    assert!(!p.connections.iter().any(|e| e.to == "engine::run"));
    assert!(
        p.observations
            .iter()
            .any(|o| o.reason.contains("no unique explicit source type"))
    );
}
#[test]
fn inline_modules_use_their_own_scope_and_imports() {
    let f = Fixture::new(&[
        ("src/lib.rs", "pub mod a;\npub mod b;\n"),
        ("src/a.rs", "pub fn run(){}\n"),
        ("src/b.rs", "pub fn run(){}\n"),
        (
            "src/main.rs",
            "use fixture::a::run;\nmod inner {\n fn run(){fixture::b::run();}\n pub fn work(){run();}\n}\nfn main(){inner::work();}\n",
        ),
    ]);
    let p = build(
        &f,
        &profile(
            &f,
            serde_json::json!([stage("a", "src/a.rs", "run"), stage("b", "src/b.rs", "run")]),
        ),
    )
    .unwrap();
    assert!(p.connections.iter().any(|e| e.to == "b"));
    assert!(!p.connections.iter().any(|e| e.to == "a"));
}
#[test]
fn conditional_import_alternatives_are_retained_without_choosing() {
    let f = Fixture::new(&[
        ("src/lib.rs", "pub mod a;\npub mod b;\n"),
        ("src/a.rs", "pub fn run(){}\n"),
        ("src/b.rs", "pub fn run(){}\n"),
        (
            "src/main.rs",
            "#[cfg(first)]\nuse fixture::a as route;\n#[cfg(second)]\nuse fixture::b as route;\nfn main(){route::run();}\n",
        ),
    ]);
    let p = build(
        &f,
        &profile(
            &f,
            serde_json::json!([stage("a", "src/a.rs", "run"), stage("b", "src/b.rs", "run")]),
        ),
    )
    .unwrap();
    assert!(
        !p.connections
            .iter()
            .any(|e| matches!(e.to.as_str(), "a" | "b"))
    );
    assert!(
        p.observations
            .iter()
            .any(|o| o.reason.contains("ambiguous source import"))
    );
}
#[test]
fn destructured_value_calls_do_not_bind_to_a_same_named_function() {
    let f = Fixture::new(&[
        ("src/lib.rs", "pub mod stage;\n"),
        ("src/stage.rs", "pub fn run(){}\n"),
        (
            "src/main.rs",
            "fn main(){let (callback, _)=callbacks(); callback();}\nfn callback(){fixture::stage::run();}\n",
        ),
    ]);
    let p = build(
        &f,
        &profile(
            &f,
            serde_json::json!([stage("stage", "src/stage.rs", "run")]),
        ),
    )
    .unwrap();
    assert!(!p.connections.iter().any(|e| e.to == "stage"));
    assert!(
        p.observations
            .iter()
            .any(|o| o.reason.contains("value call callback"))
    );
}

#[test]
fn reassigned_constructor_results_are_not_guessed() {
    let f = Fixture::new(&[
        ("src/lib.rs", "pub mod engine;"),
        (
            "src/engine.rs",
            "pub struct Runner; impl Runner {pub fn new()->Self{Self} pub fn run(&self){}}",
        ),
        (
            "src/main.rs",
            "use fixture::engine::Runner; fn main(){let mut r=Runner::new(); r=other(); r.run();}",
        ),
    ]);
    let p = build(
        &f,
        &profile(
            &f,
            serde_json::json!([stage("engine", "src/engine.rs", "Runner")]),
        ),
    )
    .unwrap();
    assert!(!p.connections.iter().any(|e| e.to == "engine::run"));
    assert!(
        p.observations
            .iter()
            .any(|o| o.reason.contains("no unique explicit source type"))
    );
}
#[test]
fn self_calls_reuse_the_inspected_impl_owner() {
    let f = Fixture::new(&[
        ("src/lib.rs", "pub mod stage;"),
        ("src/stage.rs", "pub fn run(){}"),
        (
            "src/main.rs",
            "fn main(){let h=Host::new(); h.go();} struct Host; impl Host { fn new()->Self{Self} fn go(&self){self.helper();} fn helper(&self){Self::more();} fn more(){fixture::stage::run();} }",
        ),
    ]);
    let p = build(
        &f,
        &profile(
            &f,
            serde_json::json!([stage("stage", "src/stage.rs", "run")]),
        ),
    )
    .unwrap();
    assert!(p.connections.iter().any(|e| e.to == "stage"));
}
#[test]
fn invalid_workflow_anchors_are_refused_instead_of_silently_folded() {
    let f = Fixture::new(&[
        ("src/lib.rs", "pub mod stage;"),
        ("src/stage.rs", "pub fn run(){}"),
        ("src/main.rs", "fn main(){fixture::stage::run();}"),
    ]);
    let a=Architecture::parse(&serde_json::json!({"schema":1,"source":[],"modules":[stage("stage","src/stage.rs","run")],"workflows":[{"name":"bad","path":"src/main.rs","function":"missing"}]}).to_string(),&f.0).unwrap();
    assert!(
        build(&f, &a)
            .unwrap_err()
            .to_string()
            .contains("expected one function, found 0")
    );
}

#[test]
fn later_bindings_do_not_hide_an_earlier_function_call() {
    let f = Fixture::new(&[
        ("src/lib.rs", "pub mod stage;"),
        ("src/stage.rs", "pub fn run(){}"),
        (
            "src/main.rs",
            "fn main(){run(); let run=||{};} fn run(){fixture::stage::run();}",
        ),
    ]);
    let p = build(
        &f,
        &profile(
            &f,
            serde_json::json!([stage("stage", "src/stage.rs", "run")]),
        ),
    )
    .unwrap();
    assert!(p.connections.iter().any(|e| e.to == "stage"));
}

#[test]
fn type_imports_do_not_steal_local_value_calls() {
    let f = Fixture::new(&[
        ("src/lib.rs", "pub mod engine;"),
        ("src/engine.rs", "pub struct Engine {}"),
        (
            "src/main.rs",
            "use fixture::engine::Engine as start; fn start(){} fn main(){start();}",
        ),
    ]);
    let p = build(
        &f,
        &profile(
            &f,
            serde_json::json!([stage("engine", "src/engine.rs", "Engine")]),
        ),
    )
    .unwrap();
    assert!(
        p.observations
            .iter()
            .any(|o| o.reason.contains("namespace unresolved"))
    );
    assert!(!p.connections.iter().any(|e| e.to.starts_with("engine")));
}
#[test]
fn fallible_constructor_unwrapping_is_not_inferred() {
    let f = Fixture::new(&[
        ("src/lib.rs", "pub mod engine;"),
        (
            "src/engine.rs",
            "pub struct Runner; impl Runner {pub fn new()->Result<Self>{todo!()} pub fn run(&self){}}",
        ),
        (
            "src/main.rs",
            "use fixture::engine::Runner; fn main(){let r=Runner::new()?; r.run();}",
        ),
    ]);
    let p = build(
        &f,
        &profile(
            &f,
            serde_json::json!([stage("engine", "src/engine.rs", "Runner")]),
        ),
    )
    .unwrap();
    assert!(!p.connections.iter().any(|e| e.to == "engine::run"));
    assert!(
        p.observations
            .iter()
            .any(|o| o.reason.contains("initializer result not established"))
    );
}
