//! Module-shape diff: a text change that keeps every measure is marked, not hidden.
mod support;
use atlas_lib::{discovery, review};
use support::Fixture;

fn capture(f: &Fixture) -> discovery::Snapshot {
    discovery::snapshot_workspace(f.path()).expect("snapshot")
}

#[test]
fn a_same_shape_text_change_is_listed_with_shape_unchanged() {
    let f = Fixture::new(&[
        (
            "Cargo.toml",
            "[package]\nname=\"shape-diff\"\nversion=\"0.0.0\"\nedition=\"2024\"\n",
        ),
        ("src/lib.rs", "pub mod same;\npub mod grown;\n"),
        ("src/same.rs", "pub fn value() -> u8 {\n    1\n}\n"),
        ("src/grown.rs", "pub fn value() -> u8 {\n    1\n}\n"),
    ]);
    let before = capture(&f);
    f.write("src/same.rs", "pub fn value() -> u8 {\n    2\n}\n");
    f.write(
        "src/grown.rs",
        "pub fn value() -> u8 {\n    1\n}\npub fn more() {}\n",
    );
    let after = capture(&f);
    let report = review::diff(&before, &after, "src").expect("diff");
    let rows: Vec<_> = report
        .changes
        .iter()
        .map(|change| (change.file.as_str(), change.shape_changed))
        .collect();
    assert_eq!(rows, [("src/grown.rs", true), ("src/same.rs", false)]);
}
