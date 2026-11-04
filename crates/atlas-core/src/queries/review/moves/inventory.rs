//! Native crate/class/name inventory and exact/rerooted token readings.
//! Byte containment folds members under their owner; tests and modules retain native policy.
use super::tokens::{self, tokens};
use crate::{
    Result,
    source::{
        self as scan, Item, ItemKind, Source, Visibility, discovery::Snapshot, model::SourceFile,
    },
};
use std::collections::{BTreeMap, BTreeSet};

/// A declaration's tokens in the two readings `moved` compares.
#[derive(Clone, PartialEq, Eq)]
pub(super) enum Body {
    /// Not a body comparison: fields (their struct carries them), module
    /// declarations and `use` bindings (layout a move rewrites), or a run
    /// that asked for visibility only.
    Uncompared,
    /// The span does not tokenize; reported, never counted as preserved.
    Unreadable,
    /// `exact` forgives docs, visibility and layout; `rerooted` also drops the
    /// `crate::`/`super::`/`self::` qualifiers (and the module segments after
    /// them) that a move rewrites.
    Tokens {
        exact: Vec<String>,
        rerooted: Vec<String>,
    },
}

/// One keyed declaration on one side of the comparison.
#[derive(Clone)]
pub(super) struct Site {
    pub(super) file: String,
    pub(super) line: usize,
    pub(super) visibility: Visibility,
    /// The path of a `pub(in path)`: two such rungs share a rank but not a
    /// scope.
    pub(super) restricted: Option<String>,
    pub(super) body: Body,
}

impl Site {
    pub(super) fn same_visibility(&self, other: &Self) -> bool {
        self.visibility == other.visibility && self.restricted == other.restricted
    }

    pub(super) fn visibility_text(&self) -> String {
        self.restricted.as_ref().map_or_else(
            || self.visibility.label().to_owned(),
            |path| format!("pub(in {path})"),
        )
    }
}

/// `(crate, class, name)`: the identity a move preserves.
pub(super) type Key = (String, &'static str, String);

pub(super) type Inventory = BTreeMap<Key, Vec<Site>>;

/// Every keyed declaration of the crates `scope` matches, each crate whole.
pub(super) fn inventory(
    snapshot: &Snapshot,
    matched: &dyn Fn(&str) -> bool,
    bodies: bool,
) -> Result<Inventory> {
    let mut inventory: Inventory = BTreeMap::new();
    for file in snapshot
        .graph()
        .files
        .iter()
        .filter(|file| matched(&file.crate_name))
    {
        let source = snapshot.source_facts(&file.path).ok_or_else(|| {
            snapshot.syntax_error(&file.path).map_or_else(
                || format!("snapshot lacks source facts for {}", file.path),
                |error| format!("{}:{}: {error}", file.path, error.line()),
            )
        })?;
        for (class, name, site) in file_sites(file, source, bodies)? {
            inventory
                .entry((file.crate_name.clone(), class, name))
                .or_default()
                .push(site);
        }
    }
    Ok(inventory)
}

/// How an enclosing item shapes the keys of the items its span holds.
enum Enclosing {
    /// Members are keyed `Owner::name`.
    Impl(String),
    /// Members are keyed under the module path.
    Module(String),
    /// Members move with their container (trait items, macro and extern
    /// bodies, items nested in a fn) and are compared as part of it.
    Folded,
}

/// The production declarations of one file as `(class, name, site)`.
fn file_sites(
    file: &SourceFile,
    source: &Source,
    bodies: bool,
) -> Result<Vec<(&'static str, String, Site)>> {
    if file.test_only {
        return Ok(Vec::new());
    }
    let outline = source.outline();
    let items: Vec<&Item> = outline
        .iter()
        .filter(|item| !source.test_regions().covers(item.line))
        .collect();
    let roots = if bodies {
        path_roots(&items, source, &file.path)?
    } else {
        BTreeSet::new()
    };
    let body_of = |item: &Item| {
        if !bodies {
            return Body::Uncompared;
        }
        tokens(&source.text()[item.byte_span.clone()], &roots)
    };
    let site = |item: &Item, body: Body| Site {
        file: file.path.clone(),
        line: item.line,
        visibility: item.visibility,
        restricted: item
            .visibility_restriction()
            .filter(|_| item.visibility == Visibility::PubIn)
            .map(|path| path.split_whitespace().collect()),
        body,
    };
    let mut found = Vec::new();
    let mut stack: Vec<(&Item, Enclosing)> = Vec::new();
    for item in items {
        while stack
            .last()
            .is_some_and(|(outer, _)| !outer.syntax_span.contains(&item.syntax_span.start))
        {
            stack.pop();
        }
        let module: String = stack
            .iter()
            .filter_map(|(_, enclosing)| match enclosing {
                Enclosing::Module(name) => Some(format!("{name}::")),
                _ => None,
            })
            .collect();
        match stack.last().map(|(_, enclosing)| enclosing) {
            Some(Enclosing::Folded) => continue,
            Some(Enclosing::Impl(owner)) => {
                if matches!(
                    item.kind,
                    ItemKind::Fn | ItemKind::Const | ItemKind::TypeAlias
                ) {
                    found.push((
                        "method",
                        format!("{owner}::{}", item.name),
                        site(item, body_of(item)),
                    ));
                }
                stack.push((item, Enclosing::Folded));
                continue;
            }
            _ => {}
        }
        let enclosing = match item.kind {
            ItemKind::Use | ItemKind::PubUse => {
                for binding in bindings(source, &file.path, item)? {
                    // A binding's body is its target: a move may reroot the
                    // path to it, never retarget the name.
                    let body = if bodies {
                        let rerooted = if rooted(&binding.source_path, &roots) {
                            binding.source.clone()
                        } else {
                            binding.source_path.clone()
                        };
                        Body::Tokens {
                            exact: vec![binding.source_path.clone()],
                            rerooted: vec![rerooted],
                        }
                    } else {
                        Body::Uncompared
                    };
                    let bound = if binding.local_name == "*" {
                        binding.source_path.clone()
                    } else {
                        binding.local_name.clone()
                    };
                    found.push(("use", format!("{module}{bound}"), site(item, body)));
                }
                Enclosing::Folded
            }
            ItemKind::Impl => {
                let header = source.impl_header(item).and_then(|text| text.parse().ok());
                let owner = item.trait_name.as_deref().map_or_else(
                    || format!("{module}{}", item.name),
                    |name| format!("<{module}{} as {name}>", item.name),
                );
                let header_body = if bodies {
                    header.map_or(Body::Unreadable, |header| tokens::readings(header, &roots))
                } else {
                    Body::Uncompared
                };
                found.push(("impl", owner.clone(), site(item, header_body)));
                Enclosing::Impl(owner)
            }
            ItemKind::Mod => {
                // A module declaration is layout, which a move rewrites by
                // definition; its items are keyed under its path.
                found.push((
                    "item",
                    format!("{module}{}", item.name),
                    site(item, Body::Uncompared),
                ));
                Enclosing::Module(item.name.clone())
            }
            ItemKind::MacroCall => {
                let inner = outline
                    .get(outline.partition_point(|inner| inner.line <= item.line))
                    .filter(|inner| item.span.contains(&inner.line))
                    .map_or_else(String::new, |inner| inner.name.clone());
                found.push((
                    "macro",
                    format!("{module}{}!({inner})", item.name),
                    site(item, body_of(item)),
                ));
                Enclosing::Folded
            }
            kind => {
                found.push((
                    "item",
                    format!("{module}{}", item.name),
                    site(item, body_of(item)),
                ));
                if kind == ItemKind::Struct {
                    for field in &item.fields {
                        found.push((
                            "field",
                            format!("{module}{}.{}", item.name, field.name),
                            Site {
                                restricted: field.restricted.clone(),
                                line: item.line + field.line_offset,
                                visibility: field.visibility,
                                ..site(item, Body::Uncompared)
                            },
                        ));
                    }
                }
                Enclosing::Folded
            }
        };
        stack.push((item, enclosing));
    }
    Ok(found)
}

fn bindings<'a>(source: &'a Source, file: &str, item: &Item) -> Result<&'a [scan::ImportBinding]> {
    let line = item.line;
    source
        .imports()
        .find(|import| import.bytes.start == item.syntax_span.start)
        .ok_or_else(|| format!("captured import absent at {file}:{line}"))?
        .bindings
        .as_deref()
        .map_err(|why| format!("import unavailable at {file}:{line}: {why}").into())
}

/// The path roots a move may rewrite in one file: `crate`, `super`, `self`,
/// the modules the file declares (`mod tick;` makes `tick::Engine` a path
/// from here), and every lowercase name its own imports bind to a
/// crate-relative path (`use crate::load::dispatch;` makes `dispatch::g` one).
fn path_roots(items: &[&Item], source: &Source, file: &str) -> Result<BTreeSet<String>> {
    let mut roots: BTreeSet<String> = ["crate", "super", "self"].map(str::to_owned).into();
    roots.extend(
        items
            .iter()
            .filter(|item| item.kind == ItemKind::Mod)
            .map(|item| item.name.clone()),
    );
    for item in items
        .iter()
        .filter(|item| matches!(item.kind, ItemKind::Use | ItemKind::PubUse))
    {
        for binding in bindings(source, file, item)? {
            let module = binding
                .local_name
                .starts_with(|ch: char| ch.is_ascii_lowercase() || ch == '_');
            if module
                && ["crate::", "super::", "self::"]
                    .iter()
                    .any(|root| binding.source_path.starts_with(root))
            {
                roots.insert(binding.local_name.clone());
            }
        }
    }
    Ok(roots)
}

/// Whether a path's first segment is one of the file's rewritable roots.
fn rooted(path: &str, roots: &BTreeSet<String>) -> bool {
    path.split_once("::")
        .is_some_and(|(head, _)| roots.contains(head.trim()))
}
