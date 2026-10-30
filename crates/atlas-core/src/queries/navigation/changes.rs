//! Native captured declaration-content changes, independent of body-move normalization.
use super::{Basis, Edge, Navigation, Site, census_kind};
use crate::{
    Result,
    model::Resolution,
    population::TestSelection,
    source::{Item, ItemKind},
};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

#[derive(Clone, Serialize)]
pub struct DeclarationVersion {
    pub id: String,
    pub site: Site,
    pub owner: String,
    pub top_level: bool,
    pub declaration_block: Arc<str>,
    /// Full attached docs/attributes/comments; only surrounding blank layout is omitted.
    pub attachments: Arc<str>,
}
impl DeclarationVersion {
    fn same_content(&self, other: &Self) -> bool {
        self.site.kind == other.site.kind
            && self.site.visibility == other.site.visibility
            && self.site.name == other.site.name
            && self.site.doc == other.site.doc
            && self.top_level == other.top_level
            && self.declaration_block == other.declaration_block
            && self.attachments == other.attachments
    }
}
#[derive(Serialize)]
#[serde(tag = "change", rename_all = "kebab-case")]
pub enum ChangedDeclaration {
    Removed {
        before: DeclarationVersion,
    },
    Modified {
        before: DeclarationVersion,
        after: Box<DeclarationVersion>,
    },
    Added {
        after: DeclarationVersion,
    },
}
impl ChangedDeclaration {
    pub const fn before(&self) -> Option<&DeclarationVersion> {
        match self {
            Self::Removed { before } | Self::Modified { before, .. } => Some(before),
            Self::Added { .. } => None,
        }
    }
    pub fn after(&self) -> Option<&DeclarationVersion> {
        match self {
            Self::Added { after } => Some(after),
            Self::Modified { after, .. } => Some(after.as_ref()),
            Self::Removed { .. } => None,
        }
    }
    fn displayed(&self) -> &DeclarationVersion {
        match self {
            Self::Removed { before } => before,
            Self::Added { after } => after,
            Self::Modified { after, .. } => after.as_ref(),
        }
    }
    const fn rank(&self) -> u8 {
        match self {
            Self::Removed { .. } => 0,
            Self::Modified { .. } => 1,
            Self::Added { .. } => 2,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Side {
    Current,
    Prior,
}
#[derive(Serialize)]
pub struct Incident {
    pub side: Side,
    pub edge: Edge,
}
#[derive(Serialize)]
pub struct DeclarationDiff {
    pub schema: u32,
    pub scope: String,
    pub population: crate::population::VisibilitySelection,
    pub declarations: Vec<ChangedDeclaration>,
    /// None when incident relationships were not requested.
    pub relationships: Option<Vec<Incident>>,
    pub before: Basis,
    pub after: Basis,
    pub limits: &'static str,
}
type Key = (String, String, String, String, String);
type Inventory = BTreeMap<Key, Vec<DeclarationVersion>>;
impl Navigation<'_> {
    /// Captured declaration/doc changes plus optionally known incident lexical relationships.
    pub fn declaration_diff(
        &self,
        before: &Self,
        scope: &str,
        include_relationships: bool,
    ) -> Result<DeclarationDiff> {
        if self.visibility != before.visibility {
            return Err(
                "declaration comparison requires the same subject population on both sides".into(),
            );
        }
        let scope = crate::review::comparison_scope(before.snapshot, self.snapshot, scope)?;
        let declarations = changed_declarations(
            before.declaration_inventory(&scope)?,
            self.declaration_inventory(&scope)?,
        );
        let (relationships, prior_basis, current_basis) = if include_relationships {
            let (mut edges, prior_basis) = before.incidents(&declarations, Side::Prior)?;
            let (current, current_basis) = self.incidents(&declarations, Side::Current)?;
            edges.extend(current);
            edges.sort_by(|a, b| {
                (
                    a.side,
                    &a.edge.source.file,
                    a.edge.source.line,
                    a.edge.evidence.label(),
                    &a.edge.target,
                    &a.edge.source.package,
                )
                    .cmp(&(
                        b.side,
                        &b.edge.source.file,
                        b.edge.source.line,
                        b.edge.evidence.label(),
                        &b.edge.target,
                        &b.edge.source.package,
                    ))
            });
            (Some(edges), prior_basis, current_basis)
        } else {
            (None, before.basis(), self.basis())
        };
        Ok(DeclarationDiff {
            schema: 1,
            scope,
            population: self.visibility,
            declarations,
            relationships,
            before: prior_basis,
            after: current_basis,
            limits: "Lexical file/kind/name/owner matching, not semantic equivalence. Incidence is by changed source or a unique changed target; ambiguous target-only incidence is not inferred.",
        })
    }
    fn incidents(
        &self,
        declarations: &[ChangedDeclaration],
        side: Side,
    ) -> Result<(Vec<Incident>, Basis)> {
        let versions: Vec<_> = declarations
            .iter()
            .filter_map(|change| match side {
                Side::Prior => change.before(),
                Side::Current => change.after(),
            })
            .collect();
        let ids: BTreeSet<_> = versions.iter().map(|version| version.id.as_str()).collect();
        let positions: BTreeSet<_> = versions
            .iter()
            .map(|version| {
                (
                    version.site.package.as_str(),
                    version.site.file.as_str(),
                    version.site.line,
                    version.site.name.as_str(),
                )
            })
            .collect();
        let relations = self.relations()?;
        let edges = Self::edges(relations, |source, edge| {
            positions.contains(&(
                source.package.as_str(),
                source.file.as_str(),
                source.line,
                source.name.as_str(),
            )) || matches!(&edge.target, Resolution::Unique(id) if ids.contains(id.as_str()))
        })?;
        let basis = self.site_basis(relations, versions.iter().map(|version| &version.site));
        Ok((
            edges
                .into_iter()
                .map(|edge| Incident { side, edge })
                .collect(),
            basis,
        ))
    }
    fn declaration_inventory(&self, scope: &str) -> Result<Inventory> {
        let files: BTreeMap<_, _> = self
            .snapshot
            .files(crate::population::FileSelection::SourceTree)
            .map(|file| ((file.crate_name.as_str(), file.path.as_str()), file))
            .collect();
        let mut inventory = BTreeMap::<Key, Vec<_>>::new();
        for package in &self.workspace.crates {
            for file in package
                .files
                .iter()
                .filter(|file| crate::architecture::contains(scope, &file.rel))
            {
                let observation = files
                    .get(&(package.name.as_str(), file.rel.as_str()))
                    .ok_or("source observation absent")?;
                let ids: BTreeMap<_, _> = observation
                    .declarations
                    .iter()
                    .map(|item| ((item.line, item.name.as_str()), item.id.as_str()))
                    .collect();
                let lines: Vec<_> = file.source.text().lines().collect();
                let mut stack = Vec::<&Item>::new();
                let mut impl_headers = BTreeMap::new();
                for item in file.source.outline() {
                    while stack
                        .last()
                        .is_some_and(|outer| !outer.span.contains(&item.line))
                    {
                        stack.pop();
                    }
                    if item.kind.declares()
                        && census_kind(item.kind)
                        && !file.test_file
                        && TestSelection::OutsideTestModules.admits_line(&file.source, item.line)
                        && self.visibility.admits(item.visibility)
                    {
                        let owner = owner(&stack, &lines, &mut impl_headers)?;
                        let id = ids
                            .get(&(item.line, item.name.as_str()))
                            .ok_or("declaration identity absent")?;
                        inventory
                            .entry((
                                package.name.clone(),
                                file.rel.clone(),
                                format!("{:?}", item.kind),
                                item.name.clone(),
                                owner.clone(),
                            ))
                            .or_default()
                            .push(DeclarationVersion {
                                id: (*id).into(),
                                site: Site::of(&package.name, &file.rel, item),
                                owner,
                                top_level: item.top_level,
                                declaration_block: Arc::clone(&item.declaration_block),
                                attachments: attachments(&lines, item)?,
                            });
                    }
                    stack.push(item);
                }
            }
        }
        Ok(inventory)
    }
}
fn changed_declarations(mut old: Inventory, mut new: Inventory) -> Vec<ChangedDeclaration> {
    let keys: BTreeSet<_> = old.keys().chain(new.keys()).cloned().collect();
    let mut declarations = Vec::new();
    for key in keys {
        pair(
            old.remove(&key).unwrap_or_default(),
            new.remove(&key).unwrap_or_default(),
            &mut declarations,
        );
    }
    declarations.sort_by(|a, b| {
        (
            &a.displayed().site.file,
            a.displayed().site.line,
            a.rank(),
            &a.displayed().site.name,
            &a.displayed().site.package,
        )
            .cmp(&(
                &b.displayed().site.file,
                b.displayed().site.line,
                b.rank(),
                &b.displayed().site.name,
                &b.displayed().site.package,
            ))
    });
    declarations
}

fn owner(
    stack: &[&Item],
    lines: &[&str],
    impl_headers: &mut BTreeMap<usize, String>,
) -> Result<String> {
    let mut parts = Vec::new();
    for parent in stack {
        if parent.kind == ItemKind::Impl {
            if let std::collections::btree_map::Entry::Vacant(entry) =
                impl_headers.entry(parent.line)
            {
                let text = lines
                    .get(*parent.span.start() - 1..*parent.span.end())
                    .ok_or_else(|| {
                        format!(
                            "impl span unavailable at {}:{}",
                            parent.file.display(),
                            parent.line
                        )
                    })?
                    .join("\n");
                let header = crate::review::moves::impl_header(&text).ok_or_else(|| {
                    format!(
                        "unreadable impl owner at {}:{}",
                        parent.file.display(),
                        parent.line
                    )
                })?;
                entry.insert(header.to_string());
            }
            parts.push(
                impl_headers
                    .get(&parent.line)
                    .ok_or("impl owner absent")?
                    .clone(),
            );
        } else {
            parts.push(format!("{:?}:{}", parent.kind, parent.name));
        }
    }
    Ok(parts.join("::"))
}
fn pair(
    old: Vec<DeclarationVersion>,
    mut new: Vec<DeclarationVersion>,
    changed: &mut Vec<ChangedDeclaration>,
) {
    let mut old_left = Vec::new();
    for prior in old {
        if let Some(index) = new.iter().position(|current| prior.same_content(current)) {
            new.remove(index);
        } else {
            old_left.push(prior);
        }
    }
    if let ([before], [after]) = (old_left.as_slice(), new.as_slice()) {
        changed.push(ChangedDeclaration::Modified {
            before: before.clone(),
            after: Box::new(after.clone()),
        });
    } else {
        changed.extend(
            old_left
                .into_iter()
                .map(|before| ChangedDeclaration::Removed { before }),
        );
        changed.extend(
            new.into_iter()
                .map(|after| ChangedDeclaration::Added { after }),
        );
    }
}

fn attachments(lines: &[&str], item: &Item) -> Result<Arc<str>> {
    let prefix = lines
        .get(*item.span.start() - 1..item.line - 1)
        .ok_or_else(|| {
            format!(
                "attached source unavailable at {}:{}",
                item.file.display(),
                item.line
            )
        })?;
    let first = prefix
        .iter()
        .position(|line| !line.trim().is_empty())
        .unwrap_or(prefix.len());
    let last = prefix
        .iter()
        .rposition(|line| !line.trim().is_empty())
        .map_or(first, |index| index + 1);
    Ok(Arc::from(prefix[first..last].join("\n")))
}
