use atlas_core::source::{ItemKind, RustSourceProjection, non_test_loc};
use std::path::Path;

#[test]
fn native_evidence_surfaces_exclude_fake_annotations_and_preserve_attachment() {
    let source = "const FAKE: &str = r#\"\n// fm:fake\n\"#;\n\n// fm:claim\n#[doc = \"item\"]\npub fn actual() {}\n";
    let projection = RustSourceProjection::of(source.into());
    let comments: Vec<_> = projection.line_comment_lines().collect();
    assert_eq!(comments, vec![(5, "// fm:claim")]);
    assert!(!projection.starts_in_code(1));
    let item = projection
        .outline(Path::new("lib.rs"))
        .into_iter()
        .find(|i| i.name == "actual")
        .unwrap();
    assert_eq!(item.kind, ItemKind::Fn);
    assert_eq!(*item.span.start(), 5);
    assert_eq!(*item.span.end(), 7);
}

#[test]
fn loc_keeps_native_helper_boundary_distinct_from_test_modules() {
    let source = "#[cfg(test)]\nfn helper() {}\nfn live() {}\n#[cfg(test)]\nmod tests {\n fn test() {}\n}\nfn after() {}\n";
    assert_eq!(non_test_loc(source), 4);
    let projection = RustSourceProjection::of(source.into());
    let names: Vec<_> = projection
        .declaration_lines()
        .map(|(_, item)| item.name.to_owned())
        .collect();
    assert!(names.iter().any(|name| name == "after"));
}

#[test]
fn restricted_visibility_preserves_its_written_target() {
    let projection = RustSourceProjection::of(
        "pub(in crate::sealed) struct Restricted;\npub struct Public;\n".into(),
    );
    let outline = projection.outline(Path::new("lib.rs"));
    assert_eq!(outline[0].visibility_restriction(), Some("crate::sealed"));
    assert_eq!(outline[1].visibility_restriction(), None);
}

#[test]
fn native_field_and_impl_facts_preserve_owner_and_visibility_syntax() {
    use atlas_core::source::{Visibility, impl_trait, restricted_path, struct_fields};
    let fields = struct_fields(
        "pub struct Bag<T> where T: Fn(u32) -> (u32, u32) {\n    pub(in crate::sealed) first: T,\n    #[cfg(any())] pub(super) second: u32,\n}",
    );
    assert_eq!(fields.len(), 2);
    assert_eq!(
        (fields[0].0, fields[0].1.as_str(), fields[0].2),
        (1, "first", Visibility::PubIn)
    );
    assert_eq!(fields[0].3.as_deref(), Some("crate::sealed"));
    assert_eq!(
        (fields[1].0, fields[1].1.as_str(), fields[1].2),
        (2, "second", Visibility::PubSuper)
    );
    let tuple = struct_fields("pub struct Pair<T>(pub T, pub(crate) usize);");
    assert_eq!(
        tuple
            .iter()
            .map(|field| field.1.as_str())
            .collect::<Vec<_>>(),
        ["0", "1"]
    );
    assert!(struct_fields("pub struct Unit;").is_empty());
    assert_eq!(
        impl_trait("impl<T> crate::api::Trait<T> for Bag<T> {"),
        Some("Trait")
    );
    assert_eq!(impl_trait("impl<T> Bag<T> {"), None);
    assert_eq!(
        restricted_path("pub(in crate :: sealed) first: T"),
        Some("crate::sealed".into())
    );
    assert_eq!(restricted_path("pub(crate) field: T"), None);
}

#[test]
fn lexical_paths_accept_whitespace_around_separators() {
    use atlas_core::source::impl_trait;
    assert_eq!(
        impl_trait("impl std :: fmt :: Display for Error {"),
        Some("Display")
    );
    assert_eq!(
        impl_trait("impl ::std::error::Error for Refused {"),
        Some("Error")
    );
}
