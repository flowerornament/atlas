//! Host observations for fixed policy inputs, over a private replica of captured Rust.
use super::{Inputs, Match, Occurrence, PathState, PatternState, Policy, SpellingState, anchor};
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
    // This independently callable I/O boundary must not launch helpers from
    // malformed configuration. The validator remains safe without this preflight.
    if !well_formed(snapshot, policy) {
        return inputs;
    }
    observe_new_patterns(snapshot, policy, helper, &mut inputs);
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
    text: String,
    range: Range,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Range {
    start: Position,
    byte_offset: Offsets,
}
#[derive(Deserialize)]
struct Position {
    line: usize,
}
#[derive(Deserialize)]
struct Offsets {
    start: usize,
    end: usize,
}

/// One `forbid_pattern` query's matches over its scope's captured files.
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
    let mut matches = Vec::new();
    for hit in run_helper(root, helper, pattern, &sources)? {
        let file = hit_file(root, &hit)?;
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

/// Run the helper's `pattern` over the replica's copies of `sources`
/// (none when there are no sources).
fn run_helper(
    root: &Path,
    helper: &Path,
    pattern: &str,
    sources: &BTreeMap<String, String>,
) -> crate::Result<Vec<Hit>> {
    if sources.is_empty() {
        return Ok(Vec::new());
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
    Ok(serde_json::from_slice(&output.stdout)?)
}
/// A hit's file as a captured path key; a path leaving the replica fails.
fn hit_file(root: &Path, hit: &Hit) -> crate::Result<String> {
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
    Ok(crate::source::discovery::path_key(relative))
}

/// Only the `forbid_new_pattern` observations, as `delta` needs them for
/// its base side (observe it under the published policy).
pub fn observe_spellings(snapshot: &Snapshot, policy: &Policy) -> Inputs {
    let mut inputs = Inputs::new(snapshot);
    if well_formed(snapshot, policy) {
        observe_new_patterns(snapshot, policy, Path::new("ast-grep"), &mut inputs);
    }
    inputs
}
fn well_formed(snapshot: &Snapshot, policy: &Policy) -> bool {
    let members = snapshot
        .metadata()
        .workspace_packages()
        .into_iter()
        .map(|package| package.name.as_str())
        .collect();
    policy.validate_configuration(&members).is_ok()
}
/// Each `forbid_new_pattern`'s occurrences over the capture's production
/// files, by byte range and text, per authority fact and pattern name.
fn observe_new_patterns(snapshot: &Snapshot, policy: &Policy, helper: &Path, inputs: &mut Inputs) {
    let wanted: Vec<_> = policy
        .authority
        .iter()
        .flat_map(|rule| rule.forbid_new_pattern.iter().map(move |p| (&rule.fact, p)))
        .collect();
    if wanted.is_empty() {
        return;
    }
    let files: BTreeMap<_, _> = snapshot
        .graph()
        .files
        .iter()
        .filter(|file| !file.test_only)
        .map(|file| (file.path.as_str(), file))
        .collect();
    let sources: BTreeMap<String, String> = files
        .values()
        .map(|file| (file.path.clone(), file.content_fingerprint.clone()))
        .collect();
    let replica = replica(snapshot, &files);
    for (fact, pattern) in wanted {
        let state = match &replica {
            Ok(replica) => new_pattern(replica.path(), helper, &pattern.pattern, &sources),
            Err(error) => Err(format!("cannot prepare captured syntax input: {error}").into()),
        };
        let state = state.unwrap_or_else(|error| SpellingState::Unavailable(error.to_string()));
        inputs
            .new_patterns
            .entry(fact.clone())
            .or_default()
            .insert(pattern.name.clone(), state);
    }
}
fn new_pattern(
    root: &Path,
    helper: &Path,
    pattern: &str,
    sources: &BTreeMap<String, String>,
) -> crate::Result<SpellingState> {
    let mut occurrences = Vec::new();
    for hit in run_helper(root, helper, pattern, sources)? {
        let file = hit_file(root, &hit)?;
        if !sources.contains_key(&file) {
            return Err("syntax-helper match is outside captured input".into());
        }
        let line = hit.range.start.line.saturating_add(1);
        let Offsets { start, end } = hit.range.byte_offset;
        occurrences.push(Occurrence::new(file, start, end, line, hit.text));
    }
    Ok(SpellingState::Read {
        sources: sources.clone(),
        occurrences,
    })
}
