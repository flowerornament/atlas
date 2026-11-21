//! Arrival report: one bounded page of measured facts for a reader who has not
//! seen the project. Each list states its unit, its order and its total, and
//! names the command that shows the rest. Nothing here ranks importance: an
//! order is only "by that measure", ties are reported, and a follow-up command
//! is never built from an arbitrary pick among equal rows or several binaries.
//!
//! Limits: called functions count resolved calls only, so they are a lower
//! bound; type mentions count unique lexical targets only. Without a
//! semantic index no share of calls is given: most unresolved calls are
//! into other packages, which Atlas does not resolve, and only an index
//! tells them apart. With one, the share is of workspace-function calls,
//! and the calls neither reading pins are counted apart. Diagnostics and
//! unknown-call reasons are grouped by their stable codes, each with the
//! first message as an example.
use crate::{
    Result,
    analysis::{
        Analysis,
        semantic::{Agreement, Named, Reading, SemanticIndex},
    },
    discovery::Snapshot,
    model::{Declaration, Resolution, SourceGraph},
    relations::code,
    representations::{Facts, Function},
    source::discovery::target_key,
    suggestion::Command,
};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
};

const BASIS: &str = "Source observations only: counts are not use, size is not quality, and each order is by the stated measure.";
/// Lines read from the top of a file when looking for an `@generated` marker.
const GENERATED_MARKER_LINES: usize = 5;

/// The arrival report for a workspace, one package or one path.
#[derive(Debug, Serialize)]
#[non_exhaustive]
pub struct Orientation {
    pub schema: u32,
    /// `workspace`, a package name, or a source path.
    pub scope: String,
    pub packages: Vec<Package>,
    /// Physical source files in scope, a path shared by packages counted once.
    pub files: usize,
    pub test_only_files: usize,
    /// Files whose first lines carry an `@generated` marker in a `//` comment.
    pub generated_files: Vec<String>,
    /// Files no Cargo target's module tree reaches (they have no mount).
    pub unmounted_files: Vec<String>,
    /// In a fixed order: `mass`, `blocks`, `called`, `mentioned`.
    pub sections: Vec<Section>,
    pub resolution: ResolutionSummary,
    pub next: Vec<NextCommand>,
    pub basis: &'static str,
}

#[derive(Debug, Serialize)]
#[non_exhaustive]
pub struct Package {
    pub name: String,
    pub targets: Vec<Target>,
}

/// A Cargo target as `cargo metadata` reports it.
#[derive(Debug, Serialize)]
#[non_exhaustive]
pub struct Target {
    /// Cargo's kinds, such as `lib`, `bin`, `test` or `proc-macro`.
    pub kinds: Vec<String>,
    pub name: String,
    /// The target's root file, relative to the workspace root.
    pub root: String,
}
impl Target {
    /// A library or binary root, where a reader starts; tests, benches and
    /// examples are counted rather than listed.
    pub fn is_entry(&self) -> bool {
        self.kinds.iter().any(|k| {
            matches!(
                k.as_str(),
                "lib" | "rlib" | "dylib" | "cdylib" | "staticlib" | "proc-macro" | "bin"
            )
        })
    }
}

/// One measured list. The unit and order are part of the data, so a formatter
/// cannot show the rows without them.
#[derive(Debug, Serialize)]
#[non_exhaustive]
pub struct Section {
    /// Stable key: `mass`, `blocks`, `called` or `mentioned`.
    pub id: &'static str,
    pub title: &'static str,
    /// What `count` measures.
    pub unit: &'static str,
    pub order: &'static str,
    /// The population the rows are drawn from, in `total_unit`.
    pub total: usize,
    pub total_unit: &'static str,
    pub rows: Vec<Row>,
    /// Rows tied with the first row, counting rows not shown; 1 means no tie.
    pub tied: usize,
    /// Rows left out of `rows`.
    pub more: usize,
    /// The command that lists every row, as arguments after `cargo atlas`.
    pub all: Vec<String>,
}

#[derive(Debug, Serialize)]
#[non_exhaustive]
pub struct Row {
    pub count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// A file, or a directory ending in `/`.
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    /// Files in a directory row.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub files: Option<usize>,
    /// Non-test lines of a directory row that are in `@generated` files.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generated_lines: Option<usize>,
    /// With a semantic index, in the `called` section: calling functions the
    /// index adds, beyond the resolved ones `count` holds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub by_index: Option<usize>,
}
impl Row {
    /// A row for one declaration site.
    fn site(count: usize, name: String, path: String, line: usize) -> Self {
        Self {
            count,
            name: Some(name),
            path,
            line: Some(line),
            files: None,
            generated_lines: None,
            by_index: None,
        }
    }
}

/// How much of the scope Atlas could resolve. Unresolved is not absent.
#[derive(Debug, Default, Serialize)]
#[non_exhaustive]
pub struct ResolutionSummary {
    /// Call sites, in functions in scope, resolved to one workspace function.
    pub calls_resolved: usize,
    /// Call sites in scope that are external, ambiguous or not established.
    pub calls_unknown: usize,
    /// Unknown calls grouped by their reason kinds; most first.
    pub unknown_call_reasons: Vec<Count>,
    /// Lexical type mentions from declarations in scope, by resolution.
    pub mentions_unique: usize,
    pub mentions_ambiguous: usize,
    pub mentions_unresolved: usize,
    pub diagnostics: usize,
    /// Diagnostics grouped by code; most first.
    pub diagnostic_kinds: Vec<Count>,
    /// With a semantic index: what it says of the unknown calls.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub semantic: Option<SemanticCalls>,
}

/// A semantic index's reading of the unknown calls in scope. The resolved
/// calls are workspace calls by Atlas's own reading and are not re-counted.
#[derive(Debug, Default, Serialize)]
#[non_exhaustive]
pub struct SemanticCalls {
    /// Unknown calls the index names a captured workspace function for.
    pub workspace: usize,
    /// Unknown calls the index names an item of another package for.
    pub external: usize,
    /// Unknown calls to a workspace item that is no captured function (a
    /// struct or variant constructor, a macro-written function).
    pub other_items: usize,
    /// Unknown calls the index pins to nothing, by its reading.
    pub open: BTreeMap<&'static str, usize>,
    /// The index's provenance and coverage.
    pub reading: Vec<String>,
}

#[derive(Debug, Serialize)]
#[non_exhaustive]
pub struct Count {
    /// A stable code, or several joined with `; ` for a call with several
    /// reasons.
    pub kind: String,
    pub count: usize,
    /// The first message of this kind, as an example.
    pub example: String,
}

/// A follow-up command built from names on the page.
#[derive(Debug, Serialize)]
#[non_exhaustive]
pub struct NextCommand {
    /// Arguments after `cargo atlas`. A placeholder argument is in `<angle brackets>`.
    pub args: Vec<String>,
    /// Which row produced it.
    pub why: String,
    /// False when an argument is a placeholder: no single candidate exists.
    pub runnable: bool,
}

/// Measure the arrival report for the workspace, or for a package name or
/// source path. `rows` bounds each list; every list reports what it left out.
///
/// # Errors
/// A scope with no discovered files, a file whose syntax cannot be read, or
/// a failed qualified analysis.
pub fn orient(snapshot: &Snapshot, scope: Option<&str>, rows: usize) -> Result<Orientation> {
    orient_by(snapshot, scope, rows, None)
}
/// As [`orient`], with a semantic index's reading of the calls: unknown
/// calls split into other packages, workspace functions and open ones, and
/// the most-called rows count the callers the index adds. An unavailable
/// index is refused with its reason.
pub fn semantic_orient(
    snapshot: &Snapshot,
    scope: Option<&str>,
    rows: usize,
    index: &SemanticIndex,
) -> Result<Orientation> {
    orient_by(snapshot, scope, rows, Some(index))
}
fn orient_by(
    snapshot: &Snapshot,
    scope: Option<&str>,
    rows: usize,
    index: Option<&SemanticIndex>,
) -> Result<Orientation> {
    let graph = snapshot.graph();
    let observations = super::files(graph, scope)?;
    let paths: BTreeSet<&str> = observations.iter().map(|f| f.path.as_str()).collect();
    let package_names: BTreeSet<&str> =
        observations.iter().map(|f| f.crate_name.as_str()).collect();
    let in_scope = |path: &str| paths.contains(path);

    let packages = packages(snapshot, &package_names);
    let census = file_census(snapshot, &paths);
    let mass = code_mass(snapshot, &in_scope, &census)?;

    let analysis = Analysis::new(snapshot)?;
    let facts = analysis.representations()?;
    let reading = index.map(|index| analysis.semantic(index)).transpose()?;
    if let Some(reason) = reading.as_ref().and_then(|r| r.unavailable.as_ref()) {
        return Err(format!("the semantic index is unavailable: {reason}").into());
    }
    let named = reading
        .as_ref()
        .map(|r| r.named(&facts))
        .unwrap_or_default();
    let mut resolution = ResolutionSummary::default();
    let called = call_measures(&facts, &named, reading.as_ref(), &in_scope, &mut resolution);
    let mentioned = type_mentions(graph, &in_scope, &mut resolution);
    let mut kinds = BTreeMap::new();
    for diagnostic in super::diagnostics_of(graph, &observations) {
        resolution.diagnostics += 1;
        tally(&mut kinds, code(&diagnostic.code), &diagnostic.message);
    }
    resolution.diagnostic_kinds = counts(kinds);

    // Follow-up commands take a physical path: a package scope becomes its directory.
    let physical_scope =
        scope.map(|scope| package_directory(snapshot, scope).unwrap_or_else(|| scope.to_owned()));
    let mut sections = Vec::from(size_sections(mass, physical_scope.as_deref(), rows));
    sections.extend(reference_sections(
        called,
        mentioned,
        physical_scope.as_deref(),
        rows,
    ));
    let next = next_commands(&sections, &packages, &facts.functions);
    Ok(Orientation {
        schema: 1,
        scope: scope.unwrap_or("workspace").to_owned(),
        packages,
        files: census.files,
        test_only_files: census.test_only.len(),
        generated_files: census.generated.into_iter().collect(),
        unmounted_files: census.unmounted,
        sections,
        resolution,
        next,
        basis: BASIS,
    })
}

/// The physical files in scope, by what sets them apart from ordinary
/// production source.
struct FileCensus<'g> {
    /// Physical files in scope, a path shared by packages counted once.
    files: usize,
    test_only: BTreeSet<&'g str>,
    generated: BTreeSet<String>,
    /// Files in scope with no mount in any target's module tree.
    unmounted: Vec<String>,
}
fn file_census<'g>(snapshot: &'g Snapshot, paths: &BTreeSet<&str>) -> FileCensus<'g> {
    let graph = snapshot.graph();
    let physical: Vec<_> = graph
        .physical_files()
        .filter(|f| paths.contains(f.path.as_str()))
        .collect();
    let test_only: BTreeSet<&str> = physical
        .iter()
        .filter(|f| f.test_only)
        .map(|f| f.path.as_str())
        .collect();
    let generated: BTreeSet<String> = physical
        .iter()
        .filter(|f| snapshot.source(&f.path).is_some_and(has_generated_marker))
        .map(|f| f.path.clone())
        .collect();
    let mounted: BTreeSet<&str> = graph
        .files
        .iter()
        .filter(|f| !f.mounts.is_empty())
        .map(|f| f.path.as_str())
        .collect();
    let unmounted: Vec<String> = paths
        .iter()
        .filter(|p| !mounted.contains(*p))
        .map(|p| (*p).to_owned())
        .collect();
    FileCensus {
        files: physical.len(),
        test_only,
        generated,
        unmounted,
    }
}

/// Non-test lines of the production files in scope: by directory, and the
/// largest block of each file that is not `@generated`.
struct CodeMass {
    lines: usize,
    directories: Vec<Row>,
    blocks: Vec<Row>,
}
fn code_mass(
    snapshot: &Snapshot,
    in_scope: &dyn Fn(&str) -> bool,
    census: &FileCensus<'_>,
) -> Result<CodeMass> {
    let outline = crate::review::outline(snapshot, ".")?;
    let mut dirs: BTreeMap<String, (usize, usize, usize)> = BTreeMap::new();
    let mut blocks = Vec::new();
    for file in outline
        .files
        .iter()
        .filter(|f| in_scope(&f.file) && !census.test_only.contains(f.file.as_str()))
    {
        let dir = file.file.rsplit_once('/').map_or("", |(dir, _)| dir);
        let entry = dirs.entry(format!("{dir}/")).or_default();
        entry.0 += file.non_test_lines;
        entry.1 += 1;
        if census.generated.contains(&file.file) {
            entry.2 += file.non_test_lines;
        } else if let Some(block) = &file.largest_block {
            blocks.push(Row::site(
                block.lines,
                block.name.clone(),
                file.file.clone(),
                block.line,
            ));
        }
    }
    let lines: usize = dirs.values().map(|v| v.0).sum();
    let directories: Vec<Row> = dirs
        .into_iter()
        .map(|(path, (count, files, generated_lines))| Row {
            count,
            name: None,
            path,
            line: None,
            files: Some(files),
            generated_lines: (generated_lines > 0).then_some(generated_lines),
            by_index: None,
        })
        .collect();
    Ok(CodeMass {
        lines,
        directories,
        blocks,
    })
}

/// The most-called rows, and the call counts of `resolution`: resolved
/// calls, unknown calls by reason, and with a reading what it pins them to
/// and the callers it adds.
fn call_measures(
    facts: &Facts,
    named: &[Named<'_>],
    reading: Option<&Reading>,
    in_scope: &dyn Fn(&str) -> bool,
    resolution: &mut ResolutionSummary,
) -> Ranked {
    let functions: BTreeMap<&str, &Function> =
        facts.functions.iter().map(|f| (f.id.as_str(), f)).collect();
    let function_in_scope = |id: &str| {
        functions
            .get(id)
            .map(|f| f.file.as_str())
            .is_some_and(in_scope)
    };
    let callers = resolved_callers(facts, &function_in_scope, resolution);
    unknown_calls(
        facts,
        named,
        reading.is_some(),
        &function_in_scope,
        resolution,
    );
    if let Some(reading) = reading {
        resolution.semantic.get_or_insert_default().reading = reading.describe();
    }
    let by_index = index_callers(named, &callers, &function_in_scope);
    Ranked {
        total: callers.len(),
        rows: called_rows(&callers, &functions, reading.is_some().then_some(&by_index)),
    }
}

/// The calling functions of each in-scope function, by resolved calls; counts
/// the resolved call sites made from functions in scope.
fn resolved_callers<'f>(
    facts: &'f Facts,
    function_in_scope: &dyn Fn(&str) -> bool,
    resolution: &mut ResolutionSummary,
) -> BTreeMap<&'f str, BTreeSet<&'f str>> {
    let mut callers: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for call in &facts.calls {
        if function_in_scope(&call.to) {
            callers.entry(&call.to).or_default().insert(&call.from);
        }
        if function_in_scope(&call.from) {
            resolution.calls_resolved += 1;
        }
    }
    callers
}

/// Count the unknown calls made from functions in scope, grouped by reason
/// kind, and `with_reading` by what the reading pins each to.
fn unknown_calls(
    facts: &Facts,
    named: &[Named<'_>],
    with_reading: bool,
    function_in_scope: &dyn Fn(&str) -> bool,
    resolution: &mut ResolutionSummary,
) {
    use super::callers::Pin;
    let pins = super::callers::Pins::new(named);
    let mut reasons = BTreeMap::new();
    for unknown in facts
        .unknown_calls
        .iter()
        .filter(|u| function_in_scope(&u.from))
    {
        resolution.calls_unknown += 1;
        if let Some(semantic) = with_reading.then(|| resolution.semantic.get_or_insert_default()) {
            match pins.get(unknown) {
                Some(Pin::Function(_)) => semantic.workspace += 1,
                Some(Pin::Elsewhere("external")) => semantic.external += 1,
                Some(Pin::Elsewhere(_)) => semantic.other_items += 1,
                Some(Pin::Open(open)) => *semantic.open.entry(open).or_default() += 1,
                None => *semantic.open.entry("unread").or_default() += 1,
            }
        }
        let kind = unknown
            .reasons
            .iter()
            .map(|reason| code(&reason.kind))
            .collect::<Vec<_>>()
            .join("; ");
        tally(&mut reasons, kind, &unknown.reason);
    }
    resolution.unknown_call_reasons = counts(reasons);
}

/// The calling functions the index adds to each called function: its
/// workspace targets the qualified reading lacks.
fn index_callers<'n>(
    named: &[Named<'n>],
    callers: &BTreeMap<&str, BTreeSet<&str>>,
    function_in_scope: &dyn Fn(&str) -> bool,
) -> BTreeMap<&'n str, BTreeSet<&'n str>> {
    let mut by_index: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for named in named {
        if let (Some(from), Some(to)) = (named.from, named.to)
            && named.call.agreement == Some(Agreement::Extends)
            && function_in_scope(to)
            && !callers.get(to).is_some_and(|known| known.contains(from))
        {
            by_index.entry(to).or_default().insert(from);
        }
    }
    by_index
}

/// One row per called function, counting its resolved callers; with an
/// index (`by_index`), also the callers the index adds.
fn called_rows(
    callers: &BTreeMap<&str, BTreeSet<&str>>,
    functions: &BTreeMap<&str, &Function>,
    by_index: Option<&BTreeMap<&str, BTreeSet<&str>>>,
) -> Vec<Row> {
    callers
        .iter()
        .filter_map(|(to, from)| {
            let f = functions.get(to)?;
            let name = if f.qualified.is_empty() {
                &f.name
            } else {
                &f.qualified
            };
            let mut row = Row::site(from.len(), name.clone(), f.file.clone(), f.line);
            if let Some(by_index) = by_index {
                row.by_index = Some(by_index.get(to).map_or(0, BTreeSet::len));
            }
            Some(row)
        })
        .collect()
}

/// The rows of one measured list and the population they are drawn from.
struct Ranked {
    total: usize,
    rows: Vec<Row>,
}

/// The in-scope types named by unique lexical mentions, each counted by the
/// declarations naming it; counts the mentions from declarations in scope
/// by resolution.
fn type_mentions(
    graph: &SourceGraph,
    in_scope: &dyn Fn(&str) -> bool,
    resolution: &mut ResolutionSummary,
) -> Ranked {
    let declarations: BTreeMap<&str, (&str, &Declaration)> = graph
        .files
        .iter()
        .flat_map(|f| {
            f.declarations
                .iter()
                .map(move |d| (d.id.as_str(), (f.path.as_str(), d)))
        })
        .collect();
    let mut mentioned: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for relationship in &graph.relationships {
        let from_scope = declarations
            .get(relationship.source.as_str())
            .is_some_and(|(path, _)| in_scope(path));
        match &relationship.target {
            Resolution::Unique(target) => {
                if from_scope {
                    resolution.mentions_unique += 1;
                }
                if declarations
                    .get(target.as_str())
                    .is_some_and(|(path, _)| in_scope(path))
                {
                    mentioned
                        .entry(target)
                        .or_default()
                        .insert(&relationship.source);
                }
            }
            Resolution::Ambiguous(_) if from_scope => resolution.mentions_ambiguous += 1,
            Resolution::Unresolved(_) if from_scope => resolution.mentions_unresolved += 1,
            _ => {}
        }
    }
    let rows: Vec<Row> = mentioned
        .iter()
        .filter_map(|(target, sources)| {
            let (path, declaration) = declarations.get(target)?;
            Some(Row::site(
                sources.len(),
                declaration.name.clone(),
                (*path).to_owned(),
                declaration.line,
            ))
        })
        .collect();
    Ranked {
        total: mentioned.len(),
        rows,
    }
}

/// `command`, with `--in` the physical scope when there is one.
fn scoped(command: &[&str], physical_scope: Option<&str>) -> Vec<String> {
    let mut args: Vec<String> = command.iter().map(|&a| a.to_owned()).collect();
    if let Some(scope) = physical_scope {
        args.extend(["--in".to_owned(), scope.to_owned()]);
    }
    args
}
/// The `mass` and `blocks` sections: where the lines are.
fn size_sections(mass: CodeMass, physical_scope: Option<&str>, rows: usize) -> [Section; 2] {
    [
        section(
            Measure {
                id: "mass",
                title: "Where the code is",
                unit: "non-test lines in production files, by directory",
                order: "most lines first",
                total_unit: "lines",
                all: scoped(&["nontest-loc"], physical_scope),
            },
            mass.lines,
            mass.directories,
            rows,
        ),
        section(
            Measure {
                id: "blocks",
                title: "Largest blocks",
                unit: "lines from a top-level declaration to the next, one per file; @generated files left out",
                order: "most lines first",
                total_unit: "files",
                all: scoped(&["concentration"], physical_scope),
            },
            mass.blocks.len(),
            mass.blocks,
            rows,
        ),
    ]
}
/// The `called` and `mentioned` sections: what the code refers to most.
fn reference_sections(
    called: Ranked,
    mentioned: Ranked,
    physical_scope: Option<&str>,
    rows: usize,
) -> [Section; 2] {
    [
        section(
            Measure {
                id: "called",
                title: "Most-called functions",
                unit: "distinct calling functions, counting resolved calls only, so a lower bound",
                order: "most callers first",
                total_unit: "functions with a resolved caller",
                all: vec![
                    "dependencies".into(),
                    "--in".into(),
                    physical_scope.unwrap_or(".").to_owned(),
                    "--relation".into(),
                    "calls".into(),
                    "--json".into(),
                ],
            },
            called.total,
            called.rows,
            rows,
        ),
        section(
            Measure {
                id: "mentioned",
                title: "Most-mentioned types",
                unit: "distinct declarations naming it in a signature or type body; unique lexical targets only",
                order: "most declarations first",
                total_unit: "mentioned types",
                all: vec!["dump".into(), "--json".into()],
            },
            mentioned.total,
            mentioned.rows,
            rows,
        ),
    ]
}

/// The directory of the workspace package named `name`, relative to the
/// workspace root (`.` for a package at the root).
fn package_directory(snapshot: &Snapshot, name: &str) -> Option<String> {
    let metadata = snapshot.metadata();
    let package = metadata
        .workspace_packages()
        .into_iter()
        .find(|p| p.name.as_str() == name)?;
    let directory = package.manifest_path.parent()?.as_std_path();
    if !directory.starts_with(metadata.workspace_root.as_std_path()) {
        return None;
    }
    let relative = target_key(metadata.workspace_root.as_std_path(), directory);
    Some(if relative.is_empty() {
        ".".to_owned()
    } else {
        relative
    })
}
fn packages(snapshot: &Snapshot, names: &BTreeSet<&str>) -> Vec<Package> {
    let metadata = snapshot.metadata();
    let root = metadata.workspace_root.as_std_path();
    let mut packages: Vec<Package> = metadata
        .workspace_packages()
        .into_iter()
        .filter(|p| names.contains(p.name.as_str()))
        .map(|package| Package {
            name: package.name.to_string(),
            targets: package
                .targets
                .iter()
                .map(|target| Target {
                    kinds: target.kind.iter().map(ToString::to_string).collect(),
                    name: target.name.clone(),
                    root: target_key(root, target.src_path.as_std_path()),
                })
                .collect(),
        })
        .collect();
    packages.sort_by(|a, b| a.name.cmp(&b.name));
    packages
}

fn has_generated_marker(text: &str) -> bool {
    text.lines()
        .take(GENERATED_MARKER_LINES)
        .any(|line| line.trim_start().starts_with("//") && line.contains("@generated"))
}

struct Measure {
    id: &'static str,
    title: &'static str,
    unit: &'static str,
    order: &'static str,
    total_unit: &'static str,
    all: Vec<String>,
}
fn section(measure: Measure, total: usize, mut rows: Vec<Row>, shown: usize) -> Section {
    rows.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.path.cmp(&b.path))
            .then_with(|| a.line.cmp(&b.line))
    });
    let tied = rows.first().map_or(0, |first| {
        rows.iter().take_while(|r| r.count == first.count).count()
    });
    let more = rows.len().saturating_sub(shown);
    rows.truncate(shown);
    Section {
        id: measure.id,
        title: measure.title,
        unit: measure.unit,
        order: measure.order,
        total,
        total_unit: measure.total_unit,
        rows,
        tied,
        more,
        all: measure.all,
    }
}
fn tally(map: &mut BTreeMap<String, (usize, String)>, kind: String, example: &str) {
    let entry = map.entry(kind).or_insert_with(|| (0, example.to_owned()));
    entry.0 += 1;
}
fn counts(map: BTreeMap<String, (usize, String)>) -> Vec<Count> {
    let mut counts: Vec<Count> = map
        .into_iter()
        .map(|(kind, (count, example))| Count {
            kind,
            count,
            example,
        })
        .collect();
    counts.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.kind.cmp(&b.kind)));
    counts
}

/// The first row of a section, unless it ties: a command built from one of
/// several equal rows would read as a pick.
fn leader(section: &Section) -> Option<&Row> {
    section.rows.first().filter(|_| section.tied <= 1)
}
fn next_commands(
    sections: &[Section],
    packages: &[Package],
    functions: &[Function],
) -> Vec<NextCommand> {
    let by_id = |id: &str| sections.iter().find(|s| s.id == id);
    let mut next = Vec::new();
    let directory = by_id("mass").and_then(leader).map(|row| {
        let dir = row.path.trim_end_matches('/');
        if dir.is_empty() {
            ".".to_owned()
        } else {
            dir.to_owned()
        }
    });
    if let Some(dir) = &directory {
        next.push(command(
            &["module", dir],
            "declarations in the directory with the most non-test lines",
            true,
        ));
    }
    next.extend(main_command(packages, functions));
    next.extend(by_id("mentioned").and_then(mentioned_command));
    if let Some(dir) = &directory {
        next.push(command(
            &["dependencies", "--in", dir],
            "which files in that directory call each other",
            true,
        ));
    }
    if let Some(row) = by_id("blocks").and_then(leader) {
        next.push(command(
            &["spans", "--in", &row.path],
            "item boundaries in the file holding the largest block",
            true,
        ));
    }
    next
}
fn command(args: &[&str], why: &str, runnable: bool) -> NextCommand {
    NextCommand {
        args: args.iter().map(|&a| a.to_owned()).collect(),
        why: why.to_owned(),
        runnable,
    }
}
/// The flow of the binaries' `main`: runnable for one binary, a placeholder
/// for several, none without a binary.
fn main_command(packages: &[Package], functions: &[Function]) -> Option<NextCommand> {
    let bins: BTreeSet<&str> = packages
        .iter()
        .flat_map(|p| &p.targets)
        .filter(|t| t.kinds.iter().any(|k| k == "bin"))
        .map(|t| t.root.as_str())
        .collect();
    let mains: Vec<&str> = functions
        .iter()
        .filter(|f| {
            f.name == "main"
                && matches!(f.qualified.as_str(), "" | "main")
                && bins.contains(f.file.as_str())
        })
        .map(|f| f.file.as_str())
        .collect();
    match mains.as_slice() {
        [] => None,
        [main] => Some(command(
            &["flow", "--path", main, "--function", "main", "--depth", "1"],
            "steps and branches of the binary's main",
            true,
        )),
        several => Some(command(
            &[
                "flow",
                "--path",
                "<bin root>",
                "--function",
                "main",
                "--depth",
                "1",
            ],
            &format!(
                "steps and branches of a binary's main; {} binaries (`packages` in --json lists them), so none is picked",
                several.len()
            ),
            false,
        )),
    }
}
/// Where the most-mentioned type is declared: runnable for a leader, a
/// placeholder when the first rows tie, none for an empty section.
fn mentioned_command(section: &Section) -> Option<NextCommand> {
    match leader(section) {
        Some(row) => Some(command(
            &[
                "name",
                row.name.as_deref().unwrap_or_default(),
                "--relations",
            ],
            "where the most-mentioned type is declared and what names it",
            true,
        )),
        None if section.tied > 1 => Some(command(
            &[
                "name",
                &format!("<one of the {} tied types above>", section.tied),
                "--relations",
            ],
            "where a most-mentioned type is declared; the first rows tie, so none is picked",
            false,
        )),
        None => None,
    }
}

impl Orientation {
    /// One page of text. Each printed command is rendered by
    /// [`crate::suggestion`], so it carries the run's project.
    pub fn text(&self) -> String {
        let render = |args: &[String]| Command::from_words(args).to_string();
        let mut out = String::new();
        let _ = writeln!(
            out,
            "{} orient: {}, {} package{}, {} source files ({} test-only, {} @generated, {} outside any target's module tree)",
            crate::suggestion::PROGRAM,
            self.scope,
            self.packages.len(),
            if self.packages.len() == 1 { "" } else { "s" },
            self.files,
            self.test_only_files,
            self.generated_files.len(),
            self.unmounted_files.len()
        );
        self.write_targets(&mut out);
        for section in &self.sections {
            write_section(&mut out, section, &render);
        }
        self.write_resolution(&mut out, &render);
        if !self.next.is_empty() {
            let _ = writeln!(out, "\nNext");
            for next in &self.next {
                let _ = writeln!(out, "  {:<60} # {}", render(&next.args), next.why);
            }
        }
        let _ = writeln!(
            out,
            "\nAsk by question: what is X, who names X: name · what is in a directory: module · steps in a function: flow\n  who calls whom across files: dependencies · which functions take, return or build a type: types\n  what changed since REV: diff, moved, visibility · rules: policy, delta\n{}",
            self.basis
        );
        if let Some(semantic) = &self.resolution.semantic {
            for line in &semantic.reading {
                let _ = writeln!(out, "{line}");
            }
        }
        out
    }
    fn write_targets(&self, out: &mut String) {
        const SHOWN: usize = 6;
        let _ = writeln!(
            out,
            "\nEntry points (Cargo library and binary roots; other targets counted)"
        );
        for package in self.packages.iter().take(SHOWN) {
            let roots: Vec<String> = package
                .targets
                .iter()
                .filter(|t| t.is_entry())
                .map(|t| format!("{} {}", t.kinds.join("+"), t.root))
                .collect();
            let mut others: BTreeMap<String, usize> = BTreeMap::new();
            for target in package.targets.iter().filter(|t| !t.is_entry()) {
                *others.entry(target.kinds.join("+")).or_default() += 1;
            }
            let others: Vec<String> = others
                .iter()
                .map(|(kind, n)| format!("{n} {kind}"))
                .collect();
            let _ = writeln!(
                out,
                "  {}: {}{}",
                package.name,
                if roots.is_empty() {
                    "no library or binary".to_owned()
                } else {
                    roots.join(", ")
                },
                if others.is_empty() {
                    String::new()
                } else {
                    format!(" · {}", others.join(", "))
                }
            );
        }
        if self.packages.len() > SHOWN {
            let _ = writeln!(
                out,
                "  … {} more packages; cargo metadata --no-deps --format-version 1",
                self.packages.len() - SHOWN
            );
        }
    }
    fn write_resolution(&self, out: &mut String, render: &dyn Fn(&[String]) -> String) {
        const REASONS: usize = 3;
        let r = &self.resolution;
        let calls = r.calls_resolved + r.calls_unknown;
        let _ = writeln!(out, "\nWhat Atlas could not resolve");
        match &r.semantic {
            None => {
                let _ = writeln!(
                    out,
                    "  calls: {calls} call sites in scope: {} resolved to a workspace function; {} unresolved, external or unknown, not absent (by kind below)\n    most unresolved calls are into other packages, which Atlas does not resolve; an index tells them apart: {} (runs rust-analyzer and the build scripts)",
                    r.calls_resolved,
                    r.calls_unknown,
                    render(&["orient".to_owned(), "--semantic".to_owned()])
                );
            }
            Some(semantic) => {
                let open: usize = semantic.open.values().sum();
                let workspace = r.calls_resolved + semantic.workspace;
                let _ = writeln!(
                    out,
                    "  calls: {calls} call sites in scope: {workspace} to workspace functions, {} into other packages (the index), {} to workspace items that are no captured function, {open} that neither reading pins",
                    semantic.external, semantic.other_items
                );
                let _ = writeln!(
                    out,
                    "    of the {workspace} workspace calls Atlas resolved {} ({}%) and the index the other {}",
                    r.calls_resolved,
                    (r.calls_resolved * 100).checked_div(workspace).unwrap_or(0),
                    semantic.workspace
                );
                if open > 0 {
                    let mut ordered: Vec<_> = semantic.open.iter().collect();
                    ordered.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
                    let ordered: Vec<String> = ordered
                        .iter()
                        .map(|(reading, n)| format!("{reading} {n}"))
                        .collect();
                    let _ = writeln!(out, "    left open: {}", ordered.join(", "));
                }
                let _ = writeln!(
                    out,
                    "    Atlas's own unresolved calls ({}), by kind:",
                    r.calls_unknown
                );
            }
        }
        for reason in r.unknown_call_reasons.iter().take(REASONS) {
            let _ = writeln!(
                out,
                "    {:>7}  {}  (e.g. {})",
                reason.count, reason.kind, reason.example
            );
        }
        let _ = writeln!(
            out,
            "  type mentions: {} unique, {} ambiguous, {} unresolved",
            r.mentions_unique, r.mentions_ambiguous, r.mentions_unresolved
        );
        let _ = writeln!(out, "  diagnostics: {} (grouped by code)", r.diagnostics);
        for kind in r.diagnostic_kinds.iter().take(REASONS) {
            let _ = writeln!(
                out,
                "    {:>7}  {}  (e.g. {})",
                kind.count, kind.kind, kind.example
            );
        }
    }
}

/// One section: its measure line, any tie, its rows and the command for the rest.
fn write_section(out: &mut String, section: &Section, render: &dyn Fn(&[String]) -> String) {
    let _ = writeln!(
        out,
        "\n{} ({}; {}; {} {} in all)",
        section.title, section.unit, section.order, section.total, section.total_unit
    );
    if section.tied > 1 {
        let _ = writeln!(
            out,
            "  (the first {} rows tie; tied rows are ordered by name)",
            section.tied
        );
    }
    for row in &section.rows {
        let _ = writeln!(out, "  {:>7}  {}", row.count, row_detail(row));
    }
    if section.more > 0 {
        let _ = writeln!(out, "  … {} more; {}", section.more, render(&section.all));
    }
}
/// A row after its count: a declaration's name and site, or a directory's
/// file count and `@generated` share.
fn row_detail(row: &Row) -> String {
    let site = row
        .line
        .map_or_else(|| row.path.clone(), |line| format!("{}:{line}", row.path));
    match (&row.name, row.files) {
        (Some(name), _) => match row.by_index {
            Some(added) if added > 0 => {
                format!("{name}  {site}  (+{added} by the index)")
            }
            _ => format!("{name}  {site}"),
        },
        (None, Some(files)) => {
            let generated = row
                .generated_lines
                .map_or_else(String::new, |g| format!("; {g} lines in @generated files"));
            format!(
                "{site}  {files} file{}{generated}",
                if files == 1 { "" } else { "s" }
            )
        }
        (None, None) => site,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_marker_is_a_comment_near_the_top() {
        assert!(has_generated_marker(
            "// This file is @generated by a tool.\nfn f() {}\n"
        ));
        assert!(!has_generated_marker(
            "fn f() {}\nlet s = \"@generated\";\n"
        ));
        assert!(!has_generated_marker("\n\n\n\n\n// @generated\n"));
    }
}
