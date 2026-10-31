//! Select representation contracts from supplied stage declarations and source facts.
//! Declared permissions never suppress observed constructor routes. Instrument calls
//! stay separate from contracts; native presentation lives in render/overview.rs.
use crate::{
    Result,
    architecture::{Architecture, Kind},
    representations::{self, Facts, Query, Role, Selector, Views},
};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
fn success(r: &representations::Reference) -> bool {
    r.role == Role::Output && !r.borrow && !r.wrappers.iter().any(|w| w == "Result[1]")
}
#[derive(Serialize)]
pub struct Overview {
    pub views: Views,
    pub architecture: Architecture,
    pub instruments: Vec<Instrument>,
    pub selection_rule: String,
}
#[derive(Serialize)]
pub struct Instrument {
    pub name: String,
    pub path: String,
    pub calls: Vec<InstrumentCall>,
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct InstrumentCall {
    pub from: String,
    pub to: String,
    pub file: String,
    pub line: usize,
}
/// Entries return the main forms; surface methods enter the picture when they
/// produce a form another stage accepts, or construct the declared entry type.
/// Metadata/error types are not promoted merely because a surface lists them.
pub fn project(facts: &Facts, architecture: &Architecture) -> Result<Overview> {
    let candidates: Vec<_> = facts
        .functions
        .iter()
        .filter_map(|function| {
            let module = architecture.owner(&function.file)?;
            if module.kind != Kind::Stage {
                return None;
            }
            let owner = function.owner_name();
            ((owner.is_none() && function.name == module.entry)
                || (function.public
                    && owner
                        .as_deref()
                        .is_some_and(|o| o == module.entry || module.types.iter().any(|s| s == o))))
            .then_some((function, module))
        })
        .collect();
    let candidate_stages: BTreeMap<_, _> = candidates
        .iter()
        .map(|(function, module)| (function.id.as_str(), *module))
        .collect();
    let mut operations = Vec::new();
    let mut type_ids = BTreeSet::new();
    for (function, module) in candidates {
        let outputs: Vec<_> = facts
            .references
            .iter()
            .filter(|output| output.from == function.id && success(output))
            .collect();
        let primary: Vec<_> = outputs
            .into_iter()
            .filter(|output| {
                let accepted_by_other_stage = facts.references.iter().any(|input| {
                    input.to == output.to
                        && input.role == Role::Input
                        && candidate_stages
                            .get(input.from.as_str())
                            .is_some_and(|other| other.name != module.name)
                });
                let constructs_entry = function.name.ends_with("::new")
                    && facts
                        .types
                        .iter()
                        .any(|ty| ty.id == output.to && ty.name == module.entry);
                function.owner_name().is_none() || accepted_by_other_stage || constructs_entry
            })
            .collect();
        if primary.is_empty() && function.owner_name().is_some() {
            continue;
        }
        operations.push(Selector {
            name: function.name.clone(),
            file: Some(function.file.clone()),
        });
        type_ids.extend(primary.iter().map(|output| output.to.clone()));
    }
    let types: Vec<_> = facts
        .types
        .iter()
        .filter(|ty| type_ids.contains(&ty.id))
        .map(|ty| Selector {
            name: ty.name.clone(),
            file: Some(ty.file.clone()),
        })
        .collect();
    let query = Query {
        title: "Representation contracts from declared stage APIs".into(),
        types,
        apis: operations,
        preparation_files: vec![],
        preparation_functions: vec![],
    };
    let views = representations::project(facts, &query)?;
    let functions: BTreeMap<_, _> = facts
        .functions
        .iter()
        .map(|function| (function.id.as_str(), function))
        .collect();
    let instruments = architecture
        .modules
        .iter()
        .filter(|module| module.kind == Kind::Instrument)
        .map(|module| {
            let calls: BTreeSet<_> = facts
                .calls
                .iter()
                .filter_map(|call| {
                    let (Some(caller), Some(callee)) = (
                        functions.get(call.from.as_str()),
                        functions.get(call.to.as_str()),
                    ) else {
                        return None;
                    };
                    if architecture
                        .owner(&caller.file)
                        .is_some_and(|owner| owner.name == module.name)
                        && architecture
                            .owner(&callee.file)
                            .is_none_or(|owner| owner.name != module.name)
                    {
                        Some(InstrumentCall {
                            from: caller.name.clone(),
                            to: callee.name.clone(),
                            file: call.site.path.clone(),
                            line: call.site.line,
                        })
                    } else {
                        None
                    }
                })
                .collect();
            Instrument {
                name: module.name.clone(),
                path: module.path.clone(),
                calls: calls.into_iter().collect(),
            }
        })
        .collect();
    Ok(Overview {
        views,
        architecture: architecture.clone(),
        instruments,
        selection_rule: concat!(
            "Successful output forms of declared stage entry functions; public surface methods ",
            "producing forms accepted by other stages or constructing an entry type. ",
            "Error alternatives and isolated surface metadata do not become main forms. ",
            "Extra observed literal initializers survive selection. ",
            "Instrument calls are separate observations; no runtime order inferred."
        )
        .into(),
    })
}

pub use crate::render::overview::{ascii, pele};
