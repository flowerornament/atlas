//! Native navigation claims exercised through real immutable Cargo snapshots.
use atlas_core::{
    discovery::snapshot_workspace,
    model::Evidence,
    population::VisibilitySelection,
    queries::navigation::{Census, Navigation, Tally},
};
use std::fs;

const ADMIT: &str = "//! Admission for fixture tiles — realization kernel §5.\n\nuse crate::grade::Cost;\nuse std::fmt;\n\n/// An admitted fixture tile.\npub struct Tile {\n    cost: Cost,\n}\n\n/// Refusal for the tile family.\npub(crate) enum Refusal {\n    CrossLane,\n}\n\npub fn admit(cost: Cost) -> Tile {\n    Tile { cost }\n}\n\nfn private_helper() {}\n\n#[cfg(test)]\nmod tests {\n    use super::*;\n}\n";
const GRADE: &str = "pub struct Cost;\n\npub use self::Cost as PublicCost;\n\nmacro_rules! grade_table {\n    () => {};\n}\n";
const LIB: &str =
    "//! Fixture crate root.\n\npub mod admit;\npub mod grade;\n\npub use crate::admit::Tile;\n";
fn fixture(extra: &str) -> (tempfile::TempDir, atlas_core::discovery::Snapshot) {
    let root = tempfile::tempdir().expect("navigation fixture prerequisite must succeed");
    let dir = root.path().join("crates/murail-fix/src");
    fs::create_dir_all(&dir).expect("navigation fixture prerequisite must succeed");
    fs::write(
        root.path().join("Cargo.toml"),
        "[workspace]\nmembers=['crates/murail-fix']\nresolver='3'\n",
    )
    .expect("navigation fixture prerequisite must succeed");
    fs::write(
        dir.parent()
            .expect("navigation fixture prerequisite must succeed")
            .join("Cargo.toml"),
        "[package]\nname='murail-fix'\nversion='0.0.0'\nedition='2024'\n",
    )
    .expect("navigation fixture prerequisite must succeed");
    for (name, text) in [("lib.rs", LIB), ("admit.rs", ADMIT), ("grade.rs", GRADE)] {
        fs::write(dir.join(name), text).expect("navigation fixture prerequisite must succeed");
    }
    if !extra.is_empty() {
        fs::write(dir.join("grade.rs"), format!("{GRADE}{extra}"))
            .expect("navigation fixture prerequisite must succeed");
    }
    let snapshot =
        snapshot_workspace(root.path()).expect("navigation fixture prerequisite must succeed");
    (root, snapshot)
}
#[test]
fn census_workspace_rung_is_the_per_crate_golden() {
    let (_root, snapshot) = fixture("");
    let nav = Navigation::new(&snapshot, VisibilitySelection::RestrictedOrPublic)
        .expect("navigation fixture prerequisite must succeed");
    let Census::Rows(report) = nav
        .census(None)
        .expect("navigation fixture prerequisite must succeed")
    else {
        panic!("rows")
    };
    assert_eq!(report.rows.len(), 1);
    assert_eq!(report.rows[0].key, "murail-fix");
    assert_eq!(
        report.total,
        Tally {
            types: 3,
            fns: 1,
            aliases: 2,
            macros: 1,
            documented: 2,
            token_lines: 20,
            nontest_loc: 34
        }
    );
    assert_eq!(report.total.doc_pct(), Some(50));
}
#[test]
fn census_crate_rung_lists_modules_and_pub_only_narrows() {
    let (_root, snapshot) = fixture("");
    for (visibility, types) in [
        (VisibilitySelection::RestrictedOrPublic, 2),
        (VisibilitySelection::Public, 1),
    ] {
        let nav = Navigation::new(&snapshot, visibility)
            .expect("navigation fixture prerequisite must succeed");
        let Census::Rows(report) = nav
            .census(Some("murail-fix"))
            .expect("navigation fixture prerequisite must succeed")
        else {
            panic!("rows")
        };
        assert_eq!(
            report
                .rows
                .iter()
                .map(|row| row.key.as_str())
                .collect::<Vec<_>>(),
            ["admit.rs", "grade.rs", "lib.rs"]
        );
        assert_eq!(report.rows[0].tally.types, types);
        assert_eq!(report.rows[0].tally.fns, 1);
        assert_eq!(
            report
                .rows
                .iter()
                .map(|row| row.tally.token_lines)
                .sum::<usize>(),
            report.total.token_lines
        );
        assert_eq!(report.total.token_lines, 20);
        assert_eq!(report.total.macros, 1);
    }
}
#[test]
fn name_card_shows_binding_facade_edges_and_doc_gap() {
    let (_root, snapshot) = fixture("");
    let nav = Navigation::new(&snapshot, VisibilitySelection::RestrictedOrPublic)
        .expect("navigation fixture prerequisite must succeed");
    let tile = nav
        .name("Tile")
        .expect("navigation fixture prerequisite must succeed");
    assert_eq!(tile.matches.len(), 1);
    assert_eq!(tile.matches[0].line, 7);
    assert_eq!(tile.reexports.len(), 1);
    assert_eq!(tile.refs_approx, 3);
    assert!(tile.matches[0].doc.is_some());
    assert!(tile.edges.iter().any(|edge| edge.source.name == "admit"
        && edge.target == "Tile"
        && edge.evidence == Evidence::ReturnMention));
    let cost = nav
        .name("grade::Cost")
        .expect("navigation fixture prerequisite must succeed");
    assert_eq!(cost.matches[0].file, "crates/murail-fix/src/grade.rs");
    assert!(
        cost.edges
            .iter()
            .any(|edge| edge.source.name == "Tile" && edge.evidence == Evidence::TypeBodyMention)
    );
    assert!(
        nav.name("admit")
            .expect("navigation fixture prerequisite must succeed")
            .matches[0]
            .doc
            .is_none()
    );
}
#[test]
fn module_card_shows_header_items_orphans_and_graph() {
    let (_root, snapshot) = fixture("");
    let nav = Navigation::new(&snapshot, VisibilitySelection::RestrictedOrPublic)
        .expect("navigation fixture prerequisite must succeed");
    let module = nav
        .module("crates/murail-fix/src/admit.rs")
        .expect("navigation fixture prerequisite must succeed");
    let context = &module.contexts[0];
    assert_eq!(context.tally.nontest_loc, 21);
    assert_eq!(context.undocumented, 1);
    assert_eq!((context.imports, context.internal_imports), (2, 1));
    assert_eq!(
        context
            .fan_in_files
            .as_ref()
            .expect("navigation fixture prerequisite must succeed"),
        &["crates/murail-fix/src/lib.rs"]
    );
    assert_eq!(context.unmentioned_names, ["Refusal"]);
    assert_eq!(context.edges.len(), 3);
    assert_eq!(context.items.len(), 4);
    assert_eq!(
        context.header[0],
        "Admission for fixture tiles — realization kernel §5."
    );
    let Census::Module(_) = nav
        .census(Some("crates/murail-fix/src/admit.rs"))
        .expect("navigation fixture prerequisite must succeed")
    else {
        panic!("module rung")
    };
}
#[test]
fn native_subject_visibility_does_not_drop_private_or_gated_source_bodies() {
    let (_root, snapshot) = fixture(
        "#[cfg(test)]\nfn helper(cost: Cost) -> Cost { cost }\n#[cfg(test)]\nmod tests {\n fn hidden(cost: Cost) -> Cost { cost }\n}\n",
    );
    let nav = Navigation::new(&snapshot, VisibilitySelection::Public)
        .expect("navigation fixture prerequisite must succeed");
    let name = nav
        .name("Cost")
        .expect("navigation fixture prerequisite must succeed");
    assert!(name.edges.iter().any(|edge| edge.source.name == "helper"));
    assert!(!name.edges.iter().any(|edge| edge.source.name == "hidden"));
    let helper = nav
        .name("helper")
        .expect("navigation fixture prerequisite must succeed");
    assert!(helper.matches.is_empty());
    assert_eq!(helper.hidden, 1);
}
#[test]
fn navigation_never_reopens_captured_source() {
    let (root, snapshot) = fixture("");
    root.close()
        .expect("navigation fixture prerequisite must succeed");
    let nav = Navigation::new(&snapshot, VisibilitySelection::Public)
        .expect("navigation fixture prerequisite must succeed");
    assert_eq!(
        nav.name("Tile")
            .expect("navigation fixture prerequisite must succeed")
            .matches
            .len(),
        1
    );
    assert!(
        nav.module("crates/murail-fix/src/grade.rs")
            .expect("navigation fixture prerequisite must succeed")
            .contexts[0]
            .header
            .is_empty()
    );
    assert!(nav.census(Some("absent")).is_err());
    assert!(!nav.dump().expect("captured dump").exports.is_empty());
}

#[test]
fn external_import_shadowing_remains_explicitly_unresolved() {
    for import in ["use ext::Cost;", "use ext::*;"] {
        let (root, _) = fixture("");
        fs::write(
            root.path().join("crates/murail-fix/src/foreign.rs"),
            format!("{import}\npub fn foreign() -> Cost {{ loop {{}} }}\n"),
        )
        .expect("source");
        let snapshot = snapshot_workspace(root.path()).expect("snapshot");
        let nav = Navigation::new(&snapshot, VisibilitySelection::Public).expect("navigation");
        let report = nav.name("Cost").expect("card");
        let edge = report
            .edges
            .iter()
            .find(|edge| edge.source.name == "foreign")
            .expect("explicit unknown");
        assert!(matches!(
            edge.resolution,
            atlas_core::model::Resolution::Unresolved(_)
        ));
        assert!(edge.candidates.is_empty());
    }
}
#[test]
fn shared_physical_modules_keep_every_package_and_its_own_edges() {
    let root = tempfile::tempdir().expect("fixture");
    fs::create_dir_all(root.path().join("src/inner/src")).expect("directories");
    fs::write(root.path().join("Cargo.toml"), "[package]\nname='outer'\nversion='0.0.0'\nedition='2024'\n[workspace]\nmembers=['src/inner']\n").expect("manifest");
    fs::write(root.path().join("src/lib.rs"), "pub struct Outer;\n").expect("root source");
    fs::write(
        root.path().join("src/inner/Cargo.toml"),
        "[package]\nname='inner'\nversion='0.0.0'\nedition='2024'\n",
    )
    .expect("inner manifest");
    fs::write(
        root.path().join("src/inner/src/lib.rs"),
        "pub struct Shared;\npub fn make() -> Shared { Shared }\n",
    )
    .expect("shared source");
    let snapshot = snapshot_workspace(root.path()).expect("snapshot");
    let nav = Navigation::new(&snapshot, VisibilitySelection::Public).expect("navigation");
    let report = nav.module("src/inner/src/lib.rs").expect("card");
    assert_eq!(report.contexts.len(), 2);
    for context in report.contexts {
        assert_eq!(context.edges.len(), 1);
        assert_eq!(context.edges[0].source.package, context.package);
        assert_eq!(context.edges[0].candidates[0].package, context.package);
    }
}
#[test]
fn unmentioned_names_keep_the_native_workspace_spelling_unit() {
    let root = tempfile::tempdir().expect("fixture");
    fs::write(
        root.path().join("Cargo.toml"),
        "[workspace]\nmembers=['a','b']\nresolver='3'\n",
    )
    .expect("workspace");
    for (package, source) in [
        ("a", "pub struct Same;\npub fn make() -> Same { Same }\n"),
        ("b", "pub struct Same;\n"),
    ] {
        let dir = root.path().join(package);
        fs::create_dir_all(dir.join("src")).expect("directory");
        fs::write(
            dir.join("Cargo.toml"),
            format!("[package]\nname='{package}'\nversion='0.0.0'\nedition='2024'\n"),
        )
        .expect("manifest");
        fs::write(dir.join("src/lib.rs"), source).expect("source");
    }
    let snapshot = snapshot_workspace(root.path()).expect("snapshot");
    let nav = Navigation::new(&snapshot, VisibilitySelection::Public).expect("navigation");
    let report = nav.module("b/src/lib.rs").expect("card");
    assert!(report.contexts[0].unmentioned_names.is_empty());
    assert!(report.contexts[0].edges.is_empty());
}

#[test]
fn navigation_diagnostics_follow_the_question_and_preserve_required_unknowns() {
    let (_root, snapshot) = fixture("pub fn unrelated() -> Self { loop {} }\n");
    assert!(
        snapshot
            .graph()
            .diagnostics
            .iter()
            .any(|d| d.message.contains("Self is not resolved"))
    );
    assert!(
        !snapshot
            .source_diagnostics()
            .iter()
            .any(|d| d.message.contains("Self is not resolved"))
    );
    let nav = Navigation::new(&snapshot, VisibilitySelection::Public).expect("navigation");
    let Census::Rows(census) = nav.census(None).expect("census") else {
        panic!("rows")
    };
    assert!(
        !census
            .basis
            .diagnostics
            .iter()
            .any(|d| d.message.contains("Self is not resolved"))
    );
    let cost = nav.name("Cost").expect("cost");
    assert!(
        !cost
            .basis
            .diagnostics
            .iter()
            .any(|d| d.message.contains("Self is not resolved"))
    );
    let unrelated = nav.name("unrelated").expect("unrelated");
    assert!(
        unrelated
            .basis
            .diagnostics
            .iter()
            .any(|d| d.message.contains("Self is not resolved"))
    );
}
#[cfg(unix)]
#[test]
fn source_exclusions_remain_visible_in_every_native_query() {
    let (root, _) = fixture("");
    let source = root.path().join("crates/murail-fix/src/grade.rs");
    std::os::unix::fs::symlink(
        &source,
        root.path().join("crates/murail-fix/src/excluded.rs"),
    )
    .expect("excluded source");
    let snapshot = snapshot_workspace(root.path()).expect("snapshot");
    let nav = Navigation::new(&snapshot, VisibilitySelection::Public).expect("navigation");
    let Census::Rows(census) = nav.census(None).expect("census") else {
        panic!("rows")
    };
    assert!(
        census
            .basis
            .diagnostics
            .iter()
            .any(|d| d.file.ends_with("excluded.rs"))
    );
    assert!(
        nav.name("Cost")
            .expect("name")
            .basis
            .diagnostics
            .iter()
            .any(|d| d.file.ends_with("excluded.rs"))
    );
    assert!(
        nav.module("crates/murail-fix/src/grade.rs")
            .expect("module")
            .basis
            .diagnostics
            .iter()
            .any(|d| d.file.ends_with("excluded.rs"))
    );
}

#[test]
fn a_multiline_attribute_between_cfg_and_module_cannot_add_test_lines_to_census() {
    let (root, _) = fixture("");
    let src = root.path().join("crates/murail-fix/src");
    fs::write(src.join("lib.rs"), format!("{LIB}pub mod jit;\n")).expect("root");
    fs::write(
        src.join("jit.rs"),
        "#[cfg(test)]\n#[allow(\n    dead_code\n)]\nmod tests;\n",
    )
    .expect("jit module");
    fs::create_dir_all(src.join("jit")).expect("test directory");
    fs::write(src.join("jit/tests.rs"), "pub fn test_only() {}\n").expect("test source");
    let snapshot = snapshot_workspace(root.path()).expect("snapshot");
    let nav = Navigation::new(&snapshot, VisibilitySelection::Public).expect("navigation");
    let Census::Rows(report) = nav
        .census(Some("crates/murail-fix/src/jit"))
        .expect("census")
    else {
        panic!("rows")
    };
    assert_eq!(report.total.nontest_loc, 0);
    assert_eq!(report.total.token_lines, 0);
    assert_eq!(report.total.fns, 0);
}

#[test]
fn dump_serializes_the_existing_item_and_edge_authorities() {
    use atlas_core::queries::navigation::ExportKind;
    use atlas_core::source::{ItemKind, Visibility};
    let (_root, snapshot) = fixture("");
    let nav = Navigation::new(&snapshot, VisibilitySelection::All).expect("navigation");
    let dump = nav.dump().expect("dump");
    let refusal = dump
        .declarations
        .iter()
        .find(|d| d.site.name == "Refusal")
        .expect("Refusal");
    assert_eq!(refusal.site.kind, ItemKind::Enum);
    assert_eq!(refusal.home, "murail-fix::admit");
    assert_eq!(refusal.site.visibility, Visibility::PubCrate);
    assert_eq!(refusal.id, "crates/murail-fix/src/admit.rs:12:Refusal");
    assert!(
        dump.declarations
            .iter()
            .any(|d| d.site.name == "private_helper" && d.site.visibility == Visibility::Private)
    );
    assert!(dump.edges.iter().any(|e| e.source.name == "admit"
        && e.target == "Cost"
        && e.evidence == Evidence::ParameterMention));
    assert!(dump.edges.iter().any(|e| e.source.name == "admit"
        && e.target == "Tile"
        && e.evidence == Evidence::ReturnMention));
    assert!(dump.edges.iter().any(|e| e.source.name == "Tile"
        && e.target == "Cost"
        && e.evidence == Evidence::TypeBodyMention));
    assert!(dump.exports.iter().any(|e| e.source_name == "Tile"
        && e.kind == ExportKind::Facade
        && matches!(e.target, atlas_core::model::Resolution::Unique(_))));
    assert!(dump.exports.iter().any(|e| e.source_name == "Cost"
        && e.local_name == "PublicCost"
        && e.kind == ExportKind::Reexport));
    let public = Navigation::new(&snapshot, VisibilitySelection::Public)
        .expect("navigation")
        .dump()
        .expect("dump");
    assert!(!public.declarations.iter().any(|d| d.site.name == "Refusal"));
}
#[test]
fn public_root_reexports_are_the_only_facade_edges() {
    use atlas_core::queries::navigation::ExportKind;
    let (root, _) = fixture("");
    fs::write(
        root.path().join("crates/murail-fix/src/lib.rs"),
        format!("{LIB}pub(crate) use crate::grade::Cost as InternalCost;\n"),
    )
    .expect("root");
    let snapshot = snapshot_workspace(root.path()).expect("snapshot");
    let dump = Navigation::new(&snapshot, VisibilitySelection::RestrictedOrPublic)
        .expect("navigation")
        .dump()
        .expect("dump");
    assert!(
        dump.exports
            .iter()
            .any(|e| e.local_name == "Tile" && e.kind == ExportKind::Facade)
    );
    assert!(
        dump.exports
            .iter()
            .any(|e| e.local_name == "InternalCost" && e.kind == ExportKind::Reexport)
    );
    assert!(
        dump.exports
            .iter()
            .any(|e| e.local_name == "PublicCost" && e.kind == ExportKind::Reexport)
    );
}
#[test]
fn grouped_exports_keep_bound_names_paths_and_typed_missing_targets() {
    use atlas_core::{model::Resolution, queries::navigation::BindingEvidence};
    let (root, _) = fixture("");
    fs::write(root.path().join("crates/murail-fix/src/lib.rs"), format!("{LIB}pub use crate::{{admit::Tile as PublicTile, grade::{{Cost as Fee, Missing as Unknown}}}};\n")).expect("exports");
    let snapshot = snapshot_workspace(root.path()).expect("snapshot");
    let dump = Navigation::new(&snapshot, VisibilitySelection::Public)
        .expect("navigation")
        .dump()
        .expect("dump");
    for name in ["PublicTile", "Fee"] {
        let export = dump
            .exports
            .iter()
            .find(|e| e.local_name == name)
            .expect("export");
        assert_eq!(export.evidence, BindingEvidence::QualifiedSuffix);
        assert!(matches!(export.target, Resolution::Unique(_)));
        assert!(export.source_path.starts_with("crate::"));
    }
    assert!(matches!(
        dump.exports
            .iter()
            .find(|e| e.local_name == "Unknown")
            .expect("unknown")
            .target,
        Resolution::Unresolved(_)
    ));
    let ids: std::collections::BTreeSet<_> = dump.exports.iter().map(|e| &e.id).collect();
    assert_eq!(ids.len(), dump.exports.len());
}
#[test]
fn export_candidates_preserve_qualified_and_name_preference_evidence() {
    use atlas_core::{model::Resolution, queries::navigation::BindingEvidence};
    let (root, _) = fixture("");
    let src = root.path().join("crates/murail-fix/src");
    fs::write(src.join("a.rs"), "pub struct Repeated;\n").expect("a");
    fs::write(src.join("b.rs"), "pub struct Repeated;\n").expect("b");
    fs::write(src.join("lib.rs"), format!("{LIB}pub use crate::a::Repeated as Precise;\npub use Repeated as Ambiguous;\npub use nowhere::Cost as LocalFallback;\n")).expect("root");
    let snapshot = snapshot_workspace(root.path()).expect("snapshot");
    let dump = Navigation::new(&snapshot, VisibilitySelection::Public)
        .expect("navigation")
        .dump()
        .expect("dump");
    let precise = dump
        .exports
        .iter()
        .find(|e| e.local_name == "Precise")
        .expect("precise");
    assert_eq!(precise.evidence, BindingEvidence::QualifiedSuffix);
    let Resolution::Unique(id) = &precise.target else {
        panic!("one qualified candidate")
    };
    assert!(id.contains("/a.rs:"));
    let ambiguous = dump
        .exports
        .iter()
        .find(|e| e.local_name == "Ambiguous")
        .expect("ambiguous");
    assert_eq!(ambiguous.evidence, BindingEvidence::PackageName);
    let Resolution::Ambiguous(ids) = &ambiguous.target else {
        panic!("ambiguity")
    };
    assert_eq!(ids.len(), 2);
    let local = dump
        .exports
        .iter()
        .find(|e| e.local_name == "LocalFallback")
        .expect("local");
    assert_eq!(local.evidence, BindingEvidence::PackageName);
    assert!(matches!(local.target, Resolution::Unique(_)));
}

#[test]
fn exports_keep_global_ambiguity_qualified_priority_and_package_context() {
    use atlas_core::{model::Resolution, queries::navigation::BindingEvidence};
    let root = tempfile::tempdir().expect("fixture");
    fs::write(
        root.path().join("Cargo.toml"),
        "[workspace]\nmembers=['a','b','c']\nresolver='3'\n",
    )
    .expect("workspace");
    for (package, source) in [
        (
            "a",
            "pub struct Shared;\npub struct Local;\npub use b::Shared as Qualified;\npub use Unique as Global;\npub use Common as Ambiguous;\npub use crate::nested::Repeated as AmbiguousPath;\npub use missing::Local as LocalChoice;\n",
        ),
        (
            "b",
            "pub struct Shared;\npub struct Unique;\npub struct Common;\npub mod nested;\n",
        ),
        (
            "c",
            "pub struct Shared;\npub struct Common;\npub mod nested;\n",
        ),
    ] {
        let dir = root.path().join(package);
        fs::create_dir_all(dir.join("src")).expect("source directory");
        fs::write(
            dir.join("Cargo.toml"),
            format!("[package]\nname='{package}'\nversion='0.0.0'\nedition='2024'\n"),
        )
        .expect("manifest");
        fs::write(dir.join("src/lib.rs"), source).expect("source");
        if package != "a" {
            fs::write(dir.join("src/nested.rs"), "pub struct Repeated;\n").expect("nested");
        }
    }
    let snapshot = snapshot_workspace(root.path()).expect("snapshot");
    let dump = Navigation::new(&snapshot, VisibilitySelection::Public)
        .expect("navigation")
        .dump()
        .expect("dump");
    let definition = |id: &str| {
        dump.declarations
            .iter()
            .find(|d| d.id == id)
            .expect("candidate")
    };
    for (name, expected) in [
        ("Qualified", BindingEvidence::QualifiedSuffix),
        ("Global", BindingEvidence::WorkspaceName),
        ("LocalChoice", BindingEvidence::PackageName),
    ] {
        let export = dump
            .exports
            .iter()
            .find(|e| e.local_name == name)
            .expect("export");
        assert_eq!(export.evidence, expected);
        let Resolution::Unique(id) = &export.target else {
            panic!("unique candidate")
        };
        assert_eq!(
            definition(id).site.package,
            if name == "LocalChoice" { "a" } else { "b" }
        );
    }
    for (name, expected) in [
        ("Ambiguous", BindingEvidence::WorkspaceName),
        ("AmbiguousPath", BindingEvidence::QualifiedSuffix),
    ] {
        let export = dump
            .exports
            .iter()
            .find(|e| e.local_name == name)
            .expect("export");
        assert_eq!(export.evidence, expected);
        let Resolution::Ambiguous(ids) = &export.target else {
            panic!("ambiguous candidates")
        };
        assert_eq!(ids.len(), 2);
    }
}
#[test]
fn shared_export_sites_keep_distinct_occurrence_and_declaration_ids() {
    use atlas_core::{model::Resolution, queries::navigation::ExportKind};
    let root = tempfile::tempdir().expect("fixture");
    fs::create_dir_all(root.path().join("src/inner/src")).expect("directories");
    fs::write(root.path().join("Cargo.toml"), "[package]\nname='outer'\nversion='0.0.0'\nedition='2024'\n[workspace]\nmembers=['src/inner']\n").expect("manifest");
    fs::write(root.path().join("src/lib.rs"), "pub struct Outer;\n").expect("root source");
    fs::write(
        root.path().join("src/inner/Cargo.toml"),
        "[package]\nname='inner'\nversion='0.0.0'\nedition='2024'\n",
    )
    .expect("inner manifest");
    fs::write(
        root.path().join("src/inner/src/lib.rs"),
        "pub struct Shared;\npub use Shared as PublicShared;\n",
    )
    .expect("shared source");
    let snapshot = snapshot_workspace(root.path()).expect("snapshot");
    let dump = Navigation::new(&snapshot, VisibilitySelection::Public)
        .expect("navigation")
        .dump()
        .expect("dump");
    assert_eq!(dump.exports.len(), 2);
    assert_ne!(dump.exports[0].id, dump.exports[1].id);
    for export in dump.exports {
        let Resolution::Unique(id) = &export.target else {
            panic!("package candidate")
        };
        let declaration = dump
            .declarations
            .iter()
            .find(|d| d.id == *id)
            .expect("definition");
        assert_eq!(declaration.site.package, export.site.package);
        assert_eq!(
            export.kind,
            if export.site.package == "inner" {
                ExportKind::Facade
            } else {
                ExportKind::Reexport
            }
        );
    }
}
