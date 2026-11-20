//! The cache's private serialized form of one file's syntax facts.
//!
//! These mirror types exist so that the public `#[non_exhaustive]` fact types
//! never gain `Deserialize`: outside code still cannot build them, and serde is
//! not part of their API. Each conversion destructures its source without `..`,
//! so a field added to a fact type fails to compile here until the wire form
//! carries it. Unknown enum tags decode as a failure, which the cache treats as
//! a miss.
use crate::source::{
    Field, Import, ImportBinding, Item, ItemKind, TestRegions, Visibility, syntax::Facts,
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, ops::Range, path::PathBuf, sync::Arc};

#[derive(Serialize, Deserialize)]
pub(super) struct WireFacts {
    outline: Vec<WireItem>,
    regions: (Vec<(usize, usize)>, usize),
    gated: BTreeSet<usize>,
    gated_bytes: Vec<(usize, usize)>,
    module_bytes: Vec<(usize, usize)>,
    imports: Vec<WireImport>,
    modules: Vec<(String, Option<String>, bool)>,
}

#[derive(Serialize, Deserialize)]
struct WireItem {
    kind: String,
    visibility: String,
    name: String,
    file: PathBuf,
    line: usize,
    span: (usize, usize),
    declaration_block: String,
    doc: Option<String>,
    top_level: bool,
    syntax_span: (usize, usize),
    byte_span: (usize, usize),
    attribute_names: BTreeSet<String>,
    restricted: Option<String>,
    trait_name: Option<String>,
    fields: Vec<WireField>,
}

#[derive(Serialize, Deserialize)]
struct WireField {
    line_offset: usize,
    name: String,
    visibility: String,
    restricted: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct WireImport {
    line: usize,
    end_line: usize,
    top_level: bool,
    statement: String,
    bindings: Result<Vec<WireBinding>, String>,
    bytes: (usize, usize),
    target: String,
}

#[derive(Serialize, Deserialize)]
struct WireBinding {
    source: String,
    source_path: String,
    local_name: String,
    module_only: bool,
}

const fn pair(range: &Range<usize>) -> (usize, usize) {
    (range.start, range.end)
}
const fn range((start, end): (usize, usize)) -> Range<usize> {
    start..end
}

impl WireFacts {
    pub(super) fn of(facts: &Facts) -> Self {
        let Facts {
            outline,
            regions,
            gated,
            gated_bytes,
            module_bytes,
            imports,
            modules,
        } = facts;
        let (spans, total) = regions.parts();
        Self {
            outline: outline.iter().map(WireItem::of).collect(),
            regions: (spans.to_vec(), total),
            gated: gated.clone(),
            gated_bytes: gated_bytes.iter().map(pair).collect(),
            module_bytes: module_bytes.iter().map(pair).collect(),
            imports: imports.iter().map(WireImport::of).collect(),
            modules: modules.clone(),
        }
    }

    /// The facts this wire form records, or `None` for a tag this build does
    /// not know.
    pub(super) fn into_facts(self) -> Option<Facts> {
        let Self {
            outline,
            regions: (spans, total),
            gated,
            gated_bytes,
            module_bytes,
            imports,
            modules,
        } = self;
        Some(Facts {
            outline: outline
                .into_iter()
                .map(WireItem::into_item)
                .collect::<Option<_>>()?,
            regions: TestRegions::from_parts(spans, total),
            gated,
            gated_bytes: gated_bytes.into_iter().map(range).collect(),
            module_bytes: module_bytes.into_iter().map(range).collect(),
            imports: imports.into_iter().map(WireImport::into_import).collect(),
            modules,
        })
    }
}

impl WireItem {
    fn of(item: &Item) -> Self {
        let Item {
            kind,
            visibility,
            name,
            file,
            line,
            span,
            declaration_block,
            doc,
            top_level,
            syntax_span,
            byte_span,
            attribute_names,
            restricted,
            trait_name,
            fields,
        } = item;
        Self {
            kind: kind_tag(*kind).to_owned(),
            visibility: visibility.tag().to_owned(),
            name: name.clone(),
            file: file.clone(),
            line: *line,
            span: (*span.start(), *span.end()),
            declaration_block: declaration_block.to_string(),
            doc: doc.clone(),
            top_level: *top_level,
            syntax_span: pair(syntax_span),
            byte_span: pair(byte_span),
            attribute_names: attribute_names.clone(),
            restricted: restricted.clone(),
            trait_name: trait_name.clone(),
            fields: fields.iter().map(WireField::of).collect(),
        }
    }
    fn into_item(self) -> Option<Item> {
        let Self {
            kind,
            visibility,
            name,
            file,
            line,
            span,
            declaration_block,
            doc,
            top_level,
            syntax_span,
            byte_span,
            attribute_names,
            restricted,
            trait_name,
            fields,
        } = self;
        Some(Item {
            kind: kind_from_tag(&kind)?,
            visibility: Visibility::from_tag(&visibility)?,
            name,
            file,
            line,
            span: span.0..=span.1,
            declaration_block: Arc::from(declaration_block),
            doc,
            top_level,
            syntax_span: range(syntax_span),
            byte_span: range(byte_span),
            attribute_names,
            restricted,
            trait_name,
            fields: fields
                .into_iter()
                .map(WireField::into_field)
                .collect::<Option<_>>()?,
        })
    }
}

impl WireField {
    fn of(field: &Field) -> Self {
        let Field {
            line_offset,
            name,
            visibility,
            restricted,
        } = field;
        Self {
            line_offset: *line_offset,
            name: name.clone(),
            visibility: visibility.tag().to_owned(),
            restricted: restricted.clone(),
        }
    }
    fn into_field(self) -> Option<Field> {
        Some(Field {
            line_offset: self.line_offset,
            name: self.name,
            visibility: Visibility::from_tag(&self.visibility)?,
            restricted: self.restricted,
        })
    }
}

impl WireImport {
    fn of(import: &Import) -> Self {
        let Import {
            line,
            end_line,
            top_level,
            statement,
            bindings,
            bytes,
            target,
        } = import;
        Self {
            line: *line,
            end_line: *end_line,
            top_level: *top_level,
            statement: statement.to_string(),
            bindings: bindings
                .as_ref()
                .map(|bindings| bindings.iter().map(WireBinding::of).collect())
                .map_err(Clone::clone),
            bytes: pair(bytes),
            target: target.to_string(),
        }
    }
    fn into_import(self) -> Import {
        Import {
            line: self.line,
            end_line: self.end_line,
            top_level: self.top_level,
            statement: Arc::from(self.statement),
            bindings: self.bindings.map(|bindings| {
                bindings
                    .into_iter()
                    .map(WireBinding::into_binding)
                    .collect()
            }),
            bytes: range(self.bytes),
            target: Arc::from(self.target),
        }
    }
}

impl WireBinding {
    fn of(binding: &ImportBinding) -> Self {
        let ImportBinding {
            source,
            source_path,
            local_name,
            module_only,
        } = binding;
        Self {
            source: source.clone(),
            source_path: source_path.clone(),
            local_name: local_name.clone(),
            module_only: *module_only,
        }
    }
    fn into_binding(self) -> ImportBinding {
        ImportBinding {
            source: self.source,
            source_path: self.source_path,
            local_name: self.local_name,
            module_only: self.module_only,
        }
    }
}

/// The wire tag of an item kind. Matching without a wildcard makes a new kind
/// a compile error here.
const fn kind_tag(kind: ItemKind) -> &'static str {
    match kind {
        ItemKind::Struct => "Struct",
        ItemKind::Enum => "Enum",
        ItemKind::Union => "Union",
        ItemKind::Trait => "Trait",
        ItemKind::TypeAlias => "TypeAlias",
        ItemKind::Fn => "Fn",
        ItemKind::Mod => "Mod",
        ItemKind::MacroRules => "MacroRules",
        ItemKind::PubUse => "PubUse",
        ItemKind::Const => "Const",
        ItemKind::Static => "Static",
        ItemKind::Impl => "Impl",
        ItemKind::Use => "Use",
        ItemKind::MacroCall => "MacroCall",
        ItemKind::Extern => "Extern",
    }
}
fn kind_from_tag(tag: &str) -> Option<ItemKind> {
    Some(match tag {
        "Struct" => ItemKind::Struct,
        "Enum" => ItemKind::Enum,
        "Union" => ItemKind::Union,
        "Trait" => ItemKind::Trait,
        "TypeAlias" => ItemKind::TypeAlias,
        "Fn" => ItemKind::Fn,
        "Mod" => ItemKind::Mod,
        "MacroRules" => ItemKind::MacroRules,
        "PubUse" => ItemKind::PubUse,
        "Const" => ItemKind::Const,
        "Static" => ItemKind::Static,
        "Impl" => ItemKind::Impl,
        "Use" => ItemKind::Use,
        "MacroCall" => ItemKind::MacroCall,
        "Extern" => ItemKind::Extern,
        _ => return None,
    })
}
