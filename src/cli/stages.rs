//! Native stage questions, keeping source observations and authority separate.
use atlas::{Result, discovery};
use atlas_core::{
    checks::{
        self,
        policy::{Policy, PublishedStages},
    },
    model::Diagnostic,
    population::FileSelection,
};
use clap::Args;
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

#[derive(Args)]
#[command(
    after_help = "Population: selected packages' native source trees; production stage paths and, for seams, test and mounted code. Evidence: textual paths, not execution or complete policy PASS. Project authority requires the owning project's wrapper. Example: atlas stages --root . --policy .config/architecture.toml --json"
)]
pub(crate) struct Options {
    /// Select a declared Cargo package; defaults to all applicable policy rows.
    scope: Option<String>,
    #[arg(long = "scope", conflicts_with = "scope")]
    named_scope: Option<String>,
    /// Explicit native schema-1 authored policy; relative to the workspace.
    #[arg(long)]
    policy: PathBuf,
    #[arg(long)]
    json: bool,
}
#[derive(Args)]
pub(crate) struct SeamOptions {
    #[command(flatten)]
    inputs: Options,
    /// Include complete item-level observations in text; JSON already retains them.
    #[arg(long)]
    items: bool,
}
#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Question {
    Stages,
    Seams,
}
#[derive(Serialize)]
#[serde(rename_all = "kebab-case")]
enum Status {
    ReportOnly,
    Unavailable,
    NotApplicable,
}
struct Reading {
    observations: serde_json::Value,
    violations: Vec<String>,
    unavailable: Vec<Diagnostic>,
    packages: Vec<String>,
}
fn input_fingerprints<'a>(
    snapshot: &'a discovery::Snapshot,
    packages: &[String],
    question: Question,
) -> Vec<(&'a str, &'a str, &'a str)> {
    let mut paths: BTreeSet<_> = snapshot
        .files(FileSelection::SourceTree)
        .filter(|file| packages.contains(&file.crate_name))
        .map(|file| file.path.as_str())
        .collect();
    if matches!(question, Question::Seams) {
        for name in packages {
            if let Some(mounts) = snapshot.mounted_sources(name) {
                paths.extend(mounts.files.iter().map(|mount| mount.path.as_str()));
            }
        }
    }
    snapshot
        .files(FileSelection::Discovered)
        .filter(|file| packages.contains(&file.crate_name) && paths.contains(file.path.as_str()))
        .map(|file| {
            (
                file.crate_name.as_str(),
                file.path.as_str(),
                file.content_fingerprint.as_str(),
            )
        })
        .collect()
}
fn stage_violations(report: &checks::StageReports) -> Vec<String> {
    report
        .policy_violations
        .iter()
        .cloned()
        .chain(
            report
                .crates
                .iter()
                .flat_map(|(name, reading)| reading.refusals(name)),
        )
        .collect()
}
fn read(
    snapshot: &discovery::Snapshot,
    policy: &Policy,
    scope: Option<&str>,
    question: Question,
) -> Result<Reading> {
    match question {
        Question::Stages => {
            let report = checks::stages(snapshot, policy, scope)?;
            Ok(Reading {
                observations: serde_json::to_value(&report)?,
                violations: stage_violations(&report),
                packages: report.crates.keys().cloned().collect(),
                unavailable: report.unavailable,
            })
        }
        Question::Seams => {
            // Standalone has no project interpreter. Absence is explicit;
            // library wrappers supply source-bound declarations directly.
            let report = checks::seams(snapshot, policy, &BTreeMap::new(), scope)?;
            let mut violations = stage_violations(&report.stages);
            let published = PublishedStages::default();
            violations.extend(report.crates.iter().flat_map(|(name, reading)| {
                reading.refusals(None, &report.stages.crates[name], &published)
            }));
            let mut unavailable = report.unavailable;
            unavailable.extend(report.stages.unavailable.iter().cloned());
            Ok(Reading {
                observations: serde_json::json!({
                    "crates": report.crates.values()
                        .map(checks::stages::seams::SeamReading::json)
                        .collect::<Vec<_>>(),
                    "stages": report.stages,
                    "unavailable": unavailable,
                }),
                violations,
                packages: policy
                    .stage_seams
                    .keys()
                    .filter(|name| scope.is_none_or(|scope| name.as_str() == scope))
                    .cloned()
                    .collect(),
                unavailable,
            })
        }
    }
}

pub(crate) fn run(
    root: &Path,
    metadata: &cargo_metadata::Metadata,
    options: &Options,
    question: Question,
) -> Result<()> {
    present(root, metadata, options, question, false)
}
pub(crate) fn seams(
    root: &Path,
    metadata: &cargo_metadata::Metadata,
    options: &SeamOptions,
) -> Result<()> {
    present(
        root,
        metadata,
        &options.inputs,
        Question::Seams,
        options.items,
    )
}
fn present(
    root: &Path,
    metadata: &cargo_metadata::Metadata,
    options: &Options,
    question: Question,
    items: bool,
) -> Result<()> {
    let policy_path = root.join(&options.policy).canonicalize()?;
    let text = fs::read_to_string(&policy_path)?;
    let policy = Policy::parse(&text)?;
    let snapshot = discovery::snapshot_metadata(metadata)?;
    let scope = options.named_scope.as_deref().or(options.scope.as_deref());
    let references = match question {
        Question::Stages => policy.stage_references(scope),
        Question::Seams => policy.seam_references(scope),
    };
    let reading = read(&snapshot, &policy, scope, question)?;
    let unavailable = !references.is_empty() || !reading.unavailable.is_empty();
    let status = if unavailable {
        Status::Unavailable
    } else if reading.packages.is_empty() {
        Status::NotApplicable
    } else {
        Status::ReportOnly
    };
    let fingerprints = input_fingerprints(&snapshot, &reading.packages, question);
    if options.json {
        let (files, tests) = match question {
            Question::Stages => ("source-tree", "production"),
            Question::Seams => ("source-tree-and-native-mounts", "production-and-tests"),
        };
        println!(
            "{}",
            crate::cli::json(&serde_json::json!({
                "schema": 1,
                "question": question,
                "selection": { "scope": scope, "files": files, "tests": tests },
                "basis": {
                    "policy": { "file": policy_path, "fingerprint": discovery::fingerprint(text.as_bytes()) },
                    "files": fingerprints,
                },
                "status": status,
                "observations": reading.observations,
                "violations": reading.violations,
                "unresolved_authority": references,
                "diagnostics": snapshot.graph().diagnostics,
                "limits": [
                    "Textual stage paths, not execution or complete Rust resolution.",
                    "Report-only observations; no complete project policy PASS. Project wrappers resolve boundary declarations and authority.",
                ],
            }))?
        );
    } else {
        if items {
            println!("{}", serde_json::to_string_pretty(&reading.observations)?);
        }
        for violation in &reading.violations {
            println!("{violation}");
        }
        for issue in &reading.unavailable {
            println!("{}: {}", issue.file, issue.message);
        }
        println!(
            "{} selected packages; no complete project policy verdict",
            reading.packages.len()
        );
    }
    if unavailable {
        return Err("required project authority or source unavailable; use the project's native Atlas wrapper to resolve its authority (JSON retains the field witnesses and observations)".into());
    }
    Ok(())
}
