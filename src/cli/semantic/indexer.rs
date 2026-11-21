//! `--semantic`: produce the SCIP index by running `rust-analyzer scip` on
//! the inspected workspace. Unlike every other Atlas command, this executes
//! the workspace's code: rust-analyzer builds and runs its build scripts and
//! proc macros. It is opt-in only, and never implied by another flag.
//!
//! The indexer is an owned child with a deadline: on timeout it and every
//! process it started are stopped by exact process id. It builds under
//! `<target>/atlas/semantic`, never the workspace's own build directories. The
//! workspace's manifests, configuration, build scripts and captured sources
//! are digested before and after the run, and an index made while any of them
//! changed is refused. Atlas refuses when it cannot produce an index (no
//! indexer, no `rust-src`, a failed or timed-out run); an index it did produce
//! is then judged by the same reader as a supplied one.
//!
//! Produced indexes are cached beside the facts cache, keyed by the input
//! digest, the indexer and toolchain versions and the Atlas executable, so a
//! repeat question about an unchanged workspace reuses one.
use super::super::output::protected_paths;
use atlas_lib::analysis::semantic::SemanticIndex;
use cargo_atlas::Result;
use clap::Args;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

/// How much of each indexer stream is kept for a failure message.
const TAIL: usize = 64 * 1024;
/// Produced indexes kept in the cache; the least recently used go first.
const KEPT: usize = 4;
const MAGIC: &[u8; 16] = b"atlas-scip-v1\0\0\0";

/// `--semantic` and its deadline, shared by the commands that read an index.
#[derive(Args)]
pub(in crate::cli) struct IndexerArgs {
    /// Produce the index by running `rust-analyzer scip` on this workspace.
    /// This runs the workspace's build scripts and proc macros, so use it
    /// only on code you would build. On a large workspace it takes minutes and
    /// several GB of memory. It builds under <target>/atlas/semantic, and the
    /// index is cached by its inputs, so asking again is fast.
    /// `ATLAS_RUST_ANALYZER` names the executable (default: rust-analyzer on
    /// PATH).
    #[arg(long, conflicts_with = "scip")]
    pub(in crate::cli) semantic: bool,
    /// Stop rust-analyzer, and every process it started, after this many
    /// seconds.
    #[arg(
        long,
        value_name = "SECONDS",
        default_value_t = 1800,
        requires = "semantic"
    )]
    pub(in crate::cli) semantic_timeout: u64,
}

/// Where produced indexes are kept, or why they are neither read nor kept.
pub(in crate::cli) struct IndexCache {
    pub(in crate::cli) dir: std::result::Result<PathBuf, crate::cli::cache::Uncached>,
    /// Whether a stored index may answer; verify mode only stores.
    pub(in crate::cli) reuse: bool,
}

/// How this run's index was obtained, reported beside the reading.
#[derive(Serialize)]
pub(in crate::cli) struct Indexing {
    /// `rust-analyzer --version`, as printed.
    indexer: String,
    /// `rustc -vV`'s release line in the workspace.
    toolchain: String,
    /// `hit`, `stored`, or why the index was not stored or not looked up.
    cache: String,
    /// Wall time of the indexer run; absent on a cache hit.
    #[serde(skip_serializing_if = "Option::is_none")]
    seconds: Option<f64>,
    target_dir: String,
    /// SHA-256 over the digested inputs, in hex.
    input_digest: String,
    inputs: usize,
}

/// Produce (or reuse) an index of the workspace at `root`. `captured` are
/// the source files the command read, each with the fingerprint of the
/// bytes it read; they are digested with the manifests, configuration and
/// build scripts. A captured file whose live bytes differ, before indexing
/// (so a cached index too) or after, refuses: the index would describe
/// other source than the reading it is compared with.
pub(in crate::cli) fn index(
    root: &Path,
    metadata: &cargo_metadata::Metadata,
    captured: &[(PathBuf, String)],
    args: &IndexerArgs,
    cache: &IndexCache,
) -> Result<(SemanticIndex, Indexing)> {
    let program = std::env::var_os("ATLAS_RUST_ANALYZER")
        .filter(|program| !program.is_empty())
        .map_or_else(|| PathBuf::from("rust-analyzer"), PathBuf::from);
    let indexer = version(&program)?;
    let toolchain = toolchain(root)?;
    rust_src(root)?;
    unchanged_since_capture(root, captured, "before indexing")?;
    let paths = inputs(
        metadata,
        captured.iter().map(|(path, _)| path.clone()).collect(),
    );
    let before = digest(&paths);
    let identity = identity(&before, &indexer, &toolchain, root)?;
    let target = metadata
        .target_directory
        .as_std_path()
        .join("atlas")
        .join("semantic");
    let mut indexing = Indexing {
        indexer,
        toolchain,
        cache: String::new(),
        seconds: None,
        target_dir: target.display().to_string(),
        input_digest: hex(&identity.inputs),
        inputs: paths.len(),
    };
    let entry = cache
        .dir
        .as_ref()
        .map(|dir| dir.join(format!("{}.scip", hex(&identity.key))));
    if cache.reuse
        && let Ok(entry) = &entry
        && let Some(bytes) = stored(entry)
    {
        eprintln!("atlas: reused the semantic index cached for these inputs");
        "hit".clone_into(&mut indexing.cache);
        return Ok((SemanticIndex::read_indexed(&bytes), indexing));
    }
    let output = tempfile::tempdir()?;
    let file = output.path().join("index.scip");
    indexing.seconds = Some(run_indexer(
        &program,
        root,
        &file,
        &target,
        args.semantic_timeout,
    )?);
    let after = digest(&paths);
    unchanged_since_capture(root, captured, "while rust-analyzer ran")?;
    if let Some(path) = changed(&before, &after) {
        return Err(format!(
            "semantic evidence unavailable: {} changed while rust-analyzer ran (a build script or another program wrote it); Atlas does not use an index of inputs that moved. Run again once the workspace is still",
            path.strip_prefix(root).unwrap_or(&path).display()
        )
        .into());
    }
    let bytes = fs::read(&file).map_err(|e| {
        format!("semantic evidence unavailable: rust-analyzer wrote no index ({e})")
    })?;
    let index = SemanticIndex::read_indexed(&bytes);
    indexing.cache = match (&entry, index.unavailable()) {
        (Err(why), _) => format!("not stored: {why}"),
        (Ok(_), Some(_)) => "not stored: the index is unavailable".to_owned(),
        (Ok(entry), None) => match store(entry, &bytes) {
            Ok(()) => "stored".to_owned(),
            Err(error) => format!("not stored: {error}"),
        },
    };
    Ok((index, indexing))
}

/// Refuse when a captured source's live bytes are not the bytes the
/// command read.
fn unchanged_since_capture(root: &Path, captured: &[(PathBuf, String)], when: &str) -> Result<()> {
    for (path, fingerprint) in captured {
        let live = fs::read(path)
            .ok()
            .map(|bytes| cargo_atlas::discovery::fingerprint(&bytes));
        if live.as_deref() != Some(fingerprint.as_str()) {
            return Err(format!(
                "semantic evidence unavailable: {} changed after Atlas read it ({when}); an index of it would describe other source than the reading it is compared with. Run again once the workspace is still",
                path.strip_prefix(root).unwrap_or(path).display()
            )
            .into());
        }
    }
    Ok(())
}

/// Run `rust-analyzer scip` into `file`, building under `target`, and return
/// its wall time in seconds. A run that cannot start, times out or fails
/// refuses: no index is read from it.
fn run_indexer(
    program: &Path,
    root: &Path,
    file: &Path,
    target: &Path,
    timeout: u64,
) -> Result<f64> {
    fs::create_dir_all(target)?;
    eprintln!(
        "atlas: indexing with rust-analyzer, which runs this workspace's build scripts and proc macros; building in {} (stops after {timeout} s)",
        target.display()
    );
    let mut command = Command::new(program);
    command
        .arg("scip")
        .arg(root)
        .arg("--output")
        .arg(file)
        .current_dir(root)
        .env("CARGO_TARGET_DIR", target);
    let started = Instant::now();
    let run = run(command, Duration::from_secs(timeout)).map_err(|e| {
        format!(
            "semantic evidence unavailable: cannot run {}: {e}",
            program.display()
        )
    })?;
    let seconds = started.elapsed().as_secs_f64();
    match run {
        Run::TimedOut(cleanup) => {
            let stopped = match cleanup {
                Cleanup::Confirmed => "it and the processes it started were stopped".to_owned(),
                Cleanup::Unconfirmed(why) => format!(
                    "it was stopped, but Atlas could not confirm that the processes it started stopped ({why}); check for a leftover rust-analyzer or cargo"
                ),
            };
            return Err(format!(
                "semantic evidence unavailable: rust-analyzer did not finish within {timeout} s; {stopped}. Raise --semantic-timeout, or supply an index with --scip"
            )
            .into());
        }
        Run::Exited { status, stderr, .. } if !status.success() => {
            return Err(format!(
                "semantic evidence unavailable: rust-analyzer scip failed ({status}): {}",
                last_lines(&stderr, 12)
            )
            .into());
        }
        Run::Exited { .. } => {}
    }
    Ok(seconds)
}

/// The indexer's `--version` line, or why it cannot be used.
fn version(program: &Path) -> Result<String> {
    let mut command = Command::new(program);
    command.arg("--version");
    let unavailable = |problem: String| {
        format!(
            "semantic evidence unavailable: {} --version {problem}; install rust-analyzer (for example `rustup component add rust-analyzer`) or set ATLAS_RUST_ANALYZER to its executable",
            program.display()
        )
    };
    match run(command, Duration::from_secs(60)) {
        Err(error) => Err(unavailable(format!("could not start: {error}")).into()),
        Ok(Run::TimedOut(_)) => Err(unavailable("did not answer within 60 s".to_owned()).into()),
        Ok(Run::Exited { status, stderr, .. }) if !status.success() => {
            Err(unavailable(format!("failed ({status}): {}", last_lines(&stderr, 3))).into())
        }
        Ok(Run::Exited { stdout, .. }) => {
            let line = String::from_utf8_lossy(&stdout)
                .lines()
                .next()
                .unwrap_or_default()
                .trim()
                .to_owned();
            if line.starts_with("rust-analyzer") {
                Ok(line)
            } else {
                Err(unavailable(format!("printed {line:?}, not a rust-analyzer version")).into())
            }
        }
    }
}

/// `rustc -vV` in the workspace (so its toolchain file applies): the release
/// line identifies the toolchain whose standard library is indexed.
fn toolchain(root: &Path) -> Result<String> {
    let output = Command::new("rustc")
        .arg("-vV")
        .current_dir(root)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("semantic evidence unavailable: cannot run rustc -vV: {e}"))?;
    let text = String::from_utf8_lossy(&output.stdout);
    text.lines()
        .find_map(|line| line.strip_prefix("release: "))
        .map(str::to_owned)
        .filter(|_| output.status.success())
        .ok_or_else(|| {
            format!(
                "semantic evidence unavailable: rustc -vV failed: {}",
                last_lines(&output.stderr, 3)
            )
            .into()
        })
}

/// Refuse when the standard library's source is missing: rust-analyzer then
/// writes an index without std and still succeeds, and every std call would
/// read as unresolved instead of unavailable.
fn rust_src(root: &Path) -> Result<()> {
    let library = if let Some(path) = std::env::var_os("RUST_SRC_PATH").filter(|p| !p.is_empty()) {
        PathBuf::from(path)
    } else {
        let output = Command::new("rustc")
            .args(["--print", "sysroot"])
            .current_dir(root)
            .stdin(Stdio::null())
            .output()?;
        PathBuf::from(String::from_utf8_lossy(&output.stdout).trim())
            .join("lib/rustlib/src/rust/library")
    };
    if library.join("core/src/lib.rs").is_file() {
        Ok(())
    } else {
        Err(format!(
            "semantic evidence unavailable: the toolchain's rust-src is missing ({} has no core/src/lib.rs). Without it rust-analyzer indexes no standard library, so std calls would read as unresolved. Install it with `rustup component add rust-src`, or set RUST_SRC_PATH",
            library.display()
        )
        .into())
    }
}

/// Every file whose change would change the index: the protected manifests
/// and configuration, the captured sources, and each package's build script.
fn inputs(metadata: &cargo_metadata::Metadata, sources: Vec<PathBuf>) -> Vec<PathBuf> {
    let scripts = metadata
        .workspace_packages()
        .into_iter()
        .flat_map(|package| &package.targets)
        .filter(|target| target.is_custom_build())
        .map(|target| target.src_path.as_std_path().to_owned())
        .collect::<Vec<_>>();
    let mut paths = protected_paths(metadata, sources, scripts);
    paths.sort();
    paths.dedup();
    paths
}

/// Each input's SHA-256, or `None` for an absent or unreadable file.
fn digest(paths: &[PathBuf]) -> BTreeMap<PathBuf, Option<[u8; 32]>> {
    paths
        .iter()
        .map(|path| {
            let hash = fs::read(path)
                .ok()
                .map(|bytes| Sha256::digest(bytes).into());
            (path.clone(), hash)
        })
        .collect()
}

fn changed(
    before: &BTreeMap<PathBuf, Option<[u8; 32]>>,
    after: &BTreeMap<PathBuf, Option<[u8; 32]>>,
) -> Option<PathBuf> {
    before
        .iter()
        .find(|(path, hash)| after.get(*path) != Some(hash))
        .map(|(path, _)| path.clone())
}

struct Identity {
    inputs: [u8; 32],
    key: [u8; 32],
}

/// The input digest, and the cache key: inputs, indexer, toolchain, root and
/// the Atlas executable, so any of them changing starts cold.
fn identity(
    inputs: &BTreeMap<PathBuf, Option<[u8; 32]>>,
    indexer: &str,
    toolchain: &str,
    root: &Path,
) -> Result<Identity> {
    let mut hash = Sha256::new();
    for (path, digest) in inputs {
        hash.update(path.as_os_str().as_encoded_bytes());
        hash.update([0]);
        match digest {
            Some(digest) => hash.update(digest),
            None => hash.update(b"absent"),
        }
    }
    let inputs: [u8; 32] = hash.finalize().into();
    let mut executable = Sha256::new();
    let mut file = fs::File::open(std::env::current_exe()?)?;
    let mut buffer = vec![0; 1 << 16];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        executable.update(&buffer[..read]);
    }
    let mut key = Sha256::new();
    for part in [
        &inputs[..],
        indexer.as_bytes(),
        toolchain.as_bytes(),
        root.as_os_str().as_encoded_bytes(),
        &executable.finalize()[..],
    ] {
        key.update(part);
        key.update([0]);
    }
    Ok(Identity {
        inputs,
        key: key.finalize().into(),
    })
}

/// A stored index whose header and digest check, marked as just used.
fn stored(entry: &Path) -> Option<Vec<u8>> {
    let bytes = fs::read(entry).ok()?;
    let (magic, rest) = bytes.split_at_checked(MAGIC.len())?;
    let (digest, payload) = rest.split_at_checked(32)?;
    if magic != MAGIC || Sha256::digest(payload)[..] != *digest {
        let _ = fs::remove_file(entry);
        return None;
    }
    let _ = fs::File::options()
        .append(true)
        .open(entry)
        .and_then(|file| file.set_modified(std::time::SystemTime::now()));
    Some(payload.to_vec())
}

/// Keep `bytes` at `entry` (owner-only directory on Unix), then drop the
/// least recently used indexes beyond [`KEPT`].
fn store(entry: &Path, bytes: &[u8]) -> io::Result<()> {
    let dir = entry
        .parent()
        .ok_or_else(|| io::Error::other("no cache directory"))?;
    fs::create_dir_all(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
    }
    let mut file = tempfile::NamedTempFile::new_in(dir)?;
    file.write_all(MAGIC)?;
    file.write_all(&Sha256::digest(bytes))?;
    file.write_all(bytes)?;
    file.persist(entry).map_err(|error| error.error)?;
    let mut kept: Vec<_> = fs::read_dir(dir)?
        .filter_map(std::result::Result::ok)
        .filter(|item| item.path().extension().is_some_and(|ext| ext == "scip"))
        .filter_map(|item| Some((item.metadata().ok()?.modified().ok()?, item.path())))
        .collect();
    kept.sort();
    let excess = kept.len().saturating_sub(KEPT);
    for (_, path) in kept.into_iter().take(excess) {
        let _ = fs::remove_file(path);
    }
    Ok(())
}

enum Run {
    /// The child exited; the tails of its output streams.
    Exited {
        status: ExitStatus,
        stdout: Vec<u8>,
        stderr: Vec<u8>,
    },
    /// The child was stopped at the deadline, with what became of the
    /// processes it started.
    TimedOut(Cleanup),
}

/// Whether the processes a stopped child started are known to be gone.
enum Cleanup {
    Confirmed,
    /// Why Atlas cannot say they are.
    Unconfirmed(String),
}

/// Stops the child, and on Unix every process it started, if the run ends
/// any other way than its own exit.
struct Owned(Child);
impl Owned {
    /// Stop the child and every process it started, and say whether they
    /// are known to be gone.
    fn stop(&mut self) -> Cleanup {
        let cleanup = stop_descendants(self.0.id());
        let _ = self.0.kill();
        let _ = self.0.wait();
        cleanup
    }
}
impl Drop for Owned {
    fn drop(&mut self) {
        if matches!(self.0.try_wait(), Ok(None)) {
            self.stop();
        }
        let _ = self.0.wait();
    }
}

/// Run `command` with no stdin, keeping the tail of its output, until it
/// exits or `limit` passes. It is started directly, never through a shell,
/// and stays in Atlas's process group, so an interrupt reaches it too.
fn run(mut command: Command, limit: Duration) -> io::Result<Run> {
    let mut child = Owned(
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?,
    );
    let (sender, receiver) = mpsc::channel();
    for (index, pipe) in [
        child
            .0
            .stdout
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
        child
            .0
            .stderr
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    ]
    .into_iter()
    .enumerate()
    {
        let sender = sender.clone();
        thread::Builder::new().spawn(move || {
            let tail = pipe.map(tail).unwrap_or_default();
            let _ = sender.send((index, tail));
        })?;
    }
    drop(sender);
    let deadline = Instant::now() + limit;
    let status = loop {
        if let Some(status) = child.0.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            return Ok(Run::TimedOut(child.stop()));
        }
        thread::sleep(Duration::from_millis(50));
    };
    // A process the child left behind may still hold a pipe; its output is
    // then cut short rather than waited for.
    let mut streams = [Vec::new(), Vec::new()];
    let collect = Instant::now() + Duration::from_secs(5);
    while let Ok((index, bytes)) =
        receiver.recv_timeout(collect.saturating_duration_since(Instant::now()))
    {
        streams[index] = bytes;
    }
    let [stdout, stderr] = streams;
    Ok(Run::Exited {
        status,
        stdout,
        stderr,
    })
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut out, byte| {
        let _ = write!(out, "{byte:02x}");
        out
    })
}

/// Read `pipe` to its end, keeping its last [`TAIL`] bytes.
fn tail(mut pipe: Box<dyn Read + Send>) -> Vec<u8> {
    let mut kept = Vec::new();
    let mut buffer = [0; 8192];
    while let Ok(read) = pipe.read(&mut buffer) {
        if read == 0 {
            break;
        }
        kept.extend_from_slice(&buffer[..read]);
        if kept.len() > 2 * TAIL {
            kept.drain(..kept.len() - TAIL);
        }
    }
    let start = kept.len().saturating_sub(TAIL);
    kept.split_off(start)
}

fn last_lines(bytes: &[u8], count: usize) -> String {
    let text = String::from_utf8_lossy(bytes);
    let lines: Vec<_> = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    let shown = lines[lines.len().saturating_sub(count)..].join("\n");
    if shown.is_empty() {
        "no diagnostics".to_owned()
    } else {
        shown
    }
}

/// Stop every live descendant of `pid` by exact process id, read from `ps`,
/// and confirm each is gone. A process list that cannot be read, or a
/// descendant still alive a moment after the kill, leaves it unconfirmed.
#[cfg(unix)]
fn stop_descendants(pid: u32) -> Cleanup {
    let output = match Command::new("ps")
        .args(["-A", "-o", "pid=", "-o", "ppid="])
        .stdin(Stdio::null())
        .output()
    {
        Ok(output) if output.status.success() => output,
        Ok(output) => {
            return Cleanup::Unconfirmed(format!(
                "the process list could not be read: ps {}",
                output.status
            ));
        }
        Err(error) => {
            return Cleanup::Unconfirmed(format!("the process list could not be read: {error}"));
        }
    };
    let found = descendants(pid, &String::from_utf8_lossy(&output.stdout));
    if found.is_empty() {
        return Cleanup::Confirmed;
    }
    let signal = |args: &[&str]| {
        Command::new("kill")
            .args(args)
            .args(&found)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
    };
    let _ = signal(&["-KILL"]);
    // A killed process may linger until it is reaped; give it a moment.
    for _ in 0..20 {
        let alive: Vec<&String> = found.iter().filter(|pid| signal_one(pid)).collect();
        if alive.is_empty() {
            return Cleanup::Confirmed;
        }
        thread::sleep(Duration::from_millis(50));
    }
    let alive: Vec<&str> = found
        .iter()
        .filter(|pid| signal_one(pid))
        .map(String::as_str)
        .collect();
    if alive.is_empty() {
        Cleanup::Confirmed
    } else {
        Cleanup::Unconfirmed(format!(
            "process {} still ran after it was killed",
            alive.join(", ")
        ))
    }
}
/// Whether process `pid` still exists (`kill -0`).
#[cfg(unix)]
fn signal_one(pid: &str) -> bool {
    Command::new("kill")
        .args(["-0", pid])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}
/// Every descendant of `pid` in a `ps -o pid= -o ppid=` listing.
#[cfg(unix)]
fn descendants(pid: u32, listing: &str) -> Vec<String> {
    let mut children: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for line in listing.lines() {
        let mut fields = line.split_whitespace().map(str::parse::<u32>);
        if let (Some(Ok(child)), Some(Ok(parent))) = (fields.next(), fields.next()) {
            children.entry(parent).or_default().push(child);
        }
    }
    let mut found = Vec::new();
    let mut pending = vec![pid];
    while let Some(parent) = pending.pop() {
        for &child in children.get(&parent).into_iter().flatten() {
            found.push(child.to_string());
            pending.push(child);
        }
    }
    found
}
#[cfg(not(unix))]
fn stop_descendants(_pid: u32) -> Cleanup {
    Cleanup::Unconfirmed("the processes a child starts are not tracked on this platform".into())
}
