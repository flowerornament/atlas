//! The syntax-facts cache at the CLI boundary: answers are byte-identical with
//! the cache off, cold or warm; damaged entries are misses; nothing is written
//! inside the inspected project. The fixture's trait impls, one for a foreign
//! type, make call resolution depend on the declared types and impl owners
//! that analysis derives beside the cached facts.
mod support;
use std::{collections::BTreeMap, fs, path::Path, process::Command};
use support::Fixture;

fn project() -> Fixture {
    Fixture::new(&[
        (
            "src/lib.rs",
            "pub mod a;\npub mod b;\n/// Root.\npub struct Root;\n",
        ),
        (
            "src/a.rs",
            "use crate::b::helper;\npub struct A { pub x: u8 }\npub trait Scale { fn scale(&self) -> u8; }\nimpl Scale for A { fn scale(&self) -> u8 { self.x } }\nimpl From<A> for usize { fn from(a: A) -> usize { usize::from(a.scale()) } }\npub fn make() -> A { helper(); let a = A { x: 1 }; let _ = a.scale(); let _ = usize::from(A { x: 2 }); a }\n#[cfg(test)]\nmod tests { #[test] fn t() {} }\n",
        ),
        (
            "src/b.rs",
            "pub fn helper() {}\nimpl crate::Root { pub fn new() -> Self { Self } }\n",
        ),
    ])
}

/// Every file under `dir` with its bytes.
fn tree(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut out = BTreeMap::new();
    let mut stack = vec![dir.to_owned()];
    while let Some(next) = stack.pop() {
        for entry in fs::read_dir(&next).expect("read tree").flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                out.insert(
                    path.strip_prefix(dir)
                        .expect("inside tree")
                        .display()
                        .to_string(),
                    fs::read(&path).expect("read tree file"),
                );
            }
        }
    }
    out
}

fn atlas(project: &Path, cache: &str, dir: &Path, args: &[&str]) -> (Vec<u8>, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_cargo-atlas"))
        .args(args)
        .arg("--root")
        .arg(project)
        .env("ATLAS_CACHE", cache)
        .env("ATLAS_CACHE_DIR", dir)
        .output()
        .expect("run Atlas");
    assert!(
        output.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    (
        output.stdout,
        String::from_utf8(output.stderr).expect("UTF-8 diagnostics"),
    )
}

const COMMANDS: &[&[&str]] = &[
    &["census", "--json"],
    &["dump", "--json"],
    &["module", "src/a.rs", "--items", "--json"],
    &["dependencies", "--json"],
    &["dependencies", "--all-facts", "--json"],
    &["types", "--type", "A", "--json"],
    &[
        "flow",
        "--path",
        "src/a.rs",
        "--function",
        "make",
        "--depth",
        "1",
    ],
];

#[test]
fn answers_are_identical_off_cold_and_warm_and_the_project_is_untouched() {
    let fixture = project();
    let cache = tempfile::tempdir().expect("cache directory");
    let before = tree(fixture.path());
    // stdout plus any evidence a command declares it writes (flow writes
    // target/atlas), which is then removed so each mode starts clean.
    let answer = |mode: &str, args: &[&str]| {
        let (stdout, stderr) = atlas(fixture.path(), mode, cache.path(), args);
        let evidence = fixture.path().join("target/atlas");
        let written = if evidence.exists() {
            let written = tree(&evidence);
            fs::remove_dir_all(fixture.path().join("target")).expect("remove evidence");
            written
        } else {
            BTreeMap::new()
        };
        ((stdout, written), stderr)
    };
    for args in COMMANDS {
        let (off, _) = answer("off", args);
        let (cold, _) = answer("on", args);
        let (warm, stats) = answer("stats", args);
        let (verified, _) = answer("verify", args);
        assert_eq!(cold, off, "{args:?}: cold cache changed the answer");
        assert_eq!(warm, off, "{args:?}: warm cache changed the answer");
        assert_eq!(verified, off, "{args:?}: verify mode changed the answer");
        assert!(stats.contains(" 3 hits, 0 misses"), "{args:?}: {stats}");
    }
    assert_eq!(
        tree(fixture.path()),
        before,
        "the cache wrote into the project"
    );
    assert!(!tree(cache.path()).is_empty(), "the cache stored nothing");
}

#[test]
fn damaged_entries_are_misses() {
    let fixture = project();
    let cache = tempfile::tempdir().expect("cache directory");
    let (expected, _) = atlas(fixture.path(), "off", cache.path(), &["dump", "--json"]);
    atlas(fixture.path(), "on", cache.path(), &["dump", "--json"]);
    for (path, bytes) in tree(cache.path()) {
        if !path
            .rsplit('/')
            .next()
            .is_some_and(|name| name.starts_with('.'))
        {
            let damaged: Vec<u8> = bytes.iter().map(|byte| byte ^ 0x20).collect();
            fs::write(cache.path().join(path), damaged).expect("damage entry");
        }
    }
    let (answer, stats) = atlas(fixture.path(), "stats", cache.path(), &["dump", "--json"]);
    assert_eq!(answer, expected);
    assert!(stats.contains(" 0 hits, 3 misses"), "{stats}");
    let (repaired, stats) = atlas(fixture.path(), "stats", cache.path(), &["dump", "--json"]);
    assert_eq!(repaired, expected);
    assert!(stats.contains(" 3 hits, 0 misses"), "{stats}");
}

#[test]
fn a_cache_directory_inside_the_project_is_neither_created_nor_used() {
    let fixture = project();
    let before = tree(fixture.path());
    let inside = fixture.path().join("target/cache");
    let (_, stats) = atlas(fixture.path(), "stats", &inside, &["census", "--json"]);
    assert!(stats.contains("inside the inspected workspace"), "{stats}");
    assert_eq!(tree(fixture.path()), before);
}

#[test]
fn off_and_no_cache_write_nothing_and_an_unknown_setting_is_refused() {
    let fixture = project();
    let cache = tempfile::tempdir().expect("cache directory");
    atlas(fixture.path(), "off", cache.path(), &["census", "--json"]);
    atlas(
        fixture.path(),
        "on",
        cache.path(),
        &["census", "--json", "--no-cache"],
    );
    assert!(tree(cache.path()).is_empty());
    let refused = Command::new(env!("CARGO_BIN_EXE_cargo-atlas"))
        .args(["census", "--root"])
        .arg(fixture.path())
        .env("ATLAS_CACHE", "sometimes")
        .output()
        .expect("run Atlas");
    assert!(!refused.status.success());
    assert!(String::from_utf8_lossy(&refused.stderr).contains("use on, off, stats or verify"));
}

#[test]
fn a_file_that_does_not_parse_is_never_stored() {
    let fixture = project();
    fixture.write("src/broken.rs", "pub fn broken( {\n");
    let cache = tempfile::tempdir().expect("cache directory");
    let (off, _) = atlas(fixture.path(), "off", cache.path(), &["census", "--json"]);
    atlas(fixture.path(), "on", cache.path(), &["census", "--json"]);
    let (warm, stats) = atlas(fixture.path(), "stats", cache.path(), &["census", "--json"]);
    assert_eq!(warm, off);
    assert!(stats.contains(" 3 hits, 1 misses"), "{stats}");
}
