//! Captured source bytes and portable written-Rust facts. Syntax has one syn owner;
//! lexical text policy is separate. Qualified AST caches stay local to Analysis.
mod capture;
pub mod discovery;
mod facts_cache;
mod items;
mod lexical;
pub mod model;
pub mod population;
pub mod relationships;
pub(crate) mod scan;
mod syntax;
pub(crate) mod text;
pub(crate) mod workspace;
pub(crate) use capture::{capture, capture_files};
pub(crate) use facts_cache::hex;
pub use facts_cache::{FactsCache, FactsCacheMode, FactsCacheStats};
pub(crate) use items::{Blocks, InlineModules};
pub use items::{Field, Item, ItemKind, TestRegions, Visibility};
pub use lexical::RustSourceProjection;
pub use scan::non_test_loc;
use std::{collections::BTreeSet, path::Path, sync::Arc};
pub use syntax::measure::FunctionMeasure;
#[doc(hidden)]
pub use syntax::parse_count;
pub use syntax::{
    attributes::attributes_require_test_or_features,
    fragments::{impl_trait, restricted_path, struct_fields},
    imports::{ImportBinding, import_bindings},
};
pub(crate) use syntax::{attributes::expression_attrs, identity, parse_file};

/// Syntax failure contains only portable data and a one-based source location.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum SyntaxError {
    Parse {
        line: usize,
        column: usize,
        message: String,
    },
    Unsupported {
        line: usize,
        form: String,
    },
}
impl SyntaxError {
    pub const fn line(&self) -> usize {
        match self {
            Self::Parse { line, .. } | Self::Unsupported { line, .. } => *line,
        }
    }
}
impl From<syn::Error> for SyntaxError {
    fn from(error: syn::Error) -> Self {
        let start = error.span().start();
        Self::Parse {
            line: start.line,
            column: start.column + 1,
            message: error.to_string(),
        }
    }
}
impl std::fmt::Display for SyntaxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse {
                line,
                column,
                message,
            } => write!(f, "Rust syntax at {line}:{column}: {message}"),
            Self::Unsupported { line, form } => {
                write!(f, "unsupported Rust syntax at {line}: {form}")
            }
        }
    }
}
impl std::error::Error for SyntaxError {}

/// Why a use declaration's bindings could not be read; the message names
/// what the shared grammar could not read. Display prints the message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnreadableImport(String);

impl UnreadableImport {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl std::fmt::Display for UnreadableImport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for UnreadableImport {}

/// One use declaration, normalized once with its exact source witness.
#[non_exhaustive]
pub struct Import {
    pub line: usize,
    pub end_line: usize,
    pub top_level: bool,
    pub statement: Arc<str>,
    pub bindings: Result<Vec<ImportBinding>, UnreadableImport>,
    pub bytes: std::ops::Range<usize>,
    pub target: Arc<str>,
}
/// Successfully parsed source. A Source certifies that structural facts exist;
/// it does not certify that Rust type-checks or that macros have been expanded.
#[non_exhaustive]
pub struct Source {
    projection: RustSourceProjection,
    outline: Vec<Item>,
    regions: TestRegions,
    individually_gated: BTreeSet<usize>,
    gated_bytes: Vec<std::ops::Range<usize>>,
    module_bytes: Vec<std::ops::Range<usize>>,
    modules: Vec<syntax::ModuleDeclaration>,
    imports: Vec<Import>,
    functions: Vec<FunctionMeasure>,
    code_without_imports: std::sync::OnceLock<String>,
}
impl Source {
    /// Parse written Rust into owned facts. No I/O or fallback grammar.
    pub fn of(path: &Path, text: String) -> Result<Self, SyntaxError> {
        let projection = RustSourceProjection::of(text);
        let facts = syntax::read(path, &projection)?;
        Ok(Self::from_facts(projection, facts))
    }
    /// [`Source::of`] for a file of the workspace at `root`, through the
    /// installed facts cache when it may serve that workspace.
    pub(crate) fn of_workspace_file(
        root: &Path,
        path: &str,
        text: String,
    ) -> Result<Arc<Self>, SyntaxError> {
        match capture::capture(root, path, text) {
            Captured::Parsed(source) => Ok(source),
            Captured::Invalid { error, .. } => Err(error),
        }
    }
    fn from_facts(projection: RustSourceProjection, facts: syntax::Facts) -> Self {
        Self {
            projection,
            outline: facts.outline,
            regions: facts.regions,
            gated_bytes: facts.gated_bytes,
            module_bytes: facts.module_bytes,
            individually_gated: facts.gated,
            modules: facts.modules,
            imports: facts.imports,
            functions: facts.functions,
            code_without_imports: std::sync::OnceLock::new(),
        }
    }
    /// Every function with a body, measured as Clippy reads its length and
    /// nesting (see [`FunctionMeasure`]), in source order.
    pub fn functions(&self) -> &[FunctionMeasure] {
        &self.functions
    }
    pub(crate) fn file_module_declarations(&self) -> &[syntax::ModuleDeclaration] {
        &self.modules
    }
    pub(crate) fn code_without_imports(&self) -> &str {
        self.code_without_imports.get_or_init(|| {
            let mut masked = self
                .projection
                .code_lines()
                .collect::<Vec<_>>()
                .join("\n")
                .into_bytes();
            for import in &self.imports {
                for byte in &mut masked[import.bytes.clone()] {
                    if *byte != b'\n' {
                        *byte = b' ';
                    }
                }
            }
            String::from_utf8(masked).expect("masked imports remain UTF-8")
        })
    }
    pub fn imports(&self) -> impl Iterator<Item = &Import> {
        self.imports.iter()
    }
    /// The impl header ends at syn's body opener, retaining attached attributes.
    pub(crate) fn impl_header(&self, item: &Item) -> Option<&str> {
        if item.kind != ItemKind::Impl {
            return None;
        }
        self.text()
            .get(item.byte_span.start..item.syntax_span.start + item.declaration_block.len() - 1)
    }
    pub(crate) fn import_for(&self, item: &Item) -> Option<&Import> {
        self.imports
            .iter()
            .find(|import| import.bytes.start == item.syntax_span.start)
    }
    pub(crate) fn module_test_at(&self, offset: usize) -> bool {
        self.module_bytes.iter().any(|r| r.contains(&offset))
    }
    pub(crate) fn gated_at(&self, offset: usize) -> bool {
        self.gated_bytes.iter().any(|r| r.contains(&offset))
    }
    pub fn text(&self) -> &str {
        self.projection.text()
    }
    pub fn projection(&self) -> &RustSourceProjection {
        &self.projection
    }
    /// Snapshot declarations preserve this written source order. Bind once,
    /// checking the shape, instead of joining ambiguous (line, name) keys.
    pub(crate) fn declaration_items(&self, file: &model::SourceFile) -> crate::Result<Vec<&Item>> {
        let items: Vec<_> = self
            .outline
            .iter()
            .filter(|item| item.kind.declares())
            .collect();
        if items.len() != file.declarations.len()
            || items.iter().zip(&file.declarations).any(|(item, decl)| {
                item.line != decl.line
                    || item.name != decl.name
                    || Some(item.kind) != decl.item_kind()
            })
        {
            return Err(format!(
                "captured declaration order differs from source facts: {}",
                file.path
            )
            .into());
        }
        Ok(items)
    }
    pub fn outline(&self) -> &[Item] {
        &self.outline
    }
    pub fn test_regions(&self) -> &TestRegions {
        &self.regions
    }
    pub fn individually_test_gated(&self, line: usize) -> bool {
        self.individually_gated.contains(&line)
    }
}
/// Discovery retains failed bytes too. Missing syntax is never an empty Source.
pub(crate) enum Captured {
    Parsed(Arc<Source>),
    Invalid { text: String, error: SyntaxError },
}
impl Captured {
    pub fn facts(&self) -> Option<&Source> {
        match self {
            Self::Parsed(s) => Some(s),
            Self::Invalid { .. } => None,
        }
    }
    pub fn retain(&self) -> Option<Arc<Source>> {
        match self {
            Self::Parsed(s) => Some(Arc::clone(s)),
            Self::Invalid { .. } => None,
        }
    }
    pub fn text(&self) -> &str {
        match self {
            Self::Parsed(s) => s.text(),
            Self::Invalid { text, .. } => text,
        }
    }
    pub fn error(&self) -> Option<&SyntaxError> {
        match self {
            Self::Parsed(_) => None,
            Self::Invalid { error, .. } => Some(error),
        }
    }
}
