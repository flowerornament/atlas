//! Native path-fixed debt and no-growth laws over paired measurements.
use super::super::policy::{
    Alias, AliasKind, ConcentrationPolicy, Knob, LineRow, MissingInput, Ruling,
};
use super::{Citations, DebtSummary, Measure, Summary, Verdict};
use std::collections::{BTreeMap, BTreeSet};
impl Knob {
    const fn key(self) -> &'static str {
        match self {
            Self::EntryLines => "entry_lines",
            Self::FileLines => "file_lines",
            Self::Population => "population",
            Self::ConventionalNames => "conventional_names",
        }
    }
}

impl Alias {
    /// What the alias is, and the fix a refusal names.
    fn explain(&self) -> String {
        let Self {
            module: file,
            binding,
            source,
            ..
        } = self;
        match self.kind {
            AliasKind::ModuleRelay => format!(
                "`{file}` re-homes module `{source}` at the crate root as `{binding}`; import it from its true path, `crate::{source}`"
            ),
            AliasKind::RootShadow => format!(
                "`{file}` re-exports `{source}` as `{binding}`, the name of a top-level module; import it by its true path or give it its own name"
            ),
            AliasKind::RootRestricted => format!(
                "`{file}` binds `{binding}` from `{source}` by a crate-internal `use` at the root; import it by its true path"
            ),
            AliasKind::Rename => format!(
                "`{file}` re-exports `{source}` as `{binding}`, one item under two names; rename the item itself, or re-export it under its own name"
            ),
        }
    }
}

impl ConcentrationPolicy {
    pub(super) fn in_population(&self, name: &str) -> bool {
        self.population.iter().any(|member| member == name)
    }
}
/// What one fold found: growth refuses; an improvement passes with a hint
/// naming the lower row it now permits.
#[derive(Default)]
struct Findings {
    refusals: Vec<String>,
    hints: Vec<String>,
    unavailable: Vec<MissingInput>,
}

/// The landing law: the candidate tree against its own section, and the
/// section against the published one (`None` before the section exists).
/// Every cap reads the base tree: a listed path holds at most the lesser of
/// its published row and what it measured in the base. `description` is the
/// candidate's commit description, which must cite each new ruling's bead.
pub(super) fn landing(
    policy: &ConcentrationPolicy,
    published: Option<&ConcentrationPolicy>,
    reported: &Measure,
    base: &Measure,
    new: &Measure,
    description: &str,
    citations: Option<&Citations>,
) -> Verdict {
    let base_policy = published.unwrap_or(policy);
    let mut out = Findings::default();
    if let Some(published) = published {
        loosening(policy, published, description, citations, &mut out);
    }
    let joined: BTreeSet<String> = published.map_or_else(BTreeSet::new, |published| {
        policy
            .population
            .iter()
            .filter(|name| !published.in_population(name))
            .cloned()
            .collect()
    });
    let grown: BTreeMap<String, usize> = new
        .entry
        .keys()
        .filter(|path| !base.entry.contains_key(*path))
        .filter_map(|path| base.file.get(path).map(|lines| (path.clone(), *lines)))
        .collect();
    let trees = Trees {
        base,
        now: new,
        joined: &joined,
    };
    line_rows(
        &LineRule {
            rule: "entry",
            subject: "entry file",
            limit: policy.entry_lines,
            fix: "keep it a map (header, module declarations, re-exports) and move bodies into named child modules",
        },
        &policy.entry,
        published.map(|published| published.entry.as_slice()),
        &new.entry,
        &grown,
        &trees,
        &mut out,
    );
    line_rows(
        &LineRule {
            rule: "file",
            subject: "file",
            limit: policy.file_lines,
            fix: "split it at a seam (an impl's method families, or its top-level items) into sibling modules",
        },
        &policy.file,
        published.map(|published| published.file.as_slice()),
        &new.file,
        &BTreeMap::new(),
        &trees,
        &mut out,
    );
    out.refusals.extend(new.remapped.iter().map(|site| {
        format!(
            "{site} declares a production module with `#[path]`, which hides it from the layout the scan and the name homes read; move the file to the path its module name gives"
        )
    }));
    name_rows(policy, published, &trees, description, citations, &mut out);
    alias_rows(policy, published, &trees, &mut out);
    header_counts(policy, published, &trees, &mut out);
    let total = |measure: &Measure, policy: &ConcentrationPolicy| {
        (
            Measure::over(&measure.entry, policy.entry_lines),
            Measure::over(&measure.file, policy.file_lines),
        )
    };
    let (entry_before, file_before) = total(reported, base_policy);
    let (entry_after, file_after) = total(new, policy);
    let report = Summary {
        population: policy.population.clone(),
        entry: DebtSummary {
            limit: policy.entry_lines,
            rows: policy.entry.len(),
            over_before: entry_before,
            over_after: entry_after,
        },
        file: DebtSummary {
            limit: policy.file_lines,
            rows: policy.file.len(),
            over_before: file_before,
            over_after: file_after,
        },
        names: policy.name.len(),
        aliases: policy.alias.len(),
        header_uncited: new.uncited.values().map(BTreeSet::len).sum(),
        hints: out.hints,
        measured: new.rows(policy),
    };
    Verdict {
        violations: out.refusals,
        unavailable: out.unavailable,
        report: Some(report),
    }
}

/// The section may not loosen itself: a raised limit, a crate leaving the
/// population or a new conventional name needs a new ruling that covers that
/// knob and that the landing cites.
fn loosening(
    policy: &ConcentrationPolicy,
    published: &ConcentrationPolicy,
    description: &str,
    citations: Option<&Citations>,
    out: &mut Findings,
) {
    let amendment = policy
        .amended_by
        .as_ref()
        .filter(|amendment| published.amended_by.as_ref() != Some(*amendment));
    if let Some(amendment) = amendment
        && known_uncited(
            &amendment.bead,
            description,
            citations,
            &mut out.unavailable,
        )
    {
        out.refusals.push(format!(
            "concentration amended_by names `{}`, which this landing's description does not cite; cite it as ({}) so the bead-citations gate checks it exists",
            amendment.bead, amendment.bead
        ));
    }
    let mut loosened = Vec::new();
    for (knob, old, new) in [
        (Knob::EntryLines, published.entry_lines, policy.entry_lines),
        (Knob::FileLines, published.file_lines, policy.file_lines),
    ] {
        if new > old {
            loosened.push((knob, format!("{} rose from {old} to {new}", knob.key())));
        }
    }
    loosened.extend(
        published
            .population
            .iter()
            .filter(|name| !policy.in_population(name))
            .map(|name| (Knob::Population, format!("`{name}` left the population"))),
    );
    loosened.extend(
        policy
            .conventional_names
            .iter()
            .filter(|name| !published.conventional_names.contains(name))
            .map(|name| {
                (
                    Knob::ConventionalNames,
                    format!("`{name}` became a conventional name"),
                )
            }),
    );
    if let Some(amendment) = amendment {
        for knob in &amendment.covers {
            if !loosened.iter().any(|(moved, _)| moved == knob) {
                out.refusals.push(format!(
                    "concentration amended_by covers `{}`, which this landing does not loosen; list only what it loosens",
                    knob.key()
                ));
            }
        }
    }
    for (knob, change) in loosened {
        if !amendment.is_some_and(|amendment| amendment.covers.contains(&knob)) {
            out.refusals.push(format!(
                "concentration policy loosens: {change}; name the ruling with `amended_by = {{ bead = \"<bead id>\", covers = [\"{}\"] }}` in [concentration] and cite the bead, or restore it",
                knob.key()
            ));
        }
    }
}

/// The base and candidate measures a law reads, and the crates that join the
/// population this landing.
struct Trees<'a> {
    base: &'a Measure,
    now: &'a Measure,
    joined: &'a BTreeSet<String>,
}

impl Trees<'_> {
    /// A file of a crate joining the population that the base already held:
    /// its debt is recorded as it measured, not grown.
    fn recorded(&self, path: &str) -> Option<usize> {
        self.now
            .crates_of
            .get(path)
            .filter(|owners| {
                !owners.is_empty() && owners.iter().all(|name| self.joined.contains(name))
            })
            .and_then(|_| self.base.file.get(path).copied())
    }

    /// A module home of a crate joining the population, present in both
    /// trees: pre-existing, not new.
    fn recorded_home(&self, home: &str) -> bool {
        self.joined
            .contains(home.split("::").next().unwrap_or(home))
            && self.now.modules.contains(home)
            && self.base.modules.contains(home)
    }
}

struct LineRule {
    rule: &'static str,
    subject: &'static str,
    limit: usize,
    fix: &'static str,
}

/// The line-row law of one rule. A row is a ceiling fixed to its path: the
/// path's body lines, whatever its kind now (an entry row still holds a file
/// that lost its children), stay within the row. A path over the limit with
/// no row is new debt and refuses, except a base leaf in `grown` that gained
/// its first child, which may keep an entry row at its base size. Against the
/// published rows, see [`against_published`].
fn line_rows(
    rule: &LineRule,
    rows: &[LineRow],
    published: Option<&[LineRow]>,
    measured: &BTreeMap<String, usize>,
    grown: &BTreeMap<String, usize>,
    trees: &Trees<'_>,
    out: &mut Findings,
) {
    let law = rule;
    let LineRule {
        rule,
        subject,
        limit,
        fix,
    } = *rule;
    for row in rows {
        let (path, held) = (&row.path, row.lines);
        match measured.get(path).or_else(|| trees.now.file.get(path)).copied() {
            None => out.hints.push(format!(
                "{rule} row `{path}` can be deleted: no population {subject} has that path"
            )),
            Some(lines) if lines > held => out.refusals.push(format!(
                "{subject} `{path}` is {lines} body lines, above its row {held}; {fix} (a row never rises)"
            )),
            Some(lines) if lines <= limit => out.hints.push(format!(
                "{rule} row `{path}` can be deleted: {lines} body lines, within the {limit}-line limit"
            )),
            Some(lines) if lines < held => out.hints.push(format!(
                "{rule} row `{path}` can ratchet to {lines} (row {held})"
            )),
            Some(_) => {}
        }
    }
    let listed: BTreeSet<&str> = rows.iter().map(|row| row.path.as_str()).collect();
    for (path, lines) in measured {
        if *lines <= limit || listed.contains(path.as_str()) {
            continue;
        }
        out.refusals.push(match grown.get(path) {
            Some(base) if *base > 0 && lines <= base => format!(
                "`{path}` became an entry file (it gained its first child module) at {lines} body lines, above the {limit}-line limit; bring it to at most {limit}, or carry it at no more than its base size {base} with the {rule} row {{ path = \"{path}\", lines = {lines} }}"
            ),
            Some(base) if *base > 0 => format!(
                "`{path}` became an entry file (it gained its first child module) and grew from {base} to {lines} body lines; bring it to at most {limit}, or back to at most {base} and carry it with its {rule} row"
            ),
            Some(_) => format!(
                "new {subject} `{path}` is {lines} body lines; a new entry file must be within the {limit}-line limit: {fix}"
            ),
            None => format!(
                "new {subject} `{path}` is {lines} body lines, above the {limit}-line limit; a row stays with its path, so a renamed or split file's pieces each stay within the limit: split it into pieces of at most {limit} body lines"
            ),
        });
    }
    if let Some(published) = published {
        against_published(law, rows, published, measured, grown, trees, out);
    }
}

/// A rule's rows against the published rows and the base tree. A published
/// row is fixed to its path and holds it to the lesser of the row and the
/// path's base body lines (0 if the base lacks it), so old slack is never
/// spent. A new row is a grown entry at its base size, or the true measure of
/// a file of a crate joining the population; nothing else.
fn against_published(
    rule: &LineRule,
    rows: &[LineRow],
    published: &[LineRow],
    measured: &BTreeMap<String, usize>,
    grown: &BTreeMap<String, usize>,
    trees: &Trees<'_>,
    out: &mut Findings,
) {
    let LineRule {
        rule,
        subject,
        limit,
        fix,
    } = *rule;
    let before: BTreeMap<&str, usize> = published
        .iter()
        .map(|row| (row.path.as_str(), row.lines))
        .collect();
    let current: BTreeSet<&str> = rows.iter().map(|row| row.path.as_str()).collect();
    for path in before.keys().filter(|path| !current.contains(*path)) {
        if let Some(lines) = trees.now.file.get(*path)
            && *lines > limit
            && !measured.contains_key(*path)
        {
            out.refusals.push(format!(
                "concentration {rule} row `{path}` is gone while its path is {lines} body lines, above the {limit}-line limit; its {rule} row stays while the path does: restore the row, and {fix}"
            ));
        }
    }
    for row in rows {
        let Some(old) = before.get(row.path.as_str()) else {
            match (grown.get(&row.path), trees.recorded(&row.path)) {
                (Some(base), _) if row.lines <= *base => {}
                (Some(base), _) => out.refusals.push(format!(
                    "{subject} `{}` became an entry file this landing; its {rule} row may hold at most its base size {base}, not {}",
                    row.path, row.lines
                )),
                (None, Some(base)) if row.lines <= base => {}
                (None, Some(base)) => out.refusals.push(format!(
                    "concentration {rule} row `{}` records {} lines for a crate joining the population, above the {base} it measured in the base; recorded debt is its true measure",
                    row.path, row.lines
                )),
                (None, None) => out.refusals.push(format!(
                    "new concentration {rule} row `{}` names a path the published policy does not hold; a row stays with its path, so a new path stays within the {limit}-line limit: split it into pieces of at most {limit} body lines",
                    row.path
                )),
            }
            continue;
        };
        if row.lines > *old {
            out.refusals.push(format!(
                "concentration {rule} row `{}` rose from {old} to {}; rows only go down: {fix}",
                row.path, row.lines
            ));
        }
        let base = trees.base.file.get(&row.path).copied().unwrap_or(0);
        if let Some(lines) = measured
            .get(&row.path)
            .or_else(|| trees.now.file.get(&row.path))
            .copied()
            && lines > limit
            && lines <= row.lines
            && lines > base
        {
            out.refusals.push(format!(
                "{subject} `{}` is {lines} body lines, above the {base} it measured in the base; a row's slack is never spent, so a file over the limit never grows: {fix}",
                row.path
            ));
        }
    }
}

fn name_rows(
    policy: &ConcentrationPolicy,
    published: Option<&ConcentrationPolicy>,
    trees: &Trees<'_>,
    description: &str,
    citations: Option<&Citations>,
    out: &mut Findings,
) {
    let measured = trees.now;
    let listed = |path: &BTreeSet<String>| {
        path.iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(", ")
    };
    for row in &policy.name {
        let name = &row.name;
        match measured.homes.get(name) {
            None => out.hints.push(format!(
                "name row `{name}` can be deleted: the name has one home now"
            )),
            Some(homes) if !homes.is_subset(&row.homes) => out.refusals.push(format!(
                "module name `{name}` gained {} beyond its row; rename the new module so the name keeps one meaning, or add the home to the row (a facet only)",
                listed(&homes.difference(&row.homes).cloned().collect()),
            )),
            Some(homes) if *homes != row.homes => out.hints.push(format!(
                "name row `{name}` can drop {}",
                listed(&row.homes.difference(homes).cloned().collect()),
            )),
            Some(_) => {}
        }
        let Some(published) = published else {
            continue;
        };
        match published.name.iter().find(|old| old.name == *name) {
            // New collision debt is never admitted by writing it down; only a
            // crate joining the population records the collision it brings:
            // its homes pre-exist, beside at most one pre-existing home
            // elsewhere.
            None if row.ruling == Ruling::Collision => {
                let (joined, others): (Vec<&String>, Vec<&String>) =
                    row.homes.iter().partition(|home| trees.recorded_home(home));
                let recorded = !joined.is_empty()
                    && others.len() <= 1
                    && others.iter().all(|home| trees.base.modules.contains(*home));
                if !recorded {
                    out.refusals.push(format!(
                        "module name `{name}` gets a new collision row; a collision is debt owed a rename and is not admitted by a row: give the new module a name of its own"
                    ));
                }
            }
            // A published collision gains no home, whatever its row now says,
            // and becomes a facet only by a cited re-ruling. A module that
            // moves keeps its home only if its module id stays.
            Some(old) if old.ruling == Ruling::Collision => {
                if row.ruling == Ruling::Facet {
                    match &row.reruled_by {
                        None => out.refusals.push(format!(
                            "module name `{name}` is a published collision re-ruled a facet without naming who re-ruled it; add `reruled_by = \"<bead id>\"` to its row and cite the bead, or keep it a collision"
                        )),
                        Some(bead) if known_uncited(bead, description, citations, &mut out.unavailable) => out.refusals.push(format!(
                            "module name `{name}` is re-ruled a facet by `{bead}`, which this landing's description does not cite; cite it as ({bead}) so the bead-citations gate checks it exists"
                        )),
                        Some(_) => {}
                    }
                }
                let gained: BTreeSet<String> = row
                    .homes
                    .difference(&old.homes)
                    .filter(|home| !trees.recorded_home(home))
                    .cloned()
                    .collect();
                if !gained.is_empty() {
                    out.refusals.push(format!(
                        "module name `{name}` is a published collision, two concepts owed a rename, and gained {}; give the module a name of its own, so the collision goes away (a move keeps its home only when the module id stays, as `plan.rs` to `plan/mod.rs`; re-ruling it a facet does not admit a new home)",
                        listed(&gained)
                    ));
                }
            }
            _ => {}
        }
    }
    let ruled: BTreeSet<&str> = policy.name.iter().map(|row| row.name.as_str()).collect();
    for (name, homes) in &measured.homes {
        if !ruled.contains(name.as_str()) {
            out.refusals.push(format!(
                "module name `{name}` has {} homes ({}) and no ruling; rename the new module so each name holds one meaning, or, if it is one concept seen from several stages, add a concentration name row ruling it `facet` and saying why",
                homes.len(),
                listed(homes)
            ));
        }
    }
}

fn alias_rows(
    policy: &ConcentrationPolicy,
    published: Option<&ConcentrationPolicy>,
    trees: &Trees<'_>,
    out: &mut Findings,
) {
    let measured = trees.now;
    let rows: BTreeSet<&Alias> = policy.alias.iter().collect();
    for alias in &measured.aliases {
        if !rows.contains(alias) {
            out.refusals.push(format!("new {}", alias.explain()));
        }
    }
    for alias in policy
        .alias
        .iter()
        .filter(|alias| !measured.aliases.contains(alias))
    {
        out.hints.push(format!(
            "alias row ({}, {:?}, `{}`) can be deleted: it is gone from the tree",
            alias.module, alias.kind, alias.binding
        ));
    }
    let Some(published) = published else {
        return;
    };
    let before: BTreeSet<&Alias> = published.alias.iter().collect();
    for alias in policy.alias.iter().filter(|alias| !before.contains(alias)) {
        // A crate joining the population records the aliases it already had;
        // any other new row, a moved alias included, is a new alias: retire it.
        let recorded = trees.recorded_home(&alias.module) && trees.base.aliases.contains(alias);
        if !recorded {
            out.refusals.push(format!(
                "concentration alias rows are listed once and only go away; this landing adds one: {}",
                alias.explain()
            ));
        }
    }
}

/// Uncited folder roots per crate: at most the lesser of the row and the
/// base count; a joining crate records its true base count.
fn header_counts(
    policy: &ConcentrationPolicy,
    published: Option<&ConcentrationPolicy>,
    trees: &Trees<'_>,
    out: &mut Findings,
) {
    for (name, uncited) in &trees.now.uncited {
        let held = policy.header_uncited.get(name).copied().unwrap_or(0);
        let based = trees.base.uncited.get(name);
        let base = based.map_or(0, BTreeSet::len);
        let count = uncited.len();
        let fresh: Vec<&str> = uncited
            .iter()
            .filter(|path| !based.is_some_and(|old| old.contains(*path)))
            .map(String::as_str)
            .collect();
        let shown = if fresh.is_empty() {
            uncited.iter().map(String::as_str).collect::<Vec<_>>()
        } else {
            fresh.clone()
        };
        if published.is_some() && !fresh.is_empty() {
            out.refusals.push(format!(
                "`{name}` has new folder roots whose //! header cites no formal-model section or .design spec ({}); a new folder root cites the section or spec it realizes, whatever the count",
                fresh.join(", ")
            ));
        } else if count > held {
            out.refusals.push(format!(
                "`{name}` has {count} folder roots whose //! header cites no formal-model section or .design spec, above {held}; cite the section or spec each realizes in its header ({})",
                shown.join(", ")
            ));
        } else if count < held {
            out.hints.push(format!(
                "header_uncited.{name} can ratchet to {count} (row {held})"
            ));
        }
        let Some(published) = published else {
            continue;
        };
        match published.header_uncited.get(name) {
            Some(old) if held > *old => out.refusals.push(format!(
                "concentration header_uncited.{name} rose from {old} to {held}; the count only goes down: cite the section or spec in the new root's header"
            )),
            None if trees.joined.contains(name) && held > base => out.refusals.push(format!(
                "concentration header_uncited.{name} records {held} for a crate joining the population, above the {base} it measured in the base; recorded debt is its true measure"
            )),
            _ => {}
        }
    }
}

// False means no known absence: missing/stale interpretation records unavailable.
fn known_uncited(
    bead: &str,
    description: &str,
    citations: Option<&Citations>,
    unavailable: &mut Vec<MissingInput>,
) -> bool {
    match citations {
        Some(reading)
            if reading.description_fingerprint
                == crate::discovery::fingerprint(description.as_bytes()) =>
        {
            !reading.beads.contains(bead)
        }
        supplied => {
            unavailable.push(MissingInput {
                field: format!("concentration.citation.{bead}"),
                reason: if supplied.is_some() {
                    "parsed commit citations do not match the candidate description".into()
                } else {
                    "project commit-citation interpretation unavailable".into()
                },
            });
            false
        }
    }
}
