//! A per-file cache of syntax facts, shared across Atlas runs.
//!
//! Reading one file's syntax facts is a pure function of its relative path and
//! its exact bytes. An entry is keyed by a hash of those two and of the running
//! executable, so a hit holds the facts of the bytes this run just read, never
//! of an earlier read: there is no freshness question to get wrong. Everything
//! after per-file facts (cross-file relationships, Cargo contexts, queries)
//! still runs live. Any unreadable, corrupt or unknown entry is a miss.
//!
//! The cache is off unless a caller installs one with [`FactsCache::install`].
//! It is never consulted when its directory lies inside the inspected
//! workspace, so reading a project never writes into it.
mod wire;

use super::{Captured, RustSourceProjection, Source, SyntaxError, syntax};
use sha2::{Digest, Sha256};
use std::{
    fs, io,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicUsize, Ordering::Relaxed},
    },
    time::{Duration, SystemTime},
};

const MAGIC: &[u8; 8] = b"ATLASFC1";
const HEADER: usize = MAGIC.len() + 32;
/// The whole cache directory is pruned back below three quarters of this.
const CAP_BYTES: u64 = 1 << 30;
/// Another build's entries are removed once unused for this long.
const UNUSED_BUILD: Duration = Duration::from_hours(7 * 24);
/// Pruning runs at most this often, so a run normally pays one `stat`.
const PRUNE_INTERVAL: Duration = Duration::from_hours(1);

/// Whether a hit is trusted or checked against a fresh parse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum FactsCacheMode {
    /// A hit is used without parsing.
    Reuse,
    /// Every file is parsed anyway; a hit that differs from the parse is
    /// recorded in [`FactsCacheStats::mismatches`] and the parse is used.
    Verify,
}

/// The syntax-facts cache of one executable, under one directory.
#[derive(Debug)]
pub struct FactsCache {
    /// The cache directory that holds every build's entries, absolute and
    /// resolved through its nearest existing ancestor; it may not exist yet.
    base: PathBuf,
    /// `base/facts/<build>`: this executable's entries.
    entries: PathBuf,
    build: [u8; 32],
    mode: FactsCacheMode,
    /// Whether the directories exist, created and pruned on first use by a
    /// workspace this cache may serve; the error otherwise.
    prepared: OnceLock<Result<(), String>>,
}

/// What the installed cache did in this process so far.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct FactsCacheStats {
    pub hits: usize,
    pub misses: usize,
    /// Entries a miss could not store; the run continued uncached.
    pub unwritten: usize,
    /// Files read without the cache because its directory lies inside the
    /// inspected workspace.
    pub bypassed: usize,
    /// Verify mode only: files whose entry differed from a fresh parse.
    pub mismatches: Vec<String>,
    /// Why the installed cache could not be used, when its directory could
    /// not be created on first use.
    pub unavailable: Option<String>,
}

static INSTALLED: OnceLock<FactsCache> = OnceLock::new();
static HITS: AtomicUsize = AtomicUsize::new(0);
static MISSES: AtomicUsize = AtomicUsize::new(0);
static UNWRITTEN: AtomicUsize = AtomicUsize::new(0);
static BYPASSED: AtomicUsize = AtomicUsize::new(0);
static MISMATCHES: Mutex<Vec<String>> = Mutex::new(Vec::new());

impl FactsCache {
    /// Open the cache under `dir` for the running executable. Nothing is
    /// written until a capture of a workspace that does not contain `dir`
    /// first uses it: then the directory is created (owner-only on Unix) and
    /// old entries are pruned, at most hourly.
    ///
    /// The build identity is a hash of the executable file, so any rebuild,
    /// including a development build of an unchanged version, starts cold.
    pub fn open(dir: &Path, mode: FactsCacheMode) -> io::Result<Self> {
        Self::open_for_build(dir, &executable_identity()?, mode)
    }

    /// Open the cache for an explicit build identity. For tests: real callers
    /// use [`FactsCache::open`], whose identity is the executable itself.
    #[doc(hidden)]
    pub fn open_for_build(dir: &Path, build: &[u8], mode: FactsCacheMode) -> io::Result<Self> {
        let build: [u8; 32] = Sha256::digest(build).into();
        let base = resolved(dir)?;
        let entries = base.join("facts").join(hex(&build[..8]));
        Ok(Self {
            base,
            entries,
            build,
            mode,
            prepared: OnceLock::new(),
        })
    }

    /// Create the directories and prune, once; `false` if they cannot be made.
    pub(super) fn prepare(&self) -> bool {
        self.prepared
            .get_or_init(|| {
                create_private_dir(&self.entries).map_err(|error| {
                    format!("cannot create {}: {error}", self.entries.display())
                })?;
                self.mark_used_and_prune();
                Ok(())
            })
            .is_ok()
    }

    /// Whether `dir` lies inside the workspace at the canonical `root`, so
    /// that capturing it never uses this cache.
    pub fn is_inside(&self, root: &Path) -> bool {
        self.base.starts_with(root)
    }

    /// Use this cache for every capture in this process. Only the first
    /// install takes effect; a later one is returned unchanged.
    pub fn install(self) -> Result<(), Self> {
        INSTALLED.set(self)
    }

    /// The cache directory, as opened.
    pub fn dir(&self) -> &Path {
        &self.base
    }

    /// The user cache directory Atlas uses by default: `~/Library/Caches/atlas`
    /// on macOS, `%LOCALAPPDATA%\atlas` on Windows, and otherwise
    /// `$XDG_CACHE_HOME/atlas` or `~/.cache/atlas`. `None` when the platform
    /// variable it needs is unset.
    pub fn user_dir() -> Option<PathBuf> {
        let var = |name| std::env::var_os(name).filter(|value| !value.is_empty());
        if cfg!(target_os = "macos") {
            return var("HOME").map(|home| Path::new(&home).join("Library/Caches/atlas"));
        }
        if cfg!(windows) {
            return var("LOCALAPPDATA").map(|dir| Path::new(&dir).join("atlas"));
        }
        if let Some(xdg) = var("XDG_CACHE_HOME").map(PathBuf::from)
            && xdg.is_absolute()
        {
            return Some(xdg.join("atlas"));
        }
        var("HOME").map(|home| Path::new(&home).join(".cache/atlas"))
    }

    /// Counters of the installed cache in this process.
    pub fn stats() -> FactsCacheStats {
        FactsCacheStats {
            hits: HITS.load(Relaxed),
            misses: MISSES.load(Relaxed),
            unwritten: UNWRITTEN.load(Relaxed),
            bypassed: BYPASSED.load(Relaxed),
            mismatches: MISMATCHES
                .lock()
                .map(|found| found.clone())
                .unwrap_or_default(),
            unavailable: Self::installed()
                .and_then(|cache| cache.prepared.get())
                .and_then(|prepared| prepared.clone().err()),
        }
    }

    pub(super) fn installed() -> Option<&'static Self> {
        INSTALLED.get()
    }

    /// Whether reading a workspace at `root` may use this cache: never when
    /// the cache directory is inside it.
    pub(super) fn serves(&self, root: &Path) -> bool {
        let serves = !self.is_inside(root);
        if !serves {
            BYPASSED.fetch_add(1, Relaxed);
        }
        serves
    }

    /// The facts of `text`, from the entry for these exact bytes or from a
    /// parse that then fills it. A file that does not parse is not cached.
    #[cfg(test)]
    pub(super) fn read(
        &self,
        path: &Path,
        projection: &RustSourceProjection,
    ) -> Result<syntax::Facts, SyntaxError> {
        let entry = self.entry(path, projection.text());
        match self.lookup(&entry, path, projection)? {
            Some(facts) => Ok(facts),
            None => fill(&entry, path, projection),
        }
    }

    /// The facts stored for these exact bytes, or `None` on a miss. In
    /// verify mode a hit is parsed too, and a differing entry is recorded and
    /// answered with the parse.
    fn lookup(
        &self,
        entry: &Path,
        path: &Path,
        projection: &RustSourceProjection,
    ) -> Result<Option<syntax::Facts>, SyntaxError> {
        let Some(bytes) = fs::read(entry).ok() else {
            return Ok(None);
        };
        let Some((payload, facts)) = decode(&bytes) else {
            return Ok(None);
        };
        HITS.fetch_add(1, Relaxed);
        if self.mode == FactsCacheMode::Reuse {
            return Ok(Some(facts));
        }
        let parsed = syntax::read(path, projection)?;
        if encode_payload(&parsed).as_deref() == Some(payload) {
            return Ok(Some(facts));
        }
        if let Ok(mut found) = MISMATCHES.lock() {
            found.push(path.to_string_lossy().into_owned());
        }
        Ok(Some(parsed))
    }

    fn entry(&self, path: &Path, text: &str) -> PathBuf {
        let mut key = Sha256::new();
        key.update(b"atlas syntax facts\0");
        key.update(self.build);
        key.update(path.as_os_str().as_encoded_bytes());
        key.update(b"\0");
        key.update(text.as_bytes());
        let key = hex(&key.finalize());
        self.entries.join(&key[..2]).join(&key[2..])
    }

    /// Mark this build's entries as in use and, at most hourly, remove other
    /// builds unused for a week, then prune the whole directory below its
    /// cap: other builds least recently used first, then this build's oldest
    /// entries. Failures are ignored; a concurrent run may prune too.
    fn mark_used_and_prune(&self) {
        touch_if_older(&self.entries.join(".used"), PRUNE_INTERVAL);
        let facts = self.base.join("facts");
        if !touch_if_older(&facts.join(".pruned"), PRUNE_INTERVAL) {
            return;
        }
        let mut builds: Vec<(SystemTime, PathBuf, u64)> = Vec::new();
        let mut current = 0;
        for dir in fs::read_dir(&facts).into_iter().flatten().flatten() {
            let path = dir.path();
            if !path.is_dir() {
                continue;
            }
            if path == self.entries {
                current = tree_size(&path);
                continue;
            }
            let used = modified(&path.join(".used")).unwrap_or(SystemTime::UNIX_EPOCH);
            if used.elapsed().is_ok_and(|age| age > UNUSED_BUILD) {
                let _ = fs::remove_dir_all(&path);
            } else {
                let size = tree_size(&path);
                builds.push((used, path, size));
            }
        }
        let mut total = current + builds.iter().map(|(.., size)| size).sum::<u64>();
        if total <= CAP_BYTES {
            return;
        }
        let target = CAP_BYTES / 4 * 3;
        builds.sort();
        for (_, path, size) in builds {
            if total <= target {
                return;
            }
            if fs::remove_dir_all(&path).is_ok() {
                total -= size;
            }
        }
        let mut entries = Vec::new();
        for shard in fs::read_dir(&self.entries).into_iter().flatten().flatten() {
            for entry in fs::read_dir(shard.path()).into_iter().flatten().flatten() {
                if let Ok(meta) = entry.metadata()
                    && meta.is_file()
                {
                    let written = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                    entries.push((written, entry.path(), meta.len()));
                }
            }
        }
        entries.sort();
        for (_, path, size) in entries {
            if total <= target {
                return;
            }
            if fs::remove_file(&path).is_ok() {
                total -= size;
            }
        }
    }
}

/// What a capture still needs after looking in the installed cache.
pub(super) enum Lookup {
    Found(Captured),
    /// Parse these bytes; `entry` is where to store the facts, when a cache
    /// serves this workspace.
    Parse {
        projection: RustSourceProjection,
        entry: Option<PathBuf>,
    },
}

/// Look up one file's facts in the installed cache, when it may serve `root`.
pub(super) fn lookup(root: &Path, path: &Path, text: String) -> Lookup {
    let projection = RustSourceProjection::of(text);
    let Some(cache) = FactsCache::installed()
        .filter(|cache| cache.serves(root))
        .filter(|cache| cache.prepare())
    else {
        return Lookup::Parse {
            projection,
            entry: None,
        };
    };
    let entry = cache.entry(path, projection.text());
    match cache.lookup(&entry, path, &projection) {
        Ok(None) => Lookup::Parse {
            projection,
            entry: Some(entry),
        },
        Ok(Some(facts)) => Lookup::Found(captured(projection, Ok(facts))),
        Err(error) => Lookup::Found(captured(projection, Err(error))),
    }
}

/// Parse the bytes a lookup missed, storing their facts at `entry`.
pub(super) fn parse(
    path: &Path,
    projection: RustSourceProjection,
    entry: Option<&Path>,
) -> Captured {
    let facts = match entry {
        Some(entry) => fill(entry, path, &projection),
        None => syntax::read(path, &projection),
    };
    captured(projection, facts)
}

fn captured(
    projection: RustSourceProjection,
    facts: Result<syntax::Facts, SyntaxError>,
) -> Captured {
    match facts {
        Ok(facts) => Captured::Parsed(Arc::new(Source::from_facts(projection, facts))),
        Err(error) => Captured::Invalid {
            text: projection.text().to_owned(),
            error,
        },
    }
}

/// Parse after a miss and store the facts; a failed store leaves the run
/// uncached for this file.
fn fill(
    entry: &Path,
    path: &Path,
    projection: &RustSourceProjection,
) -> Result<syntax::Facts, SyntaxError> {
    MISSES.fetch_add(1, Relaxed);
    let parsed = syntax::read(path, projection)?;
    if store(entry, &parsed).is_err() {
        UNWRITTEN.fetch_add(1, Relaxed);
    }
    Ok(parsed)
}

fn encode_payload(facts: &syntax::Facts) -> Option<Vec<u8>> {
    serde_json::to_vec(&wire::WireFacts::of(facts)).ok()
}

/// An entry is the magic, a SHA-256 of the payload, then the JSON payload.
/// Returns the payload with the facts it decodes to, or `None` for anything
/// else: a short, truncated, altered or unknown entry is a miss.
fn decode(bytes: &[u8]) -> Option<(&[u8], syntax::Facts)> {
    let (magic, rest) = bytes.split_at_checked(MAGIC.len())?;
    let (digest, payload) = rest.split_at_checked(HEADER - MAGIC.len())?;
    if magic != MAGIC || Sha256::digest(payload).as_slice() != digest {
        return None;
    }
    let facts = serde_json::from_slice::<wire::WireFacts>(payload)
        .ok()?
        .into_facts()?;
    Some((payload, facts))
}

/// Write an entry through a temporary file and a rename, so a reader sees a
/// whole entry or none. Concurrent writers of one key write the same bytes.
fn store(entry: &Path, facts: &syntax::Facts) -> io::Result<()> {
    let payload = encode_payload(facts).ok_or_else(|| io::Error::other("unencodable facts"))?;
    let shard = entry
        .parent()
        .ok_or_else(|| io::Error::other("entry has no directory"))?;
    create_private_dir(shard)?;
    let mut file = tempfile::NamedTempFile::new_in(shard)?;
    file.write_all(MAGIC)?;
    file.write_all(&Sha256::digest(&payload))?;
    file.write_all(&payload)?;
    file.persist(entry).map_err(|error| error.error)?;
    Ok(())
}

/// A SHA-256 of the running executable's bytes.
fn executable_identity() -> io::Result<Vec<u8>> {
    let mut file = fs::File::open(std::env::current_exe()?)?;
    let mut hash = Sha256::new();
    let mut buffer = vec![0; 1 << 16];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            return Ok(hash.finalize().to_vec());
        }
        hash.update(&buffer[..read]);
    }
}

/// `dir` made absolute through its nearest existing ancestor, creating
/// nothing, so a directory not yet made compares like one that exists.
fn resolved(dir: &Path) -> io::Result<PathBuf> {
    let absolute = std::path::absolute(dir)?;
    let mut missing = Vec::new();
    let mut existing = absolute.as_path();
    loop {
        if let Ok(found) = existing.canonicalize() {
            return Ok(missing
                .iter()
                .rev()
                .fold(found, |path, part| path.join(part)));
        }
        let (Some(parent), Some(name)) = (existing.parent(), existing.file_name()) else {
            return Ok(absolute);
        };
        missing.push(name.to_owned());
        existing = parent;
    }
}

fn create_private_dir(dir: &Path) -> io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    builder.create(dir)
}

fn modified(path: &Path) -> Option<SystemTime> {
    fs::metadata(path).and_then(|meta| meta.modified()).ok()
}

/// Set `path`'s modification time to now if it is missing or older than
/// `age`; returns whether it did.
fn touch_if_older(path: &Path, age: Duration) -> bool {
    if modified(path).is_some_and(|time| time.elapsed().is_ok_and(|elapsed| elapsed < age)) {
        return false;
    }
    fs::File::options()
        .create(true)
        .truncate(false)
        .write(true)
        .open(path)
        .and_then(|file| file.set_modified(SystemTime::now()))
        .is_ok()
}

fn tree_size(dir: &Path) -> u64 {
    fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| match entry.metadata() {
            Ok(meta) if meta.is_dir() => tree_size(&entry.path()),
            Ok(meta) => meta.len(),
            Err(_) => 0,
        })
        .sum()
}

/// Lowercase hexadecimal of a digest.
pub(crate) fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut out, byte| {
        let _ = write!(out, "{byte:02x}");
        out
    })
}

#[cfg(test)]
mod tests;
