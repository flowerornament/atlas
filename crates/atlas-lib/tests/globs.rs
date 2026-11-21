//! Glob imports of workspace modules bind names conservatively: a glob of
//! the module itself or an ancestor binds everything there, private items
//! and imports included; a glob of another module binds its non-private
//! items and re-exports; a name the module declares or imports explicitly
//! shadows every glob; two globs binding different items are ambiguous.
mod support;
use atlas_lib::{analysis::Analysis, discovery, population::TestSelection};
use std::collections::BTreeMap;
use support::Fixture;

const LIB: &str = "\
pub mod m;
pub mod n;
pub mod other;
use crate::other::imported;
pub fn helper() {}
struct Item;
impl Item {
    fn new() -> Self { Item }
}
fn uses_import() { imported(); }
#[cfg(test)]
mod tests {
    use super::*;
    fn ancestor() {
        helper();
        Item::new();
        imported();
    }
}
mod outside {
    use crate::m::*;
    fn non_ancestor() {
        public();
        private();
    }
}
mod shadow {
    use crate::m::*;
    fn public() {}
    fn shadowed() { public(); }
}
mod explicit {
    use crate::m::*;
    use crate::n::public;
    fn wins() { public(); }
}
mod two {
    use crate::m::*;
    use crate::n::*;
    fn conflict() { public(); }
    fn same() { shared(); }
}
mod external {
    use std::collections::*;
    fn foreign() { HashMap::<u8, u8>::new(); }
}
";

fn calls() -> BTreeMap<String, String> {
    let f = Fixture::new(&[
        (
            "Cargo.toml",
            "[package]\nname=\"globs\"\nversion=\"0.0.0\"\nedition=\"2024\"\n",
        ),
        ("src/lib.rs", LIB),
        (
            "src/m.rs",
            "pub fn public() {}\nfn private() {}\npub use crate::other::shared;\n",
        ),
        (
            "src/n.rs",
            "pub fn public() {}\npub use crate::other::shared;\n",
        ),
        ("src/other.rs", "pub fn imported() {}\npub fn shared() {}\n"),
    ]);
    let metadata = cargo_metadata::MetadataCommand::new()
        .current_dir(f.path())
        .no_deps()
        .exec()
        .expect("metadata");
    let snapshot = discovery::snapshot_metadata(&metadata).expect("snapshot");
    let facts = Analysis::new(&snapshot)
        .expect("analysis")
        .with_tests(TestSelection::All)
        .representations()
        .expect("facts");
    let names: BTreeMap<_, _> = facts
        .functions
        .iter()
        .map(|f| (f.id.as_str(), f.qualified.as_str()))
        .collect();
    let mut out = BTreeMap::new();
    for call in &facts.calls {
        out.insert(
            format!("{} {}", names[call.from.as_str()], call.site.line),
            names[call.to.as_str()].to_owned(),
        );
    }
    for unknown in &facts.unknown_calls {
        out.insert(
            format!(
                "{} {}",
                names[unknown.from.as_str()],
                unknown.call.site.line
            ),
            format!("? {}", unknown.reason),
        );
    }
    out
}

#[test]
fn globs_of_workspace_modules_bind_what_rust_would() {
    let calls = calls();
    let at = |key: &str| calls.get(key).map_or("absent", String::as_str);
    // An ancestor's glob: private items, impl owners and its imports.
    assert_eq!(at("tests::ancestor 15"), "helper", "{calls:#?}");
    assert_eq!(at("tests::ancestor 16"), "Item::new", "{calls:#?}");
    assert_eq!(at("tests::ancestor 17"), "imported", "{calls:#?}");
    // Another module's glob: its public items only.
    assert_eq!(at("outside::non_ancestor 23"), "public", "{calls:#?}");
    assert!(
        at("outside::non_ancestor 24").starts_with("? "),
        "{calls:#?}"
    );
    // The module's own item, and an explicit import, shadow the glob.
    assert_eq!(at("shadow::shadowed 30"), "shadow::public", "{calls:#?}");
    assert_eq!(at("explicit::wins 35"), "public", "{calls:#?}");
    // Two globs: different items are ambiguous; one item twice is that item.
    assert!(at("two::conflict 40").starts_with("? "), "{calls:#?}");
    assert_eq!(at("two::same 41"), "shared", "{calls:#?}");
    // An external glob binds nothing Atlas can name, and says so.
    assert!(
        at("external::foreign 45").contains("glob-imports"),
        "{calls:#?}"
    );
}
