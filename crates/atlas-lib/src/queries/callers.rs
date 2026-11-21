//! Who calls a function, and what it calls, from the qualified calls Atlas
//! resolved, up to `depth` calls away. A resolved row is a call whose target is the
//! function. A call Atlas could not resolve is never counted as a caller:
//! when it is written with the function's name it is a *possible* caller,
//! counted by the reason it stayed unresolved. A selector that matches
//! several functions answers each on its own; no candidate is picked.
//!
//! Depth follows resolved calls only, the step the prelude's
//! `calls_function` names: each function is listed at the first depth it is
//! reached, and unresolved calls beyond depth 1 are counted, not listed.
//!
//! Limits: calls inside macro input, through a function passed elsewhere as a
//! value, and trait dispatch Atlas cannot follow are not recorded, so an
//! empty answer never means "unused". A closure's calls are recorded as
//! calls of the function that writes it.
use super::{More, selector::Selector};
use crate::{
    Result,
    model::{Declaration, SourceGraph},
    representations::{Facts, Function, ResolvedCall, UnknownCall},
};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fmt::Write as _,
};

const BASIS: &str = "Basis: qualified calls resolved through use statements and modules; possible callers are unresolved calls written with the same name.";

/// A source position: path, one-based line, zero-based column.
#[derive(Clone, Debug, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub struct Site {
    pub path: String,
    pub line: usize,
    pub column: usize,
}
/// The function an answer is about.
#[derive(Clone, Debug, Serialize)]
#[non_exhaustive]
pub struct Subject {
    pub id: String,
    pub name: String,
    pub path: String,
    pub line: usize,
}
/// One resolved call into the function.
#[derive(Clone, Debug, Serialize)]
#[non_exhaustive]
pub struct Caller {
    pub from: String,
    pub from_name: String,
    pub site: Site,
    pub depth: u32,
    pub reading: &'static str,
    /// Beyond depth 1: the function one level nearer that this call reaches.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub via: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub via_name: Option<String>,
}
/// One resolved call out of the function.
#[derive(Clone, Debug, Serialize)]
#[non_exhaustive]
pub struct Callee {
    pub to: String,
    pub to_name: String,
    pub site: Site,
    pub depth: u32,
    pub reading: &'static str,
    /// Beyond depth 1: the function one level nearer that makes this call.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub via: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub via_name: Option<String>,
}
/// A call Atlas did not resolve, with the reason kind it stayed unresolved.
#[derive(Clone, Debug, Serialize)]
#[non_exhaustive]
pub struct Unresolved {
    pub from: String,
    pub site: Site,
    pub spelling: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
    /// How the call is written, for examples: `receiver.name()` or `path()`.
    #[serde(skip)]
    pub written: String,
}
#[derive(Debug, Serialize)]
#[non_exhaustive]
pub struct CallerAnswer {
    pub function: Subject,
    pub resolved: Vec<Caller>,
    /// Unresolved calls written with the function's name: possibly calls to
    /// it, never counted as callers.
    pub possibly: Vec<Unresolved>,
    pub possibly_by_kind: BTreeMap<String, usize>,
    /// Unresolved calls written with the name of a function reached at a
    /// depth below this one, by depth: counted, not listed.
    pub possibly_beyond_depth_1: usize,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub possibly_by_depth: BTreeMap<u32, usize>,
}
#[derive(Debug, Serialize)]
#[non_exhaustive]
pub struct CalleeAnswer {
    pub function: Subject,
    pub resolved: Vec<Callee>,
    /// Calls inside the function Atlas did not resolve: external or unknown,
    /// not absent.
    pub unresolved: Vec<Unresolved>,
    pub unresolved_by_kind: BTreeMap<String, usize>,
    /// Unresolved calls inside the functions reached above this depth, by
    /// depth: counted, not listed.
    pub unresolved_beyond_depth_1: usize,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub unresolved_by_depth: BTreeMap<u32, usize>,
}
#[derive(Debug, Serialize)]
#[non_exhaustive]
pub struct Report<A> {
    pub schema: &'static str,
    pub question: &'static str,
    pub selector: String,
    pub depth: u32,
    pub answers: Vec<A>,
    pub basis: &'static str,
}

/// The functions `selector` names, tried in order: a function or declaration
/// id; `FILE:LINE`, the function whose body holds the line; `Type::method` or
/// `Trait::method`; `module::name`, by the end of its module path; a bare
/// name, every function with that name. With `file`, only functions in it.
fn select<'a>(
    declarations: &Declarations<'_>,
    facts: &'a Facts,
    selector: &str,
    file: Option<&str>,
) -> Result<Vec<&'a Function>> {
    let functions: Vec<&Function> = facts
        .functions
        .iter()
        .filter(|f| file.is_none_or(|file| f.file == file))
        .collect();
    let by_id: Vec<_> = functions
        .iter()
        .copied()
        .filter(|f| f.id == selector || declarations.of(f).is_some_and(|d| d.id == selector))
        .collect();
    if !by_id.is_empty() {
        return Ok(by_id);
    }
    let parsed = Selector::parse(selector)?;
    let matches: Vec<&Function> = match parsed {
        Selector::Site { file, line, .. } => functions
            .iter()
            .copied()
            .filter(|f| {
                f.file == file
                    && declarations.of(f).map_or(f.line == line, |d| {
                        (d.line..d.line + d.block.lines().count().max(1)).contains(&line)
                    })
            })
            // The innermost body that holds the line.
            .max_by_key(|f| f.line)
            .into_iter()
            .collect(),
        Selector::Path { ref modules, name } => {
            let owned: Vec<_> = functions
                .iter()
                .copied()
                .filter(|f| !modules.is_empty() && without_generics(&f.name) == selector)
                .collect();
            if owned.is_empty() {
                functions
                    .iter()
                    .copied()
                    .filter(|f| {
                        last(&f.name) == name
                            && declarations.of(f).is_some_and(|d| {
                                parsed.admits(&d.name, &f.file, d.line, None, || {
                                    d.modules.iter().map(String::as_str).collect()
                                })
                            })
                    })
                    .collect()
            } else {
                owned
            }
        }
    };
    if matches.is_empty() {
        return Err(
            format!("no function matches {selector:?}; try cargo atlas name {selector}").into(),
        );
    }
    Ok(matches)
}

/// The deepest `depth` a call question follows.
pub const MAX_DEPTH: u32 = 16;

/// The calls a question reads, indexed once so each step touches only its
/// own rows: resolved calls by either end, unresolved calls by the last
/// segment of their spelling and by the function they are in, each in facts
/// order.
struct Calls<'a> {
    to: HashMap<&'a str, Vec<&'a ResolvedCall>>,
    from: HashMap<&'a str, Vec<&'a ResolvedCall>>,
    written: HashMap<&'a str, Vec<&'a UnknownCall>>,
    inside: HashMap<&'a str, Vec<&'a UnknownCall>>,
}
impl<'a> Calls<'a> {
    fn new(facts: &'a Facts) -> Self {
        let mut calls = Self {
            to: HashMap::new(),
            from: HashMap::new(),
            written: HashMap::new(),
            inside: HashMap::new(),
        };
        for call in &facts.calls {
            calls.to.entry(call.to.as_str()).or_default().push(call);
            calls.from.entry(call.from.as_str()).or_default().push(call);
        }
        for unknown in &facts.unknown_calls {
            calls
                .written
                .entry(last(&unknown.call.spelling))
                .or_default()
                .push(unknown);
            calls
                .inside
                .entry(unknown.from.as_str())
                .or_default()
                .push(unknown);
        }
        calls
    }
    /// The resolved calls `depth` steps out from `start`, by level: at level
    /// k, the calls between a function first reached at k-1 and one not
    /// reached before k (every call at level 1), and the functions first
    /// reached at k. `backward` walks from callee to caller.
    fn levels(
        &self,
        start: &'a str,
        depth: u32,
        backward: bool,
    ) -> Vec<(Vec<&'a ResolvedCall>, BTreeSet<&'a str>)> {
        let (index, far): (_, fn(&'a ResolvedCall) -> &'a str) = if backward {
            (&self.to, |call| call.from.as_str())
        } else {
            (&self.from, |call| call.to.as_str())
        };
        let mut seen = BTreeSet::from([start]);
        let mut frontier = BTreeSet::from([start]);
        let mut out = Vec::new();
        for level in 1..=depth {
            let calls: Vec<&ResolvedCall> = frontier
                .iter()
                .flat_map(|near| index.get(near).into_iter().flatten().copied())
                .filter(|call| level == 1 || !seen.contains(far(call)))
                .collect();
            let reached: BTreeSet<&str> = calls
                .iter()
                .map(|call| far(call))
                .filter(|id| !seen.contains(id))
                .collect();
            seen.extend(&reached);
            out.push((calls, reached.clone()));
            if reached.is_empty() {
                break;
            }
            frontier = reached;
        }
        out
    }
    /// The unresolved calls written with one of `names`, in facts order.
    fn written_as(&self, names: &BTreeSet<&str>) -> Vec<&'a UnknownCall> {
        let mut calls: Vec<&UnknownCall> = names
            .iter()
            .flat_map(|name| self.written.get(name).into_iter().flatten().copied())
            .collect();
        if names.len() > 1 {
            calls.sort_by_key(|call| std::ptr::from_ref(*call));
        }
        calls
    }
    fn inside(&self, function: &str) -> &[&'a UnknownCall] {
        self.inside.get(function).map_or(&[], Vec::as_slice)
    }
}
/// One resolved row of either direction: the function at the far end, the
/// call's site, its depth, and beyond depth 1 the function one level nearer.
struct Row<'a> {
    far: &'a Function,
    site: Site,
    depth: u32,
    via: Option<&'a Function>,
}
/// The rows of `levels` whose far end `within` keeps, sorted by depth and site.
fn rows<'a>(
    names: &Names<'a>,
    levels: &[(Vec<&'a ResolvedCall>, BTreeSet<&'a str>)],
    backward: bool,
    within: &dyn Fn(&str) -> bool,
) -> Vec<Row<'a>> {
    let mut rows = Vec::new();
    for (level, (calls, _)) in (1..).zip(levels) {
        for call in calls {
            let (near, far) = if backward {
                (&call.to, &call.from)
            } else {
                (&call.from, &call.to)
            };
            let Some(far) = names.functions.get(far.as_str()).copied() else {
                continue;
            };
            if !within(&far.file) {
                continue;
            }
            rows.push(Row {
                far,
                site: site(&call.site),
                depth: level,
                via: (level > 1)
                    .then(|| names.functions.get(near.as_str()).copied())
                    .flatten(),
            });
        }
    }
    rows.sort_by(|a, b| (a.depth, &a.site).cmp(&(b.depth, &b.site)));
    rows
}
/// The functions that call what `selector` names, up to `depth` calls away
/// (1 to [`MAX_DEPTH`]). `within` keeps the calling functions in a part of
/// the project, by file.
pub fn callers(
    graph: &SourceGraph,
    facts: &Facts,
    selector: &str,
    file: Option<&str>,
    depth: u32,
    within: &dyn Fn(&str) -> bool,
) -> Result<Report<CallerAnswer>> {
    let depth = checked_depth(depth)?;
    let names = Names::new(graph, facts);
    let calls = Calls::new(facts);
    let kept = |unknown: &&UnknownCall| {
        names
            .functions
            .get(unknown.from.as_str())
            .is_some_and(|function| within(&function.file))
    };
    let answers = select(&names.declarations, facts, selector, file)?
        .into_iter()
        .map(|function| {
            let levels = calls.levels(&function.id, depth, true);
            let resolved = rows(&names, &levels, true, within)
                .into_iter()
                .map(|row| Caller {
                    from: row.far.id.clone(),
                    from_name: names.display(row.far),
                    site: row.site,
                    depth: row.depth,
                    reading: "qualified",
                    via: row.via.map(|via| via.id.clone()),
                    via_name: row.via.map(|via| names.display(via)),
                })
                .collect();
            let possibly: Vec<Unresolved> = calls
                .written_as(&BTreeSet::from([last(&function.name)]))
                .into_iter()
                .filter(kept)
                .map(unresolved)
                .collect();
            // Beyond depth 1, the calls written as the name of a caller
            // one level nearer.
            let possibly_by_depth: BTreeMap<u32, usize> = (2..)
                .zip(&levels)
                .take_while(|(level, _)| *level <= depth)
                .map(|(level, (_, reached))| {
                    let wanted = reached
                        .iter()
                        .filter_map(|id| names.functions.get(id))
                        .map(|function| last(&function.name))
                        .collect();
                    (
                        level,
                        calls.written_as(&wanted).into_iter().filter(kept).count(),
                    )
                })
                .filter(|(_, count)| *count > 0)
                .collect();
            CallerAnswer {
                function: subject(&names, function),
                resolved,
                possibly_by_kind: by_kind(&possibly),
                possibly,
                possibly_beyond_depth_1: possibly_by_depth.values().sum(),
                possibly_by_depth,
            }
        })
        .collect();
    Ok(Report {
        schema: "atlas.callers/1",
        question: "callers",
        selector: selector.to_owned(),
        depth,
        answers,
        basis: BASIS,
    })
}
/// What the functions `selector` names call, up to `depth` calls away.
/// `within` keeps the called functions in a part of the project, by file.
pub fn callees(
    graph: &SourceGraph,
    facts: &Facts,
    selector: &str,
    file: Option<&str>,
    depth: u32,
    within: &dyn Fn(&str) -> bool,
) -> Result<Report<CalleeAnswer>> {
    let depth = checked_depth(depth)?;
    let names = Names::new(graph, facts);
    let calls = Calls::new(facts);
    let answers = select(&names.declarations, facts, selector, file)?
        .into_iter()
        .map(|function| {
            let levels = calls.levels(&function.id, depth, false);
            let resolved = rows(&names, &levels, false, within)
                .into_iter()
                .map(|row| Callee {
                    to: row.far.id.clone(),
                    to_name: names.display(row.far),
                    site: row.site,
                    depth: row.depth,
                    reading: "qualified",
                    via: row.via.map(|via| via.id.clone()),
                    via_name: row.via.map(|via| names.display(via)),
                })
                .collect();
            let unresolved: Vec<Unresolved> = calls
                .inside(&function.id)
                .iter()
                .copied()
                .map(unresolved)
                .collect();
            // Beyond depth 1, the calls inside the functions one level nearer
            // that Atlas could not follow.
            let unresolved_by_depth: BTreeMap<u32, usize> = (2..)
                .zip(&levels)
                .take_while(|(level, _)| *level <= depth)
                .map(|(level, (_, reached))| {
                    (level, reached.iter().map(|id| calls.inside(id).len()).sum())
                })
                .filter(|(_, count)| *count > 0)
                .collect();
            CalleeAnswer {
                function: subject(&names, function),
                resolved,
                unresolved_by_kind: by_kind(&unresolved),
                unresolved,
                unresolved_beyond_depth_1: unresolved_by_depth.values().sum(),
                unresolved_by_depth,
            }
        })
        .collect();
    Ok(Report {
        schema: "atlas.callees/1",
        question: "callees",
        selector: selector.to_owned(),
        depth,
        answers,
        basis: BASIS,
    })
}
fn checked_depth(depth: u32) -> Result<u32> {
    if (1..=MAX_DEPTH).contains(&depth) {
        Ok(depth)
    } else {
        Err(format!("--depth must be 1 to {MAX_DEPTH}, not {depth}").into())
    }
}

impl Report<CallerAnswer> {
    /// Text with at most `answers` functions and `rows` rows in each of
    /// their lists; a cut list ends with `… N more; ` and the command `more`
    /// names.
    pub fn text_bounded(&self, answers: usize, rows: usize, more: More<'_>) -> String {
        let mut out = heading(
            self.question,
            &self.selector,
            self.answers.iter().map(|a| &a.function),
        );
        for answer in self.answers.iter().take(answers) {
            let function = &answer.function;
            if self.answers.len() > 1 && !out.ends_with("\n\n") {
                out.push('\n');
            }
            let _ = writeln!(
                out,
                "callers of {}  {}:{}",
                function.name, function.path, function.line
            );
            let calling = answer
                .resolved
                .iter()
                .map(|row| row.from.as_str())
                .collect::<std::collections::BTreeSet<_>>()
                .len();
            let wanted = last(&function.name);
            if answer.resolved.is_empty() && answer.possibly.is_empty() {
                let _ = writeln!(
                    out,
                    "  no resolved callers, and no unresolved call is written as {wanted:?}; calls Atlas does not record (inside macros, or through a function passed elsewhere as a value) are not covered"
                );
                continue;
            }
            let _ = writeln!(
                out,
                "  resolved: {calling} calling function{}, {} call site{}",
                plural(calling),
                answer.resolved.len(),
                plural(answer.resolved.len())
            );
            let width = answer
                .resolved
                .iter()
                .take(rows)
                .map(|row| row.site.path.len() + digits(row.site.line) + 1)
                .max()
                .unwrap_or(0);
            for row in answer.resolved.iter().take(rows) {
                let at = format!("{}:{}", row.site.path, row.site.line);
                let name = match &row.via_name {
                    Some(via) => format!("{} → {via}", row.from_name),
                    None => row.from_name.clone(),
                };
                let _ = writeln!(
                    out,
                    "      {}{at:width$}  {name}",
                    level(self.depth, row.depth)
                );
            }
            tail(&mut out, "    ", answer.resolved.len(), rows, more);
            if !answer.possibly.is_empty() {
                let _ = writeln!(
                    out,
                    "  possibly: {} unresolved call{} {} written as {wanted:?}",
                    answer.possibly.len(),
                    plural(answer.possibly.len()),
                    if answer.possibly.len() == 1 {
                        "is"
                    } else {
                        "are"
                    }
                );
                kinds(
                    &mut out,
                    &answer.possibly,
                    &answer.possibly_by_kind,
                    rows,
                    more,
                );
            }
            if answer.possibly_beyond_depth_1 > 0 {
                let _ = writeln!(
                    out,
                    "  possibly, beyond depth 1: {} unresolved call{} written as the name of a caller ({})",
                    answer.possibly_beyond_depth_1,
                    plural(answer.possibly_beyond_depth_1),
                    per_depth(&answer.possibly_by_depth)
                );
            }
        }
        if self.answers.len() > answers {
            let _ = writeln!(out);
            tail(&mut out, "", self.answers.len(), answers, more);
        }
        let _ = writeln!(out, "{}", self.basis);
        out
    }
}
impl Report<CalleeAnswer> {
    /// As [`Report::<CallerAnswer>::text_bounded`], for the calls out.
    pub fn text_bounded(&self, answers: usize, rows: usize, more: More<'_>) -> String {
        let mut out = heading(
            self.question,
            &self.selector,
            self.answers.iter().map(|a| &a.function),
        );
        for answer in self.answers.iter().take(answers) {
            let function = &answer.function;
            if self.answers.len() > 1 && !out.ends_with("\n\n") {
                out.push('\n');
            }
            let _ = writeln!(
                out,
                "callees of {}  {}:{}",
                function.name, function.path, function.line
            );
            let called = answer
                .resolved
                .iter()
                .map(|row| row.to.as_str())
                .collect::<std::collections::BTreeSet<_>>()
                .len();
            let _ = writeln!(
                out,
                "  resolved: {} call{} to {called} function{}",
                answer.resolved.len(),
                plural(answer.resolved.len()),
                plural(called)
            );
            let width = answer
                .resolved
                .iter()
                .take(rows)
                .map(|row| row.site.path.len() + digits(row.site.line) + 1)
                .max()
                .unwrap_or(0);
            for row in answer.resolved.iter().take(rows) {
                let at = format!("{}:{}", row.site.path, row.site.line);
                let name = match &row.via_name {
                    Some(via) => format!("{via} → {}", row.to_name),
                    None => row.to_name.clone(),
                };
                let _ = writeln!(
                    out,
                    "      {}{at:width$}  {name}",
                    level(self.depth, row.depth)
                );
            }
            tail(&mut out, "    ", answer.resolved.len(), rows, more);
            if !answer.unresolved.is_empty() {
                let _ = writeln!(
                    out,
                    "  unresolved: {} call{}; the rest are external or unknown, not absent",
                    answer.unresolved.len(),
                    plural(answer.unresolved.len())
                );
                kinds(
                    &mut out,
                    &answer.unresolved,
                    &answer.unresolved_by_kind,
                    rows,
                    more,
                );
            }
            if answer.unresolved_beyond_depth_1 > 0 {
                let _ = writeln!(
                    out,
                    "  unresolved, beyond depth 1: {} call{} inside the functions above, external or unknown, not absent ({})",
                    answer.unresolved_beyond_depth_1,
                    plural(answer.unresolved_beyond_depth_1),
                    per_depth(&answer.unresolved_by_depth)
                );
            }
        }
        if self.answers.len() > answers {
            let _ = writeln!(out);
            tail(&mut out, "", self.answers.len(), answers, more);
        }
        let _ = writeln!(out, "{}", self.basis);
        out
    }
}

/// A row's depth column, shown only when the question went deeper than 1.
fn level(asked: u32, depth: u32) -> String {
    if asked > 1 {
        format!("{depth:>2}  ")
    } else {
        String::new()
    }
}
/// `depth 2: 3, depth 3: 1`.
fn per_depth(counts: &BTreeMap<u32, usize>) -> String {
    counts
        .iter()
        .map(|(depth, count)| format!("depth {depth}: {count}"))
        .collect::<Vec<_>>()
        .join(", ")
}
/// The heading of an answer for several functions (R16).
/// More than 3 matches also say how to name one, from the first match:
/// its owner and name, its site, or a directory to look in.
fn heading<'a>(
    question: &str,
    selector: &str,
    answers: impl ExactSizeIterator<Item = &'a Subject>,
) -> String {
    let count = answers.len();
    if count < 2 {
        return String::new();
    }
    let mut out = format!("{selector} matches {count} functions; each is answered on its own\n");
    let mut answers = answers;
    if count > 3
        && let Some(first) = answers.next()
    {
        let dir = first
            .path
            .rsplit_once('/')
            .map_or(first.path.as_str(), |(dir, _)| dir);
        let _ = writeln!(
            out,
            "  to name one: Type::method or module::name (cargo atlas {question} {}), FILE:LINE ({}:{}), or --in PATH (--in {dir})",
            first.name, first.path, first.line
        );
    }
    out.push('\n');
    out
}
/// One line per reason kind, most calls first, each with one example.
fn kinds(
    out: &mut String,
    calls: &[Unresolved],
    by_kind: &BTreeMap<String, usize>,
    rows: usize,
    more: More<'_>,
) {
    let mut ordered: Vec<_> = by_kind.iter().collect();
    ordered.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    let width = ordered.iter().map(|(_, n)| digits(**n)).max().unwrap_or(1);
    let kind_width = ordered
        .iter()
        .map(|(kind, _)| kind.len())
        .max()
        .unwrap_or(0);
    for (kind, count) in ordered.iter().take(rows) {
        let example = calls
            .iter()
            .find(|call| &call.kind == *kind)
            .map(|call| {
                format!(
                    "  e.g. {} at {}:{}",
                    call.written, call.site.path, call.site.line
                )
            })
            .unwrap_or_default();
        let _ = writeln!(out, "      {count:>width$}  {kind:kind_width$}{example}");
    }
    tail(out, "    ", ordered.len(), rows, more);
}
fn tail(out: &mut String, indent: &str, total: usize, rows: usize, more: More<'_>) {
    if total > rows {
        let _ = writeln!(
            out,
            "{}",
            super::cut_line(indent, total - rows, &more(None))
        );
    }
}
fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}
fn digits(n: usize) -> usize {
    n.to_string().len()
}
/// `name` without generic arguments: `Selector<'a>::parse` is `Selector::parse`.
fn without_generics(name: &str) -> String {
    let mut depth = 0_usize;
    name.chars()
        .filter(|c| match c {
            '<' => {
                depth += 1;
                false
            }
            '>' => {
                depth = depth.saturating_sub(1);
                false
            }
            _ => depth == 0,
        })
        .collect()
}
/// The last `::` segment of a name or spelling.
fn last(name: &str) -> &str {
    name.rsplit("::").next().unwrap_or(name)
}
fn site(site: &crate::workflow::Site) -> Site {
    Site {
        path: site.path.clone(),
        line: site.line,
        column: site.column,
    }
}
fn unresolved(unknown: &crate::representations::UnknownCall) -> Unresolved {
    let kind = if unknown.reasons.is_empty() {
        "unclassified".to_owned()
    } else {
        unknown
            .reasons
            .iter()
            .map(|reason| crate::relations::code(&reason.kind))
            .collect::<Vec<_>>()
            .join("; ")
    };
    let call = &unknown.call;
    Unresolved {
        from: unknown.from.clone(),
        site: site(&call.site),
        spelling: call.spelling.clone(),
        kind,
        subject: unknown
            .reasons
            .iter()
            .find_map(|reason| reason.subject.clone()),
        written: match &call.receiver {
            Some(receiver) => format!("{receiver}.{}()", call.spelling),
            None => format!("{}()", call.spelling),
        },
    }
}
fn by_kind(calls: &[Unresolved]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for call in calls {
        *counts.entry(call.kind.clone()).or_default() += 1;
    }
    counts
}
fn subject(names: &Names<'_>, function: &Function) -> Subject {
    Subject {
        id: function.id.clone(),
        name: names.display(function),
        path: function.file.clone(),
        line: function.line,
    }
}

/// The lexical declaration at a function's site, for its module path and
/// body span: matched by file, line and written name, and only when unique.
struct Declarations<'a>(BTreeMap<(&'a str, usize, &'a str), Option<&'a Declaration>>);
impl<'a> Declarations<'a> {
    fn new(graph: &'a SourceGraph) -> Self {
        let mut sites = BTreeMap::new();
        for file in &graph.files {
            for declaration in file.declarations.iter().filter(|d| d.kind == "Fn") {
                sites
                    .entry((
                        file.path.as_str(),
                        declaration.line,
                        declaration.name.as_str(),
                    ))
                    .and_modify(|seen: &mut Option<&Declaration>| {
                        if seen.is_some_and(|seen| seen.id != declaration.id) {
                            *seen = None;
                        }
                    })
                    .or_insert(Some(declaration));
            }
        }
        Self(sites)
    }
    fn of(&self, function: &Function) -> Option<&'a Declaration> {
        self.0
            .get(&(function.file.as_str(), function.line, last(&function.name)))
            .copied()
            .flatten()
    }
}
/// Functions by id, and how each is named in text: a method by its owner
/// (`Snapshot::graph`), a free function by its innermost module
/// (`census::run`).
struct Names<'a> {
    functions: BTreeMap<&'a str, &'a Function>,
    declarations: Declarations<'a>,
}
impl<'a> Names<'a> {
    fn new(graph: &'a SourceGraph, facts: &'a Facts) -> Self {
        Self {
            functions: facts.functions.iter().map(|f| (f.id.as_str(), f)).collect(),
            declarations: Declarations::new(graph),
        }
    }
    fn display(&self, function: &Function) -> String {
        let name = if function.qualified.is_empty() {
            &function.name
        } else {
            &function.qualified
        };
        if name.contains("::") {
            return name.clone();
        }
        let inline = self.declarations.of(function).map(|d| d.modules.as_slice());
        let module = inline
            .and_then(<[String]>::last)
            .map(String::as_str)
            .or_else(|| super::selector::file_modules(&function.file).last())
            // A crate root's directory is not a module.
            .filter(|module| *module != "src");
        module.map_or_else(|| name.clone(), |module| format!("{module}::{name}"))
    }
}
