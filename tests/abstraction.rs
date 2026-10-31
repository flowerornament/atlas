//! Architecture declarations and quotient graphs have different evidence domains.
mod support;
use atlas::{
    architecture::Architecture,
    diagram::{self, Level, Query, View},
    discovery,
};
use std::{collections::BTreeSet, fs, process::Command};
use support::Fixture;
const PROFILE: &str = r#"{
  "schema": 1,
  "source": [
    "src/lib.rs::MODULES",
    "src/lib.rs::SURFACES"
  ],
  "modules": [
    {
      "name": "a",
      "kind": "stage",
      "path": "src/a",
      "entry": "one",
      "types": [
        "Alpha"
      ],
      "dependencies": [
        "b"
      ]
    },
    {
      "name": "b",
      "kind": "stage",
      "path": "src/b.rs",
      "entry": "build",
      "types": [
        "Beta"
      ],
      "dependencies": []
    },
    {
      "name": "shared",
      "kind": "vocabulary",
      "path": "src/shared.rs",
      "entry": "",
      "types": [],
      "dependencies": []
    },
    {
      "name": "diagnostic",
      "kind": "instrument",
      "path": "src/diagnostic.rs",
      "entry": "",
      "types": [],
      "dependencies": [
        "a"
      ]
    }
  ],
  "workflows": [
    {
      "name": "one",
      "path": "src/a/mod.rs",
      "function": "one"
    }
  ]
}"#;
fn fixture() -> Fixture {
    Fixture::new(&[
        ("src/lib.rs", "pub mod a;\npub mod b;\npub mod shared;\n"),
        (
            "src/a/mod.rs",
            "pub struct Alpha;\npub struct Duplicate;\npub fn one(x: Alpha, y: Shared, z: Duplicate) -> Beta {}\n",
        ),
        ("src/a/helper.rs", "pub fn helper(x: Alpha) -> Alpha {}\n"),
        (
            "src/b.rs",
            "pub struct Beta;\npub struct Duplicate;\npub fn build(x: Alpha) -> Beta {}\n",
        ),
        ("src/shared.rs", "pub struct Shared;\n"),
        ("src/diagnostic.rs", "pub fn inspect(x: Alpha) {}\n"),
        ("architecture.json", PROFILE),
    ])
}
fn query(level: Level, view: View, focus: Option<&str>) -> Query {
    Query {
        level,
        view,
        focus: focus.map(str::to_owned),
    }
}
fn conserved(graph: &atlas::model::SourceGraph, d: &diagram::Diagram) {
    let before: BTreeSet<_> = graph.relationships.iter().map(|r| &r.id).collect();
    let after: Vec<_> = d
        .edges
        .iter()
        .flat_map(|e| &e.relationships)
        .chain(d.nodes.iter().flat_map(|n| &n.internal_relationships))
        .chain(d.coverage.hidden_relationships.values().flatten())
        .collect();
    assert_eq!(after.len(), before.len());
    assert_eq!(after.into_iter().collect::<BTreeSet<_>>(), before);
    d.validate()
        .expect("test fixture prerequisite must succeed");
}
#[test]
fn stages_are_bounded_and_dependency_arrows_are_declared_not_source_mentions() {
    let f = fixture();
    let profile = Architecture::parse(PROFILE, f.path()).unwrap();
    let graph = discovery::scan_workspace(f.path()).unwrap();
    let d = diagram::project(
        &graph,
        Some(&profile),
        &query(Level::Stages, View::Dependencies, None),
    )
    .unwrap();
    assert_eq!(d.nodes.len(), 2);
    assert_eq!(d.edges.len(), 1);
    assert_eq!(d.edges[0].evidence.label(), "reads");
    assert_eq!(d.edges[0].declarations, vec!["reads:a:b"]);
    assert!(d.edges[0].relationships.is_empty());
    assert_eq!(d.coverage.collapsed_files["vocabulary"].len(), 1);
    assert_eq!(d.coverage.collapsed_files["instrument"].len(), 1);
    assert_eq!(d.coverage.unassigned_files, vec!["src/lib.rs"]);
    conserved(&graph, &d);
}
#[test]
fn quotient_collapses_internal_edges_and_summarizes_ambiguity_without_question_nodes() {
    let f = fixture();
    let profile = Architecture::parse(PROFILE, f.path()).unwrap();
    let graph = discovery::scan_workspace(f.path()).unwrap();
    let d = diagram::project(
        &graph,
        Some(&profile),
        &query(Level::Stages, View::Types, None),
    )
    .unwrap();
    let coarse = atlas::render::mermaid(&d).unwrap();
    assert!(coarse.contains("|\"uses types\"|"));
    assert!(
        !coarse.contains("consumes")
            && !coarse.contains("produces")
            && !coarse.contains("composes")
    );
    assert_eq!(d.nodes.len(), 2);
    assert!(d.nodes.iter().all(|n| !n.label.contains('?')));
    assert!(
        d.nodes
            .iter()
            .map(|n| n.internal_relationships.len())
            .sum::<usize>()
            > 0
    );
    assert!(!d.coverage.hidden_relationships["ambiguous target summarized"].is_empty());
    conserved(&graph, &d);
    let detailed = diagram::project(
        &graph,
        Some(&profile),
        &query(Level::Declarations, View::Types, Some("a")),
    )
    .unwrap();
    assert!(
        detailed
            .nodes
            .iter()
            .any(|n| n.label.contains("2 candidates"))
    );
    conserved(&graph, &detailed);
}
#[test]
fn focused_children_preserve_only_selected_source_relationships_and_context() {
    let f = fixture();
    let profile = Architecture::parse(PROFILE, f.path()).unwrap();
    let graph = discovery::scan_workspace(f.path()).unwrap();
    let d = diagram::project(
        &graph,
        Some(&profile),
        &query(Level::Modules, View::Types, Some("a")),
    )
    .unwrap();
    assert!(d.nodes.iter().any(|n| n.label == "a::helper"));
    assert_eq!(
        d.nodes
            .iter()
            .find(|n| n.label == "a")
            .unwrap()
            .surface
            .as_ref()
            .unwrap()
            .entry,
        "one"
    );
    assert!(d.nodes.iter().any(|n| n.context && n.label == "b"));
    for id in d
        .edges
        .iter()
        .flat_map(|e| &e.relationships)
        .chain(d.nodes.iter().flat_map(|n| &n.internal_relationships))
    {
        let relation = graph.relationships.iter().find(|r| r.id == *id).unwrap();
        assert!(relation.source.starts_with("src/a/"));
    }
    assert!(
        diagram::project(
            &graph,
            Some(&profile),
            &query(Level::Modules, View::Dependencies, Some("a"))
        )
        .is_err()
    );
    conserved(&graph, &d);
    let stage = diagram::project(
        &graph,
        Some(&profile),
        &query(Level::Stages, View::Dependencies, Some("a")),
    )
    .unwrap();
    assert_eq!(stage.nodes.len(), 2);
    assert!(stage.nodes.iter().any(|n| n.label == "b" && n.context));
}
#[test]
fn profile_refuses_duplicates_overlaps_unknown_deps_and_missing_paths() {
    let f = fixture();
    let base: serde_json::Value = serde_json::from_str(PROFILE).unwrap();
    let mut cases = Vec::new();
    let mut changed = base.clone();
    changed["schema"] = 9.into();
    cases.push(changed);
    let mut changed = base.clone();
    changed["modules"][0]["dependencies"] = serde_json::json!(["missing"]);
    cases.push(changed);
    let mut changed = base.clone();
    changed["modules"][0]["path"] = "src/missing".into();
    cases.push(changed);
    let mut changed = base.clone();
    changed["modules"][1]["name"] = "a".into();
    cases.push(changed);
    let mut changed = base.clone();
    changed["modules"][1]["path"] = "src/a/helper.rs".into();
    cases.push(changed);
    let mut changed = base.clone();
    changed["workflows"][0]["path"] = "src/missing".into();
    cases.push(changed);
    let mut changed = base.clone();
    changed["workflows"][0]["function"] = "".into();
    cases.push(changed);
    let mut changed = base.clone();
    changed["workflows"]
        .as_array_mut()
        .unwrap()
        .push(base["workflows"][0].clone());
    cases.push(changed);
    for profile in cases {
        assert!(
            Architecture::parse(&profile.to_string(), f.path()).is_err(),
            "accepted {profile:?}"
        );
    }
    assert!(Architecture::parse("atlas-architecture-v1", f.path()).is_err());
}
#[test]
fn cli_paths_are_workspace_relative_with_spaces_and_docs_exclude_source_payloads() {
    let f = fixture();
    let binary = env!("CARGO_BIN_EXE_atlas");
    let run = || {
        Command::new(binary)
            .current_dir("/")
            .args([
                "map",
                f.path().to_str().unwrap(),
                "--architecture",
                "architecture.json",
                "--out",
                "docs/a map.txt",
                "--evidence-dir",
                "target/evidence with spaces",
            ])
            .output()
            .unwrap()
    };
    let first = run();
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let path = f.path().join("docs/a map.txt");
    let bytes = fs::read(&path).unwrap();
    assert!(!String::from_utf8_lossy(&bytes).contains(f.path().to_str().unwrap()));
    assert!(String::from_utf8_lossy(&bytes).contains("evidence%20with%20spaces"));
    assert!(
        f.path()
            .join("target/evidence with spaces/a map.source.json")
            .exists()
    );
    assert!(!f.path().join("docs/a map.source.json").exists());
    assert!(run().status.success());
    assert_eq!(bytes, fs::read(path).unwrap());
    let result = Command::new(binary)
        .args([
            "map",
            f.path().to_str().unwrap(),
            "--architecture",
            "architecture.json",
            "--backend",
            "pele",
        ])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let docs = f.path().join("target/atlas");
    let html = fs::read_to_string(docs.join("overview-stages-dependencies.html")).unwrap();
    assert!(!html.contains(f.path().to_str().unwrap()));
    assert!(html.contains("architecture profile"));
    assert!(html.contains("<a href=\"overview-stages-dependencies."));
    assert!(html.contains("<details><summary>"));
    assert!(html.contains("<code>src/a</code>"));
    assert!(html.contains("entry <code>one</code>"));
    assert!(docs.join("overview-stages-dependencies.mmd").exists());
    assert!(
        docs.join("overview-stages-dependencies.source.json")
            .exists()
    );
    assert!(!f.path().join("docs/atlas").exists());
}

#[test]
fn profile_refuses_non_normalized_spellings_and_symlink_boundaries() {
    let f = fixture();
    for path in ["src/a/./mod.rs", "src//a/mod.rs", "src/a/", "./src/a"] {
        let mut value: serde_json::Value = serde_json::from_str(PROFILE).unwrap();
        value["modules"][0]["path"] = path.into();
        let text = value.to_string();
        assert!(
            Architecture::parse(&text, f.path()).is_err(),
            "accepted {path}"
        );
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(f.path().join("src/a"), f.path().join("src/alias")).unwrap();
        for path in ["src/alias", "src/alias/mod.rs"] {
            let mut value: serde_json::Value = serde_json::from_str(PROFILE).unwrap();
            value["modules"][0]["path"] = path.into();
            let text = value.to_string();
            assert!(
                Architecture::parse(&text, f.path()).is_err(),
                "accepted symlink {path}"
            );
        }
    }
}
#[test]
fn html_links_refuse_active_or_absolute_urls_and_escape_local_evidence() {
    use atlas::render::{Pele, Renderer};
    let f = fixture();
    let profile = Architecture::parse(PROFILE, f.path()).unwrap();
    let graph = discovery::scan_workspace(f.path()).unwrap();
    let mut d = diagram::project(
        &graph,
        Some(&profile),
        &query(Level::Stages, View::Dependencies, None),
    )
    .unwrap();
    for link in [
        "javascript:alert(1)",
        "https://example.com",
        "//example.com",
        "/absolute/file",
        "\\example.com",
        " javascript:alert(1)",
    ] {
        d.evidence_links = vec![("test".into(), link.into())];
        assert!(Pele::default().render(&d).is_err());
    }
    d.evidence_links = vec![("a <& label".into(), "../target/a&b.json".into())];
    let html = String::from_utf8(Pele::default().render(&d).unwrap().primary.bytes).unwrap();
    assert!(html.contains("href=\"../target/a&amp;b.json\""));
    assert!(html.contains("a &lt;&amp; label"));
}

#[test]
fn html_primary_cannot_overwrite_its_svg_companion() {
    let f = fixture();
    let result = Command::new(env!("CARGO_BIN_EXE_atlas"))
        .args([
            "map",
            f.path().to_str().unwrap(),
            "--backend",
            "pele",
            "--out",
            "docs/view.svg",
        ])
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("collides"));
    assert!(!f.path().join("docs/view.svg").exists());
}
