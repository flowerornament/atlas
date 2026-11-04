//! Native move and visibility boundary regressions.
use super::inventory::Inventory;
use super::*;
use std::fs;
fn workspace(files: &[(&str, &str)]) -> Snapshot {
    let fixture = crate::test_support::Fixture::new(&[]);
    let root = fixture.path();
    fs::create_dir_all(root.join("crates/murail-fix/src")).expect("fixture directories");
    fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nmembers=[\"crates/murail-fix\"]\nresolver=\"3\"\n",
    )
    .expect("fixture workspace");
    fs::write(
        root.join("crates/murail-fix/Cargo.toml"),
        "[package]\nname=\"murail-fix\"\nversion=\"0.0.0\"\nedition=\"2024\"\n",
    )
    .expect("fixture package");
    fs::write(root.join("crates/murail-fix/src/lib.rs"), "").expect("fixture target");
    for (path, text) in files {
        let path = root.join("crates/murail-fix/src").join(path);
        fs::create_dir_all(path.parent().expect("parent")).expect("fixture source dir");
        fs::write(path, text).expect("fixture source");
    }
    let metadata = cargo_metadata::MetadataCommand::new()
        .current_dir(root)
        .no_deps()
        .exec()
        .expect("fixture metadata");
    crate::discovery::snapshot_metadata(&metadata).expect("fixture snapshot")
}

fn all(_: &str) -> bool {
    true
}

const QUEUE: &str = "\
use crate::rt::Slot;

/// A queue.
pub struct Queue<T> where T: Fn(u8) {
    head: usize,
    pub(super) tail: usize,
}

pub(crate) struct Pair(u32, pub u32);

impl<T> Queue<T> where T: Fn(u8) {
    fn push(&self) -> Slot { crate::rt::Slot::new(self.head) }
}

impl<T> Drop for Queue<T> where T: Fn(u8) {
    fn drop(&mut self) {}
}

fn helper() {}
";

/// One file renamed by the move; `after` may also rewrite it.
fn sides(before: &str, after: &str, bodies: bool) -> (Inventory, Inventory) {
    (
        inventory(&workspace(&[("queue.rs", before)]), &all, bodies).expect("before inventory"),
        inventory(&workspace(&[("rt/queue.rs", after)]), &all, bodies).expect("after inventory"),
    )
}

fn visibility_rows(before: &str, after: &str) -> Vec<(String, &'static str, String, String)> {
    let (before, after) = sides(before, after, false);
    visibility_changes(&before, &after)
        .into_iter()
        .map(|change| {
            (
                change.key.2.clone(),
                Direction::of(change.before, change.after).label(),
                change.before.visibility_text(),
                change.after.visibility_text(),
            )
        })
        .collect()
}

fn widened(before: &str, after: &str) -> Vec<String> {
    visibility_rows(before, after)
        .into_iter()
        .filter(|(_, direction, _, _)| *direction == "widened")
        .map(|(name, _, old, new)| format!("{name} {old} → {new}"))
        .collect()
}

#[test]
fn a_pure_move_changes_no_visibility() {
    assert!(visibility_rows(QUEUE, QUEUE).is_empty());
}

#[test]
fn widened_fields_methods_and_items_are_reported_across_a_move() {
    let after = QUEUE
        .replace("    head: usize", "    pub(crate) head: usize")
        .replace("    pub(super) tail", "    pub(in crate::rt) tail")
        .replace("(u32, pub u32)", "(pub(crate) u32, pub u32)")
        .replace("    fn push", "    pub fn push")
        .replace("fn helper", "pub(super) fn helper");
    assert_eq!(
        widened(QUEUE, &after),
        vec![
            "Queue.head private → pub(crate)",
            "Queue.tail pub(super) → pub(in crate::rt)",
            "Pair.0 private → pub(crate)",
            "Queue::push private → pub",
            "helper private → pub(super)",
        ]
    );
}

#[test]
fn narrowing_is_reported_as_narrowing() {
    let after = QUEUE.replace("    pub(super) tail", "    tail");
    assert_eq!(
        visibility_rows(QUEUE, &after),
        vec![(
            "Queue.tail".to_owned(),
            "narrowed",
            "pub(super)".to_owned(),
            "private".to_owned()
        )]
    );
}

#[test]
fn reexports_restricted_paths_and_same_named_sites_cannot_hide_a_widening() {
    let reexport = "mod m { pub struct X; }\npub(crate) use m::X;\n";
    assert_eq!(
        widened(reexport, &reexport.replace("pub(crate) use", "pub use")),
        vec!["X pub(crate) → pub"]
    );
    let import = "mod m { pub struct X; }\nuse m::X;\n";
    assert_eq!(
        widened(import, &import.replace("use m::X", "pub use m::X")),
        vec!["X private → pub"]
    );
    // Same-named fns in two inline modules swap visibility. The grammar
    // reads one declaration per line, as rustfmt writes them.
    let swap = "mod a {\n    fn f() {}\n}\nmod b {\n    pub fn f() {}\n}\n";
    let swapped = "mod a {\n    pub fn f() {}\n}\nmod b {\n    fn f() {}\n}\n";
    assert_eq!(widened(swap, swapped), vec!["a::f private → pub"]);
    // `pub(in crate)` is the crate rung; an ancestor scope widens.
    assert_eq!(
        widened(
            "pub(in crate::a::b) fn f() {}\n",
            "pub(in crate) fn f() {}\n"
        ),
        vec!["f pub(in crate::a::b) → pub(crate)"]
    );
    assert_eq!(
        widened(
            "pub(in crate::a::b) fn f() {}\n",
            "pub(in crate::a) fn f() {}\n"
        ),
        vec!["f pub(in crate::a::b) → pub(in crate::a)"]
    );
    assert_eq!(
        visibility_rows(
            "pub(in crate::a) fn f() {}\n",
            "pub(in crate::b) fn f() {}\n"
        )[0]
        .1,
        "rescoped"
    );
    assert_eq!(
        widened(
            "unsafe extern \"C\" fn f() {}\n",
            "pub unsafe extern \"C\" fn f() {}\n"
        ),
        vec!["f private → pub"]
    );
    assert_eq!(
        widened(
            "pub struct P(#[allow(x)] u32);\n",
            "pub struct P(#[allow(x)] pub u32);\n"
        ),
        vec!["P.0 private → pub"]
    );
}

#[test]
fn a_path_scope_matches_the_whole_crate_before_reporting() {
    // The item moved into the scoped folder and widened on the way.
    let before = workspace(&[("tick.rs", "fn advance() {}\n")]);
    let after = workspace(&[("engine/tick.rs", "pub(crate) fn advance() {}\n")]);
    let scope = Scope::of(&before, &after, Some("crates/murail-fix/src/engine")).expect("scope");
    let crates = scope.crates(&before, &after);
    let matched = |name: &str| crates.contains(name);
    let (old, new) = (
        inventory(&before, &matched, true).expect("before inventory"),
        inventory(&after, &matched, true).expect("after inventory"),
    );
    let reported: Vec<_> = visibility_changes(&old, &new)
        .into_iter()
        .filter(|change| scope.admits(change.key, change.after))
        .map(|change| change.key.2.clone())
        .collect();
    assert_eq!(reported, vec!["advance"]);
    let report = BodyReport::between(&old, &new, &scope);
    assert_eq!((report.exact, report.relocated), (1, 1));
    assert!(report.only_after.is_empty() && report.only_before.is_empty());
    for refused in ["", "crates/murail-fix/src/nowhere", "murail-absent"] {
        assert!(
            Scope::of(&before, &after, Some(refused)).is_err(),
            "{refused:?}"
        );
    }
}

fn classes(before: &str, after: &str) -> (usize, usize, usize, usize, usize) {
    let (before, after) = sides(before, after, true);
    let report = BodyReport::between(&before, &after, &Scope::Workspace);
    (
        report.exact,
        report.rerooted.len(),
        report.changed.len(),
        report.only_before.len(),
        report.only_after.len(),
    )
}

#[test]
fn a_move_that_rewrites_docs_visibility_and_layout_is_exact() {
    let after = QUEUE
        .replace("/// A queue.", "/// The queue, now in rt.")
        .replace("    fn push", "    pub(crate) fn push");
    // Queue, Pair, two impl headers, push, drop, helper.
    assert_eq!(classes(QUEUE, &after), (7, 0, 0, 0, 0));
    assert_eq!(
        classes(
            "fn f() { g(|x| h(x)); }\n",
            "fn f() {\n    g(|x| {\n        h(x)\n    });\n}\n"
        ),
        (1, 0, 0, 0, 0)
    );
}

#[test]
fn a_module_import_alias_reroots_like_its_path() {
    assert_eq!(
        classes(
            "use crate::dispatch;\nfn f() { dispatch::g() }\n",
            "fn f() { crate::load::classify::g() }\n"
        ),
        (0, 1, 0, 0, 0)
    );
    // A type annotation's `:` is not a path separator.
    assert_eq!(
        classes("fn f(s: crate::a::S) {}\n", "fn f(s: super::S) {}\n"),
        (0, 1, 0, 0, 0)
    );
    // A name the file does not import stays a module name.
    assert_eq!(
        classes(
            "fn f() { dispatch::g() }\n",
            "fn f() { crate::load::classify::g() }\n"
        ),
        (0, 0, 1, 0, 0)
    );
    // `std::fmt::x` keeps its interior `fmt` though `fmt` is an alias.
    assert_eq!(
        classes(
            "use crate::fmt;\nfn f() { std::fmt::x() }\n",
            "use crate::fmt;\nfn f() { std::x() }\n"
        ),
        (0, 0, 1, 0, 0)
    );
}

#[test]
fn a_retargeted_import_binding_is_changed() {
    // The body stays exact; the binding `a` now names another module.
    assert_eq!(
        classes(
            "use crate::a;\nfn f() { a::g() }\n",
            "use crate::b as a;\nfn f() { a::g() }\n"
        ),
        (1, 0, 1, 0, 0)
    );
    // A crate root re-exporting through its own child modules.
    assert_eq!(
        classes(
            "mod tick;\npub use tick::Engine;\n",
            "mod engine;\npub use engine::Engine;\n"
        ),
        (0, 0, 0, 0, 0)
    );
    // Rerooting the import itself is the move's own rewrite.
    assert_eq!(
        classes(
            "use crate::tick::Slot;\nfn f() -> Slot { Slot }\n",
            "use super::Slot;\nfn f() -> Slot { Slot }\n"
        ),
        (1, 0, 0, 0, 0)
    );
}

#[test]
fn crate_relative_rerooting_is_its_own_visible_class() {
    let after = QUEUE.replace("crate::rt::Slot::new", "super::Slot::new");
    assert_eq!(classes(QUEUE, &after), (6, 1, 0, 0, 0));
}

#[test]
fn semantic_changes_are_never_preserved() {
    // The check can go red: each pair differs in meaning.
    let cases = [
        (
            "fn f() -> u64 { u32::MAX as u64 }\n",
            "fn f() -> u64 { u64::MAX as u64 }\n",
        ),
        (
            "fn f() -> f64 { f32::EPSILON.into() }\n",
            "fn f() -> f64 { f64::EPSILON.into() }\n",
        ),
        (
            "fn f(v: &[u8]) -> u8 { v.iter().copied().collect::<u8>() }\n",
            "fn f(v: &[u8]) -> u8 { v.iter().copied().sum::<u8>() }\n",
        ),
        ("fn f() { a::g() }\n", "fn f() { b::g() }\n"),
        (
            "fn f(x: &mut u8, y: &mut u8) { std::mem::swap(x, y) }\n",
            "fn f(x: &mut u8, y: &mut u8) { std::ptr::swap(x, y) }\n",
        ),
        ("fn f() { crate::a::g() }\n", "fn f() { crate::b::G() }\n"),
        ("#[cfg(feature = \"x\")]\nfn f() {}\n", "fn f() {}\n"),
        ("const X: u32 = 1;\n", "const X: u32 = 2;\n"),
        ("enum E { A, B }\n", "enum E { A, C }\n"),
        (
            "trait T { fn f(&self) -> u8 { 1 } }\n",
            "trait T { fn f(&self) -> u8 { 2 } }\n",
        ),
        (
            "macro_rules! m { () => { 1 } }\n",
            "macro_rules! m { () => { 2 } }\n",
        ),
        (
            "fn f() { fn g() -> u8 { 1 } }\n",
            "fn f() { fn g() -> u8 { 2 } }\n",
        ),
        (
            "thread_local! { static S: u8 = 1; }\n",
            "thread_local! { static S: u8 = 2; }\n",
        ),
        (
            "fn f() { g(|x| { a(); h(x) }); }\n",
            "fn f() { g(|x| h(x)); }\n",
        ),
        (
            "fn f() -> bool { h(|x: bool| { x || true } && false) }\n",
            "fn f() -> bool { h(|x: bool| x || true && false) }\n",
        ),
        (
            "fn f(a: u8) -> u8 { a | { 1 } * 2 }\n",
            "fn f(a: u8) -> u8 { a | 1 * 2 }\n",
        ),
    ];
    for (before, after) in cases {
        assert_eq!(classes(before, after), (0, 0, 1, 0, 0), "{before}");
    }
}
#[test]
fn public_reports_retain_unreadable_and_ambiguous_evidence() {
    let before = workspace(&[("a.rs", "fn same() {}\nfn broken() {\n")]);
    let after = workspace(&[
        ("a.rs", "pub fn same() {}\nfn broken() {\n"),
        ("b.rs", "fn same() {}\n"),
    ]);
    assert!(
        visibility(&before, &after, None).is_err(),
        "invalid Rust has no structural inventory"
    );
    assert!(moved(&before, &after, None).is_err());
    assert!(after.syntax_error("crates/murail-fix/src/a.rs").is_some());
    // Ambiguity is still retained for valid, same-named sites.
    let before = workspace(&[("a.rs", "fn same() {}\n")]);
    let after = workspace(&[("a.rs", "pub fn same() {}\n"), ("b.rs", "fn same() {}\n")]);
    let report = visibility(&before, &after, None).expect("valid ambiguity report");
    assert_eq!(report.ambiguous_keys, 1);
    assert_eq!(report.ambiguous[0].name, "same");
    assert_eq!(report.ambiguous[0].after.len(), 2);
    assert!(report.widened.is_empty());
}
#[test]
fn removed_scopes_are_read_from_the_before_snapshot() {
    let before = workspace(&[("old/item.rs", "fn retired() {}\n")]);
    let after = workspace(&[]);
    let report = moved(&before, &after, Some("./crates/murail-fix/src/old/")).expect("old scope");
    assert_eq!(report.only_before.len(), 1);
    assert_eq!(report.only_before[0].name, "retired");
    assert_eq!(report.basis.before.sources.len(), 2);
    assert!(visibility(&before, &after, Some("crates/murail-fix/src/old")).is_ok());
}
#[test]
fn an_inherent_impl_loop_is_not_a_trait_header() {
    let snapshot = workspace(&[(
        "engine.rs",
        "impl Engine {\n    fn step(&self) { for value in values {} }\n}\n",
    )]);
    let found = inventory(&snapshot, &all, true).expect("inventory");
    assert!(found.keys().any(|key| key.1 == "impl" && key.2 == "Engine"));
    assert!(
        found
            .keys()
            .any(|key| key.1 == "method" && key.2 == "Engine::step")
    );
    assert!(!found.keys().any(|key| key.2.contains(" as ")));
}
#[test]
fn impl_comment_words_do_not_rekey_a_declaration() {
    assert_eq!(
        classes(
            "impl /* for Bogus */ Trait for Foo {\n    fn f() {}\n}\n",
            "impl Trait for Foo {\n    fn f() {}\n}\n"
        ),
        (2, 0, 0, 0, 0)
    );
}
#[test]
fn attributed_trait_impls_keep_the_trait_in_their_keys() {
    let snapshot = workspace(&[(
        "engine.rs",
        "#[cfg(feature = \"one\")]\nimpl Trait for Engine {\n    fn step(&self) {}\n}\n",
    )]);
    let found = inventory(&snapshot, &all, true).expect("inventory");
    assert!(
        found
            .keys()
            .any(|key| key.1 == "impl" && key.2 == "<Engine as Trait>")
    );
    assert!(
        found
            .keys()
            .any(|key| key.1 == "method" && key.2 == "<Engine as Trait>::step")
    );
}
#[test]
fn qualified_traits_keep_their_last_segment_after_token_formatting() {
    let snapshot = workspace(&[(
        "errors.rs",
        "impl std::fmt::Display for Refused {}\nimpl std::error::Error for Refused {}\n",
    )]);
    let found = inventory(&snapshot, &all, true).expect("inventory");
    assert!(found.keys().any(|key| key.2 == "<Refused as Display>"));
    assert!(found.keys().any(|key| key.2 == "<Refused as Error>"));
    assert!(!found.keys().any(|key| key.2.contains(" as std>")));
}
#[test]
fn const_argument_braces_do_not_end_an_impl_header() {
    assert_eq!(
        classes(
            "impl Trait<{1 + 2}> for Foo {}\n",
            "impl Trait<{1 + 3}> for Foo {}\n"
        ),
        (0, 0, 1, 0, 0)
    );
    assert_eq!(
        classes("#[cfg(feature = \"one\")]\nimpl Foo {}\n", "impl Foo {}\n"),
        (0, 0, 1, 0, 0)
    );
}
#[test]
fn unreadable_imports_cannot_look_like_no_binding_changes() {
    let before = workspace(&[("item.rs", "use crate::X;\nfn f() {}\n")]);
    let after = workspace(&[("item.rs", "use crate::{X as};\nfn f() {}\n")]);
    for bodies in [true, false] {
        let error = inventory(&after, &all, bodies)
            .err()
            .expect("unavailable import");
        assert!(error.to_string().contains("src/item.rs:1"));
    }
    assert!(moved(&before, &after, None).is_err());
    assert!(visibility(&before, &after, None).is_err());
}

#[test]
fn a_tuple_comma_is_not_layout() {
    assert_eq!(
        classes("fn f() { let x = (1,); }\n", "fn f() { let x = (1); }\n"),
        (0, 0, 1, 0, 0)
    );
}
#[test]
fn macro_input_is_not_visibility_documentation_or_layout() {
    for (before, after) in [
        ("fn f() { m!(pub); }\n", "fn f() { m!(); }\n"),
        ("#[custom(pub)]\nfn f() {}\n", "#[custom()]\nfn f() {}\n"),
        (
            "fn f() { m!(#[doc = \"x\"] item); }\n",
            "fn f() { m!(item); }\n",
        ),
        ("fn f() { m!(x,); }\n", "fn f() { m!(x); }\n"),
        (
            "macro_rules! m { (pub) => { 1 }; () => { 2 }; }\n",
            "macro_rules! m { () => { 1 }; () => { 2 }; }\n",
        ),
    ] {
        assert_eq!(classes(before, after), (0, 0, 1, 0, 0), "{before}");
    }
}
#[cfg(unix)]
#[test]
fn excluded_source_diagnostics_survive_the_comparison_basis() {
    let before = workspace(&[("item.rs", "fn f() {}\n")]);
    // Snapshot diagnostics are sealed; obtain this case through real discovery.
    let fixture = crate::test_support::Fixture::new(&[]);
    let root = fixture.path();
    fs::create_dir_all(root.join("src")).expect("fixture");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname=\"murail-fix\"\nversion=\"0.0.0\"\nedition=\"2024\"\n",
    )
    .expect("manifest");
    fs::write(root.join("src/lib.rs"), "fn f() {}\n").expect("source");
    std::os::unix::fs::symlink(root.join("src/lib.rs"), root.join("src/excluded.rs"))
        .expect("symlink");
    let metadata = cargo_metadata::MetadataCommand::new()
        .current_dir(root)
        .no_deps()
        .exec()
        .expect("metadata");
    let after = crate::discovery::snapshot_metadata(&metadata).expect("snapshot");
    let report = moved(&before, &after, None).expect("report");
    assert!(!report.basis.after.sources.contains_key("src/excluded.rs"));
    assert!(
        report
            .basis
            .after
            .diagnostics
            .iter()
            .any(|d| d.file == "src/excluded.rs")
    );
}
