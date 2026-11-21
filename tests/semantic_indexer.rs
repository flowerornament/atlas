//! `--semantic` runs an indexer as an owned, bounded child, refuses when it
//! cannot produce an index, and never answers from inputs that moved. A fake
//! rust-analyzer stands in for the real one; one test runs the real one when
//! it is installed.
#![cfg(unix)]
mod support;
use serde_json::Value;
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::Path,
    process::{Command, Output},
};
use support::Fixture;

const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/crates/atlas-lib/tests/fixtures/semantic"
);

/// The fake answers `--version` (in `probe` mode it first edits the
/// project, between Atlas's capture and the indexing), then for `scip` logs
/// its target directory, acts out `FAKE_MODE` and copies the committed
/// index to `--output`.
const FAKE: &str = r#"#!/bin/sh
if [ "$1" = "--version" ]; then
  if [ "$FAKE_MODE" = "proxy" ]; then echo "error: infinite recursion detected" >&2; exit 1; fi
  if [ "$FAKE_MODE" = "probe" ]; then echo "// written after the capture" >> "$FAKE_ROOT/src/lib.rs"; fi
  echo "rust-analyzer 0.0.0-fake"; exit 0
fi
root="$2"; out="$4"
echo "$CARGO_TARGET_DIR" >> "$FAKE_LOG"
case "$FAKE_MODE" in
  sleep) sleep 120 & echo $! > "$FAKE_PID"; wait; exit 0 ;;
  fail) echo "fake indexer failure" >&2; exit 3 ;;
  touch) echo "// written during indexing" >> "$root/src/lib.rs" ;;
esac
cp "$FAKE_INDEX" "$out"
"#;

struct Setup {
    project: Fixture,
    /// Fake tool, its log, a rust-src stand-in and the cache, outside the project.
    tools: Fixture,
}

fn setup() -> Setup {
    let file = |path: &str| fs::read_to_string(format!("{FIXTURE}/{path}")).expect(path);
    let project = Fixture::new(&[
        ("Cargo.toml", &file("Cargo.toml")),
        ("src/lib.rs", &file("src/lib.rs")),
    ]);
    let tools = Fixture::new(&[
        ("rust-analyzer", FAKE),
        ("library/core/src/lib.rs", "// stand-in\n"),
    ]);
    fs::set_permissions(
        tools.path().join("rust-analyzer"),
        fs::Permissions::from_mode(0o755),
    )
    .expect("make the fake executable");
    Setup { project, tools }
}

impl Setup {
    fn run(&self, mode: &str, args: &[&str]) -> Output {
        let tools = self.tools.path();
        Command::new(env!("CARGO_BIN_EXE_cargo-atlas"))
            .current_dir("/")
            .args(args)
            .arg("--root")
            .arg(self.project.path())
            .env("ATLAS_RUST_ANALYZER", tools.join("rust-analyzer"))
            .env("RUST_SRC_PATH", tools.join("library"))
            .env("ATLAS_CACHE", "on")
            .env("ATLAS_CACHE_DIR", tools.join("cache"))
            .env("FAKE_MODE", mode)
            .env("FAKE_ROOT", self.project.path())
            .env("FAKE_LOG", tools.join("log"))
            .env("FAKE_PID", tools.join("pid"))
            .env("FAKE_INDEX", format!("{FIXTURE}/index.scip"))
            .output()
            .expect("run cargo-atlas")
    }
    /// As `run`, with `PATH` set to `path` alone.
    fn run_with_path(&self, mode: &str, args: &[&str], path: &Path) -> Output {
        let tools = self.tools.path();
        Command::new(env!("CARGO_BIN_EXE_cargo-atlas"))
            .current_dir("/")
            .args(args)
            .arg("--root")
            .arg(self.project.path())
            .env("PATH", path)
            .env("ATLAS_RUST_ANALYZER", tools.join("rust-analyzer"))
            .env("RUST_SRC_PATH", tools.join("library"))
            .env("ATLAS_CACHE", "off")
            .env("FAKE_MODE", mode)
            .env("FAKE_ROOT", self.project.path())
            .env("FAKE_LOG", tools.join("log"))
            .env("FAKE_PID", tools.join("pid"))
            .env("FAKE_INDEX", format!("{FIXTURE}/index.scip"))
            .output()
            .expect("run cargo-atlas")
    }
    fn runs(&self) -> Vec<String> {
        fs::read_to_string(self.tools.path().join("log"))
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }
}

fn refused(output: &Output, message: &str) {
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "expected a refusal: {stderr}");
    assert!(stderr.contains("semantic evidence unavailable"), "{stderr}");
    assert!(stderr.contains(message), "{stderr}");
    assert!(output.stdout.is_empty());
}

fn json(output: &Output) -> Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("JSON output")
}

#[test]
fn an_index_is_produced_once_then_reused_for_unchanged_inputs() {
    let s = setup();
    let first = json(&s.run("", &["dump", "--json", "--semantic"]));
    assert_eq!(first["semantic"]["status"], "available");
    assert_eq!(first["semantic"]["provenance"]["origin"], "indexed");
    assert_eq!(first["semantic"]["agreement"]["agrees"], 2);
    assert_eq!(first["semantic_indexing"]["cache"], "stored");
    assert_eq!(
        first["semantic_indexing"]["indexer"],
        "rust-analyzer 0.0.0-fake"
    );
    // The indexer builds under the workspace's target/atlas/semantic only.
    let target = Path::new(&s.runs()[0]).to_owned();
    assert!(
        target.ends_with("target/atlas/semantic"),
        "{}",
        target.display()
    );
    assert!(target.is_dir());
    assert!(!s.project.path().join("target/debug").exists());

    let second = json(&s.run("", &["dump", "--json", "--semantic"]));
    assert_eq!(second["semantic_indexing"]["cache"], "hit");
    assert_eq!(second["semantic"], first["semantic"]);
    assert_eq!(s.runs().len(), 1, "a hit runs no indexer");

    s.project.write("src/lib.rs", "pub fn changed() {}\n");
    let third = json(&s.run("", &["dump", "--json", "--semantic"]));
    assert_eq!(third["semantic_indexing"]["cache"], "stored");
    assert_eq!(s.runs().len(), 2, "changed inputs index again");
}

#[test]
fn no_cache_runs_the_indexer_every_time_and_keeps_nothing() {
    let s = setup();
    for _ in 0..2 {
        let out = json(&s.run("", &["--no-cache", "dump", "--json", "--semantic"]));
        assert_eq!(out["semantic_indexing"]["cache"], "not stored: off");
    }
    assert_eq!(s.runs().len(), 2);
    assert!(!s.tools.path().join("cache/semantic").exists());
}

#[test]
fn flow_writes_semantic_evidence_from_a_produced_index() {
    let s = setup();
    let out = s.run(
        "",
        &[
            "flow",
            "--path",
            "src/lib.rs",
            "--function",
            "generic",
            "--semantic",
        ],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let evidence = fs::read_dir(s.project.path().join("target/atlas"))
        .expect("flow evidence directory")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| path.to_string_lossy().ends_with(".semantic.json"))
        .expect("semantic evidence file");
    let reading: Value = serde_json::from_slice(&fs::read(evidence).unwrap()).unwrap();
    assert_eq!(reading["provenance"]["origin"], "indexed");
    assert_eq!(reading["indexing"]["cache"], "stored");
}

#[test]
fn a_run_past_its_deadline_is_stopped_with_everything_it_started() {
    let s = setup();
    let out = s.run(
        "sleep",
        &["dump", "--json", "--semantic", "--semantic-timeout", "1"],
    );
    refused(&out, "did not finish within 1 s");
    let stderr = String::from_utf8_lossy(&out.stderr);
    let pid = fs::read_to_string(s.tools.path().join("pid")).expect("the fake's child pid");
    let alive = Command::new("kill")
        .args(["-0", pid.trim()])
        .output()
        .expect("check the child");
    if stderr.contains("could not confirm") {
        // A host that hides the process list: the claim is qualified.
        let _ = Command::new("kill").args(["-KILL", pid.trim()]).output();
    } else {
        assert!(
            stderr.contains("it and the processes it started were stopped"),
            "{stderr}"
        );
        assert!(
            !alive.status.success(),
            "the indexer's child {pid} survived although Atlas said it stopped"
        );
    }
}

/// Without a process list Atlas cannot find the processes the indexer
/// started, so it says the cleanup is unconfirmed instead of claiming it
/// (codex finding 13).
#[test]
fn a_timeout_without_a_process_list_says_cleanup_is_unconfirmed() {
    let s = setup();
    let bin = s.tools.path().join("bin");
    fs::create_dir_all(&bin).expect("bin");
    // Everything the run needs except `ps`: the toolchain probe's rustc
    // and cargo, the fake's sleep and cp, and kill.
    for tool in ["sleep", "cp", "rustc", "cargo", "kill"] {
        let found = Command::new("/bin/sh")
            .args(["-c", &format!("command -v {tool}")])
            .output()
            .expect("locate tool");
        let path = String::from_utf8_lossy(&found.stdout)
            .lines()
            .map(str::trim)
            .find(|line| line.starts_with('/'))
            .map(str::to_owned)
            .or_else(|| {
                ["/bin", "/usr/bin"]
                    .iter()
                    .map(|dir| format!("{dir}/{tool}"))
                    .find(|path| Path::new(path).exists())
            })
            .unwrap_or_else(|| panic!("{tool} is not installed"));
        std::os::unix::fs::symlink(&path, bin.join(tool)).expect("link tool");
    }
    let out = s.run_with_path(
        "sleep",
        &["dump", "--json", "--semantic", "--semantic-timeout", "1"],
        &bin,
    );
    refused(&out, "did not finish within 1 s");
    refused(
        &out,
        "could not confirm that the processes it started stopped (the process list could not be read",
    );
    let pid = fs::read_to_string(s.tools.path().join("pid")).expect("the fake's child pid");
    let _ = Command::new("kill").args(["-KILL", pid.trim()]).output();
}

#[test]
fn a_missing_or_broken_indexer_is_refused_before_anything_runs() {
    let s = setup();
    let out = Command::new(env!("CARGO_BIN_EXE_cargo-atlas"))
        .args(["dump", "--json", "--semantic", "--root"])
        .arg(s.project.path())
        .env("ATLAS_RUST_ANALYZER", s.tools.path().join("absent"))
        .env("ATLAS_CACHE", "off")
        .output()
        .unwrap();
    refused(&out, "could not start");
    refused(&out, "ATLAS_RUST_ANALYZER");
    // A rustup proxy without the component fails its version probe.
    refused(
        &s.run("proxy", &["dump", "--json", "--semantic"]),
        "infinite recursion",
    );
    assert!(s.runs().is_empty());
}

#[test]
fn a_missing_rust_src_is_refused_not_read_as_no_std_calls() {
    let s = setup();
    let out = Command::new(env!("CARGO_BIN_EXE_cargo-atlas"))
        .args(["dump", "--json", "--semantic", "--root"])
        .arg(s.project.path())
        .env("ATLAS_RUST_ANALYZER", s.tools.path().join("rust-analyzer"))
        .env("RUST_SRC_PATH", s.tools.path().join("nowhere"))
        .env("ATLAS_CACHE", "off")
        .env("FAKE_LOG", s.tools.path().join("log"))
        .output()
        .unwrap();
    refused(&out, "rust-src is missing");
    assert!(s.runs().is_empty());
}

#[test]
fn a_failed_run_or_inputs_that_moved_are_refused_and_never_cached() {
    let s = setup();
    refused(
        &s.run("fail", &["dump", "--json", "--semantic"]),
        "fake indexer failure",
    );
    refused(
        &s.run("touch", &["dump", "--json", "--semantic"]),
        "src/lib.rs changed after Atlas read it (while rust-analyzer ran)",
    );
    assert!(!s.tools.path().join("cache/semantic").exists());
}

/// Source that changes after the command read it, before the indexer runs,
/// is refused, and so is a stored index of the unchanged inputs: the index
/// would describe other bytes than the reading it is compared with (codex
/// finding 12).
#[test]
fn source_changed_after_the_capture_is_refused_before_indexing_and_on_reuse() {
    let s = setup();
    let original = fs::read_to_string(s.project.path().join("src/lib.rs")).expect("source");
    refused(
        &s.run("probe", &["dump", "--json", "--semantic"]),
        "src/lib.rs changed after Atlas read it (before indexing)",
    );
    assert!(s.runs().is_empty(), "the indexer must not run");
    fs::write(s.project.path().join("src/lib.rs"), &original).expect("restore");
    assert_eq!(
        json(&s.run("", &["dump", "--json", "--semantic"]))["semantic_indexing"]["cache"],
        "stored"
    );
    refused(
        &s.run("probe", &["dump", "--json", "--semantic"]),
        "src/lib.rs changed after Atlas read it (before indexing)",
    );
}

#[test]
fn scip_and_semantic_are_alternatives() {
    let s = setup();
    let out = s.run("", &["dump", "--json", "--scip", "x.scip", "--semantic"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("cannot be used with"));
}

/// The real indexer on the public fixture, when one is installed with its
/// rust-src; skipped otherwise.
#[test]
fn the_real_indexer_agrees_with_the_committed_index() {
    let program = std::env::var_os("ATLAS_RUST_ANALYZER").unwrap_or_else(|| "rust-analyzer".into());
    let usable = Command::new(&program)
        .arg("--version")
        .output()
        .is_ok_and(|out| out.status.success() && out.stdout.starts_with(b"rust-analyzer"));
    if !usable {
        // No usable rust-analyzer here (set ATLAS_RUST_ANALYZER to run this).
        return;
    }
    let s = setup();
    let locked = Command::new("cargo")
        .args(["generate-lockfile", "--offline"])
        .current_dir(s.project.path())
        .status()
        .expect("cargo generate-lockfile");
    assert!(locked.success());
    let out = Command::new(env!("CARGO_BIN_EXE_cargo-atlas"))
        .args(["dump", "--json", "--semantic", "--root"])
        .arg(s.project.path())
        .env("ATLAS_RUST_ANALYZER", &program)
        .env("ATLAS_CACHE", "off")
        .output()
        .unwrap();
    if String::from_utf8_lossy(&out.stderr).contains("rust-src is missing") {
        // The toolchain has no rust-src, so the real run is refused by design.
        return;
    }
    let reading = json(&out);
    assert_eq!(reading["semantic"]["status"], "available");
    assert_eq!(reading["semantic"]["coverage"]["sysroot_observed"], true);
    assert_eq!(reading["semantic"]["agreement"]["agrees"], 2);
    assert!(reading["semantic"]["agreement"].get("disagrees").is_none());
}
