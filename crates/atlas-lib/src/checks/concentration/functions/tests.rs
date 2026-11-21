//! The tree law and the shape of the function fields, clause by clause.
use super::*;

fn policy(rows: &str) -> ConcentrationPolicy {
    toml::from_str(&format!(
        "population = ['fix']\nentry_lines = 300\nfile_lines = 2000\nconventional_names = []\nfunction_lines = 10\nfunction_nesting = 3\n{rows}"
    ))
    .expect("fixture section")
}
fn function(name: &str, lines: usize, nesting: usize) -> Measured {
    Measured {
        file: "src/lib.rs".into(),
        function: name.into(),
        lines,
        nesting,
        fingerprint: String::new(),
    }
}

#[test]
fn an_unlisted_function_over_a_limit_refuses_with_its_row() {
    let found = tree(
        &policy(""),
        &[
            function("big", 12, 2),
            function("deep", 4, 5),
            function("fine", 10, 3),
        ],
    );
    assert_eq!(found.refusals.len(), 2, "{:?}", found.refusals);
    assert!(
        found.refusals[0].contains("`big` in `src/lib.rs` is 12 code lines, above the 10 limit")
    );
    assert!(found.refusals[0].contains(r#"{ file = "src/lib.rs", function = "big", lines = 12 }"#));
    assert!(
        found.refusals[1].contains("`deep` in `src/lib.rs` is 5 levels deep, above the 3 limit")
    );
}

#[test]
fn a_row_holds_its_measure_and_never_rises() {
    let held = policy("function = [{ file = 'src/lib.rs', function = 'big', lines = 12 }]");
    assert!(tree(&held, &[function("big", 12, 2)]).refusals.is_empty());
    let risen = tree(&held, &[function("big", 13, 2)]);
    assert!(
        risen.refusals[0].contains("above its row's lines 12"),
        "{:?}",
        risen.refusals
    );
    // A row that gives `lines` does not hold the other measure.
    let deeper = tree(&held, &[function("big", 12, 4)]);
    assert!(
        deeper.refusals[0].contains("its row gives no `nesting`"),
        "{:?}",
        deeper.refusals
    );
}

#[test]
fn a_row_below_its_measure_or_gone_is_a_hint_or_for_an_owner_a_refusal() {
    let unowned = policy("function = [{ file = 'src/lib.rs', function = 'big', lines = 12 }]");
    let owned = policy(
        "function = [{ file = 'src/lib.rs', function = 'big', lines = 12, owner = 'ticket-1' }]",
    );
    for (measure, hint) in [
        (
            vec![function("big", 11, 2)],
            "can ratchet `lines` to 11 (row 12)",
        ),
        (
            vec![function("big", 9, 2)],
            "can drop `lines = 12`: 9 code lines, within the 10 limit",
        ),
        (
            vec![],
            "can be deleted: no production function has that name there",
        ),
    ] {
        let found = tree(&unowned, &measure);
        assert!(found.refusals.is_empty(), "{:?}", found.refusals);
        assert!(
            found.hints.iter().any(|text| text.contains(hint)),
            "{hint}: {:?}",
            found.hints
        );
        let found = tree(&owned, &measure);
        assert!(
            found
                .refusals
                .iter()
                .any(|text| text.contains(hint) && text.contains("owned by `ticket-1`")),
            "{hint}: {:?}",
            found.refusals
        );
    }
}

#[test]
fn the_function_fields_need_a_coherent_shape() {
    for (rows, error) in [
        (
            "function = [{ file = 'a.rs', function = 'f' }]",
            "gives neither `lines` nor `nesting`",
        ),
        (
            "function = [{ file = 'a.rs', function = 'f', lines = 11 }, { file = 'a.rs', function = 'f', lines = 12 }]",
            "duplicate function row `f` in `a.rs`",
        ),
        (
            "generated = [{ path = 'gen', reason = 'bindgen' }]\nfunction = [{ file = 'gen/a.rs', function = 'f', lines = 11 }]",
            "is under a `generated` path",
        ),
        (
            "generated = [{ path = 'gen', reason = ' ' }]",
            "need a path and a reason",
        ),
    ] {
        let found = validate(&policy(rows)).unwrap_err().to_string();
        assert!(found.contains(error), "{rows}: {found}");
    }
    let no_limit: ConcentrationPolicy = toml::from_str(
        "population = ['fix']\nentry_lines = 300\nfile_lines = 2000\nconventional_names = []\nfunction = [{ file = 'a.rs', function = 'f', nesting = 4 }]",
    )
    .expect("fixture");
    let found = validate(&no_limit).unwrap_err().to_string();
    assert!(
        found.contains("gives `nesting`, but `function_nesting` is not set"),
        "{found}"
    );
}
