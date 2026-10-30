//! The writers census: who builds or changes a boundary type listed whole.
//!
//! A `writers` row names a type, its stage and its producers. The census
//! derives the type's writer methods from the type itself: every method of
//! an `impl` of it in its stage that takes `&mut self` or returns the type
//! (constructors, `Default`, builders), plus any methods and free functions
//! the row adds. Calls from any other owner are hidden debt the listing would
//! otherwise bless, counted by spelling over production, test and mounted
//! code: `.m(`, `Ty::m(`, `<Ty>::m(`, and the same through each type alias
//! or `use … as` rename of the type in the crate, and `f(` for functions.

use std::collections::{BTreeMap, BTreeSet};

use super::super::super::{CrateMap, FileMap, ItemKind, module_path_of};
use super::Site;
use crate::checks::policy;
use crate::scan;

/// One call outside the producers: its site (`path` is the spelling) and
/// the call's line, by which a call is known across revisions.
pub(super) struct WriterCall {
    pub(super) site: Site,
    pub(super) code: String,
}

/// One writer type's census.
#[derive(Default)]
pub(super) struct WriterCensus {
    /// Whether the row's stage declares the type.
    pub(super) declared: bool,
    /// The writer methods: derived from the type, with the row's additions.
    pub(super) methods: BTreeSet<String>,
    /// Other names the crate gives the type (aliases and renames).
    pub(super) aliases: BTreeSet<String>,
    /// Per calling owner outside the producers: the calls.
    pub(super) calls: BTreeMap<String, Vec<WriterCall>>,
}

/// The census of each writers row over a crate.
pub(super) fn census(
    crate_map: &CrateMap,
    writers: &BTreeMap<String, policy::Writers>,
) -> BTreeMap<String, WriterCensus> {
    let files = owned_files(crate_map);
    writers
        .iter()
        .map(|(ty, row)| (ty.clone(), census_of(ty, row, &files)))
        .collect()
}

/// Every file of the crate with its owner (top-level module), mounted
/// files included.
fn owned_files(crate_map: &CrateMap) -> Vec<(String, &FileMap)> {
    crate_map
        .files
        .iter()
        .filter_map(|file| {
            module_path_of(&crate_map.src_rel, &file.rel).map(|module| (head(&module), file))
        })
        .chain(
            crate_map
                .mounted
                .iter()
                .map(|mount| (head(&mount.module), &mount.file)),
        )
        .collect()
}

fn head(module: &str) -> String {
    module.split("::").next().unwrap_or_default().to_owned()
}

fn census_of(ty: &str, row: &policy::Writers, files: &[(String, &FileMap)]) -> WriterCensus {
    let mut census = WriterCensus {
        methods: row.methods.iter().cloned().collect(),
        ..WriterCensus::default()
    };
    // A struct of the stage that holds the type (`CompactResult { arena }`)
    // carries a new one out of the method that returns it.
    let mut produced: BTreeSet<String> = BTreeSet::from(["Self".to_owned(), ty.to_owned()]);
    for (_, file) in files.iter().filter(|(owner, _)| *owner == row.stage) {
        produced.extend(
            file.source
                .outline()
                .iter()
                .filter(|item| item.kind == ItemKind::Struct && item.name != ty)
                .filter(|item| scan::identifiers(&item.declaration_block).any(|name| name == ty))
                .map(|item| item.name.clone()),
        );
    }
    for (owner, file) in files {
        if *owner == row.stage {
            derive_methods(ty, &produced, file, &mut census);
        }
        census.aliases.extend(aliases(ty, file));
    }
    let names: Vec<&str> = std::iter::once(ty)
        .chain(census.aliases.iter().map(String::as_str))
        .collect();
    let spellings: Vec<String> = census
        .methods
        .iter()
        .flat_map(|method| {
            std::iter::once(format!(".{method}(")).chain(names.iter().flat_map(move |name| {
                [format!("{name}::{method}("), format!("<{name}>::{method}(")]
            }))
        })
        .chain(row.functions.iter().map(|function| format!("{function}(")))
        .collect();
    for (owner, file) in files {
        if row.producers.contains(owner) {
            continue;
        }
        for (index, code) in file.source.projection().code_lines().enumerate() {
            let Some(spelling) = spellings.iter().find(|spelling| called(code, spelling)) else {
                continue;
            };
            census
                .calls
                .entry(owner.clone())
                .or_default()
                .push(WriterCall {
                    site: Site {
                        file: file.rel.clone(),
                        line: index + 1,
                        path: spelling.clone(),
                    },
                    code: code.trim().to_owned(),
                });
        }
    }
    census
}

/// The type's declaration and the writer methods of its `impl` blocks in
/// one file of its stage: each takes `&mut self` or returns a `produced`
/// type (the type itself, or a struct of the stage carrying one).
fn derive_methods(
    ty: &str,
    produced: &BTreeSet<String>,
    file: &FileMap,
    census: &mut WriterCensus,
) {
    let impls: Vec<_> = file
        .source
        .outline()
        .iter()
        .filter(|item| item.top_level && item.name == ty)
        .inspect(|item| {
            if matches!(
                item.kind,
                ItemKind::Struct | ItemKind::Enum | ItemKind::Union
            ) {
                census.declared = true;
            }
        })
        .filter(|item| item.kind == ItemKind::Impl)
        .map(|item| item.span.clone())
        .collect();
    for method in file.source.outline().iter().filter(|item| {
        item.kind == ItemKind::Fn
            && !item.top_level
            && impls.iter().any(|span| span.contains(&item.line))
    }) {
        let signature = method
            .declaration_block
            .split('{')
            .next()
            .unwrap_or_default();
        let returns = signature.split_once("->").map(|(_, ret)| ret);
        let returns_the_type =
            returns.is_some_and(|ret| scan::identifiers(ret).any(|name| produced.contains(name)));
        if signature.contains("&mut self") || returns_the_type {
            census.methods.insert(method.name.clone());
        }
    }
}

/// Names `file` gives the type: `type A = …Ty;` and `use …::Ty as A;`.
fn aliases(ty: &str, file: &FileMap) -> Vec<String> {
    let names_ty = |source: &str| {
        source
            .trim()
            .trim_end_matches(';')
            .rsplit("::")
            .next()
            .map(str::trim)
            == Some(ty)
    };
    let mut found = Vec::new();
    for item in file.source.outline() {
        match item.kind {
            ItemKind::TypeAlias if item.name != ty => {
                if item
                    .declaration_block
                    .split_once('=')
                    .is_some_and(|(_, rhs)| names_ty(rhs))
                {
                    found.push(item.name.clone());
                }
            }
            ItemKind::Use | ItemKind::PubUse => {
                let Some(import) = file.source.import_at(item.line) else {
                    continue;
                };
                let Ok(bindings) = &import.bindings else {
                    continue;
                };
                found.extend(
                    bindings
                        .iter()
                        .filter(|binding| {
                            binding.local_name != ty && names_ty(&binding.source_path)
                        })
                        .map(|binding| binding.local_name.clone()),
                );
            }
            _ => {}
        }
    }
    found
}

/// Whether `code` calls `spelling`, with a word boundary before a name
/// spelling (`remap_term(` is not `do_remap_term(`).
fn called(code: &str, spelling: &str) -> bool {
    code.match_indices(spelling).any(|(at, _)| {
        spelling.starts_with(['.', '<'])
            || code[..at]
                .chars()
                .next_back()
                .is_none_or(|ch| !(ch == '_' || ch.is_ascii_alphanumeric()))
    })
}
