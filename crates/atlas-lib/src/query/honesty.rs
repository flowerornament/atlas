//! Two checks that keep a derived row from claiming more than the facts
//! observed. They catch an omission, not a misuse: a rule that reads the
//! unknowns has accounted for them, whether or not it reads them well.
//!
//! - **Basis.** A column that says how a target was chosen (`unique` or
//!   `candidate`) is kept in the head or pinned to a value wherever it is
//!   read. Dropping it turns every candidate into a claim. A head column
//!   filled from a basis column is a basis column itself, so dropping it one
//!   rule later is caught too.
//! - **Absence.** A negated relation whose observations have an unknowns
//!   companion (`call` and `unknown_call`) is read with that companion in
//!   the same rule. Otherwise "no resolved call" reads as "no call", while
//!   an unresolved call could be the one. Companions propagate to derived
//!   relations.
use super::{
    Error, ErrorKind, Origin,
    check::{CHead, CLit, CRule, CTerm, RelId, RelInfo},
    syntax::{Const, Op, Program},
};
use serde::Serialize;
use std::collections::BTreeSet;

/// One of the honesty checks, which a caller may accept by name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum Check {
    /// A resolution basis column dropped.
    Basis,
    /// A negation that does not read the negated relation's unknowns.
    Absence,
}
impl Check {
    pub const ALL: [Self; 2] = [Self::Basis, Self::Absence];
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Basis => "basis",
            Self::Absence => "absence",
        }
    }
}

/// What a relation inherits from the stored relations it is derived from.
#[derive(Clone, Default)]
struct Lineage {
    /// Columns that hold a resolution basis.
    basis: BTreeSet<usize>,
    /// Unknowns companions of the relations it derives from.
    unknowns: BTreeSet<RelId>,
    /// Itself and every relation it derives from.
    ancestry: BTreeSet<RelId>,
}

fn lineage(rels: &[RelInfo], rules: &[CRule]) -> Vec<Lineage> {
    let by_name = |name: &str| rels.iter().position(|r| r.name == name);
    let mut lineage: Vec<Lineage> = rels
        .iter()
        .enumerate()
        .map(|(id, rel)| {
            let mut l = Lineage::default();
            l.ancestry.insert(id);
            if let Some(stored) = rel.stored {
                l.basis
                    .extend(stored.basis.and_then(|b| stored.position(b)));
                l.unknowns.extend(stored.unknowns.and_then(by_name));
            }
            l
        })
        .collect();
    loop {
        let mut changed = false;
        for rule in rules {
            let mut basis_vars = BTreeSet::new();
            let mut unknowns = BTreeSet::new();
            let mut ancestry = BTreeSet::new();
            for literal in &rule.body {
                if let CLit::Pos(rel, terms) = literal {
                    let from = &lineage[*rel];
                    for &c in &from.basis {
                        if let Some(CTerm::Var(v)) = terms.get(c) {
                            basis_vars.insert(*v);
                        }
                    }
                    unknowns.extend(from.unknowns.iter().copied());
                    ancestry.extend(from.ancestry.iter().copied());
                }
            }
            let columns: BTreeSet<usize> = rule
                .terms
                .iter()
                .enumerate()
                .filter(|(_, t)| matches!(t, CHead::Term(CTerm::Var(v)) if basis_vars.contains(v)))
                .map(|(c, _)| c)
                .collect();
            let head = &mut lineage[rule.head];
            let before = (head.basis.len(), head.unknowns.len(), head.ancestry.len());
            head.basis.extend(columns);
            head.unknowns.extend(unknowns);
            head.ancestry.extend(ancestry);
            changed |= before != (head.basis.len(), head.unknowns.len(), head.ancestry.len());
        }
        if !changed {
            return lineage;
        }
    }
}

fn names(rels: &[RelInfo], ids: &BTreeSet<RelId>) -> String {
    ids.iter()
        .map(|&r| format!("`{}`", rels[r].name))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The name a query or rule head goes by in a finding.
fn subject(rels: &[RelInfo], rule: &CRule) -> String {
    let name = &rels[rule.head].name;
    if name.starts_with('?') {
        "this query".into()
    } else {
        format!("`{name}`")
    }
}

/// Every finding of both checks over the rules a program runs.
pub(crate) fn review(
    rels: &[RelInfo],
    rules: &[CRule],
    user: &Program,
    prelude: &Program,
) -> Vec<(Check, Error)> {
    let lineage = lineage(rels, rules);
    let mut findings = Vec::new();
    for rule in rules {
        let program = if rule.origin == Origin::Prelude {
            prelude
        } else {
            user
        };
        let kept: BTreeSet<usize> = rule
            .terms
            .iter()
            .filter_map(|t| match t {
                CHead::Term(CTerm::Var(v)) => Some(*v),
                _ => None,
            })
            .collect();
        let pinned: BTreeSet<usize> = rule
            .body
            .iter()
            .filter_map(|l| match l {
                CLit::Cmp(Op::Eq, CTerm::Var(v), CTerm::Const(Const::Text(_)))
                | CLit::Cmp(Op::Eq, CTerm::Const(Const::Text(_)), CTerm::Var(v)) => Some(*v),
                _ => None,
            })
            .collect();
        for literal in &rule.body {
            let CLit::Pos(rel, terms) = literal else {
                continue;
            };
            for &c in &lineage[*rel].basis {
                let held = match terms.get(c) {
                    Some(CTerm::Const(_)) => true,
                    Some(CTerm::Var(v)) => kept.contains(v) || pinned.contains(v),
                    _ => false,
                };
                if !held {
                    findings.push((
                        Check::Basis,
                        program.error(
                            ErrorKind::Honesty,
                            rule.at,
                            format!(
                                "basis: {} reads `{}` without keeping its `{}` column, so a candidate would read as certain. Keep the column in the head, or pin it (`{}: \"unique\"`)\n    in: {}",
                                subject(rels, rule),
                                rels[*rel].name,
                                rels[*rel].columns[c].0,
                                rels[*rel].columns[c].0,
                                rule.text
                            ),
                        ),
                    ));
                }
            }
        }
        for (i, literal) in rule.body.iter().enumerate() {
            let CLit::Neg(rel, _) = literal else {
                continue;
            };
            let unknowns = &lineage[*rel].unknowns;
            if unknowns.is_empty() {
                continue;
            }
            let accounted = rule.body.iter().enumerate().any(|(j, other)| match other {
                CLit::Pos(r, _) | CLit::Neg(r, _) if j != i => {
                    lineage[*r].ancestry.iter().any(|a| unknowns.contains(a))
                }
                _ => false,
            });
            if !accounted {
                findings.push((
                    Check::Absence,
                    program.error(
                        ErrorKind::Honesty,
                        rule.at,
                        format!(
                            "absence: {} negates `{}`, whose unresolved observations are in {}; no row there is not evidence of none. Read the unknowns in the same rule (for example, a separate outcome for names they could match)\n    in: {}",
                            subject(rels, rule),
                            rels[*rel].name,
                            names(rels, unknowns),
                            rule.text
                        ),
                    ),
                ));
            }
        }
    }
    findings
}

/// Stored relations a program reads whose unknowns companion it never
/// reads: a missing row there is not evidence of absence.
pub(crate) fn unread_unknowns(rels: &[RelInfo], stored: &BTreeSet<RelId>) -> Vec<String> {
    let mut notes = Vec::new();
    for &rel in stored {
        let Some(companion) = rels[rel].stored.and_then(|s| s.unknowns) else {
            continue;
        };
        if stored.iter().any(|&r| rels[r].name == companion) {
            continue;
        }
        notes.push(format!(
            "`{}` has an unknowns relation, `{companion}`, that this program does not read; a row missing from `{}` is not evidence that none exists",
            rels[rel].name, rels[rel].name
        ));
    }
    notes
}
