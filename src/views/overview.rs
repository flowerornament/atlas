//! Select representation contracts from supplied stage declarations and source facts.
//! Declared permissions never suppress observed constructor routes. Instrument calls
//! stay separate from contracts; native presentation lives in render/overview.rs.
//! A population larger than one figure is divided into panels by declaring module;
//! every selected type appears, and nothing is truncated.
use crate::{
    Result,
    architecture::{Architecture, Kind, Module},
    types::{self, Facts, Function, MAX_ROWS, MAX_TYPES, Query, Role, Selector, Views},
};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
fn success(r: &types::Reference) -> bool {
    r.role == Role::Output && !r.borrow && !r.wrappers.iter().any(|w| w == "Result[1]")
}
#[derive(Serialize)]
pub struct Overview {
    /// Contract figures in declared module order. One panel when the selected
    /// population fits a figure.
    pub panels: Vec<Panel>,
    pub architecture: Architecture,
    pub instruments: Vec<Instrument>,
    pub selection_rule: String,
    /// How the selected types were divided among panels.
    pub split: Vec<Group>,
}
/// One contract figure: the types declared by `modules`, with every selected
/// operation that takes or returns one of them, wherever it is declared.
#[derive(Serialize)]
pub struct Panel {
    pub modules: Vec<String>,
    pub views: Views,
}
/// Selected types declared in one module (`None`: outside every declared module).
#[derive(Serialize)]
pub struct Group {
    pub module: Option<String>,
    pub types: usize,
    /// Index into `panels`.
    pub panel: usize,
}
const OUTSIDE: &str = "outside declared modules";
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
    let candidates = stage_surfaces(facts, architecture);
    let (operations, type_ids) = main_forms(facts, &candidates);
    let (panels, split) = panels(facts, architecture, &operations, &type_ids)?;
    let instruments = instruments(facts, architecture);
    Ok(Overview {
        panels,
        split,
        architecture: architecture.clone(),
        instruments,
        selection_rule: concat!(
            "Successful output forms of declared stage entry functions; public surface methods ",
            "producing forms accepted by other stages or constructing an entry type. ",
            "Error alternatives and isolated surface metadata do not become main forms. ",
            "Extra observed literal initializers survive selection. ",
            "Above 12 types or 40 API contracts, panels group whole declaring modules in declared order; ",
            "each panel also shows every selected operation taking or returning its types. ",
            "Instrument calls are separate observations; no runtime order inferred."
        )
        .into(),
    })
}

/// Functions in a declared stage that are its free entry function, or a public
/// method of its entry type or of a type its surface lists.
fn stage_surfaces<'a>(
    facts: &'a Facts,
    architecture: &'a Architecture,
) -> Vec<(&'a Function, &'a Module)> {
    facts
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
        .collect()
}

/// The selected operations and the types of their main forms. An entry
/// function keeps all its successful outputs; a surface method is kept only
/// for an output another stage accepts or one that constructs the entry type.
fn main_forms<'a>(
    facts: &Facts,
    candidates: &[(&'a Function, &Module)],
) -> (Vec<&'a Function>, BTreeSet<String>) {
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
        operations.push(*function);
        type_ids.extend(primary.iter().map(|output| output.to.clone()));
    }
    (operations, type_ids)
}

/// For each declared instrument, the observed direct calls from inside it to
/// functions outside it.
fn instruments(facts: &Facts, architecture: &Architecture) -> Vec<Instrument> {
    let functions: BTreeMap<_, _> = facts
        .functions
        .iter()
        .map(|function| (function.id.as_str(), function))
        .collect();
    architecture
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
        .collect()
}

/// Divide the selected types into panels within both figure limits,
/// [`MAX_TYPES`] types and [`MAX_ROWS`] API contracts, keeping each
/// declaring module's types together. A module that alone exceeds a limit
/// refuses rather than being split, and nothing is dropped.
fn panels(
    facts: &Facts,
    architecture: &Architecture,
    operations: &[&Function],
    type_ids: &BTreeSet<String>,
) -> Result<(Vec<Panel>, Vec<Group>)> {
    if type_ids.is_empty() {
        return Err("overview: the declared stage APIs return no workspace types to draw".into());
    }
    let figures = Figures::new(facts, architecture, operations, type_ids);
    let packed = figures.pack()?;
    let count = packed.len();
    let mut panels = Vec::new();
    let mut split = Vec::new();
    for (number, members) in packed.iter().enumerate() {
        let modules: Vec<String> = members
            .iter()
            .map(|&g| {
                figures.groups[g]
                    .0
                    .clone()
                    .unwrap_or_else(|| OUTSIDE.into())
            })
            .collect();
        let title = if count == 1 {
            "Representation contracts from declared stage APIs".to_owned()
        } else {
            format!(
                "Representation contracts from declared stage APIs, panel {} of {count}: {}",
                number + 1,
                modules.join(", ")
            )
        };
        for &g in members {
            split.push(Group {
                module: figures.groups[g].0.clone(),
                types: figures.groups[g].1.len(),
                panel: number,
            });
        }
        panels.push(Panel {
            modules,
            views: figures.select(members, title)?,
        });
    }
    Ok((panels, split))
}

/// The selected types grouped by declaring module, and the operations a
/// figure over some of those groups draws.
struct Figures<'a> {
    facts: &'a Facts,
    architecture: &'a Architecture,
    operations: &'a [&'a Function],
    /// Declared module order, then types declared outside every module;
    /// only groups with a selected type.
    groups: Vec<(Option<String>, Vec<&'a types::Type>)>,
    all: BTreeSet<&'a str>,
}

impl<'a> Figures<'a> {
    fn new(
        facts: &'a Facts,
        architecture: &'a Architecture,
        operations: &'a [&'a Function],
        type_ids: &'a BTreeSet<String>,
    ) -> Self {
        let mut groups: Vec<(Option<String>, Vec<&types::Type>)> = architecture
            .modules
            .iter()
            .map(|m| (Some(m.name.clone()), vec![]))
            .chain([(None, vec![])])
            .collect();
        for ty in facts.types.iter().filter(|ty| type_ids.contains(&ty.id)) {
            let module = module_of(architecture, &ty.file);
            if let Some((_, types)) = groups.iter_mut().find(|(name, _)| *name == module) {
                types.push(ty);
            }
        }
        groups.retain(|(_, types)| !types.is_empty());
        Self {
            facts,
            architecture,
            operations,
            groups,
            all: type_ids.iter().map(String::as_str).collect(),
        }
    }

    fn touches(&self, op: &Function, ids: &BTreeSet<&str>) -> bool {
        self.facts
            .references
            .iter()
            .any(|r| r.from == op.id && ids.contains(r.to.as_str()))
    }

    /// An operation touching no selected type is drawn with its own module's
    /// group, or with the first group; it is never left out.
    fn home(&self, op: &Function) -> usize {
        let module = module_of(self.architecture, &op.file);
        self.groups
            .iter()
            .position(|(name, _)| *name == module)
            .unwrap_or(0)
    }

    /// The figure over the `members` groups: their types and every operation
    /// that touches one of them or makes its home there.
    fn select(&self, members: &[usize], title: String) -> Result<Views> {
        let types: Vec<_> = members
            .iter()
            .flat_map(|&g| self.groups[g].1.iter().copied())
            .collect();
        let ids: BTreeSet<&str> = types.iter().map(|t| t.id.as_str()).collect();
        let apis = self
            .operations
            .iter()
            .filter(|op| {
                self.touches(op, &ids)
                    || (!self.touches(op, &self.all) && members.contains(&self.home(op)))
            })
            .map(|op| Selector {
                name: op.name.clone(),
                file: Some(op.file.clone()),
            })
            .collect();
        types::select(
            self.facts,
            &Query {
                title,
                types: types
                    .iter()
                    .map(|ty| Selector {
                        name: ty.name.clone(),
                        file: Some(ty.file.clone()),
                    })
                    .collect(),
                apis,
                preparation_files: vec![],
                preparation_functions: vec![],
            },
        )
    }

    /// Pack whole modules, in order, into as few panels as fit both limits.
    fn pack(&self) -> Result<Vec<Vec<usize>>> {
        let fits =
            |views: &Views| views.types.len() <= MAX_TYPES && views.contracts.len() <= MAX_ROWS;
        let mut packed: Vec<Vec<usize>> = vec![];
        let mut current: Vec<usize> = vec![];
        for (index, (module, _)) in self.groups.iter().enumerate() {
            let mut trial = current.clone();
            trial.push(index);
            if fits(&self.select(&trial, String::new())?) {
                current = trial;
                continue;
            }
            let alone = self.select(&[index], String::new())?;
            if !fits(&alone) {
                return Err(format!(
                    "overview: module `{}` alone selects {} types and {} API contracts; one figure holds at most {MAX_TYPES} types and {MAX_ROWS} contracts, and Atlas does not split a module or drop either",
                    module.as_deref().unwrap_or(OUTSIDE),
                    alone.types.len(),
                    alone.contracts.len()
                )
                .into());
            }
            packed.push(std::mem::replace(&mut current, vec![index]));
        }
        packed.push(current);
        Ok(packed)
    }
}

fn module_of(architecture: &Architecture, file: &str) -> Option<String> {
    architecture.owner(file).map(|m| m.name.clone())
}

pub use crate::render::overview::{ascii, pele};
