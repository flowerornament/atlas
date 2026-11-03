//! Compatibility paths for canonical source facts; no second structural grammar.
pub use super::items::*;
pub use super::lexical::RustSourceProjection;
pub use super::syntax::imports::import_bindings;
pub(crate) use super::syntax::imports::{ImportBinding, item_use_bindings};

/// Identifier-bounded lexical mentions, deliberately approximate.
pub(crate) fn identifiers(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .filter(|t| !t.is_empty() && !t.starts_with(|c: char| c.is_ascii_digit()))
}
pub fn non_test_loc(text: &str) -> Result<usize, super::SyntaxError> {
    Ok(TestRegions::of(text)?.non_test_loc())
}
#[cfg(test)]
use std::path::Path;
#[cfg(test)]
fn scan_source(text: &str, path: &Path) -> Vec<Item> {
    RustSourceProjection::of(text.into())
        .outline(path)
        .unwrap()
        .into_iter()
        .filter(|i| i.kind.declares())
        .collect()
}
#[cfg(test)]
fn module_header_lines(text: &str) -> Vec<String> {
    RustSourceProjection::of(text.into()).module_header_lines()
}
#[cfg(test)]
fn test_module_declarations(text: &str) -> Vec<(String, Option<String>)> {
    super::Source::of(Path::new("input.rs"), text.into())
        .unwrap()
        .file_module_declarations()
        .iter()
        .filter(|(_, _, tests)| *tests)
        .map(|(n, p, _)| (n.clone(), p.clone()))
        .collect()
}
#[cfg(test)]
fn implies_test(predicate: &str) -> bool {
    syn::parse_str::<syn::Meta>(predicate)
        .is_ok_and(|m| super::syntax::attributes::implies(&m, &[]))
}
#[cfg(test)]
mod tests {
    use super::*;

    fn item(line: &str) -> Item {
        let completed = if line.trim_end().ends_with('{') {
            format!("{line}}}")
        } else {
            line.to_owned()
        };
        crate::source::syntax::read(
            Path::new("fragment.rs"),
            &RustSourceProjection::of(completed),
        )
        .expect("valid item fragment")
        .outline
        .remove(0)
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
            (constant.kind, constant.name.as_str()),
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
                (found.kind, found.name.as_str()),
                (ItemKind::Impl, expected),
                "line: {line}"
            );
        }
        // `implement` is an identifier, not the `impl` keyword.
        assert_eq!(
            crate::source::syntax::read(
                Path::new("bad.rs"),
                &RustSourceProjection::of("implement stuff".into())
            )
            .ok()
            .and_then(|f| f.outline.into_iter().next()),
            None
        );
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
        assert_eq!(
            (single.kind, single.name.as_str()),
            (ItemKind::PubUse, "Cost")
        );
        let grouped = item("pub use grade::{Cost, Grade};");
        assert_eq!(
            (grouped.kind, grouped.name.as_str()),
            (ItemKind::PubUse, "grade::{Cost, Grade}")
        );
        // A private import is an outline-only item, never a declaration.
        assert_eq!(item("use std::fmt;").kind, ItemKind::Use);
        assert!(!ItemKind::Use.declares());
        let rules = item("macro_rules! grade_table {");
        assert_eq!(
            (rules.kind, rules.name.as_str()),
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
            import_bindings(&export.declaration_block).unwrap(),
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
        assert!(
            super::super::Source::of(Path::new("fixture.rs"), source.into()).is_err(),
            "a statement at file scope is not Rust item syntax"
        );
        let source = source.replace(
            "static_assertions_placeholder();",
            "static_assertions_placeholder!();",
        );
        let source = source.as_str();
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
        let regions = TestRegions::of(source).unwrap();
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
        let regions = TestRegions::of(source).unwrap();
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
    fn nested_commas_are_parsed_by_the_rust_grammar() {
        assert_eq!(item("enum State { Stop(u32,u32), Go }").name, "State");
        assert!(!implies_test("any(feature = \"a,b\", test)"));
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
        assert_eq!(non_test_loc(source).unwrap(), 7);
        // No boundary: the whole file counts.
        assert_eq!(non_test_loc("fn a() {}\nfn b() {}\n").unwrap(), 2);
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
