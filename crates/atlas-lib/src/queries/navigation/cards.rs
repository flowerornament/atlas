//! Definition, binding and module cards; project-specific enrichment remains caller-owned.
use super::{Basis, Navigation, Tally, census_kind};
use crate::{
    Result,
    discovery::module_path_of,
    model::{Evidence, Resolution},
    population::{TestSelection, VisibilitySelection},
    queries::selector::Selector,
    source::{Item, ItemKind, Visibility},
    workspace::{CrateMap, FileMap},
};
use serde::Serialize;
use std::collections::BTreeSet;

#[derive(Clone, Serialize)]
#[non_exhaustive]
pub struct Site {
    pub package: String,
    pub file: String,
    pub line: usize,
    pub kind: ItemKind,
    pub visibility: Visibility,
    pub name: String,
    pub doc: Option<String>,
    pub visibility_restriction: Option<String>,
    #[serde(skip)]
    pub(super) offset: usize,
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
            offset: item.syntax_span.start,
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
#[non_exhaustive]
pub struct Edge {
    pub source: Site,
    pub target: String,
    pub candidates: Vec<Site>,
    pub resolution: Resolution,
    pub evidence: Evidence,
}
#[derive(Serialize)]
#[non_exhaustive]
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
#[non_exhaustive]
pub struct Module {
    pub schema: u32,
    pub path: String,
    pub population: VisibilitySelection,
    pub contexts: Vec<ModuleContext>,
    pub basis: Basis,
}
#[derive(Serialize)]
#[non_exhaustive]
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
/// The declarations a name selector admits, grouped as the card reports them.
#[derive(Default)]
struct Declarations<'w> {
    matches: Vec<Site>,
    hidden: usize,
    reexports: Vec<Site>,
    impls: Vec<Site>,
    /// Defining sites (file, line) of matches and hidden declarations.
    defining: BTreeSet<(&'w str, usize)>,
}
impl Navigation<'_> {
    /// A `NAME`, `MODULE::NAME` or `FILE:LINE[:COLUMN]` selector. The modules
    /// match the end of the declaration's module path: its file's directories
    /// and name, then enclosing inline `mod` blocks. No Rust resolution implied.
    pub fn name(&self, query: &str) -> Result<Name> {
        self.require_sources(None)?;
        let selector = Selector::parse(query)?;
        let Declarations {
            mut matches,
            hidden,
            reexports,
            impls,
            defining,
        } = self.declarations(&selector);
        if matches.is_empty() && hidden == 0 {
            return Err(format!("no declaration of `{query}` in the selected source scope; give NAME, MODULE::NAME (the end of its module path, inline modules included) or FILE:LINE; macro-generated declarations are outside the lexical grammar").into());
        }
        // A site selector names whatever is declared there.
        let name = selector
            .name()
            .or_else(|| matches.first().map(|site: &Site| site.name.as_str()))
            .unwrap_or_default()
            .to_owned();
        let name = name.as_str();
        let refs_approx = self.approximate_references(name, &defining);
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
    /// Sort every outside-test declaration the selector admits into the card's
    /// groups: visible matches, hidden ones (counted only), impls, and
    /// re-exports, which a path selector names by their own module path.
    fn declarations(&self, selector: &Selector<'_>) -> Declarations<'_> {
        let mut found = Declarations::default();
        for package in &self.workspace.crates {
            for file in &package.files {
                let modules = crate::source::InlineModules::of(file.source.outline());
                for item in file.items(TestSelection::OutsideTestModules) {
                    let inline = || modules.enclosing(item);
                    if item.kind == ItemKind::PubUse {
                        let Some(name) = selector.name() else {
                            continue;
                        };
                        // A re-export is selected by its own module path.
                        if selector.admits(name, &file.rel, item.line, None, inline)
                            && file.source.import_for(item).is_some_and(|import| {
                                import.bindings.as_ref().is_ok_and(|bindings| {
                                    bindings.iter().any(|binding| {
                                        binding.source == name || binding.local_name == name
                                    })
                                })
                            })
                        {
                            found
                                .reexports
                                .push(Site::of(&package.name, &file.rel, item));
                        }
                    } else if selector.admits(&item.name, &file.rel, item.line, None, inline) {
                        let site = Site::of(&package.name, &file.rel, item);
                        if item.kind == ItemKind::Impl {
                            found.impls.push(site);
                            continue;
                        }
                        found.defining.insert((file.rel.as_str(), item.line));
                        if self.visibility.admits(item.visibility) {
                            found.matches.push(site);
                        } else {
                            found.hidden += 1;
                        }
                    }
                }
            }
        }
        found
    }
    /// Lexical identifier tokens spelled `name` on outside-test code lines,
    /// excluding the defining lines themselves: an approximation, not resolved uses.
    fn approximate_references(&self, name: &str, defining: &BTreeSet<(&str, usize)>) -> usize {
        self.workspace
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
            .sum()
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
        .map(|import| import.target.as_ref())
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
