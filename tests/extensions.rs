//! Public contracts are exercised by independent binaries, not only in-crate helpers.
#[path = "support/adapter.rs"]
mod example_adapter;
mod support;
use cargo_atlas::{
    adapter::{self, Request, Snapshot},
    discovery,
    render::{Ascii, Pele, Renderer, UnsupportedView, View},
    types,
};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};
use support::Fixture;
const SOURCE: &str = "pub mod build;\npub mod data;\nenum Role{Pass,Resource}\nconst PIPELINE:&[(&str,&str,Role,&str)]=&[(\"build\",\"build\",Role::Pass,\"Compiler pass\"),(\"data\",\"\",Role::Resource,\"Shared data\")];\n";
fn project() -> Fixture {
    Fixture::new(&[
        ("src/lib.rs", SOURCE),
        (
            "src/build.rs",
            "pub fn build()->crate::data::Graph{crate::data::Graph{n:0}}",
        ),
        ("src/data.rs", "pub struct Graph{pub n:u32}"),
    ])
}
fn input(f: &Fixture) -> (Request, types::Facts) {
    let m = cargo_metadata::MetadataCommand::new()
        .current_dir(f.path())
        .no_deps()
        .exec()
        .expect("test fixture prerequisite must succeed");
    let g = discovery::scan_metadata(&m).expect("test fixture prerequisite must succeed");
    let facts = types::extract(f.path(), &m, &g).expect("test fixture prerequisite must succeed");
    (
        Request::new(
            Snapshot::from_facts(&g, &facts, "src/lib.rs")
                .expect("test fixture prerequisite must succeed"),
            "src/lib.rs".into(),
        )
        .expect("test fixture prerequisite must succeed"),
        facts,
    )
}
fn plugin() -> &'static Path {
    example_adapter::example()
}

#[test]
fn example_adapter_preserves_custody_and_does_not_edit_inspected_source() {
    let f = project();
    let (request, _) = input(&f);
    let reply = adapter::invoke(plugin(), f.path(), &request).unwrap().reply;
    assert_eq!(reply.producer.name, "example-pipeline");
    assert_eq!(reply.snapshot_id, request.snapshot.id);
    let a = reply
        .architecture()
        .unwrap()
        .clone()
        .bind(f.path())
        .unwrap();
    assert_eq!(a.modules.len(), 2);
    assert_eq!(a.provenance, ["src/lib.rs::PIPELINE"]);
    assert_eq!(
        fs::read_to_string(f.path().join("src/lib.rs")).unwrap(),
        SOURCE
    );
    f.write("src/data.rs", "pub struct Graph{pub n:u64}");
    assert!(
        adapter::invoke(plugin(), f.path(), &request)
            .unwrap_err()
            .to_string()
            .contains("source changed")
    );
}
#[test]
fn protocol_and_source_reference_checks_are_not_the_adapters_semantic_authority() {
    let f = project();
    let (request, _) = input(&f);
    let mut reply = adapter::invoke(plugin(), f.path(), &request).unwrap().reply;
    reply.protocol += 1;
    assert!(
        adapter::validate_reply(&request, &reply)
            .unwrap_err()
            .to_string()
            .contains("incompatible")
    );
    reply.protocol = adapter::PROTOCOL;
    reply.snapshot_id = "other".into();
    assert!(
        adapter::validate_reply(&request, &reply)
            .unwrap_err()
            .to_string()
            .contains("different source")
    );
    reply.snapshot_id = request.snapshot.id.clone();
    let adapter::Outcome::Declarations { architecture } = &mut reply.result else {
        panic!()
    };
    architecture.provenance.push("src/lib.rs::INVENTED".into());
    assert!(
        adapter::validate_reply(&request, &reply)
            .unwrap_err()
            .to_string()
            .contains("source witness")
    );
    assert!(adapter::invoke(&f.path().join("missing-adapter"), f.path(), &request).is_err());
    let mut bad = request;
    bad.protocol += 1;
    let mut child = Command::new(plugin())
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    serde_json::to_writer(child.stdin.take().unwrap(), &bad).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let reply: adapter::Reply = serde_json::from_slice(&output.stdout).unwrap();
    assert!(
        matches!(reply.result,adapter::Outcome::Refused{message} if message.contains("unsupported adapter protocol"))
    );
}
#[test]
fn constant_syntax_stays_opaque_when_computed_and_local_const_work_is_not_a_call() {
    let f = project();
    f.write("src/lib.rs",&(SOURCE.to_owned()+"const COMPUTED:&[&str]=make().finish();\nconst EXPANDED:&[&str]=list!();\n#[cfg(test)] const HIDDEN:&str=\"no\";\nfn local(){const X:crate::data::Graph=crate::data::Graph{n:1};}\n"));
    let (request, facts) = input(&f);
    for name in ["COMPUTED", "EXPANDED"] {
        assert!(matches!(
            request.snapshot.constant("src/lib.rs", name).unwrap().value,
            types::Literal::Opaque(_)
        ));
    }
    assert!(request.snapshot.constant("src/lib.rs", "HIDDEN").is_err());
    assert!(request.snapshot.constant("src/lib.rs", "X").is_err());
    let local = facts.functions.iter().find(|f| f.name == "local").unwrap();
    assert!(!facts.constructors.iter().any(|c| c.from == local.id));
}

#[test]
fn call_syntax_in_constants_arrives_structured_but_unevaluated() {
    use types::Literal;
    let f = project();
    f.write("src/lib.rs", &(SOURCE.to_owned() + "const ROWS:&[(&str,Option<Part>)]=&[(\"a\",Some(Part::Host)),(\"b\",None)];\nconst NAMED:Option<&str>=Some(\"x\");\nconst WRAPPED:Wrapper=Wrapper(1,X);\nconst GENERIC:Option<u8>=Option::<u8>::Some(1);\n"));
    let (request, _) = input(&f);
    let value = |name| {
        request
            .snapshot
            .constant("src/lib.rs", name)
            .unwrap()
            .value
            .clone()
    };
    let rows = value("ROWS");
    let rows = rows.array().unwrap();
    let (path, args) = rows[0].tuple().unwrap()[1].call().unwrap();
    assert_eq!(path, ["Some"]);
    assert_eq!(args[0].path().unwrap(), ["Part", "Host"]);
    assert_eq!(rows[1].tuple().unwrap()[1].path().unwrap(), ["None"]);
    let named = value("NAMED");
    let (path, args) = named.call().unwrap();
    assert_eq!(
        (path, args[0].string()),
        (&["Some".to_owned()][..], Some("x"))
    );
    let wrapped = value("WRAPPED");
    let (path, args) = wrapped.call().unwrap();
    assert_eq!(path, ["Wrapper"]);
    assert!(matches!(&args[0], Literal::Opaque(text) if text == "1"));
    assert_eq!(args[1].path().unwrap(), ["X"]);
    // A callee with generic arguments is not a plain path.
    assert!(matches!(value("GENERIC"), Literal::Opaque(_)));
    // The JSON form adapters receive.
    assert_eq!(
        serde_json::to_value(&named).unwrap(),
        serde_json::json!({"kind": "call", "value": {"path": ["Some"], "args": [{"kind": "string", "value": "x"}]}})
    );
}

struct GraphOnly;
impl Renderer for GraphOnly {
    fn render(
        &self,
        g: &cargo_atlas::diagram::Diagram,
    ) -> cargo_atlas::Result<cargo_atlas::render::Artifact> {
        Ascii.render(g)
    }
}
#[test]
fn graph_and_table_families_share_backends_without_erasing_their_models() {
    let f = project();
    let (_, facts) = input(&f);
    let q = types::Query {
        title: "Graph routes".into(),
        types: vec![types::Selector {
            name: "Graph".into(),
            file: None,
        }],
        apis: vec![],
        preparation_files: vec![],
        preparation_functions: vec![],
    };
    let views = types::project(&facts, &q).unwrap();
    let graph = cargo_atlas::workflow::read(f.path(), "src/build.rs", "build")
        .unwrap()
        .diagram();
    for view in [
        View::Graph(&graph),
        View::Types(&views, types::Panel::Producers),
    ] {
        let text = Ascii.render_view(view).unwrap();
        assert_eq!(text.primary.suffix, "txt");
        let svg = Pele {
            html: false,
            theme: cargo_atlas::render::Theme::Light,
            bare: false,
        }
        .render_view(view)
        .unwrap();
        assert_eq!(svg.primary.suffix, "svg");
        assert!(
            String::from_utf8(svg.primary.bytes)
                .unwrap()
                .contains("Graph")
        );
    }
    let error = GraphOnly
        .render_view(View::Types(&views, types::Panel::Producers))
        .err()
        .unwrap();
    assert_eq!(error.downcast_ref::<UnsupportedView>().unwrap().0, "types");
    let html = Pele {
        html: true,
        theme: cargo_atlas::render::Theme::Light,
        bare: false,
    }
    .render_view(View::Types(&views, types::Panel::Producers))
    .unwrap();
    assert_eq!(html.primary.suffix, "html");
    assert!(html.companions.iter().any(|d| d.suffix == "svg"));
}

#[test]
fn independently_built_adapters_use_only_public_api_and_the_stock_cli() {
    let cli = Path::new(env!("CARGO_BIN_EXE_cargo-atlas"));
    let before = fs::read(cli).unwrap();
    let adapter = plugin();
    assert_eq!(
        before,
        fs::read(cli).unwrap(),
        "building a public consumer does not rebuild the stock CLI"
    );
    assert!(adapter.starts_with(example_adapter::consumer_target()));
    let f = project();
    let run = Command::new(cli)
        .arg("overview")
        .arg(f.path())
        .arg("--adapter")
        .arg(adapter)
        .arg("--json")
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let result: serde_json::Value = serde_json::from_slice(&run.stdout).unwrap();
    assert_eq!(result["adapter"]["producer"]["version"], "0.0.0");
    let modules = result["overview"]["architecture"]["modules"]
        .as_array()
        .unwrap();
    assert_eq!(modules[0]["label"], "Compiler pass");
    assert_eq!(modules[1]["kind"], "support");
    assert_eq!(modules[1]["label"], "Shared data");
}

#[test]
fn overview_requires_an_explicit_adapter() {
    let f = project();
    let run = Command::new(env!("CARGO_BIN_EXE_cargo-atlas"))
        .arg("overview")
        .arg(f.path())
        .arg("--json")
        .output()
        .unwrap();
    assert!(!run.status.success());
    assert!(String::from_utf8_lossy(&run.stderr).contains("--adapter"));
}

#[cfg(unix)]
fn executable(f: &Fixture, body: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = f.path().join("target/mock-adapter");
    f.write(
        "target/mock-adapter",
        &format!("#!/usr/bin/env node\n{body}\n"),
    );
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))
        .expect("test fixture prerequisite must succeed");
    path
}
#[cfg(unix)]
#[test]
fn malformed_and_oversized_streams_refuse_without_partial_artifacts() {
    let f = project();
    let (request, _) = input(&f);
    let malformed = executable(&f, "process.stdout.write('not JSON');");
    assert!(
        adapter::invoke(&malformed, f.path(), &request)
            .unwrap_err()
            .to_string()
            .contains("JSON")
    );
    let large = executable(
        &f,
        &format!(
            "process.stdout.write(' '.repeat({}));",
            adapter::MAX_REPLY_BYTES + 1
        ),
    );
    assert!(
        adapter::invoke(&large, f.path(), &request)
            .unwrap_err()
            .to_string()
            .contains("exceeds")
    );
    assert!(!f.path().join("target/atlas/overview.svg").exists());
}
#[cfg(unix)]
#[test]
fn source_changes_during_an_adapter_call_refuse_before_rendering() {
    let f = project();
    let (request, _) = input(&f);
    let reply = adapter::invoke(plugin(), f.path(), &request).unwrap().reply;
    let body = format!(
        "const fs=require('node:fs');fs.readFileSync(0);fs.appendFileSync({},'\\n// changed during adapter\\n');process.stdout.write({});",
        serde_json::to_string(&f.path().join("src/data.rs")).unwrap(),
        serde_json::to_string(&serde_json::to_string(&reply).unwrap()).unwrap()
    );
    let mutator = executable(&f, &body);
    let output = Command::new(env!("CARGO_BIN_EXE_cargo-atlas"))
        .arg("overview")
        .arg(f.path())
        .arg("--adapter")
        .arg(mutator)
        .args(["--backend", "pele", "--out", "target/denied.svg"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("source changed"));
    assert!(!f.path().join("target/denied.svg").exists());
    assert!(!f.path().join("target/denied.json").exists());
}

#[test]
fn module_selectors_distinguish_qualified_paths_unique_leaves_and_ambiguity() {
    let f = Fixture::new(&[
        ("src/lib.rs", "pub mod parent;\npub mod other;"),
        ("src/parent/mod.rs", "pub mod child;"),
        ("src/parent/child.rs", "pub struct Value;"),
        ("src/other.rs", "pub struct Other;"),
    ]);
    let (request, _) = input(&f);
    assert_eq!(
        request.snapshot.module_root("child").unwrap(),
        "src/parent/child.rs"
    );
    assert_eq!(
        request.snapshot.module_root("parent::child").unwrap(),
        "src/parent/child.rs"
    );
    assert!(request.snapshot.module_root("missing::child").is_err());
    f.write("src/other.rs", "pub mod child;");
    f.write("src/other/child.rs", "pub struct Other;");
    let (request, _) = input(&f);
    assert!(request.snapshot.module_root("child").is_err());
    assert_eq!(
        request.snapshot.module_root("parent::child").unwrap(),
        "src/parent/child.rs"
    );
}

#[test]
fn scoped_module_selectors_look_only_in_the_declaring_package() {
    // Two packages each have a module whose last segment is `check`.
    let f = Fixture::new(&[
        (
            "Cargo.toml",
            "[package]\nname='fixture'\nversion='0.1.0'\nedition='2024'\n[workspace]\nmembers=['tools']\n",
        ),
        ("src/lib.rs", "pub mod check;\npub mod report;"),
        ("src/check.rs", "pub struct Finding;"),
        ("src/report.rs", "pub struct Summary;"),
        (
            "tools/Cargo.toml",
            "[package]\nname='tools'\nversion='0.1.0'\nedition='2024'\n",
        ),
        ("tools/src/lib.rs", "pub mod release;"),
        ("tools/src/release/mod.rs", "pub mod check;"),
        ("tools/src/release/check.rs", "pub struct Step;"),
    ]);
    let (request, _) = input(&f);
    let snapshot = &request.snapshot;
    let lib = snapshot
        .files
        .iter()
        .find(|f| f.path == "src/lib.rs")
        .unwrap();
    assert_eq!(lib.packages, ["fixture"]);
    assert_eq!(
        snapshot.module_root_from("src/lib.rs", "check").unwrap(),
        "src/check.rs"
    );
    assert_eq!(
        snapshot
            .module_root_from("tools/src/lib.rs", "check")
            .unwrap(),
        "tools/src/release/check.rs"
    );
    // The unscoped selector still sees both packages and refuses.
    assert!(snapshot.module_root("check").is_err());
    assert!(
        snapshot
            .module_root_from("src/missing.rs", "check")
            .is_err()
    );

    // Ambiguity inside the declaring package still refuses.
    f.write(
        "src/lib.rs",
        "pub mod check;\npub mod report;\npub mod nested;",
    );
    f.write("src/nested/mod.rs", "pub mod check;");
    f.write("src/nested/check.rs", "pub struct Other;");
    let (request, _) = input(&f);
    assert!(
        request
            .snapshot
            .module_root_from("src/lib.rs", "check")
            .is_err()
    );
    assert_eq!(
        request
            .snapshot
            .module_root_from("src/lib.rs", "nested::check")
            .unwrap(),
        "src/nested/check.rs"
    );
}

#[cfg(unix)]
#[test]
fn successful_diagnostics_survive_the_process_boundary() {
    let f = project();
    let (request, _) = input(&f);
    let reply = adapter::invoke(plugin(), f.path(), &request).unwrap().reply;
    let body = format!(
        "require('node:fs').readFileSync(0);process.stderr.write('example diagnostic\\n');process.stdout.write({});",
        serde_json::to_string(&serde_json::to_string(&reply).unwrap()).unwrap()
    );
    let program = executable(&f, &body);
    let invocation = adapter::invoke(&program, f.path(), &request).unwrap();
    assert_eq!(invocation.diagnostics, "example diagnostic\n");
    assert!(invocation.reply.architecture().is_ok());
}

#[test]
fn overview_cannot_replace_the_adapter_it_invoked() {
    let f = project();
    let adapter = f.path().join("adapter.txt");
    fs::copy(plugin(), &adapter).expect("copy adapter");
    let before = fs::read(&adapter).expect("adapter bytes");
    let result = Command::new(env!("CARGO_BIN_EXE_cargo-atlas"))
        .arg("overview")
        .arg("--root")
        .arg(f.path())
        .arg("--adapter")
        .arg(&adapter)
        .args(["--out", "adapter.txt"])
        .output()
        .expect("Atlas");
    assert!(!result.status.success());
    assert!(
        String::from_utf8_lossy(&result.stderr).contains("collision"),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(fs::read(&adapter).expect("adapter bytes"), before);
    assert!(!f.path().join("adapter.json").exists());
}

#[test]
fn overview_protects_a_relative_adapter_in_the_callers_directory() {
    let f = project();
    let launch = Fixture::new(&[("src/lib.rs", "")]);
    let adapter = launch.path().join("adapter.txt");
    fs::copy(plugin(), &adapter).expect("copy adapter");
    let before = fs::read(&adapter).expect("adapter bytes");
    let result = Command::new(env!("CARGO_BIN_EXE_cargo-atlas"))
        .current_dir(launch.path())
        .arg("overview")
        .arg("--root")
        .arg(f.path())
        .args(["--adapter", "adapter.txt", "--out"])
        .arg(&adapter)
        .output()
        .expect("Atlas");
    assert!(!result.status.success());
    assert!(
        String::from_utf8_lossy(&result.stderr).contains("collision"),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(fs::read(&adapter).expect("adapter bytes"), before);
    assert!(!launch.path().join("adapter.json").exists());
}
