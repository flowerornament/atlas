//! The function grain of `[concentration]`: each production function of a
//! population crate, measured once by the syntax pass
//! (`Source::functions`), against `function_lines`, `function_nesting` and
//! the `function` rows that hold a function above a limit.
//!
//! The population is the crates' production source and build scripts:
//! test files, test modules, test-gated items and paths under `generated`
//! are out. A row is fixed to its file and its function as `flow
//! --function` spells it. The tree law here reads one tree; the law over a
//! change (rows held to the base, rows following a moved function) is the
//! landing's.
use super::super::policy::{ConcentrationPolicy, FunctionRow};
use super::super::{FileMap, Workspace};
use crate::{architecture::contains, population::TestSelection};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

/// One production function's measures in one file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct Measured {
    pub file: String,
    /// As `flow --function` prints it, `#N` for a repeat in the file.
    pub function: String,
    pub lines: usize,
    pub nesting: usize,
    /// The signature's and body's token digest, for following a move.
    pub fingerprint: String,
}

/// Every production function of the population crates, outside
/// `generated` paths, in file and source order.
pub(in crate::checks) fn measured(ws: &Workspace, policy: &ConcentrationPolicy) -> Vec<Measured> {
    let generated = |path: &str| policy.generated.iter().any(|g| contains(&g.path, path));
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    for crate_map in ws.crates.iter().filter(|c| policy.in_population(&c.name)) {
        let files = crate_map
            .production_modules()
            .map(|(_, file)| file)
            .chain(&crate_map.build);
        for file in files.filter(|file| !generated(&file.rel)) {
            // A file two population crates read is measured once.
            if seen.insert(file.rel.clone()) {
                out.extend(production_functions(file));
            }
        }
    }
    out
}

fn production_functions(file: &FileMap) -> impl Iterator<Item = Measured> + '_ {
    file.source
        .functions()
        .iter()
        .filter(|m| TestSelection::Production.admits_offset(&file.source, m.start))
        .map(|m| Measured {
            file: file.rel.clone(),
            function: m.function.clone(),
            lines: m.lines,
            nesting: m.nesting,
            fingerprint: m.fingerprint.clone(),
        })
}

/// What the tree law found: refusals, and hints that pass.
#[derive(Default)]
pub(in crate::checks) struct Found {
    pub refusals: Vec<String>,
    pub hints: Vec<String>,
}

/// One measure of one function, against its limit and its row's value.
struct Reading<'a> {
    unit: &'static str,
    field: &'static str,
    limit: usize,
    value: usize,
    held: Option<usize>,
    function: &'a Measured,
}

/// The tree law: an unlisted function over a limit, or a row's measure
/// above its row, refuses; a row the function has fallen below, or one
/// whose function is gone or back within the limits, is a hint, and
/// refuses when the row is owned (an owned row equals its measure).
pub(in crate::checks) fn tree(policy: &ConcentrationPolicy, measured: &[Measured]) -> Found {
    let mut found = Found::default();
    let rows: BTreeMap<(&str, &str), &FunctionRow> = policy
        .function
        .iter()
        .map(|row| ((row.file.as_str(), row.function.as_str()), row))
        .collect();
    for function in measured {
        let row = rows
            .get(&(function.file.as_str(), function.function.as_str()))
            .copied();
        for reading in readings(policy, function, row) {
            judge(&reading, row, &mut found);
        }
    }
    let present: BTreeSet<(&str, &str)> = measured
        .iter()
        .map(|f| (f.file.as_str(), f.function.as_str()))
        .collect();
    for row in policy
        .function
        .iter()
        .filter(|row| !present.contains(&(row.file.as_str(), row.function.as_str())))
    {
        let change = format!(
            "function row `{}` in `{}` can be deleted: no production function has that name there",
            row.function, row.file
        );
        stale(change, row, &mut found);
    }
    found
}

fn readings<'a>(
    policy: &ConcentrationPolicy,
    function: &'a Measured,
    row: Option<&FunctionRow>,
) -> Vec<Reading<'a>> {
    let mut out = Vec::new();
    if let Some(limit) = policy.function_lines {
        out.push(Reading {
            unit: "code lines",
            field: "lines",
            limit,
            value: function.lines,
            held: row.and_then(|row| row.lines),
            function,
        });
    }
    if let Some(limit) = policy.function_nesting {
        out.push(Reading {
            unit: "levels deep",
            field: "nesting",
            limit,
            value: function.nesting,
            held: row.and_then(|row| row.nesting),
            function,
        });
    }
    out
}

fn judge(reading: &Reading<'_>, row: Option<&FunctionRow>, found: &mut Found) {
    let Reading {
        unit,
        field,
        limit,
        value,
        held,
        function,
    } = *reading;
    let (name, file) = (&function.function, &function.file);
    match (row, held) {
        (None, _) if value > limit => found.refusals.push(format!(
            "function `{name}` in `{file}` is {value} {unit}, above the {limit} limit; split it along its decisions, or hold it with the row {{ file = \"{file}\", function = \"{name}\", {field} = {value} }}"
        )),
        (Some(_), None) if value > limit => found.refusals.push(format!(
            "function `{name}` in `{file}` is {value} {unit}, above the {limit} limit, and its row gives no `{field}`; add `{field} = {value}` to it, or bring the function within the limit"
        )),
        (Some(_), Some(held)) if value > held => found.refusals.push(format!(
            "function `{name}` in `{file}` is {value} {unit}, above its row's {field} {held}; bring it back to at most {held} (a row never rises)"
        )),
        (Some(row), Some(held)) if value <= limit => stale(
            format!("function row `{name}` in `{file}` can drop `{field} = {held}`: {value} {unit}, within the {limit} limit"),
            row,
            found,
        ),
        (Some(row), Some(held)) if value < held => stale(
            format!("function row `{name}` in `{file}` can ratchet `{field}` to {value} (row {held})"),
            row,
            found,
        ),
        _ => {}
    }
}

/// A row that may be lowered or deleted: a hint, or for an owned row a
/// refusal, since an owned row must equal its measure.
fn stale(change: String, row: &FunctionRow, found: &mut Found) {
    match &row.owner {
        Some(owner) => found.refusals.push(format!(
            "{change}; it is owned by `{owner}`, and an owned row must equal its measure"
        )),
        None => found.hints.push(change),
    }
}

/// The shape of the function fields, before anything is measured.
pub(in crate::checks) fn validate(policy: &ConcentrationPolicy) -> crate::Result<()> {
    for generated in &policy.generated {
        if generated.path.trim().is_empty() || generated.reason.trim().is_empty() {
            return Err("policy: concentration generated rows need a path and a reason".into());
        }
    }
    let mut rows = BTreeSet::new();
    for row in &policy.function {
        let at = format!("function row `{}` in `{}`", row.function, row.file);
        if !rows.insert((&row.file, &row.function)) {
            return Err(format!("policy: duplicate {at}").into());
        }
        if row.lines.is_none() && row.nesting.is_none() {
            return Err(format!("policy: {at} gives neither `lines` nor `nesting`").into());
        }
        if row.lines.is_some() && policy.function_lines.is_none() {
            return Err(
                format!("policy: {at} gives `lines`, but `function_lines` is not set").into(),
            );
        }
        if row.nesting.is_some() && policy.function_nesting.is_none() {
            return Err(
                format!("policy: {at} gives `nesting`, but `function_nesting` is not set").into(),
            );
        }
        if policy
            .generated
            .iter()
            .any(|g| contains(&g.path, &row.file))
        {
            return Err(
                format!("policy: {at} is under a `generated` path, which is not measured").into(),
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
