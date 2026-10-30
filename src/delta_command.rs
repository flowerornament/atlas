//! Resolve identities, verify the source capture, judge paired inputs, then present the result.
use atlas::{Result, discovery};
use atlas_core::{
    checks::{
        concentration,
        delta::{self, Basis},
        policy::{self, Policy, Status},
    },
    revision::GitStore,
};
use clap::Args;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    time::Instant,
};
mod presentation;

#[derive(Args)]
#[command(
    after_help = "Question: what architecture debt does this pinned candidate add relative to the baseline? Source must match the materialized candidate; this query never snapshots working edits. Policies using stage_seams or project-owned references require their native wrapper; this CLI reports those inputs as UNAVAILABLE. JSON REFUSE is data (exit 0); consumers must admit the verdict. Unavailable evidence exits unsuccessfully. Example: atlas delta HEAD~1 --candidate HEAD --policy .config/architecture.toml --citation-markers .design/,fm/ --json"
)]
pub(crate) struct Options {
    /// Published revision whose policy and source form the baseline.
    reference: String,
    /// Exact candidate revision; defaults to the current Git/JJ commit.
    #[arg(long)]
    candidate: Option<String>,
    /// Tracked workspace-relative authored schema-1 policy.
    #[arg(long)]
    policy: String,
    /// Explicit project header citation markers; no project vocabulary is built in.
    #[arg(long, value_delimiter = ',')]
    citation_markers: Vec<String>,
    /// Explicit features whose module remaps are permitted as project test support.
    #[arg(long, value_delimiter = ',')]
    test_support_features: Vec<String>,
    #[arg(long)]
    json: bool,
}
pub(crate) fn run(
    root: &Path,
    metadata: &cargo_metadata::Metadata,
    options: &Options,
) -> Result<()> {
    let started = Instant::now();
    let store = GitStore::discover(root)?;
    let (before, published_bytes, baseline_id) = {
        let baseline = store.revision(&options.reference)?;
        (
            discovery::snapshot_workspace(baseline.path())?,
            baseline.file(&options.policy)?,
            baseline.commit_id().to_owned(),
        )
    };
    let candidate_reference = match &options.candidate {
        Some(reference) => reference.clone(),
        None => store.current()?,
    };
    let after = discovery::snapshot_metadata(metadata)?;
    let (policy_bytes, candidate_id) = {
        let candidate = store.revision(&candidate_reference)?;
        store.verify_capture(&candidate, &after)?;
        (
            candidate.file(&options.policy)?,
            candidate.commit_id().to_owned(),
        )
    };
    if std::fs::read(root.join(&options.policy))? != policy_bytes {
        return Err("policy bytes changed or do not belong to the pinned candidate".into());
    }
    let published = Policy::parse(std::str::from_utf8(&published_bytes)?)?;
    let policy = Policy::parse(std::str::from_utf8(&policy_bytes)?)?;
    let basis = Basis {
        baseline: baseline_id,
        candidate: candidate_id.clone(),
    };
    let description = store.description(&candidate_id)?;
    let mut inputs = delta::Inputs::new(&after, basis, description);
    inputs.policy = policy::observe(&after, &policy);
    if !options.citation_markers.is_empty() {
        let markers = options
            .citation_markers
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        inputs.concentration = concentration::Inputs {
            before_headers: concentration::observe_headers(&before, &markers)?,
            after_headers: concentration::observe_headers(&after, &markers)?,
            citations: None,
            test_support_features: Vec::new(),
        };
    }
    inputs
        .concentration
        .test_support_features
        .clone_from(&options.test_support_features);
    let report = delta::check(&before, &after, &published, &policy, &inputs)?;
    let stats = Stats::read(&report, &policy)?;
    let display = presentation::display(
        &report,
        &policy,
        stats.as_ref(),
        started.elapsed().as_millis(),
    )?;
    let output = presentation::json(
        &report,
        &policy,
        stats.as_ref(),
        &options.policy,
        &sha256(&policy_bytes)?,
        &display,
        started.elapsed().as_millis(),
    );
    if options.json {
        println!("{}", crate::json(&output)?);
        if report.status() == Status::Unavailable {
            return Err(
                "delta evidence unavailable; supply the owning project's required observations"
                    .into(),
            );
        }
        Ok(())
    } else if report.status() == Status::Pass {
        println!("{display}");
        Ok(())
    } else {
        Err(display.into())
    }
}
#[derive(serde::Serialize)]
struct CountChange {
    before: usize,
    after: usize,
}
#[derive(serde::Serialize)]
struct Largest {
    module: String,
    block: String,
    block_loc: CountChange,
    top_level_items: CountChange,
}
struct Stats {
    before: usize,
    after: usize,
    tokens: usize,
    density_before: usize,
    density_after: usize,
    size_delta: i64,
    surface_delta: i64,
    surface: BTreeMap<String, usize>,
    largest: Option<Largest>,
    touched: usize,
    denser: bool,
    added: Vec<String>,
    removed: Vec<String>,
}
impl Stats {
    fn read(report: &delta::Report, policy: &Policy) -> Result<Option<Self>> {
        let Some(measured) = &report.measurements else {
            return Ok(None);
        };
        let total = |counts: &BTreeMap<String, delta::measure::CrateMeasure>,
                     field: fn(&delta::measure::CrateMeasure) -> usize| {
            policy
                .population
                .core
                .iter()
                .map(|name| counts.get(name).map_or(0, field))
                .sum::<usize>()
        };
        let before = total(&measured.before.crates, |row| row.token_lines);
        let after = total(&measured.after.crates, |row| row.token_lines);
        let tokens = total(&measured.after.crates, |row| row.tokens);
        let old_tokens = total(&measured.before.crates, |row| row.tokens);
        let density_before = old_tokens.saturating_mul(10) / before.max(1);
        let density_after = tokens.saturating_mul(10) / after.max(1);
        let block = |shape: Option<&atlas_core::review::Shape>| {
            shape
                .and_then(|shape| shape.largest_block.as_ref())
                .map_or(0, |block| block.lines)
        };
        let largest = measured
            .shape
            .changes
            .iter()
            .max_by_key(|change| block(change.before.as_ref()).max(block(change.after.as_ref())))
            .map(|change| Largest {
                module: change.file.clone(),
                block: change
                    .after
                    .as_ref()
                    .or(change.before.as_ref())
                    .and_then(|shape| shape.largest_block.as_ref())
                    .map_or_else(String::new, |block| block.name.clone()),
                block_loc: CountChange {
                    before: block(change.before.as_ref()),
                    after: block(change.after.as_ref()),
                },
                top_level_items: CountChange {
                    before: change
                        .before
                        .as_ref()
                        .map_or(0, |shape| shape.top_level_items),
                    after: change
                        .after
                        .as_ref()
                        .map_or(0, |shape| shape.top_level_items),
                },
            });
        let denser = (after < before && density_after > density_before)
            || largest
                .as_ref()
                .is_some_and(|largest| largest.block_loc.after > largest.block_loc.before);
        let surface = measured
            .after
            .crates
            .iter()
            .map(|(name, row)| (name.clone(), row.surface))
            .collect();
        let surface_delta = policy
            .surface
            .keys()
            .try_fold(0i64, |sum, name| -> Result<_> {
                Ok(sum
                    .checked_add(difference(
                        measured
                            .before
                            .crates
                            .get(name)
                            .map_or(0, |row| row.surface),
                        measured.after.crates.get(name).map_or(0, |row| row.surface),
                    )?)
                    .ok_or("surface delta overflow")?)
            })?;
        let edges = |dag: &BTreeMap<String, BTreeSet<String>>| {
            dag.iter()
                .flat_map(|(from, targets)| targets.iter().map(move |to| format!("{from}->{to}")))
                .collect::<BTreeSet<_>>()
        };
        let old = edges(&measured.before.dependencies);
        let new = edges(&measured.after.dependencies);
        Ok(Some(Self {
            before,
            after,
            tokens,
            density_before,
            density_after,
            size_delta: difference(before, after)?,
            surface_delta,
            surface,
            largest,
            touched: measured.shape.changes.len(),
            denser,
            added: new.difference(&old).cloned().collect(),
            removed: old.difference(&new).cloned().collect(),
        }))
    }
}
fn difference(before: usize, after: usize) -> Result<i64> {
    Ok(i64::try_from(after)?
        .checked_sub(i64::try_from(before)?)
        .ok_or("count difference overflow")?)
}

fn sha256(bytes: &[u8]) -> Result<String> {
    use std::fmt::Write as _;
    Ok(Sha256::digest(bytes).iter().try_fold(
        String::with_capacity(64),
        |mut text, byte| -> std::result::Result<_, std::fmt::Error> {
            write!(text, "{byte:02x}")?;
            Ok(text)
        },
    )?)
}
