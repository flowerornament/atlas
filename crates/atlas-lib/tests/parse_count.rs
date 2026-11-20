//! How many times each file is parsed. Counts are per thread, and each test
//! runs on its own, so parallel tests do not disturb them.
mod support;
use atlas_lib::{analysis::Analysis, discovery, source::parse_count};
use support::Fixture;

const FILES: usize = 3;

fn fixture() -> Fixture {
    Fixture::new(&[
        (
            "Cargo.toml",
            "[package]\nname=\"parse-count\"\nversion=\"0.0.0\"\nedition=\"2024\"\n",
        ),
        ("src/lib.rs", "pub mod a;\npub mod b;\npub struct Root;\n"),
        (
            "src/a.rs",
            "pub struct A;\npub fn make() -> A { crate::b::helper(); A }\n",
        ),
        ("src/b.rs", "pub fn helper() {}\n"),
    ])
}

fn metadata(f: &Fixture) -> cargo_metadata::Metadata {
    cargo_metadata::MetadataCommand::new()
        .current_dir(f.path())
        .no_deps()
        .exec()
        .expect("fixture metadata")
}

#[test]
fn capture_parses_each_file_once() {
    let f = fixture();
    let metadata = metadata(&f);
    let before = parse_count();
    let snapshot = discovery::snapshot_metadata(&metadata).expect("snapshot");
    assert_eq!(snapshot.graph().files.len(), FILES);
    assert_eq!(parse_count() - before, FILES);
}

#[test]
fn a_lazy_analysis_parses_each_file_again() {
    let f = fixture();
    let metadata = metadata(&f);
    let snapshot = discovery::snapshot_metadata(&metadata).expect("snapshot");
    let before = parse_count();
    let analysis = Analysis::new(&snapshot).expect("analysis");
    analysis.representations().expect("representations");
    // Today: capture's parse is not reused, so analysis parses every file
    // once more. atlas-6ln lowers this to zero for prepared analyses.
    assert_eq!(parse_count() - before, FILES);
}

#[test]
fn capture_on_worker_threads_counts_each_parse_for_the_caller() {
    const MANY: usize = 40;
    let mut files = vec![
        (
            "Cargo.toml".to_owned(),
            "[package]\nname=\"parse-many\"\nversion=\"0.0.0\"\nedition=\"2024\"\n".to_owned(),
        ),
        (
            "src/lib.rs".to_owned(),
            (1..MANY)
                .map(|i| ["pub mod m", &i.to_string(), ";\n"].concat())
                .collect(),
        ),
    ];
    files.extend((1..MANY).map(|i| (format!("src/m{i}.rs"), format!("pub fn f{i}() {{}}\n"))));
    let files: Vec<_> = files
        .iter()
        .map(|(p, t)| (p.as_str(), t.as_str()))
        .collect();
    let f = Fixture::new(&files);
    let metadata = metadata(&f);
    let before = parse_count();
    let snapshot = discovery::snapshot_metadata(&metadata).expect("snapshot");
    assert_eq!(snapshot.graph().files.len(), MANY);
    assert_eq!(parse_count() - before, MANY);
}
