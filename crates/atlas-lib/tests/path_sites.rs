//! Path sites: each `use` leaf and each written path in production code,
//! resolved through the qualified index, glob re-exports and type aliases
//! included, with what binds its first segment.
mod support;
use atlas_lib::{analysis::Analysis, analysis::index::sites::SiteForm, discovery};
use support::Fixture;

/// Each production site in `file`: line, written path, where it leads in
/// the first scope (`?` for ambiguous), and what binds its first segment.
fn sites(f: &Fixture, file: &str) -> Vec<(usize, String, String, String, SiteForm)> {
    let snapshot = discovery::snapshot_workspace(f.path()).expect("snapshot");
    let join = |path: &Option<Vec<String>>| path.as_ref().map_or("?".into(), |p| p.join("::"));
    Analysis::new(&snapshot)
        .expect("analysis")
        .path_sites()
        .expect("sites")
        .into_iter()
        .filter(|site| site.file == file)
        .map(|site| {
            (
                site.line,
                site.written.clone(),
                join(&site.targets[0]),
                site.through
                    .as_ref()
                    .map_or(String::new(), |p| p.join("::")),
                site.form,
            )
        })
        .collect()
}
fn fixture(user: &str) -> Fixture {
    Fixture::new(&[
        (
            "Cargo.toml",
            "[package]\nname=\"tutorial\"\nversion=\"0.0.0\"\nedition=\"2024\"\n",
        ),
        ("src/lib.rs", "pub mod front;\npub mod user;\n"),
        (
            "src/front/mod.rs",
            "pub mod parse;\npub use self::parse::*;\npub use parse::Tree as Door;\n",
        ),
        (
            "src/front/parse.rs",
            "pub struct Tree;\nimpl Tree { pub fn new() -> Self { Tree } }\npub fn helper() {}\n",
        ),
        ("src/user.rs", user),
    ])
}
const HOME: &str = "tutorial::front::parse";

#[test]
fn use_leaves_and_paths_resolve_through_every_route() {
    let f = fixture(
        "use crate::front::parse::{helper, Tree as Renamed};\nuse crate::front as f;\ntype Alias = crate::front::parse::Tree;\npub fn run() {\n    crate::front::parse::helper(); f::parse::helper();\n    crate::front::Door::new();\n    crate::front::helper();\n    let _ = Alias::new();\n    Renamed::new();\n}\n",
    );
    let found = sites(&f, "src/user.rs");
    let at = |line: usize, written: &str| {
        found
            .iter()
            .find(|s| s.0 == line && s.1 == written)
            .unwrap_or_else(|| panic!("{line} {written}: {found:#?}"))
    };
    // Two leaves of one `use` list, the rename read as its source path.
    assert_eq!(
        at(1, "crate::front::parse::helper").2,
        format!("{HOME}::helper")
    );
    assert_eq!(
        at(1, "crate::front::parse::Tree").2,
        format!("{HOME}::Tree")
    );
    // Two paths on one line are two sites; a respelled parent resolves.
    assert_eq!(
        at(5, "crate::front::parse::helper").2,
        format!("{HOME}::helper")
    );
    assert_eq!(at(5, "f::parse::helper").3, "tutorial::front");
    // A re-export door, a glob re-export door, and a type alias all lead
    // into the home.
    assert_eq!(
        at(6, "crate::front::Door::new").2,
        format!("{HOME}::Tree::new")
    );
    assert_eq!(at(7, "crate::front::helper").2, format!("{HOME}::helper"));
    assert_eq!(at(8, "Alias::new").2, format!("{HOME}::Tree::new"));
    // Through an import or an alias, the binding is named: such a path
    // repeats the import's reading.
    assert_eq!(at(8, "Alias::new").3, format!("{HOME}::Tree"));
    assert_eq!(at(9, "Renamed::new").3, format!("{HOME}::Tree"));
}

#[test]
fn macro_input_is_read_as_code_or_as_spelled_paths_and_tests_are_out() {
    let f = fixture(
        "pub fn run() {\n    assert!(crate::front::parse::helper() == ());\n    custom! { go crate::front::parse::helper }\n}\n#[cfg(test)]\nmod tests { fn t() { crate::front::parse::helper(); } }\n",
    );
    let found = sites(&f, "src/user.rs");
    let home: Vec<_> = found.iter().filter(|s| s.2.starts_with(HOME)).collect();
    assert_eq!(home.len(), 2, "{found:#?}");
    assert_eq!(home[0].4, SiteForm::MacroArgument);
    assert_eq!(home[1].4, SiteForm::MacroText);
    assert!(
        home.iter().all(|s| s.0 < 5),
        "a test module is not production: {found:#?}"
    );
}

#[test]
fn super_and_globs_resolve_relative_to_their_module() {
    let f = Fixture::new(&[
        (
            "Cargo.toml",
            "[package]\nname=\"tutorial\"\nversion=\"0.0.0\"\nedition=\"2024\"\n",
        ),
        ("src/lib.rs", "pub mod front;\n"),
        (
            "src/front/mod.rs",
            "pub mod parse;\npub mod child;\nfn shell() { parse::helper(); }\n",
        ),
        ("src/front/parse.rs", "pub fn helper() {}\n"),
        (
            "src/front/child.rs",
            "use super::*;\npub mod grand { pub fn g() { super::super::parse::helper(); super::parse::helper(); } }\npub fn c() { super::parse::helper(); parse::helper(); }\n",
        ),
    ]);
    let found = sites(&f, "src/front/child.rs");
    let target = |line: usize, written: &str| {
        found
            .iter()
            .find(|s| s.0 == line && s.1 == written)
            .map_or_else(|| panic!("{line} {written}: {found:#?}"), |s| s.2.clone())
    };
    assert_eq!(target(1, "super"), "tutorial::front");
    assert_eq!(
        target(2, "super::super::parse::helper"),
        format!("{HOME}::helper")
    );
    // From a grandchild, `super::parse` is the child's, which has none.
    assert_ne!(target(2, "super::parse::helper"), format!("{HOME}::helper"));
    assert_eq!(target(3, "super::parse::helper"), format!("{HOME}::helper"));
    // Through the parent's glob: the glob is the binding.
    assert_eq!(target(3, "parse::helper"), format!("{HOME}::helper"));
    let door = sites(&f, "src/front/mod.rs");
    assert!(
        door.iter()
            .any(|s| s.1 == "parse::helper" && s.3.is_empty()),
        "{door:#?}"
    );
}
