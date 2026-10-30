//! Cleanup follows the owned extraction even if a legacy reader changes its path.
use atlas_core::revision::Revision;
use std::{fs, path::Path, process::Command};
fn git(root: &Path, args: &[&str]) -> String {
    let result = Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .expect("revision fixture prerequisite must succeed");
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    String::from_utf8(result.stdout)
        .expect("revision fixture prerequisite must succeed")
        .trim()
        .into()
}
fn repository() -> tempfile::TempDir {
    let root = tempfile::tempdir().expect("revision fixture prerequisite must succeed");
    fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname='fixture'\nversion='0.0.0'\nedition='2024'\n",
    )
    .expect("revision fixture prerequisite must succeed");
    fs::create_dir(root.path().join("src")).expect("revision fixture prerequisite must succeed");
    fs::write(root.path().join("src/lib.rs"), "pub struct Captured;\n")
        .expect("revision fixture prerequisite must succeed");
    git(root.path(), &["init", "-q"]);
    git(root.path(), &["add", "."]);
    git(
        root.path(),
        &[
            "-c",
            "user.name=Atlas Test",
            "-c",
            "user.email=atlas@example.invalid",
            "commit",
            "-qm",
            "fixture",
        ],
    );
    root
}
#[test]
fn redirected_public_root_cannot_redirect_cleanup() {
    let repository = repository();
    let decoy = tempfile::tempdir().unwrap();
    let sentinel = decoy.path().join("must-survive");
    fs::write(&sentinel, "owned by another caller").unwrap();
    let mut revision = Revision::read(repository.path(), "HEAD").unwrap();
    let allocated = revision.root.clone();
    assert!(allocated.join("Cargo.toml").is_file());
    revision.root = decoy.path().to_owned();
    drop(revision);
    assert_eq!(
        fs::read_to_string(sentinel).unwrap(),
        "owned by another caller"
    );
    assert!(!allocated.exists(), "the allocation must still be cleaned");
    assert!(repository.path().join("Cargo.toml").is_file());
}
#[test]
fn independent_revision_leases_preserve_exact_source_and_clean_separately() {
    let repository = repository();
    let first = Revision::read(repository.path(), "HEAD").unwrap();
    let second = Revision::read(repository.path(), "HEAD").unwrap();
    let first_path = first.root.clone();
    let second_path = second.root.clone();
    assert_ne!(first_path, second_path);
    assert_eq!(first.commit, git(repository.path(), &["rev-parse", "HEAD"]));
    assert_eq!(first.commit, second.commit);
    fs::write(
        repository.path().join("src/lib.rs"),
        "changed working source",
    )
    .unwrap();
    assert_eq!(
        fs::read_to_string(first_path.join("src/lib.rs")).unwrap(),
        "pub struct Captured;\n"
    );
    drop(first);
    assert!(!first_path.exists());
    assert!(second_path.join("Cargo.toml").is_file());
    drop(second);
    assert!(!second_path.exists());
    assert_eq!(
        fs::read_to_string(repository.path().join("src/lib.rs")).unwrap(),
        "changed working source"
    );
}

#[test]
fn capture_verification_refuses_dirty_and_untracked_source_without_changing_it() {
    use atlas_core::{discovery::snapshot_workspace, revision::GitStore};
    let root = repository();
    let store = GitStore::discover(root.path()).unwrap();
    let revision = store.revision("HEAD").unwrap();
    let captured = snapshot_workspace(root.path()).unwrap();
    store.verify_capture(&revision, &captured).unwrap();
    fs::write(root.path().join("src/lib.rs"), "pub struct Changed;\n").unwrap();
    let dirty = snapshot_workspace(root.path()).unwrap();
    assert!(store.verify_capture(&revision, &dirty).is_err());
    assert_eq!(
        fs::read_to_string(root.path().join("src/lib.rs")).unwrap(),
        "pub struct Changed;\n"
    );
    fs::write(root.path().join("src/lib.rs"), "pub struct Captured;\n").unwrap();
    assert!(
        store.verify_capture(&revision, &dirty).is_err(),
        "restoring disk bytes cannot relabel an already changed capture"
    );
    fs::write(root.path().join("src/untracked.rs"), "pub struct Hidden;\n").unwrap();
    let untracked = snapshot_workspace(root.path()).unwrap();
    assert!(store.verify_capture(&revision, &untracked).is_err());
    assert!(root.path().join("src/untracked.rs").is_file());
}
#[test]
fn a_nested_workspace_cannot_borrow_its_ancestor_repository_identity() {
    let root = repository();
    let nested = root.path().join("nested");
    fs::create_dir(&nested).unwrap();
    assert!(atlas_core::revision::GitStore::discover(&nested).is_err());
}
#[test]
fn compatibility_fields_cannot_relabel_the_owned_extraction() {
    let root = repository();
    let store = atlas_core::revision::GitStore::discover(root.path()).unwrap();
    let mut revision = store.revision("HEAD").unwrap();
    let snapshot = atlas_core::discovery::snapshot_workspace(root.path()).unwrap();
    revision.commit = "different identity".into();
    assert!(store.verify_capture(&revision, &snapshot).is_err());
    assert_ne!(revision.commit_id(), revision.commit);
}
#[cfg(unix)]
#[test]
fn revision_files_refuse_symlink_custody() {
    let root = repository();
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("policy"), "external").unwrap();
    std::os::unix::fs::symlink(outside.path().join("policy"), root.path().join("policy")).unwrap();
    git(root.path(), &["add", "policy"]);
    git(
        root.path(),
        &[
            "-c",
            "user.name=Atlas Test",
            "-c",
            "user.email=atlas@example.invalid",
            "commit",
            "-qm",
            "link",
        ],
    );
    let revision = Revision::read(root.path(), "HEAD").unwrap();
    assert!(revision.file("policy").is_err());
}

#[test]
#[ignore = "requires JJ; run explicitly to qualify non-mutating JJ revision custody"]
fn jj_resolution_and_verification_never_snapshot_working_edits() {
    let root = repository();
    let initialized = Command::new("jj")
        .current_dir(root.path())
        .args(["git", "init", "--colocate"])
        .output()
        .unwrap();
    assert!(
        initialized.status.success(),
        "{}",
        String::from_utf8_lossy(&initialized.stderr)
    );
    let store = atlas_core::revision::GitStore::discover(root.path()).unwrap();
    let id = store.current().unwrap();
    fs::write(root.path().join("src/lib.rs"), "pub struct Dirty;\n").unwrap();
    let revision = store.revision("@").unwrap();
    assert_eq!(revision.commit_id(), id);
    assert_eq!(
        fs::read_to_string(revision.path().join("src/lib.rs")).unwrap(),
        "pub struct Captured;\n"
    );
    let snapshot = atlas_core::discovery::snapshot_workspace(root.path()).unwrap();
    assert!(store.verify_capture(&revision, &snapshot).is_err());
    assert_eq!(store.current().unwrap(), id);
    assert_eq!(
        fs::read_to_string(root.path().join("src/lib.rs")).unwrap(),
        "pub struct Dirty;\n"
    );
}

#[cfg(unix)]
#[test]
fn custody_compares_gits_executable_flag_not_untracked_permission_bits() {
    use std::os::unix::fs::PermissionsExt;
    let root = repository();
    git(root.path(), &["config", "core.filemode", "true"]);
    let file = root.path().join("src/lib.rs");
    let mode = fs::metadata(&file).unwrap().permissions().mode();
    fs::set_permissions(&file, fs::Permissions::from_mode(mode ^ 0o010)).unwrap();
    assert!(
        git(root.path(), &["diff", "--raw"]).is_empty(),
        "Git does not record this group-execute-bit change"
    );
    let store = atlas_core::revision::GitStore::discover(root.path()).unwrap();
    let revision = store.revision("HEAD").unwrap();
    let snapshot = atlas_core::discovery::snapshot_workspace(root.path()).unwrap();
    store.verify_capture(&revision, &snapshot).unwrap();
}
