//! Who calls a function, and what it calls, from the qualified calls Atlas
//! resolved (one level). A resolved row is a call whose target is the
//! function. A call Atlas could not resolve is never counted as a caller:
//! when it is written with the function's name it is a *possible* caller,
//! counted by the reason it stayed unresolved. A selector that matches
//! several functions answers each on its own; no candidate is picked.
//!
//! Limits: calls inside macro input, through function pointers or closures
//! passed elsewhere, and trait dispatch Atlas cannot follow are not recorded,
//! so an empty answer never means "unused".
use super::{More, selector::Selector};
use crate::{
    Result,
    model::{Declaration, SourceGraph},
    representations::{Facts, Function},
};
use serde::Serialize;
use std::{collections::BTreeMap, fmt::Write as _};

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
    pub possibly_beyond_depth_1: usize,
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
    graph: &'a SourceGraph,
    facts: &'a Facts,
    selector: &str,
    file: Option<&str>,
) -> Result<Vec<&'a Function>> {
    let declarations = Declarations::new(graph);
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

/// The functions that call what `selector` names. `within` keeps the
/// calling functions in a part of the project, by file.
pub fn callers(
    graph: &SourceGraph,
    facts: &Facts,
    selector: &str,
    file: Option<&str>,
    within: &dyn Fn(&str) -> bool,
) -> Result<Report<CallerAnswer>> {
    let names = Names::new(graph, facts);
    let answers = select(graph, facts, selector, file)?
        .into_iter()
        .map(|function| {
            let mut resolved: Vec<Caller> = facts
                .calls
                .iter()
                .filter(|call| call.to == function.id)
                .filter_map(|call| {
                    let from = names.functions.get(call.from.as_str())?;
                    within(&from.file).then(|| Caller {
                        from: call.from.clone(),
                        from_name: names.display(from),
                        site: site(&call.site),
                        depth: 1,
                        reading: "qualified",
                    })
                })
                .collect();
            resolved.sort_by(|a, b| a.site.cmp(&b.site));
            let wanted = last(&function.name);
            let possibly: Vec<Unresolved> = facts
                .unknown_calls
                .iter()
                .filter(|unknown| last(&unknown.call.spelling) == wanted)
                .filter(|unknown| {
                    names
                        .functions
                        .get(unknown.from.as_str())
                        .is_some_and(|from| within(&from.file))
                })
                .map(unresolved)
                .collect();
            CallerAnswer {
                function: subject(&names, function),
                resolved,
                possibly_by_kind: by_kind(&possibly),
                possibly,
                possibly_beyond_depth_1: 0,
            }
        })
        .collect();
    Ok(Report {
        schema: "atlas.callers/1",
        question: "callers",
        selector: selector.to_owned(),
        depth: 1,
        answers,
        basis: BASIS,
    })
}
/// What the functions `selector` names call. `within` keeps the called
/// functions in a part of the project, by file.
pub fn callees(
    graph: &SourceGraph,
    facts: &Facts,
    selector: &str,
    file: Option<&str>,
    within: &dyn Fn(&str) -> bool,
) -> Result<Report<CalleeAnswer>> {
    let names = Names::new(graph, facts);
    let answers = select(graph, facts, selector, file)?
        .into_iter()
        .map(|function| {
            let mut resolved: Vec<Callee> = facts
                .calls
                .iter()
                .filter(|call| call.from == function.id)
                .filter_map(|call| {
                    let to = names.functions.get(call.to.as_str())?;
                    within(&to.file).then(|| Callee {
                        to: call.to.clone(),
                        to_name: names.display(to),
                        site: site(&call.site),
                        depth: 1,
                        reading: "qualified",
                    })
                })
                .collect();
            resolved.sort_by(|a, b| a.site.cmp(&b.site));
            let unresolved: Vec<Unresolved> = facts
                .unknown_calls
                .iter()
                .filter(|unknown| unknown.from == function.id)
                .map(unresolved)
                .collect();
            CalleeAnswer {
                function: subject(&names, function),
                resolved,
                unresolved_by_kind: by_kind(&unresolved),
                unresolved,
            }
        })
        .collect();
    Ok(Report {
        schema: "atlas.callees/1",
        question: "callees",
        selector: selector.to_owned(),
        depth: 1,
        answers,
        basis: BASIS,
    })
}

impl Report<CallerAnswer> {
    /// Text with at most `answers` functions and `rows` rows in each of
    /// their lists; a cut list ends with `… N more; ` and the command `more`
    /// names.
    pub fn text_bounded(&self, answers: usize, rows: usize, more: More<'_>) -> String {
        let mut out = heading(&self.selector, self.answers.len());
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
                    "  no resolved callers, and no unresolved call is written as {wanted:?}; calls Atlas does not record (inside macros, through function pointers or closures passed elsewhere) are not covered"
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
                let _ = writeln!(out, "      {at:width$}  {}", row.from_name);
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
        let mut out = heading(&self.selector, self.answers.len());
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
                let _ = writeln!(out, "      {at:width$}  {}", row.to_name);
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
        }
        if self.answers.len() > answers {
            let _ = writeln!(out);
            tail(&mut out, "", self.answers.len(), answers, more);
        }
        let _ = writeln!(out, "{}", self.basis);
        out
    }
}

/// The heading of an answer for several functions (R16).
fn heading(selector: &str, answers: usize) -> String {
    if answers > 1 {
        format!("{selector} matches {answers} functions; each is answered on its own\n\n")
    } else {
        String::new()
    }
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
        let _ = writeln!(out, "{indent}… {} more; {}", total - rows, more(None));
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
