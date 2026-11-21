//! An authority's home, resolved once for every rule that reads it: the
//! existence check, `allowed_readers` and `forbid_new_*`.
//!
//! A home is an item (`FILE::ITEM`) or a module: a file, or a directory for
//! the module rooted there. An item that is a `mod` declaration is its
//! module, as a directory home is. Module membership comes from the index's
//! file scopes, so `#[path]` mounts are followed; a file mounted both
//! inside and outside the home makes it unresolved.
use crate::{
    discovery::Snapshot,
    population::TestSelection,
    source::{Item, ItemKind, Source},
};
use std::collections::{BTreeMap, BTreeSet};

/// Each production file's module scopes, as the index reads them.
pub(in crate::checks) type Scopes = BTreeMap<String, Vec<Vec<String>>>;

/// A resolved home.
pub(in crate::checks) struct Home {
    /// The home as a module path (`crate::front::parse`, or an item's).
    pub path: Vec<String>,
    /// Files wholly the home's: a module home's files.
    pub files: BTreeSet<String>,
    /// An item home's file and declaration span.
    pub item: Option<(String, std::ops::Range<usize>)>,
}
impl Home {
    /// The files no reader of the home can be: its own, and an item home's.
    pub(in crate::checks) fn own_files(&self) -> BTreeSet<String> {
        let mut files = self.files.clone();
        files.extend(self.item.iter().map(|(file, _)| file.clone()));
        files
    }
    /// Where the home's bytes are: a whole file, or an item's span.
    pub(in crate::checks) fn spans(&self) -> Vec<(String, Option<std::ops::Range<usize>>)> {
        let files = self.files.iter().map(|file| (file.clone(), None));
        let item = self
            .item
            .iter()
            .map(|(file, span)| (file.clone(), Some(span.clone())));
        files.chain(item).collect()
    }
}

/// Why a home names no one production module or item.
pub(in crate::checks) enum HomeUnresolved {
    NotMounted { file: String },
    MountedAsSeveral { file: String, modules: usize },
    NoModuleRoot { home: String },
    MountedBothWays { file: String, home: String },
    NotOneItem { home: String, found: usize },
}
impl std::fmt::Display for HomeUnresolved {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotMounted { file } => write!(f, "`{file}` is not mounted in production code"),
            Self::MountedAsSeveral { file, modules } => {
                write!(f, "`{file}` is mounted as {modules} modules; a home is one")
            }
            Self::NoModuleRoot { home } => write!(f, "no production module is rooted at `{home}`"),
            Self::MountedBothWays { file, home } => write!(
                f,
                "`{file}` is mounted both inside and outside the home `{home}`; mount it once"
            ),
            Self::NotOneItem { home, found } => {
                write!(
                    f,
                    "the home `{home}` names {found} production items; a home is one"
                )
            }
        }
    }
}

/// The production items an item home `name` in `source` can be: one
/// declared there by that name, of a kind a home may be.
pub(in crate::checks) fn home_items<'a>(source: &'a Source, name: &str) -> Vec<&'a Item> {
    source
        .outline()
        .iter()
        .filter(|entry| {
            home_kind(entry.kind)
                && entry.name == name
                && TestSelection::Production.admits_line(source, entry.line)
        })
        .collect()
}
const fn home_kind(kind: ItemKind) -> bool {
    matches!(
        kind,
        ItemKind::Struct
            | ItemKind::Enum
            | ItemKind::Union
            | ItemKind::Fn
            | ItemKind::Trait
            | ItemKind::TypeAlias
            | ItemKind::Const
            | ItemKind::Static
            | ItemKind::Mod
    )
}

/// Resolve `home` in `snapshot`, whose production files have `scopes`.
pub(in crate::checks) fn resolve(
    snapshot: &Snapshot,
    scopes: &Scopes,
    home: &str,
) -> Result<Home, HomeUnresolved> {
    let Some((file, item)) = home.split_once("::") else {
        let file = module_file(home, scopes)?;
        return module(home, one_scope(&file, scopes)?, scopes);
    };
    let mut path = one_scope(file, scopes)?;
    path.push(item.to_owned());
    let items = snapshot
        .source_facts(file)
        .map(|source| home_items(source, item))
        .unwrap_or_default();
    let [found] = items.as_slice() else {
        return Err(HomeUnresolved::NotOneItem {
            home: home.to_owned(),
            found: items.len(),
        });
    };
    // A `mod` item is its module: the files mounted inside it are the home's.
    let files = if found.kind == ItemKind::Mod {
        module(home, path.clone(), scopes)?.files
    } else {
        BTreeSet::new()
    };
    Ok(Home {
        path,
        files,
        item: Some((file.to_owned(), found.syntax_span.clone())),
    })
}
/// A module home at `path`: every file mounted only inside it.
fn module(home: &str, path: Vec<String>, scopes: &Scopes) -> Result<Home, HomeUnresolved> {
    let inside = |scope: &Vec<String>| scope.starts_with(&path);
    let mut files = BTreeSet::new();
    for (file, scopes) in scopes {
        match (scopes.iter().all(inside), scopes.iter().any(inside)) {
            (true, _) if !scopes.is_empty() => {
                files.insert(file.clone());
            }
            (false, true) => {
                return Err(HomeUnresolved::MountedBothWays {
                    file: file.clone(),
                    home: home.to_owned(),
                });
            }
            _ => {}
        }
    }
    Ok(Home {
        path,
        files,
        item: None,
    })
}
/// The file of a module home: the file itself, or a directory's module
/// root (`dir/mod.rs` or `dir.rs`).
fn module_file(home: &str, scopes: &Scopes) -> Result<String, HomeUnresolved> {
    if std::path::Path::new(home)
        .extension()
        .is_some_and(|ext| ext == "rs")
    {
        return Ok(home.to_owned());
    }
    let directory = home.trim_end_matches('/');
    [format!("{directory}/mod.rs"), format!("{directory}.rs")]
        .into_iter()
        .find(|file| scopes.contains_key(file))
        .ok_or_else(|| HomeUnresolved::NoModuleRoot {
            home: home.to_owned(),
        })
}
/// The one production module path of `file`.
fn one_scope(file: &str, scopes: &Scopes) -> Result<Vec<String>, HomeUnresolved> {
    let mut found: Vec<&Vec<String>> = scopes.get(file).into_iter().flatten().collect();
    found.dedup();
    match found.as_slice() {
        [one] => Ok((*one).clone()),
        [] => Err(HomeUnresolved::NotMounted {
            file: file.to_owned(),
        }),
        many => Err(HomeUnresolved::MountedAsSeveral {
            file: file.to_owned(),
            modules: many.len(),
        }),
    }
}
