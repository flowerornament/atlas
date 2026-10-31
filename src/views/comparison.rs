//! Compare source observations by file/symbol and role, excluding line-number churn.
use crate::{
    Result,
    representations::{Facts, Query, Role},
};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
#[derive(Clone, Debug, Serialize)]
pub struct Change {
    pub category: String,
    pub subject: String,
    pub before: Option<String>,
    pub after: Option<String>,
}
#[derive(Debug, Serialize)]
pub struct Comparison {
    pub before: String,
    pub after: String,
    pub changes: Vec<Change>,
    pub notes: Vec<String>,
}
fn name(s: &str) -> String {
    let mut depth = 0;
    let mut out = String::new();
    for c in s.chars() {
        match c {
            '<' => depth += 1,
            '>' => depth -= 1,
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out
}
fn observations(f: &Facts, query: Option<&Query>) -> Result<BTreeMap<(String, String), String>> {
    let types: BTreeMap<_, _> = f
        .types
        .iter()
        .map(|t| {
            (
                t.id.as_str(),
                format!(
                    "{}::{}",
                    t.file,
                    if t.qualified.is_empty() {
                        &t.name
                    } else {
                        &t.qualified
                    }
                ),
            )
        })
        .collect();
    let functions: BTreeMap<_, _> = f
        .functions
        .iter()
        .map(|x| {
            (
                x.id.as_str(),
                format!(
                    "{}::{}",
                    x.file,
                    name(if x.qualified.is_empty() {
                        &x.name
                    } else {
                        &x.qualified
                    })
                ),
            )
        })
        .collect();
    let selected: BTreeSet<_> = f
        .types
        .iter()
        .filter(|t| {
            query.is_none_or(|q| {
                q.types
                    .iter()
                    .any(|s| s.name == t.name && s.file.as_deref().is_none_or(|p| p == t.file))
            })
        })
        .map(|t| t.id.as_str())
        .collect();
    // Only identities that contribute observations to this question must match.
    // Unrelated cfg alternatives are not a reason to discard a selected view.
    let related_functions: BTreeSet<_> = f
        .references
        .iter()
        .filter(|reference| {
            selected.contains(reference.to.as_str())
                && matches!(reference.role, Role::Input | Role::Output)
        })
        .map(|reference| reference.from.as_str())
        .chain(
            f.constructors
                .iter()
                .chain(&f.default_calls)
                .filter(|constructor| selected.contains(constructor.to.as_str()))
                .map(|constructor| constructor.from.as_str()),
        )
        .collect();
    let mut identities = BTreeSet::new();
    for key in types
        .iter()
        .filter(|(id, _)| selected.contains(*id))
        .map(|(_, key)| key)
        .chain(
            functions
                .iter()
                .filter(|(id, _)| related_functions.contains(*id))
                .map(|(_, key)| key),
        )
    {
        if !identities.insert(key) {
            return Err(format!(
                "ambiguous stable source symbol {key}; no arbitrary comparison match is permitted"
            )
            .into());
        }
    }
    let mut out = BTreeMap::new();
    for t in f.types.iter().filter(|t| selected.contains(t.id.as_str())) {
        out.insert(
            ("type".into(), types[t.id.as_str()].clone()),
            if t.alias.is_some() {
                t.declaration()
            } else if t.fields.is_empty() {
                "(no direct fields)".into()
            } else {
                t.fields
                    .iter()
                    .map(|(name, ty)| format!("{name}: {ty}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            },
        );
    }
    for r in f.references.iter().filter(|r| {
        selected.contains(r.to.as_str()) && matches!(r.role, Role::Input | Role::Output)
    }) {
        let Some(from) = functions.get(r.from.as_str()) else {
            continue;
        };
        let key = format!(
            "{from} {:?} {} {:?} → {}",
            r.role,
            r.slot,
            r.wrappers,
            types[r.to.as_str()]
        );
        out.insert(
            ("contract".into(), key),
            format!("{} | {:?}", r.expression, r.wrappers),
        );
    }
    for r in f
        .references
        .iter()
        .filter(|r| r.role == Role::Field && selected.contains(r.from.as_str()))
    {
        let Some(from) = types.get(r.from.as_str()) else {
            continue;
        };
        out.insert(
            ("field".into(), format!("{from}.{}", r.slot)),
            r.expression.clone(),
        );
    }
    let mut sites = BTreeMap::<String, usize>::new();
    for c in f
        .constructors
        .iter()
        .filter(|c| selected.contains(c.to.as_str()))
    {
        if let Some(from) = functions.get(c.from.as_str()) {
            *sites
                .entry(format!("{from} constructs {}", types[c.to.as_str()]))
                .or_default() += 1;
        }
    }
    for c in f
        .default_calls
        .iter()
        .filter(|c| selected.contains(c.to.as_str()))
    {
        if let Some(from) = functions.get(c.from.as_str()) {
            *sites
                .entry(format!(
                    "{from} explicitly calls {}::default",
                    types[c.to.as_str()]
                ))
                .or_default() += 1;
        }
    }
    for (key, count) in sites {
        out.insert(("constructor".into(), key), count.to_string());
    }
    let returners: BTreeSet<_> = f
        .references
        .iter()
        .filter(|r| r.role == Role::Output && !r.borrow && selected.contains(r.to.as_str()))
        .map(|r| r.from.as_str())
        .collect();
    let mut outgoing = BTreeMap::<&str, Vec<String>>::new();
    for c in &f.calls {
        if returners.contains(c.to.as_str())
            && let Some(to) = functions.get(c.to.as_str())
        {
            outgoing.entry(&c.from).or_default().push(to.clone());
        }
    }
    for function in f
        .functions
        .iter()
        .filter(|x| returners.contains(x.id.as_str()) && query.is_none_or(|q| q.prepares(x)))
    {
        let calls = outgoing
            .get(function.id.as_str())
            .cloned()
            .unwrap_or_default();
        out.insert(
            (
                "preparation".into(),
                functions[function.id.as_str()].clone(),
            ),
            calls.join(" → "),
        );
    }
    Ok(out)
}
/// No rename inference: file/symbol moves are additions/removals, and alias expansion/unknowns remain limits.
pub fn compare(
    before: &Facts,
    after: &Facts,
    query: Option<&Query>,
    labels: (String, String),
) -> Result<Comparison> {
    let a = observations(before, query)?;
    let b = observations(after, query)?;
    let mut changes = Vec::new();
    let keys: BTreeSet<_> = a.keys().chain(b.keys()).cloned().collect();
    for (kind, key) in keys {
        let old = a.get(&(kind.clone(), key.clone()));
        let new = b.get(&(kind.clone(), key.clone()));
        if old != new {
            changes.push(Change {
                category: kind,
                subject: key,
                before: old.cloned(),
                after: new.cloned(),
            });
        }
    }
    Ok(Comparison {
        before: labels.0,
        after: labels.1,
        changes,
        notes: vec![
            "Keys are physical file + source symbol, not line number. Moves/renames are reported as removal/addition; ambiguous keys refuse.".into(),
            "Contracts, field syntax, literal-site counts and direct producer-call sequences are separate source observations, not semantic equivalence or runtime value flow.".into(),
            format!(
                "Unknown observations: before {} types / {} calls; after {} types / {} calls. No missing-edge claim is made.",
                before.unknown_types.len(), before.unknown_calls.len(),
                after.unknown_types.len(), after.unknown_calls.len()
            ),
        ],
    })
}
impl Comparison {
    pub fn text(&self) -> String {
        let mut out = format!(
            "{} → {}\n{} changed source observations\n",
            self.before,
            self.after,
            self.changes.len()
        );
        for c in &self.changes {
            write!(
                out,
                "\n[{}] {}\n  before: {}\n  after: {}\n",
                c.category,
                c.subject,
                c.before.as_deref().unwrap_or("absent"),
                c.after.as_deref().unwrap_or("absent")
            )
            .expect("formatting into a String cannot fail");
        }
        for n in &self.notes {
            write!(out, "\n{n}").expect("formatting into a String cannot fail");
        }
        out.push('\n');
        out
    }
    /// Aligned source-observation rows; no graph edges or behavioral summary invented.
    pub fn pele(&self) -> crate::Result<crate::render::Artifact> {
        crate::render::comparison::pele(self)
    }
}
