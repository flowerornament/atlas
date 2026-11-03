//! Portable written-item facts and the native test-module line population.
use std::{
    ops::RangeInclusive,
    path::{Path, PathBuf},
};

/// The item kinds the grammar recognizes. Census policy (which kinds count as
/// census rows, spec §3) belongs to consumers; the scanner reports everything
/// it can name, including `const`/`static` so code anchors keep resolving.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum ItemKind {
    Struct,
    Enum,
    /// A `union`: a type, counted with structs and enums.
    Union,
    Trait,
    TypeAlias,
    Fn,
    Mod,
    MacroRules,
    /// A written public or restricted re-export. Private imports have the
    /// outline-only Use kind; actual bound names live in ImportBinding.
    PubUse,
    Const,
    Static,
    /// A top-level `impl` block, named by its target type (`impl Tile`,
    /// `impl Trait for Tile` → `Tile`). Not a census kind — it exists because
    /// the density grade's denominator is "top-level items" and the worst
    /// god-objects are one giant impl with zero pub types (spec law 2).
    Impl,
    /// A private `use` import. An item to Rust and a line a move must carry,
    /// but never a declaration: only the outline view sees it.
    Use,
    /// A macro invocation (`thread_local! { .. }`), named by its
    /// macro path. Its expansion is the scanner's blind spot; its span is not.
    /// Outline-only, like [`ItemKind::Use`].
    MacroCall,
    /// An `extern crate` or an `extern "ABI" { .. }` block: an item to Rust,
    /// declaring nothing the census counts. Outline-only.
    Extern,
}

impl ItemKind {
    /// Stable schema-1 tag, independent of diagnostic Debug formatting.
    pub const fn tag(self) -> &'static str {
        match self {
            Self::Struct => "Struct",
            Self::Enum => "Enum",
            Self::Union => "Union",
            Self::Trait => "Trait",
            Self::TypeAlias => "TypeAlias",
            Self::Fn => "Fn",
            Self::Mod => "Mod",
            Self::MacroRules => "MacroRules",
            Self::PubUse => "PubUse",
            Self::Const => "Const",
            Self::Static => "Static",
            Self::Impl => "Impl",
            Self::Use => "Use",
            Self::MacroCall => "MacroCall",
            Self::Extern => "Extern",
        }
    }
    /// Decode a supported schema-1 tag. Unknown wire tags remain unknown.
    pub fn from_tag(tag: &str) -> Option<Self> {
        match tag {
            "Struct" => Some(Self::Struct),
            "Enum" => Some(Self::Enum),
            "Union" => Some(Self::Union),
            "Trait" => Some(Self::Trait),
            "TypeAlias" => Some(Self::TypeAlias),
            "Fn" => Some(Self::Fn),
            "Mod" => Some(Self::Mod),
            "MacroRules" => Some(Self::MacroRules),
            "PubUse" => Some(Self::PubUse),
            "Const" => Some(Self::Const),
            "Static" => Some(Self::Static),
            "Impl" => Some(Self::Impl),
            "Use" => Some(Self::Use),
            "MacroCall" => Some(Self::MacroCall),
            "Extern" => Some(Self::Extern),
            _ => None,
        }
    }

    /// Whether the kind declares a name the census, graph and policy count.
    /// The outline-only kinds are items to Rust but declare nothing.
    pub const fn declares(self) -> bool {
        !matches!(self, Self::Use | Self::MacroCall | Self::Extern)
    }
}

/// Declaration visibility: `pub(self)` is private; every other restricted
/// scope is its own rung. The census reads any non-private rung as pub-ish.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum Visibility {
    Pub,
    PubCrate,
    /// `pub(in path)`: wider than `super`, never wider than the crate.
    PubIn,
    PubSuper,
    Private,
}

impl Visibility {
    /// Stable schema-1 tag, independent of diagnostic Debug formatting.
    pub const fn tag(self) -> &'static str {
        match self {
            Self::Pub => "Pub",
            Self::PubCrate => "PubCrate",
            Self::PubIn => "PubIn",
            Self::PubSuper => "PubSuper",
            Self::Private => "Private",
        }
    }
    /// Decode a supported schema-1 tag. Unknown wire tags remain unknown.
    pub fn from_tag(tag: &str) -> Option<Self> {
        match tag {
            "Pub" => Some(Self::Pub),
            "PubCrate" => Some(Self::PubCrate),
            "PubIn" => Some(Self::PubIn),
            "PubSuper" => Some(Self::PubSuper),
            "Private" => Some(Self::Private),
            _ => None,
        }
    }

    /// Native written-visibility ladder; this is not Rust effective visibility.
    pub const fn rank(self) -> u8 {
        match self {
            Self::Private => 0,
            Self::PubSuper => 1,
            Self::PubIn => 2,
            Self::PubCrate => 3,
            Self::Pub => 4,
        }
    }
    /// Written visibility spelling; restricted paths are carried separately.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Private => "private",
            Self::PubSuper => "pub(super)",
            Self::PubIn => "pub(in path)",
            Self::PubCrate => "pub(crate)",
            Self::Pub => "pub",
        }
    }
}

/// One struct field's written surface. Tuple fields use their numeric position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    pub line_offset: usize,
    pub name: String,
    pub visibility: Visibility,
    pub restricted: Option<String>,
}

/// One declared item with its declaration block — the carrier the atlas verbs
/// (spec §6 slices A–B) fold into census rows and co-mention edges.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub kind: ItemKind,
    pub visibility: Visibility,
    pub name: String,
    pub file: PathBuf,
    /// 1-indexed declaration line.
    pub line: usize,
    /// 1-indexed inclusive lines the item occupies: from its first attached
    /// outer doc, attribute or comment line through the brace or semicolon
    /// that closes it. Lines between items (blanks, `//!`, inner attributes)
    /// belong to no span; a move that conserves lines conserves both.
    pub span: RangeInclusive<usize>,
    /// Written interface text for lexical co-mentions. Functions, impls, traits
    /// and inline modules end at their body opener. Other items retain their
    /// full syntax, including fields/variants and opaque macro payloads.
    /// This never substitutes for qualified signature/type observations.
    pub declaration_block: std::sync::Arc<str>,
    /// First line of the item's `///` doc comment — the atlas's comment
    /// surface (spec law 6). `None` is a visible documentation gap, not an
    /// omission of the scanner.
    pub doc: Option<String>,
    /// Whether this is a direct file item, independent of formatting.
    /// Associated members and items nested in modules or bodies are not top-level.
    pub top_level: bool,
    /// Exact syntax bytes, excluding attached comments and attributes.
    pub syntax_span: std::ops::Range<usize>,
    /// Exact bytes including attached outer documentation/comments/attributes.
    pub byte_span: std::ops::Range<usize>,
    pub(crate) attribute_names: std::collections::BTreeSet<String>,
    pub(crate) restricted: Option<String>,
    pub(crate) trait_name: Option<String>,
    pub(crate) fields: Vec<Field>,
}

impl Item {
    /// Written attribute path, independent of spaces, comments and line breaks.
    pub fn has_attribute(&self, path: &str) -> bool {
        self.attribute_names.contains(path)
    }
    pub(crate) fn attribute_text<'a>(&self, source: &'a str) -> &'a str {
        &source[self.byte_span.start..self.syntax_span.start]
    }

    /// Written restricted visibility target, without resolving module ancestry.
    /// `pub(crate)` yields `crate`; `pub(in crate::scope)` yields `crate::scope`.
    pub fn visibility_restriction(&self) -> Option<&str> {
        self.restricted.as_deref()
    }
}

pub struct TestRegions {
    /// Inclusive 1-based line ranges, in source order and non-overlapping.
    spans: Vec<(usize, usize)>,
    total: usize,
}

impl TestRegions {
    pub fn of(text: &str) -> Result<Self, super::SyntaxError> {
        Ok(super::Source::of(Path::new("input.rs"), text.to_owned())?.regions)
    }
    pub(crate) fn new(total: usize, ranges: Vec<RangeInclusive<usize>>) -> Self {
        let mut spans: Vec<_> = ranges.into_iter().map(|r| (*r.start(), *r.end())).collect();
        spans.sort_unstable();
        let mut merged: Vec<(usize, usize)> = Vec::new();
        for (start, end) in spans {
            if let Some(last) = merged.last_mut()
                && start <= last.1
            {
                last.1 = last.1.max(end);
            } else {
                merged.push((start, end));
            }
        }
        Self {
            spans: merged,
            total,
        }
    }

    /// Whether a 1-based line lies in a test region.
    pub fn covers(&self, line: usize) -> bool {
        self.spans
            .iter()
            .any(|&(start, end)| (start..=end).contains(&line))
    }

    /// First test-region start strictly after a one-based source line.
    pub fn next_start_after(&self, line: usize) -> Option<usize> {
        self.spans
            .iter()
            .map(|(start, _)| *start)
            .find(|start| *start > line)
    }

    /// Lines outside every test region.
    pub fn non_test_loc(&self) -> usize {
        let covered: usize = self
            .spans
            .iter()
            .map(|&(start, end)| end.saturating_sub(start) + 1)
            .sum();
        self.total.saturating_sub(covered)
    }
}
