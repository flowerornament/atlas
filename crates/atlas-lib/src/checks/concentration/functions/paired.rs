//! The function grain's law over a change: the candidate's limits and rows
//! against the published ones, and its functions against the base tree's.
//!
//! A function limit may not rise or go, and a new `generated` path may not
//! appear. Each candidate row is matched to its published row: by its file
//! and name, or, where the file writes that name twice, by the function's
//! fingerprint, so a twin inserted above cannot take another's row; a row
//! whose function moved follows it only to one function with the same last
//! name segment and the same fingerprint, whose old place is gone. A new
//! row, or a row value that rose, is a raise for the host to admit, with
//! the row's owner. A row is a cap held to the lesser of its published
//! value and its function's base measure: a function that grows within an
//! unchanged row refuses.
use super::Measured;
use crate::checks::policy::{ConcentrationPolicy, FunctionRow, Raise};
use std::collections::BTreeSet;

/// The law's findings: refusals, and raises for the host.
#[derive(Default)]
pub(in crate::checks) struct Paired {
    pub refusals: Vec<String>,
    pub raises: Vec<Raise>,
}

/// Judge the candidate `policy` and its tree `now` against the `published`
/// policy and the `base` tree.
pub(in crate::checks) fn paired(
    policy: &ConcentrationPolicy,
    published: &ConcentrationPolicy,
    base: &[Measured],
    now: &[Measured],
) -> Paired {
    let mut out = Paired::default();
    limits(policy, published, &mut out);
    let known: BTreeSet<&str> = published
        .generated
        .iter()
        .map(|g| g.path.as_str())
        .collect();
    for generated in policy
        .generated
        .iter()
        .filter(|g| !known.contains(g.path.as_str()))
    {
        out.refusals.push(format!(
            "concentration generated gains `{}`, which takes its functions out of the function limits; a path is marked generated only with the policy that first measures it",
            generated.path
        ));
    }
    let trees = Trees {
        published,
        base,
        now,
    };
    for row in &policy.function {
        if let Some(function) = find(now, &row.file, &row.function) {
            judge_row(row, function, trees.resolve(row, function), &mut out);
        }
    }
    out
}

/// A function limit that rises or is removed refuses; setting one is free.
fn limits(policy: &ConcentrationPolicy, published: &ConcentrationPolicy, out: &mut Paired) {
    for (name, old, new) in [
        (
            "function_lines",
            published.function_lines,
            policy.function_lines,
        ),
        (
            "function_nesting",
            published.function_nesting,
            policy.function_nesting,
        ),
    ] {
        match (old, new) {
            (Some(old), Some(new)) if new > old => out.refusals.push(format!(
                "concentration {name} rose from {old} to {new}; a function limit is not raised: split the functions, or hold each with a row"
            )),
            (Some(old), None) => out.refusals.push(format!(
                "concentration {name} {old} was removed; a function limit stays once set"
            )),
            _ => {}
        }
    }
}

struct Trees<'a> {
    published: &'a ConcentrationPolicy,
    base: &'a [Measured],
    now: &'a [Measured],
}

impl<'a> Trees<'a> {
    /// The published row this candidate row continues, with its function
    /// in the base tree; none when the row is new.
    fn resolve(
        &self,
        row: &FunctionRow,
        function: &Measured,
    ) -> Option<(&'a FunctionRow, &'a Measured)> {
        let name = stem(&row.function);
        let twins = |tree: &[Measured]| {
            tree.iter()
                .filter(|f| f.file == row.file && stem(&f.function) == name)
                .count()
        };
        let in_place = if twins(self.base) > 1 || twins(self.now) > 1 {
            // A repeated name: only the same fingerprint is the same function.
            self.base.iter().find(|f| {
                f.file == row.file
                    && stem(&f.function) == name
                    && f.fingerprint == function.fingerprint
            })
        } else {
            find(self.base, &row.file, &row.function)
        };
        in_place
            .and_then(|base| Some((self.row(&base.file, &base.function)?, base)))
            .or_else(|| self.followed(function))
    }
    /// A published row whose function is gone from its place and reappears,
    /// unchanged and alone, as `function`.
    fn followed(&self, function: &Measured) -> Option<(&'a FunctionRow, &'a Measured)> {
        let last = last(&function.function);
        let mut moved = self.published.function.iter().filter_map(|row| {
            let base = find(self.base, &row.file, &row.function)?;
            let gone = find(self.now, &row.file, &row.function).is_none();
            (gone && last == self::last(&row.function) && base.fingerprint == function.fingerprint)
                .then_some((row, base))
        });
        let found = moved.next()?;
        moved.next().is_none().then_some(found)
    }
    fn row(&self, file: &str, function: &str) -> Option<&'a FunctionRow> {
        self.published
            .function
            .iter()
            .find(|row| row.file == file && row.function == function)
    }
}

/// One candidate row against its published row and base function.
fn judge_row(
    row: &FunctionRow,
    function: &Measured,
    published: Option<(&FunctionRow, &Measured)>,
    out: &mut Paired,
) {
    for (field, value, now) in [
        ("lines", row.lines, function.lines),
        ("nesting", row.nesting, function.nesting),
    ] {
        let Some(after) = value else { continue };
        let before = published.and_then(|(row, _)| field_of(row, field));
        if before.is_none_or(|before| after > before) {
            out.raises.push(Raise {
                subject: format!(
                    "concentration.function.{}::{}.{field}",
                    row.file, row.function
                ),
                before,
                after,
                measured: now,
                owner: row.owner.clone(),
            });
            continue;
        }
        let base = published.map_or(now, |(_, base)| field_of_measure(base, field));
        let cap = before.map_or(base, |before| before.min(base));
        if now > cap {
            out.refusals.push(format!(
                "function `{}` in `{}` grew to {now} {field}, above its base {base} (row {after}); a row holds a function at the lesser of the row and its base: bring it back to {cap}",
                row.function, row.file
            ));
        }
    }
}

fn field_of(row: &FunctionRow, field: &str) -> Option<usize> {
    if field == "lines" {
        row.lines
    } else {
        row.nesting
    }
}
fn field_of_measure(function: &Measured, field: &str) -> usize {
    if field == "lines" {
        function.lines
    } else {
        function.nesting
    }
}
fn find<'a>(tree: &'a [Measured], file: &str, function: &str) -> Option<&'a Measured> {
    tree.iter()
        .find(|f| f.file == file && f.function == function)
}
/// A function's name without its `#N` repeat number.
fn stem(function: &str) -> &str {
    function.split_once('#').map_or(function, |(stem, _)| stem)
}
/// The last `::` segment of a function's name, without its repeat number.
fn last(function: &str) -> &str {
    let stem = stem(function);
    stem.rsplit("::").next().unwrap_or(stem)
}

#[cfg(test)]
mod tests;
