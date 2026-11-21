//! A module read from a `cfg_attr(..., path = "...")` file is mounted from
//! every candidate: the unconditional file and each conditional one. Atlas
//! evaluates no cfg, so a name both define is ambiguous, never the default
//! file's. The first case is the 2026-11-21 review's fixture.
mod support;
use atlas_lib::{analysis::Analysis, discovery};
use support::Fixture;

fn facts(
    files: &[(&str, &str)],
) -> (
    atlas_lib::model::SourceGraph,
    atlas_lib::representations::Facts,
) {
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
    (snapshot.graph().clone(), facts)
}
const MANIFEST: &str = "[package]\nname=\"cfg-path\"\nversion=\"0.1.0\"\nedition=\"2024\"\n";

#[test]
fn a_conditional_path_is_a_candidate_beside_the_default_file() {
    let (graph, facts) = facts(&[
        ("Cargo.toml", MANIFEST),
        (
            "src/lib.rs",
            "#[cfg_attr(all(), path = \"alternate.rs\")]\nmod chosen;\npub fn caller() { chosen::hit(); }\n",
        ),
        ("src/alternate.rs", "pub fn hit() {}\n"),
        ("src/chosen.rs", "pub fn hit() {}\n"),
    ]);
    for path in ["src/alternate.rs", "src/chosen.rs"] {
        let file = graph.files.iter().find(|f| f.path == path).expect(path);
        assert!(
            file.mounts.iter().any(|m| m.modules == ["chosen"]),
            "{path}"
        );
    }
    assert!(facts.calls.is_empty(), "{:?}", facts.calls);
    let [unknown] = facts.unknown_calls.as_slice() else {
        panic!("{:?}", facts.unknown_calls);
    };
    assert!(unknown.reason.contains("ambiguous"), "{}", unknown.reason);
}

#[test]
fn conditional_paths_alone_mount_each_file_and_nothing_is_missing() {
    let (graph, facts) = facts(&[
        ("Cargo.toml", MANIFEST),
        (
            "src/lib.rs",
            "#[cfg_attr(unix, path = \"os/unix.rs\")]\n#[cfg_attr(windows, path = \"../platform/windows.rs\")]\nmod os;\npub fn caller() { os::only_unix(); }\n",
        ),
        ("src/os/unix.rs", "pub fn only_unix() {}\n"),
        ("platform/windows.rs", "pub fn only_windows() {}\n"),
    ]);
    for path in ["src/os/unix.rs", "platform/windows.rs"] {
        let file = graph.files.iter().find(|f| f.path == path).expect(path);
        assert!(file.mounts.iter().any(|m| m.modules == ["os"]), "{path}");
    }
    assert!(
        !graph
            .diagnostics
            .iter()
            .any(|d| d.code == atlas_lib::model::DiagnosticCode::MountMissing),
        "{:?}",
        graph.diagnostics
    );
    // One candidate defines the name: that is its only reading.
    assert_eq!(facts.calls.len(), 1, "{:?}", facts.unknown_calls);
}
