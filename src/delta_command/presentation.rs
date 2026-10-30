//! Native gate-compatible projection and human display; no source inspection here.
use super::{Stats, difference};
use atlas::Result;
use atlas_core::checks::{
    delta,
    policy::{Policy, Status},
};
use serde_json::{Value, json};
use std::fmt::Write as _;
fn verdict(status: Status) -> &'static str {
    match status {
        Status::Pass => "PASS",
        Status::Refuse => "REFUSE",
        Status::Unavailable => "UNAVAILABLE",
    }
}
pub(super) fn json(
    report: &delta::Report,
    policy: &Policy,
    stats: Option<&Stats>,
    path: &str,
    sha256: &str,
    display: &str,
    elapsed_ms: u128,
) -> Value {
    let size=stats.map(|s|json!({"before":s.before,"after":s.after,"delta":s.size_delta,"ceiling":policy.size.core_token_lines,"tokens":s.tokens,"tokens_per_line_tenths":{"before":s.density_before,"after":s.density_after}}));
    let shape=stats.map(|s|json!({"touched_modules":s.touched,"largest_touched_block":s.largest,"denser":s.denser}));
    let coupling=stats.map_or_else(||json!({"status":"unavailable","reason":"required source measurements unavailable","added":null,"removed":null,"violations":null}),|s|json!({"status":"available","reason":null,"added":s.added,"removed":s.removed,"violations":report.coupling_violations.len()}));
    let surface=stats.map(|s|json!({"delta":s.surface_delta,"current":s.surface,"ceilings":policy.surface,"authority_homes":report.policy.authority_homes.len()}));
    let seams=report.seams.iter().map(|s|json!({"crate":s.crate_name,"debt_items":s.totals.debt_items,"unlisted_exports":s.totals.unlisted_exports,"writer_sites":s.totals.writer_sites,"test_pairs":s.totals.test_pairs,"listed":s.listed,"unlisted":s.unlisted,"floor_added":s.floor_added})).collect::<Vec<_>>();
    json!({
        "verb":"delta", "basis":report.basis,
        "policy":{"path":path,"sha256":sha256,"raises":report.raises},
        "command":format!("atlas delta {} --candidate {}",report.basis.baseline,report.basis.candidate),
        "size":size, "shape":shape, "coupling":coupling,
        "concentration":report.concentration, "surface":surface, "seams":seams,
        "violations":report.violations, "unavailable":report.unavailable,
        "verdict":verdict(report.status()), "elapsed_ms":elapsed_ms, "display":display,
    })
}
pub(super) fn display(
    report: &delta::Report,
    policy: &Policy,
    stats: Option<&Stats>,
    elapsed: u128,
) -> Result<String> {
    let before = &report.basis.baseline;
    let after = &report.basis.candidate;
    let mut out = format!(
        "atlas delta {} -> {} · {elapsed}ms",
        &before[..12.min(before.len())],
        &after[..12.min(after.len())]
    );
    if let Some(s) = stats {
        write!(
            out,
            "\nsize      {} core token-lines · {}/{} · {}.{} tokens/line",
            signed(s.size_delta),
            s.after,
            policy.size.core_token_lines,
            s.density_after / 10,
            s.density_after % 10
        )?;
        if let Some(largest) = &s.largest {
            write!(
                out,
                "\nshape     {}::{} block {}->{} · items {}->{} · {} touched{}",
                largest.module,
                largest.block,
                largest.block_loc.before,
                largest.block_loc.after,
                largest.top_level_items.before,
                largest.top_level_items.after,
                s.touched,
                if s.denser { " · DENSER" } else { "" }
            )?;
        } else {
            out.push_str("\nshape     no production module changed");
        }
    } else {
        out.push_str("\nsize      unavailable\nshape     unavailable");
    }
    if let Some(tree) = &report.concentration {
        let over = |row: &atlas_core::checks::concentration::DebtSummary| -> Result<_> {
            Ok(format!(
                "{} lines over {} ({})",
                row.over_after,
                row.limit,
                signed(difference(row.over_before, row.over_after)?)
            ))
        };
        write!(
            out,
            "\ntree      entry files {} · files {} · {} ruled names · {} aliases · {} uncited roots",
            over(&tree.entry)?,
            over(&tree.file)?,
            tree.names,
            tree.aliases,
            tree.header_uncited
        )?;
    } else {
        out.push_str("\ntree      unavailable");
    }
    if let Some(s) = stats {
        write!(
            out,
            "\ncoupling  +{} -{} edges · {} graph violations",
            s.added.len(),
            s.removed.len(),
            report.coupling_violations.len()
        )?;
        let surface = policy
            .surface
            .iter()
            .map(|(name, limit)| {
                format!(
                    "{name} {}/{limit}",
                    s.surface.get(name).copied().unwrap_or(0)
                )
            })
            .collect::<Vec<_>>()
            .join(" · ");
        write!(
            out,
            "\nsurface   {} public declarations · {surface} · {} authorities",
            signed(s.surface_delta),
            report.policy.authority_homes.len()
        )?;
    } else {
        out.push_str("\ncoupling  unavailable\nsurface   unavailable");
    }
    for seam in &report.seams {
        write!(
            out,
            "\nseams     {} · {} debt items · {} unlisted exports · {} writer sites · {} test pairs",
            seam.crate_name,
            seam.totals.debt_items,
            seam.totals.unlisted_exports,
            seam.totals.writer_sites,
            seam.totals.test_pairs
        )?;
        if !seam.floor_added.is_empty() {
            write!(
                out,
                "\nfloor     + {} (read whole by every stage)",
                seam.floor_added.join(", ")
            )?;
        }
        for (names, sign) in [(&seam.listed, '+'), (&seam.unlisted, '-')] {
            if !names.is_empty() {
                write!(out, "\nboundary  {sign} {}", names.join(", "))?;
            }
        }
    }
    write!(out, "\nverdict   {}", verdict(report.status()))?;
    for hint in report
        .concentration
        .iter()
        .flat_map(|summary| &summary.hints)
    {
        write!(out, "\nhint      {hint}")?;
    }
    for refusal in &report.violations {
        write!(out, "\nrefusal   {refusal}")?;
    }
    for missing in &report.unavailable {
        write!(out, "\nunavailable {}: {}", missing.field, missing.reason)?;
    }
    Ok(out)
}
fn signed(value: i64) -> String {
    if value > 0 {
        format!("+{value}")
    } else {
        value.to_string()
    }
}
