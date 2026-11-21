//! The law over a change, clause by clause.
use super::*;

fn section(extra: &str) -> ConcentrationPolicy {
    toml::from_str(&format!(
        "population = ['fix']\nentry_lines = 300\nfile_lines = 2000\nconventional_names = []\nfunction_lines = 10\nfunction_nesting = 3\n{extra}"
    ))
    .expect("fixture section")
}
fn at(file: &str, function: &str, lines: usize, fingerprint: &str) -> Measured {
    Measured {
        file: file.into(),
        function: function.into(),
        lines,
        nesting: 1,
        fingerprint: fingerprint.into(),
    }
}
const ROW: &str = "function = [{ file = 'a.rs', function = 'big', lines = 12 }]";

fn limits(lines: Option<usize>, extra: &str) -> ConcentrationPolicy {
    let lines = lines.map_or_else(String::new, |n| format!("function_lines = {n}\n"));
    toml::from_str(&format!(
        "population = ['fix']\nentry_lines = 300\nfile_lines = 2000\nconventional_names = []\nfunction_nesting = 3\n{lines}{extra}"
    ))
    .expect("fixture section")
}

#[test]
fn a_raised_or_removed_limit_and_a_new_generated_path_refuse() {
    let published = limits(Some(10), "");
    for (policy, refusal) in [
        (limits(Some(10), ""), None),
        (limits(Some(9), ""), None),
        (
            limits(Some(11), ""),
            Some("function_lines rose from 10 to 11"),
        ),
        (limits(None, ""), Some("function_lines 10 was removed")),
        (
            limits(
                Some(10),
                "generated = [{ path = 'gen', reason = 'a tool' }]",
            ),
            Some("generated gains `gen`"),
        ),
    ] {
        let found = paired(&policy, &published, &[], &[]);
        match refusal {
            None => assert!(found.refusals.is_empty(), "{:?}", found.refusals),
            Some(text) => assert!(
                found.refusals.iter().any(|r| r.contains(text)),
                "{text}: {:?}",
                found.refusals
            ),
        }
    }
}

#[test]
fn a_new_row_and_a_risen_row_are_raises_with_their_owner() {
    let published = section(ROW);
    let base = [at("a.rs", "big", 12, "x"), at("a.rs", "new", 5, "n")];
    let now = [at("a.rs", "big", 14, "y"), at("a.rs", "new", 13, "m")];
    let policy = section(
        "function = [{ file = 'a.rs', function = 'big', lines = 14, owner = 'ticket-1' }, { file = 'a.rs', function = 'new', lines = 13 }]",
    );
    let found = paired(&policy, &published, &base, &now);
    assert!(found.refusals.is_empty(), "{:?}", found.refusals);
    let raises: Vec<_> = found
        .raises
        .iter()
        .map(|r| {
            (
                r.subject.as_str(),
                r.before,
                r.after,
                r.measured,
                r.owner.as_deref(),
            )
        })
        .collect();
    assert_eq!(
        raises,
        [
            (
                "concentration.function.a.rs::big.lines",
                Some(12),
                14,
                14,
                Some("ticket-1")
            ),
            ("concentration.function.a.rs::new.lines", None, 13, 13, None),
        ]
    );
}

#[test]
fn growth_within_an_unchanged_row_refuses() {
    let policy = section(ROW);
    // The row says 12 but the base measured 11: the cap is 11.
    let found = paired(
        &policy,
        &policy,
        &[at("a.rs", "big", 11, "x")],
        &[at("a.rs", "big", 12, "y")],
    );
    assert!(found.raises.is_empty());
    assert!(
        found.refusals[0].contains("grew to 12 lines, above its base 11 (row 12)"),
        "{:?}",
        found.refusals
    );
    let held = paired(
        &policy,
        &policy,
        &[at("a.rs", "big", 12, "x")],
        &[at("a.rs", "big", 12, "y")],
    );
    assert!(held.refusals.is_empty() && held.raises.is_empty());
}

#[test]
fn a_row_follows_an_unchanged_move_and_nothing_else() {
    let published = section(ROW);
    let moved = section("function = [{ file = 'b.rs', function = 'big', lines = 12 }]");
    let base = [at("a.rs", "big", 12, "same")];
    // Moved unchanged: the row follows, no raise.
    let found = paired(&moved, &published, &base, &[at("b.rs", "big", 12, "same")]);
    assert!(
        found.raises.is_empty() && found.refusals.is_empty(),
        "{:?}",
        found.raises
    );
    // Moved and edited: a new function, so a new row is a raise.
    let found = paired(
        &moved,
        &published,
        &base,
        &[at("b.rs", "big", 12, "edited")],
    );
    assert_eq!(found.raises.len(), 1);
    assert_eq!(found.raises[0].before, None);
    // The old one still there: an unrelated `big` elsewhere inherits nothing.
    let found = paired(
        &moved,
        &published,
        &base,
        &[at("a.rs", "big", 12, "same"), at("b.rs", "big", 12, "same")],
    );
    assert_eq!(found.raises.len(), 1, "{:?}", found.raises);
}

#[test]
fn a_twin_inserted_above_does_not_take_the_row() {
    // The row holds the second `f`; a new twin above renumbers it to `f#3`.
    let published = section("function = [{ file = 'a.rs', function = 'f#2', lines = 12 }]");
    let base = [at("a.rs", "f", 1, "first"), at("a.rs", "f#2", 12, "held")];
    let now = [
        at("a.rs", "f", 1, "first"),
        at("a.rs", "f#2", 12, "inserted"),
        at("a.rs", "f#3", 12, "held"),
    ];
    // The row moved with its function: no raise.
    let followed = section(
        "function = [{ file = 'a.rs', function = 'f#3', lines = 12 }, { file = 'a.rs', function = 'f#2', lines = 12 }]",
    );
    let found = paired(&followed, &published, &base, &now);
    let raised: Vec<_> = found.raises.iter().map(|r| r.subject.as_str()).collect();
    // `f#2` is now another function: its row is new, a raise.
    assert_eq!(
        raised,
        ["concentration.function.a.rs::f#2.lines"],
        "{:?}",
        found.raises
    );
}
