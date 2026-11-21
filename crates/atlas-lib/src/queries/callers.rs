//! Who calls a function, and what it calls, from the qualified calls Atlas
//! resolved and, given one, a semantic reading's, up to `depth` calls away. A resolved row is a call whose target is the
//! function. A call Atlas could not resolve is never counted as a caller:
//! when it is written with the function's name it is a *possible* caller,
//! counted by the reason it stayed unresolved. A selector that matches
//! several functions answers each on its own; no candidate is picked.
//!
//! Depth follows resolved calls only, the step the prelude's
//! `calls_function` names: each function is listed at the first depth it is
//! reached, and unresolved calls beyond depth 1 are counted, not listed.
//!
//! A semantic reading (one build configuration's) adds the calls it names a
//! captured function for that the qualified reading lacks, as rows with
//! reading `semantic`, and settles the unresolved calls: confirmed (now a
//! row), ruled out (another function, an external item, a workspace item
//! that is no captured function), or left open, which alone stay listed.
//! It never removes a qualified row.
//!
//! Limits: calls inside macro input, through a function passed elsewhere as a
//! value, and trait dispatch Atlas cannot follow are not recorded, so an
//! empty answer never means "unused". A closure's calls are recorded as
//! calls of the function that writes it.
use super::{More, selector::Selector};
use crate::{
    Result,
    analysis::semantic::{Agreement, Named, Reading, Semantic},
    model::{Declaration, SourceGraph},
    representations::{Facts, Function, UnknownCall},
};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fmt::Write as _,
};

const BASIS: &str = "Basis: qualified calls resolved through use statements and modules; possible callers are unresolved calls written with the same name.";
const SEMANTIC_BASIS: &str = "Basis: qualified calls resolved through use statements and modules, and calls a SCIP index names a captured function for (reading semantic, one build configuration); possible callers are unresolved calls written with the same name that neither reading pins.";

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
    /// With a semantic reading: what it says of this call, which pins no
    /// function (`trait-method`, `local`, `no-definition`, `not-indexed`, ...).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub semantic: Option<&'static str>,
    /// How the call is written, for examples: `receiver.name()` or `path()`.
    #[serde(skip)]
    pub written: String,
}
/// What a semantic reading did with an answer's unresolved calls (at depth
/// 1): each is confirmed, ruled out, or left open, and only the open ones
/// stay listed.
#[derive(Debug, Default, Serialize)]
#[non_exhaustive]
pub struct Settled {
    /// Calls the index names a captured function for: listed as resolved,
    /// reading `semantic`. For callers, only calls of this function.
    pub confirmed: usize,
    /// Callers only: calls written with the name that the index names
    /// another captured function for.
    #[serde(skip_serializing_if = "is_zero")]
    pub other_function: usize,
    /// Calls the index names something other than a captured function for,
    /// by reading: `external` (another package) or `definition` (a workspace
    /// item that is no captured function, such as a variant constructor).
    pub elsewhere: BTreeMap<&'static str, usize>,
    /// Calls the index pins to nothing, by reading: still listed.
    pub open: BTreeMap<&'static str, usize>,
}
#[allow(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde's skip_serializing_if passes a reference"
)]
fn is_zero(n: &usize) -> bool {
    *n == 0
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
    /// With a semantic reading: how it settled the unresolved calls written
    /// with the name; `possibly` holds only those it left open.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub semantic: Option<Settled>,
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
    /// With a semantic reading: how it settled the calls inside the
    /// function; `unresolved` holds only those it left open.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub semantic: Option<Settled>,
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
    /// With a semantic reading: its provenance and coverage.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub semantic: Vec<String>,
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
    to: HashMap<&'a str, Vec<Edge<'a>>>,
    from: HashMap<&'a str, Vec<Edge<'a>>>,
    written: HashMap<&'a str, Vec<&'a UnknownCall>>,
    inside: HashMap<&'a str, Vec<&'a UnknownCall>>,
    /// With a semantic reading: what it says of each recorded call, by site
    /// and spelling.
    pins: HashMap<(&'a str, usize, usize, &'a str), Pin<'a>>,
}
/// One resolved call by one reading.
#[derive(Clone, Copy)]
struct Edge<'a> {
    from: &'a str,
    to: &'a str,
    site: &'a crate::workflow::Site,
    reading: &'static str,
}
/// What a semantic reading says of a call Atlas left unresolved.
#[derive(Clone, Copy)]
enum Pin<'a> {
    /// A captured workspace function.
    Function(&'a str),
    /// A workspace item that is not a captured function (a struct or enum
    /// variant constructor), or an item of another package.
    Elsewhere(&'static str),
    /// Nothing that rules a function out: open dispatch, a local value, a
    /// generated or inactive symbol, or no usable index entry.
    Open(&'static str),
}
impl<'a> Calls<'a> {
    fn new(facts: &'a Facts, semantic: &[Named<'a>]) -> Self {
        let mut calls = Self {
            to: HashMap::new(),
            from: HashMap::new(),
            written: HashMap::new(),
            inside: HashMap::new(),
            pins: HashMap::new(),
        };
        let mut add = |edge: Edge<'a>| {
            calls.to.entry(edge.to).or_default().push(edge);
            calls.from.entry(edge.from).or_default().push(edge);
        };
        for call in &facts.calls {
            add(Edge {
                from: &call.from,
                to: &call.to,
                site: &call.site,
                reading: "qualified",
            });
        }
        // The semantic edges the qualified reading lacks; an agreeing one is
        // the same call, already an edge.
        for named in semantic {
            if let (Some(from), Some(to)) = (named.from, named.to)
                && named.call.agreement != Some(Agreement::Agrees)
            {
                add(Edge {
                    from,
                    to,
                    site: &named.call.site,
                    reading: "semantic",
                });
            }
        }
        for named in semantic {
            let site = &named.call.site;
            calls.pins.insert(
                (
                    site.path.as_str(),
                    site.line,
                    site.column,
                    named.call.spelling.as_str(),
                ),
                pin(named),
            );
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
    ) -> Vec<(Vec<Edge<'a>>, BTreeSet<&'a str>)> {
        let (index, far): (_, fn(&Edge<'a>) -> &'a str) = if backward {
            (&self.to, |edge| edge.from)
        } else {
            (&self.from, |edge| edge.to)
        };
        let mut seen = BTreeSet::from([start]);
        let mut frontier = BTreeSet::from([start]);
        let mut out = Vec::new();
        for level in 1..=depth {
            let calls: Vec<Edge<'a>> = frontier
                .iter()
                .flat_map(|near| index.get(near).into_iter().flatten().copied())
                .filter(|edge| level == 1 || !seen.contains(far(edge)))
                .collect();
            let reached: BTreeSet<&str> = calls
                .iter()
                .map(far)
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
    /// What the semantic reading says of an unresolved call, if one was given.
    fn pin(&self, unknown: &UnknownCall) -> Option<Pin<'a>> {
        let site = &unknown.call.site;
        self.pins
            .get(&(
                site.path.as_str(),
                site.line,
                site.column,
                unknown.call.spelling.as_str(),
            ))
            .copied()
    }
}
impl Calls<'_> {
    /// Whether an unresolved call is still unresolved after the semantic
    /// reading: it pins nothing, or there is no reading.
    fn remains(&self, unknown: &UnknownCall) -> bool {
        matches!(self.pin(unknown), None | Some(Pin::Open(_)))
    }
    /// Sort `unknowns` by what the semantic reading says of them: the ones
    /// it leaves open, as rows, and how it settled the rest. `this` is the
    /// function a callers question is about; a callees question passes none,
    /// and every function the index names confirms. Without a reading every
    /// call stays open and no summary is made.
    fn settle<'u>(
        &self,
        unknowns: impl IntoIterator<Item = &'u UnknownCall>,
        this: Option<&str>,
        semantic: bool,
    ) -> (Vec<Unresolved>, Option<Settled>) {
        let mut settled = Settled::default();
        let mut open = Vec::new();
        for unknown in unknowns {
            match self.pin(unknown) {
                Some(Pin::Function(id)) if this.is_none_or(|this| this == id) => {
                    settled.confirmed += 1;
                }
                Some(Pin::Function(_)) => settled.other_function += 1,
                Some(Pin::Elsewhere(reading)) => {
                    *settled.elsewhere.entry(reading).or_default() += 1;
                }
                pin => {
                    let mut row = unresolved(unknown);
                    if semantic {
                        // A call the reading did not cover at all.
                        let reading = match pin {
                            Some(Pin::Open(reading)) => reading,
                            _ => "unread",
                        };
                        row.semantic = Some(reading);
                        *settled.open.entry(reading).or_default() += 1;
                    }
                    open.push(row);
                }
            }
        }
        (open, semantic.then_some(settled))
    }
}
fn pin<'a>(named: &Named<'a>) -> Pin<'a> {
    match (&named.call.semantic, named.to) {
        (_, Some(function)) => Pin::Function(function),
        (Semantic::Definition { .. }, None) if named.shared => Pin::Open("shared-site"),
        (Semantic::Definition { .. }, None) => Pin::Elsewhere("definition"),
        (Semantic::External { .. }, _) => Pin::Elsewhere("external"),
        (Semantic::TraitMethod { .. }, _) => Pin::Open("trait-method"),
        (Semantic::Local, _) => Pin::Open("local"),
        (Semantic::NoDefinition { .. }, _) => Pin::Open("no-definition"),
        (Semantic::Ambiguous { .. }, _) => Pin::Open("ambiguous"),
        (Semantic::Mismatch { .. }, _) => Pin::Open("mismatch"),
        (Semantic::NotIndexed { .. }, _) => Pin::Open("not-indexed"),
    }
}
/// One resolved row of either direction: the function at the far end, the
/// call's site, its depth, and beyond depth 1 the function one level nearer.
struct Row<'a> {
    far: &'a Function,
    site: Site,
    depth: u32,
    via: Option<&'a Function>,
    reading: &'static str,
}
/// The rows of `levels` whose far end `within` keeps, sorted by depth and site.
fn rows<'a>(
    names: &Names<'a>,
    levels: &[(Vec<Edge<'a>>, BTreeSet<&'a str>)],
    backward: bool,
    within: &dyn Fn(&str) -> bool,
) -> Vec<Row<'a>> {
    let mut rows = Vec::new();
    for (level, (calls, _)) in (1..).zip(levels) {
        for call in calls {
            let (near, far) = if backward {
                (call.to, call.from)
            } else {
                (call.from, call.to)
            };
            let Some(far) = names.functions.get(far).copied() else {
                continue;
            };
            if !within(&far.file) {
                continue;
            }
            rows.push(Row {
                far,
                site: site(call.site),
                depth: level,
                via: (level > 1)
                    .then(|| names.functions.get(near).copied())
                    .flatten(),
                reading: call.reading,
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
    callers_by(graph, facts, selector, file, depth, within, None)
}
/// As [`callers`], adding the calls `reading` names a captured function for
/// (rows with reading `semantic`, where the qualified reading has none), and
/// listing as possible only the calls neither reading pins. The caller
/// passes the reading made over these `facts` (`Analysis::semantic`); one
/// whose index is unavailable is refused with its reason.
pub fn semantic_callers(
    graph: &SourceGraph,
    facts: &Facts,
    selector: &str,
    file: Option<&str>,
    depth: u32,
    within: &dyn Fn(&str) -> bool,
    reading: &Reading,
) -> Result<Report<CallerAnswer>> {
    usable(reading)?;
    callers_by(graph, facts, selector, file, depth, within, Some(reading))
}
fn usable(reading: &Reading) -> Result<()> {
    match &reading.unavailable {
        Some(reason) => Err(format!("the semantic index is unavailable: {reason}").into()),
        None => Ok(()),
    }
}
fn callers_by(
    graph: &SourceGraph,
    facts: &Facts,
    selector: &str,
    file: Option<&str>,
    depth: u32,
    within: &dyn Fn(&str) -> bool,
    reading: Option<&Reading>,
) -> Result<Report<CallerAnswer>> {
    let depth = checked_depth(depth)?;
    let names = Names::new(graph, facts);
    let named = reading
        .map(|reading| reading.named(facts))
        .unwrap_or_default();
    let calls = Calls::new(facts, &named);
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
                    reading: row.reading,
                    via: row.via.map(|via| via.id.clone()),
                    via_name: row.via.map(|via| names.display(via)),
                })
                .collect();
            let (possibly, semantic) = calls.settle(
                calls
                    .written_as(&BTreeSet::from([last(&function.name)]))
                    .into_iter()
                    .filter(kept),
                Some(&function.id),
                reading.is_some(),
            );
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
                        calls
                            .written_as(&wanted)
                            .into_iter()
                            .filter(kept)
                            .filter(|unknown| calls.remains(unknown))
                            .count(),
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
                semantic,
            }
        })
        .collect();
    Ok(Report {
        schema: "atlas.callers/1",
        question: "callers",
        selector: selector.to_owned(),
        depth,
        answers,
        basis: basis(reading),
        semantic: reading.map(Reading::describe).unwrap_or_default(),
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
    callees_by(graph, facts, selector, file, depth, within, None)
}
/// As [`callees`], with `reading` as [`semantic_callers`] takes it: the
/// calls it names a captured function for are resolved rows, and only the
/// calls it pins to nothing stay listed as unresolved.
pub fn semantic_callees(
    graph: &SourceGraph,
    facts: &Facts,
    selector: &str,
    file: Option<&str>,
    depth: u32,
    within: &dyn Fn(&str) -> bool,
    reading: &Reading,
) -> Result<Report<CalleeAnswer>> {
    usable(reading)?;
    callees_by(graph, facts, selector, file, depth, within, Some(reading))
}
fn callees_by(
    graph: &SourceGraph,
    facts: &Facts,
    selector: &str,
    file: Option<&str>,
    depth: u32,
    within: &dyn Fn(&str) -> bool,
    reading: Option<&Reading>,
) -> Result<Report<CalleeAnswer>> {
    let depth = checked_depth(depth)?;
    let names = Names::new(graph, facts);
    let named = reading
        .map(|reading| reading.named(facts))
        .unwrap_or_default();
    let calls = Calls::new(facts, &named);
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
                    reading: row.reading,
                    via: row.via.map(|via| via.id.clone()),
                    via_name: row.via.map(|via| names.display(via)),
                })
                .collect();
            let (unresolved, semantic) = calls.settle(
                calls.inside(&function.id).iter().copied(),
                None,
                reading.is_some(),
            );
            // Beyond depth 1, the calls inside the functions one level nearer
            // that Atlas could not follow.
            let unresolved_by_depth: BTreeMap<u32, usize> = (2..)
                .zip(&levels)
                .take_while(|(level, _)| *level <= depth)
                .map(|(level, (_, reached))| {
                    let open = reached
                        .iter()
                        .flat_map(|id| calls.inside(id))
                        .filter(|unknown| calls.remains(unknown))
                        .count();
                    (level, open)
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
                semantic,
            }
        })
        .collect();
    Ok(Report {
        schema: "atlas.callees/1",
        question: "callees",
        selector: selector.to_owned(),
        depth,
        answers,
        basis: basis(reading),
        semantic: reading.map(Reading::describe).unwrap_or_default(),
    })
}
fn basis(reading: Option<&Reading>) -> &'static str {
    if reading.is_some() {
        SEMANTIC_BASIS
    } else {
        BASIS
    }
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
                let none = if answer.semantic.is_some() {
                    format!(
                        "no resolved callers by either reading, and no unresolved call written as {wanted:?} is left open"
                    )
                } else {
                    format!("no resolved callers, and no unresolved call is written as {wanted:?}")
                };
                let _ = writeln!(
                    out,
                    "  {none}; calls Atlas does not record (inside macros, or through a function passed elsewhere as a value) are not covered"
                );
                settled(&mut out, answer.semantic.as_ref());
                continue;
            }
            let _ = writeln!(
                out,
                "  resolved: {calling} calling function{}, {} call site{}{}",
                plural(calling),
                answer.resolved.len(),
                plural(answer.resolved.len()),
                by_reading(
                    answer.semantic.as_ref(),
                    answer.resolved.iter().map(|row| row.reading)
                )
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
                    "      {}{at:width$}  {name}{}",
                    level(self.depth, row.depth),
                    marker(row.reading)
                );
            }
            tail(&mut out, "    ", answer.resolved.len(), rows, more);
            if !answer.possibly.is_empty() {
                let _ = writeln!(
                    out,
                    "  possibly: {} unresolved call{} {} written as {wanted:?}{}",
                    answer.possibly.len(),
                    plural(answer.possibly.len()),
                    if answer.possibly.len() == 1 {
                        "is"
                    } else {
                        "are"
                    },
                    if answer.semantic.is_some() {
                        " and pinned by neither reading"
                    } else {
                        ""
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
            settled(&mut out, answer.semantic.as_ref());
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
        footer(&mut out, self.basis, &self.semantic);
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
                "  resolved: {} call{} to {called} function{}{}",
                answer.resolved.len(),
                plural(answer.resolved.len()),
                plural(called),
                by_reading(
                    answer.semantic.as_ref(),
                    answer.resolved.iter().map(|row| row.reading)
                )
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
                    "      {}{at:width$}  {name}{}",
                    level(self.depth, row.depth),
                    marker(row.reading)
                );
            }
            tail(&mut out, "    ", answer.resolved.len(), rows, more);
            if !answer.unresolved.is_empty() {
                let rest = if answer.semantic.is_some() {
                    " that neither reading pins: unknown, not absent"
                } else {
                    "; the rest are external or unknown, not absent"
                };
                let _ = writeln!(
                    out,
                    "  unresolved: {} call{}{rest}",
                    answer.unresolved.len(),
                    plural(answer.unresolved.len()),
                );
                kinds(
                    &mut out,
                    &answer.unresolved,
                    &answer.unresolved_by_kind,
                    rows,
                    more,
                );
            }
            settled(&mut out, answer.semantic.as_ref());
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
        footer(&mut out, self.basis, &self.semantic);
        out
    }
}

/// ` (qualified 3, semantic only 1)` after a resolved count, with a reading.
fn by_reading<'r>(settled: Option<&Settled>, readings: impl Iterator<Item = &'r str>) -> String {
    if settled.is_none() {
        return String::new();
    }
    let (mut qualified, mut semantic) = (0, 0);
    for reading in readings {
        if reading == "semantic" {
            semantic += 1;
        } else {
            qualified += 1;
        }
    }
    format!(" (qualified {qualified}, semantic only {semantic})")
}
/// A resolved row's mark when only the semantic reading names it.
fn marker(reading: &str) -> &'static str {
    if reading == "semantic" {
        "  [semantic]"
    } else {
        ""
    }
}
/// What the semantic reading settled, after an answer's unresolved list.
fn settled(out: &mut String, settled: Option<&Settled>) {
    let Some(settled) = settled else {
        return;
    };
    let counts = |counts: &BTreeMap<&str, usize>| {
        counts
            .iter()
            .map(|(reading, n)| format!("{reading} {n}"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let mut parts = Vec::new();
    if settled.confirmed > 0 {
        parts.push(format!(
            "confirmed {} (rows marked [semantic])",
            settled.confirmed
        ));
    }
    if settled.other_function > 0 {
        parts.push(format!(
            "names another function for {}",
            settled.other_function
        ));
    }
    let elsewhere: usize = settled.elsewhere.values().sum();
    if elsewhere > 0 {
        parts.push(format!(
            "not a captured function {elsewhere} ({})",
            counts(&settled.elsewhere)
        ));
    }
    let open: usize = settled.open.values().sum();
    if open > 0 {
        parts.push(format!("left open {open} ({})", counts(&settled.open)));
    }
    if !parts.is_empty() {
        let _ = writeln!(
            out,
            "  the index read the unresolved calls: {}",
            parts.join("; ")
        );
    }
}
fn footer(out: &mut String, basis: &str, semantic: &[String]) {
    let _ = writeln!(out, "{basis}");
    for line in semantic {
        let _ = writeln!(out, "{line}");
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
        semantic: None,
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
