//! Canonical written-Rust structure. Parse once to portable facts, retaining original spans.
//! Macro payloads are opaque; this performs neither expansion nor type/name resolution.
pub(crate) mod attributes;
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
    pub modules: Vec<(String, Option<String>, bool)>,
}
pub(super) fn read(path: &Path, projection: &RustSourceProjection) -> Result<Facts, SyntaxError> {
    let ast = parse_file(projection.text()).map_err(SyntaxError::from)?;
    let mut walk = walk::Walk::new(path, projection);
    walk.visit_file(&ast);
    walk.finish()
}

/// syn recognizes the preamble; reparsing its blanked prefix preserves original
/// byte coordinates, which syn::parse_file otherwise rebases after BOM/shebang.
pub(crate) fn parse_file(text: &str) -> syn::Result<syn::File> {
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
