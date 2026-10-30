//! Definition, binding and module cards; project FM enrichment remains caller-owned.
use super::{Basis, Navigation, Tally, census_kind};
use crate::{
    Result,
    discovery::module_path_of,
    model::{Evidence, Resolution},
    population::{TestSelection, VisibilitySelection},
    source::{Item, ItemKind, Visibility},
    workspace::{CrateMap, FileMap},
};
use serde::Serialize;
use std::collections::BTreeSet;

#[derive(Clone, Serialize)]
pub struct Site {
    pub package: String,
    pub file: String,
    pub line: usize,
    pub kind: ItemKind,
    pub visibility: Visibility,
    pub name: String,
    pub doc: Option<String>,
    pub visibility_restriction: Option<String>,
}
impl Site {
    pub(super) fn matches_diagnostic(&self, diagnostic: &crate::model::Diagnostic) -> bool {
        self.file == diagnostic.file
            && diagnostic.line.is_none_or(|line| self.line == line)
            && diagnostic
                .package
                .as_ref()
                .is_none_or(|package| self.package == *package)
    }
    pub(super) fn of(package: &str, file: &str, item: &Item) -> Self {
        Self {
            package: package.into(),
            file: file.into(),
            line: item.line,
            kind: item.kind,
            visibility: item.visibility,
            name: item.name.clone(),
            doc: item.doc.clone(),
            visibility_restriction: item.visibility_restriction().map(str::to_owned),
        }
    }
}
#[derive(Serialize)]
pub struct Edge {
    pub source: Site,
    pub target: String,
    pub candidates: Vec<Site>,
    pub resolution: Resolution,
    pub evidence: Evidence,
}
#[derive(Serialize)]
pub struct Name {
    pub schema: u32,
    pub query: String,
    pub population: VisibilitySelection,
    pub matches: Vec<Site>,
    pub hidden: usize,
    pub reexports: Vec<Site>,
    pub impls: Vec<Site>,
    pub refs_approx: usize,
    pub edges: Vec<Edge>,
    pub basis: Basis,
}
#[derive(Serialize)]
pub struct Module {
    pub schema: u32,
    pub path: String,
    pub population: VisibilitySelection,
    pub contexts: Vec<ModuleContext>,
    pub basis: Basis,
}
#[derive(Serialize)]
pub struct ModuleContext {
    pub package: String,
    pub header: Vec<String>,
    pub tally: Tally,
    pub undocumented: usize,
    pub imports: usize,
    pub internal_imports: usize,
    /// None for the crate root: there is no module path whose imports can be counted.
    pub fan_in_files: Option<Vec<String>>,
    pub items: Vec<Site>,
    /// Selected type spellings absent from declaration mentions anywhere in the workspace.
    pub unmentioned_names: Vec<String>,
    pub edges: Vec<Edge>,
}
impl Navigation<'_> {
    /// Bare name or the native path-fragment::Name spelling; no Rust resolution implied.
    pub fn name(&self, query: &str) -> Result<Name> {
        let (fragment, name) = query
            .rsplit_once("::")
            .map_or((None, query), |(fragment, name)| {
                (Some(fragment.replace("::", "/")), name)
            });
        if name.is_empty() {
            return Err("a name query must name an identifier".into());
        }
        let in_fragment = |path: &str| {
            fragment.as_ref().is_none_or(|part| {
                path == part
                    || path.ends_with(&format!("/{part}"))
                    || path.ends_with(&format!("/{part}.rs"))
            })
        };
        let mut matches = Vec::new();
        let mut hidden = 0;
        let mut reexports = Vec::new();
        let mut impls = Vec::new();
        let mut defining = BTreeSet::new();
        for package in &self.workspace.crates {
            for file in &package.files {
                for item in file.items(TestSelection::OutsideTestModules) {
                    if item.kind == ItemKind::PubUse {
                        if in_fragment(&file.rel)
                            && file.source.import_at(item.line).is_some_and(|import| {
                                import.bindings.as_ref().is_ok_and(|bindings| {
                                    bindings.iter().any(|binding| {
                                        binding.source == name || binding.local_name == name
                                    })
                                })
                            })
                        {
                            reexports.push(Site::of(&package.name, &file.rel, item));
                        }
                    } else if item.name == name && in_fragment(&file.rel) {
                        let site = Site::of(&package.name, &file.rel, item);
                        if item.kind == ItemKind::Impl {
                            impls.push(site);
                            continue;
                        }
                        defining.insert((file.rel.as_str(), item.line));
                        if self.visibility.admits(item.visibility) {
                            matches.push(site);
                        } else {
                            hidden += 1;
                        }
                    }
                }
            }
        }
        if matches.is_empty() && hidden == 0 {
            return Err(format!("no declaration of `{query}` in the selected source scope; macro-generated declarations are outside the lexical grammar").into());
        }
        let refs_approx = self
            .workspace
            .crates
            .iter()
            .flat_map(|p| &p.files)
            .flat_map(|file| {
                file.code_lines(TestSelection::OutsideTestModules)
                    .filter(|(line, _)| !defining.contains(&(file.rel.as_str(), *line)))
            })
            .map(|(_, code)| {
                crate::scan::identifiers(code)
                    .filter(|token| *token == name)
                    .count()
            })
            .sum();
        let relations = self.relations()?;
        let edges = Self::edges(relations, |site, edge| {
            edge.spelling == name || defining.contains(&(site.file.as_str(), site.line))
        })?;
        matches.sort_by(|a, b| (&a.file, a.line, &a.package).cmp(&(&b.file, b.line, &b.package)));
        let displayed: Vec<_> = matches
            .iter()
            .chain(&reexports)
            .chain(&impls)
            .chain(edges.iter().map(|edge| &edge.source))
            .collect();
        let basis = self.site_basis(relations, displayed.into_iter());
        Ok(Name {
            schema: 1,
            query: query.into(),
            population: self.visibility,
            matches,
            hidden,
            reexports,
            impls,
            refs_approx,
            edges,
            basis,
        })
    }
    pub fn module(&self, path: &str) -> Result<Module> {
        let path = path.trim_start_matches("./");
        let relations = self.relations()?;
        let mut contexts = Vec::new();
        for package in &self.workspace.crates {
            for file in package.files.iter().filter(|f| f.rel == path) {
                let mut tally = Tally::default();
                tally.add_file(file, self.visibility);
                let items: Vec<_> = file
                    .items(TestSelection::OutsideTestModules)
                    .map(|item| Site::of(&package.name, &file.rel, item))
                    .collect();
                let undocumented = items
                    .iter()
                    .filter(|item| {
                        census_kind(item.kind)
                            && self.visibility.admits(item.visibility)
                            && item.doc.is_none()
                    })
                    .count();
                let (imports, internal_imports) = import_counts(file);
                let fan_in_files = fan_in(package, file);
                let unmentioned_names = items
                    .iter()
                    .filter(|item| {
                        matches!(
                            item.kind,
                            ItemKind::Struct | ItemKind::Enum | ItemKind::Trait
                        ) && self.visibility.admits(item.visibility)
                            && !relations.mentioned.contains(&item.name)
                    })
                    .map(|item| item.name.clone())
                    .collect();
                contexts.push(ModuleContext {
                    package: package.name.clone(),
                    header: file.source.projection().module_header_lines(),
                    tally,
                    undocumented,
                    imports,
                    internal_imports,
                    fan_in_files,
                    items,
                    unmentioned_names,
                    edges: Self::edges(relations, |source, _| {
                        source.file == path && source.package == package.name
                    })?,
                });
            }
        }
        if contexts.is_empty() {
            return Err(format!("no workspace source module at `{path}`").into());
        }
        Ok(Module {
            schema: 1,
            path: path.into(),
            population: self.visibility,
            contexts,
            basis: self.relation_basis(relations, |diagnostic| diagnostic.file == path),
        })
    }
    pub(super) fn edges(
        relations: &super::relations::Relations,
        selected: impl Fn(&Site, &crate::model::Relationship) -> bool,
    ) -> Result<Vec<Edge>> {
        let sites = &relations.sites;
        let mut edges = Vec::new();
        for edge in &relations.relationships {
            let source = sites
                .get(edge.source.as_str())
                .ok_or("relationship source absent")?;
            if !selected(source, edge) {
                continue;
            }
            let targets: &[String] = match &edge.target {
                Resolution::Unique(id) => std::slice::from_ref(id),
                Resolution::Ambiguous(ids) => ids,
                Resolution::Unresolved(_) => &[],
            };
            let candidates = targets
                .iter()
                .map(|id| {
                    sites
                        .get(id.as_str())
                        .cloned()
                        .ok_or_else(|| "relationship target absent".into())
                })
                .collect::<Result<Vec<_>>>()?;
            edges.push(Edge {
                source: source.clone(),
                target: edge.spelling.clone(),
                candidates,
                resolution: edge.target.clone(),
                evidence: edge.evidence,
            });
        }
        edges.sort_by(|a, b| {
            (
                &a.source.file,
                a.source.line,
                a.evidence.label(),
                &a.target,
                &a.source.package,
            )
                .cmp(&(
                    &b.source.file,
                    b.source.line,
                    b.evidence.label(),
                    &b.target,
                    &b.source.package,
                ))
        });
        Ok(edges)
    }
}
// The donor's fan measure observes the written use prefix, not resolved bindings.
// Restrict that reading to the retained import sites instead of rescanning every code line.
fn use_targets(file: &FileMap) -> impl Iterator<Item = &str> {
    file.source
        .imports()
        .filter(|import| {
            !file.test_file
                && TestSelection::OutsideTestModules.admits_line(&file.source, import.line)
        })
        .filter_map(|import| import.statement.lines().find_map(crate::scan::use_import))
}
fn import_counts(file: &FileMap) -> (usize, usize) {
    use_targets(file).fold((0, 0), |(total, internal), target| {
        (
            total + 1,
            internal
                + usize::from(
                    ["crate::", "super::", "self::"]
                        .iter()
                        .any(|prefix| target.starts_with(prefix)),
                ),
        )
    })
}
fn fan_in(package: &CrateMap, file: &FileMap) -> Option<Vec<String>> {
    let module = module_path_of(&package.src_rel, &file.rel)?;
    let segment = module.rsplit("::").next()?;
    Some(
        package
            .files
            .iter()
            .filter(|other| other.rel != file.rel)
            .filter(|other| {
                use_targets(other).any(|target| {
                    target.contains(&module)
                        || crate::scan::identifiers(target).any(|token| token == segment)
                })
            })
            .map(|other| other.rel.clone())
            .collect(),
    )
}
