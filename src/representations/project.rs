//! Select facts by a saved question; constructors outside the API reach remain visible.
use super::{
    Constructor, Facts, Function, Query, Reference, ResolvedCall, Role, Selector, Type,
    UnknownCall, UnknownType,
};
use crate::Result;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Debug, Serialize)]
pub struct Contract {
    pub function: Function,
    pub inputs: Vec<Reference>,
    pub outputs: Vec<Reference>,
    pub literals: Vec<Constructor>,
    /// Named workspace error forms, even when not selected as main representations.
    pub failures: Vec<(Type, Reference)>,
    pub additional_constructor: bool,
}
#[derive(Clone, Debug, Serialize)]
pub struct Preparation {
    pub function: Function,
    pub outputs: Vec<Reference>,
    /// Source-ordered direct calls returning selected types, including repeated calls.
    pub calls: Vec<(ResolvedCall, Function)>,
}
/// Three native view models. A matrix and comparison rows are not graph nodes.
#[derive(Clone, Debug, Serialize)]
pub struct Views {
    pub query: Query,
    pub types: Vec<Type>,
    pub contracts: Vec<Contract>,
    pub retention: Vec<Reference>,
    pub producers: Vec<Producers>,
    pub preparation: Vec<Preparation>,
    pub unknown_types: Vec<UnknownType>,
    pub unknown_calls: Vec<UnknownCall>,
    pub notes: Vec<String>,
}
fn matches(s: &Selector, name: &str, file: &str) -> bool {
    s.file.as_deref().is_none_or(|f| f == file) && s.name == name
}
fn function<'a>(facts: &'a Facts, selector: &Selector) -> Result<&'a Function> {
    let candidates: Vec<_> = facts
        .functions
        .iter()
        .filter(|f| matches(selector, &f.name, &f.file))
        .collect();
    let [function] = candidates.as_slice() else {
        return Err(format!(
            "function {}: found {} functions; use exact name and file",
            selector.name,
            candidates.len()
        )
        .into());
    };
    Ok(function)
}
/// Resolve every selected name uniquely; an ambiguous name is an error, not a guess.
pub fn project(facts: &Facts, query: &Query) -> Result<Views> {
    if query.types.is_empty() || query.types.len() > 12 {
        return Err(
            "select 1–12 types per figure; use complementary questions for larger populations"
                .into(),
        );
    }
    let mut selected = BTreeMap::new();
    for s in &query.types {
        let found: Vec<_> = facts
            .types
            .iter()
            .filter(|t| matches(s, &t.name, &t.file))
            .collect();
        let [t] = found.as_slice() else {
            return Err(format!(
                "type {}: found {} declarations; qualify its file or inspect source facts",
                s.name,
                found.len()
            )
            .into());
        };
        selected.insert(t.id.clone(), (*t).clone());
    }
    let mut types: Vec<_> = selected.values().cloned().collect();
    types.sort_by(|a, b| (&a.name, &a.file).cmp(&(&b.name, &b.file)));
    let refs: Vec<_> = facts
        .references
        .iter()
        .filter(|r| selected.contains_key(&r.to))
        .cloned()
        .collect();
    let returners: BTreeSet<_> = refs
        .iter()
        .filter(|r| r.role == Role::Output && !r.borrow)
        .map(|r| r.from.as_str())
        .collect();
    let functions: BTreeMap<_, _> = facts.functions.iter().map(|f| (f.id.as_str(), f)).collect();
    let mut operations: BTreeSet<String> = BTreeSet::new();
    if query.apis.is_empty() {
        operations.extend(
            facts
                .functions
                .iter()
                .filter(|f| (f.public || f.trait_impl) && returners.contains(f.id.as_str()))
                .map(|f| f.id.clone()),
        );
    } else {
        for s in &query.apis {
            let f = function(facts, s)?;
            operations.insert(f.id.clone());
        }
    }
    let mut outgoing = BTreeMap::<&str, Vec<&ResolvedCall>>::new();
    for call in &facts.calls {
        outgoing.entry(&call.from).or_default().push(call);
    }
    let mut reached = operations.clone();
    let mut todo: Vec<_> = reached.iter().cloned().collect();
    while let Some(id) = todo.pop() {
        for call in outgoing.get(id.as_str()).into_iter().flatten() {
            if reached.insert(call.to.clone()) {
                todo.push(call.to.clone());
            }
        }
    }
    let additional: BTreeSet<_> = facts
        .constructors
        .iter()
        .filter(|c| selected.contains_key(&c.to) && !reached.contains(&c.from))
        .map(|c| c.from.clone())
        .collect();
    operations.extend(additional.iter().cloned());
    let contracts: Vec<_> = operations
        .iter()
        .map(|id| Contract {
            function: functions[id.as_str()].clone(),
            inputs: refs
                .iter()
                .filter(|r| r.from == *id && r.role == Role::Input)
                .cloned()
                .collect(),
            outputs: refs
                .iter()
                .filter(|r| {
                    r.from == *id
                        && r.role == Role::Output
                        && !r.borrow
                        && !r.wrappers.iter().any(|w| w == "Result[1]")
                })
                .cloned()
                .collect(),
            failures: facts
                .references
                .iter()
                .filter(|r| {
                    r.from == *id
                        && r.role == Role::Output
                        && r.wrappers.iter().any(|w| w == "Result[1]")
                })
                .filter_map(|r| {
                    facts
                        .types
                        .iter()
                        .find(|t| t.id == r.to)
                        .map(|t| (t.clone(), r.clone()))
                })
                .collect(),
            literals: facts
                .constructors
                .iter()
                .filter(|c| c.from == *id && selected.contains_key(&c.to))
                .cloned()
                .collect(),
            additional_constructor: additional.contains(id),
        })
        .collect();
    for selector in &query.preparation_functions {
        let function = function(facts, selector)?;
        if !returners.contains(function.id.as_str()) || !query.prepares(function) {
            return Err(format!("preparation function {} must return a selected type and belong to the selected preparation files", selector.name).into());
        }
    }
    let mut preparation = Vec::new();
    for f in facts
        .functions
        .iter()
        .filter(|f| query.prepares(f) && returners.contains(f.id.as_str()))
    {
        let mut calls: Vec<_> = facts
            .calls
            .iter()
            .filter(|c| c.from == f.id && returners.contains(c.to.as_str()))
            .map(|c| (c.clone(), functions[c.to.as_str()].clone()))
            .collect();
        calls.sort_by_key(|(c, _)| (c.site.line, c.site.column));
        preparation.push(Preparation {
            function: f.clone(),
            outputs: refs
                .iter()
                .filter(|r| r.from == f.id && r.role == Role::Output && !r.borrow)
                .cloned()
                .collect(),
            calls,
        });
    }
    preparation.sort_by(|a, b| {
        (&a.function.file, a.function.line).cmp(&(&b.function.file, b.function.line))
    });
    if contracts.len() > 40 || preparation.len() > 40 {
        return Err("view exceeds 40 API contracts or preparation rows; narrow the saved question (nothing was truncated)".into());
    }
    for file in &query.preparation_files {
        if !facts.fingerprints.iter().any(|(f, _)| f == file) {
            return Err(
                format!("preparation file {file} is outside scanned production source").into(),
            );
        }
    }
    let relevant: BTreeSet<_> = operations
        .iter()
        .map(String::as_str)
        .chain(preparation.iter().map(|p| p.function.id.as_str()))
        .chain(selected.keys().map(String::as_str))
        .map(str::to_owned)
        .collect();
    let mut views = Views {
        query: query.clone(),
        producers: vec![],
        types,
        contracts,
        retention: refs
            .iter()
            .filter(|r| r.role == Role::Field && selected.contains_key(&r.from))
            .cloned()
            .collect(),
        preparation,
        unknown_types: facts
            .unknown_types
            .iter()
            .filter(|u| relevant.contains(u.from.as_str()))
            .cloned()
            .collect(),
        unknown_calls: facts
            .unknown_calls
            .iter()
            .filter(|u| relevant.contains(u.from.as_str()))
            .cloned()
            .collect(),
        notes: facts.limitations.clone(),
    };
    views.producers = views.producers(facts);
    Ok(views)
}

/// Every observed returner and immediate literal initializer for one selected type.
#[derive(Clone, Debug, serde::Serialize)]
pub struct Producers {
    pub representation: Type,
    pub returned: Vec<(Function, Vec<Reference>)>,
    pub constructed: Vec<(Function, Vec<usize>)>,
    pub default_calls: Vec<(Function, Vec<usize>)>,
}
impl Views {
    pub fn producers(&self, facts: &Facts) -> Vec<Producers> {
        let functions: BTreeMap<_, _> =
            facts.functions.iter().map(|f| (f.id.as_str(), f)).collect();
        let mut returns = BTreeMap::<&str, BTreeMap<&str, Vec<Reference>>>::new();
        let mut defaults = BTreeMap::<&str, BTreeMap<&str, Vec<usize>>>::new();
        for c in &facts.default_calls {
            defaults
                .entry(&c.to)
                .or_default()
                .entry(&c.from)
                .or_default()
                .push(c.line);
        }
        let mut literals = BTreeMap::<&str, BTreeMap<&str, Vec<usize>>>::new();
        for r in &facts.references {
            if r.role == Role::Output && !r.borrow {
                returns
                    .entry(&r.to)
                    .or_default()
                    .entry(&r.from)
                    .or_default()
                    .push(r.clone());
            }
        }
        for c in &facts.constructors {
            literals
                .entry(&c.to)
                .or_default()
                .entry(&c.from)
                .or_default()
                .push(c.line);
        }
        self.types
            .iter()
            .map(|t| Producers {
                representation: t.clone(),
                returned: returns
                    .remove(t.id.as_str())
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|(id, refs)| functions.get(id).map(|f| ((*f).clone(), refs)))
                    .collect(),
                default_calls: defaults
                    .remove(t.id.as_str())
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|(id, lines)| functions.get(id).map(|f| ((*f).clone(), lines)))
                    .collect(),
                constructed: literals
                    .remove(t.id.as_str())
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|(id, lines)| functions.get(id).map(|f| ((*f).clone(), lines)))
                    .collect(),
            })
            .collect()
    }
}
