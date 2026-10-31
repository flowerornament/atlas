use atlas_core::{discovery, review};
use cargo_metadata::MetadataCommand;
mod support;
use std::fs;
use support::Fixture;
fn fixture() -> Fixture {
    Fixture::new(&[
        (
            "Cargo.toml",
            "[workspace]\nmembers=[\"xtask\"]\nresolver=\"3\"\n",
        ),
        (
            "xtask/Cargo.toml",
            "[package]\nname=\"snapshot-fixture\"\nversion=\"0.0.0\"\nedition=\"2024\"\n[lib]\npath=\"../scripts/model.rs\"\n",
        ),
        (
            "scripts/model.rs",
            "/// Published type.\npub struct Token;\n",
        ),
    ])
}

#[test]
fn snapshot_normalizes_external_target_mount_and_keeps_original_outline_bytes() {
    let f = fixture();
    let metadata = MetadataCommand::new()
        .current_dir(f.path())
        .no_deps()
        .exec()
        .unwrap();
    let snapshot = discovery::snapshot_metadata(&metadata).unwrap();
    assert_eq!(snapshot.graph().files.len(), 1);
    assert_eq!(snapshot.graph().files[0].path, "scripts/model.rs");
    assert!(!snapshot.graph().files[0].mounts.is_empty());
    fs::write(f.path().join("scripts/model.rs"), "pub struct Replaced;\n").unwrap();
    let report = review::outline(&snapshot, "scripts").unwrap();
    assert_eq!(report.files[0].items[0].name, "Token");
    assert_eq!(report.files[0].items[0].first, 1);
    assert_eq!(report.files[0].items[0].last, 2);
    assert_eq!(report.files[0].non_test_lines, 2);
    assert_eq!(report.files[0].top_level_items, 1);
    assert_eq!(report.files[0].largest_block.as_ref().unwrap().lines, 1);
    assert!(report.files[0].uncovered_code.is_empty());
    assert_eq!(
        report.files[0].fingerprint,
        snapshot.graph().files[0].content_fingerprint
    );
    assert_eq!(review::outline(&snapshot, ".").unwrap().files.len(), 1);
    assert_eq!(
        review::outline(&snapshot, "scripts/").unwrap().scope,
        "scripts"
    );
    assert!(review::outline(&snapshot, "absent").is_err());
}

#[test]
fn captured_facts_keep_item_gates_distinct_from_native_line_population() {
    let fixture = fixture();
    fs::write(
        fixture.path().join("scripts/model.rs"),
        "#[cfg(test)]\nfn helper() {}\n\npub fn live() {}\n",
    )
    .expect("write gated fixture");
    let metadata = MetadataCommand::new()
        .current_dir(fixture.path())
        .no_deps()
        .exec()
        .expect("fixture metadata");
    let snapshot = discovery::snapshot_metadata(&metadata).expect("capture fixture");
    let source = snapshot
        .source_facts("scripts/model.rs")
        .expect("source facts");
    assert!(source.individually_test_gated(2));
    assert!(!source.test_regions().covers(2));
    assert_eq!(source.test_regions().non_test_loc(), 4);
    assert_eq!(source.outline()[0].name, "helper");
    assert!(
        snapshot.graph().files[0]
            .declarations
            .iter()
            .find(|item| item.name == "helper")
            .expect("helper declaration")
            .test_only
    );
    fs::write(
        fixture.path().join("scripts/model.rs"),
        "pub struct Later;\n",
    )
    .expect("replace live file");
    assert_eq!(source.projection().line_comment_lines().count(), 0);
    assert_eq!(source.outline()[0].name, "helper");
}

fn capture(fixture: &Fixture) -> discovery::Snapshot {
    let metadata = MetadataCommand::new()
        .current_dir(fixture.path())
        .no_deps()
        .exec()
        .expect("fixture metadata");
    discovery::snapshot_metadata(&metadata).expect("capture fixture")
}

#[test]
fn shape_diff_ignores_test_body_edits_and_reports_production_depth() {
    let fixture = fixture();
    let path = fixture.path().join("scripts/model.rs");
    fs::write(
        &path,
        "pub fn live() {}\n#[cfg(test)]\nmod tests {\n    fn old() {}\n}\n",
    )
    .unwrap();
    let before = capture(&fixture);
    fs::write(
        &path,
        "pub fn live() {}\n#[cfg(test)]\nmod tests {\n    fn changed() { let value = 1; }\n}\n",
    )
    .unwrap();
    let only_tests_changed = capture(&fixture);
    assert!(
        review::diff(&before, &only_tests_changed, ".")
            .unwrap()
            .changes
            .is_empty()
    );
    fs::write(&path, "pub fn live() {\n    let value = 1;\n}\n#[cfg(test)]\nmod tests {\n    fn changed() {}\n}\n").unwrap();
    let after = capture(&fixture);
    let diff = review::diff(&before, &after, "scripts/").unwrap();
    assert_eq!(diff.changes.len(), 1);
    let change = &diff.changes[0];
    assert_eq!(change.before.as_ref().unwrap().non_test_lines, 1);
    assert_eq!(change.after.as_ref().unwrap().non_test_lines, 3);
    assert_eq!(
        change
            .after
            .as_ref()
            .unwrap()
            .largest_block
            .as_ref()
            .unwrap()
            .lines,
        3
    );
}

#[test]
fn shape_diff_can_select_a_scope_removed_from_the_after_tree() {
    let fixture = fixture();
    let before = capture(&fixture);
    fs::write(
        fixture.path().join("scripts/after.rs"),
        "pub struct After;\n",
    )
    .unwrap();
    fs::write(fixture.path().join("xtask/Cargo.toml"), "[package]\nname=\"snapshot-fixture\"\nversion=\"0.0.0\"\nedition=\"2024\"\n[lib]\npath=\"../scripts/after.rs\"\n").unwrap();
    let after = capture(&fixture);
    let diff = review::diff(&before, &after, "scripts/model.rs").unwrap();
    assert_eq!(diff.changes.len(), 1);
    assert!(diff.changes[0].before.is_some());
    assert!(diff.changes[0].after.is_none());
    assert!(review::diff(&before, &after, "missing").is_err());
}

#[test]
fn documentation_uses_supplied_citations_and_explicit_test_population() {
    use atlas_core::{
        documentation::{self, Finding, HeaderPopulation, HeaderScope, Policy},
        population::{Population, TestSelection, VisibilitySelection},
        source::ItemKind,
    };
    let fixture = fixture();
    fs::write(fixture.path().join("scripts/model.rs"), "//! CR-D1\n/// Public contract.\npub struct Documented;\npub struct Missing;\n#[cfg(test)]\npub fn gated() {}\n").unwrap();
    let snapshot = capture(&fixture);
    let kinds = [ItemKind::Struct, ItemKind::Fn];
    let markers = ["CR-"];
    let mut policy = Policy {
        items: Population {
            visibility: VisibilitySelection::Public,
            tests: TestSelection::OutsideTestModules,
        },
        headers: HeaderPopulation::AllFiles,
        header_scope: HeaderScope::File,
        item_kinds: &kinds,
        citation_markers: Some(&markers),
    };
    let native = documentation::audit(&snapshot, "scripts", &policy).unwrap();
    assert_eq!(native.checked_items, 3);
    assert_eq!(native.findings.len(), 2);
    assert!(
        !native
            .findings
            .iter()
            .any(|finding| matches!(finding, Finding::UncitedHeader { .. }))
    );
    policy.items.tests = TestSelection::Production;
    let production = documentation::audit(&snapshot, ".", &policy).unwrap();
    assert_eq!(production.checked_items, 2);
    assert_eq!(production.findings.len(), 1);
    let other_markers = ["FM-"];
    policy.citation_markers = Some(&other_markers);
    assert!(
        documentation::audit(&snapshot, ".", &policy)
            .unwrap()
            .findings
            .iter()
            .any(|finding| matches!(finding, Finding::UncitedHeader { .. }))
    );
    policy.citation_markers = None;
    assert!(
        documentation::audit(&snapshot, ".", &policy)
            .unwrap()
            .citation_markers
            .is_none()
    );
    let empty = [""];
    policy.citation_markers = Some(&empty);
    assert!(documentation::audit(&snapshot, ".", &policy).is_err());
}

#[test]
fn shape_diff_refuses_a_scope_that_became_ambiguous() {
    let fixture = fixture();
    fs::create_dir_all(fixture.path().join("pkg-a/src")).unwrap();
    fs::write(
        fixture.path().join("pkg-a/Cargo.toml"),
        "[package]\nname=\"pkg-a\"\nversion=\"0.0.0\"\nedition=\"2024\"\n",
    )
    .unwrap();
    fs::write(
        fixture.path().join("pkg-a/src/lib.rs"),
        "pub struct Before;\n",
    )
    .unwrap();
    fs::write(
        fixture.path().join("Cargo.toml"),
        "[workspace]\nmembers=[\"pkg-a\"]\nresolver=\"3\"\n",
    )
    .unwrap();
    let before = capture(&fixture);
    fs::create_dir_all(fixture.path().join("pkg-b/src")).unwrap();
    fs::write(
        fixture.path().join("pkg-b/Cargo.toml"),
        "[package]\nname=\"pkg-b\"\nversion=\"0.0.0\"\nedition=\"2024\"\n",
    )
    .unwrap();
    fs::write(
        fixture.path().join("pkg-b/src/lib.rs"),
        "pub struct Added;\n",
    )
    .unwrap();
    fs::write(
        fixture.path().join("Cargo.toml"),
        "[workspace]\nmembers=[\"pkg-a\",\"pkg-b\"]\nresolver=\"3\"\n",
    )
    .unwrap();
    let after = capture(&fixture);
    let error = review::diff(&before, &after, "src").unwrap_err();
    assert!(matches!(
        error.downcast_ref::<discovery::ScopeError>(),
        Some(discovery::ScopeError::Ambiguous { .. })
    ));
}

#[test]
fn documentation_distinguishes_nested_coverage_and_rustdoc_attribute_limits() {
    use atlas_core::{
        documentation::{self, Finding, HeaderPopulation, HeaderScope, Policy},
        population::{Population, TestSelection, VisibilitySelection},
        source::ItemKind,
    };
    let fixture = fixture();
    fs::write(fixture.path().join("scripts/model.rs"), "pub mod inner {\n    //! CR-D1\n}\n#[doc = \"Attribute documentation\"]\npub struct AttributeDoc;\n").unwrap();
    let snapshot = capture(&fixture);
    let kinds = [ItemKind::Struct];
    let markers = ["CR-"];
    let mut policy = Policy {
        items: Population {
            visibility: VisibilitySelection::Public,
            tests: TestSelection::Production,
        },
        headers: HeaderPopulation::AllFiles,
        header_scope: HeaderScope::File,
        item_kinds: &kinds,
        citation_markers: Some(&markers),
    };
    let file_scope = documentation::audit(&snapshot, ".", &policy).unwrap();
    assert!(
        file_scope
            .findings
            .iter()
            .any(|finding| matches!(finding, Finding::MissingHeader { .. }))
    );
    assert!(file_scope.findings.iter().any(|finding| matches!(finding, Finding::MissingOuterLineDoc { name, .. } if name == "AttributeDoc")));
    assert!(
        file_scope
            .limits
            .iter()
            .any(|limit| limit.contains("doc attributes"))
    );
    policy.header_scope = HeaderScope::AnyModule;
    let native_coverage = documentation::audit(&snapshot, ".", &policy).unwrap();
    assert!(!native_coverage.findings.iter().any(|finding| matches!(
        finding,
        Finding::MissingHeader { .. } | Finding::UncitedHeader { .. }
    )));
}

#[test]
fn qualified_capture_keeps_bytes_and_cargo_context_after_external_changes() {
    use atlas_core::analysis::Analysis;
    let fixture = fixture();
    fs::write(fixture.path().join("scripts/model.rs"), "pub struct Token;\npub fn make() -> Token { Token }\npub fn caller() -> Token { make() }\n").expect("qualified source");
    let mut metadata = MetadataCommand::new()
        .current_dir(fixture.path())
        .no_deps()
        .exec()
        .expect("Cargo context");
    let snapshot = discovery::snapshot_metadata(&metadata).expect("capture source and context");
    metadata.workspace_members.clear();
    fs::remove_file(fixture.path().join("scripts/model.rs"))
        .expect("remove disk source after capture");
    let analysis = Analysis::new(&snapshot).expect("parse captured bytes");
    let facts = analysis
        .representations()
        .expect("qualified captured observations");
    assert!(facts.types.iter().any(|ty| ty.name == "Token"));
    let make = facts
        .functions
        .iter()
        .find(|f| f.name == "make")
        .expect("captured make");
    let caller = facts
        .functions
        .iter()
        .find(|f| f.name == "caller")
        .expect("captured caller");
    assert!(
        facts
            .calls
            .iter()
            .any(|call| call.from == caller.id && call.to == make.id)
    );
    let (flow, diagram) = analysis
        .expanded("scripts/model.rs", "caller", 1)
        .expect("expand captured external target");
    assert_eq!(flow.function, "caller");
    assert!(diagram.edges.iter().any(|edge| edge.evidence
        == atlas_core::diagram::EdgeEvidence::SourceOrder
        && edge.label.as_deref() == Some("call")));
    assert!(diagram.edges.iter().any(|edge| {
        edge.label
            .as_deref()
            .is_some_and(|label| label.starts_with("return to"))
    }));
    diagram.validate().expect("captured source-control graph");
    assert!(!snapshot.metadata().workspace_members.is_empty());
}
