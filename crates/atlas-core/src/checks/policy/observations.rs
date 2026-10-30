//! Host observations for fixed policy inputs, over a private replica of captured Rust.
use super::{Inputs, Match, PathState, PatternState, Policy, anchor};
use crate::{
    architecture::{contains, relative_path},
    discovery::Snapshot,
    model::SourceFile,
    population::FileSelection,
};
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    io::Write as _,
    path::{Component, Path},
    process::Command,
};

/// Resolve generic host inputs. Project authority references stay unresolved.
pub fn observe(snapshot: &Snapshot, policy: &Policy) -> Inputs {
    observe_with_helper(snapshot, policy, Path::new("ast-grep"))
}
/// Select the syntax helper explicitly for a native integration or controlled test.
/// Source is copied from the snapshot; the helper never rereads the inspected tree.
pub fn observe_with_helper(snapshot: &Snapshot, policy: &Policy, helper: &Path) -> Inputs {
    let mut inputs = Inputs::new(snapshot);
    let members = snapshot
        .metadata()
        .workspace_packages()
        .into_iter()
        .map(|package| package.name.as_str())
        .collect();
    // This independently callable I/O boundary must not launch helpers from
    // malformed configuration. The validator remains safe without this preflight.
    if policy.validate_configuration(&members).is_err() {
        return inputs;
    }
    for path in policy.authority.iter().flat_map(|rule| &rule.forbid_path) {
        if inputs.paths.contains_key(path) {
            continue;
        }
        let value = observe_path(snapshot.metadata().workspace_root.as_std_path(), path);
        inputs.paths.insert(path.clone(), value);
    }
    let queries: std::collections::BTreeSet<_> = policy
        .authority
        .iter()
        .flat_map(|rule| &rule.forbid_pattern)
        .collect();
    if queries.is_empty() {
        return inputs;
    }
    let roots: Vec<_> = queries
        .iter()
        .filter_map(|query| anchor(query, "forbid_pattern").ok().map(|(root, _)| root))
        .collect();
    let files: BTreeMap<_, _> = snapshot
        .files(FileSelection::SourceTree)
        .filter(|file| roots.iter().any(|root| contains(root, &file.path)))
        .map(|file| (file.path.as_str(), file))
        .collect();
    let line_counts: BTreeMap<_, _> = files
        .keys()
        .filter_map(|file| {
            snapshot
                .source(file)
                .map(|source| (*file, source.lines().count()))
        })
        .collect();
    let replica = replica(snapshot, &files);
    for query in queries {
        let result = match &replica {
            Ok(replica) => query_pattern(replica.path(), query, helper, &files, &line_counts),
            Err(error) => Err(format!("cannot prepare captured syntax input: {error}").into()),
        };
        inputs.patterns.insert(
            query.clone(),
            match result {
                Ok(result) => result,
                Err(error) => PatternState::Unavailable(error.to_string()),
            },
        );
    }
    inputs
}
fn observe_path(root: &Path, path: &str) -> PathState {
    let mut boundary = root.to_path_buf();
    let mut parts = Path::new(path).components().peekable();
    while let Some(part) = parts.next() {
        boundary.push(part);
        match std::fs::symlink_metadata(&boundary) {
            Ok(metadata) if parts.peek().is_some() && metadata.file_type().is_symlink() => {
                return PathState::Unavailable(
                    "forbidden path crosses an intermediate symlink".into(),
                );
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return PathState::Absent,
            Err(error) => return PathState::Unavailable(error.to_string()),
        }
    }
    PathState::Present
}
fn replica(
    snapshot: &Snapshot,
    files: &BTreeMap<&str, &SourceFile>,
) -> crate::Result<tempfile::TempDir> {
    let directory = tempfile::Builder::new().prefix("atlas-policy-").tempdir()?;
    for relative in files.keys() {
        relative_path(relative)?;
        let destination = directory.path().join(relative);
        std::fs::create_dir_all(destination.parent().ok_or("captured source lacks parent")?)?;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(destination)?;
        file.write_all(
            snapshot
                .source(relative)
                .ok_or("pattern population lacks captured bytes")?
                .as_bytes(),
        )?;
    }
    Ok(directory)
}
#[derive(Deserialize)]
struct Hit {
    file: String,
    range: Range,
}
#[derive(Deserialize)]
struct Range {
    start: Position,
}
#[derive(Deserialize)]
struct Position {
    line: usize,
}
fn query_pattern(
    root: &Path,
    query: &str,
    helper: &Path,
    files: &BTreeMap<&str, &SourceFile>,
    line_counts: &BTreeMap<&str, usize>,
) -> crate::Result<PatternState> {
    let (scope, pattern) = anchor(query, "forbid_pattern")?;
    let sources: BTreeMap<_, _> = files
        .values()
        .filter(|file| contains(scope, &file.path))
        .map(|file| (file.path.clone(), file.content_fingerprint.clone()))
        .collect();
    if sources.is_empty() {
        return Ok(PatternState::Read {
            sources,
            matches: Vec::new(),
        });
    }
    let helper = if helper.is_relative() && helper.components().count() > 1 {
        std::path::absolute(helper)?
    } else {
        helper.to_owned()
    };
    let output = Command::new(helper)
        .current_dir(root)
        .args([
            "run",
            "--config",
            "/dev/null",
            "--pattern",
            pattern,
            "--lang",
            "rust",
            "--json",
        ])
        .args(sources.keys().map(|file| root.join(file)))
        .output()?;
    if !matches!(output.status.code(), Some(0 | 1)) || !output.stderr.is_empty() {
        return Err(format!(
            "syntax helper failed ({:?}): {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr).trim()
        )
        .into());
    }
    let hits: Vec<Hit> = serde_json::from_slice(&output.stdout)?;
    let mut matches = Vec::new();
    for hit in hits {
        let path = Path::new(&hit.file);
        let relative = if path.is_absolute() {
            path.strip_prefix(root)?
        } else {
            path
        };
        if relative
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
        {
            return Err("syntax-helper path escaped its input".into());
        }
        let file = relative.to_string_lossy().replace('\\', "/");
        let line = hit
            .range
            .start
            .line
            .checked_add(1)
            .ok_or("syntax-helper source line overflow")?;
        if !sources.contains_key(&file)
            || line_counts
                .get(file.as_str())
                .is_none_or(|lines| line > *lines)
        {
            return Err("syntax-helper match is outside captured input".into());
        }
        matches.push(Match { file, line });
    }
    Ok(PatternState::Read { sources, matches })
}
