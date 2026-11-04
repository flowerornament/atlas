//! Qualified analysis over one retained capture, with local lazy syntax caches.
//! Index, representation observations and source control share these bytes.
//! Test selection resets only this Analysis's workflow cache; Source stays shared.
pub mod index;
pub mod representations;
mod sources;
pub mod workflow;

use self::{index::Index, sources::Sources};
use crate::{Result, architecture::Architecture, discovery::Snapshot, representations::Facts};

/// Share captured bytes and lazily parse each selected file once for this invocation.
/// Context-independent workflow facts are also materialized once and shared across queries.
/// Lexical inventory/test evidence remains owned by the snapshot, not a second AST census.
pub struct Analysis<'a> {
    snapshot: &'a Snapshot,
    sources: Sources,
    tests: crate::population::TestSelection,
}
impl<'a> Analysis<'a> {
    pub(crate) fn snapshot(&self) -> &Snapshot {
        self.snapshot
    }
    pub(crate) fn syntax(&self, path: &str) -> Result<&syn::File> {
        if let Some(error) = self.snapshot.syntax_error(path) {
            return Err(format!("{path}: {error}").into());
        }
        self.sources
            .get(path)
            .ok_or_else(|| format!("captured syntax absent for {path}"))?
            .ast(path)
    }
    pub fn new(snapshot: &'a Snapshot) -> Result<Self> {
        let sources = sources::retain_sources(snapshot.captured_sources().map(Ok))?;
        Ok(Self {
            snapshot,
            sources,
            tests: crate::population::TestSelection::Production,
        })
    }
    /// Select test evidence without reparsing or changing the captured source population.
    pub fn with_tests(mut self, tests: crate::population::TestSelection) -> Self {
        if self.tests != tests {
            for source in self.sources.values_mut() {
                source.clear_workflow();
            }
        }
        self.tests = tests;
        self
    }
    fn require_selected_syntax(&self) -> Result<()> {
        for file in self
            .snapshot
            .graph()
            .physical_files()
            .filter(|file| self.tests.admits_file(file.test_only))
        {
            if let Some(error) = self.snapshot.syntax_error(&file.path) {
                return Err(format!("{}: {error}", file.path).into());
            }
        }
        Ok(())
    }
    /// Written type, literal and call observations from this capture.
    pub fn representations(&self) -> Result<Facts> {
        self.require_selected_syntax()?;
        representations::extract::from_selected_sources(
            &self.sources,
            self.snapshot.metadata(),
            self.snapshot.graph(),
            self.tests,
        )
    }
    /// Bind the same captured names to a caller's declared architecture.
    pub fn index(&self, architecture: &Architecture) -> Result<Index> {
        self.require_selected_syntax()?;
        Index::from_selected_sources(
            &self.sources,
            self.snapshot.metadata(),
            self.snapshot.graph(),
            architecture,
            self.tests,
        )
    }
    /// Bounded source-call expansion; no execution or path-feasibility claim.
    pub fn expanded(
        &self,
        path: &str,
        function: &str,
        depth: u8,
    ) -> Result<(workflow::Workflow, crate::diagram::Diagram)> {
        self.expanded_with_detail(path, function, depth, workflow::Detail::Expressions)
    }
    /// Source-control contraction retains its folded nodes in structured coverage.
    pub fn expanded_with_detail(
        &self,
        path: &str,
        function: &str,
        depth: u8,
        detail: workflow::Detail,
    ) -> Result<(workflow::Workflow, crate::diagram::Diagram)> {
        workflow::check_depth(depth)?;
        let architecture = Architecture {
            schema: 1,
            provenance: vec![],
            modules: vec![],
            workflows: vec![],
        };
        workflow::expand_with_detail(&self.index(&architecture)?, path, function, depth, detail)
    }
}
