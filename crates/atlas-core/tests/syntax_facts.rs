//! Structural facts are independent of formatting; unknown syntax never becomes no work.
mod support;
use atlas_core::{
    analysis::Analysis,
    checks::{self, policy::Policy, typed_errors},
    discovery::snapshot_workspace,
    population::{TestSelection, VisibilitySelection},
    queries::navigation::Navigation,
    source::{ItemKind, Source, SyntaxError},
};
use std::path::Path;
use support::Fixture;
const MANIFEST: &str = "[package]\nname='fixture'\nversion='0.0.0'\nedition='2024'\n[workspace]\n";
fn fixture(files: &[(&str, &str)]) -> Fixture {
    let mut input = vec![("Cargo.toml", MANIFEST)];
    input.extend_from_slice(files);
    Fixture::new(&input)
}

#[test]
fn law_structural_nesting_and_spans_survive_compact_unicode_source() {
    let text = "// orientation\npub struct Café { pub value: u32 } impl Café { pub fn value(&self)->u32 { self.value } }\nmod nested { pub struct Child; }\nfn outer(){ fn local(){} }\n";
    let source = Source::of(Path::new("src/lib.rs"), text.into()).unwrap();
    let facts: Vec<_> = source
        .outline()
        .iter()
        .map(|i| (i.name.as_str(), i.top_level))
        .collect();
    assert_eq!(
        facts,
        [
            ("Café", true),
            ("Café", true),
            ("value", false),
            ("nested", true),
            ("Child", false),
            ("outer", true),
            ("local", false)
        ]
    );
    for item in source.outline() {
        let written = &source.text()[item.syntax_span.clone()];
        assert!(!written.is_empty());
        if item.name == "value" {
            assert!(written.starts_with("pub fn value"));
            assert_eq!(&*item.declaration_block, "pub fn value(&self)->u32 {");
        }
    }
    assert_eq!(*source.outline()[0].span.start(), 1);
    assert_eq!(
        *source.outline()[1].span.start(),
        2,
        "same-line neighbor does not inherit the first item's comment"
    );
    let raw = Source::of(Path::new("raw.rs"), "pub struct r#type;".into()).unwrap();
    assert_eq!(raw.outline()[0].name, "r#type");
}

#[test]
fn law_distinct_same_line_imports_preserve_other_stage_paths() {
    let fixture = fixture(&[
        ("src/lib.rs", "mod a; mod b;\n"),
        (
            "src/a.rs",
            "use crate::b::One; use crate::b::Two; fn f() { crate::b::Three; }\n",
        ),
        (
            "src/b.rs",
            "pub struct One; pub struct Two; pub struct Three;\n",
        ),
    ]);
    let snapshot = snapshot_workspace(fixture.path()).unwrap();
    let source = snapshot.source_facts("src/a.rs").unwrap();
    let imports: Vec<_> = source.imports().collect();
    assert_eq!(imports.len(), 2);
    assert_eq!(
        &source.text()[imports[0].bytes.clone()],
        "use crate::b::One;"
    );
    assert_eq!(
        &source.text()[imports[1].bytes.clone()],
        "use crate::b::Two;"
    );
    let policy=Policy::parse("schema=1\nprovenance='authored'\nsurface={}\nauthority=[]\nstage_order=[{crate='fixture',stages=['a','b']}]\n[population]\ncore=['fixture']\n[size]\ncore_token_lines=100\ncrate={}\n[crate_graph]\nrule=[]\n").unwrap();
    let report = checks::stages(&snapshot, &policy, None).unwrap();
    assert!(report.unavailable.is_empty());
    for name in ["One", "Two", "Three"] {
        assert!(
            report.crates["fixture"]
                .crossing
                .iter()
                .any(|edge| edge.path.ends_with(name)),
            "{name} disappeared"
        );
    }
}

#[test]
fn law_precise_test_sites_do_not_exempt_their_same_line_neighbors() {
    let fixture = fixture(&[(
        "src/lib.rs",
        "#[cfg(all(\n test, feature=\"x\"\n))] fn hidden(){} fn live(){}\n",
    )]);
    let snapshot = snapshot_workspace(fixture.path()).unwrap();
    let declarations = &snapshot.graph().files[0].declarations;
    assert!(
        declarations
            .iter()
            .find(|d| d.name == "hidden")
            .unwrap()
            .test_only
    );
    assert!(
        !declarations
            .iter()
            .find(|d| d.name == "live")
            .unwrap()
            .test_only
    );
    let facts = Analysis::new(&snapshot).unwrap().representations().unwrap();
    assert!(facts.functions.iter().any(|f| f.name == "live"));
    assert!(!facts.functions.iter().any(|f| f.name == "hidden"));
    // Native line budgets cannot divide that shared line, so they refuse explicitly.
    assert!(
        Navigation::new(&snapshot, VisibilitySelection::All)
            .unwrap()
            .census(None)
            .is_err()
    );
}

#[test]
fn law_invalid_capture_keeps_identity_and_required_failures() {
    for (parent, child_is_test) in [
        ("#[cfg(test)] mod child;\n", true),
        ("mod child;\n", false),
        ("#[cfg(test)] mod child; fn broken(\n", false),
    ] {
        let fixture = fixture(&[("src/lib.rs", parent), ("src/child.rs", "pub fn broken(\n")]);
        let snapshot = snapshot_workspace(fixture.path()).unwrap();
        assert_eq!(snapshot.source("src/child.rs"), Some("pub fn broken(\n"));
        assert!(snapshot.source_facts("src/child.rs").is_none());
        assert!(snapshot.syntax_error("src/child.rs").is_some());
        let file = snapshot
            .graph()
            .files
            .iter()
            .find(|f| f.path == "src/child.rs")
            .unwrap();
        assert!(!file.content_fingerprint.is_empty());
        assert_eq!(file.test_only, child_is_test);
        let analysis = Analysis::new(&snapshot).unwrap();
        assert_eq!(analysis.representations().is_ok(), child_is_test);
        let readings = typed_errors::read(&analysis, &["fixture".into()]).unwrap();
        assert_eq!(readings["fixture"].unavailable.is_empty(), child_is_test);
        assert!(
            Analysis::new(&snapshot)
                .unwrap()
                .with_tests(TestSelection::All)
                .representations()
                .is_err()
        );
    }
}

#[test]
fn law_macro_tokens_stay_opaque_and_unsupported_items_are_errors() {
    let text = "macro_rules! m { () => { pub struct Ghost; }; }\nm!();\npub struct Real;\nimpl Real { opaque!(); }\ntrait Contract { opaque!(); }\nunsafe extern \"C\" { opaque!(); }\n";
    let source = Source::of(Path::new("macro.rs"), text.into()).unwrap();
    assert!(!source.outline().iter().any(|i| i.name == "Ghost"));
    assert_eq!(
        source
            .outline()
            .iter()
            .filter(|i| i.kind == ItemKind::MacroRules)
            .count(),
        1
    );
    assert_eq!(
        source
            .outline()
            .iter()
            .filter(|item| item.kind == ItemKind::MacroCall)
            .count(),
        4,
        "module and associated opaque sites retain their spans"
    );
    assert!(matches!(
        Source::of(Path::new("alias.rs"), "trait Alias = Send;".into()),
        Err(SyntaxError::Unsupported { .. })
    ));
}

#[test]
fn law_bom_shebang_and_comment_shaped_literals_keep_source_locations() {
    let text = "\u{feff}#!/usr/bin/env rust-script\nconst TEXT: &str = r#\"// pub struct Ghost;\"#;\npub struct Real;\n";
    let source = Source::of(Path::new("script.rs"), text.into()).unwrap();
    let real = source.outline().iter().find(|i| i.name == "Real").unwrap();
    assert_eq!(real.line, 3);
    assert_eq!(&source.text()[real.syntax_span.clone()], "pub struct Real;");
}

#[test]
fn law_portable_capture_does_not_retain_thread_local_syn_nodes() {
    fn portable<T: Send + Sync>() {}
    portable::<Source>();
    portable::<atlas_core::discovery::Snapshot>();
}

#[test]
fn law_same_line_sites_keep_kinds_ids_and_export_bindings_distinct() {
    let fixture = fixture(&[(
        "src/lib.rs",
        "pub struct Item; impl Item {} pub struct Target; pub use Target as Alias; pub use Target as Alias;\n",
    )]);
    let snapshot = snapshot_workspace(fixture.path()).unwrap();
    let decls = &snapshot.graph().files[0].declarations;
    assert_eq!(
        decls
            .iter()
            .map(|d| &d.id)
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        decls.len()
    );
    let nav = Navigation::new(&snapshot, VisibilitySelection::All).unwrap();
    let dump = nav.dump().unwrap();
    assert!(
        dump.declarations
            .iter()
            .any(|d| d.site.name == "Item" && d.site.kind == ItemKind::Struct)
    );
    assert_eq!(
        dump.exports
            .iter()
            .filter(|e| e.local_name == "Alias")
            .count(),
        2
    );
}

#[test]
fn law_native_module_lines_begin_at_the_gate_not_its_attached_comments() {
    let text = "fn live(){}\n\n// tests heading\n\n#[cfg(test)]\nmod tests { fn t(){} }\n";
    let source = Source::of(Path::new("lib.rs"), text.into()).unwrap();
    let module = source
        .outline()
        .iter()
        .find(|i| i.kind == ItemKind::Mod)
        .unwrap();
    assert_eq!(
        *module.span.start(),
        5,
        "a separated plain comment is not attached"
    );
    assert_eq!(source.test_regions().non_test_loc(), 4);
    let text = "/// Test docs.\n#[cfg(test)]\nmod tests {}\n";
    let source = Source::of(Path::new("lib.rs"), text.into()).unwrap();
    assert_eq!(*source.outline()[0].span.start(), 1);
    assert_eq!(
        source.test_regions().non_test_loc(),
        1,
        "native lines retain the preceding doc"
    );
}
