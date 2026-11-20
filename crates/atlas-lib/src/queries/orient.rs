//! Arrival report: one bounded page of measured facts for a reader who has not
//! seen the project. Each list states its unit, its order and its total, and
//! names the command that shows the rest. Nothing here ranks importance: an
//! order is only "by that measure", ties are reported, and a follow-up command
//! is never built from an arbitrary pick among equal rows or several binaries.
//!
//! Limits: called functions count resolved calls only, so they are a lower
//! bound; type mentions count unique lexical targets only. Diagnostics and
//! unknown-call reasons are grouped by their stable codes, each with the
//! first message as an example.
use crate::{
    Result,
    analysis::Analysis,
    discovery::Snapshot,
    model::{Declaration, Resolution},
    relations::code,
    representations::Function,
    source::discovery::target_key,
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
    let graph = snapshot.graph();
    let observations = super::files(graph, scope)?;
    let paths: BTreeSet<&str> = observations.iter().map(|f| f.path.as_str()).collect();
    let package_names: BTreeSet<&str> =
        observations.iter().map(|f| f.crate_name.as_str()).collect();
    let in_scope = |path: &str| paths.contains(path);

    let packages = packages(snapshot, &package_names);
    let physical: Vec<_> = graph
        .physical_files()
        .filter(|f| in_scope(&f.path))
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

    let outline = crate::review::outline(snapshot, ".")?;
    let mut dirs: BTreeMap<String, (usize, usize, usize)> = BTreeMap::new();
    let mut blocks = Vec::new();
    for file in outline
        .files
        .iter()
        .filter(|f| in_scope(&f.file) && !test_only.contains(f.file.as_str()))
    {
        let dir = file.file.rsplit_once('/').map_or("", |(dir, _)| dir);
        let entry = dirs.entry(format!("{dir}/")).or_default();
        entry.0 += file.non_test_lines;
        entry.1 += 1;
        if generated.contains(&file.file) {
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
    let mass: Vec<Row> = dirs
        .into_iter()
        .map(|(path, (count, files, generated_lines))| Row {
            count,
            name: None,
            path,
            line: None,
            files: Some(files),
            generated_lines: (generated_lines > 0).then_some(generated_lines),
        })
        .collect();

    let facts = Analysis::new(snapshot)?.representations()?;
    let functions: BTreeMap<&str, &Function> =
        facts.functions.iter().map(|f| (f.id.as_str(), f)).collect();
    let file_of = |id: &str| functions.get(id).map(|f| f.file.as_str());
    let mut callers: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    let mut resolution = ResolutionSummary::default();
    for call in &facts.calls {
        if file_of(&call.to).is_some_and(in_scope) {
            callers.entry(&call.to).or_default().insert(&call.from);
        }
        if file_of(&call.from).is_some_and(in_scope) {
            resolution.calls_resolved += 1;
        }
    }
    let mut reasons = BTreeMap::new();
    for unknown in facts
        .unknown_calls
        .iter()
        .filter(|u| file_of(&u.from).is_some_and(in_scope))
    {
        resolution.calls_unknown += 1;
        let kind = unknown
            .reasons
            .iter()
            .map(|reason| code(&reason.kind))
            .collect::<Vec<_>>()
            .join("; ");
        tally(&mut reasons, kind, &unknown.reason);
    }
    resolution.unknown_call_reasons = counts(reasons);
    let called: Vec<Row> = callers
        .iter()
        .filter_map(|(to, from)| {
            let f = functions.get(to)?;
            let name = if f.qualified.is_empty() {
                &f.name
            } else {
                &f.qualified
            };
            Some(Row::site(from.len(), name.clone(), f.file.clone(), f.line))
        })
        .collect();

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
    let types: Vec<Row> = mentioned
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
    let mut kinds = BTreeMap::new();
    for diagnostic in super::diagnostics_of(graph, &observations) {
        resolution.diagnostics += 1;
        tally(&mut kinds, code(&diagnostic.code), &diagnostic.message);
    }
    resolution.diagnostic_kinds = counts(kinds);

    // Follow-up commands take a physical path: a package scope becomes its directory.
    let physical_scope =
        scope.map(|scope| package_directory(snapshot, scope).unwrap_or_else(|| scope.to_owned()));
    let scoped = |command: &[&str]| -> Vec<String> {
        let mut args: Vec<String> = command.iter().map(|&a| a.to_owned()).collect();
        if let Some(scope) = &physical_scope {
            args.extend(["--scope".to_owned(), scope.clone()]);
        }
        args
    };
    let sections = vec![
        section(
            Measure {
                id: "mass",
                title: "Where the code is",
                unit: "non-test lines in production files, by directory",
                order: "most lines first",
                total_unit: "lines",
                all: scoped(&["nontest-loc"]),
            },
            lines,
            mass,
            rows,
        ),
        section(
            Measure {
                id: "blocks",
                title: "Largest blocks",
                unit: "lines from a top-level declaration to the next, one per file; @generated files left out",
                order: "most lines first",
                total_unit: "files",
                all: scoped(&["concentration"]),
            },
            blocks.len(),
            blocks,
            rows,
        ),
        section(
            Measure {
                id: "called",
                title: "Most-called functions",
                unit: "distinct calling functions, counting resolved calls only, so a lower bound",
                order: "most callers first",
                total_unit: "functions with a resolved caller",
                all: vec![
                    "dependencies".into(),
                    "--scope".into(),
                    physical_scope.clone().unwrap_or_else(|| ".".to_owned()),
                    "--relation".into(),
                    "calls".into(),
                    "--json".into(),
                ],
            },
            callers.len(),
            called,
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
            mentioned.len(),
            types,
            rows,
        ),
    ];
    let next = next_commands(&sections, &packages, &facts.functions);
    Ok(Orientation {
        schema: 1,
        scope: scope.unwrap_or("workspace").to_owned(),
        packages,
        files: physical.len(),
        test_only_files: test_only.len(),
        generated_files: generated.into_iter().collect(),
        unmounted_files: unmounted,
        sections,
        resolution,
        next,
        basis: BASIS,
    })
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
    let command = |args: &[&str], why: &str, runnable: bool| NextCommand {
        args: args.iter().map(|&a| a.to_owned()).collect(),
        why: why.to_owned(),
        runnable,
    };
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
        [] => {}
        [main] => next.push(command(
            &["flow", "--path", main, "--function", "main", "--depth", "1"],
            "steps and branches of the binary's main",
            true,
        )),
        several => next.push(command(
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
    if let Some(section) = by_id("mentioned") {
        match leader(section) {
            Some(row) => next.push(command(
                &[
                    "name",
                    row.name.as_deref().unwrap_or_default(),
                    "--relations",
                ],
                "where the most-mentioned type is declared and what names it",
                true,
            )),
            None if section.tied > 1 => next.push(command(
                &[
                    "name",
                    &format!("<one of the {} tied types above>", section.tied),
                    "--relations",
                ],
                "where a most-mentioned type is declared; the first rows tie, so none is picked",
                false,
            )),
            None => {}
        }
    }
    if let Some(dir) = &directory {
        next.push(command(
            &["dependencies", "--scope", dir],
            "which files in that directory call each other",
            true,
        ));
    }
    if let Some(row) = by_id("blocks").and_then(leader) {
        next.push(command(
            &["spans", "--scope", &row.path],
            "item boundaries in the file holding the largest block",
            true,
        ));
    }
    next
}

/// Render arguments after `cargo atlas`, quoting any that a shell would split.
pub fn command_line(args: &[String]) -> String {
    let mut line = String::from("cargo atlas");
    for arg in args {
        line.push(' ');
        if !arg.is_empty()
            && arg
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "-_./:=@,+".contains(c))
            || arg.starts_with('<')
        {
            line.push_str(arg);
        } else {
            let _ = write!(line, "'{}'", arg.replace('\'', r"'\''"));
        }
    }
    line
}

impl Orientation {
    /// One page of text. `suffix` is appended to every printed command, such
    /// as the `--root` the caller used, so each command runs as printed.
    pub fn text(&self, suffix: &[String]) -> String {
        let render = |args: &[String]| {
            let mut all = args.to_vec();
            all.extend(suffix.iter().cloned());
            command_line(&all)
        };
        let mut out = String::new();
        let _ = writeln!(
            out,
            "cargo atlas orient: {}, {} package{}, {} source files ({} test-only, {} @generated, {} outside any target's module tree)",
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
                let site = row
                    .line
                    .map_or_else(|| row.path.clone(), |line| format!("{}:{line}", row.path));
                let detail = match (&row.name, row.files) {
                    (Some(name), _) => format!("{name}  {site}"),
                    (None, Some(files)) => {
                        let generated = row.generated_lines.map_or_else(String::new, |g| {
                            format!("; {g} lines in @generated files")
                        });
                        format!(
                            "{site}  {files} file{}{generated}",
                            if files == 1 { "" } else { "s" }
                        )
                    }
                    (None, None) => site,
                };
                let _ = writeln!(out, "  {:>7}  {detail}", row.count);
            }
            if section.more > 0 {
                let _ = writeln!(out, "  ... {} more; {}", section.more, render(&section.all));
            }
        }
        self.write_resolution(&mut out);
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
                "  ... {} more packages; cargo metadata --no-deps --format-version 1",
                self.packages.len() - SHOWN
            );
        }
    }
    fn write_resolution(&self, out: &mut String) {
        const REASONS: usize = 3;
        let r = &self.resolution;
        let calls = r.calls_resolved + r.calls_unknown;
        let _ = writeln!(out, "\nWhat Atlas could not resolve");
        let _ = writeln!(
            out,
            "  calls: {} of {calls} call sites resolved to a workspace function ({}%); the rest are external or unknown, not absent",
            r.calls_resolved,
            (r.calls_resolved * 100).checked_div(calls).unwrap_or(0)
        );
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

    #[test]
    fn commands_quote_what_a_shell_would_split() {
        let args = |a: &[&str]| a.iter().map(|&s| s.to_owned()).collect::<Vec<_>>();
        assert_eq!(
            command_line(&args(&["module", "src/a b"])),
            "cargo atlas module 'src/a b'"
        );
        assert_eq!(
            command_line(&args(&["flow", "--path", "<bin root>"])),
            "cargo atlas flow --path <bin root>"
        );
    }
}
