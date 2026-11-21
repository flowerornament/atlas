//! `forbid_new_*` over a change: held spelling debt outside the home may
//! only fall, and moves only with its item. Occurrences are supplied as a
//! wrapper would, an observed class `narrow` at each `as u8`.
mod support;
use atlas_lib::checks::{
    concentration,
    delta::{self, Basis, Inputs},
    policy::{Occurrence, Policy, SpellingState},
};
use atlas_lib::discovery::{Snapshot, snapshot_workspace};
use std::collections::BTreeMap;
use support::Fixture;

const POLICY: &str = "schema=1\nprovenance='authored'\nsurface={fix=50}\nauthority=[{fact='narrowing',home='src/width.rs::Width',forbid_new_observed=['narrow']ALLOWED}]\n[population]\ncore=['fix']\n[size]\ncore_token_lines=1000\ncrate={fix=1000}\n[crate_graph]\nrule=[{name='fix',allowed=[],law='leaf'}]\n[concentration]\npopulation=['fix']\nentry_lines=300\nfile_lines=1000\nconventional_names=[]\n";
fn policy(allowed: &str) -> Policy {
    Policy::parse(&POLICY.replace("ALLOWED", allowed)).expect("policy")
}
/// A capture of the crate with `files` beside `lib.rs` and the home.
fn capture(files: &[(&str, &str)]) -> (Fixture, Snapshot) {
    let modules = files
        .iter()
        .map(|(path, _)| format!("pub mod {};\n", path.trim_end_matches(".rs")))
        .collect::<Vec<_>>()
        .concat();
    let mut all = vec![
        (
            "Cargo.toml".to_owned(),
            "[package]\nname='fix'\nversion='0.0.0'\nedition='2024'\n[workspace]\n".to_owned(),
        ),
        (
            "src/lib.rs".to_owned(),
            format!("//! fix\npub mod width;\n{modules}"),
        ),
        ("src/width.rs".to_owned(), "pub struct Width;\n".to_owned()),
    ];
    all.extend(
        files
            .iter()
            .map(|(p, t)| (format!("src/{p}"), (*t).to_owned())),
    );
    let refs: Vec<(&str, &str)> = all.iter().map(|(p, t)| (p.as_str(), t.as_str())).collect();
    let fixture = Fixture::new(&refs);
    let snapshot = snapshot_workspace(fixture.path()).expect("snapshot");
    (fixture, snapshot)
}
/// What a wrapper observing `as u8` would supply for this capture.
fn observed(snapshot: &Snapshot) -> BTreeMap<String, BTreeMap<String, SpellingState>> {
    let mut sources = BTreeMap::new();
    let mut occurrences = Vec::new();
    for file in snapshot.graph().files.iter().filter(|f| !f.test_only) {
        sources.insert(file.path.clone(), file.content_fingerprint.clone());
        let text = snapshot.source(&file.path).unwrap_or_default();
        for (start, found) in text.match_indices("as u8") {
            let line = text[..start].matches('\n').count() + 1;
            occurrences.push(Occurrence::new(
                file.path.clone(),
                start,
                start + found.len(),
                line,
                found.into(),
            ));
        }
    }
    let state = SpellingState::Read {
        sources,
        occurrences,
    };
    BTreeMap::from([(
        "narrowing".into(),
        BTreeMap::from([("narrow".into(), state)]),
    )])
}
fn judge(
    published: &Policy,
    policy: &Policy,
    before: &[(&str, &str)],
    after: &[(&str, &str)],
) -> delta::Report {
    judge_citing(published, policy, before, after, &[])
}
/// As `judge`, with a description citing `cited` (parsed as the project's
/// citation reader would).
fn judge_citing(
    published: &Policy,
    policy: &Policy,
    before: &[(&str, &str)],
    after: &[(&str, &str)],
    cited: &[&str],
) -> delta::Report {
    let description = format!("change ({})", cited.join(", "));
    let (_b, base) = capture(before);
    let (_a, now) = capture(after);
    let mut inputs = Inputs::new(
        &now,
        Basis {
            baseline: "b".into(),
            candidate: "a".into(),
        },
        description.clone(),
    );
    inputs.policy.observed = observed(&now);
    inputs.before_policy.observed = observed(&base);
    inputs.concentration = concentration::Inputs {
        before_headers: concentration::observe_headers(&base, &["fix"]).expect("headers"),
        after_headers: concentration::observe_headers(&now, &["fix"]).expect("headers"),
        citations: Some(concentration::Citations {
            description_fingerprint: atlas_lib::discovery::fingerprint(description.as_bytes()),
            beads: cited.iter().map(|bead| (*bead).to_owned()).collect(),
        }),
        test_support_features: Vec::new(),
    };
    delta::check(&base, &now, published, policy, &inputs).expect("delta")
}
fn spelling_findings(report: &delta::Report) -> Vec<&String> {
    report
        .violations
        .iter()
        .filter(|v| v.contains("narrow"))
        .collect()
}
const RENDER: &str = "pub fn render(x: u32) -> (u8, u8) { (x as u8, x as u8) }\n";

#[test]
fn held_debt_stays_and_growth_refuses() {
    let p = policy("");
    let same = judge(&p, &p, &[("a.rs", RENDER)], &[("a.rs", RENDER)]);
    assert!(spelling_findings(&same).is_empty(), "{:?}", same.violations);
    let summary = &same.spellings["narrowing"];
    assert_eq!((summary.debt_before, summary.debt_after), (2, 2));
    // Blank lines and reordering change no key.
    let moved = judge(
        &p,
        &p,
        &[("a.rs", RENDER)],
        &[("a.rs", &format!("\n\npub fn other() {{}}\n{RENDER}"))],
    );
    assert!(
        spelling_findings(&moved).is_empty(),
        "{:?}",
        moved.violations
    );
    let third = "pub fn render(x: u32) -> (u8, u8, u8) { (x as u8, x as u8, x as u8) }\n";
    let grown = judge(&p, &p, &[("a.rs", RENDER)], &[("a.rs", third)]);
    assert!(
        spelling_findings(&grown)[0]
            .contains("`src/a.rs::render` holds 3 `narrow:as u8` outside its home (base 2)"),
        "{:?}",
        grown.violations
    );
    let new = judge(
        &p,
        &p,
        &[("a.rs", RENDER)],
        &[(
            "a.rs",
            &format!("{RENDER}pub fn more(x: u32) -> u8 {{ x as u8 }}\n"),
        )],
    );
    assert!(
        spelling_findings(&new)[0].contains("`src/a.rs::more` newly holds 1"),
        "{:?}",
        new.violations
    );
    let renamed = judge(
        &p,
        &p,
        &[("a.rs", RENDER)],
        &[("a.rs", &RENDER.replace("render", "paint"))],
    );
    assert!(
        spelling_findings(&renamed)[0].contains("`src/a.rs::paint` newly holds 2"),
        "{:?}",
        renamed.violations
    );
}

#[test]
fn debt_moves_only_with_its_item() {
    let p = policy("");
    // An item moved between kept files, and a whole file renamed: neutral.
    let between = judge(
        &p,
        &p,
        &[("a.rs", RENDER), ("b.rs", "pub fn b() {}\n")],
        &[("a.rs", "pub fn a() {}\n"), ("b.rs", RENDER)],
    );
    assert!(
        spelling_findings(&between).is_empty(),
        "{:?}",
        between.violations
    );
    let renamed_file = judge(&p, &p, &[("a.rs", RENDER)], &[("c.rs", RENDER)]);
    assert!(
        spelling_findings(&renamed_file).is_empty(),
        "{:?}",
        renamed_file.violations
    );
    // A moved file keeps its items' debt; a new spelling there still refuses.
    let moved_and_grown = judge(
        &p,
        &p,
        &[("a.rs", RENDER)],
        &[(
            "c.rs",
            &format!("{RENDER}pub fn more(x: u32) -> u8 {{ x as u8 }}\n"),
        )],
    );
    assert_eq!(
        spelling_findings(&moved_and_grown).len(),
        1,
        "{:?}",
        moved_and_grown.violations
    );
    // A copy whose original stays frees nothing.
    let copied = judge(
        &p,
        &p,
        &[("a.rs", RENDER)],
        &[("a.rs", RENDER), ("b.rs", RENDER)],
    );
    assert!(
        spelling_findings(&copied)[0].contains("`src/b.rs::render` newly holds 2"),
        "{:?}",
        copied.violations
    );
    // A reduction in `a.rs::new`, which stays, pays for no growth in `b.rs::new`.
    let a_new = |n: usize| {
        format!(
            "pub fn new(x: u32) -> u8 {{ {} }}\n",
            vec!["x as u8"; n].join(" + ")
        )
    };
    let traded = judge(
        &p,
        &p,
        &[("a.rs", &a_new(2)), ("b.rs", &a_new(1))],
        &[("a.rs", &a_new(1)), ("b.rs", &a_new(2))],
    );
    assert!(
        spelling_findings(&traded)[0].contains("`src/b.rs::new` holds 2"),
        "{:?}",
        traded.violations
    );
}

#[test]
fn allowances_raise_exactly_and_a_guard_stays() {
    let none = policy("");
    let row = |n: usize| {
        policy(&format!(
            ",allowed_in=[{{file='src/a.rs',item='render',counts={{''={{'narrow:as u8'={n}}}}},reason='one byte per channel'}}]"
        ))
    };
    let exact = judge(&none, &row(2), &[("a.rs", RENDER)], &[("a.rs", RENDER)]);
    assert!(
        spelling_findings(&exact).is_empty(),
        "{:?}",
        exact.violations
    );
    assert!(
        exact.raises.iter().any(|r| r.subject
            == "authority.narrowing.allowed_in.src/a.rs::render..narrow:as u8"
            && r.after == 2
            && r.measured == 2
            && r.before.is_none()),
        "{:?}",
        exact.raises
    );
    let inexact = judge(&none, &row(3), &[("a.rs", RENDER)], &[("a.rs", RENDER)]);
    assert!(
        spelling_findings(&inexact)
            .iter()
            .any(|v| v.contains("raised to 3, measured 2")),
        "{:?}",
        inexact.violations
    );
    let dropped = Policy::parse(&POLICY.replace(",forbid_new_observed=['narrow']ALLOWED", ""))
        .expect("policy");
    let removed = judge(&none, &dropped, &[("a.rs", RENDER)], &[("a.rs", RENDER)]);
    assert!(
        removed
            .violations
            .iter()
            .any(|v| v.contains("`narrow` removed or its pattern changed while the base holds 2")),
        "{:?}",
        removed.violations
    );
}

#[test]
fn a_cited_amendment_is_the_one_way_to_drop_a_guard_holding_debt() {
    let held = policy("");
    let with = |amendment: &str| {
        Policy::parse(&POLICY.replace(
            ",forbid_new_observed=['narrow']ALLOWED",
            &format!(",amended_by={{bead='x-1',covers=[{amendment}]}}"),
        ))
        .expect("policy")
    };
    let tree = [("a.rs", RENDER)];
    let amended = judge_citing(&held, &with("'narrow'"), &tree, &tree, &["x-1"]);
    assert!(amended.violations.is_empty(), "{:?}", amended.violations);
    let uncited = judge_citing(&held, &with("'narrow'"), &tree, &tree, &[]);
    assert!(
        uncited.violations.iter().any(|v| v
            .contains("amended_by names `x-1`, which this landing's description does not cite")),
        "{:?}",
        uncited.violations
    );
    let wide = judge_citing(&held, &with("'narrow','other'"), &tree, &tree, &["x-1"]);
    assert!(
        wide.violations
            .iter()
            .any(|v| v.contains("covers `other`, which this landing neither changes nor removes")),
        "{:?}",
        wide.violations
    );
    // Removing the authority entry removes its guards: nowhere to amend.
    let gone = Policy::parse(&POLICY.replace(
        "authority=[{fact='narrowing',home='src/width.rs::Width',forbid_new_observed=['narrow']ALLOWED}]\n",
        "authority=[{fact='other',home='src/width.rs::Width'}]\n",
    ))
    .expect("policy");
    let removed = judge(&held, &gone, &tree, &tree);
    assert!(
        removed
            .violations
            .iter()
            .any(|v| v.contains("`narrow` removed or its pattern changed while the base holds 2")),
        "{:?}",
        removed.violations
    );
}

#[test]
fn an_unobserved_or_stale_side_is_unavailable() {
    let p = policy("");
    let (_b, base) = capture(&[("a.rs", RENDER)]);
    let (_a, now) = capture(&[("a.rs", RENDER)]);
    let mut inputs = Inputs::new(
        &now,
        Basis {
            baseline: "b".into(),
            candidate: "a".into(),
        },
        "d".into(),
    );
    inputs.policy.observed = observed(&now);
    let report = delta::check(&base, &now, &p, &p, &inputs).expect("delta");
    assert!(
        report
            .unavailable
            .iter()
            .any(|m| m.field == "delta.before.authority.narrowing.forbid_new.narrow"),
        "{:?}",
        report
            .unavailable
            .iter()
            .map(|m| &m.field)
            .collect::<Vec<_>>()
    );
    inputs.before_policy.observed = observed(&now);
    let (_c, other) = capture(&[("a.rs", "pub fn changed() {}\n")]);
    inputs.before_policy.observed = observed(&other);
    let report = delta::check(&base, &now, &p, &p, &inputs).expect("delta");
    assert!(
        report
            .unavailable
            .iter()
            .any(|m| m.reason.contains("observation basis differs")),
        "{:?}",
        report
            .unavailable
            .iter()
            .map(|m| &m.reason)
            .collect::<Vec<_>>()
    );
}

/// A home naming a `mod` item is that module: spellings in its files are
/// at home, not debt (atlas-3n1.3).
#[test]
fn a_mod_item_home_is_its_module() {
    let files = [
        (
            "Cargo.toml".to_owned(),
            "[package]\nname='fix'\nversion='0.0.0'\nedition='2024'\n[workspace]\n".to_owned(),
        ),
        (
            "src/lib.rs".to_owned(),
            "//! fix\npub mod width;\npub mod core;\n".to_owned(),
        ),
        ("src/width.rs".to_owned(), "pub struct Width;\n".to_owned()),
        (
            "src/core/mod.rs".to_owned(),
            "pub mod interpret;\n".to_owned(),
        ),
        (
            "src/core/interpret/mod.rs".to_owned(),
            "pub mod eval;\n".to_owned(),
        ),
        ("src/core/interpret/eval.rs".to_owned(), RENDER.to_owned()),
    ];
    let refs: Vec<(&str, &str)> = files
        .iter()
        .map(|(p, t)| (p.as_str(), t.as_str()))
        .collect();
    let fixture = Fixture::new(&refs);
    let snapshot = snapshot_workspace(fixture.path()).expect("snapshot");
    let policy = Policy::parse(&POLICY.replace("ALLOWED", "").replace(
        "home='src/width.rs::Width'",
        "home='src/core/mod.rs::interpret'",
    ))
    .expect("policy");
    let mut inputs = Inputs::new(
        &snapshot,
        Basis {
            baseline: "b".into(),
            candidate: "a".into(),
        },
        "d".into(),
    );
    inputs.policy.observed = observed(&snapshot);
    inputs.before_policy.observed = observed(&snapshot);
    let report = delta::check(&snapshot, &snapshot, &policy, &policy, &inputs).expect("delta");
    let summary = &report.spellings["narrowing"];
    assert_eq!(
        (summary.debt_before, summary.debt_after),
        (0, 0),
        "{:?}",
        report.violations
    );
}
