//! Policy validation from captured source and explicit host/project observations.
use super::{Policy, Reference, anchor};
use crate::{
    analysis::Analysis,
    architecture::contains,
    discovery::Snapshot,
    population::{FileSelection, TestSelection},
    source::ItemKind,
};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};

/// Observations are tied to a captured workspace; absent entries are unavailable.
#[derive(Serialize)]
pub struct Inputs {
    workspace: PathBuf,
    pub paths: BTreeMap<String, PathState>,
    pub patterns: BTreeMap<String, PatternState>,
    pub references: Vec<ReferenceResolution>,
}
impl Inputs {
    pub fn new(snapshot: &Snapshot) -> Self {
        Self {
            workspace: snapshot.metadata().workspace_root.as_std_path().to_owned(),
            paths: BTreeMap::new(),
            patterns: BTreeMap::new(),
            references: Vec::new(),
        }
    }
}
#[derive(Serialize)]
#[serde(tag = "kind", content = "reason", rename_all = "kebab-case")]
pub enum PathState {
    Present,
    Absent,
    Unavailable(String),
}
#[derive(Serialize)]
#[serde(tag = "kind", content = "reading", rename_all = "kebab-case")]
pub enum PatternState {
    Read {
        sources: BTreeMap<String, String>,
        matches: Vec<Match>,
    },
    Unavailable(String),
}
#[derive(Serialize)]
pub struct Match {
    pub file: String,
    pub line: usize,
}
#[derive(Serialize)]
pub struct ReferenceResolution {
    pub reference: Reference,
    pub result: Resolution,
}
#[derive(Serialize)]
#[serde(tag = "kind", content = "reason", rename_all = "kebab-case")]
pub enum Resolution {
    Valid,
    Invalid(String),
    Unavailable(String),
}
#[derive(Serialize)]
pub struct MissingInput {
    pub field: String,
    pub reason: String,
}
#[derive(Default, Serialize)]
pub struct Report {
    pub violations: Vec<String>,
    pub unavailable: Vec<MissingInput>,
    pub typed_errors: BTreeMap<String, crate::checks::typed_errors::Reading>,
    pub authority_homes: BTreeMap<String, Match>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    Pass,
    Refuse,
    Unavailable,
}
impl Report {
    pub fn status(&self) -> Status {
        if !self.unavailable.is_empty() {
            Status::Unavailable
        } else if !self.violations.is_empty() {
            Status::Refuse
        } else {
            Status::Pass
        }
    }
    fn missing(&mut self, field: impl Into<String>, reason: impl Into<String>) {
        self.unavailable.push(MissingInput {
            field: field.into(),
            reason: reason.into(),
        });
    }
}
/// Validate the configured generic rules, retaining missing evidence separately.
/// Project wrappers supply authority resolutions; parsing a reference is insufficient.
pub fn validate(
    analysis: &Analysis<'_>,
    policy: &Policy,
    inputs: &Inputs,
) -> crate::Result<Report> {
    let snapshot = analysis.snapshot();
    let members: BTreeSet<_> = snapshot
        .metadata()
        .workspace_packages()
        .into_iter()
        .map(|package| package.name.as_str())
        .collect();
    let mut report = Report::default();
    if let Err(error) = policy.validate_configuration(&members) {
        report.violations.push(error.to_string());
        return Ok(report);
    }
    if inputs.workspace != snapshot.metadata().workspace_root.as_std_path() {
        report.missing(
            "inputs.workspace",
            "helper observations belong to another workspace",
        );
        return Ok(report);
    }
    report.typed_errors = crate::checks::typed_errors::read(analysis, &policy.typed_errors)?;
    for (name, reading) in &report.typed_errors {
        if !reading.boundaries.is_empty() {
            let sites: Vec<_> = reading
                .boundaries
                .iter()
                .flat_map(|(file, lines)| lines.iter().map(move |line| format!("{file}:{line}")))
                .collect();
            report.violations.push(format!("typed-error crate `{name}` has production Result<_, String> at {}; retain the producing phase's typed cause",sites.join(", ")));
        }
        if !reading.unavailable.is_empty() {
            report.unavailable.push(MissingInput {field:format!("typed_errors.{name}"),reason:"required type syntax or source population unavailable; see typed_errors reading".into()});
        }
    }
    read_authorities(snapshot, policy, inputs, &mut report)?;
    resolve_references(policy, inputs, &mut report);
    Ok(report)
}
fn resolve_references(policy: &Policy, inputs: &Inputs, report: &mut Report) {
    for needed in policy.external_references() {
        let field = needed.field.join(".");
        let found: Vec<_> = inputs
            .references
            .iter()
            .filter(|row| {
                row.reference.field == needed.field && row.reference.written == needed.written
            })
            .collect();
        match found.as_slice() {
            [row] => match &row.result {
                Resolution::Valid => {}
                Resolution::Invalid(reason) => report.violations.push(format!(
                    "policy: {field} authority `{}` is invalid: {reason}",
                    needed.written
                )),
                Resolution::Unavailable(reason) => report.missing(field, reason),
            },
            [] => report.missing(
                field,
                format!(
                    "project authority `{}` has not been resolved",
                    needed.written
                ),
            ),
            _ => report.missing(field, "competing project authority resolutions"),
        }
    }
}
fn read_authorities(
    snapshot: &Snapshot,
    policy: &Policy,
    inputs: &Inputs,
    report: &mut Report,
) -> crate::Result<()> {
    // A path names one physical subject; a production placement is not erased
    // by that same file also belonging to a test-only package context.
    let mut files: BTreeMap<&str, bool> = BTreeMap::new();
    for file in snapshot.files(FileSelection::SourceTree) {
        files
            .entry(&file.path)
            .and_modify(|production| *production |= !file.test_only)
            .or_insert(!file.test_only);
    }
    let line_counts: BTreeMap<_, _> = files
        .keys()
        .filter_map(|file| {
            snapshot
                .source(file)
                .map(|source| (*file, source.lines().count()))
        })
        .collect();
    for authority in &policy.authority {
        let (path, item) = anchor(&authority.home, "authority home")?;
        if !files.contains_key(path) {
            let gaps = source_gaps(snapshot, path);
            if !gaps.is_empty() {
                for gap in gaps {
                    report.missing(format!("authority.{}.home", authority.fact), &gap.message);
                }
                continue;
            }
            report.violations.push(format!(
                "policy: authority `{}` has missing home file `{path}`",
                authority.fact
            ));
            continue;
        }
        let source = snapshot
            .source_facts(path)
            .ok_or("authority population lacks captured source")?;
        let sites: Vec<_> = source
            .outline()
            .iter()
            .filter(|entry| {
                files[path]
                    && anchor_kind(entry.kind)
                    && entry.name == item
                    && TestSelection::Production.admits_line(source, entry.line)
            })
            .map(|entry| entry.line)
            .collect();
        if let [line] = sites.as_slice() {
            report.authority_homes.insert(
                authority.fact.clone(),
                Match {
                    file: path.into(),
                    line: *line,
                },
            );
        } else {
            report.violations.push(format!("policy: authority `{}` home `{}` resolves {} times in its source population at {:?}; expected exactly one",authority.fact,authority.home,sites.len(),sites));
        }
        for sentinel in &authority.forbid_item {
            let (root, name) = anchor(sentinel, "forbid_item")?;
            let gaps = source_gaps(snapshot, root);
            for gap in &gaps {
                report.missing(
                    format!("authority.{}.forbid_item", authority.fact),
                    &gap.message,
                );
            }
            let population: Vec<_> = files
                .iter()
                .filter(|(file, _)| contains(root, file))
                .collect();
            if population.is_empty() {
                if gaps.is_empty() {
                    report.violations.push(format!(
                        "policy: forbid_item `{sentinel}` has no source population"
                    ));
                }
                continue;
            }
            let mut sites = Vec::new();
            for (file, production) in population {
                let source = snapshot
                    .source_facts(file)
                    .ok_or("forbidden-item population lacks source")?;
                sites.extend(
                    source
                        .outline()
                        .iter()
                        .filter(|entry| {
                            *production
                                && anchor_kind(entry.kind)
                                && entry.name == name
                                && TestSelection::Production.admits_line(source, entry.line)
                        })
                        .map(|entry| format!("{file}:{}", entry.line)),
                );
            }
            if !sites.is_empty() {
                report.violations.push(format!(
                    "policy: authority `{}` forbidden item `{sentinel}` exists at {}",
                    authority.fact,
                    sites.join(", ")
                ));
            }
        }
        for path in &authority.forbid_path {
            match inputs.paths.get(path) {
                Some(PathState::Absent) => {}
                Some(PathState::Present) => report.violations.push(format!(
                    "policy: authority `{}` forbidden path `{path}` exists",
                    authority.fact
                )),
                Some(PathState::Unavailable(reason)) => {
                    report.missing(format!("authority.{}.forbid_path", authority.fact), reason);
                }
                None => report.missing(
                    format!("authority.{}.forbid_path", authority.fact),
                    format!("path `{path}` has not been observed"),
                ),
            }
        }
        for query in &authority.forbid_pattern {
            read_pattern(
                snapshot,
                &line_counts,
                query,
                &authority.fact,
                inputs,
                report,
            )?;
        }
    }
    Ok(())
}
fn read_pattern(
    snapshot: &Snapshot,
    files: &BTreeMap<&str, usize>,
    query: &str,
    fact: &str,
    inputs: &Inputs,
    report: &mut Report,
) -> crate::Result<()> {
    let (root, pattern) = anchor(query, "forbid_pattern")?;
    let field = format!("authority.{fact}.forbid_pattern");
    let gaps = source_gaps(snapshot, root);
    for gap in &gaps {
        report.missing(&field, &gap.message);
    }
    let expected: BTreeMap<_, _> = files
        .keys()
        .filter(|file| contains(root, file))
        .map(|file| {
            let source = snapshot
                .physical_file(file)
                .ok_or("pattern population lacks fingerprint")?;
            Ok(((*file).to_owned(), source.content_fingerprint.clone()))
        })
        .collect::<crate::Result<_>>()?;
    if expected.is_empty() {
        if gaps.is_empty() {
            report.violations.push(format!(
                "policy: forbid_pattern under `{root}` has no source population"
            ));
        }
        return Ok(());
    }
    match inputs.patterns.get(query) {
        Some(PatternState::Read { sources, matches }) if sources == &expected => {
            if matches.iter().any(|site| {
                !expected.contains_key(&site.file)
                    || site.line == 0
                    || files
                        .get(site.file.as_str())
                        .is_none_or(|lines| site.line > *lines)
            }) {
                report.missing(
                    field,
                    "syntax-helper matches escaped their captured source population",
                );
            } else if !matches.is_empty() {
                report.violations.push(format!(
                    "policy: authority `{fact}` forbidden pattern `{pattern}` matched {} time(s)",
                    matches.len()
                ));
            }
        }
        Some(PatternState::Read { .. }) => report.missing(
            field,
            "syntax-helper source basis differs from the captured population",
        ),
        Some(PatternState::Unavailable(reason)) => report.missing(field, reason),
        None => report.missing(
            field,
            format!("syntax query `{query}` has not been observed"),
        ),
    }
    Ok(())
}
const fn anchor_kind(kind: ItemKind) -> bool {
    matches!(
        kind,
        ItemKind::Struct
            | ItemKind::Enum
            | ItemKind::Union
            | ItemKind::Fn
            | ItemKind::Trait
            | ItemKind::TypeAlias
            | ItemKind::Const
            | ItemKind::Static
            | ItemKind::Mod
    )
}

fn source_gaps<'a>(snapshot: &'a Snapshot, scope: &str) -> Vec<&'a crate::model::Diagnostic> {
    snapshot
        .graph()
        .diagnostics
        .iter()
        .filter(|issue| {
            snapshot.source_facts(&issue.file).is_none()
                && (contains(scope, &issue.file) || contains(&issue.file, scope))
        })
        .collect()
}
