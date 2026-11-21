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
