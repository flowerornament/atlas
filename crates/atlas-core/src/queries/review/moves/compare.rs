//! Pair native keys without interpreting their effective visibility or behavior.
//! Exact bodies cancel before rerooted/remaining bodies; repeated keys remain approximate.
use super::{
    Ambiguity, Location,
    inventory::{Body, Inventory, Key, Site},
    scope::Scope,
};
use std::collections::BTreeSet;

/// One finding: a key whose two sides differ, with both sites.
pub(super) struct Change<'a> {
    pub(super) key: &'a Key,
    pub(super) before: &'a Site,
    pub(super) after: &'a Site,
    /// Several sites share the key on a side, so the pairing is a guess.
    pub(super) ambiguous: bool,
}

/// The direction of a visibility change along the ladder. Two `pub(in ..)`
/// scopes compare by path ancestry; unrelated paths are re-scoped.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Direction {
    Widened,
    Narrowed,
    Rescoped,
}

impl Direction {
    pub(super) fn of(before: &Site, after: &Site) -> Self {
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
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::Widened => "widened",
            Self::Narrowed => "narrowed",
            Self::Rescoped => "rescoped",
        }
    }
}

/// Every visibility change: per key, equal visibilities cancel first, then
/// the rest pair in ladder order; unpaired sites are additions or removals.
pub(super) fn visibility_changes<'a>(
    before: &'a Inventory,
    after: &'a Inventory,
) -> Vec<Change<'a>> {
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
pub(super) fn ambiguities(before: &Inventory, after: &Inventory, scope: &Scope) -> Vec<Ambiguity> {
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
pub(super) struct BodyReport<'a> {
    pub(super) exact: usize,
    /// Of the exact and rerooted pairs, those whose file changed.
    pub(super) relocated: usize,
    pub(super) rerooted: Vec<Change<'a>>,
    pub(super) changed: Vec<Change<'a>>,
    pub(super) only_before: Vec<(&'a Key, &'a Site)>,
    pub(super) only_after: Vec<(&'a Key, &'a Site)>,
    pub(super) unreadable: Vec<(&'a Key, &'a Site)>,
}

impl<'a> BodyReport<'a> {
    /// Per key, pair exact bodies first, then rerooted ones, then the rest in
    /// order as changed. Pairing spans the matched crates; a finding is
    /// reported when its current site (or, for a removal, its old site) lies
    /// in scope.
    pub(super) fn between(before: &'a Inventory, after: &'a Inventory, scope: &Scope) -> Self {
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
