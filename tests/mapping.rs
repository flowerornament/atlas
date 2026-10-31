//! End-to-end source facts and renderer fidelity, not golden architecture text.
use atlas::{
    diagram::{self, Level as Detail},
    discovery,
    model::{Evidence, Resolution},
    render::{Ascii, Renderer, mermaid},
};
mod support;
use support::Fixture;
fn project(
    graph: &atlas::model::SourceGraph,
    scope: &str,
    level: Detail,
) -> atlas::Result<atlas::diagram::Diagram> {
    diagram::project(
        graph,
        None,
        &diagram::Query {
            focus: Some(scope.into()),
            level,
            view: diagram::View::Types,
        },
    )
}
#[test]
fn all_files_survive_disconnected_and_empty_projection() {
    let f = Fixture::new(&[
        (
            "src/lib.rs",
            "pub struct Input;\npub fn build(x: Input) -> Output {}\npub struct Output;\n",
        ),
        ("src/empty.rs", ""),
        ("src/disconnected.rs", "pub struct Alone;\n"),
    ]);
    let graph = discovery::scan_workspace(f.path()).unwrap();
    let diagram = project(&graph, "src", Detail::Files).unwrap();
    assert_eq!(diagram.inventory.len(), 3);
    assert_eq!(diagram.nodes.len(), 3);
    assert!(diagram.inventory.contains(&"src/empty.rs".into()));
    let ids: Vec<_> = graph.relationships.iter().map(|e| e.id.clone()).collect();
    let carried: Vec<_> = diagram
        .edges
        .iter()
        .flat_map(|e| e.relationships.clone())
        .chain(
            diagram
                .nodes
                .iter()
                .flat_map(|n| n.internal_relationships.clone()),
        )
        .collect();
    assert_eq!(ids.len(), carried.len());
    assert!(ids.iter().all(|id| carried.contains(id)));
    let text = String::from_utf8(Ascii.render(&diagram).unwrap().primary.bytes).unwrap();
    for file in ["lib.rs", "empty.rs", "disconnected.rs"] {
        assert!(text.contains(file));
    }
}
#[test]
#[expect(
    clippy::many_single_char_names,
    reason = "short fixture and graph-vertex bindings mirror the source relationships under test"
)]
fn params_returns_and_type_bodies_keep_evidence_and_direction() {
    let f = Fixture::new(&[(
        "src/lib.rs",
        "pub struct Input;\npub struct Output;\npub struct Engine {\n  code: Output,\n}\npub fn emit(x: Input) -> Output { unreachable!() }\n",
    )]);
    let g = discovery::scan_workspace(f.path()).unwrap();
    for (name, kind, target) in [
        ("emit", Evidence::ParameterMention, "Input"),
        ("emit", Evidence::ReturnMention, "Output"),
        ("Engine", Evidence::TypeBodyMention, "Output"),
    ] {
        assert!(
            g.relationships
                .iter()
                .any(|r| r.source.ends_with(&format!(":{name}"))
                    && r.evidence == kind
                    && r.spelling == target)
        );
    }
    let d = project(&g, "src", Detail::Declarations).unwrap();
    let m = mermaid(&d).unwrap();
    let a = String::from_utf8(Ascii.render(&d).unwrap().primary.bytes).unwrap();
    for edge in &d.edges {
        assert!(m.contains(edge.evidence.label()));
        assert!(a.contains(edge.evidence.label()));
    }
    assert_eq!(d.edges.len(), 3);
}
#[test]
fn duplicate_spelling_stays_ambiguous_and_external_binding_unresolved() {
    let f = Fixture::new(&[
        (
            "src/lib.rs",
            "pub mod a;\npub mod b;\npub fn build(x: Shared) {}\n",
        ),
        ("src/a.rs", "pub struct Shared;\n"),
        ("src/b.rs", "pub struct Shared;\n"),
    ]);
    let g = discovery::scan_workspace(f.path()).unwrap();
    assert!(
        g.relationships
            .iter()
            .any(|r| matches!(&r.target,Resolution::Ambiguous(ids) if ids.len()==2))
    );
    let d = project(&g, "src/lib.rs", Detail::Declarations).unwrap();
    assert!(d.nodes.iter().any(|n| n.label.contains("2 candidates")));
    f.write(
        "src/lib.rs",
        "use foreign::Shared;\npub mod a;\npub mod b;\npub fn build(x: Shared) {}\n",
    );
    let g = discovery::scan_workspace(f.path()).unwrap();
    assert!(
        g.relationships
            .iter()
            .any(|r| matches!(r.target, Resolution::Unresolved(_)))
    );
}
#[test]
fn test_path_mounts_and_helpers_do_not_pollute_production_mentions() {
    let f = Fixture::new(&[
        (
            "src/lib.rs",
            "pub struct Value;\n#[cfg(test)]\n#[path = \"special.rs\"]\nmod testing;\n#[cfg(test)]\nfn helper(x: Value) {}\npub fn real(x: Value) {}\n",
        ),
        (
            "src/special.rs",
            "mod nested;\npub fn test_only(x: Value) {}\n",
        ),
        ("src/nested.rs", "pub fn also_test(x: Value) {}\n"),
    ]);
    let g = discovery::scan_workspace(f.path()).unwrap();
    assert!(
        g.files
            .iter()
            .find(|f| f.path == "src/special.rs")
            .unwrap()
            .test_only
    );
    assert!(
        g.files
            .iter()
            .find(|f| f.path == "src/nested.rs")
            .unwrap()
            .test_only
    );
    assert_eq!(g.relationships.len(), 1);
    assert!(g.relationships[0].source.ends_with(":real"));
    // A production mount takes precedence over a test mount of the same file.
    f.write("src/lib.rs","pub struct Value;\n#[cfg(test)]\n#[path = \"special.rs\"]\nmod testing;\n#[path = \"special.rs\"]\nmod production;\n");
    let g = discovery::scan_workspace(f.path()).unwrap();
    assert!(
        !g.files
            .iter()
            .find(|f| f.path == "src/special.rs")
            .unwrap()
            .test_only
    );
}
#[test]
fn source_shaped_strings_comments_and_bounds_do_not_become_runtime_edges() {
    let f = Fixture::new(&[(
        "src/lib.rs",
        "pub struct Value;\npub struct Out;\nconst TEXT: &str = r#\"\npub struct Phantom;\n\"#;\n// pub struct Ghost;\npub fn map<F: Fn(Value) -> Value>(x: F) -> Out {}\n",
    )]);
    let g = discovery::scan_workspace(f.path()).unwrap();
    assert!(
        !g.files[0]
            .declarations
            .iter()
            .any(|d| d.name == "Phantom" || d.name == "Ghost")
    );
    assert!(
        g.relationships
            .iter()
            .any(|r| r.spelling == "Value" && r.evidence == Evidence::ParameterMention)
    );
    assert!(
        g.relationships
            .iter()
            .any(|r| r.spelling == "Out" && r.evidence == Evidence::ReturnMention)
    );
    assert!(
        !g.relationships
            .iter()
            .any(|r| r.spelling == "Value" && r.evidence == Evidence::ReturnMention)
    );
}
#[test]
fn scopes_fail_explicitly_and_scoped_targets_remain_context() {
    let f = Fixture::new(&[
        ("src/lib.rs", "pub mod a;\npub mod b;\n"),
        ("src/a.rs", "pub fn run(x: Input) {}\n"),
        ("src/b.rs", "pub struct Input;\n"),
    ]);
    let g = discovery::scan_workspace(f.path()).unwrap();
    assert!(project(&g, "missing", Detail::Files).is_err());
    let d = project(&g, "src/a.rs", Detail::Declarations).unwrap();
    assert_eq!(d.inventory, vec!["src/a.rs"]);
    assert!(
        d.nodes
            .iter()
            .any(|n| n.context && n.label.contains("Input"))
    );
    let mut ordered = d.nodes.iter().map(|n| &n.id).collect::<Vec<_>>();
    let before = ordered.clone();
    ordered.sort();
    assert_eq!(before, ordered);
}

#[test]
#[expect(
    clippy::many_single_char_names,
    reason = "short fixture and graph-vertex bindings mirror the source relationships under test"
)]
fn cycle_diamond_and_ambiguous_edges_are_conserved_by_projection() {
    let f = Fixture::new(&[(
        "src/lib.rs",
        "pub struct A {\n  b: B,\n  c: C,\n}\npub struct B {\n  d: D,\n}\npub struct C {\n  d: D,\n}\npub struct D {\n  a: A,\n}\n",
    )]);
    let g = discovery::scan_workspace(f.path()).unwrap();
    let d = project(&g, "src", Detail::Declarations).unwrap();
    assert_eq!(g.relationships.len(), 5);
    assert_eq!(d.edges.len(), 5);
    let m = mermaid(&d).unwrap();
    assert_eq!(m.matches("-->").count(), 5);
    let a = String::from_utf8(Ascii.render(&d).unwrap().primary.bytes).unwrap();
    assert_eq!(a.matches("--composes").count(), 5);
}
#[test]
fn mermaid_labels_escape_quotes_html_and_directives() {
    use atlas::diagram::{Diagram, Node};
    let label = "name\"<script>&%%{init:";
    let d = Diagram {
        title: "test".into(),
        nodes: vec![Node {
            id: "not a Mermaid id]".into(),
            label: label.into(),
            group: "src/\"unsafe\"".into(),
            source_sites: vec![],
            context: false,
            internal_relationships: vec![],
            surface: None,
        }],
        edges: vec![],
        inventory: vec![],
        notes: vec![],
        level: Detail::Declarations,
        view: diagram::View::Types,
        coverage: diagram::Coverage::default(),
        evidence_links: vec![],
    };
    let m = mermaid(&d).unwrap();
    assert!(m.contains("#quot;"));
    assert!(m.contains("#60;script#62;"));
    assert!(m.contains("n0["));
    assert!(!m.contains("<script>"));
}
#[test]
fn embedded_pele_renders_without_checkout_adapter() {
    use atlas::render::Pele;
    let f = Fixture::new(&[("src/lib.rs", "pub struct Value;")]);
    let g = discovery::scan_workspace(f.path()).unwrap();
    let d = project(&g, "src", Detail::Files).unwrap();
    let artifact = Pele { html: false }.render(&d).unwrap();
    let svg = String::from_utf8(artifact.primary.bytes).unwrap();
    assert!(svg.contains("<svg"));
    assert!(svg.contains("lib.rs"));
}

#[test]
fn utf8_source_is_inventoried_without_byte_boundary_panics() {
    let f = Fixture::new(&[(
        "src/lib.rs",
        "pub struct Café;\nimpl Café {\n fn hello(&self) {}\n}\n",
    )]);
    let g = discovery::scan_workspace(f.path()).unwrap();
    assert_eq!(g.files.len(), 1);
    assert!(g.files[0].content_fingerprint.starts_with("fnv1a64:"));
    let first = g.files[0].content_fingerprint.clone();
    f.write("src/lib.rs", "pub struct Other;\n");
    let g = discovery::scan_workspace(f.path()).unwrap();
    assert_ne!(first, g.files[0].content_fingerprint);
}

#[test]
fn malformed_external_diagram_is_refused_by_both_backends() {
    use atlas::diagram::{Diagram, Edge};
    let d = Diagram {
        title: "bad".into(),
        nodes: vec![],
        edges: vec![Edge {
            label: None,
            from: "missing".into(),
            to: "also missing".into(),
            evidence: Evidence::ParameterMention.into(),
            declarations: vec![],
            relationships: vec![],
        }],
        inventory: vec![],
        notes: vec![],
        level: Detail::Declarations,
        view: diagram::View::Types,
        coverage: diagram::Coverage::default(),
        evidence_links: vec![],
    };
    assert!(Ascii.render(&d).is_err());
    assert!(mermaid(&d).is_err());
}

#[test]
fn library_names_and_workspace_dependency_aliases_stay_internal() {
    let f = Fixture::new(&[
        (
            "library/Cargo.toml",
            "[package]\nname='actual-package'\nversion='0.1.0'\nedition='2024'\n[lib]\nname='actual_lib'\n",
        ),
        ("library/src/lib.rs", "pub struct Shared;\n"),
        (
            "consumer/Cargo.toml",
            "[package]\nname='consumer'\nversion='0.1.0'\nedition='2024'\n[dependencies]\nlocal_alias={package='actual-package',path='../library'}\n",
        ),
        (
            "consumer/src/lib.rs",
            "use local_alias::Shared;\npub fn build(x: Shared) {}\n",
        ),
    ]);
    f.write(
        "Cargo.toml",
        "[workspace]\nmembers=['library','consumer']\nresolver='3'\n",
    );
    let g = discovery::scan_workspace(f.path()).unwrap();
    assert!(g.import_roots["consumer"].contains(&"local_alias".into()));
    assert!(g.import_roots["consumer"].contains(&"actual_lib".into()));
    let edge = g
        .relationships
        .iter()
        .find(|r| r.source.ends_with(":build"))
        .unwrap();
    assert!(matches!(edge.target, Resolution::Unique(_)));
    f.write(
        "consumer/src/lib.rs",
        "use actual_lib::Shared;\npub fn build(x: Shared) {}\n",
    );
    let g = discovery::scan_workspace(f.path()).unwrap();
    assert!(
        g.relationships
            .iter()
            .any(|r| r.source.ends_with(":build") && matches!(r.target, Resolution::Unique(_)))
    );
}

#[test]
fn cli_retains_evidence_when_external_render_fails_and_refuses_collisions() {
    use std::process::Command;
    let f = Fixture::new(&[(
        "src/lib.rs",
        &format!(
            "pub struct Value;\npub fn {}(x: Value) -> Value {{}}\n",
            "long".repeat(15000)
        ),
    )]);
    let output = f.path().join("failed.html");
    let result = Command::new(env!("CARGO_BIN_EXE_atlas"))
        .args([
            "map",
            f.path().to_str().unwrap(),
            "--level",
            "declarations",
            "--backend",
            "pele",
            "--out",
            output.to_str().unwrap(),
            "--evidence-dir",
            f.path().to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(!output.exists());
    for extension in ["mmd", "source.json", "diagram.json"] {
        assert!(
            output.with_extension(extension).exists(),
            "missing {extension}"
        );
    }
    let collision = f.path().join("collision.mmd");
    let result = Command::new(env!("CARGO_BIN_EXE_atlas"))
        .args([
            "map",
            f.path().to_str().unwrap(),
            "--out",
            collision.to_str().unwrap(),
            "--backend",
            "pele",
        ])
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(!collision.exists());
    assert!(String::from_utf8_lossy(&result.stderr).contains("collides"));
}
