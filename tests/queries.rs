//! Query facts come from the same source population and keep uncertainty visible.
mod support;
use atlas::{discovery, queries};
use support::Fixture;
#[test]
fn census_and_module_queries_share_the_source_population() {
    let f = Fixture::new(&[
        ("src/lib.rs", "pub mod a;\npub mod b;\n"),
        ("src/a.rs", "pub struct Value;\npub fn build(x: Value) {}\n"),
        ("src/b.rs", "pub struct Other;\n"),
        ("src/empty.rs", ""),
    ]);
    let graph = discovery::scan_workspace(f.path()).unwrap();
    let census = queries::census(&graph, None).unwrap();
    assert_eq!(census.files, 4);
    assert_eq!(census.kinds["Struct"], 2);
    let module = queries::module(&graph, "a").unwrap();
    assert_eq!(module.files.len(), 1);
    assert_eq!(module.files[0].path, "src/a.rs");
    assert_eq!(module.relationships.len(), 1);
    assert_eq!(queries::census(&graph, Some("a")).unwrap().declarations, 2);
    assert!(queries::module(&graph, "missing").is_err());
}
#[test]
fn name_and_related_keep_ambiguous_matches_and_candidate_references() {
    let f = Fixture::new(&[
        (
            "src/lib.rs",
            "pub mod a;\npub mod b;\npub fn build(x: Shared) {}\n",
        ),
        ("src/a.rs", "pub struct Shared;\n"),
        ("src/b.rs", "pub struct Shared;\n"),
    ]);
    let graph = discovery::scan_workspace(f.path()).unwrap();
    let names = queries::name(&graph, "Shared").unwrap();
    assert!(names.ambiguous);
    assert_eq!(names.matches.len(), 2);
    for card in &names.matches {
        assert!(card.incoming.is_empty());
        assert_eq!(card.possible_incoming.len(), 1);
    }
    let json = serde_json::to_value(&names).unwrap();
    assert_eq!(
        json["matches"][0]["possible_incoming"][0]["target"]["status"],
        "ambiguous"
    );
    let id = &names.matches[0].declaration.id;
    let one = queries::name(&graph, id).unwrap();
    assert!(!one.ambiguous);
    assert_eq!(one.matches.len(), 1);
    let build = queries::name(&graph, "build").unwrap();
    assert_eq!(build.matches[0].outgoing.len(), 1);
    assert!(build.text(true).contains("parameter") || build.text(true).contains("consumes"));
    assert!(queries::name(&graph, "missing").is_err());
}
