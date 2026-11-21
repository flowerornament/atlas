//! The census on one written capture: owners, the home, grants, drops.
use super::*;

const LIB: &str = "pub struct Width;
pub fn render(x: u32) -> (u8, u8) {
    (x as u8, x as u8)
}
pub struct Shown;
impl std::fmt::Display for Shown {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let x = 1u32;
        let _ = x as u8;
        write!(f, \"x as u8\")
    }
}
impl std::fmt::Debug for Shown {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let x = 1u32;
        let _ = x as u8;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    fn t(x: u32) -> u8 { x as u8 }
}
";

fn snapshot() -> Snapshot {
    let fixture = crate::test_support::Fixture::new(&[
        (
            "Cargo.toml",
            "[package]\nname='fix'\nversion='0.0.0'\nedition='2024'\n[workspace]\n",
        ),
        ("src/lib.rs", LIB),
    ]);
    crate::discovery::snapshot_workspace(fixture.path()).expect("snapshot")
}
/// Each `x as u8` in the file, as a pattern named `narrow` would report it.
fn occurrences() -> Vec<(String, Occurrence)> {
    LIB.match_indices("x as u8")
        .map(|(start, text)| {
            let line = LIB[..start].lines().count();
            let occurrence = Occurrence::new(
                "src/lib.rs".into(),
                start,
                start + text.len(),
                line.max(1),
                text.into(),
            );
            ("narrow".to_owned(), occurrence)
        })
        .collect()
}
fn authority(extra: &str) -> AuthorityRule {
    toml::from_str(&format!(
        "fact = 'narrowing'\nhome = 'src/lib.rs::Width'\nforbid_new_pattern = [{{ name = 'narrow', pattern = '$X as u8' }}]\n{extra}"
    ))
    .expect("fixture authority")
}
fn held(census: &Census) -> Vec<(String, usize, usize)> {
    census
        .keys
        .iter()
        .map(|(key, held)| (key.item.clone(), held.count, held.debt()))
        .collect()
}

#[test]
fn occurrences_are_owned_by_their_item_and_test_code_and_strings_drop() {
    let found = census(&snapshot(), &authority(""), &[], &occurrences());
    assert_eq!(
        held(&found),
        [("Shown::fmt".into(), 2, 2), ("render".into(), 2, 2)],
        "{found:?}"
    );
    // The string literal and the test module's cast are dropped.
    assert_eq!(found.dropped, 2);
}

#[test]
fn a_trait_impl_row_bounds_its_own_impl_only() {
    let row = "allowed_in = [{ file = 'src/lib.rs', item = 'impl std::fmt::Display for Shown', counts = { 'fmt' = { narrow = 1 } }, reason = 'shows the narrowed value' }]";
    let found = census(&snapshot(), &authority(row), &[], &occurrences());
    // Display's cast is granted; Debug's, under the same owner name, is not.
    assert_eq!(held(&found)[0], ("Shown::fmt".into(), 2, 1), "{found:?}");
    assert_eq!(found.used.values().copied().collect::<Vec<_>>(), [1]);
}

#[test]
fn a_function_row_grants_up_to_its_bound_and_the_home_holds_no_debt() {
    let row = "allowed_in = [{ file = 'src/lib.rs', item = 'render', counts = { '' = { narrow = 1 } }, reason = 'one byte per channel' }]";
    let found = census(&snapshot(), &authority(row), &[], &occurrences());
    assert_eq!(held(&found)[1], ("render".into(), 2, 1), "{found:?}");
    let home = [("src/lib.rs".to_owned(), None)];
    assert!(
        census(&snapshot(), &authority(""), &home, &occurrences())
            .keys
            .is_empty()
    );
}

#[test]
fn the_fields_need_a_coherent_shape() {
    for (extra, error) in [
        (
            "forbid_new_observed = ['narrow']",
            "must be nonempty, unique and free of `:`",
        ),
        ("forbid_new_observed = ['a:b']", "free of `:`"),
        (
            "allowed_in = [{ file = 'src/lib.rs', item = 'render', counts = { '' = { narrow = 0 } }, reason = 'r' }]",
            "bounds `narrow` at 0",
        ),
        (
            "allowed_in = [{ file = 'src/lib.rs', item = 'render', counts = { '' = { other = 1 } }, reason = 'r' }]",
            "names `other`, which is neither",
        ),
        (
            "allowed_in = [{ file = 'src/lib.rs', item = 'render', counts = { '' = { narrow = 1 } }, reason = ' ' }]",
            "needs an item, a reason and counts",
        ),
    ] {
        let found = validate(&authority(extra)).unwrap_err().to_string();
        assert!(found.contains(error), "{extra}: {found}");
    }
    let lone: AuthorityRule = toml::from_str(
        "fact = 'f'\nhome = 'src/lib.rs::Width'\nallowed_in = [{ file = 'src/lib.rs', item = 'render', counts = { '' = { x = 1 } }, reason = 'r' }]",
    )
    .expect("fixture");
    assert!(
        validate(&lone)
            .unwrap_err()
            .to_string()
            .contains("no forbid_new_pattern or forbid_new_observed")
    );
    let class = authority(
        "forbid_new_observed = ['suffix']\nallowed_in = [{ file = 'src/lib.rs', item = 'render', counts = { '' = { 'suffix:1u8' = 1 } }, reason = 'r' }]",
    );
    assert!(validate(&class).is_ok());
}
