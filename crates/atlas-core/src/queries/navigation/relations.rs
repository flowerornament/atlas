//! Native subject selection over the existing lexical relationship derivation.
use super::{Basis, Navigation, Site};
use crate::{Result, model::SourceGraph, population::TestSelection};
use std::collections::{BTreeMap, BTreeSet};

pub(super) struct Relations {
    pub relationships: Vec<crate::model::Relationship>,
    pub diagnostics: Vec<crate::model::Diagnostic>,
    pub mentioned: BTreeSet<String>,
    pub sites: BTreeMap<String, Site>,
}
impl Navigation<'_> {
    pub(super) fn relations(&self) -> Result<&Relations> {
        self.relations
            .get_or_init(|| self.read_relations().map_err(|error| error.to_string()))
            .as_ref()
            .map_err(|why| why.clone().into())
    }
    pub(super) fn relation_basis(
        &self,
        relations: &Relations,
        applies: impl Fn(&crate::model::Diagnostic) -> bool,
    ) -> Basis {
        let mut basis = self.basis();
        basis.diagnostics.extend(
            relations
                .diagnostics
                .iter()
                .filter(|diagnostic| applies(diagnostic))
                .cloned(),
        );
        basis
    }
    pub(super) fn site_basis<'a>(
        &self,
        relations: &Relations,
        sites: impl Iterator<Item = &'a Site>,
    ) -> Basis {
        let mut displayed = BTreeMap::<&str, Vec<&Site>>::new();
        for site in sites {
            displayed.entry(&site.file).or_default().push(site);
        }
        self.relation_basis(relations, |diagnostic| {
            displayed
                .get(diagnostic.file.as_str())
                .is_some_and(|sites| sites.iter().any(|site| site.matches_diagnostic(diagnostic)))
        })
    }
    fn read_relations(&self) -> Result<Relations> {
        let files: BTreeSet<_> = self
            .workspace
            .crates
            .iter()
            .flat_map(|p| p.files.iter().map(|f| (&p.name, &f.rel)))
            .collect();
        let captured = self.snapshot.graph();
        let mut graph = SourceGraph {
            schema: captured.schema,
            root: captured.root.clone(),
            import_roots: captured.import_roots.clone(),
            files: captured
                .files
                .iter()
                .filter(|f| files.contains(&(&f.crate_name, &f.path)))
                .cloned()
                .collect(),
            relationships: Vec::new(),
            diagnostics: Vec::new(),
        };
        let mut sources = BTreeMap::new();
        let mut registry = BTreeSet::new();
        let mut sites = BTreeMap::new();
        for file in &mut graph.files {
            let source = self
                .snapshot
                .retain_source(&file.path)
                .ok_or("captured source absent")?;
            for import in source.imports().filter(|i| {
                !file.test_only && TestSelection::OutsideTestModules.admits_line(&source, i.line)
            }) {
                if let Err(why) = &import.bindings {
                    return Err(format!(
                        "import unavailable at {}:{}: {why}",
                        file.path, import.line
                    )
                    .into());
                }
            }
            let items: BTreeMap<_, _> = source
                .outline()
                .iter()
                .map(|item| ((item.line, item.name.as_str()), item))
                .collect();
            for declaration in &mut file.declarations {
                declaration.test_only = file.test_only
                    || !TestSelection::OutsideTestModules.admits_line(&source, declaration.line);
                let item = items
                    .get(&(declaration.line, declaration.name.as_str()))
                    .ok_or("captured declaration absent")?;
                if self.visibility.admits(item.visibility) {
                    registry.insert(declaration.id.clone());
                }
                sites.insert(
                    declaration.id.clone(),
                    Site::of(&file.crate_name, &file.path, item),
                );
            }
            sources.insert(file.path.clone(), source);
        }
        let mentioned = crate::relationships::derive_navigation(&mut graph, &sources, &registry);
        Ok(Relations {
            relationships: graph.relationships,
            diagnostics: graph.diagnostics,
            mentioned,
            sites,
        })
    }
}
