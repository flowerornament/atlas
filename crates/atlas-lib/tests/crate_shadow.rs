//! A relative path names the caller's module first: a local module (or
//! type) named like a workspace crate shadows the crate, as in Rust; a
//! local function of that name does not, since a path's first segment is
//! read in the type namespace. The first case is the 2026-11-21 review's
//! fixture.
mod support;
use atlas_lib::{analysis::Analysis, discovery};
use std::collections::BTreeMap;
use support::Fixture;

/// Each resolved call's caller and target, by their qualified names.
fn calls(files: &[(&str, &str)]) -> BTreeMap<String, String> {
    let f = Fixture::new(files);
    let metadata = cargo_metadata::MetadataCommand::new()
        .current_dir(f.path())
        .no_deps()
        .exec()
        .expect("metadata");
    let snapshot = discovery::snapshot_metadata(&metadata).expect("snapshot");
    let facts = Analysis::new(&snapshot)
        .expect("analysis")
        .representations()
        .expect("facts");
    let names: BTreeMap<_, _> = facts
        .functions
        .iter()
        .map(|f| (f.id.as_str(), format!("{} {}", f.file, f.qualified)))
        .collect();
    facts
        .calls
        .iter()
        .map(|c| (names[c.from.as_str()].clone(), names[c.to.as_str()].clone()))
        .collect()
}

#[test]
fn a_local_module_shadows_a_crate_of_its_name() {
    let calls = calls(&[
        (
            "Cargo.toml",
            "[package]\nname=\"crate-shadow\"\nversion=\"0.1.0\"\nedition=\"2024\"\n",
        ),
        (
            "src/lib.rs",
            "mod crate_shadow { pub fn target() {} }\nfn target() {}\npub fn caller() { crate_shadow::target(); }\n",
        ),
    ]);
    assert_eq!(
        calls.get("src/lib.rs caller").map(String::as_str),
        Some("src/lib.rs crate_shadow::target"),
        "{calls:#?}"
    );
}

#[test]
fn a_local_function_does_not_shadow_a_crate_of_its_name() {
    let calls = calls(&[
        (
            "Cargo.toml",
            "[workspace]\nmembers=[\"a\",\"b\"]\nresolver=\"3\"\n",
        ),
        (
            "a/Cargo.toml",
            "[package]\nname=\"a\"\nversion=\"0.1.0\"\nedition=\"2024\"\n[dependencies]\nb={path=\"../b\"}\n",
        ),
        (
            "a/src/lib.rs",
            "fn b() {}\npub fn caller() { b::target(); }\n",
        ),
        (
            "b/Cargo.toml",
            "[package]\nname=\"b\"\nversion=\"0.1.0\"\nedition=\"2024\"\n",
        ),
        ("b/src/lib.rs", "pub fn target() {}\n"),
    ]);
    assert_eq!(
        calls.get("a/src/lib.rs caller").map(String::as_str),
        Some("b/src/lib.rs target"),
        "{calls:#?}"
    );
}

/// The same rule for a re-export's target, which the index reads while it
/// is still being built.
#[test]
fn a_reexport_of_a_local_module_names_the_module_not_the_crate() {
    let calls = calls(&[
        (
            "Cargo.toml",
            "[package]\nname=\"crate-shadow\"\nversion=\"0.1.0\"\nedition=\"2024\"\n",
        ),
        (
            "src/lib.rs",
            "mod crate_shadow { pub fn target() {} }\nfn target() {}\npub use crate_shadow::target as exported;\npub mod user;\n",
        ),
        ("src/user.rs", "pub fn caller() { crate::exported(); }\n"),
    ]);
    assert_eq!(
        calls.get("src/user.rs caller").map(String::as_str),
        Some("src/lib.rs crate_shadow::target"),
        "{calls:#?}"
    );
}

/// Two workspace crates `a` and `b`, with `a` depending on `b` and `a`'s
/// library written as `lib`.
fn with_crate_b(lib: &str) -> BTreeMap<String, String> {
    calls(&[
        (
            "Cargo.toml",
            "[workspace]\nmembers=[\"a\",\"b\"]\nresolver=\"3\"\n",
        ),
        (
            "a/Cargo.toml",
            "[package]\nname=\"a\"\nversion=\"0.1.0\"\nedition=\"2024\"\n[dependencies]\nb={path=\"../b\"}\n",
        ),
        ("a/src/lib.rs", lib),
        (
            "b/Cargo.toml",
            "[package]\nname=\"b\"\nversion=\"0.1.0\"\nedition=\"2024\"\n",
        ),
        ("b/src/lib.rs", "pub fn target() {}\n"),
    ])
}

/// A glob import binds a name before the extern prelude does, so a module
/// a glob brings in shadows a crate of its name.
#[test]
fn a_glob_binds_a_name_before_a_crate_of_that_name() {
    let calls = with_crate_b(
        "mod adapters { pub mod b { pub fn target() {} } }\nmod user { use crate::adapters::*; pub fn caller() { b::target(); } }\n",
    );
    assert_eq!(
        calls.get("a/src/lib.rs user::caller").map(String::as_str),
        Some("a/src/lib.rs adapters::b::target"),
        "{calls:#?}"
    );
}

/// A leading `::` names a crate, never a module in scope, in a call and in
/// an import alike.
#[test]
fn a_leading_double_colon_names_the_crate_not_a_module() {
    let calls = with_crate_b(
        "mod b { pub fn target() {} }\npub fn called() { ::b::target(); }\nmod imported { use ::b::target; pub fn caller() { target(); } }\n",
    );
    for caller in ["a/src/lib.rs called", "a/src/lib.rs imported::caller"] {
        assert_eq!(
            calls.get(caller).map(String::as_str),
            Some("b/src/lib.rs target"),
            "{caller}: {calls:#?}"
        );
    }
}
