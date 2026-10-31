//! Native written-type claims at the snapshot -> cached AST -> check boundary.
use atlas_core::{
    analysis::Analysis,
    checks::typed_errors::{self, Reading, Unavailable},
    discovery::snapshot_workspace,
    population::TestSelection,
};
mod support;
use std::fs;
use support::Fixture;
fn fixture(source: &str) -> Fixture {
    Fixture::new(&[
        (
            "Cargo.toml",
            "[package]\nname='fixture'\nversion='0.0.0'\nedition='2024'\n[workspace]\n",
        ),
        ("src/lib.rs", source),
    ])
}

fn read(source: &str) -> Reading {
    let fixture = fixture(source);
    let snapshot = snapshot_workspace(fixture.path()).expect("capture fixture source");
    let analysis = Analysis::new(&snapshot).expect("create retained analysis");
    typed_errors::read(&analysis, &["fixture".into()])
        .expect("read native type rule")
        .remove("fixture")
        .expect("selected package has a reading")
}
#[test]
fn native_type_positions_and_qualified_paths_refuse() {
    for source in [
        "fn load() -> Result<(), String> { todo!() }",
        "fn load() -> std::result::Result<(), std::string::String> { todo!() }",
        "type Phase<T> = core::result::Result<T, alloc::string::String>;",
        "struct Cached { run: fn() -> Result<(), String> }",
        "struct Cached { result: Option<Result<(), String>> }",
        "impl Engine { fn load() -> Result<(), String> { todo!() } }",
        "type Phase<T> = ::core::result::Result<T, ::alloc::string::String>;",
    ] {
        let report = read(source);
        assert!(report.unavailable.is_empty(), "{source}");
        assert_eq!(
            report.boundaries["src/lib.rs"]
                .iter()
                .copied()
                .collect::<Vec<_>>(),
            [1],
            "{source}"
        );
    }
}
#[test]
fn native_qualified_type_path_trivia_cannot_hide_string_errors() {
    for source in [
        "fn load() -> std::result::Result<(), std::string::String> { todo!() }",
        "fn load() -> std :: result :: Result<(), std :: string :: String> { todo!() }",
        "fn load() -> std::result::Result<(), std::string:: /* kept */ String> { todo!() }",
    ] {
        let report = read(source);
        assert!(report.unavailable.is_empty());
        assert!(!report.boundaries.is_empty(), "{source}");
    }
}
#[test]
fn native_values_comments_and_test_only_items_are_not_error_boundaries() {
    for source in [
        "fn text() -> Result<String, DomainError> { todo!() }",
        "type Pair = Other<(), String>;",
        "const TEXT: &str = \"Result<(), String>\";",
        "// fn load() -> Result<(), String> {}",
        "#[cfg(test)]\nfn helper() -> Result<(), String> { todo!() }",
        "#[cfg(test)]\nmod fixtures { fn helper() -> Result<(), String> { todo!() } }",
        "#[cfg(all(test, feature = \"gpu\"))]\nimpl Engine { fn helper() -> Result<(), String> { todo!() } }",
        "type Raw = r#Result<(),String>;",
        "type RawError = Result<(),r#String>;",
        "type Paren = Result<(),(String)>;",
        "type Borrow = Result<(), &String>;",
        "type QualifiedSelf<T> = Result<(), <T as Trait>::String>;",
    ] {
        let report = read(source);
        assert!(report.unavailable.is_empty(), "{source}");
        assert!(report.boundaries.is_empty(), "{source}");
    }
    assert!(
        !read("#[cfg(any(test, feature = \"gpu\"))]\nfn load() -> Result<(), String> { todo!() }")
            .boundaries
            .is_empty()
    );
    assert!(
        !read("#[cfg(test)]\nfn helper() {}\nfn load() -> Result<(), String> { todo!() }")
            .boundaries
            .is_empty()
    );
}
#[test]
fn repeated_type_matches_on_one_source_line_keep_the_native_line_unit() {
    let report = read("struct Fields { a: Result<(),String>, b: Option<Result<(),String>> }\n");
    assert_eq!(report.boundaries["src/lib.rs"].len(), 1);
}
#[test]
fn unavailable_syntax_cannot_become_zero_and_checks_never_reopen_source() {
    let bad = read("pub fn broken(");
    assert!(bad.boundaries.is_empty());
    assert!(matches!(
        bad.unavailable.as_slice(),
        [Unavailable::Syntax { .. }]
    ));
    let fixture = fixture("pub fn load()->Result<(),String> { todo!() }\n");
    let snapshot = snapshot_workspace(fixture.path()).unwrap();
    fs::remove_file(fixture.path().join("src/lib.rs")).unwrap();
    let analysis = Analysis::new(&snapshot)
        .unwrap()
        .with_tests(TestSelection::All);
    for _ in 0..2 {
        let reports = typed_errors::read(&analysis, &["fixture".into()]).unwrap();
        assert!(!reports["fixture"].boundaries.is_empty());
        assert!(reports["fixture"].unavailable.is_empty());
    }
    assert!(typed_errors::read(&analysis, &["absent".into()]).is_err());
    assert!(typed_errors::read(&analysis, &["fixture".into(), "fixture".into()]).is_err());
}
#[cfg(unix)]
#[test]
fn an_excluded_source_directory_is_unavailable_not_an_empty_census() {
    let fixture = fixture("pub fn clean() {}\n");
    let outside = fixture.path().join("outside");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("hidden.rs"), "type Hidden=Result<(),String>;").unwrap();
    std::os::unix::fs::symlink(outside, fixture.path().join("src/hidden")).unwrap();
    let snapshot = snapshot_workspace(fixture.path()).unwrap();
    let analysis = Analysis::new(&snapshot).unwrap();
    let reports = typed_errors::read(&analysis, &["fixture".into()]).unwrap();
    assert!(reports["fixture"].boundaries.is_empty());
    assert!(
        reports["fixture"]
            .unavailable
            .iter()
            .any(|issue| matches!(issue, Unavailable::ExcludedSource { .. }))
    );
}
