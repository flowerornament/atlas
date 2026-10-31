//! Extracted Atlas declaration scanner; see PROVENANCE.md.
//! Lexical, rustfmt-shaped scanning, not a Rust name resolver.
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
    /// A `pub use` (or `pub(crate)`/`pub(super)` use) — a re-export, never a
    /// declaration; private `use` imports are not items at all.
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

/// One recognized declaration line, borrowing from the scanned line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineItem<'a> {
    pub kind: ItemKind,
    pub visibility: Visibility,
    pub name: &'a str,
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
    /// The declaration text used for co-mention edges: the signature for fns
    /// (through the body-opening brace on multi-line signatures), the body to
    /// the matching closing brace for structs/enums (fields and variants are
    /// the composition edges), a `pub use` through its semicolon, and the
    /// single declaration line otherwise.
    pub declaration_block: std::sync::Arc<str>,
    /// First line of the item's `///` doc comment — the atlas's comment
    /// surface (spec law 6). `None` is a visible documentation gap, not an
    /// omission of the scanner.
    pub doc: Option<String>,
    /// Whether the declaration starts at column 0. Under rustfmt this is the
    /// "top-level item" boundary the density grade counts (spec law 2):
    /// methods inside an impl and items inside inline modules are indented
    /// and therefore not top-level.
    pub top_level: bool,
}

impl Item {
    /// Written restricted visibility target, without resolving module ancestry.
    /// `pub(crate)` yields `crate`; `pub(in crate::scope)` yields `crate::scope`.
    pub fn visibility_restriction(&self) -> Option<&str> {
        let (_, _, scope) = visibility_parts(self.declaration_block.trim_start());
        let scope = scope?;
        Some(scope.strip_prefix("in ").unwrap_or(scope).trim())
    }
}

/// Identifier-bounded tokens shared by the item and relationship grammars.
pub(crate) fn identifiers(text: &str) -> impl Iterator<Item = &str> {
    text.split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
        .filter(|token| {
            !token.is_empty() && !token.starts_with(|character: char| character.is_ascii_digit())
        })
}

/// Every test-only region of one file: each top-level inline module whose
/// `cfg` gate implies `test`, whatever the module is named.
///
/// This is the one test boundary authority, shared by Atlas census, delta, and
/// the concentration report. It is a set of regions rather than a
/// single terminal line because test code is not always last: a file may open
/// a `loom_tests` module, return to production code, and close with `tests`.
/// A single boundary counted every such interleaved module as production
/// (~3.5k lines workspace-wide, `murail-8tlip`).
///
/// Test-only *helpers* outside a module (`#[cfg(test)] fn helper()`) stay
/// counted as production, deliberately: gating one item is a property of that
/// item, and treating it as a boundary hides the production code after it
/// (the replicate.rs regression class).
pub struct TestRegions {
    /// Inclusive 1-based line ranges, in source order and non-overlapping.
    spans: Vec<(usize, usize)>,
    total: usize,
}

impl TestRegions {
    pub fn of(text: &str) -> Self {
        RustSourceProjection::of(text.to_owned()).test_regions()
    }

    fn from_projection(projected: &RustSourceProjection) -> Self {
        let lines: Vec<&str> = projected.text().lines().collect();
        let mut spans = Vec::new();
        let mut index = 0;
        while index < lines.len() {
            let opened = (projected.lines[index].starts_in_code && gates_test(lines[index]))
                .then(|| declaration_after_attributes(&lines, index))
                .flatten()
                .filter(|&start| opens_inline_module(lines[start]));
            match opened {
                Some(start) => {
                    let end = closing_line(projected.projected_lines(), start);
                    spans.push((index + 1, end + 1));
                    index = end + 1;
                }
                None => index += 1,
            }
        }
        Self {
            spans,
            total: lines.len(),
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

/// Non-test LOC of one file — the `TestRegions` count, exposed for the
/// `nontest-loc` verb and Atlas delta.
pub fn non_test_loc(text: &str) -> usize {
    TestRegions::of(text).non_test_loc()
}

/// The file modules (`mod name;`) a file declares under a test-implying gate,
/// paired with any explicit `#[path = "..."]` attribute.
/// Their whole file is test code, which no single-file scan can know — the
/// atlas resolves these against the crate's file list.
#[cfg(test)]
pub(crate) fn test_module_declarations(text: &str) -> Vec<(String, Option<String>)> {
    crate::source::Source::of(Path::new("input.rs"), text.to_owned())
        .file_module_declarations()
        .iter()
        .filter(|(_, _, test_only)| *test_only)
        .cloned()
        .map(|(name, path, _)| (name, path))
        .collect()
}

/// Whether a line is a top-level attribute whose `cfg` predicate implies
/// `test`. Column 0 keeps this to the file's own items, matching
/// [`Item::top_level`].
fn gates_test(line: &str) -> bool {
    line.strip_prefix("#[cfg(")
        .and_then(|rest| rest.strip_suffix(")]"))
        .is_some_and(implies_test)
}

/// Whether an outer attribute stack gates its item to test builds.
/// Unlike [`TestRegions`], this is a property of that item alone.
fn attributes_require_test(line: &str) -> bool {
    attributes_require_test_or_features(line, &[])
}
/// Whether an attribute stack restricts an item to tests or caller-declared support features.
/// This proves the supported all/any forms; opaque predicates never earn an exemption.
pub fn attributes_require_test_or_features(line: &str, features: &[&str]) -> bool {
    use syn::parse::Parser;
    let parser = |input: syn::parse::ParseStream<'_>| {
        let attributes = syn::Attribute::parse_outer(input)?;
        let _: proc_macro2::TokenStream = input.parse()?;
        Ok(attributes)
    };
    parser.parse_str(line).is_ok_and(|attributes| attributes.iter().any(|attribute| {
        attribute.path().is_ident("test") || matches!(&attribute.meta,
            syn::Meta::List(meta) if attribute.path().is_ident("cfg") && implies_test_or_features(&meta.tokens.to_string(),features))
    }))
}

/// Whether a `cfg` predicate implies `test` — the condition under which the
/// gated item exists *only* in test builds. This reads the predicate's own
/// boolean semantics rather than its spelling, which is what distinguishes
/// `all(test, feature = "loom-tests")` (test-only) from `any(test, feature =
/// "test-strategies")` (also compiled for that feature, so production) and
/// from `not(all(test, ..))` (the negation, production). Both shapes are live
/// in the workspace.
fn implies_test(predicate: &str) -> bool {
    implies_test_or_features(predicate, &[])
}
fn implies_test_or_features(predicate: &str, features: &[&str]) -> bool {
    let predicate = predicate.trim();
    if let Some(arguments) = strip_call(predicate, "all") {
        return split_top_level(arguments)
            .into_iter()
            .any(|argument| implies_test_or_features(argument, features));
    }
    if let Some(arguments) = strip_call(predicate, "any") {
        return split_top_level(arguments)
            .into_iter()
            .all(|argument| implies_test_or_features(argument, features));
    }
    if predicate == "test" {
        return true;
    }
    if !features.is_empty()
        && let Some((name, value)) = predicate.split_once('=')
        && name.trim() == "feature"
        && let Ok(value) = syn::parse_str::<syn::LitStr>(value.trim())
    {
        return features.contains(&value.value().as_str());
    }
    false
}

/// The arguments of `name(..)`, or `None` when the predicate is not that call.
fn strip_call<'a>(predicate: &'a str, name: &str) -> Option<&'a str> {
    predicate
        .strip_prefix(name)?
        .trim_start()
        .strip_prefix('(')?
        .strip_suffix(')')
}

/// The first non-attribute line at or after `index`, skipping the attribute
/// stack (`#[cfg(test)] #[allow(..)] mod tests {`).
fn declaration_after_attributes(lines: &[&str], index: usize) -> Option<usize> {
    (index..lines.len()).find(|&next| !lines[next].starts_with("#["))
}

fn module_path_attribute(line: &str) -> Option<&str> {
    line.strip_prefix("#[path")?
        .trim_start()
        .strip_prefix('=')?
        .trim()
        .strip_suffix(']')?
        .trim()
        .strip_prefix('"')?
        .strip_suffix('"')
}

fn opens_inline_module(line: &str) -> bool {
    module_name(line).is_some_and(|rest| rest.trim_start().starts_with('{'))
}

fn declares_file_module(line: &str) -> Option<&str> {
    let rest = module_name(line)?;
    rest.trim_start().starts_with(';').then(|| {
        line[..line.len() - rest.len()]
            .trim_end()
            .rsplit(' ')
            .next()
            .unwrap_or_default()
    })
}

/// The text following `mod <name>` on a top-level module line, or `None` when
/// the line declares no module.
fn module_name(line: &str) -> Option<&str> {
    let (_, rest) = split_visibility(line);
    let after = keyword(rest, "mod")?;
    let end = after
        .find(|character: char| !character.is_alphanumeric() && character != '_')
        .unwrap_or(after.len());
    (end > 0).then(|| &after[end..])
}

/// The line closing a top-level block opened at `start` — under rustfmt's
/// column-0 discipline, the next bare `}`.
fn closing_line(lines: &[ProjectedLine], start: usize) -> usize {
    (start + 1..lines.len())
        .find(|&index| lines[index].code == "}")
        .unwrap_or(lines.len().saturating_sub(1))
}

/// The `//!` module-header lines (content after `//!`, trimmed). The first
/// entry is the module's orientation line; the joined block is the comment
/// audit's citation surface (spec law 6). Collected file-wide — inner docs of
/// nested inline modules are rare enough to leave to the heuristic.
#[cfg(test)]
pub(crate) fn module_header_lines(text: &str) -> Vec<String> {
    RustSourceProjection::of(text.to_owned()).module_header_lines()
}

/// Scan Rust source text into its declared items, attributing them to `file`.
///
/// Doc attribution follows rustdoc's shape line-wise: a `///` run attaches to
/// the next declaration through any attributes, comments, and blank lines; any
/// other intervening line (a statement, an inner attribute) detaches it.
#[cfg(test)]
pub(crate) fn scan_source(text: &str, file: &Path) -> Vec<Item> {
    let mut items = RustSourceProjection::of(text.to_owned()).outline(file);
    items.retain(|item| item.kind.declares());
    items
}

/// Every item of one file, outline-only kinds included, in source order.
///
/// Span starts follow the same forward walk as doc attribution: an outer doc,
/// attribute or plain `//` comment opens the attached run; a blank line closes
/// a run of plain comments only, because rustdoc attaches docs across blanks.
fn scan_projected_source(file: &Path, projected: &RustSourceProjection) -> Vec<Item> {
    let lines: Vec<&str> = projected.text().lines().collect();
    let mut items = Vec::new();
    let mut pending_doc: Option<String> = None;
    let mut attached_from: Option<usize> = None;
    let mut attached_outer = false;
    let mut attr_depth = 0usize;
    for (index, line) in lines.iter().enumerate() {
        let code_line = projected.lines[index].code.as_str();
        let trimmed = line.trim_start();
        if attr_depth > 0 {
            attr_depth = next_attr_depth(attr_depth, code_line.trim_start());
            continue;
        }
        if let Some(doc) = projected.outer_doc_comment(index, line) {
            // First line of the run is the doc summary the atlas displays.
            pending_doc.get_or_insert_with(|| doc.trim().to_owned());
            attached_from.get_or_insert(index);
            attached_outer = true;
            continue;
        }
        if code_line.trim_start().starts_with("#[") {
            attr_depth = next_attr_depth(0, code_line.trim_start());
            attached_from.get_or_insert(index);
            attached_outer = true;
            continue;
        }
        if trimmed.is_empty() {
            if !attached_outer {
                attached_from = None;
            }
            continue;
        }
        if trimmed.starts_with("//") && !trimmed.starts_with("//!") {
            if projected.lines[index].starts_in_code {
                attached_from.get_or_insert(index);
            }
            continue;
        }
        if code_line.trim().is_empty() {
            continue;
        }
        let first_line = attached_from.take().unwrap_or(index) + 1;
        attached_outer = false;
        let Some(found) = classify_line(code_line) else {
            pending_doc = None;
            continue;
        };
        let declaration_block = match found.kind {
            ItemKind::Struct | ItemKind::Enum => {
                braced_block(&lines, projected.projected_lines(), index)
            }
            ItemKind::PubUse | ItemKind::Use => semicolon_block(&lines, index),
            // Impl blocks capture the header only (through the opening
            // brace): bodies are excluded by design — signatures are the
            // intentional interface (spec law 4) — and the methods inside are
            // scanned as their own (non-top-level) items.
            ItemKind::Fn | ItemKind::Impl => {
                signature_block(&lines, projected.projected_lines(), index)
            }
            _ => (*line).to_owned(),
        };
        items.push(Item {
            kind: found.kind,
            visibility: found.visibility,
            name: found.name.to_owned(),
            file: file.to_path_buf(),
            line: index + 1,
            span: first_line..=item_end(projected.projected_lines(), index) + 1,
            declaration_block: declaration_block.into(),
            doc: pending_doc.take(),
            top_level: !line.starts_with([' ', '\t']),
        });
    }
    items
}

/// The 0-based line closing the item declared at `start`: a `;` outside
/// every bracket, or the line where the first `{` group (with any enclosing
/// brackets) closes. Counting runs over the lexical code projection, so
/// strings, chars and comments cannot open or close a span; an unterminated
/// item degrades to the end of the file.
fn item_end(code: &[ProjectedLine], start: usize) -> usize {
    let mut depth = 0usize;
    let mut braced = false;
    for (offset, projected) in code[start..].iter().enumerate() {
        for ch in projected.code.chars() {
            match ch {
                '{' => {
                    depth += 1;
                    braced = true;
                }
                '(' | '[' => depth += 1,
                '}' | ')' | ']' => depth = depth.saturating_sub(1),
                ';' if depth == 0 => return start + offset,
                _ => {}
            }
        }
        if braced && depth == 0 {
            return start + offset;
        }
    }
    code.len().saturating_sub(1)
}

/// Track how deep inside a (possibly multi-line) `#[...]` attribute the scan
/// is, by square-bracket counting over the lexical code projection.
fn next_attr_depth(depth: usize, line: &str) -> usize {
    line.chars().fold(depth, |depth, ch| match ch {
        '[' => depth + 1,
        ']' => depth.saturating_sub(1),
        _ => depth,
    })
}

/// Capture a struct or enum declaration through its matching closing brace.
///
/// Unit and tuple forms stop at their terminating `;`. Brace counting is
/// textual; an unterminated block degrades to the rest of the file.
fn braced_block(lines: &[&str], code: &[ProjectedLine], start: usize) -> String {
    let mut depth = 0usize;
    let mut opened = false;
    let mut end = start;
    'outer: for (offset, projected) in code[start..].iter().enumerate() {
        end = start + offset;
        for ch in projected.code.chars() {
            match ch {
                '{' => {
                    depth += 1;
                    opened = true;
                }
                '}' => {
                    depth = depth.saturating_sub(1);
                    if opened && depth == 0 {
                        break 'outer;
                    }
                }
                ';' if !opened => break 'outer,
                _ => {}
            }
        }
    }
    lines[start..=end].join("\n")
}

/// Capture a function signature through its body-opening `{` or terminating `;`.
///
/// This preserves the whole signature when rustfmt breaks it across lines.
fn signature_block(lines: &[&str], code: &[ProjectedLine], start: usize) -> String {
    let mut end = start;
    for (offset, projected) in code[start..].iter().enumerate() {
        end = start + offset;
        if projected.code.contains('{') || projected.code.contains(';') {
            break;
        }
    }
    lines[start..=end].join("\n")
}

#[derive(Debug)]
struct ProjectedLine {
    code: String,
    /// Whether this line begins outside a continued string or block comment.
    starts_in_code: bool,
}

/// One lexical view of Rust source shared by every line-based scan.
pub struct RustSourceProjection {
    text: String,
    lines: Vec<ProjectedLine>,
}

pub(crate) struct TestGate {
    pub span: RangeInclusive<usize>,
    pub following_item: bool,
}

impl RustSourceProjection {
    pub fn of(text: String) -> Self {
        Self {
            lines: code_projection(&text),
            text,
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    /// Every item Rust sees in the file — declarations, private imports and
    /// column-0 macro invocations — each with its line span. Nesting follows
    /// span containment (methods inside their impl).
    pub fn outline(&self, file: &Path) -> Vec<Item> {
        scan_projected_source(file, self)
    }

    pub fn test_regions(&self) -> TestRegions {
        TestRegions::from_projection(self)
    }

    /// Masked brackets bound attributes, so strings/comments cannot end them.
    /// Parse the original text, retaining multiline cfg and trailing comments.
    pub(crate) fn test_gates(&self) -> Vec<TestGate> {
        let raw: Vec<_> = self.text.lines().collect();
        let mut gates = Vec::new();
        let mut index = 0;
        while index < self.lines.len() {
            let start = index;
            let line = &self.lines[index];
            if line.starts_in_code && line.code.trim_start().starts_with("#[") {
                let mut depth = next_attr_depth(0, &line.code);
                while depth > 0 && index + 1 < self.lines.len() {
                    index += 1;
                    depth = next_attr_depth(depth, &self.lines[index].code);
                }
                if attributes_require_test(&raw[start..=index].join("\n")) {
                    let code = self.lines[start..=index]
                        .iter()
                        .map(|line| line.code.as_str())
                        .collect::<Vec<_>>()
                        .join("\n");
                    gates.push(TestGate {
                        span: start + 1..=index + 1,
                        following_item: skip_attributes(&code).is_empty(),
                    });
                }
            }
            index += 1;
        }
        gates
    }

    /// File children, explicit mounts, and whether the declaration's gate
    /// implies test. A file may have several mounts; the workspace resolves
    /// their inherited boundaries together.
    pub(crate) fn file_module_sites(&self) -> Vec<(usize, String, Option<String>)> {
        let lines: Vec<_> = self.text.lines().collect();
        (0..lines.len())
            .filter(|&index| self.lines[index].starts_in_code)
            .filter_map(|index| {
                let name = declares_file_module(lines[index])?.to_owned();
                let mut attribute_start = index;
                while attribute_start > 0 && lines[attribute_start - 1].starts_with("#[") {
                    attribute_start -= 1;
                }
                let path = lines[attribute_start..index]
                    .iter()
                    .find_map(|line| module_path_attribute(line))
                    .map(str::to_owned);
                Some((index + 1, name, path))
            })
            .collect()
    }

    fn projected_lines(&self) -> &[ProjectedLine] {
        &self.lines
    }

    /// Recognized declarations, with one-based source lines.
    pub fn declaration_lines(&self) -> impl Iterator<Item = (usize, LineItem<'_>)> {
        self.code_lines().enumerate().filter_map(|(index, line)| {
            classify_line(line)
                .filter(|item| item.kind.declares())
                .map(|item| (index + 1, item))
        })
    }
    /// Full-line comments in code, excluding comment-shaped string contents.
    pub fn line_comment_lines(&self) -> impl Iterator<Item = (usize, &str)> {
        self.text
            .lines()
            .enumerate()
            .filter(|(index, line)| {
                self.lines[*index].starts_in_code && line.trim_start().starts_with("//")
            })
            .map(|(index, line)| (index + 1, line))
    }
    /// Whether this zero-based line starts in lexical code.
    pub fn starts_in_code(&self, index: usize) -> bool {
        self.lines
            .get(index)
            .is_some_and(|line| line.starts_in_code)
    }
    pub fn code_lines(&self) -> impl Iterator<Item = &str> {
        self.lines.iter().map(|line| line.code.as_str())
    }

    /// Lexical //! documentation, with one-based line witnesses.
    pub fn module_header_sites(&self) -> impl Iterator<Item = (usize, &str)> {
        self.text
            .lines()
            .zip(&self.lines)
            .enumerate()
            .filter(|(_, (_, projected))| projected.starts_in_code)
            .filter_map(|(index, (line, _))| {
                line.trim_start()
                    .strip_prefix("//!")
                    .map(|text| (index + 1, text.trim()))
            })
    }

    /// Lexical //! documentation lines, including nested module docs.
    pub fn module_header_lines(&self) -> Vec<String> {
        self.module_header_sites()
            .map(|(_, text)| text.to_owned())
            .collect()
    }

    fn outer_doc_comment<'a>(&self, index: usize, line: &'a str) -> Option<&'a str> {
        self.lines[index]
            .starts_in_code
            .then(|| line.trim_start().strip_prefix("///"))
            .flatten()
    }
}

#[derive(Debug, Clone, Copy)]
enum LexicalState {
    Code,
    BlockComment(usize),
    String { escaped: bool },
    RawString { hashes: usize },
}

/// Project Rust source onto code bytes while preserving line and byte offsets.
///
/// This is deliberately smaller than a parser: it recognizes only comments
/// and string literals, the two lexical forms that can counterfeit declaration
/// lines or braces for the line grammar.
fn code_projection(text: &str) -> Vec<ProjectedLine> {
    let bytes = text.as_bytes();
    let mut state = LexicalState::Code;
    let mut starts_in_code = true;
    let mut line = Vec::new();
    let mut lines = Vec::new();
    let mut index = 0usize;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte == b'\n' {
            lines.push(ProjectedLine {
                code: String::from_utf8(line).expect("masked source remains UTF-8"),
                starts_in_code,
            });
            line = Vec::new();
            starts_in_code = matches!(state, LexicalState::Code);
            index += 1;
            continue;
        }
        match state {
            LexicalState::Code if bytes[index..].starts_with(b"//") => {
                while index < bytes.len() && bytes[index] != b'\n' {
                    line.push(b' ');
                    index += 1;
                }
            }
            LexicalState::Code if bytes[index..].starts_with(b"/*") => {
                line.extend_from_slice(b"  ");
                state = LexicalState::BlockComment(1);
                index += 2;
            }
            LexicalState::Code => {
                if let Some((prefix_len, hashes)) = raw_string_open(&bytes[index..]) {
                    line.extend(std::iter::repeat_n(b' ', prefix_len));
                    state = LexicalState::RawString { hashes };
                    index += prefix_len;
                } else if let Some(literal_len) = (byte == b'\'')
                    .then(|| char_literal_len(&text[index..]))
                    .flatten()
                {
                    line.extend(std::iter::repeat_n(b' ', literal_len));
                    index += literal_len;
                } else if byte == b'"' {
                    line.push(b' ');
                    state = LexicalState::String { escaped: false };
                    index += 1;
                } else {
                    line.push(byte);
                    index += 1;
                }
            }
            LexicalState::BlockComment(depth) if bytes[index..].starts_with(b"/*") => {
                line.extend_from_slice(b"  ");
                state = LexicalState::BlockComment(depth + 1);
                index += 2;
            }
            LexicalState::BlockComment(depth) if bytes[index..].starts_with(b"*/") => {
                line.extend_from_slice(b"  ");
                state = if depth == 1 {
                    LexicalState::Code
                } else {
                    LexicalState::BlockComment(depth - 1)
                };
                index += 2;
            }
            LexicalState::BlockComment(_) => {
                line.push(b' ');
                index += 1;
            }
            LexicalState::String { escaped } => {
                line.push(b' ');
                state = if escaped {
                    LexicalState::String { escaped: false }
                } else if byte == b'\\' {
                    LexicalState::String { escaped: true }
                } else if byte == b'"' {
                    LexicalState::Code
                } else {
                    state
                };
                index += 1;
            }
            LexicalState::RawString { hashes } => {
                let closes = byte == b'"'
                    && bytes
                        .get(index + 1..index + 1 + hashes)
                        .is_some_and(|suffix| suffix.iter().all(|byte| *byte == b'#'));
                line.push(b' ');
                index += 1;
                if closes {
                    line.extend(std::iter::repeat_n(b' ', hashes));
                    index += hashes;
                    state = LexicalState::Code;
                }
            }
        }
    }
    if !line.is_empty() {
        lines.push(ProjectedLine {
            code: String::from_utf8(line).expect("masked source remains UTF-8"),
            starts_in_code,
        });
    }
    lines
}

fn raw_string_open(bytes: &[u8]) -> Option<(usize, usize)> {
    let prefix = if bytes.starts_with(b"br") {
        2
    } else if bytes.starts_with(b"r") {
        1
    } else {
        return None;
    };
    let hashes = bytes[prefix..]
        .iter()
        .take_while(|byte| **byte == b'#')
        .count();
    (bytes.get(prefix + hashes) == Some(&b'"')).then_some((prefix + hashes + 1, hashes))
}

fn char_literal_len(text: &str) -> Option<usize> {
    let body = text.strip_prefix('\'')?;
    let content_len = if let Some(escaped) = body.strip_prefix('\\') {
        if let Some(unicode) = escaped.strip_prefix("u{") {
            unicode.find('}')? + 4
        } else if escaped.starts_with('x') {
            4
        } else {
            escaped.chars().next()?.len_utf8() + 1
        }
    } else {
        body.chars().next()?.len_utf8()
    };
    (body.as_bytes().get(content_len) == Some(&b'\'')).then_some(content_len + 2)
}

fn semicolon_block(lines: &[&str], start: usize) -> String {
    let mut end = start;
    for (offset, line) in lines[start..].iter().enumerate() {
        end = start + offset;
        if line.contains(';') {
            break;
        }
    }
    lines[start..=end].join("\n")
}

/// One local name introduced by a captured `use` declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportBinding {
    pub source: String,
    pub source_path: String,
    pub local_name: String,
    /// A braced `self` (`use a::b::{self};`) binds only the module `b`, never
    /// a same-named value or macro.
    pub module_only: bool,
}

/// Extract bindings from a scanner-owned `use` block.
///
/// Nested groups are flattened; globs remain explicit unresolved entries
/// instead of disappearing from downstream inventories.
pub fn import_bindings(declaration: &str) -> Vec<ImportBinding> {
    try_import_bindings(declaration).unwrap_or_default()
}
pub(crate) fn try_import_bindings(declaration: &str) -> Result<Vec<ImportBinding>, syn::Error> {
    syn::parse_str::<syn::ItemUse>(declaration).map(|item| item_use_bindings(&item))
}

/// The native stage reader's use prefix, through the shared visibility grammar.
pub(crate) fn use_import(line: &str) -> Option<&str> {
    let (_, rest) = split_visibility(line.trim_start());
    rest.strip_prefix("use ").map(str::trim_start)
}

pub(crate) fn item_use_bindings(item: &syn::ItemUse) -> Vec<ImportBinding> {
    let mut bindings = use_tree_bindings(&item.tree);
    if item.leading_colon.is_some() {
        for binding in &mut bindings {
            binding.source_path.insert_str(0, "::");
        }
    }
    bindings
}

/// One use-tree grammar for lexical bindings and qualified import projections.
pub(crate) fn use_tree_bindings(tree: &syn::UseTree) -> Vec<ImportBinding> {
    fn walk(tree: &syn::UseTree, prefix: &[String], out: &mut Vec<ImportBinding>) {
        match tree {
            syn::UseTree::Path(path) => {
                let mut prefix = prefix.to_vec();
                prefix.push(path.ident.to_string());
                walk(&path.tree, &prefix, out);
            }
            syn::UseTree::Group(group) => {
                for tree in &group.items {
                    walk(tree, prefix, out);
                }
            }
            leaf => {
                let (source, alias, module_only) = match leaf {
                    syn::UseTree::Name(name) => {
                        (name.ident.to_string(), None, name.ident == "self")
                    }
                    syn::UseTree::Rename(name) => (
                        name.ident.to_string(),
                        Some(name.rename.to_string()),
                        name.ident == "self",
                    ),
                    syn::UseTree::Glob(_) => ("*".into(), None, false),
                    _ => return,
                };
                let mut path = prefix.to_vec();
                if source != "self" {
                    path.push(source);
                }
                if let Some(source) = path.last() {
                    out.push(ImportBinding {
                        source: source.clone(),
                        source_path: path.join("::"),
                        local_name: alias.unwrap_or_else(|| source.clone()),
                        module_only,
                    });
                }
            }
        }
    }
    let mut out = Vec::new();
    walk(tree, &[], &mut out);
    out.sort_by(|a, b| (&a.local_name, &a.source_path).cmp(&(&b.local_name, &b.source_path)));
    out.dedup();
    out
}

/// Split on commas outside every bracket family and string literal — the one
/// argument splitter, shared by the struct/enum member scan and the `cfg`
/// predicate reader. Tuple variants (`Stop(u32, u32)`) and string arguments
/// (`feature = "a,b"`) therefore stay whole.
pub(crate) fn split_top_level(text: &str) -> Vec<&str> {
    let mut depth = 0usize;
    let mut quoted = false;
    let mut start = 0usize;
    let mut parts = Vec::new();
    for (index, byte) in text.bytes().enumerate() {
        match byte {
            b'"' => quoted = !quoted,
            b'{' | b'(' | b'[' if !quoted => depth += 1,
            b'}' | b')' | b']' if !quoted => depth = depth.saturating_sub(1),
            b',' if depth == 0 && !quoted => {
                parts.push(&text[start..index]);
                start = index + 1;
            }
            _ => {}
        }
    }
    parts.push(&text[start..]);
    parts
}

/// Classify one source line as an item declaration — the grammar authority.
///
/// Handles fn qualifiers in any grammatical position (`pub const async unsafe
/// extern "C" fn`), distinguishing `const fn` (a fn) from `const NAME:` (a
/// const item) by lookahead — the keyword-order bug this extraction retires.
pub(crate) fn classify_line(line: &str) -> Option<LineItem<'_>> {
    let rest = line.trim_start();
    if rest.starts_with("//") || rest.starts_with("#[") {
        return None;
    }
    let (visibility, rest) = split_visibility(rest);

    if let Some(after) = rest.strip_prefix("macro_rules!") {
        return named(ItemKind::MacroRules, visibility, after.trim_start());
    }
    if let Some(after) = keyword(rest, "use") {
        let kind = if visibility == Visibility::Private {
            ItemKind::Use
        } else {
            ItemKind::PubUse
        };
        return Some(LineItem {
            kind,
            visibility,
            name: use_target(after),
        });
    }
    if let Some(path) = macro_invocation(rest) {
        return Some(LineItem {
            kind: ItemKind::MacroCall,
            visibility,
            name: path,
        });
    }

    // Fn qualifiers: `const? async? unsafe? extern "abi"?` before `fn`. A lone
    // `const` not followed by another qualifier or `fn` opens a const item.
    // `impl` is checked inside the loop so `unsafe impl Send for T` resolves.
    let mut rest = rest;
    loop {
        if let Some(after) = keyword(rest, "fn") {
            return named(ItemKind::Fn, visibility, after);
        }
        if let Some(after) = rest
            .strip_prefix("impl")
            .filter(|after| after.starts_with(char::is_whitespace) || after.starts_with('<'))
        {
            return Some(LineItem {
                kind: ItemKind::Impl,
                visibility,
                name: impl_target(after.trim_start()),
            });
        }
        if let Some(after) = keyword(rest, "async").or_else(|| keyword(rest, "unsafe")) {
            rest = after;
            continue;
        }
        if let Some(after) = keyword(rest, "extern") {
            let (abi, tail) = after
                .strip_prefix('"')
                .and_then(|abi| abi.split_once('"'))
                .map_or(("C", after), |(abi, tail)| (abi, tail.trim_start()));
            if tail.starts_with('{') {
                return Some(LineItem {
                    kind: ItemKind::Extern,
                    visibility,
                    name: abi,
                });
            }
            if let Some(name) = keyword(tail, "crate") {
                return named(ItemKind::Extern, visibility, name);
            }
            rest = tail;
            continue;
        }
        if let Some(after) = keyword(rest, "const") {
            if ["fn", "async", "unsafe", "extern"]
                .iter()
                .any(|qualifier| keyword(after, qualifier).is_some())
            {
                rest = after;
                continue;
            }
            return named(ItemKind::Const, visibility, after);
        }
        break;
    }

    for (word, kind) in [
        ("struct", ItemKind::Struct),
        ("enum", ItemKind::Enum),
        ("union", ItemKind::Union),
        ("trait", ItemKind::Trait),
        ("type", ItemKind::TypeAlias),
        ("mod", ItemKind::Mod),
        ("static", ItemKind::Static),
    ] {
        if let Some(after) = keyword(rest, word) {
            let after = if kind == ItemKind::Static {
                keyword(after, "mut").unwrap_or(after)
            } else {
                after
            };
            return named(kind, visibility, after);
        }
    }
    None
}

/// The macro path of an item-position invocation (`thread_local! {`,
/// `std::thread_local!(`), or `None` when the line opens no invocation.
fn macro_invocation(rest: &str) -> Option<&str> {
    let end = rest
        .find(|ch: char| !(ch == ':' || ch == '_' || ch.is_ascii_alphanumeric()))
        .filter(|&end| end > 0)?;
    let path = &rest[..end];
    let opens = rest[end..]
        .strip_prefix('!')?
        .trim_start()
        .starts_with(['{', '(', '[']);
    (opens && path.starts_with(|ch: char| ch == '_' || ch.is_ascii_alphabetic())).then_some(path)
}

/// Strip a leading `word` when whitespace-delimited, returning the trimmed
/// remainder — the keyword boundary rule for the whole grammar.
fn keyword<'a>(rest: &'a str, word: &str) -> Option<&'a str> {
    let after = rest.strip_prefix(word)?;
    after
        .chars()
        .next()
        .is_some_and(char::is_whitespace)
        .then(|| after.trim_start())
}

fn named(kind: ItemKind, visibility: Visibility, rest: &str) -> Option<LineItem<'_>> {
    let raw = rest.strip_prefix("r#").unwrap_or(rest);
    let end = raw
        .find(|ch: char| !(ch == '_' || ch.is_ascii_alphanumeric()))
        .unwrap_or(raw.len());
    let name = &raw[..end];
    (!name.is_empty()).then_some(LineItem {
        kind,
        visibility,
        name,
    })
}

/// The impl target's head type name: the type after `for` in a trait impl,
/// the subject type otherwise (`impl<T> Grade<T> for Tile<T>` → `Tile`).
/// Falls back to the trait when the target has no head identifier
/// (`impl Marker for (A, B)` → `Marker`), then to the literal `impl` — the
/// block still counts as a top-level item even when it cannot be named.
fn impl_target(after: &str) -> &str {
    let rest = skip_generics(after);
    let (trait_side, subject) = match split_top_level_for(rest) {
        Some((trait_side, subject)) => (Some(trait_side), subject),
        None => (None, rest),
    };
    type_head(subject)
        .or_else(|| trait_side.and_then(type_head))
        .unwrap_or("impl")
}

/// Skip a leading `<...>` generic-parameter list by angle-bracket counting.
/// `>` preceded by `-` is a return arrow inside a bound (`Fn() -> T`), not a
/// closer. Textual by design; an unterminated list degrades to empty.
fn skip_generics(rest: &str) -> &str {
    let Some(inner) = rest.strip_prefix('<') else {
        return rest;
    };
    let mut depth = 1usize;
    let mut previous = '<';
    for (index, ch) in inner.char_indices() {
        match ch {
            '<' => depth += 1,
            '>' if previous != '-' => {
                depth -= 1;
                if depth == 0 {
                    return inner[index + 1..].trim_start();
                }
            }
            _ => {}
        }
        previous = ch;
    }
    ""
}

/// Split `Trait for Target` at the ` for ` outside any bracket nesting.
/// The trait an impl header implements (`impl<T> Grade<T> for Tile<T> {` →
/// `Grade`), or `None` for an inherent impl. Trait-impl members inherit the
/// trait's visibility and carry none of their own.
pub fn impl_trait(header: &str) -> Option<&str> {
    let after = header.trim_start();
    let after = keyword(after, "unsafe").unwrap_or(after);
    let rest = after.strip_prefix("impl")?;
    let (trait_side, _) = split_top_level_for(skip_generics(rest.trim_start()))?;
    Some(type_head(trait_side.trim_start_matches('!')).unwrap_or("impl"))
}

/// The fields of a struct, read from its lexically projected declaration
/// (`declaration` starts at the `struct` line): `(line offset, name,
/// visibility, pub(in) path)` per field, tuple fields named by position. Unit structs have
/// none. Generic brackets, and parentheses inside a `where` clause, nest like
/// angles, so the body is the first `{` or `(` outside them.
pub fn struct_fields(declaration: &str) -> Vec<(usize, String, Visibility, Option<String>)> {
    let (_, rest) = split_visibility(declaration.trim_start());
    let Some(after_keyword) = keyword(rest, "struct") else {
        return Vec::new();
    };
    let head = declaration.len() - after_keyword.len();
    let mut angle = 0usize;
    let mut previous = ' ';
    let mut in_where = false;
    let mut open = None;
    for (index, ch) in declaration
        .char_indices()
        .skip_while(|(index, _)| *index < head)
    {
        if angle == 0 && keyword(&declaration[index..], "where").is_some() {
            in_where |= !previous.is_ascii_alphanumeric() && previous != '_';
        }
        match ch {
            '<' => angle += 1,
            '(' if in_where => angle += 1,
            ')' if in_where => angle = angle.saturating_sub(1),
            '>' if previous != '-' => angle = angle.saturating_sub(1),
            '{' | '(' if angle == 0 => {
                open = Some((index, ch));
                break;
            }
            ';' if angle == 0 => break,
            _ => {}
        }
        previous = ch;
    }
    let Some((open, opener)) = open else {
        return Vec::new();
    };
    let mut depth = 0usize;
    let mut close = declaration.len();
    for (index, ch) in declaration[open..].char_indices() {
        match ch {
            '{' | '(' | '[' => depth += 1,
            '}' | ')' | ']' => {
                depth -= 1;
                if depth == 0 {
                    close = open + index;
                    break;
                }
            }
            _ => {}
        }
    }
    let body = &declaration[open + 1..close];
    let mut fields = Vec::new();
    let mut consumed = open + 1;
    for (position, segment) in split_top_level(body).into_iter().enumerate() {
        let segment_start = consumed;
        consumed += segment.len() + 1;
        let field = skip_attributes(segment);
        if field.trim().is_empty() {
            continue;
        }
        let offset = declaration[..segment_start + (segment.len() - field.len())]
            .matches('\n')
            .count();
        let (visibility, rest) = split_visibility(field);
        let name = if opener == '{' {
            match rest.split_once(':') {
                Some((name, _)) => name.trim().to_owned(),
                None => continue,
            }
        } else {
            position.to_string()
        };
        fields.push((offset, name, visibility, restricted_path(field)));
    }
    fields
}

/// The text after any leading `#[...]` attributes and whitespace.
pub(crate) fn skip_attributes(mut text: &str) -> &str {
    loop {
        text = text.trim_start();
        let Some(rest) = text
            .strip_prefix('#')
            .and_then(|tail| tail.trim_start().strip_prefix('['))
        else {
            return text;
        };
        let mut depth = 1usize;
        let Some(end) = rest.find(|ch: char| {
            match ch {
                '[' => depth += 1,
                ']' => depth -= 1,
                _ => {}
            }
            depth == 0
        }) else {
            return "";
        };
        text = &rest[end + 1..];
    }
}

fn split_top_level_for(text: &str) -> Option<(&str, &str)> {
    let bytes = text.as_bytes();
    let mut depth = 0usize;
    let mut previous = b' ';
    for (index, &byte) in bytes.iter().enumerate() {
        if depth == 0 && text.is_char_boundary(index) && text[index..].starts_with(" for ") {
            return Some((text[..index].trim_end(), text[index + 5..].trim_start()));
        }
        match byte {
            b'<' | b'(' | b'[' => depth += 1,
            b'>' if previous != b'-' => depth = depth.saturating_sub(1),
            b')' | b']' => depth = depth.saturating_sub(1),
            _ => {}
        }
        previous = byte;
    }
    None
}

/// The head identifier of a type expression: strips `&`, lifetimes, `mut`,
/// and `dyn`, then takes the last path segment before generics
/// (`&mut crate::plan::Tile<T>` → `Tile`). `None` when the type has no head
/// identifier (tuples, slices, primitive literals like `[T; 4]`).
fn type_head(text: &str) -> Option<&str> {
    let mut rest = text.trim_start();
    loop {
        rest = rest.trim_start_matches('&').trim_start();
        if let Some(after) = rest.strip_prefix('\'') {
            let end = after
                .find(|ch: char| !(ch == '_' || ch.is_ascii_alphanumeric()))
                .unwrap_or(after.len());
            rest = after[end..].trim_start();
            continue;
        }
        if let Some(after) = keyword(rest, "mut").or_else(|| keyword(rest, "dyn")) {
            rest = after;
            continue;
        }
        break;
    }
    rest = rest.strip_prefix("::").unwrap_or(rest);
    loop {
        rest = rest.trim_start();
        let end = rest
            .find(|ch: char| !(ch == '_' || ch.is_ascii_alphanumeric()))
            .unwrap_or(rest.len());
        let name = &rest[..end];
        if !name
            .chars()
            .next()
            .is_some_and(|ch| ch == '_' || ch.is_ascii_alphabetic())
        {
            return None;
        }
        let tail = rest[end..].trim_start();
        if let Some(next) = tail.strip_prefix("::") {
            rest = next;
        } else {
            return Some(name);
        }
    }
}

fn split_visibility(rest: &str) -> (Visibility, &str) {
    let (visibility, tail, _) = visibility_parts(rest);
    (visibility, tail)
}

/// Normalized target of a written pub(in) restriction; other visibility rungs have none.
pub fn restricted_path(text: &str) -> Option<String> {
    let (visibility, _, scope) = visibility_parts(text.trim_start());
    if visibility != Visibility::PubIn {
        return None;
    }
    Some(keyword(scope?, "in")?.split_whitespace().collect())
}

fn visibility_parts(rest: &str) -> (Visibility, &str, Option<&str>) {
    let Some(after) = rest.strip_prefix("pub") else {
        return (Visibility::Private, rest, None);
    };
    if !after
        .chars()
        .next()
        .is_some_and(|ch| ch.is_whitespace() || ch == '(')
    {
        return (Visibility::Private, rest, None);
    }
    let after = after.trim_start();
    let Some(restricted) = after.strip_prefix('(') else {
        return (Visibility::Pub, after, None);
    };
    let Some((scope, tail)) = restricted.split_once(')') else {
        return (Visibility::Private, rest, None);
    };
    let visibility = match scope.trim() {
        "super" | "in super" => Visibility::PubSuper,
        "self" | "in self" => Visibility::Private,
        "crate" | "in crate" => Visibility::PubCrate,
        _ => Visibility::PubIn,
    };
    (visibility, tail.trim_start(), Some(scope.trim()))
}

/// The re-exported name: the last path segment (or `as` alias) for a simple
/// `pub use a::b::C;`; the raw target text for grouped or glob re-exports.
fn use_target(after: &str) -> &str {
    let target = after.split(';').next().unwrap_or(after).trim_end();
    if target.contains(['{', '*']) {
        return target;
    }
    if let Some((_, alias)) = target.rsplit_once(" as ") {
        return alias.trim();
    }
    target.rsplit("::").next().unwrap_or(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(line: &str) -> LineItem<'_> {
        classify_line(line).expect("line declares an item")
    }

    #[test]
    fn fn_qualifiers_yield_the_fn_name() {
        // The keyword-order bug this module retires: `pub const fn` used to
        // resolve to the name "fn".
        assert_eq!(
            item("pub const fn cost_of(x: u32) -> u32 {").name,
            "cost_of"
        );
        assert_eq!(
            item("pub async fn fetch_block(&self) -> Block {").name,
            "fetch_block"
        );
        assert_eq!(
            item("pub unsafe fn from_raw(ptr: *const u8) -> Self {").name,
            "from_raw"
        );
        assert_eq!(
            item("pub const unsafe extern \"C\" fn trampoline() {").name,
            "trampoline"
        );
        assert_eq!(item("async fn local_probe() {").kind, ItemKind::Fn);
        // A lone `const` still opens a const item, not a fn.
        let constant = item("pub const MAX_LANES: usize = 4;");
        assert_eq!(
            (constant.kind, constant.name),
            (ItemKind::Const, "MAX_LANES")
        );
    }

    #[test]
    fn impl_blocks_are_named_by_their_target() {
        // Inherent, generic, trait, path-qualified, and unsafe impls all
        // resolve to the target type's head identifier.
        let cases = [
            ("impl Tile {", "Tile"),
            ("impl Tile<T> {", "Tile"),
            ("impl<T: Clone> Grade<T> for Tile<T> {", "Tile"),
            ("impl fmt::Display for LaneTile {", "LaneTile"),
            ("impl<'a> Iterator for Walker<'a> {", "Walker"),
            ("unsafe impl Send for Tile {}", "Tile"),
            ("impl Drop for Guard {", "Guard"),
            ("impl<F: Fn() -> usize> Probe<F> {", "Probe"),
            ("impl From<&str> for Name {", "Name"),
            // No head identifier on the target: fall back to the trait.
            ("impl Marker for (A, B) {", "Marker"),
            ("impl Marker for [T; 4] {", "Marker"),
        ];
        for (line, expected) in cases {
            let found = item(line);
            assert_eq!(
                (found.kind, found.name),
                (ItemKind::Impl, expected),
                "line: {line}"
            );
        }
        // `implement` is an identifier, not the `impl` keyword.
        assert_eq!(classify_line("implement stuff"), None);
    }

    #[test]
    fn impl_capture_is_top_level_with_a_signature_block() {
        let source = "\
pub struct Tile;

impl Grade for Tile
where
    Tile: Clone,
{
    /// Method doc.
    pub fn cost(&self) -> Cost {
        Cost::ZERO
    }
}
";
        let items = scan_source(source, Path::new("fixture.rs"));
        let imp = items
            .iter()
            .find(|item| item.kind == ItemKind::Impl)
            .expect("impl captured");
        assert_eq!(
            (imp.name.as_str(), imp.line, imp.top_level),
            ("Tile", 3, true)
        );
        // Header only, through the body-opening brace: the where clause is in,
        // the method body is out.
        assert!(imp.declaration_block.contains("Tile: Clone"));
        assert!(!imp.declaration_block.contains("cost"));
        // The method inside is scanned as its own item, but not top-level.
        let method = items
            .iter()
            .find(|item| item.name == "cost")
            .expect("method scanned");
        assert_eq!((method.kind, method.top_level), (ItemKind::Fn, false));
        let tile = items
            .iter()
            .find(|item| item.kind == ItemKind::Struct)
            .expect("struct scanned");
        assert!(tile.top_level);
    }

    #[test]
    fn visibility_ladder_is_recognized() {
        assert_eq!(item("pub fn a() {}").visibility, Visibility::Pub);
        assert_eq!(
            item("pub(crate) fn b() {}").visibility,
            Visibility::PubCrate
        );
        assert_eq!(
            item("pub(super) fn c() {}").visibility,
            Visibility::PubSuper
        );
        assert_eq!(item("fn d() {}").visibility, Visibility::Private);
        assert_eq!(item("pub(self) fn e() {}").visibility, Visibility::Private);
    }

    #[test]
    fn reexports_and_macros_are_recognized() {
        let single = item("pub use crate::grade::Cost;");
        assert_eq!((single.kind, single.name), (ItemKind::PubUse, "Cost"));
        let grouped = item("pub use grade::{Cost, Grade};");
        assert_eq!(
            (grouped.kind, grouped.name),
            (ItemKind::PubUse, "grade::{Cost, Grade}")
        );
        // A private import is an outline-only item, never a declaration.
        assert_eq!(item("use std::fmt;").kind, ItemKind::Use);
        assert!(!ItemKind::Use.declares());
        let rules = item("macro_rules! grade_table {");
        assert_eq!(
            (rules.kind, rules.name),
            (ItemKind::MacroRules, "grade_table")
        );
    }

    #[test]
    fn multiline_nested_reexports_share_one_captured_grammar() {
        let text = "\
pub use crate::measurement::{
    block::{BlockFrame, BlockRefusal as Refusal},
    Floor,
    hidden::*,
};
";
        let items = scan_source(text, Path::new("src/lib.rs"));
        let export = items.first().expect("multiline re-export is captured");
        assert_eq!(export.kind, ItemKind::PubUse);
        assert_eq!(export.declaration_block.as_ref(), text.trim_end());
        assert_eq!(
            import_bindings(&export.declaration_block),
            vec![
                ImportBinding {
                    source: "*".to_owned(),
                    source_path: "crate::measurement::hidden::*".to_owned(),
                    local_name: "*".to_owned(),
                    module_only: false,
                },
                ImportBinding {
                    source: "BlockFrame".to_owned(),
                    source_path: "crate::measurement::block::BlockFrame".to_owned(),
                    local_name: "BlockFrame".to_owned(),
                    module_only: false,
                },
                ImportBinding {
                    source: "Floor".to_owned(),
                    source_path: "crate::measurement::Floor".to_owned(),
                    local_name: "Floor".to_owned(),
                    module_only: false,
                },
                ImportBinding {
                    source: "BlockRefusal".to_owned(),
                    source_path: "crate::measurement::block::BlockRefusal".to_owned(),
                    local_name: "Refusal".to_owned(),
                    module_only: false,
                },
            ]
        );
    }

    #[test]
    fn struct_blocks_capture_to_the_closing_brace() {
        let source = "\
pub struct Tile {
    lanes: Vec<LaneId>,
    cost: Cost,
}

pub struct Marker;

pub enum Verdict {
    Admitted(Tile),
    Refused { reason: String },
}

pub fn admit(
    candidate: Candidate,
) -> Verdict {
    Verdict::Admitted(Tile::default())
}
";
        let items = scan_source(source, Path::new("realize/admit.rs"));
        let by_name = |name: &str| {
            items
                .iter()
                .find(|item| item.name == name)
                .expect("scanned item present")
        };

        let tile = by_name("Tile");
        assert_eq!((tile.kind, tile.line), (ItemKind::Struct, 1));
        assert!(tile.declaration_block.ends_with('}'));
        assert!(tile.declaration_block.contains("lanes: Vec<LaneId>"));

        let marker = by_name("Marker");
        assert_eq!(marker.declaration_block.as_ref(), "pub struct Marker;");

        let verdict = by_name("Verdict");
        assert_eq!(verdict.kind, ItemKind::Enum);
        assert!(
            verdict
                .declaration_block
                .contains("Refused { reason: String }")
        );

        // Multi-line fn signatures capture through the body-opening brace and
        // exclude the body.
        let admit = by_name("admit");
        assert!(admit.declaration_block.contains("-> Verdict {"));
        assert!(!admit.declaration_block.contains("Tile::default"));
    }

    #[test]
    fn doc_summaries_attach_through_attributes() {
        let source = "\
//! Module header line.
//! Second header line.

/// Summary line of Tile.
///
/// Detail the atlas never shows.
#[derive(Debug, Clone)]
#[allow(
    dead_code,
    reason = \"multi-line attribute between doc and item\"
)]
pub struct Tile;

/// Detached by an intervening statement.
static_assertions_placeholder();

pub fn undocumented() {}

/// Doc on a re-export.
pub use crate::grade::Cost;
";
        let items = scan_source(source, Path::new("fixture.rs"));
        let doc_of = |name: &str| {
            items
                .iter()
                .find(|item| item.name == name)
                .expect("scanned item present")
                .doc
                .clone()
        };
        assert_eq!(doc_of("Tile").as_deref(), Some("Summary line of Tile."));
        assert_eq!(doc_of("undocumented"), None);
        assert_eq!(doc_of("Cost").as_deref(), Some("Doc on a re-export."));
        assert_eq!(
            module_header_lines(source),
            vec!["Module header line.", "Second header line."]
        );
        assert!(module_header_lines("pub fn a() {}\n").is_empty());
        assert_eq!(
            module_header_lines(
                r##"const FIXTURE: &str = r#"
//! Not a module header.
"#;
//! Real module header.
"##
            ),
            vec!["Real module header."]
        );
    }

    #[test]
    fn test_regions_cover_every_gated_module_not_only_the_last() {
        // The murail-8tlip class: production code between two test modules.
        // A single terminal boundary counted the interleaved module — and
        // everything after it — as production.
        let source = "\
pub fn production_a() {}

#[cfg(all(test, feature = \"loom-tests\"))]
mod loom_tests {
    fn inner() {}
}

pub fn production_b() {}

#[cfg(test)]
mod tests {
    use super::*;
}
";
        let regions = TestRegions::of(source);
        assert!(regions.covers(4), "the loom module is test code");
        assert!(!regions.covers(8), "production_b is not");
        assert!(regions.covers(11), "the terminal module is test code");
        // 13 lines, 4 in each region: the two declarations, the blank lines
        // around them, and the two production fns remain.
        assert_eq!(regions.non_test_loc(), 5);
    }

    #[test]
    fn fixture_source_inside_strings_cannot_declare_items_or_close_test_regions() {
        let source = r##"
#[cfg(test)]
mod tests {
    fn fixture() {
        let ordinary = "\
pub fn ghost() {
}
";
        let raw = r#"
pub struct Phantom {
}
"#;
        assert!(!ordinary.is_empty() && !raw.is_empty());
    }
}

pub fn real() {}
"##;
        let regions = TestRegions::of(source);
        assert!(regions.covers(5), "ordinary fixture text stays test-only");
        assert!(regions.covers(9), "raw fixture text stays test-only");
        assert!(
            !regions.covers(17),
            "production after the module remains live"
        );

        let items = scan_source(source, Path::new("fixture.rs"));
        let names: Vec<&str> = items.iter().map(|item| item.name.as_str()).collect();
        assert!(names.contains(&"real"));
        assert!(!names.contains(&"ghost"));
        assert!(!names.contains(&"Phantom"));
    }

    #[test]
    fn character_literals_cannot_change_declaration_boundaries() {
        let source = "\
pub struct Token<const C: char = '{'>;
pub struct ByteToken<const C: u8 = b'}'>;
pub struct After;
";
        let items = scan_source(source, Path::new("fixture.rs"));
        assert_eq!(items.len(), 3);
        assert_eq!(
            items[0].declaration_block.as_ref(),
            "pub struct Token<const C: char = '{'>;"
        );
        assert_eq!(
            items[1].declaration_block.as_ref(),
            "pub struct ByteToken<const C: u8 = b'}'>;"
        );
        assert_eq!(items[2].name, "After");
    }

    #[test]
    fn cfg_gates_are_read_as_predicates_not_spellings() {
        // Every shape below is live in the workspace.
        assert!(implies_test("test"));
        assert!(implies_test("all(test, feature = \"loom-tests\")"));
        assert!(implies_test("all(feature = \"gpu\", test)"));
        // `any` also compiles without test, and `not` is the negation: both
        // are production code that merely mentions the word.
        assert!(!implies_test("any(test, feature = \"test-strategies\")"));
        assert!(!implies_test("not(all(test, feature = \"loom-tests\"))"));
        assert!(!implies_test("not(test)"));
        assert!(!implies_test("feature = \"testing\""));
    }

    #[test]
    fn test_module_declarations_name_whole_test_files() {
        let source = "\
#[cfg(test)]
mod finite_interior_tests;

pub(crate) mod production;

#[cfg(test)]
mod tests {
}
";
        assert_eq!(
            test_module_declarations(source),
            vec![("finite_interior_tests".to_owned(), None)]
        );
    }

    #[test]
    fn test_module_declarations_carry_explicit_paths() {
        let source = "#[path = \"../tests/support/check.rs\"]\n#[cfg(test)]\nmod checks;\n";
        assert_eq!(
            test_module_declarations(source),
            vec![(
                "checks".to_owned(),
                Some("../tests/support/check.rs".to_owned())
            )]
        );
    }

    #[test]
    fn split_top_level_keeps_bracketed_and_quoted_commas_whole() {
        assert_eq!(
            split_top_level("Stop(u32, u32), Go"),
            vec!["Stop(u32, u32)", " Go"]
        );
        assert_eq!(
            split_top_level("feature = \"a,b\", test"),
            vec!["feature = \"a,b\"", " test"]
        );
    }

    #[test]
    fn non_test_boundary_keeps_gated_helpers_as_production() {
        // The replicate.rs regression class: an early test-only helper must
        // not truncate the count — gating one item is a property of that
        // item, not a boundary.
        let source = "\
pub fn production_a() {}

#[cfg(test)]
fn test_only_helper() {}

pub fn production_b() {}

#[cfg(test)]
mod tests {
    use super::*;
}
";
        assert_eq!(non_test_loc(source), 7);
        // No boundary: the whole file counts.
        assert_eq!(non_test_loc("fn a() {}\nfn b() {}\n"), 2);
    }
}

#[cfg(test)]
mod wire_tags {
    use super::{ItemKind, Visibility};
    #[test]
    fn all_supported_tags_round_trip_and_unknowns_stay_unknown() {
        for (kind, tag) in [
            (ItemKind::Struct, "Struct"),
            (ItemKind::Enum, "Enum"),
            (ItemKind::Union, "Union"),
            (ItemKind::Trait, "Trait"),
            (ItemKind::TypeAlias, "TypeAlias"),
            (ItemKind::Fn, "Fn"),
            (ItemKind::Mod, "Mod"),
            (ItemKind::MacroRules, "MacroRules"),
            (ItemKind::PubUse, "PubUse"),
            (ItemKind::Const, "Const"),
            (ItemKind::Static, "Static"),
            (ItemKind::Impl, "Impl"),
            (ItemKind::Use, "Use"),
            (ItemKind::MacroCall, "MacroCall"),
            (ItemKind::Extern, "Extern"),
        ] {
            assert_eq!(kind.tag(), tag);
            assert_eq!(ItemKind::from_tag(tag), Some(kind));
        }
        for unknown in ["", "FutureTag", "pub", "struct"] {
            assert_eq!(ItemKind::from_tag(unknown), None);
        }
        for (kind, tag) in [
            (Visibility::Pub, "Pub"),
            (Visibility::PubCrate, "PubCrate"),
            (Visibility::PubIn, "PubIn"),
            (Visibility::PubSuper, "PubSuper"),
            (Visibility::Private, "Private"),
        ] {
            assert_eq!(kind.tag(), tag);
            assert_eq!(Visibility::from_tag(tag), Some(kind));
        }
        for unknown in ["", "FutureTag", "pub", "struct"] {
            assert_eq!(Visibility::from_tag(unknown), None);
        }
    }
}
