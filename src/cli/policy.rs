//! Read authored policy, observe required inputs, validate, then print the verdict.
use atlas::{Result, discovery};
use atlas_core::{
    analysis::Analysis,
    checks::policy::{self, Policy, Status},
    population::FileSelection,
};
use clap::Args;
use std::{
    fs,
    path::{Path, PathBuf},
};
#[derive(Args)]
#[command(
    after_help = "Question: are policy inputs valid against captured source and required observations? This does not adjudicate measured architecture or a landing delta. Population: native source-tree files, with each rule's production/test selection. Missing helpers or project authority are unavailable, never PASS. Example: atlas policy --root . --policy .config/architecture.toml --json"
)]
pub(crate) struct Options {
    /// Native schema-1 authored policy, relative to the workspace or absolute.
    #[arg(long)]
    policy: PathBuf,
    #[arg(long)]
    json: bool,
}
pub(crate) fn run(
    root: &Path,
    metadata: &cargo_metadata::Metadata,
    options: &Options,
) -> Result<()> {
    let path = root.join(&options.policy).canonicalize()?;
    let text = fs::read_to_string(&path)?;
    let policy = Policy::parse(&text)?;
    let snapshot = discovery::snapshot_metadata(metadata)?;
    let analysis = Analysis::new(&snapshot)?;
    let inputs = policy::observe(&snapshot, &policy);
    let report = policy::validate(&analysis, &policy, &inputs)?;
    let status = report.status();
    let sources: Vec<_> = snapshot
        .files(FileSelection::SourceTree)
        .map(|file| (&file.crate_name, &file.path, &file.content_fingerprint))
        .collect();
    if options.json {
        println!(
            "{}",
            crate::cli::json(&serde_json::json!({
                "schema": 1,
                "question": "policy",
                "judgment": "policy-inputs",
                "status": status,
                "basis": {
                    "policy": {
                        "file": path,
                        "fingerprint": discovery::fingerprint(text.as_bytes()),
                    },
                    "sources": sources,
                },
                "report": report,
                "observations": inputs,
                "diagnostics": snapshot.graph().diagnostics,
                "limits": [
                    "Policy-input validation only: size/surface/concentration measurements, dependency judgments and baseline/candidate ratchets belong to delta. Written source is not compilation or complete Rust resolution.",
                    "Project authority requires the owning wrapper; pattern helpers read private copies of captured bytes.",
                ]
            }))?
        );
    } else {
        let label = match status {
            Status::Pass => "PASS",
            Status::Refuse => "REFUSE",
            Status::Unavailable => "UNAVAILABLE",
        };
        println!(
            "policy inputs: {label} · {} core crates · {} authority homes",
            policy.population.core.len(),
            report.authority_homes.len()
        );
        for violation in &report.violations {
            println!("{violation}");
        }
        for missing in &report.unavailable {
            println!("{}: {}", missing.field, missing.reason);
        }
    }
    match status {
        Status::Pass => Ok(()),
        Status::Refuse => Err("policy refused; see source findings".into()),
        Status::Unavailable => Err("policy required inputs unavailable; resolve helpers and use the native wrapper for project authority".into()),
    }
}
