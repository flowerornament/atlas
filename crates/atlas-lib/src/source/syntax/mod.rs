//! Canonical written-Rust structure. Parse once to portable facts, retaining original spans.
//! Macro payloads are opaque; this performs neither expansion nor type/name resolution.
pub(crate) mod attributes;
mod distributed_slice;
pub(crate) mod fragments;
pub(crate) mod imports;
mod walk;
use super::{Item, RustSourceProjection, SyntaxError, TestRegions};
use std::{collections::BTreeSet, path::Path};
use syn::visit::Visit;

pub(super) struct Facts {
    pub outline: Vec<Item>,
    pub regions: TestRegions,
    pub gated: BTreeSet<usize>,
    pub gated_bytes: Vec<std::ops::Range<usize>>,
    pub module_bytes: Vec<std::ops::Range<usize>>,
    pub imports: Vec<super::Import>,
    pub modules: Vec<ModuleDeclaration>,
}
/// A file module declaration (`mod name;`) as written, with the inline
/// modules around it. Where its file is found is discovery's decision.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct ModuleDeclaration {
    /// The name as written; a raw identifier keeps its `r#`.
    pub name: String,
    /// A `#[path]` attribute's value.
    pub path: Option<String>,
    /// The `path` values of `#[cfg_attr(PREDICATE, path = "...")]`: files
    /// the module is read from under some configuration. Atlas evaluates no
    /// cfg, so each is a candidate beside the unconditional one.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub alternatives: Vec<String>,
    /// Under a test-only gate, on the declaration or an enclosing module.
    pub test_gated: bool,
    /// The enclosing inline `mod` blocks, outermost first.
    pub inline: Vec<InlineModule>,
    /// The declaring item's `syntax_span.start` in the file's outline.
    pub item: usize,
}
impl ModuleDeclaration {
    /// The module's path below the declaring file's module: the enclosing
    /// inline modules, then its own name.
    pub fn module_path(&self) -> impl Iterator<Item = &str> {
        self.inline
            .iter()
            .map(|inline| inline.name.as_str())
            .chain(std::iter::once(self.name.as_str()))
    }
    /// [`Self::module_path`] spelled with `::`.
    pub fn qualified_name(&self) -> String {
        self.module_path().collect::<Vec<_>>().join("::")
    }
}
/// An inline `mod name { ... }` around a file module declaration.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct InlineModule {
    pub name: String,
    /// Its `#[path]` attribute's value, which replaces its name as the
    /// directory its file modules are found in.
    pub path: Option<String>,
}
pub(super) fn read(path: &Path, projection: &RustSourceProjection) -> Result<Facts, SyntaxError> {
    let ast = parse_file(projection.text()).map_err(SyntaxError::from)?;
    let mut walk = walk::Walk::new(path, projection);
    walk.visit_file(&ast);
    walk.finish()
}

std::thread_local! {
    /// Files parsed for this thread. Capture's workers add their parses to
    /// the thread that started them, so a test reading this before and after
    /// sees only its own work.
    static PARSES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}
/// How many files this thread has parsed into syntax trees so far, including
/// parses done for it on capture workers. A facts-cache hit parses nothing.
/// For tests and measurement only; the value has no meaning across threads.
#[doc(hidden)]
pub fn parse_count() -> usize {
    PARSES.with(std::cell::Cell::get)
}
/// Attribute parses done on worker threads to this thread.
pub(super) fn count_parses(parses: usize) {
    PARSES.with(|count| count.set(count.get() + parses));
}

/// syn recognizes the preamble; reparsing its blanked prefix preserves original
/// byte coordinates, which syn::parse_file otherwise rebases after BOM/shebang.
pub(crate) fn parse_file(text: &str) -> syn::Result<syn::File> {
    PARSES.with(|count| count.set(count.get() + 1));
    let mut ast = syn::parse_file(text)?;
    let bom = if text.starts_with('\u{feff}') {
        '\u{feff}'.len_utf8()
    } else {
        0
    };
    let prefix = bom + ast.shebang.as_ref().map_or(0, String::len);
    if prefix == 0 {
        return Ok(ast);
    }
    let mut input = " ".repeat(prefix);
    input.push_str(&text[prefix..]);
    let shebang = ast.shebang.take();
    let mut parsed = syn::parse_file(&input)?;
    parsed.shebang = shebang;
    Ok(parsed)
}
