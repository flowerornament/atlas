//! Invocation-local syntax and workflow caches over retained physical source.
//! Analysis resets selected workflow facts; the shared portable Source is unchanged.
use super::workflow;
use crate::{Result, source::model::SourceGraph};
use std::{cell::OnceCell, collections::BTreeMap, path::Path, sync::Arc};

/// One retained physical source; syntax is lazy and workflow facts are shared across indexes.
pub(super) struct CachedSource {
    source: Arc<crate::source::Source>,
    ast: OnceCell<std::result::Result<syn::File, String>>,
    workflow: OnceCell<workflow::ParsedFile>,
}
impl CachedSource {
    pub(super) fn text(&self) -> &str {
        self.source.text()
    }
    pub(super) fn source(&self) -> &crate::source::Source {
        &self.source
    }
    pub(super) fn ast(&self, path: &str) -> Result<&syn::File> {
        self.ast
            .get_or_init(|| {
                crate::source::parse_file(self.source.text())
                    .map_err(|error| format!("parse source {path}: {error}"))
            })
            .as_ref()
            .map_err(|error| error.clone().into())
    }
    pub(super) fn clear_workflow(&mut self) {
        self.workflow.take();
    }
    pub(super) fn workflow(
        &self,
        path: &str,
        tests: crate::population::TestSelection,
    ) -> Result<&workflow::ParsedFile> {
        let ast = self.ast(path)?;
        Ok(self
            .workflow
            .get_or_init(|| workflow::read_ast(&self.source, path, ast, tests)))
    }
}
pub(super) type Sources = BTreeMap<String, CachedSource>;
/// Retain each physical source once; selected ASTs are cached on first use.
pub(super) fn retain_sources(
    sources: impl IntoIterator<Item = Result<(String, Arc<crate::source::Source>)>>,
) -> Result<Sources> {
    sources
        .into_iter()
        .map(|input| {
            let (path, source) = input?;
            Ok((
                path,
                CachedSource {
                    source,
                    ast: OnceCell::new(),
                    workflow: OnceCell::new(),
                },
            ))
        })
        .collect()
}
pub(super) fn read_sources(root: &Path, graph: &SourceGraph) -> Result<Sources> {
    retain_sources(graph.physical_files().filter(|f| !f.test_only).map(|f| {
        let source = std::fs::read_to_string(root.join(&f.path))?;
        if crate::discovery::fingerprint(source.as_bytes()) != f.content_fingerprint {
            return Err(format!(
                "source changed after discovery: {}; rescan before extracting",
                f.path
            )
            .into());
        }
        Ok((
            f.path.clone(),
            Arc::new(crate::source::Source::of(Path::new(&f.path), source)?),
        ))
    }))
}
