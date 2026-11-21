//! Held spelling debt for `forbid_new_pattern` and `forbid_new_observed`:
//! one capture's occurrences of each spelling outside its authority's home,
//! counted per (file, owning item, spelling), less what `allowed_in` rows
//! grant. The law over a change (debt may only fall, and moves only with
//! their item) is delta's; this is the census both sides share.
//!
//! An occurrence's owner is the chain of outline items whose syntax span
//! holds it, outermost first, joined with `::` (an impl by its self-type
//! head, as `Type::method` is spelled elsewhere; `use`, macro calls and
//! `extern` blocks never join it; `""` is outside every item). Atlas
//! computes it from the outline, never from the helper or the wrapper.
//! Occurrences in test code, test-only files and inside literal tokens are
//! dropped, and counted.
use super::{Allowance, AuthorityRule};
use crate::{
    discovery::Snapshot,
    population::TestSelection,
    source::{Item, ItemKind, Source},
};
use serde::Serialize;
use std::collections::BTreeMap;

/// One spelling occurrence in one capture's bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct Occurrence {
    pub file: String,
    pub start: usize,
    pub end: usize,
    pub line: usize,
    pub text: String,
}
impl Occurrence {
    pub fn new(file: String, start: usize, end: usize, line: usize, text: String) -> Self {
        Self {
            file,
            start,
            end,
            line,
            text,
        }
    }
}

/// A held-debt key: a file, the item that owns the spelling there, and the
/// spelling (a pattern's name, or `class:text` for an observed class).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[non_exhaustive]
pub struct Key {
    pub file: String,
    pub item: String,
    pub spelling: String,
}

/// One key's counts on one side.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct Held {
    pub count: usize,
    pub granted: usize,
    /// The first occurrence's line, for messages.
    pub line: usize,
}
impl Held {
    pub fn debt(&self) -> usize {
        self.count - self.granted
    }
}

/// One side's census for one authority.
#[derive(Debug, Default, Serialize)]
#[non_exhaustive]
pub struct Census {
    pub keys: BTreeMap<Key, Held>,
    /// Per `allowed_in` row (by index), relative path and spelling: how much
    /// of the bound the side uses.
    pub used: BTreeMap<(usize, String, String), usize>,
    /// Occurrences left out: in test code or inside a literal.
    pub dropped: usize,
    /// Files with production source, so an empty side can refuse.
    pub files: usize,
}

/// The census of `occurrences` (spelling key, occurrence) for `authority`,
/// whose home spans `home` (file and byte range, or a whole file).
pub(in crate::checks) fn census(
    snapshot: &Snapshot,
    authority: &AuthorityRule,
    home: &[(String, Option<std::ops::Range<usize>>)],
    occurrences: &[(String, Occurrence)],
) -> Census {
    let mut census = Census {
        files: snapshot
            .graph()
            .files
            .iter()
            .filter(|f| !f.test_only)
            .count(),
        ..Census::default()
    };
    let mut literals: BTreeMap<&str, Vec<std::ops::Range<usize>>> = BTreeMap::new();
    for (spelling, occurrence) in occurrences {
        let Some(source) = production_source(snapshot, &occurrence.file) else {
            census.dropped += 1;
            continue;
        };
        let literal = literals
            .entry(&occurrence.file)
            .or_insert_with(|| literal_ranges(source.text()))
            .iter()
            .any(|range| range.contains(&occurrence.start));
        if !TestSelection::Production.admits_offset(source, occurrence.start) || literal {
            census.dropped += 1;
            continue;
        }
        if at_home(home, occurrence) {
            continue;
        }
        let chain = chain(source, occurrence.start, occurrence.end);
        let key = Key {
            file: occurrence.file.clone(),
            item: names(&chain).join("::"),
            spelling: spelling.clone(),
        };
        let granted = grant(&authority.allowed_in, &chain, &key, &mut census.used);
        let held = census.keys.entry(key).or_insert_with(|| Held {
            line: occurrence.line,
            ..Held::default()
        });
        held.count += 1;
        held.granted += usize::from(granted);
    }
    census
}

fn production_source<'a>(snapshot: &'a Snapshot, file: &str) -> Option<&'a Source> {
    let production = snapshot
        .graph()
        .files
        .iter()
        .any(|f| f.path == file && !f.test_only);
    production.then(|| snapshot.source_facts(file)).flatten()
}
fn at_home(home: &[(String, Option<std::ops::Range<usize>>)], occurrence: &Occurrence) -> bool {
    home.iter().any(|(file, span)| {
        *file == occurrence.file
            && span
                .as_ref()
                .is_none_or(|span| span.start <= occurrence.start && occurrence.end <= span.end)
    })
}

/// The outline items whose syntax span holds `start..end`, outermost
/// first; `use`, macro calls and `extern` blocks never own a spelling.
pub(in crate::checks) fn chain(source: &Source, start: usize, end: usize) -> Vec<&Item> {
    let mut items: Vec<&Item> = source
        .outline()
        .iter()
        .filter(|item| {
            !matches!(
                item.kind,
                ItemKind::Use | ItemKind::PubUse | ItemKind::MacroCall | ItemKind::Extern
            )
        })
        .filter(|item| item.syntax_span.start <= start && end <= item.syntax_span.end)
        .collect();
    items.sort_by_key(|item| {
        (
            item.syntax_span.start,
            std::cmp::Reverse(item.syntax_span.end),
        )
    });
    items
}
fn names<'a>(chain: &[&'a Item]) -> Vec<&'a str> {
    chain.iter().map(|item| item.name.as_str()).collect()
}

/// Take one unit of the first `allowed_in` row (in `key.file`) that
/// addresses a prefix of `chain`, under the remainder's relative path, if
/// its bound has room.
fn grant(
    rows: &[Allowance],
    chain: &[&Item],
    key: &Key,
    used: &mut BTreeMap<(usize, String, String), usize>,
) -> bool {
    for (index, row) in rows
        .iter()
        .enumerate()
        .filter(|(_, row)| row.file == key.file)
    {
        let Some(relative) = address(row, chain) else {
            continue;
        };
        let bound = row
            .counts
            .get(&relative)
            .and_then(|counts| counts.get(&key.spelling))
            .copied()
            .unwrap_or(0);
        let taken = used
            .entry((index, relative, key.spelling.clone()))
            .or_default();
        if *taken < bound {
            *taken += 1;
            return true;
        }
    }
    false
}

/// Where `row` addresses `chain`: the path relative to its item (`""` for
/// the item itself), or `None`. A row names a top-level trait impl by its
/// header (`impl Scalar for f32`, whitespace collapsed, generics and where
/// clause kept), or a chain prefix ending at a fn, struct, enum, union,
/// type, const or static by its names; never a module or a macro.
fn address(row: &Allowance, chain: &[&Item]) -> Option<String> {
    let rest = |from: usize| names(&chain[from..]).join("::");
    if let Some(first) = chain.first()
        && first.kind == ItemKind::Impl
        && first.top_level
        && impl_header(first) == row.item
    {
        return Some(rest(1));
    }
    (1..=chain.len()).find_map(|len| {
        let last = chain[len - 1];
        let addressable = matches!(
            last.kind,
            ItemKind::Fn
                | ItemKind::Struct
                | ItemKind::Enum
                | ItemKind::Union
                | ItemKind::TypeAlias
                | ItemKind::Const
                | ItemKind::Static
        );
        (addressable && names(&chain[..len]).join("::") == row.item).then(|| rest(len))
    })
}
/// An impl's header up to its `{`, whitespace collapsed.
fn impl_header(item: &Item) -> String {
    let block = item.declaration_block.split('{').next().unwrap_or_default();
    block.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The byte ranges of a file's string, char and byte literal tokens.
fn literal_ranges(text: &str) -> Vec<std::ops::Range<usize>> {
    fn walk(stream: proc_macro2::TokenStream, out: &mut Vec<std::ops::Range<usize>>) {
        for token in stream {
            match token {
                proc_macro2::TokenTree::Group(group) => walk(group.stream(), out),
                proc_macro2::TokenTree::Literal(literal) => {
                    let written = literal.to_string();
                    if written.contains('"') || written.contains('\'') {
                        out.push(literal.span().byte_range());
                    }
                }
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    if let Ok(stream) = text.parse::<proc_macro2::TokenStream>() {
        walk(stream, &mut out);
    }
    out
}

/// The shape of `forbid_new_pattern`, `forbid_new_observed` and
/// `allowed_in`, before anything is observed.
pub(super) fn validate(authority: &AuthorityRule) -> crate::Result<()> {
    let fail = |why: String| -> crate::Result<()> {
        Err(format!("policy: authority `{}`: {why}", authority.fact).into())
    };
    let mut names = std::collections::BTreeSet::new();
    let written = authority
        .forbid_new_pattern
        .iter()
        .map(|p| p.name.as_str())
        .chain(authority.forbid_new_observed.iter().map(String::as_str));
    for name in written {
        if name.trim().is_empty() || name.contains(':') || !names.insert(name) {
            return fail(format!(
                "forbid_new name `{name}` must be nonempty, unique and free of `:`"
            ));
        }
    }
    if authority
        .forbid_new_pattern
        .iter()
        .any(|p| p.pattern.trim().is_empty())
    {
        return fail("a forbid_new_pattern needs a pattern".into());
    }
    if authority
        .amended_by
        .as_ref()
        .is_some_and(|amendment| amendment.covers.is_empty())
    {
        return fail("amended_by must name the spellings it covers".into());
    }
    if !authority.allowed_in.is_empty() && names.is_empty() {
        return fail("allowed_in grants spellings, but no forbid_new_pattern or forbid_new_observed is written".into());
    }
    let mut granted = std::collections::BTreeSet::new();
    for row in &authority.allowed_in {
        validate_row(authority, row, &names, &mut granted)?;
    }
    Ok(())
}
fn validate_row<'a>(
    authority: &AuthorityRule,
    row: &'a Allowance,
    names: &std::collections::BTreeSet<&str>,
    granted: &mut std::collections::BTreeSet<(&'a str, &'a str, &'a str, &'a str)>,
) -> crate::Result<()> {
    let at = format!(
        "policy: authority `{}`: allowed_in `{}::{}`",
        authority.fact, row.file, row.item
    );
    crate::architecture::relative_path(&row.file).map_err(|error| format!("{at}: {error}"))?;
    if row.item.trim().is_empty() || row.reason.trim().is_empty() || row.counts.is_empty() {
        return Err(format!("{at} needs an item, a reason and counts").into());
    }
    let observed = |spelling: &str| {
        spelling.split_once(':').is_some_and(|(class, _)| {
            authority
                .forbid_new_observed
                .iter()
                .any(|name| name == class)
        })
    };
    for (relative, counts) in &row.counts {
        if counts.is_empty() {
            return Err(format!("{at} gives no spelling under `{relative}`").into());
        }
        for (spelling, bound) in counts {
            if *bound == 0 {
                return Err(format!("{at} bounds `{spelling}` at 0; delete it instead").into());
            }
            if !names.contains(spelling.as_str()) && !observed(spelling) {
                return Err(format!(
                    "{at} names `{spelling}`, which is neither a forbid_new_pattern name nor `class:text` of a forbid_new_observed class"
                ).into());
            }
            if !granted.insert((
                row.file.as_str(),
                row.item.as_str(),
                relative.as_str(),
                spelling.as_str(),
            )) {
                return Err(format!("{at} grants `{relative}` `{spelling}` twice").into());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
