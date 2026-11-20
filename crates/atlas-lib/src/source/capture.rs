//! Reading and capturing many files at once: reads and facts-cache lookups in
//! order on the calling thread, then parses of what the cache did not hold on
//! worker threads.
//!
//! Each file's capture depends only on its path and bytes, so the result is
//! the same as a serial loop: results go into a map keyed by relative path,
//! and a read failure is reported for the first failing file in the given
//! order, before anything is parsed. Lookups stay serial because a hit is
//! cheaper than a thread's share of coordination.
use super::{Captured, RustSourceProjection, facts_cache, syntax};
use std::{
    collections::BTreeMap,
    fs, io,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering::Relaxed},
    },
};

/// Below this many parses per worker, starting a thread costs more than it saves.
const PARSES_PER_WORKER: usize = 8;

/// Capture one file through the installed facts cache, if any.
pub(crate) fn capture(root: &Path, rel: &str, text: String) -> Captured {
    let path = Path::new(rel);
    match facts_cache::lookup(root, path, text) {
        facts_cache::Lookup::Found(captured) => captured,
        facts_cache::Lookup::Parse { projection, entry } => {
            facts_cache::parse(path, projection, entry.as_deref())
        }
    }
}

/// Read and capture every `(relative, absolute)` file not yet in `captured`.
/// Parses done on workers are counted for the calling thread.
pub(crate) fn capture_files(
    root: &Path,
    files: &[(String, PathBuf)],
    captured: &mut BTreeMap<String, Arc<Captured>>,
) -> io::Result<()> {
    let mut misses: Vec<(&str, RustSourceProjection, Option<PathBuf>)> = Vec::new();
    for (rel, path) in files {
        if captured.contains_key(rel) {
            continue;
        }
        match facts_cache::lookup(root, Path::new(rel), fs::read_to_string(path)?) {
            facts_cache::Lookup::Found(source) => {
                captured.insert(rel.clone(), Arc::new(source));
            }
            facts_cache::Lookup::Parse { projection, entry } => {
                misses.push((rel, projection, entry));
            }
        }
    }
    let workers = std::thread::available_parallelism()
        .map_or(1, std::num::NonZero::get)
        .min(misses.len().div_ceil(PARSES_PER_WORKER));
    if workers <= 1 {
        for (rel, projection, entry) in misses {
            let source = facts_cache::parse(Path::new(rel), projection, entry.as_deref());
            captured.insert(rel.to_owned(), Arc::new(source));
        }
        return Ok(());
    }
    let queue = Mutex::new(misses);
    let done = Mutex::new(Vec::new());
    let parses = AtomicUsize::new(0);
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                let before = syntax::parse_count();
                let mut local = Vec::new();
                loop {
                    // Take one file and release the queue before parsing it.
                    let next = queue.lock().expect("a capture worker panicked").pop();
                    let Some((rel, projection, entry)) = next else {
                        break;
                    };
                    let source = facts_cache::parse(Path::new(rel), projection, entry.as_deref());
                    local.push((rel, source));
                }
                parses.fetch_add(syntax::parse_count() - before, Relaxed);
                done.lock()
                    .expect("a capture worker panicked")
                    .extend(local);
            });
        }
    });
    syntax::count_parses(parses.into_inner());
    for (rel, source) in done.into_inner().expect("a capture worker panicked") {
        captured.insert(rel.to_owned(), Arc::new(source));
    }
    Ok(())
}
