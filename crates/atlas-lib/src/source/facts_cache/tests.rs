use super::*;
use std::time::SystemTime;

const TEXT: &str = "/// A doc.\npub struct A { pub(crate) x: u8 }\nuse std::{fmt, io::Read as _};\n#[cfg(test)]\nmod tests { #[test] fn t() {} }\nimpl A { pub fn f(&self) {} }\n";

fn facts_of(path: &Path, text: &str) -> (RustSourceProjection, syntax::Facts) {
    let projection = RustSourceProjection::of(text.to_owned());
    let facts = syntax::read(path, &projection).expect("fixture parses");
    (projection, facts)
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("read corpus directory").flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

/// Every source file of this repository survives the wire form unchanged.
#[test]
fn wire_form_round_trips_the_repository() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    rust_files(&manifest.join("src"), &mut files);
    rust_files(&manifest.join("../../src"), &mut files);
    assert!(files.len() > 50, "corpus too small: {}", files.len());
    for file in files {
        let text = fs::read_to_string(&file).expect("read corpus file");
        let projection = RustSourceProjection::of(text);
        let Ok(facts) = syntax::read(&file, &projection) else {
            continue;
        };
        let payload = encode_payload(&facts).expect("encode facts");
        let mut entry = MAGIC.to_vec();
        entry.extend_from_slice(&Sha256::digest(&payload));
        entry.extend_from_slice(&payload);
        let (_, decoded) = decode(&entry).expect("decode own entry");
        assert_eq!(decoded.outline, facts.outline, "{}", file.display());
        assert_eq!(
            encode_payload(&decoded),
            Some(payload),
            "{}",
            file.display()
        );
    }
}

#[test]
fn a_second_read_of_the_same_bytes_hits_without_parsing() {
    let dir = tempfile::tempdir().expect("cache directory");
    let cache = FactsCache::open_for_build(dir.path(), b"build", FactsCacheMode::Reuse)
        .expect("open cache");
    let path = Path::new("src/a.rs");
    let (projection, expected) = facts_of(path, TEXT);
    let before = syntax::parse_count();
    let first = cache.read(path, &projection).expect("miss parses");
    assert_eq!(syntax::parse_count() - before, 1);
    assert!(cache.entry(path, TEXT).is_file(), "a miss fills its entry");
    let second = cache.read(path, &projection).expect("hit");
    assert_eq!(syntax::parse_count() - before, 1, "a hit parses nothing");
    assert_eq!(first.outline, expected.outline);
    assert_eq!(second.outline, expected.outline);
    assert_eq!(encode_payload(&second), encode_payload(&expected));
    // Other bytes, or the same bytes at another path, are other entries.
    let edited = TEXT.replace("x: u8", "x: u16");
    assert_ne!(cache.entry(path, &edited), cache.entry(path, TEXT));
    assert_ne!(
        cache.entry(Path::new("src/b.rs"), TEXT),
        cache.entry(path, TEXT)
    );
}

#[test]
fn another_executable_never_reads_these_entries() {
    let dir = tempfile::tempdir().expect("cache directory");
    let path = Path::new("src/a.rs");
    let (projection, _) = facts_of(path, TEXT);
    let one = FactsCache::open_for_build(dir.path(), b"one", FactsCacheMode::Reuse)
        .expect("open first build");
    one.read(path, &projection).expect("fill first build");
    let two = FactsCache::open_for_build(dir.path(), b"two", FactsCacheMode::Reuse)
        .expect("open second build");
    assert_ne!(one.entries, two.entries);
    let before = syntax::parse_count();
    two.read(path, &projection).expect("second build misses");
    assert_eq!(syntax::parse_count() - before, 1, "a new build starts cold");
}

#[test]
fn a_damaged_entry_is_a_miss_and_is_replaced() {
    let dir = tempfile::tempdir().expect("cache directory");
    let cache = FactsCache::open_for_build(dir.path(), b"build", FactsCacheMode::Reuse)
        .expect("open cache");
    let path = Path::new("src/a.rs");
    let (projection, expected) = facts_of(path, TEXT);
    cache.read(path, &projection).expect("fill");
    let entry = cache.entry(path, TEXT);
    let whole = fs::read(&entry).expect("read entry");
    let mut flipped = whole.clone();
    let last = flipped.len() - 2;
    flipped[last] ^= 1;
    for damaged in [
        Vec::new(),
        b"ATLASFC1".to_vec(),
        whole[..whole.len() / 2].to_vec(),
        flipped,
    ] {
        fs::write(&entry, &damaged).expect("damage entry");
        let before = syntax::parse_count();
        let facts = cache.read(path, &projection).expect("damaged entry parses");
        assert_eq!(
            syntax::parse_count() - before,
            1,
            "a damaged entry is a miss"
        );
        assert_eq!(encode_payload(&facts), encode_payload(&expected));
        assert_eq!(fs::read(&entry).expect("reread entry"), whole);
    }
}

#[test]
fn verify_mode_reports_an_entry_that_differs_from_a_parse() {
    let dir = tempfile::tempdir().expect("cache directory");
    let cache = FactsCache::open_for_build(dir.path(), b"build", FactsCacheMode::Verify)
        .expect("open cache");
    let path = Path::new("src/verify_mode_mismatch.rs");
    let (projection, expected) = facts_of(path, TEXT);
    // A well-formed entry holding the facts of other bytes.
    let (_, other) = facts_of(path, "pub fn other() {}\n");
    store(&cache.entry(path, TEXT), &other).expect("plant entry");
    let facts = cache.read(path, &projection).expect("verify parses");
    assert_eq!(encode_payload(&facts), encode_payload(&expected));
    assert!(
        FactsCache::stats()
            .mismatches
            .contains(&path.display().to_string())
    );
}

#[test]
fn a_cache_inside_the_workspace_is_not_used() {
    let workspace = tempfile::tempdir().expect("workspace");
    let root = workspace.path().canonicalize().expect("canonical root");
    let inside = FactsCache::open_for_build(&root.join("cache"), b"build", FactsCacheMode::Reuse)
        .expect("open cache");
    assert!(!inside.serves(&root));
    let elsewhere = tempfile::tempdir().expect("cache directory");
    let outside = FactsCache::open_for_build(elsewhere.path(), b"build", FactsCacheMode::Reuse)
        .expect("open cache");
    assert!(outside.serves(&root));
}

#[test]
fn pruning_removes_builds_unused_for_a_week() {
    let dir = tempfile::tempdir().expect("cache directory");
    let stale = dir.path().join("facts/0123456789abcdef");
    let recent = dir.path().join("facts/fedcba9876543210");
    for build in [&stale, &recent] {
        fs::create_dir_all(build.join("aa")).expect("build directory");
        fs::write(build.join("aa/entry"), b"x").expect("entry");
        fs::write(build.join(".used"), b"").expect("used marker");
    }
    fs::File::options()
        .write(true)
        .open(stale.join(".used"))
        .and_then(|file| file.set_modified(SystemTime::UNIX_EPOCH))
        .expect("age marker");
    FactsCache::open_for_build(dir.path(), b"build", FactsCacheMode::Reuse).expect("open cache");
    assert!(!stale.exists(), "a build unused for a week is removed");
    assert!(recent.join("aa/entry").exists(), "a recent build is kept");
}
