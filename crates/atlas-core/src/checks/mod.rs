//! Source-backed architecture observations and fixed policy checks.
pub mod concentration;
pub mod delta;
pub mod policy;
pub mod stages;
pub mod typed_errors;
use crate::workspace;

use crate::discovery::module_path_of;
use crate::source::ItemKind;
use std::collections::BTreeMap;
use workspace::{CrateMap, FileMap, Workspace};

/// Stage observations and configuration findings; no complete policy PASS is implied.
#[derive(serde::Serialize)]
pub struct StageReports {
    pub crates: BTreeMap<String, stages::StageReport>,
    pub policy_violations: Vec<String>,
    pub unavailable: Vec<crate::model::Diagnostic>,
}
/// Read the native stage population from captured source and explicit authored policy.
pub fn stages(
    snapshot: &crate::discovery::Snapshot,
    policy: &policy::Policy,
    key: Option<&str>,
) -> crate::Result<StageReports> {
    let workspace = Workspace::of(snapshot)?;
    Ok(read_stages(&workspace, policy, key)?.0)
}
fn read_stages(
    workspace: &Workspace,
    policy: &policy::Policy,
    key: Option<&str>,
) -> crate::Result<(StageReports, BTreeMap<String, stages::RootTable>)> {
    let mut out = BTreeMap::new();
    let mut roots = BTreeMap::new();
    let mut unavailable = Vec::new();
    for order in policy
        .stage_order
        .iter()
        .filter(|order| key.is_none_or(|key| order.crate_name == key))
    {
        if out.contains_key(&order.crate_name) {
            return Err(format!("duplicate stage_order for {}", order.crate_name).into());
        }
        let package = workspace.crate_named(&order.crate_name).ok_or_else(|| {
            format!(
                "stage order names {}, which is not a workspace package",
                order.crate_name
            )
        })?;
        let root = stages::RootTable::of(package, crate::population::TestSelection::Production);
        let mut report = stages::StageReport::from_root(package, &order.stages, &root);
        report.interfaces = policy.stage_reads.get(&order.crate_name).cloned();
        if let Some(seams) = policy.stage_seams.get(&order.crate_name) {
            report.floor.clone_from(&seams.floor);
        }
        for issue in report
            .unjudged
            .iter()
            .filter(|issue| issue.why == stages::Unjudged::UnreadableImport)
        {
            unavailable.push(crate::model::Diagnostic {
                file: issue.file.clone(),
                line: Some(issue.line),
                package: Some(package.name.clone()),
                message: "stage use syntax could not be read".into(),
            });
        }
        roots.insert(order.crate_name.clone(), root);
        out.insert(order.crate_name.clone(), report);
    }
    if out.is_empty() && key.is_some() {
        return Err("no stage_order declared for the selected package".into());
    }
    Ok((
        StageReports {
            crates: out,
            policy_violations: stages::interface_policy_violations(
                &policy.stage_order,
                &policy.stage_reads,
                &policy.stage_seams,
                key,
            ),
            unavailable,
        },
        roots,
    ))
}

/// Item-level seam observations; unavailable required inputs never become zero counts.
pub struct SeamReports {
    pub stages: StageReports,
    pub crates: BTreeMap<String, stages::seams::SeamReading>,
    pub unavailable: Vec<crate::model::Diagnostic>,
}

/// Read native seams after project wrappers interpret stage boundary declarations.
/// Keys are Cargo package then top-level stage. Missing and stale declarations refuse.
pub fn seams(
    snapshot: &crate::discovery::Snapshot,
    policy: &policy::Policy,
    declarations: &BTreeMap<String, BTreeMap<String, stages::seams::BoundaryDeclaration>>,
    key: Option<&str>,
) -> crate::Result<SeamReports> {
    let workspace = Workspace::of(snapshot)?;
    let mut stages = StageReports {
        crates: BTreeMap::new(),
        policy_violations: Vec::new(),
        unavailable: Vec::new(),
    };
    let mut roots = BTreeMap::new();
    for name in policy
        .stage_seams
        .keys()
        .filter(|name| key.is_none_or(|key| name.as_str() == key))
    {
        if !policy
            .stage_order
            .iter()
            .any(|order| &order.crate_name == name)
        {
            continue;
        }
        let (selected, selected_roots) = read_stages(&workspace, policy, Some(name))?;
        stages.crates.extend(selected.crates);
        stages.policy_violations.extend(selected.policy_violations);
        stages.unavailable.extend(selected.unavailable);
        roots.extend(selected_roots);
    }
    let (crates, unavailable) = seam_readings(
        snapshot,
        &workspace,
        policy,
        declarations,
        key,
        &stages,
        &roots,
    )?;
    Ok(SeamReports {
        stages,
        crates,
        unavailable,
    })
}
fn seam_readings(
    snapshot: &crate::discovery::Snapshot,
    workspace: &Workspace,
    policy: &policy::Policy,
    declarations: &BTreeMap<String, BTreeMap<String, stages::seams::BoundaryDeclaration>>,
    key: Option<&str>,
    stages: &StageReports,
    roots: &BTreeMap<String, stages::RootTable>,
) -> crate::Result<(
    BTreeMap<String, stages::seams::SeamReading>,
    Vec<crate::model::Diagnostic>,
)> {
    let mut crates = BTreeMap::new();
    let mut unavailable = Vec::new();
    for (name, held) in policy
        .stage_seams
        .iter()
        .filter(|(name, _)| key.is_none_or(|key| name.as_str() == key))
    {
        let Some(order) = policy
            .stage_order
            .iter()
            .find(|order| &order.crate_name == name)
        else {
            unavailable.push(crate::model::Diagnostic {
                package: Some(name.clone()),
                file: String::new(),
                line: None,
                message: "stage seams lack their required stage_order".into(),
            });
            continue;
        };
        let package = workspace
            .crate_named(name)
            .ok_or("seamed package is absent")?;
        let supplied = declarations.get(name);
        let start = unavailable.len();
        if let Some(mounts) = snapshot.mounted_sources(name) {
            for missing in &mounts.unavailable {
                unavailable.push(crate::model::Diagnostic {
                    package: Some(name.clone()),
                    file: missing.declaring_file.clone(),
                    line: None,
                    message: format!(
                        "required test mount {} unavailable: {}",
                        missing.module,
                        missing.candidates.join(", ")
                    ),
                });
            }
        }
        for file in package
            .files
            .iter()
            .chain(package.mounted.iter().map(|mount| &mount.file))
        {
            for import in file
                .source
                .imports()
                .filter(|import| import.bindings.is_err())
            {
                unavailable.push(crate::model::Diagnostic {
                    package: Some(name.clone()),
                    file: file.rel.clone(),
                    line: Some(import.line),
                    message: "required seam import syntax unavailable".into(),
                });
            }
        }
        if unavailable.len() != start {
            continue;
        }
        let empty = BTreeMap::new();
        let reading = stages::seams::SeamReading::of(
            package,
            order,
            &stages.crates[name],
            held,
            supplied.unwrap_or(&empty),
            &roots[name],
        );
        match reading {
            Ok(reading) => {
                crates.insert(name.clone(), reading);
            }
            Err(issues) => unavailable.extend(issues),
        }
    }
    Ok((crates, unavailable))
}
