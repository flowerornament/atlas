//! `forbid_new_pattern` and `forbid_new_observed` over a change: held
//! spelling debt outside an authority's home may only fall.
//!
//! The base is counted under the published policy (its patterns and
//! `allowed_in` rows), the candidate under its own. A key is (file, owning
//! item, spelling). Debt that falls is freed only when its item left its
//! file (the file is gone, or no item there is spelled so), and only for
//! the same item and spelling arriving elsewhere: a reduction inside an
//! item that stays pays for nothing, so unrelated items of one name never
//! trade debt. A key whose debt rises draws from that pool; any remainder
//! refuses. A guard removed, or a pattern changed, while the base holds
//! debt under it refuses unless a cited `amended_by` covers it. A new or raised `allowed_in` bound is a raise for
//! the host, refused unless it equals what the candidate uses.
use super::super::policy::{
    AuthorityRule, Census, Key, MissingInput, Occurrence, Policy, Raise, SpellingState,
};
use super::Report;
use crate::discovery::Snapshot;
use std::collections::BTreeMap;

/// One side of the change: its capture, its policy entry for the fact, and
/// what was observed there.
pub(super) struct Side<'a> {
    pub name: &'static str,
    pub snapshot: &'a Snapshot,
    pub inputs: &'a super::super::policy::Inputs,
}

/// The landing's description and its parsed citations, for amendments.
pub(super) struct Citing<'a> {
    pub description: &'a str,
    pub citations: Option<&'a crate::checks::concentration::Citations>,
}

fn writes(rule: &AuthorityRule) -> bool {
    !rule.forbid_new_pattern.is_empty() || !rule.forbid_new_observed.is_empty()
}

/// Judge every authority that writes `forbid_new_*` on either side: one
/// the candidate still guards is counted on both sides; one whose guards
/// the candidate dropped (or whose entry it removed) is held to what the
/// base holds.
pub(super) fn judge(
    (published, policy): (&Policy, &Policy),
    (before, after): (&Side<'_>, &Side<'_>),
    citing: &Citing<'_>,
    out: &mut Report,
) {
    let base_rule = |fact: &str| published.authority.iter().find(|rule| rule.fact == fact);
    for authority in policy.authority.iter().filter(|rule| writes(rule)) {
        judge_guarded(base_rule(&authority.fact), authority, (before, after), out);
        if let Some(published) = base_rule(&authority.fact).filter(|rule| writes(rule)) {
            judge_guards(published, Some(authority), before, citing, out);
        }
    }
    for published in published.authority.iter().filter(|rule| writes(rule)) {
        let now = policy
            .authority
            .iter()
            .find(|rule| rule.fact == published.fact);
        if !now.is_some_and(writes) {
            judge_guards(published, now, before, citing, out);
        }
    }
}

/// The ratchet and raises of an authority the candidate guards.
fn judge_guarded(
    base_rule: Option<&AuthorityRule>,
    authority: &AuthorityRule,
    (before, after): (&Side<'_>, &Side<'_>),
    out: &mut Report,
) {
    let (Some(base), Some(now)) = (
        side_census(before, base_rule.unwrap_or(authority), authority, out),
        side_census(after, authority, authority, out),
    ) else {
        return;
    };
    for (census, side) in [(&base, before), (&now, after)] {
        if census.files == 0 {
            out.violations.push(format!(
                "authority `{}` forbid_new: the {} has no production source; held debt cannot be established from an empty population",
                authority.fact, side.name
            ));
        }
    }
    out.spellings
        .insert(authority.fact.clone(), summary(&base, &now, after.snapshot));
    ratchet(&authority.fact, &base, &now, after.snapshot, out);
    raises(base_rule, authority, &now, out);
}

/// The published guards' law, on the base's count under them.
fn judge_guards(
    published: &AuthorityRule,
    now: Option<&AuthorityRule>,
    before: &Side<'_>,
    citing: &Citing<'_>,
    out: &mut Report,
) {
    if let Some(base) = side_census(before, published, published, out) {
        guards(published, now, &base, citing, out);
    }
}

/// One side's census, or `None` with each missing observation reported.
fn side_census(
    side: &Side<'_>,
    rule: &AuthorityRule,
    names_from: &AuthorityRule,
    out: &mut Report,
) -> Option<Census> {
    let fact = &names_from.fact;
    let mut occurrences = Vec::new();
    let mut complete = true;
    let names = rule
        .forbid_new_pattern
        .iter()
        .map(|p| (p.name.as_str(), &side.inputs.new_patterns))
        .chain(
            rule.forbid_new_observed
                .iter()
                .map(|c| (c.as_str(), &side.inputs.observed)),
        );
    for (name, states) in names {
        let observed = rule.forbid_new_observed.iter().any(|class| class == name);
        let field = format!("delta.{}.authority.{fact}.forbid_new.{name}", side.name);
        match read_state(
            side.snapshot,
            states.get(fact).and_then(|by| by.get(name)),
            observed,
        ) {
            Ok(found) => {
                occurrences.extend(found.into_iter().map(|o| (key_of(name, observed, &o), o)));
            }
            Err(reason) => {
                complete = false;
                out.unavailable.push(MissingInput { field, reason });
            }
        }
    }
    let Some(home) = home_spans(side.snapshot, &rule.home) else {
        out.unavailable.push(MissingInput {
            field: format!("delta.{}.authority.{fact}.home", side.name),
            reason: format!(
                "the home `{}` does not resolve exactly once here",
                rule.home
            ),
        });
        return None;
    };
    complete.then(|| super::super::policy::census(side.snapshot, rule, &home, &occurrences))
}
fn key_of(name: &str, observed: bool, occurrence: &Occurrence) -> String {
    if observed {
        format!("{name}:{}", occurrence.text)
    } else {
        name.to_owned()
    }
}

/// A side's occurrences, checked against its capture: the files read are
/// its production files, each occurrence lies in them, and its text is
/// the bytes there.
fn read_state(
    snapshot: &Snapshot,
    state: Option<&SpellingState>,
    observed: bool,
) -> Result<Vec<Occurrence>, String> {
    let (sources, occurrences) = match state {
        Some(SpellingState::Read {
            sources,
            occurrences,
        }) => (sources, occurrences),
        Some(SpellingState::Unavailable(reason)) => return Err(reason.clone()),
        None if observed => return Err("the wrapper did not observe this class".into()),
        None => return Err("the pattern was not observed".into()),
    };
    let production: BTreeMap<&str, &str> = snapshot
        .graph()
        .files
        .iter()
        .filter(|file| !file.test_only)
        .map(|file| (file.path.as_str(), file.content_fingerprint.as_str()))
        .collect();
    let basis: BTreeMap<&str, &str> = sources
        .iter()
        .map(|(f, p)| (f.as_str(), p.as_str()))
        .collect();
    if basis != production {
        return Err("observation basis differs from the captured population".into());
    }
    for occurrence in occurrences {
        let text = snapshot
            .source(&occurrence.file)
            .and_then(|text| text.get(occurrence.start..occurrence.end));
        if text != Some(occurrence.text.as_str()) {
            return Err(format!(
                "observations escaped the captured population at {}:{}",
                occurrence.file, occurrence.line
            ));
        }
    }
    Ok(occurrences.clone())
}

/// The home's spans: an item home's syntax span, a module home's files.
fn home_spans(
    snapshot: &Snapshot,
    home: &str,
) -> Option<Vec<(String, Option<std::ops::Range<usize>>)>> {
    if let Some((file, item)) = home.split_once("::") {
        let source = snapshot.source_facts(file)?;
        let mut spans = source.outline().iter().filter(|entry| entry.name == item);
        let found = spans.next()?;
        return spans
            .next()
            .is_none()
            .then(|| vec![(file.to_owned(), Some(found.syntax_span.clone()))]);
    }
    let module = home.trim_end_matches('/');
    Some(
        snapshot
            .graph()
            .files
            .iter()
            .filter(|file| {
                crate::architecture::contains(
                    module.trim_end_matches(".rs"),
                    file.path.trim_end_matches(".rs"),
                ) || file.path == module
            })
            .map(|file| (file.path.clone(), None))
            .collect(),
    )
}

fn summary(base: &Census, now: &Census, after: &Snapshot) -> super::SpellingSummary {
    let total = |census: &Census| {
        census
            .keys
            .values()
            .map(super::super::policy::Held::debt)
            .sum()
    };
    let files_with_macros = after
        .graph()
        .files
        .iter()
        .filter(|file| !file.test_only)
        .filter(|file| after.source(&file.path).is_some_and(invokes_macro))
        .count();
    super::SpellingSummary {
        debt_before: total(base),
        debt_after: total(now),
        dropped: now.dropped,
        files_with_macros,
    }
}
/// Whether the text invokes a macro (`name!`), by its tokens.
fn invokes_macro(text: &str) -> bool {
    fn walk(tokens: Vec<proc_macro2::TokenTree>) -> bool {
        tokens.windows(2).any(|pair| {
            matches!((&pair[0], &pair[1]), (proc_macro2::TokenTree::Ident(_), proc_macro2::TokenTree::Punct(p)) if p.as_char() == '!')
        }) || tokens.into_iter().any(|token| match token {
            proc_macro2::TokenTree::Group(group) => walk(group.stream().into_iter().collect()),
            _ => false,
        })
    }
    text.parse::<proc_macro2::TokenStream>()
        .is_ok_and(|stream| walk(stream.into_iter().collect()))
}

/// The held-debt ratchet with the stricter move rule.
fn ratchet(fact: &str, base: &Census, now: &Census, after: &Snapshot, out: &mut Report) {
    let debt = |census: &Census, key: &Key| {
        census
            .keys
            .get(key)
            .map_or(0, super::super::policy::Held::debt)
    };
    let mut pool: BTreeMap<(String, String), usize> = BTreeMap::new();
    for (key, held) in &base.keys {
        let fallen = held.debt().saturating_sub(debt(now, key));
        if fallen > 0 && !item_in_file(after, &key.file, &key.item) {
            *pool
                .entry((key.item.clone(), key.spelling.clone()))
                .or_default() += fallen;
        }
    }
    for (key, held) in &now.keys {
        let before = debt(base, key);
        let mut rise = held.debt().saturating_sub(before);
        let funds = pool
            .entry((key.item.clone(), key.spelling.clone()))
            .or_default();
        let drawn = rise.min(*funds);
        *funds -= drawn;
        rise -= drawn;
        if rise == 0 {
            continue;
        }
        let at = format!("{}:{}", key.file, held.line);
        out.violations.push(if base.keys.contains_key(key) {
            format!(
                "authority `{fact}`: `{}::{}` holds {} `{}` outside its home (base {before}) at {at}; spell it through the home, or remove the growth: an item's count only falls",
                key.file, key.item, held.debt(), key.spelling
            )
        } else {
            format!(
                "authority `{fact}`: `{}::{}` newly holds {} `{}` outside its home at {at}; spell it through the home, or give the item an `allowed_in` row (a count stays with its item, so a renamed, copied or new item starts at 0)",
                key.file, key.item, held.debt(), key.spelling
            )
        });
    }
}
/// Whether the candidate's `file` still has an item spelled `item`.
fn item_in_file(snapshot: &Snapshot, file: &str, item: &str) -> bool {
    let Some(source) = snapshot.source_facts(file) else {
        return false;
    };
    item.is_empty()
        || source.outline().iter().any(|entry| {
            let chain =
                super::super::policy::chain(source, entry.syntax_span.start, entry.syntax_span.end);
            chain
                .iter()
                .map(|i| i.name.as_str())
                .collect::<Vec<_>>()
                .join("::")
                == item
        })
}

/// The published spellings the candidate removed or whose pattern it
/// changed (an entry removed removes them all).
fn changed(published: &AuthorityRule, now: Option<&AuthorityRule>) -> Vec<String> {
    let patterns = published.forbid_new_pattern.iter().filter(|pattern| {
        now.and_then(|now| {
            now.forbid_new_pattern
                .iter()
                .find(|p| p.name == pattern.name)
        })
        .is_none_or(|now| now.pattern != pattern.pattern)
    });
    let classes = published
        .forbid_new_observed
        .iter()
        .filter(|class| now.is_none_or(|now| !now.forbid_new_observed.contains(class)));
    patterns
        .map(|pattern| pattern.name.clone())
        .chain(classes.cloned())
        .collect()
}

/// A published spelling removed, or its pattern changed, while the base
/// holds debt under it refuses, unless the candidate's new `amended_by`
/// covers it and the description cites it. An amendment covering what
/// this landing does not change refuses.
fn guards(
    published: &AuthorityRule,
    now: Option<&AuthorityRule>,
    base: &Census,
    citing: &Citing<'_>,
    out: &mut Report,
) {
    let fact = &published.fact;
    let held = |name: &str| -> usize {
        base.keys
            .iter()
            .filter(|(key, _)| {
                key.spelling
                    .split_once(':')
                    .map_or(key.spelling.as_str(), |(class, _)| class)
                    == name
            })
            .map(|(_, held)| held.debt())
            .sum()
    };
    let changed = changed(published, now);
    let amendment = now
        .and_then(|now| now.amended_by.as_ref())
        .filter(|amendment| published.amended_by.as_ref() != Some(*amendment));
    if let Some(amendment) = amendment {
        let section = format!("authority.{fact}");
        if crate::checks::concentration::known_uncited(
            &section,
            &amendment.bead,
            citing.description,
            citing.citations,
            &mut out.unavailable,
        ) {
            out.violations.push(format!(
                "authority `{fact}`: amended_by names `{}`, which this landing's description does not cite; cite it as ({})",
                amendment.bead, amendment.bead
            ));
        }
        for name in amendment
            .covers
            .iter()
            .filter(|name| !changed.contains(name))
        {
            out.violations.push(format!(
                "authority `{fact}`: amended_by covers `{name}`, which this landing neither changes nor removes; list only what it changes"
            ));
        }
    }
    let covered =
        |name: &String| amendment.is_some_and(|amendment| amendment.covers.contains(name));
    for name in changed.iter().filter(|name| !covered(name)) {
        let n = held(name);
        if n > 0 {
            out.violations.push(format!(
                "authority `{fact}`: forbid_new spelling `{name}` removed or its pattern changed while the base holds {n}; retain its guard (a changed pattern could hide held debt), or name the ruling with `amended_by = {{ bead = \"<reference>\", covers = [\"{name}\"] }}` in this authority and cite it"
            ));
        }
    }
}

/// Each new or raised `allowed_in` bound: a raise, exact or refused.
fn raises(published: Option<&AuthorityRule>, rule: &AuthorityRule, now: &Census, out: &mut Report) {
    for (index, row) in rule.allowed_in.iter().enumerate() {
        let before_row = published.and_then(|p| {
            p.allowed_in
                .iter()
                .find(|r| r.file == row.file && r.item == row.item)
        });
        for (relative, counts) in &row.counts {
            for (spelling, bound) in counts {
                let before = before_row
                    .and_then(|r| r.counts.get(relative))
                    .and_then(|c| c.get(spelling))
                    .copied();
                if before.is_some_and(|before| *bound <= before) {
                    continue;
                }
                let measured = now
                    .used
                    .get(&(index, relative.clone(), spelling.clone()))
                    .copied()
                    .unwrap_or(0);
                out.raises.push(Raise {
                    subject: format!(
                        "authority.{}.allowed_in.{}::{}.{relative}.{spelling}",
                        rule.fact, row.file, row.item
                    ),
                    before,
                    after: *bound,
                    measured,
                    owner: row.owner.clone(),
                });
                if *bound != measured {
                    out.violations.push(format!(
                        "authority `{}`: allowed_in `{}::{}` `{relative}` `{spelling}` raised to {bound}, measured {measured}; set the raise exactly to its measure",
                        rule.fact, row.file, row.item
                    ));
                }
            }
        }
    }
}
