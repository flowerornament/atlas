//! Semantic call evidence read from SCIP indexes: a committed index of a
//! fixture package, and doctored indexes for each way an index can fall short.
mod support;
use atlas_lib::analysis::{
    Analysis,
    semantic::{Agreement, Reading, Semantic, SemanticIndex},
};
use atlas_lib::discovery;
use serde_json::Value;
use support::Fixture;

const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/semantic");

fn read(fixture: &Fixture, index: &[u8]) -> Reading {
    let snapshot = discovery::snapshot_workspace(fixture.path()).expect("fixture snapshot");
    Analysis::new(&snapshot)
        .expect("analysis")
        .semantic(&SemanticIndex::read(index))
        .expect("semantic reading")
}
fn kind(semantic: &Semantic) -> String {
    serde_json::to_value(semantic).expect("serialize reading")["reading"]
        .as_str()
        .expect("reading kind")
        .to_owned()
}
fn agreement(value: Option<Agreement>) -> String {
    serde_json::to_value(value)
        .expect("serialize agreement")
        .as_str()
        .unwrap_or("none")
        .to_owned()
}

#[test]
fn committed_index_reads_beside_the_qualified_targets_of_the_fixture() {
    let file = |path: &str| std::fs::read_to_string(format!("{FIXTURE}/{path}")).expect(path);
    let fixture = Fixture::new(&[
        ("Cargo.toml", &file("Cargo.toml")),
        ("src/lib.rs", &file("src/lib.rs")),
    ]);
    let index = std::fs::read(format!("{FIXTURE}/index.scip")).expect("committed index");
    let reading = read(&fixture, &index);
    assert_eq!(reading.status, "available", "{:?}", reading.unavailable);
    let rows: Vec<_> = reading
        .calls
        .iter()
        .map(|c| {
            (
                c.site.line,
                c.spelling.as_str(),
                kind(&c.semantic),
                agreement(c.agreement),
            )
        })
        .collect();
    let expected = [
        // `usize::from(u16)` beside a workspace `impl From<Slot> for usize`.
        (7, "usize::from", "external", "extends"),
        (17, "base", "definition", "agrees"),
        (28, "base", "definition", "agrees"),
        (32, "run", "definition", "extends"),
        // Generic receiver: the trait declaration, dispatch open.
        (35, "run", "trait_method", "extends"),
        (38, "String::from", "external", "extends"),
        (39, "len", "external", "extends"),
        (43, "call", "local", "extends"),
        // A body-local `fn label` shares its symbol with the module's.
        (49, "label", "ambiguous", "ambiguous"),
        // A block-local `use crate::base as label`.
        (53, "label", "definition", "extends"),
        (56, "usize::from", "definition", "extends"),
        (68, "generated", "definition", "extends"),
        // A derived `clone` has no definition in source.
        (74, "clone", "no_definition", "silent"),
    ];
    let expected: Vec<_> = expected
        .iter()
        .map(|&(line, spelling, kind, agreement)| {
            (line, spelling, kind.to_owned(), agreement.to_owned())
        })
        .collect();
    assert_eq!(rows, expected);
    let json = serde_json::to_value(&reading).expect("serialize reading");
    assert_eq!(json["evidence"], "semantic");
    assert_eq!(json["provenance"]["origin"], "supplied");
    assert_eq!(json["provenance"]["producer"], "rust-analyzer");
    assert!(
        json["provenance"]["configuration"]
            .as_str()
            .is_some_and(|text| text.contains("host target")),
        "every reading names its one configuration"
    );
    assert_eq!(json["coverage"]["sysroot_observed"], true);
    assert_eq!(json["agreement"]["agrees"], 2);
    assert!(json["agreement"].get("disagrees").is_none());
}

/// A minimal SCIP encoder for doctored indexes.
mod scip {
    fn varint(mut v: u64, out: &mut Vec<u8>) {
        loop {
            let byte = u8::try_from(v & 0x7f).expect("seven bits");
            v >>= 7;
            if v == 0 {
                out.push(byte);
                return;
            }
            out.push(byte | 0x80);
        }
    }
    fn field(number: u64, value: &[u8], out: &mut Vec<u8>) {
        varint(number << 3 | 2, out);
        varint(u64::try_from(value.len()).expect("length"), out);
        out.extend_from_slice(value);
    }
    fn number(number: u64, value: u64, out: &mut Vec<u8>) {
        varint(number << 3, out);
        varint(value, out);
    }
    pub struct Document {
        pub path: &'static str,
        pub encoding: u64,
        /// (line, start, end, symbol, roles), zero-based UTF-8 columns.
        pub occurrences: Vec<(u32, u32, u32, String, u64)>,
        /// (symbol, kind)
        pub symbols: Vec<(String, u64)>,
        /// `Document.text`, when the indexer writes it.
        pub text: Option<&'static str>,
    }
    pub fn index(documents: &[Document]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut tool = Vec::new();
        field(1, b"test-indexer", &mut tool);
        field(2, b"1", &mut tool);
        let mut metadata = Vec::new();
        field(2, &tool, &mut metadata);
        field(1, &metadata, &mut out);
        for d in documents {
            let mut document = Vec::new();
            field(1, d.path.as_bytes(), &mut document);
            for (line, start, end, symbol, roles) in &d.occurrences {
                let mut range = Vec::new();
                for v in [line, start, end] {
                    varint(u64::from(*v), &mut range);
                }
                let mut occurrence = Vec::new();
                field(1, &range, &mut occurrence);
                field(2, symbol.as_bytes(), &mut occurrence);
                number(3, *roles, &mut occurrence);
                field(2, &occurrence, &mut document);
            }
            for (symbol, kind) in &d.symbols {
                let mut information = Vec::new();
                field(1, symbol.as_bytes(), &mut information);
                number(5, *kind, &mut information);
                field(3, &information, &mut document);
            }
            if let Some(text) = d.text {
                field(5, text.as_bytes(), &mut document);
            }
            number(6, d.encoding, &mut document);
            field(2, &document, &mut out);
        }
        out
    }
}

const SOURCE: &str = "pub fn base() -> u8 {\n    0\n}\npub fn other() -> u8 {\n    1\n}\npub fn direct() -> usize {\n    base();\n    String::new().len()\n}\n";
const BASE: &str = "pkg cargo fixture 0.1.0 lib/base().";
const OTHER: &str = "pkg cargo fixture 0.1.0 lib/other().";
const NEW: &str = "pkg cargo alloc 1 lib/string/String#new().";
const LEN: &str = "pkg cargo alloc 1 lib/string/String#len().";

fn source_fixture(source: &str) -> Fixture {
    Fixture::new(&[
        (
            "Cargo.toml",
            "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[workspace]\n",
        ),
        ("src/lib.rs", source),
    ])
}
/// A faithful index of SOURCE: definitions of base and other, and the three
/// calls in direct.
fn faithful() -> scip::Document {
    scip::Document {
        path: "src/lib.rs",
        encoding: 1,
        occurrences: vec![
            (0, 7, 11, BASE.into(), 1),
            (3, 7, 12, OTHER.into(), 1),
            (7, 4, 8, BASE.into(), 0),
            (8, 12, 15, NEW.into(), 0),
            (8, 18, 21, LEN.into(), 0),
        ],
        symbols: vec![(BASE.into(), 17), (OTHER.into(), 17)],
        text: None,
    }
}
fn base_call(reading: &Reading) -> (String, String) {
    let call = reading
        .calls
        .iter()
        .find(|c| c.spelling == "base")
        .expect("base call");
    (kind(&call.semantic), agreement(call.agreement))
}

#[test]
fn a_faithful_index_agrees_and_names_external_items() {
    let reading = read(&source_fixture(SOURCE), &scip::index(&[faithful()]));
    assert_eq!(base_call(&reading), ("definition".into(), "agrees".into()));
    let json = serde_json::to_value(&reading).expect("serialize");
    assert_eq!(json["readings"]["external"], 2);
}

#[test]
fn an_index_without_standard_library_symbols_is_unavailable_not_external() {
    let mut document = faithful();
    document.occurrences.retain(|o| !o.3.contains(" alloc "));
    let reading = read(&source_fixture(SOURCE), &scip::index(&[document]));
    assert_eq!(reading.status, "unavailable");
    assert!(
        reading
            .unavailable
            .as_deref()
            .is_some_and(|r| r.contains("rust-src"))
    );
    assert!(
        reading.calls.is_empty(),
        "an unavailable index reads no call"
    );
}

#[test]
fn other_position_encodings_refuse() {
    let mut document = faithful();
    document.encoding = 2;
    let reading = read(&source_fixture(SOURCE), &scip::index(&[document]));
    assert!(
        reading
            .unavailable
            .as_deref()
            .is_some_and(|r| r.contains("position encoding 2"))
    );
}

#[test]
fn document_order_does_not_change_the_reading() {
    let other = || scip::Document {
        path: "src/unused.rs",
        encoding: 1,
        occurrences: vec![(0, 0, 3, NEW.into(), 0)],
        symbols: vec![],
        text: None,
    };
    let fixture = source_fixture(SOURCE);
    let forward = read(&fixture, &scip::index(&[faithful(), other()]));
    let backward = read(&fixture, &scip::index(&[other(), faithful()]));
    let strip = |reading: &Reading| {
        let mut json = serde_json::to_value(reading).expect("serialize");
        json["provenance"]["index_digest"] = Value::Null;
        json
    };
    assert_eq!(strip(&forward), strip(&backward));
}

#[test]
fn a_symbol_defined_at_two_sites_is_ambiguous_never_a_choice() {
    let mut document = faithful();
    document.occurrences.push((3, 7, 12, BASE.into(), 1));
    let reading = read(&source_fixture(SOURCE), &scip::index(&[document]));
    assert_eq!(
        base_call(&reading),
        ("ambiguous".into(), "ambiguous".into())
    );
    let json = serde_json::to_value(&reading).expect("serialize");
    assert_eq!(json["coverage"]["symbols_with_several_definitions"], 1);
}

#[test]
fn source_edited_after_indexing_reads_as_not_indexed() {
    let edited = SOURCE.replace("    base();", "    based();");
    let reading = read(&source_fixture(&edited), &scip::index(&[faithful()]));
    let call = reading
        .calls
        .iter()
        .find(|c| c.spelling == "based")
        .expect("edited call");
    let json = serde_json::to_value(&call.semantic).expect("serialize");
    assert_eq!(json["reading"], "not_indexed");
    assert_eq!(json["reason"], "index text differs from captured source");
    assert_eq!(agreement(call.agreement), "silent");
}

#[test]
fn a_definition_elsewhere_disagrees_and_both_readings_are_kept() {
    let mut document = faithful();
    document.occurrences[0] = (3, 7, 12, BASE.into(), 1);
    document.occurrences[1] = (0, 7, 11, OTHER.into(), 1);
    let reading = read(&source_fixture(SOURCE), &scip::index(&[document]));
    assert_eq!(
        base_call(&reading),
        ("definition".into(), "disagrees".into())
    );
    let call = reading
        .calls
        .iter()
        .find(|c| c.spelling == "base")
        .expect("base call");
    let json = serde_json::to_value(call).expect("serialize");
    assert_eq!(json["qualified"]["target"], "function");
    assert_eq!(json["qualified"]["site"], "src/lib.rs:1");
    assert_eq!(json["semantic"]["site"]["line"], 4);
}

#[test]
fn unusable_indexes_name_their_reason() {
    let fixture = source_fixture(SOURCE);
    let reason = |bytes: &[u8]| read(&fixture, bytes).unavailable.unwrap_or_default();
    assert!(reason(b"\x0fnot an index").contains("not valid SCIP"));
    assert!(reason(&scip::index(&[])).contains("no documents"));
    assert!(reason(&scip::index(&[faithful(), faithful()])).contains("appears twice"));
    let mut outside = faithful();
    outside.path = "src/elsewhere.rs";
    let reading = read(&fixture, &scip::index(&[outside]));
    assert_eq!(reading.status, "available");
    let json = serde_json::to_value(&reading).expect("serialize");
    assert_eq!(json["coverage"]["files_not_indexed"][0], "src/lib.rs");
    assert_eq!(base_call(&reading), ("not_indexed".into(), "silent".into()));
}

/// A same-length name written where the index has another: position and
/// length match, so only the symbol's name tells the index is stale. It is
/// never read as an edge.
#[test]
fn a_symbol_that_names_another_function_is_a_mismatch_never_an_edge() {
    let edited = SOURCE.replace("    base();", "    bass();");
    let reading = read(&source_fixture(&edited), &scip::index(&[faithful()]));
    let call = reading
        .calls
        .iter()
        .find(|c| c.spelling == "bass")
        .expect("edited call");
    let json = serde_json::to_value(&call.semantic).expect("serialize");
    assert_eq!(json["reading"], "mismatch");
    assert_eq!(json["symbol"], BASE);
    assert_eq!(agreement(call.agreement), "silent");
}

/// An index that carries a document's text, for text the tree no longer has,
/// reads every call there as not indexed.
#[test]
fn a_document_whose_indexed_text_differs_is_not_indexed() {
    let mut document = faithful();
    document.text = Some(SOURCE);
    let edited = SOURCE.replace("    1\n", "    2\n");
    let reading = read(&source_fixture(&edited), &scip::index(&[document]));
    let json = serde_json::to_value(&reading.calls[0].semantic).expect("serialize");
    assert_eq!(json["reading"], "not_indexed");
    assert_eq!(json["reason"], "index is for other source");
    // The same text reads as before.
    let mut document = faithful();
    document.text = Some(SOURCE);
    let reading = read(&source_fixture(SOURCE), &scip::index(&[document]));
    assert_eq!(base_call(&reading), ("definition".into(), "agrees".into()));
}
