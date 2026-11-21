//! Exact repository discovery and read-only JJ/Git identity resolution.
use super::{Revision, output};
use crate::{Result, discovery::Snapshot};
use std::fmt::Write as _;
use std::{
    collections::BTreeSet,
    fs,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::Command,
};

/// Repository selection belongs to the requested root, not inherited Git state.
/// Only child environments change; concurrent callers keep their own process state.
fn repository_command(program: &str) -> Command {
    let mut command = Command::new(program);
    for (name, _) in std::env::vars_os() {
        if name.to_string_lossy().starts_with("GIT_") {
            command.env_remove(name);
        }
    }
    command.env("GIT_OPTIONAL_LOCKS", "0");
    command
}

#[non_exhaustive]
pub struct GitStore {
    pub(super) directory: PathBuf,
    root: PathBuf,
    jj: bool,
}
impl GitStore {
    pub fn discover(root: &Path) -> Result<Self> {
        let root = root.canonicalize()?;
        let jj = root.join(".jj").try_exists()?;
        if !jj && !root.join(".git").try_exists()? {
            return Err("revision custody requires .git or .jj at the selected workspace root; ancestor and subdirectory workspaces are not supported".into());
        }
        if !jj {
            let reported = output(
                repository_command("git")
                    .current_dir(&root)
                    .args(["rev-parse", "--show-toplevel"]),
            )?;
            if Path::new(&reported).canonicalize()? != root {
                return Err("Git toplevel differs from the requested workspace root".into());
            }
        }
        let directory = if jj {
            output(repository_command("jj").current_dir(&root).args([
                "--ignore-working-copy",
                "git",
                "root",
            ]))?
        } else {
            output(
                repository_command("git")
                    .current_dir(&root)
                    .args(["rev-parse", "--absolute-git-dir"]),
            )?
        };
        let directory = PathBuf::from(directory);
        if !directory.is_absolute() || !fs::metadata(&directory)?.is_dir() {
            return Err("backing Git directory must be an absolute directory".into());
        }
        Ok(Self {
            directory,
            root,
            jj,
        })
    }
    pub fn resolve(&self, revision: &str) -> Result<String> {
        let named = if self.jj {
            let reading = output(repository_command("jj").current_dir(&self.root).args([
                "--ignore-working-copy",
                "log",
                "-r",
                revision,
                "--no-graph",
                "-T",
                "commit_id ++ \"\\n\"",
            ]))
            .map_err(|error| jj_refusal(revision, &error.to_string()))?;
            let ids = reading.lines().collect::<Vec<_>>();
            if ids.len() != 1 {
                return Err(format!(
                    "revision `{revision}` must resolve to exactly one commit, found {}",
                    ids.len()
                )
                .into());
            }
            let id = ids[0];
            if !matches!(id.len(), 40 | 64) || !id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return Err("JJ did not return one complete Git commit id".into());
            }
            id.to_owned()
        } else {
            revision.to_owned()
        };
        output(self.command().args([
            "rev-parse",
            "--verify",
            "--end-of-options",
            &format!("{named}^{{commit}}"),
        ]))
    }
    pub fn current(&self) -> Result<String> {
        self.resolve(if self.jj { "@" } else { "HEAD" })
    }
    pub fn description(&self, commit: &str) -> Result<String> {
        let id = self.resolve(commit)?;
        let result = self
            .command()
            .args(["log", "-1", "--format=%B", &id, "--"])
            .output()?;
        if !result.status.success() {
            return Err(String::from_utf8_lossy(&result.stderr)
                .trim()
                .to_owned()
                .into());
        }
        Ok(String::from_utf8(result.stdout)?)
    }
    pub(super) fn command(&self) -> Command {
        let mut command = repository_command("git");
        command.arg("--git-dir").arg(&self.directory);
        command
    }
    /// Bind the live capture to the pinned extraction without changing the project.
    pub fn verify_capture(&self, revision: &Revision, capture: &Snapshot) -> Result<()> {
        if revision.root != revision.directory.path() || revision.commit != revision.object_id {
            return Err(
                "legacy revision fields were changed; they cannot redefine extraction custody"
                    .into(),
            );
        }
        if capture
            .metadata()
            .workspace_root
            .as_std_path()
            .canonicalize()?
            != self.root
        {
            return Err("source capture belongs to another workspace".into());
        }
        self.verify_tree(revision)?;
        self.verify_tracked_files(revision)?;
        let expected = crate::discovery::snapshot_workspace(&revision.root)?;
        verify_population(capture, &expected)
    }
    /// The materialized commit has the pinned candidate's tree.
    fn verify_tree(&self, revision: &Revision) -> Result<()> {
        let current = self.current()?;
        let current_tree = output(self.command().args([
            "rev-parse",
            "--verify",
            "--end-of-options",
            &format!("{current}^{{tree}}"),
        ]))?;
        let candidate_tree = output(self.command().args([
            "rev-parse",
            "--verify",
            "--end-of-options",
            &format!("{}^{{tree}}", revision.object_id),
        ]))?;
        if current_tree != candidate_tree {
            return Err("pinned candidate differs from the materialized commit tree; materialize it before measuring".into());
        }
        Ok(())
    }
    /// Every path the candidate tracks is the same file or symlink in the
    /// working tree and in the extraction, reached through no intermediate
    /// symlink in either.
    fn verify_tracked_files(&self, revision: &Revision) -> Result<()> {
        let listing = self
            .command()
            .args([
                "ls-tree",
                "-r",
                "-z",
                "--name-only",
                "--full-tree",
                &revision.object_id,
            ])
            .output()?;
        if !listing.status.success() {
            return Err(String::from_utf8_lossy(&listing.stderr)
                .trim()
                .to_owned()
                .into());
        }
        let mut directories = BTreeSet::new();
        for name in listing
            .stdout
            .split(|byte| *byte == 0)
            .filter(|name| !name.is_empty())
        {
            let name = std::str::from_utf8(name)?;
            crate::architecture::relative_path(name)?;
            let actual = self.root.join(name);
            let expected = revision.directory.path().join(name);
            let parts = Path::new(name).components().collect::<Vec<_>>();
            for root in [&self.root, &revision.root] {
                let mut parent = root.to_owned();
                for part in &parts[..parts.len() - 1] {
                    parent.push(part);
                    if directories.insert(parent.clone())
                        && fs::symlink_metadata(&parent)?.file_type().is_symlink()
                    {
                        return Err(format!(
                            "tracked path `{name}` crosses an intermediate symlink"
                        )
                        .into());
                    }
                }
            }
            if !same_entry(&actual, &expected)? {
                return Err(format!("pinned candidate differs from the working tree at `{name}`; materialize the candidate before measuring").into());
            }
        }
        Ok(())
    }
}
/// Two tracked entries are the same symlink target, or regular files with
/// the same executable mode and bytes; any other pair differs.
fn same_entry(actual: &Path, expected: &Path) -> Result<bool> {
    let actual_meta = fs::symlink_metadata(actual)?;
    let expected_meta = fs::symlink_metadata(expected)?;
    if actual_meta.file_type().is_symlink() && expected_meta.file_type().is_symlink() {
        Ok(fs::read_link(actual)? == fs::read_link(expected)?)
    } else if actual_meta.is_file() && expected_meta.is_file() {
        Ok(same_mode(&actual_meta, &expected_meta)? && same_bytes(actual, expected)?)
    } else {
        Ok(false)
    }
}
/// The live capture has the candidate's package population, source bytes
/// and normal dependencies; untracked source in the capture differs.
fn verify_population(capture: &Snapshot, expected: &Snapshot) -> Result<()> {
    if capture.graph().import_roots != expected.graph().import_roots
        || capture.graph().files != expected.graph().files
    {
        return Err("captured source/package population differs from the pinned candidate, including possible untracked source".into());
    }
    for file in &capture.graph().files {
        if capture.source(&file.path) != expected.source(&file.path) {
            return Err(format!(
                "captured bytes for {} differ from the pinned candidate",
                file.path
            )
            .into());
        }
    }
    if normal_dependencies(capture) != normal_dependencies(expected) {
        return Err("captured normal dependencies differ from the pinned candidate".into());
    }
    Ok(())
}
/// Each workspace package's normal dependency names.
fn normal_dependencies(
    snapshot: &Snapshot,
) -> std::collections::BTreeMap<String, BTreeSet<String>> {
    snapshot
        .metadata()
        .workspace_packages()
        .into_iter()
        .map(|package| {
            (
                package.name.to_string(),
                package
                    .dependencies
                    .iter()
                    .filter(|dependency| dependency.kind == cargo_metadata::DependencyKind::Normal)
                    .map(|dependency| dependency.name.clone())
                    .collect::<BTreeSet<_>>(),
            )
        })
        .collect()
}
#[cfg(unix)]
#[expect(
    clippy::unnecessary_wraps,
    reason = "the shared custody interface returns unavailable on non-Unix hosts that cannot prove executable modes"
)]
fn same_mode(a: &fs::Metadata, b: &fs::Metadata) -> Result<bool> {
    use std::os::unix::fs::PermissionsExt;
    // Git records owner-execute, not the untracked group/other permission bits.
    Ok((a.permissions().mode() & 0o100) == (b.permissions().mode() & 0o100))
}
#[cfg(not(unix))]
fn same_mode(_a: &fs::Metadata, _b: &fs::Metadata) -> Result<bool> {
    Err("executable-mode custody is unavailable on this platform".into())
}
fn same_bytes(a: &Path, b: &Path) -> Result<bool> {
    let mut a = BufReader::new(fs::File::open(a)?);
    let mut b = BufReader::new(fs::File::open(b)?);
    loop {
        let left = a.fill_buf()?;
        let right = b.fill_buf()?;
        let count = left.len().min(right.len());
        if count == 0 {
            return Ok(left.is_empty() && right.is_empty());
        }
        if left[..count] != right[..count] {
            return Ok(false);
        }
        a.consume(count);
        b.consume(count);
    }
}

/// Explain a revision jj refused: Atlas reads revisions through jj in a jj
/// workspace, so a git spelling such as `HEAD~1` fails there.
fn jj_refusal(revision: &str, error: &str) -> Box<dyn std::error::Error + Send + Sync> {
    let mut message = format!(
        "revision `{revision}` was read as a jj revision, because this is a jj workspace, and jj refused it: {}",
        error.trim()
    );
    if let Some(spelling) = jj_spelling(revision) {
        write!(
            message,
            "; jj keeps git's HEAD at `@-`, the parent of the working-copy change `@`, so the commit git calls `{revision}` is `{spelling}` (or give a commit id)"
        )
        .expect("formatting into a String cannot fail");
    } else {
        message.push_str(
            "; give a jj revision such as `@-` (the working copy's parent) or a commit id",
        );
    }
    message.into()
}

/// The jj spelling of a git `HEAD`, `HEAD~N`, `HEAD^` chain, assuming jj keeps
/// git's HEAD at the working copy's parent, as it does in a colocated
/// repository. Other spellings, including second parents (`^2`), get none.
fn jj_spelling(revision: &str) -> Option<String> {
    let mut rest = revision.strip_prefix("HEAD")?;
    let mut generations = 1usize;
    while let Some(first) = rest.chars().next() {
        rest = &rest[1..];
        let digits = rest.len() - rest.trim_start_matches(|c: char| c.is_ascii_digit()).len();
        let count = if digits == 0 {
            1
        } else {
            rest[..digits].parse().ok()?
        };
        rest = &rest[digits..];
        match first {
            '~' => generations = generations.checked_add(count)?,
            '^' if count == 1 => generations = generations.checked_add(1)?,
            '^' if count == 0 => {}
            _ => return None,
        }
    }
    (generations <= 64).then(|| format!("@{}", "-".repeat(generations)))
}

#[cfg(test)]
mod spelling_tests {
    use super::{jj_refusal, jj_spelling};

    #[test]
    fn git_head_chains_get_their_jj_spelling() {
        for (git, jj) in [
            ("HEAD", "@-"),
            ("HEAD~", "@--"),
            ("HEAD~1", "@--"),
            ("HEAD^", "@--"),
            ("HEAD~~4", "@------"),
            ("HEAD^^", "@---"),
            ("HEAD~3^", "@-----"),
        ] {
            assert_eq!(jj_spelling(git).as_deref(), Some(jj), "{git}");
        }
        for other in ["main", "@-", "HEAD^2", "HEAD~x", "HEADS", "abc123"] {
            assert_eq!(jj_spelling(other), None, "{other}");
        }
    }

    #[test]
    fn a_refusal_says_it_read_a_jj_revision() {
        let message = jj_refusal("HEAD~~4", "Error: Revision `HEAD` doesn't exist\n").to_string();
        assert!(message.contains("read as a jj revision"), "{message}");
        assert!(
            message.contains("git calls `HEAD~~4` is `@------`"),
            "{message}"
        );
        let message = jj_refusal("nope", "Error: Revision `nope` doesn't exist").to_string();
        assert!(message.contains("such as `@-`"), "{message}");
    }
}
