//! Source evidence for moves: native lexical keys, written visibility and normalized tokens.
//! Matching spans whole crates before report selection. No report proves equivalent behavior.
use crate::{
    Result,
    discovery::Snapshot,
    model::SourceFile,
    source::{self as scan, Item, ItemKind, Source, Visibility},
};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
mod tokens;
pub(crate) use tokens::impl_header;
use tokens::tokens;

/// Report selection does not narrow matching: a path admits its whole crate.
enum Scope {
    Workspace,
    Crate(String),
    Path(String),
}
impl Scope {
    fn of(before: &Snapshot, after: &Snapshot, argument: Option<&str>) -> Result<Self> {
        let Some(argument) = argument else {
            return Ok(Self::Workspace);
        };
        if argument.is_empty() {
            return Err("an empty scope names nothing".into());
        }
        if argument == "." {
            return Ok(Self::Workspace);
        }
        let path = argument.trim_start_matches("./").trim_end_matches('/');
        if before
            .graph()
            .files
            .iter()
            .chain(&after.graph().files)
            .any(|file| file.crate_name == path)
        {
            return Ok(Self::Crate(path.to_owned()));
        }
        let path = super::comparison_scope(before, after, path)?;
        if path.is_empty() {
            return Ok(Self::Workspace);
        }
        Ok(Self::Path(path))
    }
    fn admits_file(&self, path: &str) -> bool {
        match self {
            Self::Path(scope) => crate::architecture::contains(scope, path),
            _ => true,
        }
    }
    fn crates(&self, before: &Snapshot, after: &Snapshot) -> BTreeSet<String> {
        before
            .graph()
            .files
            .iter()
            .chain(&after.graph().files)
            .filter(|file| match self {
                Self::Workspace => true,
                Self::Crate(name) => file.crate_name == *name,
                Self::Path(_) => self.admits_file(&file.path),
            })
            .map(|file| file.crate_name.clone())
            .collect()
    }
    fn admits(&self, key: &Key, site: &Site) -> bool {
        match self {
            Self::Workspace => true,
            Self::Crate(name) => key.0 == *name,
            Self::Path(_) => self.admits_file(&site.file),
        }
    }
    fn selection(&self) -> Option<&str> {
        match self {
            Self::Workspace => None,
            Self::Crate(name) | Self::Path(name) => Some(name),
        }
    }
}
/// A declaration's tokens in the two readings `moved` compares.
#[derive(Clone, PartialEq, Eq)]
enum Body {
    /// Not a body comparison: fields (their struct carries them), module
    /// declarations and `use` bindings (layout a move rewrites), or a run
    /// that asked for visibility only.
    Uncompared,
    /// The span does not tokenize; reported, never counted as preserved.
    Unreadable,
    /// `exact` forgives docs, visibility and layout; `rerooted` also drops the
    /// `crate::`/`super::`/`self::` qualifiers (and the module segments after
    /// them) that a move rewrites.
    Tokens {
        exact: Vec<String>,
        rerooted: Vec<String>,
    },
}

/// One keyed declaration on one side of the comparison.
#[derive(Clone)]
struct Site {
    file: String,
    line: usize,
    visibility: Visibility,
    /// The path of a `pub(in path)`: two such rungs share a rank but not a
    /// scope.
    restricted: Option<String>,
    body: Body,
}

impl Site {
    fn same_visibility(&self, other: &Self) -> bool {
        self.visibility == other.visibility && self.restricted == other.restricted
    }

    fn visibility_text(&self) -> String {
        self.restricted.as_ref().map_or_else(
            || self.visibility.label().to_owned(),
            |path| format!("pub(in {path})"),
        )
    }
}

/// `(crate, class, name)`: the identity a move preserves.
type Key = (String, &'static str, String);

type Inventory = BTreeMap<Key, Vec<Site>>;

/// Every keyed declaration of the crates `scope` matches, each crate whole.
fn inventory(
    snapshot: &Snapshot,
    matched: &dyn Fn(&str) -> bool,
    bodies: bool,
) -> Result<Inventory> {
    let mut inventory: Inventory = BTreeMap::new();
    for file in snapshot
        .graph()
        .files
        .iter()
        .filter(|file| matched(&file.crate_name))
    {
        let source = snapshot
            .source_facts(&file.path)
            .ok_or_else(|| format!("snapshot lacks source facts for {}", file.path))?;
        for (class, name, site) in file_sites(file, source, bodies)? {
            inventory
                .entry((file.crate_name.clone(), class, name))
                .or_default()
                .push(site);
        }
    }
    Ok(inventory)
}

/// How an enclosing item shapes the keys of the items its span holds.
enum Enclosing {
    /// Members are keyed `Owner::name`.
    Impl(String),
    /// Members are keyed under the module path.
    Module(String),
    /// Members move with their container (trait items, macro and extern
    /// bodies, items nested in a fn) and are compared as part of it.
    Folded,
}

/// The production declarations of one file as `(class, name, site)`.
fn file_sites(
    file: &SourceFile,
    source: &Source,
    bodies: bool,
) -> Result<Vec<(&'static str, String, Site)>> {
    if file.test_only {
        return Ok(Vec::new());
    }
    let outline = source.outline();
    let items: Vec<&Item> = outline
        .iter()
        .filter(|item| !source.test_regions().covers(item.line))
        .collect();
    let code: Vec<&str> = source.projection().code_lines().collect();
    let text: Vec<&str> = source.text().lines().collect();
    let roots = if bodies {
        path_roots(&items, source, &file.path)?
    } else {
        BTreeSet::new()
    };
    let body_of = |item: &Item| {
        if !bodies {
            return Body::Uncompared;
        }
        let span = &text[*item.span.start() - 1..*item.span.end()];
        tokens(&span.join("\n"), &roots)
    };
    let site = |item: &Item, body: Body| Site {
        file: file.path.clone(),
        line: item.line,
        visibility: item.visibility,
        restricted: item
            .visibility_restriction()
            .filter(|_| item.visibility == Visibility::PubIn)
            .map(|path| path.split_whitespace().collect()),
        body,
    };
    let mut found = Vec::new();
    let mut stack: Vec<(&Item, Enclosing)> = Vec::new();
    for item in items {
        while stack
            .last()
            .is_some_and(|(outer, _)| !outer.span.contains(&item.line))
        {
            stack.pop();
        }
        let module: String = stack
            .iter()
            .filter_map(|(_, enclosing)| match enclosing {
                Enclosing::Module(name) => Some(format!("{name}::")),
                _ => None,
            })
            .collect();
        match stack.last().map(|(_, enclosing)| enclosing) {
            Some(Enclosing::Folded) => continue,
            Some(Enclosing::Impl(owner)) => {
                if matches!(
                    item.kind,
                    ItemKind::Fn | ItemKind::Const | ItemKind::TypeAlias
                ) {
                    found.push((
                        "method",
                        format!("{owner}::{}", item.name),
                        site(item, body_of(item)),
                    ));
                }
                stack.push((item, Enclosing::Folded));
                continue;
            }
            _ => {}
        }
        let enclosing = match item.kind {
            ItemKind::Use | ItemKind::PubUse => {
                for binding in bindings(source, &file.path, item.line)? {
                    // A binding's body is its target: a move may reroot the
                    // path to it, never retarget the name.
                    let body = if bodies {
                        let rerooted = if rooted(&binding.source_path, &roots) {
                            binding.source.clone()
                        } else {
                            binding.source_path.clone()
                        };
                        Body::Tokens {
                            exact: vec![binding.source_path.clone()],
                            rerooted: vec![rerooted],
                        }
                    } else {
                        Body::Uncompared
                    };
                    let bound = if binding.local_name == "*" {
                        binding.source_path.clone()
                    } else {
                        binding.local_name.clone()
                    };
                    found.push(("use", format!("{module}{bound}"), site(item, body)));
                }
                Enclosing::Folded
            }
            ItemKind::Impl => {
                let header =
                    tokens::impl_header(&text[*item.span.start() - 1..*item.span.end()].join("\n"));
                let header_text = header
                    .as_ref()
                    .map_or_else(String::new, ToString::to_string);
                let owner = scan::impl_trait(crate::scan::skip_attributes(&header_text))
                    .map_or_else(
                        || format!("{module}{}", item.name),
                        |name| format!("<{module}{} as {name}>", item.name),
                    );
                let header_body = if bodies {
                    header.map_or(Body::Unreadable, |header| tokens::readings(header, &roots))
                } else {
                    Body::Uncompared
                };
                found.push(("impl", owner.clone(), site(item, header_body)));
                Enclosing::Impl(owner)
            }
            ItemKind::Mod => {
                // A module declaration is layout, which a move rewrites by
                // definition; its items are keyed under its path.
                found.push((
                    "item",
                    format!("{module}{}", item.name),
                    site(item, Body::Uncompared),
                ));
                Enclosing::Module(item.name.clone())
            }
            ItemKind::MacroCall => {
                let inner = outline
                    .get(outline.partition_point(|inner| inner.line <= item.line))
                    .filter(|inner| item.span.contains(&inner.line))
                    .map_or_else(String::new, |inner| inner.name.clone());
                found.push((
                    "macro",
                    format!("{module}{}!({inner})", item.name),
                    site(item, body_of(item)),
                ));
                Enclosing::Folded
            }
            kind => {
                found.push((
                    "item",
                    format!("{module}{}", item.name),
                    site(item, body_of(item)),
                ));
                if kind == ItemKind::Struct {
                    let declaration = code[item.line - 1..*item.span.end()].join("\n");
                    for (offset, field, visibility, restricted) in scan::struct_fields(&declaration)
                    {
                        found.push((
                            "field",
                            format!("{module}{}.{field}", item.name),
                            Site {
                                restricted,
                                line: item.line + offset,
                                visibility,
                                ..site(item, Body::Uncompared)
                            },
                        ));
                    }
                }
                Enclosing::Folded
            }
        };
        stack.push((item, enclosing));
    }
    Ok(found)
}

fn bindings<'a>(source: &'a Source, file: &str, line: usize) -> Result<&'a [scan::ImportBinding]> {
    source
        .import_at(line)
        .ok_or_else(|| format!("captured import absent at {file}:{line}"))?
        .bindings
        .as_deref()
        .map_err(|why| format!("import unavailable at {file}:{line}: {why}").into())
}

/// The path roots a move may rewrite in one file: `crate`, `super`, `self`,
/// the modules the file declares (`mod tick;` makes `tick::Engine` a path
/// from here), and every lowercase name its own imports bind to a
/// crate-relative path (`use crate::load::dispatch;` makes `dispatch::g` one).
fn path_roots(items: &[&Item], source: &Source, file: &str) -> Result<BTreeSet<String>> {
    let mut roots: BTreeSet<String> = ["crate", "super", "self"].map(str::to_owned).into();
    roots.extend(
        items
            .iter()
            .filter(|item| item.kind == ItemKind::Mod)
            .map(|item| item.name.clone()),
    );
    for item in items
        .iter()
        .filter(|item| matches!(item.kind, ItemKind::Use | ItemKind::PubUse))
    {
        for binding in bindings(source, file, item.line)? {
            let module = binding
                .local_name
                .starts_with(|ch: char| ch.is_ascii_lowercase() || ch == '_');
            if module
                && ["crate::", "super::", "self::"]
                    .iter()
                    .any(|root| binding.source_path.starts_with(root))
            {
                roots.insert(binding.local_name.clone());
            }
        }
    }
    Ok(roots)
}

/// Whether a path's first segment is one of the file's rewritable roots.
fn rooted(path: &str, roots: &BTreeSet<String>) -> bool {
    path.split_once("::")
        .is_some_and(|(head, _)| roots.contains(head.trim()))
}

/// One finding: a key whose two sides differ, with both sites.
struct Change<'a> {
    key: &'a Key,
    before: &'a Site,
    after: &'a Site,
    /// Several sites share the key on a side, so the pairing is a guess.
    ambiguous: bool,
}

/// The direction of a visibility change along the ladder. Two `pub(in ..)`
/// scopes compare by path ancestry; unrelated paths are re-scoped.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Direction {
    Widened,
    Narrowed,
    Rescoped,
}

impl Direction {
    fn of(before: &Site, after: &Site) -> Self {
        let (old, new) = (before.visibility.rank(), after.visibility.rank());
        if new != old {
            return if new > old {
                Self::Widened
            } else {
                Self::Narrowed
            };
        }
        let within = |inner: &str, outer: &str| {
            inner
                .strip_prefix(outer)
                .is_some_and(|rest| rest.starts_with("::"))
        };
        match (before.restricted.as_deref(), after.restricted.as_deref()) {
            (Some(old), Some(new)) if within(old, new) => Self::Widened,
            (Some(old), Some(new)) if within(new, old) => Self::Narrowed,
            _ => Self::Rescoped,
        }
    }

    #[cfg(test)]
    const fn label(self) -> &'static str {
        match self {
            Self::Widened => "widened",
            Self::Narrowed => "narrowed",
            Self::Rescoped => "rescoped",
        }
    }
}

/// Every visibility change: per key, equal visibilities cancel first, then
/// the rest pair in ladder order; unpaired sites are additions or removals.
fn visibility_changes<'a>(before: &'a Inventory, after: &'a Inventory) -> Vec<Change<'a>> {
    let mut changes = Vec::new();
    for (key, new_sites) in after {
        let Some(old_sites) = before.get(key) else {
            continue;
        };
        let ambiguous = old_sites.len() > 1 || new_sites.len() > 1;
        let mut old: Vec<&Site> = old_sites.iter().collect();
        let mut new: Vec<&Site> = Vec::new();
        for site in new_sites {
            match old
                .iter()
                .position(|candidate| candidate.same_visibility(site))
            {
                Some(index) => {
                    old.swap_remove(index);
                }
                None => new.push(site),
            }
        }
        old.sort_by_key(|site| site.visibility.rank());
        new.sort_by_key(|site| site.visibility.rank());
        changes.extend(old.into_iter().zip(new).map(|(before, after)| Change {
            key,
            before,
            after,
            ambiguous,
        }));
    }
    changes.sort_by(|left, right| {
        (&left.after.file, left.after.line, left.key).cmp(&(
            &right.after.file,
            right.after.line,
            right.key,
        ))
    });
    changes
}

/// Keys with several sites on either side among the reported ones: their
/// pairing is approximate, so a swap between same-named sites can cancel.
fn ambiguities(before: &Inventory, after: &Inventory, scope: &Scope) -> Vec<Ambiguity> {
    after
        .iter()
        .filter(|(key, sites)| {
            sites.iter().any(|site| scope.admits(key, site))
                && (sites.len() > 1 || before.get(*key).is_some_and(|old| old.len() > 1))
        })
        .map(|(key, sites)| Ambiguity {
            crate_name: key.0.clone(),
            class: key.1,
            name: key.2.clone(),
            before: before
                .get(key)
                .into_iter()
                .flatten()
                .map(Location::from)
                .collect(),
            after: sites.iter().map(Location::from).collect(),
        })
        .collect()
}

fn reading(site: &Site, rerooted: bool) -> Option<&[String]> {
    match &site.body {
        Body::Tokens {
            exact,
            rerooted: paths,
        } => Some(if rerooted { paths } else { exact }),
        _ => None,
    }
}
/// The body comparison of two inventories, reported within a scope.
struct BodyReport<'a> {
    exact: usize,
    /// Of the exact and rerooted pairs, those whose file changed.
    relocated: usize,
    rerooted: Vec<Change<'a>>,
    changed: Vec<Change<'a>>,
    only_before: Vec<(&'a Key, &'a Site)>,
    only_after: Vec<(&'a Key, &'a Site)>,
    unreadable: Vec<(&'a Key, &'a Site)>,
}

impl<'a> BodyReport<'a> {
    /// Per key, pair exact bodies first, then rerooted ones, then the rest in
    /// order as changed. Pairing spans the matched crates; a finding is
    /// reported when its current site (or, for a removal, its old site) lies
    /// in scope.
    fn between(before: &'a Inventory, after: &'a Inventory, scope: &Scope) -> Self {
        let mut report = Self {
            exact: 0,
            relocated: 0,
            rerooted: Vec::new(),
            changed: Vec::new(),
            only_before: Vec::new(),
            only_after: Vec::new(),
            unreadable: Vec::new(),
        };
        let keys: BTreeSet<&Key> = before.keys().chain(after.keys()).collect();
        for key in keys {
            let sides = |inventory: &'a Inventory| -> Vec<&'a Site> {
                inventory
                    .get(key)
                    .map(|sites| {
                        sites
                            .iter()
                            .filter(|site| site.body != Body::Uncompared)
                            .collect()
                    })
                    .unwrap_or_default()
            };
            let (mut old, mut new) = (sides(before), sides(after));
            if old.is_empty() && new.is_empty() {
                continue;
            }
            let ambiguous = old.len() > 1 || new.len() > 1;
            for sites in [&mut old, &mut new] {
                sites.retain(|site| {
                    let readable = site.body != Body::Unreadable;
                    if !readable && scope.admits(key, site) {
                        report.unreadable.push((key, site));
                    }
                    readable
                });
            }
            // A `use` binding reports only a retarget: moves add, drop and
            // reroot imports by design.
            let binding = key.1 == "use";
            let mut unpaired = Vec::new();
            for site in new {
                let pass = |rerooted: bool, old: &Vec<&Site>| {
                    old.iter().position(|candidate| {
                        reading(candidate, rerooted) == reading(site, rerooted)
                    })
                };
                let reported = scope.admits(key, site);
                let counted = reported && !binding;
                if let Some(index) = pass(false, &old) {
                    let candidate = old.swap_remove(index);
                    if counted {
                        report.exact += 1;
                        report.relocated += usize::from(candidate.file != site.file);
                    }
                } else if let Some(index) = pass(true, &old) {
                    let candidate = old.swap_remove(index);
                    if counted {
                        report.relocated += usize::from(candidate.file != site.file);
                        report.rerooted.push(Change {
                            key,
                            before: candidate,
                            after: site,
                            ambiguous,
                        });
                    }
                } else {
                    unpaired.push(site);
                }
            }
            for site in unpaired {
                let reported = scope.admits(key, site);
                if old.is_empty() {
                    if reported && !binding {
                        report.only_after.push((key, site));
                    }
                } else {
                    let candidate = old.remove(0);
                    if reported {
                        report.changed.push(Change {
                            key,
                            before: candidate,
                            after: site,
                            ambiguous,
                        });
                    }
                }
            }
            report.only_before.extend(
                old.into_iter()
                    .filter(|site| !binding && scope.admits(key, site))
                    .map(|site| (key, site)),
            );
        }
        report
    }
}

/// Fingerprints and discovery limits for every file in the matching population.
#[derive(Debug, Serialize)]
pub struct SnapshotBasis {
    pub sources: BTreeMap<String, String>,
    /// Workspace-wide discovery diagnostics, including exclusions without a captured file.
    pub diagnostics: Vec<crate::model::Diagnostic>,
}
#[derive(Debug, Serialize)]
pub struct Basis {
    pub before: SnapshotBasis,
    pub after: SnapshotBasis,
}
impl Basis {
    fn of(before: &Snapshot, after: &Snapshot, matched: &dyn Fn(&str) -> bool) -> Self {
        let side = |snapshot: &Snapshot| {
            let sources: BTreeMap<_, _> = snapshot
                .graph()
                .files
                .iter()
                .filter(|file| matched(&file.crate_name))
                .map(|file| (file.path.clone(), file.content_fingerprint.clone()))
                .collect();
            // Exclusions can have no captured file (for example a symlink).
            // Retain workspace diagnostics rather than silently dropping that evidence.
            let diagnostics = snapshot.graph().diagnostics.clone();
            SnapshotBasis {
                sources,
                diagnostics,
            }
        };
        Self {
            before: side(before),
            after: side(after),
        }
    }
}
#[derive(Debug, Serialize)]
pub struct Location {
    pub file: String,
    pub line: usize,
    pub visibility: String,
}
impl From<&Site> for Location {
    fn from(site: &Site) -> Self {
        Self {
            file: site.file.clone(),
            line: site.line,
            visibility: site.visibility_text(),
        }
    }
}
#[derive(Debug, Serialize)]
pub struct Pair {
    pub crate_name: String,
    pub class: &'static str,
    pub name: String,
    pub before: Location,
    pub after: Location,
    pub ambiguous: bool,
}
impl From<&Change<'_>> for Pair {
    fn from(change: &Change<'_>) -> Self {
        Self {
            crate_name: change.key.0.clone(),
            class: change.key.1,
            name: change.key.2.clone(),
            before: change.before.into(),
            after: change.after.into(),
            ambiguous: change.ambiguous,
        }
    }
}
#[derive(Debug, Serialize)]
pub struct Single {
    pub crate_name: String,
    pub class: &'static str,
    pub name: String,
    pub site: Location,
}
impl From<(&Key, &Site)> for Single {
    fn from((key, site): (&Key, &Site)) -> Self {
        Self {
            crate_name: key.0.clone(),
            class: key.1,
            name: key.2.clone(),
            site: site.into(),
        }
    }
}
/// Every candidate site for a repeated native key; pairing is not a unique resolution.
#[derive(Debug, Serialize)]
pub struct Ambiguity {
    pub crate_name: String,
    pub class: &'static str,
    pub name: String,
    pub before: Vec<Location>,
    pub after: Vec<Location>,
}
#[derive(Debug, Serialize)]
pub struct VisibilityReport {
    pub schema: u32,
    pub scope: Option<String>,
    pub population: crate::population::TestSelection,
    pub basis: Basis,
    pub widened: Vec<Pair>,
    pub rescoped: Vec<Pair>,
    pub narrowed: Vec<Pair>,
    pub ambiguous_keys: usize,
    pub ambiguous: Vec<Ambiguity>,
    pub limits: Vec<&'static str>,
}
/// Written visibility changes, matched by crate/class/name; repeated names pair approximately.
/// This does not calculate Rust's effective visibility or prove a move safe.
pub fn visibility(
    before: &Snapshot,
    after: &Snapshot,
    scope: Option<&str>,
) -> Result<VisibilityReport> {
    let scope = Scope::of(before, after, scope)?;
    let crates = scope.crates(before, after);
    let matched = |name: &str| crates.contains(name);
    let old = inventory(before, &matched, false)?;
    let new = inventory(after, &matched, false)?;
    let changes = visibility_changes(&old, &new);
    let rows = |direction| {
        changes
            .iter()
            .filter(|change| {
                scope.admits(change.key, change.after)
                    && Direction::of(change.before, change.after) == direction
            })
            .map(Pair::from)
            .collect()
    };
    let ambiguous = ambiguities(&old, &new, &scope);
    Ok(VisibilityReport {
        schema: 1,
        scope: scope.selection().map(str::to_owned),
        population: crate::population::TestSelection::OutsideTestModules,
        basis: Basis::of(before, after, &matched),
        widened: rows(Direction::Widened),
        rescoped: rows(Direction::Rescoped),
        narrowed: rows(Direction::Narrowed),
        ambiguous_keys: ambiguous.len(),
        ambiguous,
        limits: vec![
            "Written visibility ladder, not effective Rust visibility; module-relative scopes are not resolved.",
            "Repeated keys pair approximately; equal visibility occurrences cancel before remaining pairs.",
            "Native field inventory covers structs, not unions; union field visibility is not measured.",
        ],
    })
}
#[derive(Debug, Serialize)]
pub struct MoveReport {
    pub schema: u32,
    pub scope: Option<String>,
    pub population: crate::population::TestSelection,
    pub basis: Basis,
    pub exact: usize,
    pub relocated: usize,
    pub rerooted: Vec<Pair>,
    pub changed: Vec<Pair>,
    pub only_before: Vec<Single>,
    pub only_after: Vec<Single>,
    pub unreadable: Vec<Single>,
    pub limits: Vec<&'static str>,
}
/// Compare native normalized token readings. Scope narrows reported sites after whole-crate matching.
/// Exact ignores docs, written visibility and layout. Rerooted also forgives selected path qualifiers.
/// Neither reading establishes semantic equivalence; unreadable spans remain explicit.
pub fn moved(before: &Snapshot, after: &Snapshot, scope: Option<&str>) -> Result<MoveReport> {
    let scope = Scope::of(before, after, scope)?;
    let crates = scope.crates(before, after);
    let matched = |name: &str| crates.contains(name);
    let old = inventory(before, &matched, true)?;
    let new = inventory(after, &matched, true)?;
    let report = BodyReport::between(&old, &new, &scope);
    Ok(MoveReport {
        schema: 1,
        scope: scope.selection().map(str::to_owned),
        population: crate::population::TestSelection::OutsideTestModules,
        basis: Basis::of(before, after, &matched),
        exact: report.exact,
        relocated: report.relocated,
        rerooted: report.rerooted.iter().map(Pair::from).collect(),
        changed: report.changed.iter().map(Pair::from).collect(),
        only_before: report.only_before.into_iter().map(Single::from).collect(),
        only_after: report.only_after.into_iter().map(Single::from).collect(),
        unreadable: report.unreadable.into_iter().map(Single::from).collect(),
        limits: vec![
            "Native lexical keys are crate/class/name, including inline-module and impl-owner names; duplicate keys remain approximate.",
            "Exact forgives docs, visibility and layout, including closure-brace/trailing-comma normalization outside parentheses and macro input.",
            "Rerooting forgives crate/super/self and declared or imported module paths; different destinations can reroot equal, so review these rows.",
            "No macro expansion, type resolution or behavior-equivalence claim; macro input tokens are preserved without normalization.",
            "Modules and fields are not separate body comparisons; use rows report named-binding retargeting only; glob retargets are not measured.",
        ],
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        sync::atomic::{AtomicUsize, Ordering},
    };
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    fn workspace(files: &[(&str, &str)]) -> Snapshot {
        let root = std::env::temp_dir().join(format!(
            "atlas-move-fixture-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("crates/murail-fix/src")).expect("fixture directories");
        fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers=[\"crates/murail-fix\"]\nresolver=\"3\"\n",
        )
        .expect("fixture workspace");
        fs::write(
            root.join("crates/murail-fix/Cargo.toml"),
            "[package]\nname=\"murail-fix\"\nversion=\"0.0.0\"\nedition=\"2024\"\n",
        )
        .expect("fixture package");
        fs::write(root.join("crates/murail-fix/src/lib.rs"), "").expect("fixture target");
        for (path, text) in files {
            let path = root.join("crates/murail-fix/src").join(path);
            fs::create_dir_all(path.parent().expect("parent")).expect("fixture source dir");
            fs::write(path, text).expect("fixture source");
        }
        let metadata = cargo_metadata::MetadataCommand::new()
            .current_dir(&root)
            .no_deps()
            .exec()
            .expect("fixture metadata");
        let snapshot = crate::discovery::snapshot_metadata(&metadata).expect("fixture snapshot");
        fs::remove_dir_all(root).expect("clean fixture");
        snapshot
    }

    fn all(_: &str) -> bool {
        true
    }

    const QUEUE: &str = "\
use crate::rt::Slot;

/// A queue.
pub struct Queue<T> where T: Fn(u8) {
    head: usize,
    pub(super) tail: usize,
}

pub(crate) struct Pair(u32, pub u32);

impl<T> Queue<T> where T: Fn(u8) {
    fn push(&self) -> Slot { crate::rt::Slot::new(self.head) }
}

impl<T> Drop for Queue<T> where T: Fn(u8) {
    fn drop(&mut self) {}
}

fn helper() {}
";

    /// One file renamed by the move; `after` may also rewrite it.
    fn sides(before: &str, after: &str, bodies: bool) -> (Inventory, Inventory) {
        (
            inventory(&workspace(&[("queue.rs", before)]), &all, bodies).expect("before inventory"),
            inventory(&workspace(&[("rt/queue.rs", after)]), &all, bodies)
                .expect("after inventory"),
        )
    }

    fn visibility_rows(before: &str, after: &str) -> Vec<(String, &'static str, String, String)> {
        let (before, after) = sides(before, after, false);
        visibility_changes(&before, &after)
            .into_iter()
            .map(|change| {
                (
                    change.key.2.clone(),
                    Direction::of(change.before, change.after).label(),
                    change.before.visibility_text(),
                    change.after.visibility_text(),
                )
            })
            .collect()
    }

    fn widened(before: &str, after: &str) -> Vec<String> {
        visibility_rows(before, after)
            .into_iter()
            .filter(|(_, direction, _, _)| *direction == "widened")
            .map(|(name, _, old, new)| format!("{name} {old} → {new}"))
            .collect()
    }

    #[test]
    fn a_pure_move_changes_no_visibility() {
        assert!(visibility_rows(QUEUE, QUEUE).is_empty());
    }

    #[test]
    fn widened_fields_methods_and_items_are_reported_across_a_move() {
        let after = QUEUE
            .replace("    head: usize", "    pub(crate) head: usize")
            .replace("    pub(super) tail", "    pub(in crate::rt) tail")
            .replace("(u32, pub u32)", "(pub(crate) u32, pub u32)")
            .replace("    fn push", "    pub fn push")
            .replace("fn helper", "pub(super) fn helper");
        assert_eq!(
            widened(QUEUE, &after),
            vec![
                "Queue.head private → pub(crate)",
                "Queue.tail pub(super) → pub(in crate::rt)",
                "Pair.0 private → pub(crate)",
                "Queue::push private → pub",
                "helper private → pub(super)",
            ]
        );
    }

    #[test]
    fn narrowing_is_reported_as_narrowing() {
        let after = QUEUE.replace("    pub(super) tail", "    tail");
        assert_eq!(
            visibility_rows(QUEUE, &after),
            vec![(
                "Queue.tail".to_owned(),
                "narrowed",
                "pub(super)".to_owned(),
                "private".to_owned()
            )]
        );
    }

    #[test]
    fn reexports_restricted_paths_and_same_named_sites_cannot_hide_a_widening() {
        let reexport = "mod m { pub struct X; }\npub(crate) use m::X;\n";
        assert_eq!(
            widened(reexport, &reexport.replace("pub(crate) use", "pub use")),
            vec!["X pub(crate) → pub"]
        );
        let import = "mod m { pub struct X; }\nuse m::X;\n";
        assert_eq!(
            widened(import, &import.replace("use m::X", "pub use m::X")),
            vec!["X private → pub"]
        );
        // Same-named fns in two inline modules swap visibility. The grammar
        // reads one declaration per line, as rustfmt writes them.
        let swap = "mod a {\n    fn f() {}\n}\nmod b {\n    pub fn f() {}\n}\n";
        let swapped = "mod a {\n    pub fn f() {}\n}\nmod b {\n    fn f() {}\n}\n";
        assert_eq!(widened(swap, swapped), vec!["a::f private → pub"]);
        // `pub(in crate)` is the crate rung; an ancestor scope widens.
        assert_eq!(
            widened(
                "pub(in crate::a::b) fn f() {}\n",
                "pub(in crate) fn f() {}\n"
            ),
            vec!["f pub(in crate::a::b) → pub(crate)"]
        );
        assert_eq!(
            widened(
                "pub(in crate::a::b) fn f() {}\n",
                "pub(in crate::a) fn f() {}\n"
            ),
            vec!["f pub(in crate::a::b) → pub(in crate::a)"]
        );
        assert_eq!(
            visibility_rows(
                "pub(in crate::a) fn f() {}\n",
                "pub(in crate::b) fn f() {}\n"
            )[0]
            .1,
            "rescoped"
        );
        assert_eq!(
            widened(
                "unsafe extern \"C\" fn f() {}\n",
                "pub unsafe extern \"C\" fn f() {}\n"
            ),
            vec!["f private → pub"]
        );
        assert_eq!(
            widened(
                "pub struct P(#[allow(x)] u32);\n",
                "pub struct P(#[allow(x)] pub u32);\n"
            ),
            vec!["P.0 private → pub"]
        );
    }

    #[test]
    fn a_path_scope_matches_the_whole_crate_before_reporting() {
        // The item moved into the scoped folder and widened on the way.
        let before = workspace(&[("tick.rs", "fn advance() {}\n")]);
        let after = workspace(&[("engine/tick.rs", "pub(crate) fn advance() {}\n")]);
        let scope =
            Scope::of(&before, &after, Some("crates/murail-fix/src/engine")).expect("scope");
        let crates = scope.crates(&before, &after);
        let matched = |name: &str| crates.contains(name);
        let (old, new) = (
            inventory(&before, &matched, true).expect("before inventory"),
            inventory(&after, &matched, true).expect("after inventory"),
        );
        let reported: Vec<_> = visibility_changes(&old, &new)
            .into_iter()
            .filter(|change| scope.admits(change.key, change.after))
            .map(|change| change.key.2.clone())
            .collect();
        assert_eq!(reported, vec!["advance"]);
        let report = BodyReport::between(&old, &new, &scope);
        assert_eq!((report.exact, report.relocated), (1, 1));
        assert!(report.only_after.is_empty() && report.only_before.is_empty());
        for refused in ["", "crates/murail-fix/src/nowhere", "murail-absent"] {
            assert!(
                Scope::of(&before, &after, Some(refused)).is_err(),
                "{refused:?}"
            );
        }
    }

    fn classes(before: &str, after: &str) -> (usize, usize, usize, usize, usize) {
        let (before, after) = sides(before, after, true);
        let report = BodyReport::between(&before, &after, &Scope::Workspace);
        (
            report.exact,
            report.rerooted.len(),
            report.changed.len(),
            report.only_before.len(),
            report.only_after.len(),
        )
    }

    #[test]
    fn a_move_that_rewrites_docs_visibility_and_layout_is_exact() {
        let after = QUEUE
            .replace("/// A queue.", "/// The queue, now in rt.")
            .replace("    fn push", "    pub(crate) fn push");
        // Queue, Pair, two impl headers, push, drop, helper.
        assert_eq!(classes(QUEUE, &after), (7, 0, 0, 0, 0));
        assert_eq!(
            classes(
                "fn f() { g(|x| h(x)); }\n",
                "fn f() {\n    g(|x| {\n        h(x)\n    });\n}\n"
            ),
            (1, 0, 0, 0, 0)
        );
    }

    #[test]
    fn a_module_import_alias_reroots_like_its_path() {
        assert_eq!(
            classes(
                "use crate::dispatch;\nfn f() { dispatch::g() }\n",
                "fn f() { crate::load::classify::g() }\n"
            ),
            (0, 1, 0, 0, 0)
        );
        // A type annotation's `:` is not a path separator.
        assert_eq!(
            classes("fn f(s: crate::a::S) {}\n", "fn f(s: super::S) {}\n"),
            (0, 1, 0, 0, 0)
        );
        // A name the file does not import stays a module name.
        assert_eq!(
            classes(
                "fn f() { dispatch::g() }\n",
                "fn f() { crate::load::classify::g() }\n"
            ),
            (0, 0, 1, 0, 0)
        );
        // `std::fmt::x` keeps its interior `fmt` though `fmt` is an alias.
        assert_eq!(
            classes(
                "use crate::fmt;\nfn f() { std::fmt::x() }\n",
                "use crate::fmt;\nfn f() { std::x() }\n"
            ),
            (0, 0, 1, 0, 0)
        );
    }

    #[test]
    fn a_retargeted_import_binding_is_changed() {
        // The body stays exact; the binding `a` now names another module.
        assert_eq!(
            classes(
                "use crate::a;\nfn f() { a::g() }\n",
                "use crate::b as a;\nfn f() { a::g() }\n"
            ),
            (1, 0, 1, 0, 0)
        );
        // A crate root re-exporting through its own child modules.
        assert_eq!(
            classes(
                "mod tick;\npub use tick::Engine;\n",
                "mod engine;\npub use engine::Engine;\n"
            ),
            (0, 0, 0, 0, 0)
        );
        // Rerooting the import itself is the move's own rewrite.
        assert_eq!(
            classes(
                "use crate::tick::Slot;\nfn f() -> Slot { Slot }\n",
                "use super::Slot;\nfn f() -> Slot { Slot }\n"
            ),
            (1, 0, 0, 0, 0)
        );
    }

    #[test]
    fn crate_relative_rerooting_is_its_own_visible_class() {
        let after = QUEUE.replace("crate::rt::Slot::new", "super::Slot::new");
        assert_eq!(classes(QUEUE, &after), (6, 1, 0, 0, 0));
    }

    #[test]
    fn semantic_changes_are_never_preserved() {
        // The check can go red: each pair differs in meaning.
        let cases = [
            (
                "fn f() -> u64 { u32::MAX as u64 }\n",
                "fn f() -> u64 { u64::MAX as u64 }\n",
            ),
            (
                "fn f() -> f64 { f32::EPSILON.into() }\n",
                "fn f() -> f64 { f64::EPSILON.into() }\n",
            ),
            (
                "fn f(v: &[u8]) -> u8 { v.iter().copied().collect::<u8>() }\n",
                "fn f(v: &[u8]) -> u8 { v.iter().copied().sum::<u8>() }\n",
            ),
            ("fn f() { a::g() }\n", "fn f() { b::g() }\n"),
            (
                "fn f(x: &mut u8, y: &mut u8) { std::mem::swap(x, y) }\n",
                "fn f(x: &mut u8, y: &mut u8) { std::ptr::swap(x, y) }\n",
            ),
            ("fn f() { crate::a::g() }\n", "fn f() { crate::b::G() }\n"),
            ("#[cfg(feature = \"x\")]\nfn f() {}\n", "fn f() {}\n"),
            ("const X: u32 = 1;\n", "const X: u32 = 2;\n"),
            ("enum E { A, B }\n", "enum E { A, C }\n"),
            (
                "trait T { fn f(&self) -> u8 { 1 } }\n",
                "trait T { fn f(&self) -> u8 { 2 } }\n",
            ),
            (
                "macro_rules! m { () => { 1 } }\n",
                "macro_rules! m { () => { 2 } }\n",
            ),
            (
                "fn f() { fn g() -> u8 { 1 } }\n",
                "fn f() { fn g() -> u8 { 2 } }\n",
            ),
            (
                "thread_local! { static S: u8 = 1; }\n",
                "thread_local! { static S: u8 = 2; }\n",
            ),
            (
                "fn f() { g(|x| { a(); h(x) }); }\n",
                "fn f() { g(|x| h(x)); }\n",
            ),
            (
                "fn f() -> bool { h(|x: bool| { x || true } && false) }\n",
                "fn f() -> bool { h(|x: bool| x || true && false) }\n",
            ),
            (
                "fn f(a: u8) -> u8 { a | { 1 } * 2 }\n",
                "fn f(a: u8) -> u8 { a | 1 * 2 }\n",
            ),
        ];
        for (before, after) in cases {
            assert_eq!(classes(before, after), (0, 0, 1, 0, 0), "{before}");
        }
    }
    #[test]
    fn public_reports_retain_unreadable_and_ambiguous_evidence() {
        let before = workspace(&[("a.rs", "fn same() {}\nfn broken() {\n")]);
        let after = workspace(&[
            ("a.rs", "pub fn same() {}\nfn broken() {\n"),
            ("b.rs", "fn same() {}\n"),
        ]);
        let visibility = visibility(&before, &after, None).expect("visibility report");
        assert_eq!(visibility.ambiguous_keys, 1);
        assert_eq!(visibility.ambiguous[0].name, "same");
        assert_eq!(visibility.ambiguous[0].after.len(), 2);
        // The unchanged private occurrence cancels; an addition is not a paired widening.
        assert!(visibility.widened.is_empty());
        let bodies = moved(&before, &after, None).expect("move report");
        assert!(!bodies.unreadable.is_empty());
        assert!(
            bodies
                .basis
                .before
                .sources
                .contains_key("crates/murail-fix/src/a.rs")
        );
        assert!(!bodies.limits.is_empty());
    }
    #[test]
    fn removed_scopes_are_read_from_the_before_snapshot() {
        let before = workspace(&[("old/item.rs", "fn retired() {}\n")]);
        let after = workspace(&[]);
        let report =
            moved(&before, &after, Some("./crates/murail-fix/src/old/")).expect("old scope");
        assert_eq!(report.only_before.len(), 1);
        assert_eq!(report.only_before[0].name, "retired");
        assert_eq!(report.basis.before.sources.len(), 2);
        assert!(visibility(&before, &after, Some("crates/murail-fix/src/old")).is_ok());
    }
    #[test]
    fn an_inherent_impl_loop_is_not_a_trait_header() {
        let snapshot = workspace(&[(
            "engine.rs",
            "impl Engine {\n    fn step(&self) { for value in values {} }\n}\n",
        )]);
        let found = inventory(&snapshot, &all, true).expect("inventory");
        assert!(found.keys().any(|key| key.1 == "impl" && key.2 == "Engine"));
        assert!(
            found
                .keys()
                .any(|key| key.1 == "method" && key.2 == "Engine::step")
        );
        assert!(!found.keys().any(|key| key.2.contains(" as ")));
    }
    #[test]
    fn impl_comment_words_do_not_rekey_a_declaration() {
        assert_eq!(
            classes(
                "impl /* for Bogus */ Trait for Foo {\n    fn f() {}\n}\n",
                "impl Trait for Foo {\n    fn f() {}\n}\n"
            ),
            (2, 0, 0, 0, 0)
        );
    }
    #[test]
    fn attributed_trait_impls_keep_the_trait_in_their_keys() {
        let snapshot = workspace(&[(
            "engine.rs",
            "#[cfg(feature = \"one\")]\nimpl Trait for Engine {\n    fn step(&self) {}\n}\n",
        )]);
        let found = inventory(&snapshot, &all, true).expect("inventory");
        assert!(
            found
                .keys()
                .any(|key| key.1 == "impl" && key.2 == "<Engine as Trait>")
        );
        assert!(
            found
                .keys()
                .any(|key| key.1 == "method" && key.2 == "<Engine as Trait>::step")
        );
    }
    #[test]
    fn qualified_traits_keep_their_last_segment_after_token_formatting() {
        let snapshot = workspace(&[(
            "errors.rs",
            "impl std::fmt::Display for Refused {}\nimpl std::error::Error for Refused {}\n",
        )]);
        let found = inventory(&snapshot, &all, true).expect("inventory");
        assert!(found.keys().any(|key| key.2 == "<Refused as Display>"));
        assert!(found.keys().any(|key| key.2 == "<Refused as Error>"));
        assert!(!found.keys().any(|key| key.2.contains(" as std>")));
    }
    #[test]
    fn const_argument_braces_do_not_end_an_impl_header() {
        assert_eq!(
            classes(
                "impl Trait<{1 + 2}> for Foo {}\n",
                "impl Trait<{1 + 3}> for Foo {}\n"
            ),
            (0, 0, 1, 0, 0)
        );
        assert_eq!(
            classes("#[cfg(feature = \"one\")]\nimpl Foo {}\n", "impl Foo {}\n"),
            (0, 0, 1, 0, 0)
        );
    }
    #[test]
    fn unreadable_imports_cannot_look_like_no_binding_changes() {
        let before = workspace(&[("item.rs", "use crate::X;\nfn f() {}\n")]);
        let after = workspace(&[("item.rs", "use crate::{X as};\nfn f() {}\n")]);
        for bodies in [true, false] {
            let error = inventory(&after, &all, bodies)
                .err()
                .expect("unavailable import");
            assert!(error.to_string().contains("src/item.rs:1"));
        }
        assert!(moved(&before, &after, None).is_err());
        assert!(visibility(&before, &after, None).is_err());
    }

    #[test]
    fn a_tuple_comma_is_not_layout() {
        assert_eq!(
            classes("fn f() { let x = (1,); }\n", "fn f() { let x = (1); }\n"),
            (0, 0, 1, 0, 0)
        );
    }
    #[test]
    fn macro_input_is_not_visibility_documentation_or_layout() {
        for (before, after) in [
            ("fn f() { m!(pub); }\n", "fn f() { m!(); }\n"),
            ("#[custom(pub)]\nfn f() {}\n", "#[custom()]\nfn f() {}\n"),
            (
                "fn f() { m!(#[doc = \"x\"] item); }\n",
                "fn f() { m!(item); }\n",
            ),
            ("fn f() { m!(x,); }\n", "fn f() { m!(x); }\n"),
            (
                "macro_rules! m { (pub) => { 1 }; () => { 2 }; }\n",
                "macro_rules! m { () => { 1 }; () => { 2 }; }\n",
            ),
        ] {
            assert_eq!(classes(before, after), (0, 0, 1, 0, 0), "{before}");
        }
    }
    #[cfg(unix)]
    #[test]
    fn excluded_source_diagnostics_survive_the_comparison_basis() {
        let before = workspace(&[("item.rs", "fn f() {}\n")]);
        // Snapshot diagnostics are sealed; obtain this case through real discovery.
        let root = std::env::temp_dir().join(format!("atlas-move-symlink-{}", std::process::id()));
        fs::create_dir_all(root.join("src")).expect("fixture");
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname=\"murail-fix\"\nversion=\"0.0.0\"\nedition=\"2024\"\n",
        )
        .expect("manifest");
        fs::write(root.join("src/lib.rs"), "fn f() {}\n").expect("source");
        std::os::unix::fs::symlink(root.join("src/lib.rs"), root.join("src/excluded.rs"))
            .expect("symlink");
        let metadata = cargo_metadata::MetadataCommand::new()
            .current_dir(&root)
            .no_deps()
            .exec()
            .expect("metadata");
        let after = crate::discovery::snapshot_metadata(&metadata).expect("snapshot");
        fs::remove_dir_all(root).expect("cleanup");
        let report = moved(&before, &after, None).expect("report");
        assert!(!report.basis.after.sources.contains_key("src/excluded.rs"));
        assert!(
            report
                .basis
                .after
                .diagnostics
                .iter()
                .any(|d| d.file == "src/excluded.rs")
        );
    }
}
