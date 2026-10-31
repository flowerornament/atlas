//! Contract identity ambiguity is judged over the observations in the question.
mod support;
use atlas::{
    analysis::Analysis,
    comparison, discovery,
    representations::{Facts, Query},
};
use support::Fixture;

fn facts(source: &str) -> Facts {
    let fixture = Fixture::new(&[("src/lib.rs", source)]);
    let snapshot =
        discovery::snapshot_workspace(fixture.path()).expect("capture comparison fixture");
    Analysis::new(&snapshot)
        .expect("comparison analysis")
        .representations()
        .expect("comparison facts")
}
#[test]
fn unrelated_duplicate_identities_do_not_poison_a_selected_contract() {
    let before = facts(
        "pub struct Token;\npub fn make() -> Token { Token }\n#[cfg(unix)]\nfn helper() {}\n#[cfg(not(unix))]\nfn helper() {}\n",
    );
    let after = facts(
        "pub struct Token;\npub fn make() -> Token { Token }\npub fn another() -> Token { Token }\n#[cfg(unix)]\nfn helper() {}\n#[cfg(not(unix))]\nfn helper() {}\n",
    );
    let query: Query =
        serde_json::from_str(r#"{"title":"Token contracts","types":[{"name":"Token"}]}"#).unwrap();
    let report = comparison::compare(
        &before,
        &after,
        Some(&query),
        ("before".into(), "after".into()),
    )
    .unwrap();
    assert!(
        report
            .changes
            .iter()
            .any(|change| change.subject.contains("another") && change.after.is_some())
    );
}
#[test]
fn selected_contract_and_constructor_ambiguity_still_refuses() {
    let query: Query =
        serde_json::from_str(r#"{"title":"Token contracts","types":[{"name":"Token"}]}"#).unwrap();
    for bodies in [
        "#[cfg(unix)]\npub fn make() -> Token { Token }\n#[cfg(not(unix))]\npub fn make() -> Token { Token }\n",
        "#[cfg(unix)]\npub fn make() { let _ = Token {}; }\n#[cfg(not(unix))]\npub fn make() { let _ = Token {}; }\n",
        "#[cfg(unix)]\npub fn make() { let _ = Token::default(); }\n#[cfg(not(unix))]\npub fn make() { let _ = Token::default(); }\n",
    ] {
        let source = format!("pub struct Token {{}}\n{bodies}");
        let observed = facts(&source);
        let error = comparison::compare(
            &observed,
            &observed,
            Some(&query),
            ("before".into(), "after".into()),
        )
        .unwrap_err();
        assert!(
            error.to_string().contains("ambiguous stable source symbol"),
            "{bodies}: {error}"
        );
    }
}
