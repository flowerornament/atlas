//! Physical inventory and contextual name resolution are different observations.
use atlas_core::{
    analysis::Analysis,
    discovery,
    population::{FileSelection, TestSelection},
    queries, review,
};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new(files: &[(&str, &str)]) -> Self {
        let root = std::env::temp_dir().join(format!(
            "atlas-context-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        for (path, text) in files {
            let path = root.join(path);
            fs::create_dir_all(path.parent().expect("fixture file parent"))
                .expect("fixture directories");
            fs::write(path, text).expect("fixture source");
        }
        Self(root)
    }
    fn capture(&self) -> discovery::Snapshot {
        discovery::snapshot_workspace(&self.0).expect("fixture snapshot")
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn shared_package_roots_keep_contexts_and_one_physical_report() {
    let fixture = Fixture::new(&[
        (
            "Cargo.toml",
            "[workspace]\nmembers=['left','right']\nresolver='3'\n",
        ),
        (
            "left/Cargo.toml",
            "[package]\nname='left'\nversion='0.0.0'\nedition='2024'\n[lib]\npath='../shared.rs'\n",
        ),
        (
            "right/Cargo.toml",
            "[package]\nname='right'\nversion='0.0.0'\nedition='2024'\n[lib]\npath='../shared.rs'\n",
        ),
        (
            "shared.rs",
            "pub struct Token {}\npub fn make() -> Token { Token {} }\npub fn caller() -> Token { make() }\n",
        ),
    ]);
    let snapshot = fixture.capture();
    assert_eq!(snapshot.graph().files.len(), 2);
    assert_eq!(snapshot.graph().physical_files().count(), 1);
    assert_eq!(snapshot.files(FileSelection::SourceTree).count(), 0);
    let cards = queries::name(snapshot.graph(), "Token").unwrap();
    assert!(cards.ambiguous);
    assert_ne!(
        cards.matches[0].declaration.id,
        cards.matches[1].declaration.id
    );
    assert_eq!(queries::census(snapshot.graph(), None).unwrap().files, 1);
    assert_eq!(review::outline(&snapshot, ".").unwrap().files.len(), 1);
    let facts = Analysis::new(&snapshot).unwrap().representations().unwrap();
    assert_eq!(facts.types.len(), 1);
    assert_eq!(facts.functions.len(), 2);
    assert_eq!(facts.fingerprints.len(), 1);
    assert_eq!(
        facts.calls.len(),
        1,
        "contexts agree on the same physical body"
    );
    assert_eq!(facts.constructors.len(), 1);
    assert_eq!(facts.references.len(), 2);
    for level in [
        atlas_core::diagram::Level::Files,
        atlas_core::diagram::Level::Declarations,
    ] {
        let diagram = atlas_core::diagram::project(
            snapshot.graph(),
            None,
            &atlas_core::diagram::Query {
                focus: None,
                level,
                view: atlas_core::diagram::View::Types,
            },
        )
        .unwrap();
        diagram.validate().unwrap();
        assert_eq!(diagram.inventory, ["shared.rs"]);
        if level == atlas_core::diagram::Level::Files {
            assert_eq!(diagram.nodes.len(), 1);
        } else {
            assert!(
                diagram
                    .nodes
                    .iter()
                    .any(|node| node.label.contains("@left"))
            );
            assert!(
                diagram
                    .nodes
                    .iter()
                    .any(|node| node.label.contains("@right"))
            );
        }
    }
}

#[test]
fn targets_sharing_one_root_do_not_overwrite_each_other() {
    let fixture = Fixture::new(&[
        (
            "Cargo.toml",
            "[package]\nname='fixture'\nversion='0.0.0'\nedition='2024'\n[lib]\npath='src/shared.rs'\n[[bin]]\nname='runner'\npath='src/shared.rs'\n[[test]]\nname='integration'\npath='src/shared.rs'\n",
        ),
        ("src/shared.rs", "pub fn make() {}\nfn main() { make(); }\n"),
    ]);
    let snapshot = fixture.capture();
    let architecture = atlas_core::architecture::Architecture {
        schema: 1,
        provenance: vec![],
        modules: vec![],
        workflows: vec![],
    };
    let analysis = Analysis::new(&snapshot).unwrap();
    let index = analysis.index(&architecture).unwrap();
    assert_eq!(
        index.functions.len(),
        4,
        "library and binary each retain their functions"
    );
    assert_eq!(index.roots.len(), 1);
    assert!(
        index.select("src/shared.rs", "main").is_err(),
        "a physical selector cannot choose a target"
    );
    assert_eq!(
        analysis
            .with_tests(TestSelection::All)
            .index(&architecture)
            .unwrap()
            .functions
            .len(),
        6
    );
    let mounts = &snapshot.graph().files[0].mounts;
    assert_eq!(mounts.len(), 3);
    assert_eq!(mounts.iter().filter(|mount| mount.test_only).count(), 1);
}

#[test]
fn differing_module_contexts_never_become_unique_physical_arrows() {
    let fixture = Fixture::new(&[
        (
            "Cargo.toml",
            "[package]\nname='fixture'\nversion='0.0.0'\nedition='2024'\n",
        ),
        ("src/lib.rs", "pub mod a;\npub mod b;\n"),
        (
            "src/a.rs",
            "pub struct Token;\npub fn other() {}\n#[path=\"common.rs\"]\npub mod common;\n",
        ),
        (
            "src/b.rs",
            "pub struct Token;\npub fn other() {}\n#[path=\"common.rs\"]\npub mod common;\n",
        ),
        (
            "src/common.rs",
            "pub fn pass(value: super::Token) -> super::Token { value }\npub fn call() { super::other(); }\n",
        ),
    ]);
    let snapshot = fixture.capture();
    let facts = Analysis::new(&snapshot).unwrap().representations().unwrap();
    let pass = facts
        .functions
        .iter()
        .find(|function| function.name == "pass")
        .unwrap();
    assert!(
        !facts
            .references
            .iter()
            .any(|reference| reference.from == pass.id)
    );
    assert_eq!(
        facts
            .unknown_types
            .iter()
            .filter(|unknown| unknown.from == pass.id)
            .count(),
        2
    );
    let call = facts
        .functions
        .iter()
        .find(|function| function.name == "call")
        .unwrap();
    assert!(!facts.calls.iter().any(|edge| edge.from == call.id));
    assert!(
        facts
            .unknown_calls
            .iter()
            .any(|unknown| unknown.from == call.id && unknown.reason.contains("contexts"))
    );
}

#[test]
fn population_selects_imports_and_items_from_the_same_test_evidence() {
    let fixture = Fixture::new(&[
        (
            "Cargo.toml",
            "[package]\nname='fixture'\nversion='0.0.0'\nedition='2024'\n",
        ),
        (
            "src/lib.rs",
            "pub mod live;\npub mod test_version;\nuse crate::live::Token;\n#[cfg(all(test, feature=\"checks\"))]\nuse crate::test_version::Token;\n#[test]\nfn helper() {}\npub fn pass(value: Token) -> Token { value }\n#[cfg(test)] const HIDDEN: &str = \"no\";\npub fn next() {}\n",
        ),
        ("src/live.rs", "pub struct Token;\n"),
        ("src/test_version.rs", "pub struct Token;\n"),
    ]);
    let snapshot = fixture.capture();
    let analysis = Analysis::new(&snapshot).unwrap();
    let production = analysis.representations().unwrap();
    let pass = production
        .functions
        .iter()
        .find(|function| function.name == "pass")
        .unwrap();
    assert_eq!(
        production
            .references
            .iter()
            .filter(|reference| reference.from == pass.id)
            .count(),
        2
    );
    assert!(
        !production
            .functions
            .iter()
            .any(|function| function.name == "helper")
    );
    assert!(
        !production
            .constants
            .iter()
            .any(|constant| constant.name == "HIDDEN")
    );
    assert!(
        production
            .functions
            .iter()
            .any(|function| function.name == "next"),
        "an inline attribute cannot gate the following declaration"
    );
    let all = analysis
        .with_tests(TestSelection::All)
        .representations()
        .unwrap();
    assert!(
        all.functions
            .iter()
            .any(|function| function.name == "helper")
    );
    assert!(
        all.constants
            .iter()
            .any(|constant| constant.name == "HIDDEN")
    );
    assert!(
        !all.references
            .iter()
            .any(|reference| reference.from == pass.id),
        "competing imports stay ambiguous"
    );
}

#[test]
fn symlink_and_parent_escape_mounts_are_excluded_without_aliasing_workspace_files() {
    let fixture = Fixture::new(&[
        (
            "Cargo.toml",
            "[package]\nname='fixture'\nversion='0.0.0'\nedition='2024'\n",
        ),
        (
            "src/lib.rs",
            "#[path=\"../../outside.rs\"]\nmod escaped;\n#[path=\"linked.rs\"]\nmod linked;\n",
        ),
        ("outside.rs", "pub struct Inside;\n"),
    ]);
    #[cfg(unix)]
    std::os::unix::fs::symlink(
        fixture.0.join("outside.rs"),
        fixture.0.join("src/linked.rs"),
    )
    .unwrap();
    let snapshot = fixture.capture();
    assert!(
        !snapshot
            .graph()
            .files
            .iter()
            .any(|file| file.path == "outside.rs")
    );
    assert!(
        snapshot.graph().files[0]
            .mounts
            .iter()
            .all(|mount| mount.modules.is_empty())
    );
    assert!(
        snapshot
            .graph()
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("module escaped"))
    );
    #[cfg(unix)]
    assert!(
        snapshot
            .graph()
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("symlink"))
    );
}

#[test]
fn production_analysis_does_not_parse_excluded_test_targets() {
    let fixture = Fixture::new(&[
        (
            "Cargo.toml",
            "[package]\nname='fixture'\nversion='0.0.0'\nedition='2024'\n",
        ),
        ("src/lib.rs", "pub struct Live;\n"),
        ("tests/broken.rs", "fn invalid Rust\n"),
    ]);
    let snapshot = fixture.capture();
    let analysis = Analysis::new(&snapshot).unwrap();
    assert_eq!(analysis.representations().unwrap().types.len(), 1);
    assert!(
        analysis
            .with_tests(TestSelection::All)
            .representations()
            .is_err()
    );
}

#[test]
fn package_queries_do_not_inherit_another_packages_mount_diagnostics() {
    let fixture = Fixture::new(&[
        (
            "Cargo.toml",
            "[workspace]\nmembers=['left','right']\nresolver='3'\n",
        ),
        (
            "left/Cargo.toml",
            "[package]\nname='left'\nversion='0.0.0'\nedition='2024'\n[lib]\npath='src/shared.rs'\n",
        ),
        (
            "right/Cargo.toml",
            "[package]\nname='right'\nversion='0.0.0'\nedition='2024'\n[lib]\npath='../left/src/shared.rs'\n",
        ),
        ("left/src/shared.rs", "mod child;\n"),
        ("left/src/child.rs", "pub struct Child;\n"),
    ]);
    let snapshot = fixture.capture();
    assert_eq!(
        queries::census(snapshot.graph(), Some("left"))
            .unwrap()
            .diagnostics,
        0
    );
    assert!(
        queries::module(snapshot.graph(), "left")
            .unwrap()
            .diagnostics
            .is_empty()
    );
    let right = queries::module(snapshot.graph(), "right").unwrap();
    assert!(
        right
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.package.as_deref() == Some("right")
                && diagnostic.message.contains("module child"))
    );
}

#[test]
fn lexical_and_ast_populations_agree_on_commented_stacked_and_multiline_gates() {
    let fixture = Fixture::new(&[
        (
            "Cargo.toml",
            "[package]\nname='fixture'\nversion='0.0.0'\nedition='2024'\n",
        ),
        (
            "src/lib.rs",
            "#[cfg(test)] // reason\n#[allow(dead_code)]\nfn commented() {}\n#[cfg(test)] #[allow(dead_code)]\nfn stacked() {}\n#[cfg(all(\n    test,\n    feature=\"checks\",\n))]\nfn multiline() {}\n#[cfg(all(\n    test,\n    feature=\"checks\",\n))]\nmod hidden;\npub fn live() {}\n",
        ),
        ("src/hidden.rs", "pub struct Hidden;\n"),
    ]);
    let snapshot = fixture.capture();
    let file = snapshot.physical_file("src/lib.rs").unwrap();
    let source = snapshot.source_facts("src/lib.rs").unwrap();
    let population = atlas_core::population::Population {
        tests: TestSelection::Production,
        visibility: atlas_core::population::VisibilitySelection::All,
    };
    let lexical: Vec<_> = source
        .outline()
        .iter()
        .filter(|item| {
            item.kind == atlas_core::source::ItemKind::Fn && population.admits(file, source, item)
        })
        .map(|item| item.name.as_str())
        .collect();
    let facts = Analysis::new(&snapshot).unwrap().representations().unwrap();
    let ast: Vec<_> = facts
        .functions
        .iter()
        .map(|function| function.name.as_str())
        .collect();
    assert_eq!(lexical, ["live"]);
    assert_eq!(ast, lexical);
    assert!(facts.types.is_empty());
    let hidden = snapshot.physical_file("src/hidden.rs").unwrap();
    assert!(hidden.test_only);
    assert!(hidden.mounts.iter().all(|mount| mount.test_only));
}

#[test]
fn explicit_file_workflows_can_still_select_test_bodies() {
    let fixture = Fixture::new(&[
        (
            "Cargo.toml",
            "[package]\nname='fixture'\nversion='0.0.0'\nedition='2024'\n",
        ),
        (
            "src/lib.rs",
            "#[cfg(test)]\nmod tests {\n    fn witness() { observe(); }\n}\n",
        ),
    ]);
    let workflow = atlas_core::workflow::read(&fixture.0, "src/lib.rs", "tests::witness").unwrap();
    assert_eq!(workflow.calls[0].spelling, "observe");
    assert_eq!(
        atlas_core::workflow::read_file(&fixture.0, "src/lib.rs")
            .unwrap()
            .functions
            .len(),
        1
    );
}

#[test]
fn native_mount_population_follows_support_children_without_expanding_src_budgets() {
    let fixture = Fixture::new(&[
        (
            "Cargo.toml",
            "[package]\nname='fixture'\nversion='0.0.0'\nedition='2024'\n",
        ),
        (
            "src/lib.rs",
            "pub mod load;\n#[cfg(test)]\n#[path=\"../tests/support.rs\"]\nmod test_support;\n",
        ),
        ("src/load.rs", "pub fn load() {}\n"),
        ("tests/support.rs", "mod nested;\n"),
        ("tests/support/nested.rs", "pub fn witness() {}\n"),
    ]);
    let snapshot = fixture.capture();
    assert_eq!(snapshot.files(FileSelection::SourceTree).count(), 2);
    let mounted = snapshot
        .mounted_sources("fixture")
        .unwrap()
        .files
        .as_slice();
    assert_eq!(mounted.len(), 2);
    assert!(
        mounted
            .iter()
            .any(|mount| mount.path == "tests/support.rs" && mount.module == "test_support")
    );
    assert!(
        mounted
            .iter()
            .any(|mount| mount.path == "tests/support/nested.rs"
                && mount.module == "test_support::nested")
    );
    for mount in mounted {
        assert!(snapshot.source_facts(&mount.path).is_some());
        assert!(snapshot.physical_file(&mount.path).unwrap().test_only);
    }
    assert_eq!(
        Analysis::new(&snapshot)
            .unwrap()
            .representations()
            .unwrap()
            .functions
            .len(),
        1
    );
}

#[test]
fn mounted_source_keeps_multiple_locations_and_missing_evidence_visible() {
    let fixture = Fixture::new(&[
        (
            "Cargo.toml",
            "[package]\nname='fixture'\nversion='0.0.0'\nedition='2024'\n",
        ),
        (
            "src/lib.rs",
            "#[cfg(test)]\n#[path=\"../tests/common.rs\"]\nmod a;\n#[cfg(test)]\n#[path=\"../tests/common.rs\"]\nmod b;\n#[path=\"../missing.rs\"]\nmod missing;\n",
        ),
        ("tests/common.rs", "#[path=\"../src/lib.rs\"]\nmod cycle;\n"),
    ]);
    let snapshot = fixture.capture();
    assert!(
        snapshot
            .mounted_sources("fixture")
            .unwrap()
            .unavailable
            .iter()
            .any(|issue| issue.module == "missing" && issue.candidates == ["missing.rs"])
    );
    let mounted = snapshot
        .mounted_sources("fixture")
        .unwrap()
        .files
        .as_slice();
    assert_eq!(
        mounted
            .iter()
            .filter(|mount| mount.path == "tests/common.rs")
            .map(|mount| mount.module.as_str())
            .collect::<Vec<_>>(),
        ["a", "b"]
    );
    assert_eq!(
        snapshot
            .graph()
            .physical_files()
            .filter(|file| file.path == "tests/common.rs")
            .count(),
        1
    );
    assert!(snapshot.graph().diagnostics.iter().any(|diagnostic| {
        diagnostic.package.as_deref() == Some("fixture")
            && diagnostic
                .message
                .contains("required source-tree mount missing")
    }));
}

#[test]
fn repeated_relationship_readings_deduplicate_witnesses_without_merging_packages() {
    let fixture = Fixture::new(&[
        (
            "Cargo.toml",
            "[workspace]\nmembers=['left','right']\nresolver='3'\n",
        ),
        (
            "left/Cargo.toml",
            "[package]\nname='left'\nversion='0.0.0'\nedition='2024'\n[lib]\npath='../shared.rs'\n",
        ),
        (
            "right/Cargo.toml",
            "[package]\nname='right'\nversion='0.0.0'\nedition='2024'\n[lib]\npath='../shared.rs'\n",
        ),
        (
            "shared.rs",
            "pub struct Shared;\nimpl Shared {\n    pub fn make() -> Self { Self }\n}\n",
        ),
    ]);
    let snapshot = fixture.capture();
    let mut graph = snapshot.graph().clone();
    for _ in 0..2 {
        let witnesses = graph
            .diagnostics
            .iter()
            .filter(|d| d.message.contains("Self is not resolved"))
            .collect::<Vec<_>>();
        assert_eq!(witnesses.len(), 2);
        assert!(
            witnesses
                .iter()
                .any(|d| d.package.as_deref() == Some("left"))
        );
        assert!(
            witnesses
                .iter()
                .any(|d| d.package.as_deref() == Some("right"))
        );
        atlas_core::relationships::derive(&mut graph);
    }
}
