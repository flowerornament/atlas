//! The command line's syntax-facts cache: on by default, in the user cache
//! directory, so that a run reuses the facts of files whose bytes an earlier
//! run already read. Answers are identical with the cache on, off or cold; it
//! never changes output, so its use is reported only when asked for.
//!
//! `ATLAS_CACHE` is `on` (the default), `off`, `stats` (on, plus a count on
//! stderr) or `verify` (parse every file anyway and fail the run if an entry
//! differs). `ATLAS_CACHE_DIR` replaces the directory; `--no-cache` is `off`.
//! A directory inside the inspected workspace is never created or used.
use atlas_lib::source::{FactsCache, FactsCacheMode};
use cargo_atlas::Result;
use std::path::{Path, PathBuf};

pub(super) struct Session {
    stats: bool,
    verify: bool,
    /// Where entries live, or why the run is uncached.
    place: std::result::Result<PathBuf, String>,
}

/// Install the cache this run's settings ask for, for the workspace at the
/// canonical `root`. A directory that cannot be opened leaves the run
/// uncached rather than failing it.
pub(super) fn open(no_cache: bool, root: &Path) -> Result<Session> {
    let setting = std::env::var("ATLAS_CACHE").unwrap_or_default();
    let (mode, stats) = match setting.as_str() {
        "" | "on" => (Some(FactsCacheMode::Reuse), false),
        "stats" => (Some(FactsCacheMode::Reuse), true),
        "verify" => (Some(FactsCacheMode::Verify), false),
        "off" => (None, false),
        other => {
            return Err(format!("ATLAS_CACHE is {other:?}; use on, off, stats or verify").into());
        }
    };
    let mode = mode.filter(|_| !no_cache);
    let dir = std::env::var_os("ATLAS_CACHE_DIR")
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .or_else(FactsCache::user_dir);
    let place = match (mode, dir) {
        (None, _) => Err("off".to_owned()),
        (Some(_), None) => Err("no user cache directory; set ATLAS_CACHE_DIR".to_owned()),
        (Some(_), Some(dir)) if resolved(&dir).starts_with(root) => Err(format!(
            "not used: {} is inside the inspected workspace",
            dir.display()
        )),
        (Some(mode), Some(dir)) => match FactsCache::open(&dir, mode) {
            Ok(cache) => {
                let opened = cache.dir().to_owned();
                cache
                    .install()
                    .map_err(|_| "a cache was already installed".to_owned())
                    .map(|()| opened)
            }
            Err(error) => Err(format!("unavailable at {}: {error}", dir.display())),
        },
    };
    Ok(Session {
        stats,
        verify: mode == Some(FactsCacheMode::Verify),
        place,
    })
}

/// `dir` made absolute through its nearest existing ancestor, without
/// creating anything.
fn resolved(dir: &Path) -> PathBuf {
    let absolute = std::path::absolute(dir).unwrap_or_else(|_| dir.to_owned());
    let mut missing = Vec::new();
    let mut existing = absolute.as_path();
    loop {
        if let Ok(found) = existing.canonicalize() {
            return missing
                .iter()
                .rev()
                .fold(found, |path, part| path.join(part));
        }
        let (Some(parent), Some(name)) = (existing.parent(), existing.file_name()) else {
            return absolute;
        };
        missing.push(name.to_owned());
        existing = parent;
    }
}

impl Session {
    /// Report the cache's use if asked, and in verify mode turn a differing
    /// entry into the run's failure.
    pub(super) fn finish(self, result: Result<()>) -> Result<()> {
        let counts = FactsCache::stats();
        if self.stats {
            match &self.place {
                Ok(dir) => eprintln!(
                    "atlas: facts cache: {} hits, {} misses, {} unwritten, {} files read uncached inside the workspace ({})",
                    counts.hits,
                    counts.misses,
                    counts.unwritten,
                    counts.bypassed,
                    dir.display()
                ),
                Err(why) => eprintln!("atlas: facts cache: {why}"),
            }
        }
        if self.verify && !counts.mismatches.is_empty() && result.is_ok() {
            return Err(format!(
                "facts cache entries differ from a fresh parse: {}",
                counts.mismatches.join(", ")
            )
            .into());
        }
        result
    }
}
