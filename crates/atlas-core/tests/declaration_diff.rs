//! Donor declaration-review claims at the public immutable-snapshot boundary.
use atlas_core::{
    discovery::{Snapshot, snapshot_workspace},
    model::Evidence,
    population::VisibilitySelection,
    queries::navigation::{ChangedDeclaration, Navigation, Side},
};
use std::fs;

fn captures(before: &str, after: &str) -> (Snapshot, Snapshot) {
    let root = tempfile::tempdir().expect("fixture");
    fs::create_dir(root.path().join("src")).expect("source");
    fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname='fixture'\nversion='0.0.0'\nedition='2024'\n[workspace]\n",
    )
    .expect("manifest");
    let path = root.path().join("src/lib.rs");
    fs::write(&path, before).expect("before");
    let before = snapshot_workspace(root.path()).expect("capture before");
    fs::write(&path, after).expect("after");
    let after = snapshot_workspace(root.path()).expect("capture after");
    (before, after)
}
const BEFORE: &str = "pub struct Cost;\npub struct Tile { cost: Cost }\npub fn admit(cost: Cost) -> Tile { Tile { cost } }\nfn unchanged() {}\n";
const AFTER: &str = "\npub struct Cost;\npub struct Candidate { cost: Cost }\npub fn admit(cost: Cost) -> Candidate { Candidate { cost } }\nfn unchanged() {}\n";
#[test]
fn line_motion_is_not_a_change_but_renames_and_signatures_are() {
    let (before, after) = captures(BEFORE, AFTER);
    let prior = Navigation::new(&before, VisibilitySelection::All).expect("prior");
    let current = Navigation::new(&after, VisibilitySelection::All).expect("current");
    let report = current
        .declaration_diff(&prior, ".", false)
        .expect("review");
    let summary = report
        .declarations
        .iter()
        .map(|change| match change {
            ChangedDeclaration::Added { after } => ("added", after.site.name.as_str()),
            ChangedDeclaration::Removed { before } => ("removed", before.site.name.as_str()),
            ChangedDeclaration::Modified { after, .. } => ("modified", after.site.name.as_str()),
        })
        .collect::<Vec<_>>();
    assert_eq!(
        summary,
        [
            ("removed", "Tile"),
            ("added", "Candidate"),
            ("modified", "admit")
        ]
    );
    assert!(report.relationships.is_none());
    let admit = report
        .declarations
        .iter()
        .find_map(|change| match change {
            ChangedDeclaration::Modified { before, after } if after.site.name == "admit" => {
                Some((before, after))
            }
            _ => None,
        })
        .expect("modified admit");
    assert_eq!(
        admit.0.declaration_block.as_ref(),
        "pub fn admit(cost: Cost) -> Tile { Tile { cost } }"
    );
    assert_eq!(
        admit.1.declaration_block.as_ref(),
        "pub fn admit(cost: Cost) -> Candidate { Candidate { cost } }"
    );
}
#[test]
fn incident_edges_connect_current_vocabulary_and_retired_names() {
    let (before, after) = captures(BEFORE, AFTER);
    let prior = Navigation::new(&before, VisibilitySelection::All).expect("prior");
    let current = Navigation::new(&after, VisibilitySelection::All).expect("current");
    let report = current.declaration_diff(&prior, ".", true).expect("review");
    let edges = report.relationships.expect("requested relationships");
    assert!(edges.iter().any(|edge| edge.side == Side::Prior
        && edge.edge.source.name == "admit"
        && edge.edge.target == "Tile"
        && edge.edge.evidence == Evidence::ReturnMention));
    assert!(edges.iter().any(|edge| edge.side == Side::Current
        && edge.edge.source.name == "admit"
        && edge.edge.target == "Candidate"
        && edge.edge.evidence == Evidence::ReturnMention));
    assert!(edges.iter().any(|edge| edge.side == Side::Current
        && edge.edge.source.name == "Candidate"
        && edge.edge.target == "Cost"
        && edge.edge.evidence == Evidence::TypeBodyMention));
}
#[test]
fn observed_owner_changes_cannot_cancel_identical_nested_declarations() {
    let (before, after) = captures(
        "pub mod a {\n pub fn same() {}\n}\n",
        "pub mod b {\n pub fn same() {}\n}\n",
    );
    let prior = Navigation::new(&before, VisibilitySelection::All).expect("prior");
    let current = Navigation::new(&after, VisibilitySelection::All).expect("current");
    let report = current
        .declaration_diff(&prior, ".", false)
        .expect("review");
    assert_eq!(report.declarations.len(), 2);
    assert!(report.declarations.iter().any(|change| matches!(change, ChangedDeclaration::Removed { before } if before.owner.contains("Mod:a"))));
    assert!(report.declarations.iter().any(|change| matches!(change, ChangedDeclaration::Added { after } if after.owner.contains("Mod:b"))));
}
#[test]
fn test_scope_visibility_and_documentation_keep_native_boundaries() {
    let source = "#[cfg(test)]\npub fn helper() {}\n#[cfg(test)]\nmod tests {\n pub fn ignored() {}\n}\npub(crate) fn internal() {}\n";
    let changed = source
        .replace("helper()", "helper(x: u32)")
        .replace("ignored()", "ignored(x: u32)")
        .replace("internal()", "internal(x: u32)");
    let (before, after) = captures(source, &changed);
    let prior = Navigation::new(&before, VisibilitySelection::Public).expect("prior");
    let current = Navigation::new(&after, VisibilitySelection::Public).expect("current");
    let report = current
        .declaration_diff(&prior, ".", false)
        .expect("review");
    assert_eq!(report.declarations.len(), 1);
    assert_eq!(
        report.declarations[0]
            .after()
            .expect("current declaration")
            .site
            .name,
        "helper"
    );
}
#[test]
fn item_review_does_not_need_unrelated_import_resolution() {
    let (before, after) = captures(
        "use crate::{Broken as};\npub struct Old;\n",
        "use crate::{Broken as};\npub struct New;\n",
    );
    let prior = Navigation::new(&before, VisibilitySelection::All).expect("prior");
    let current = Navigation::new(&after, VisibilitySelection::All).expect("current");
    assert_eq!(
        current
            .declaration_diff(&prior, ".", false)
            .expect("declarations")
            .declarations
            .len(),
        2
    );
    assert!(current.declaration_diff(&prior, ".", true).is_err());
}

#[test]
fn changing_a_later_doc_line_or_attached_attribute_is_a_modification() {
    for (before, after) in [
        (
            "/// first\n/// old detail\npub struct Item;\n",
            "/// first\n/// new detail\npub struct Item;\n",
        ),
        (
            "#[cfg(feature = \"one\")]\npub struct Item;\n",
            "#[cfg(feature = \"two\")]\npub struct Item;\n",
        ),
    ] {
        let (before, after) = captures(before, after);
        let prior = Navigation::new(&before, VisibilitySelection::All).expect("prior");
        let current = Navigation::new(&after, VisibilitySelection::All).expect("current");
        let report = current
            .declaration_diff(&prior, ".", false)
            .expect("review");
        assert_eq!(report.declarations.len(), 1);
        assert!(matches!(
            report.declarations[0],
            ChangedDeclaration::Modified { .. }
        ));
    }
}
#[test]
fn mismatched_subject_populations_cannot_become_false_source_changes() {
    let (before, after) = captures("fn helper() {}\n", "fn helper() {}\n");
    let prior = Navigation::new(&before, VisibilitySelection::Public).expect("prior");
    let current = Navigation::new(&after, VisibilitySelection::All).expect("current");
    assert!(current.declaration_diff(&prior, ".", false).is_err());
}

#[test]
fn captured_declaration_boundaries_are_explicit_for_inline_and_multiline_bodies() {
    for (before, after, changes) in [
        ("pub fn f() { old(); }\n", "pub fn f() { new(); }\n", 1),
        (
            "pub fn f() {\n old();\n}\n",
            "pub fn f() {\n new();\n}\n",
            0,
        ),
    ] {
        let (before, after) = captures(before, after);
        let prior = Navigation::new(&before, VisibilitySelection::All).expect("prior");
        let current = Navigation::new(&after, VisibilitySelection::All).expect("current");
        assert_eq!(
            current
                .declaration_diff(&prior, ".", false)
                .expect("review")
                .declarations
                .len(),
            changes
        );
    }
}
#[test]
fn attached_attribute_payload_blank_lines_are_not_layout_to_discard() {
    let (before, after) = captures(
        "#[doc = r#\"first\n\nlast\"#]\npub struct Item;\n",
        "#[doc = r#\"first\nlast\"#]\npub struct Item;\n",
    );
    let prior = Navigation::new(&before, VisibilitySelection::All).expect("prior");
    let current = Navigation::new(&after, VisibilitySelection::All).expect("current");
    assert_eq!(
        current
            .declaration_diff(&prior, ".", false)
            .expect("review")
            .declarations
            .len(),
        1
    );
}
