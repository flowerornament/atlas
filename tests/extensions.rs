//! Public contracts are exercised by independent binaries, not only in-crate helpers.
mod support;
use atlas::{
    adapter::{self, Request, Snapshot},
    discovery,
    render::{Ascii, Pele, Renderer, UnsupportedView, View},
    representations,
};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};
use support::Fixture;
const SOURCE: &str = "pub mod build;\npub mod data;\nenum Layer{Stage,Vocabulary}\nconst MODULES:&[(&str,Option<()>,Layer,&[&str])]=&[(\"build\",None,Layer::Stage,&[]),(\"data\",None,Layer::Vocabulary,&[])];\nconst SURFACES:&[(&str,&str,&[&str])]=&[(\"build\",\"build\",&[\"Graph\"])];\n";
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
fn input(f: &Fixture) -> (Request, representations::Facts) {
    let m = cargo_metadata::MetadataCommand::new()
        .current_dir(f.path())
        .no_deps()
        .exec()
        .expect("test fixture prerequisite must succeed");
    let g = discovery::scan_metadata(&m).expect("test fixture prerequisite must succeed");
    let facts =
        representations::extract(f.path(), &m, &g).expect("test fixture prerequisite must succeed");
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
    Path::new(env!("CARGO_BIN_EXE_atlas-girard"))
}

#[test]
fn official_adapter_preserves_custody_and_does_not_edit_inspected_source() {
    let f = project();
    let (request, _) = input(&f);
    let reply = adapter::invoke(plugin(), f.path(), &request).unwrap().reply;
    assert_eq!(reply.producer.name, "atlas-girard");
    assert_eq!(reply.snapshot_id, request.snapshot.id);
    let a = reply
        .architecture()
        .unwrap()
        .clone()
        .bind(f.path())
        .unwrap();
    assert_eq!(a.modules.len(), 2);
    assert_eq!(
        a.provenance,
        ["src/lib.rs::MODULES", "src/lib.rs::SURFACES"]
    );
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
    f.write("src/lib.rs",&(SOURCE.to_owned()+"const COMPUTED:&[&str]=make();\n#[cfg(test)] const HIDDEN:&str=\"no\";\nfn local(){const X:crate::data::Graph=crate::data::Graph{n:1};}\n"));
    let (request, facts) = input(&f);
    assert!(matches!(
        request
            .snapshot
            .constant("src/lib.rs", "COMPUTED")
            .unwrap()
            .value,
        representations::Literal::Opaque(_)
    ));
    assert!(request.snapshot.constant("src/lib.rs", "HIDDEN").is_err());
    assert!(request.snapshot.constant("src/lib.rs", "X").is_err());
    let local = facts.functions.iter().find(|f| f.name == "local").unwrap();
    assert!(!facts.constructors.iter().any(|c| c.from == local.id));
}

struct GraphOnly;
impl Renderer for GraphOnly {
    fn render(&self, g: &atlas::diagram::Diagram) -> atlas::Result<atlas::render::Artifact> {
        Ascii.render(g)
    }
}
#[test]
fn graph_and_table_families_share_backends_without_erasing_their_models() {
    let f = project();
    let (_, facts) = input(&f);
    let q = representations::Query {
        title: "Graph routes".into(),
        types: vec![representations::Selector {
            name: "Graph".into(),
            file: None,
        }],
        apis: vec![],
        preparation_files: vec![],
        preparation_functions: vec![],
    };
    let views = representations::project(&facts, &q).unwrap();
    let graph = atlas::workflow::read(f.path(), "src/build.rs", "build")
        .unwrap()
        .diagram();
    for view in [
        View::Graph(&graph),
        View::Representations(&views, representations::Panel::Producers),
    ] {
        let text = Ascii.render_view(view).unwrap();
        assert_eq!(text.primary.suffix, "txt");
        let svg = Pele { html: false }.render_view(view).unwrap();
        assert_eq!(svg.primary.suffix, "svg");
        assert!(
            String::from_utf8(svg.primary.bytes)
                .unwrap()
                .contains("Graph")
        );
    }
    let error = GraphOnly
        .render_view(View::Representations(
            &views,
            representations::Panel::Producers,
        ))
        .err()
        .unwrap();
    assert_eq!(
        error.downcast_ref::<UnsupportedView>().unwrap().0,
        "representations"
    );
    let html = Pele { html: true }
        .render_view(View::Representations(
            &views,
            representations::Panel::Producers,
        ))
        .unwrap();
    assert_eq!(html.primary.suffix, "html");
    assert!(html.companions.iter().any(|d| d.suffix == "svg"));
}

#[test]
fn independently_built_adapters_use_only_public_api_and_the_stock_cli() {
    let consumer = Fixture::new(&[]);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    consumer.write("Cargo.toml",&format!("[package]\nname='external-atlas-consumer'\nversion='0.0.0'\nedition='2024'\n[workspace]\n[dependencies]\natlas={{path={:?}}}\n[[bin]]\nname='girard-adapter'\npath='src/girard.rs'\n[[bin]]\nname='foreign-adapter'\npath='src/foreign.rs'\n",root.to_str().unwrap()));
    consumer.write("src/girard.rs", include_str!("../src/bin/atlas-girard.rs"));
    consumer.write(
        "src/foreign.rs",
        include_str!("../examples/foreign-adapter.rs"),
    );
    let cli = Path::new(env!("CARGO_BIN_EXE_atlas"));
    let before = fs::read(cli).unwrap();
    let build = Command::new("cargo")
        .args(["build", "--offline", "--quiet"])
        .current_dir(consumer.path())
        .env("CARGO_TARGET_DIR", root.join("target/adapter-consumer"))
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    assert_eq!(
        before,
        fs::read(cli).unwrap(),
        "building a public consumer does not rebuild the stock CLI"
    );
    let f = project();
    let girard = root
        .join("target/adapter-consumer/debug/girard-adapter")
        .with_extension(std::env::consts::EXE_EXTENSION);
    let run = Command::new(cli)
        .arg("overview")
        .arg(f.path())
        .arg("--adapter")
        .arg(girard)
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
    f.write("src/lib.rs","pub mod build;\npub mod data;\nenum Role{Pass,Resource} const PIPELINE:&[(&str,&str,Role,&str)]=&[(\"build\",\"build\",Role::Pass,\"Compiler pass\"),(\"data\",\"\",Role::Resource,\"Shared data\")];");
    let foreign = root
        .join("target/adapter-consumer/debug/foreign-adapter")
        .with_extension(std::env::consts::EXE_EXTENSION);
    let run = Command::new(cli)
        .arg("overview")
        .arg(f.path())
        .arg("--adapter")
        .arg(foreign)
        .arg("--json")
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let result: serde_json::Value = serde_json::from_slice(&run.stdout).unwrap();
    let modules = result["overview"]["architecture"]["modules"]
        .as_array()
        .unwrap();
    assert_eq!(modules[0]["label"], "Compiler pass");
    assert_eq!(modules[1]["kind"], "support");
    assert_eq!(modules[1]["label"], "Shared data");
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
    let output = Command::new(env!("CARGO_BIN_EXE_atlas"))
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
    let result = Command::new(env!("CARGO_BIN_EXE_atlas"))
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
    let result = Command::new(env!("CARGO_BIN_EXE_atlas"))
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
