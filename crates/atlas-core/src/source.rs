//! Rust lexical projection shared by native tools and workspace analysis.
pub use crate::scan::{
    ImportBinding, Item, ItemKind, LineItem, RustSourceProjection, TestRegions, Visibility,
    attributes_require_test_or_features, impl_trait, import_bindings, non_test_loc,
    restricted_path, struct_fields,
};

/// One use declaration, normalized once with its source witness and parse outcome.
pub struct Import {
    pub line: usize,
    pub end_line: usize,
    pub top_level: bool,
    pub statement: std::sync::Arc<str>,
    pub bindings: Result<Vec<ImportBinding>, String>,
}

/// Captured bytes and their lexical facts from one physical source file.
/// Construction belongs to discovery; projections only borrow these facts.
pub struct Source {
    projection: RustSourceProjection,
    outline: Vec<Item>,
    regions: TestRegions,
    individually_gated: std::collections::BTreeSet<usize>,
    modules: Vec<(String, Option<String>, bool)>,
    imports: std::collections::BTreeMap<usize, Import>,
}
impl Source {
    /// Derive lexical facts once from owned bytes; no filesystem access occurs here.
    pub fn of(path: &std::path::Path, text: String) -> Self {
        let projection = RustSourceProjection::of(text);
        let outline = projection.outline(path);
        let regions = projection.test_regions();
        let gates = projection.test_gates();
        let mut gated: std::collections::BTreeSet<_> =
            gates.iter().flat_map(|gate| gate.span.clone()).collect();
        for item in &outline {
            if gated.contains(&item.line)
                || gates.iter().any(|gate| {
                    gate.following_item
                        && item.span.contains(gate.span.start())
                        && *gate.span.end() < item.line
                })
            {
                gated.extend(item.span.clone());
            }
        }
        let modules = projection
            .file_module_sites()
            .into_iter()
            .map(|(line, name, path)| (name, path, gated.contains(&line)))
            .collect();
        let imports = outline
            .iter()
            .filter(|item| matches!(item.kind, ItemKind::Use | ItemKind::PubUse))
            .map(|item| {
                (
                    item.line,
                    Import {
                        line: item.line,
                        end_line: *item.span.end(),
                        top_level: item.top_level,
                        statement: std::sync::Arc::clone(&item.declaration_block),
                        bindings: crate::scan::try_import_bindings(&item.declaration_block)
                            .map_err(|error| error.to_string()),
                    },
                )
            })
            .collect();
        Self {
            projection,
            outline,
            regions,
            individually_gated: gated,
            modules,
            imports,
        }
    }
    pub(crate) fn file_module_declarations(&self) -> &[(String, Option<String>, bool)] {
        &self.modules
    }
    /// All captured use statements, including test-gated and unreadable syntax.
    pub fn imports(&self) -> impl Iterator<Item = &Import> {
        self.imports.values()
    }
    pub(crate) fn import_at(&self, line: usize) -> Option<&Import> {
        self.imports.get(&line)
    }
    /// Original bytes, including comments and attributes.
    pub fn text(&self) -> &str {
        self.projection.text()
    }
    /// Lexical mask shared with comment and declaration consumers.
    pub fn projection(&self) -> &RustSourceProjection {
        &self.projection
    }
    /// All attached lexical items, including private imports and macro invocations.
    pub fn outline(&self) -> &[Item] {
        &self.outline
    }
    /// Module-level test regions used by the native LOC population.
    pub fn test_regions(&self) -> &TestRegions {
        &self.regions
    }
    /// Directly gated declarations remain separate from module-level test ancestry.
    pub fn individually_test_gated(&self, line: usize) -> bool {
        self.individually_gated.contains(&line)
    }
}
