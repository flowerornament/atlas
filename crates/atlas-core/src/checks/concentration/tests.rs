//! Native debt-law assertions; project citation grammar remains native.
use super::super::FileMap;
use super::super::policy::AliasKind;
use super::measure::body_lines;
use super::*;
use serde_json::{Value, json};

fn captured(packages: &[(&str, &[(&str, &str)])]) -> Workspace {
    let root = tempfile::tempdir().expect("source fixture");
    let members = packages
        .iter()
        .map(|(name, _)| format!("'crates/{name}'"))
        .collect::<Vec<_>>()
        .join(",");
    std::fs::write(
        root.path().join("Cargo.toml"),
        format!("[workspace]\nmembers=[{members}]\nresolver='3'\n"),
    )
    .expect("workspace fixture");
    for (name, files) in packages {
        let directory = root.path().join(format!("crates/{name}"));
        std::fs::create_dir_all(directory.join("src")).expect("source directory");
        std::fs::write(
            directory.join("Cargo.toml"),
            format!("[package]\nname='{name}'\nversion='0.0.0'\nedition='2024'\n"),
        )
        .expect("package fixture");
        for (file, text) in *files {
            let path = directory.join("src").join(file);
            std::fs::create_dir_all(path.parent().expect("file parent")).expect("file directory");
            std::fs::write(path, text).expect("fixture source");
        }
    }
    Workspace::of(&crate::discovery::snapshot_workspace(root.path()).expect("captured fixture"))
        .expect("workspace view")
}
fn workspace(files: &[(&str, &str)]) -> Workspace {
    captured(&[("fix", files)])
}
fn headers(workspace: &Workspace) -> BTreeMap<String, HeaderCitation> {
    workspace
        .crates
        .iter()
        .flat_map(|package| &package.files)
        .map(|file| {
            (
                file.rel.clone(),
                HeaderCitation {
                    content_fingerprint: crate::discovery::fingerprint(
                        file.source.text().as_bytes(),
                    ),
                    cited: file
                        .source
                        .projection()
                        .module_header_lines()
                        .join("\n")
                        .contains(".design/"),
                },
            )
        })
        .collect()
}
fn measured(workspace: &Workspace, policy: &ConcentrationPolicy) -> Measure {
    Measure::of(workspace, policy, &headers(workspace), &["test-strategies"])
}
struct TestVerdict {
    violations: Vec<String>,
    report: Value,
}
fn landing(
    policy: &ConcentrationPolicy,
    published: Option<&ConcentrationPolicy>,
    before: &Workspace,
    after: &Workspace,
    description: &str,
) -> TestVerdict {
    // Explicit interpretations of the two citation-bearing native fixture inputs.
    let beads = match description {
        "(murail-x)" => BTreeSet::from(["murail-x".into()]),
        "concentration: loosen (murail-ruling)" => BTreeSet::from(["murail-ruling".into()]),
        "" | "no citation" => BTreeSet::new(),
        other => panic!("uninterpreted fixture description: {other}"),
    };
    let citations = Citations {
        description_fingerprint: crate::discovery::fingerprint(description.as_bytes()),
        beads,
    };
    let reported = measured(before, published.unwrap_or(policy));
    let base = measured(before, policy);
    let new = measured(after, policy);
    assert!(reported.unavailable.is_empty());
    assert!(base.unavailable.is_empty());
    assert!(new.unavailable.is_empty());
    let result = ratchets::landing(
        policy,
        published,
        &reported,
        &base,
        &new,
        description,
        Some(&citations),
    );
    assert!(result.unavailable.is_empty());
    TestVerdict {
        violations: result.violations,
        report: serde_json::to_value(result.report).expect("typed result JSON"),
    }
}
fn policy(section: &str) -> ConcentrationPolicy {
    toml::from_str(&format!(
            "population = ['fix']\nentry_lines = 3\nfile_lines = 5\nconventional_names = ['error']\n{section}"
        ))
        .expect("fixture section")
}

#[test]
fn inline_module_relay_has_the_same_identity_after_extraction() {
    let lib = "pub use evaluation::table_ids;\nmod evaluation;\n";
    let inline = workspace(&[
        ("lib.rs", lib),
        (
            "evaluation.rs",
            "pub mod table_ids {\n    pub const ID: u32 = 1;\n}\n",
        ),
    ]);
    let extracted = workspace(&[
        ("lib.rs", lib),
        ("evaluation.rs", "pub mod table_ids;\n"),
        ("evaluation/table_ids.rs", "pub const ID: u32 = 1;\n"),
    ]);
    let policy = policy("");
    let before = measured(&inline, &policy);
    let after = measured(&extracted, &policy);
    assert_eq!(before.aliases, after.aliases);
    assert!(before.aliases.iter().any(|alias| {
        alias.kind == AliasKind::ModuleRelay && alias.source == "evaluation::table_ids"
    }));
    // Logical identity adds no physical file or body measurement.
    assert_eq!(before.file.len(), 2);
    assert_eq!(before.file["crates/fix/src/evaluation.rs"], 3);
    assert!(!before.modules.contains("fix::evaluation::table_ids"));
    for tree in [&inline, &extracted] {
        assert!(
            land(&policy, Some(&policy), tree, tree)
                .violations
                .iter()
                .any(|row| { row.contains("re-homes module `evaluation::table_ids`") })
        );
    }
}

#[test]
fn inline_module_relay_discovery_excludes_tests_and_local_scopes() {
    let tree = workspace(&[
        (
            "lib.rs",
            "mod evaluation;\npub use evaluation::{outer, tests, local};\npub use evaluation::outer::inner;\n",
        ),
        (
            "evaluation.rs",
            "pub mod outer {\n    pub mod inner {}\n}\n#[cfg(test)]\npub mod tests {\n    pub mod hidden {}\n}\nfn scope() {\n    pub mod local {}\n}\n",
        ),
    ]);
    let measured = measured(&tree, &policy(""));
    let relays: BTreeSet<_> = measured
        .aliases
        .iter()
        .filter(|alias| alias.kind == AliasKind::ModuleRelay)
        .map(|alias| alias.source.as_str())
        .collect();
    assert_eq!(
        relays,
        BTreeSet::from(["evaluation::outer", "evaluation::outer::inner"])
    );
    let base = workspace(&[
        ("lib.rs", "mod evaluation;\n"),
        (
            "evaluation.rs",
            "pub mod outer {\n    pub mod inner {}\n}\n",
        ),
    ]);
    assert!(
        land(&policy(""), Some(&policy("")), &base, &tree)
            .violations
            .iter()
            .any(|row| row.contains("re-homes module `evaluation::outer::inner`"))
    );
}

#[test]
fn inline_module_relay_discovery_uses_the_shared_test_and_lexical_boundary() {
    let tree = workspace(&[
        (
            "lib.rs",
            "pub mod live {\n    pub mod inner {}\n}\n#[cfg(test)]\nmod fixtures;\npub use fixtures::hidden;\npub use live::inner;\npub use fake::inner;\n",
        ),
        ("fixtures.rs", "pub mod hidden {}\n"),
        (
            "text.rs",
            "const TEXT: &str = r#\"\npub mod fake {\n    pub mod inner {}\n}\n\"#;\n",
        ),
    ]);
    let measured = measured(&tree, &policy(""));
    let relays: BTreeSet<_> = measured
        .aliases
        .iter()
        .filter(|alias| alias.kind == AliasKind::ModuleRelay)
        .map(|alias| alias.source.as_str())
        .collect();
    assert_eq!(relays, BTreeSet::from(["live::inner"]));
    assert!(!measured.file.contains_key("crates/fix/src/fixtures.rs"));
}

#[test]
fn inline_module_relay_discovery_excludes_discarded_macro_tokens() {
    let tree = workspace(&[(
        "lib.rs",
        "macro_rules! discard {\n    ($($tt:tt)*) => {};\n}\npub mod outer {\n    discard! {\n        pub mod inner {}\n    }\n    #[allow(non_camel_case_types)]\n    pub struct inner;\n}\npub use outer::inner;\n",
    )]);
    let measured = measured(&tree, &policy(""));
    assert!(measured.aliases.is_empty(), "{:?}", measured.aliases);
}

fn lines(count: usize) -> String {
    "pub fn f() {}\n".repeat(count)
}

/// The landing law with a description that cites nothing.
fn land(
    policy: &ConcentrationPolicy,
    published: Option<&ConcentrationPolicy>,
    before: &Workspace,
    after: &Workspace,
) -> TestVerdict {
    landing(policy, published, before, after, "")
}

const CITED: &str = "//! The fixture (`.design/fixture.md`).\n";

fn refusals(
    policy: &ConcentrationPolicy,
    published: Option<&ConcentrationPolicy>,
    tree: &Workspace,
) -> Vec<String> {
    land(policy, published, tree, tree).violations
}

#[test]
fn an_entry_file_over_the_limit_needs_an_exact_row() {
    // The `mod a;` declaration is not counted: four lines, not five.
    let lib = format!("{CITED}mod a;\n{}", lines(4));
    let tree = workspace(&[("lib.rs", &lib), ("a.rs", "")]);
    assert_eq!(
        measured(&tree, &policy("")).entry["crates/fix/src/lib.rs"],
        4
    );
    let missing = refusals(&policy(""), None, &tree);
    assert_eq!(missing.len(), 1, "{missing:?}");
    assert!(missing[0].starts_with("new entry file `crates/fix/src/lib.rs` is 4 body lines"));
    let exact = policy("entry = [{ path = 'crates/fix/src/lib.rs', lines = 4 }]");
    assert!(refusals(&exact, None, &tree).is_empty());
    // A row is a ceiling: a shrink passes, with the ratchet as a hint.
    let slack = policy("entry = [{ path = 'crates/fix/src/lib.rs', lines = 5 }]");
    let shrunk = land(&slack, Some(&slack), &tree, &tree);
    assert!(shrunk.violations.is_empty(), "{:?}", shrunk.violations);
    assert_eq!(
        shrunk.report["hints"],
        json!(["entry row `crates/fix/src/lib.rs` can ratchet to 4 (row 5)"])
    );
    let grown = workspace(&[("lib.rs", &format!("{lib}pub fn g() {{}}\n")), ("a.rs", "")]);
    assert!(refusals(&exact, None, &grown)[0].contains("above its row 4"));
    // A single-file crate's root orients no folder: only the file rule binds.
    let single = workspace(&[("lib.rs", &format!("{CITED}{}", lines(4)))]);
    assert!(refusals(&policy(""), None, &single).is_empty());
}

#[test]
fn deleting_a_listed_file_passes_with_a_hint() {
    let published = policy("file = [{ path = 'crates/fix/src/a.rs', lines = 6 }]");
    let tree = workspace(&[("lib.rs", CITED)]);
    let verdict = land(&published, Some(&published), &tree, &tree);
    assert!(verdict.violations.is_empty(), "{:?}", verdict.violations);
    assert_eq!(
        verdict.report["hints"],
        json!(["file row `crates/fix/src/a.rs` can be deleted: no population file has that path"])
    );
}

#[test]
fn a_child_module_declaration_never_taxes_its_entry_file() {
    let lib = format!("{CITED}mod a;\n{}", lines(4));
    let published: ConcentrationPolicy = toml::from_str(
            "population = ['fix']\nentry_lines = 3\nfile_lines = 100\nconventional_names = []\nentry = [{ path = 'crates/fix/src/lib.rs', lines = 4 }]",
        )
        .expect("fixture section");
    let before = workspace(&[("lib.rs", &lib), ("a.rs", "")]);
    let after = workspace(&[
        (
            "lib.rs",
            &format!("{lib}/// The new child.\n#[cfg(feature = \"x\")]\npub(crate) mod b;\n"),
        ),
        ("a.rs", ""),
        ("b.rs", ""),
    ]);
    let verdict = land(&published, Some(&published), &before, &after);
    assert!(verdict.violations.is_empty(), "{:?}", verdict.violations);
}

#[test]
fn a_repeated_module_name_needs_a_ruling_and_a_collision_gains_no_home() {
    let tree = workspace(&[
        ("lib.rs", &format!("{CITED}mod a;\nmod b;\nmod plan;\n")),
        ("a.rs", &format!("{CITED}mod plan;\nmod error;\n")),
        ("a/plan.rs", ""),
        ("a/error.rs", ""),
        ("b.rs", &format!("{CITED}mod error;\n")),
        ("b/error.rs", ""),
        ("plan.rs", ""),
    ]);
    let unruled = refusals(&policy(""), None, &tree);
    assert_eq!(
        unruled.len(),
        1,
        "conventional names are exempt: {unruled:?}"
    );
    assert!(unruled[0].starts_with("module name `plan` has 2 homes"));
    let ruled = policy(
        "name = [{ name = 'plan', ruling = 'collision', why = 'two plans', homes = ['fix::a::plan', 'fix::plan'] }]",
    );
    assert!(refusals(&ruled, Some(&ruled), &tree).is_empty());
    let published = policy(
        "name = [{ name = 'plan', ruling = 'collision', why = 'two plans', homes = ['fix::plan', 'fix::b::plan'] }]",
    );
    assert!(
        refusals(&ruled, Some(&published), &tree)
            .iter()
            .any(|row| row.contains("gained fix::a::plan"))
    );
}

/// The section at the committed limits, for the critic's cases at their
/// real sizes.
fn committed_limits(section: &str) -> ConcentrationPolicy {
    toml::from_str(&format!(
            "population = ['fix']\nentry_lines = 300\nfile_lines = 2000\nconventional_names = []\n{section}"
        ))
        .expect("fixture section")
}

// Critic case 2: halving migrate/migration.rs into
// migration/correspondence.rs makes the leaf an entry; it may carry a row
// at no more than its base size.
#[test]
fn a_leaf_that_gains_its_first_child_carries_its_base_size() {
    let file = |count: usize| format!("{CITED}{}", lines(count));
    let lib = format!("{CITED}mod migrate;\n");
    let migrate = format!("{CITED}mod migration;\n");
    let base = workspace(&[
        ("lib.rs", &lib),
        ("migrate.rs", &migrate),
        ("migrate/migration.rs", &file(800)),
    ]);
    let halved = workspace(&[
        ("lib.rs", &lib),
        ("migrate.rs", &migrate),
        (
            "migrate/migration.rs",
            &format!("{CITED}mod correspondence;\n{}", lines(400)),
        ),
        ("migrate/migration/correspondence.rs", &file(400)),
    ]);
    let published = committed_limits("");
    let unrowed = land(&published, Some(&published), &base, &halved).violations;
    assert_eq!(unrowed.len(), 1, "{unrowed:?}");
    assert!(
        unrowed[0].contains("became an entry file (it gained its first child module) at 400")
            && unrowed[0]
                .contains("bring it to at most 300, or carry it at no more than its base size 800")
            && !unrowed[0].contains("moved file"),
        "{unrowed:?}"
    );
    let carried =
        committed_limits("entry = [{ path = 'crates/fix/src/migrate/migration.rs', lines = 400 }]");
    let verdict = land(&carried, Some(&published), &base, &halved);
    assert!(verdict.violations.is_empty(), "{:?}", verdict.violations);
    let above_base =
        committed_limits("entry = [{ path = 'crates/fix/src/migrate/migration.rs', lines = 801 }]");
    assert!(
        land(&above_base, Some(&published), &base, &halved)
            .violations
            .iter()
            .any(|row| row.contains("may hold at most its base size 800"))
    );
}

#[test]
fn a_new_collision_row_and_a_reruled_collision_gaining_a_home_refuse() {
    let tree = workspace(&[
        ("lib.rs", &format!("{CITED}mod a;\nmod plan;\n")),
        ("a.rs", &format!("{CITED}mod plan;\n")),
        ("a/plan.rs", ""),
        ("plan.rs", ""),
    ]);
    let homes = "homes = ['fix::a::plan', 'fix::plan']";
    let collision = policy(&format!(
        "name = [{{ name = 'plan', ruling = 'collision', why = 'two plans', {homes} }}]"
    ));
    let facet = policy(&format!(
        "name = [{{ name = 'plan', ruling = 'facet', why = 'one plan', {homes} }}]"
    ));
    let empty = policy("");
    assert!(
        refusals(&collision, Some(&empty), &tree)
            .iter()
            .any(|row| row.contains("new collision row")),
    );
    assert!(refusals(&facet, Some(&empty), &tree).is_empty());
    let published = policy(
        "name = [{ name = 'plan', ruling = 'collision', why = 'two plans', homes = ['fix::plan', 'fix::b::plan'] }]",
    );
    assert!(
        refusals(&facet, Some(&published), &tree)
            .iter()
            .any(|row| row.contains("re-ruling it a facet does not admit a new home"))
    );
    // A collision that becomes a facet names the bead that re-ruled it.
    let same_homes = policy(&format!(
        "name = [{{ name = 'plan', ruling = 'collision', why = 'two plans', {homes} }}]"
    ));
    assert!(
        refusals(&facet, Some(&same_homes), &tree)
            .iter()
            .any(|row| row.contains("without naming who re-ruled it"))
    );
    let reruled = policy(&format!(
        "name = [{{ name = 'plan', ruling = 'facet', why = 'one plan', reruled_by = 'murail-x', {homes} }}]"
    ));
    assert!(
        refusals(&reruled, Some(&same_homes), &tree)
            .iter()
            .any(|row| row.contains("does not cite"))
    );
    let verdict = landing(&reruled, Some(&same_homes), &tree, &tree, "(murail-x)");
    assert!(verdict.violations.is_empty(), "{:?}", verdict.violations);
    // The unruled-name fix offers a facet ruling, never a collision row.
    let unruled = refusals(&empty, None, &tree);
    assert!(unruled[0].contains("ruling it `facet`") && !unruled[0].contains("collision"));
}

#[test]
fn a_new_alias_and_a_new_alias_row_refuse_at_landing() {
    let tree = workspace(&[
        ("lib.rs", &format!("{CITED}mod a;\npub use a::inner;\n")),
        ("a.rs", &format!("{CITED}pub mod inner;\n")),
        ("a/inner.rs", ""),
    ]);
    let empty = policy("");
    let unlisted = refusals(&empty, Some(&empty), &tree);
    assert_eq!(unlisted.len(), 1, "{unlisted:?}");
    assert!(unlisted[0].starts_with("new `fix` re-homes module `a::inner`"));
    let listed = policy(
        "alias = [{ module = 'fix', kind = 'module_relay', binding = 'inner', source = 'a::inner' }]",
    );
    assert!(refusals(&listed, None, &tree).is_empty());
    assert!(refusals(&listed, Some(&listed), &tree).is_empty());
    let added = refusals(&listed, Some(&empty), &tree);
    assert_eq!(added.len(), 1, "{added:?}");
    assert!(added[0].contains("alias rows are listed once and only go away"));
}

#[test]
fn body_lines_leave_out_navigation_only() {
    let text = "//! Header (`.design/x.md`).\nuse std::fmt;\n#[cfg(feature = \"x\")]\nuse std::{\n    io,\n    mem,\n};\n/// Re-exported.\npub use crate::a::A;\n#[cfg(test)]\nmod tests;\npub mod child;\nmod inline {\n    pub fn f() {}\n}\npub fn g() {}\n";
    let file = FileMap::of("crates/fix/src/lib.rs".into(), text.into());
    assert_eq!(file.token_lines(), 14, "[size] counts every line");
    // The inline module and its body, and the fn, are body.
    assert_eq!(body_lines(&file), 4);
}

fn section(head: &str, rows: &str) -> ConcentrationPolicy {
    toml::from_str(&format!("{head}\n{rows}")).expect("fixture section")
}

const BASE_KNOBS: &str =
    "population = ['fix', 'gone']\nentry_lines = 3\nfile_lines = 5\nconventional_names = ['error']";

// Critic: the section may not loosen itself, in any of its three forms,
// unless the landing names a new, cited ruling that covers the knob.
#[test]
fn the_section_cannot_loosen_itself_without_a_named_ruling() {
    let tree = workspace(&[("lib.rs", CITED)]);
    let published = section(BASE_KNOBS, "");
    let cases = [
        (
            "population = ['fix', 'gone']\nentry_lines = 4\nfile_lines = 6\nconventional_names = ['error']",
            vec![
                "entry_lines rose from 3 to 4",
                "file_lines rose from 5 to 6",
            ],
            "['entry_lines', 'file_lines']",
        ),
        (
            "population = ['fix']\nentry_lines = 3\nfile_lines = 5\nconventional_names = ['error']",
            vec!["`gone` left the population"],
            "['population']",
        ),
        (
            "population = ['fix', 'gone']\nentry_lines = 3\nfile_lines = 5\nconventional_names = ['error', 'plan']",
            vec!["`plan` became a conventional name"],
            "['conventional_names']",
        ),
    ];
    let cited = "concentration: loosen (murail-ruling)";
    for (knobs, expected, covers) in cases {
        let refused = refusals(&section(knobs, ""), Some(&published), &tree);
        assert_eq!(refused.len(), expected.len(), "{refused:?}");
        for (row, change) in refused.iter().zip(&expected) {
            assert!(row.contains(change) && row.contains("amended_by"), "{row}");
        }
        let ruled = section(
            knobs,
            &format!("amended_by = {{ bead = 'murail-ruling', covers = {covers} }}"),
        );
        let verdict = landing(&ruled, Some(&published), &tree, &tree, cited);
        assert!(verdict.violations.is_empty(), "{:?}", verdict.violations);
        // Uncited: the bead-citations gate could not check the bead.
        let uncited = landing(&ruled, Some(&published), &tree, &tree, "no citation").violations;
        assert_eq!(uncited.len(), 1, "{uncited:?}");
        assert!(uncited[0].contains("does not cite"));
        // Uncovered: a ruling for another knob waives nothing here, and
        // names a knob this landing does not loosen.
        let other = if covers.contains("population") {
            "['file_lines']"
        } else {
            "['population']"
        };
        let uncovered = section(
            knobs,
            &format!("amended_by = {{ bead = 'murail-ruling', covers = {other} }}"),
        );
        assert_eq!(
            landing(&uncovered, Some(&published), &tree, &tree, cited)
                .violations
                .len(),
            expected.len() + 1
        );
        // A ruling already published rules nothing new.
        let stale = section(
            BASE_KNOBS,
            &format!("amended_by = {{ bead = 'murail-ruling', covers = {covers} }}"),
        );
        assert_eq!(
            landing(&ruled, Some(&stale), &tree, &tree, cited)
                .violations
                .len(),
            expected.len()
        );
    }
    // A ruling may cover only what this landing loosens.
    let loosened = section(
        "population = ['fix', 'gone']\nentry_lines = 4\nfile_lines = 5\nconventional_names = ['error']",
        "amended_by = { bead = 'murail-ruling', covers = ['entry_lines', 'population'] }",
    );
    let extra = landing(&loosened, Some(&published), &tree, &tree, cited).violations;
    assert_eq!(extra.len(), 1, "{extra:?}");
    assert!(extra[0].contains("covers `population`, which this landing does not loosen"));
    // Malformed bead syntax remains the native wrapper's parser assertion.
    // Tightening needs no ruling.
    let tighter = section(
        "population = ['fix', 'gone', 'more']\nentry_lines = 2\nfile_lines = 4\nconventional_names = []",
        "",
    );
    assert!(refusals(&tighter, Some(&published), &tree).is_empty());
}

/// Two crates: `fix`, and `extra`, which a landing may bring into the
/// population.
fn two_crates(fix: &[(&str, &str)], extra: &[(&str, &str)]) -> Workspace {
    captured(&[("extra", extra), ("fix", fix)])
}

const JOINED: &str = "population = ['fix', 'extra']\nentry_lines = 3\nfile_lines = 5\nconventional_names = ['error']";

// Critic: a crate joining the population records its debt as it
// measured; an inflated row is refused, a true one admitted.
#[test]
fn a_joining_crate_records_its_true_measure() {
    let fix = [("lib.rs", CITED)];
    let extra = [
        ("lib.rs", &*format!("{CITED}mod big;\n")),
        ("big.rs", &*lines(8)),
    ];
    let tree = two_crates(&fix, &extra);
    let published = policy("");
    let row = |count: usize| {
        section(
            JOINED,
            &format!("file = [{{ path = 'crates/extra/src/big.rs', lines = {count} }}]"),
        )
    };
    assert!(
        land(&row(8), Some(&published), &tree, &tree)
            .violations
            .is_empty()
    );
    let inflated = land(&row(12), Some(&published), &tree, &tree).violations;
    assert!(
        inflated.iter().any(|refusal| refusal
            .contains("records 12 lines for a crate joining the population, above the 8")),
        "{inflated:?}"
    );
    // A file the base lacks is new, joining crate or not.
    let base = two_crates(&fix, &[("lib.rs", CITED)]);
    let fresh = two_crates(
        &fix,
        &[
            ("lib.rs", &*format!("{CITED}mod big;\n")),
            ("big.rs", &*lines(8)),
        ],
    );
    assert!(
        land(&row(8), Some(&published), &base, &fresh)
            .violations
            .iter()
            .any(|refusal| refusal.contains("names a path the published policy does not hold"))
    );
    // Its header count is recorded true as well.
    let uncited = two_crates(&fix, &[("lib.rs", "mod big;\n"), ("big.rs", "")]);
    let header = |count: usize| section(JOINED, &format!("[header_uncited]\nextra = {count}"));
    assert!(
        land(&header(1), Some(&published), &uncited, &uncited)
            .violations
            .is_empty()
    );
    assert!(
        land(&header(3), Some(&published), &uncited, &uncited)
            .violations
            .iter()
            .any(|refusal| refusal.contains("records 3 for a crate joining the population"))
    );
}

// Critic: a shrunk file cannot grow back into its row's slack, and a
// deleted listed path recreated over the limit is new debt (base 0).
#[test]
fn a_listed_path_never_grows_past_its_base() {
    let published = policy("file = [{ path = 'crates/fix/src/a.rs', lines = 10 }]");
    let lib = format!("{CITED}mod a;\n");
    let tree = |count: usize| workspace(&[("lib.rs", &lib), ("a.rs", &lines(count))]);
    let shrunk = land(&published, Some(&published), &tree(10), &tree(7));
    assert!(shrunk.violations.is_empty(), "{:?}", shrunk.violations);
    let regrown = land(&published, Some(&published), &tree(7), &tree(9)).violations;
    assert!(
            regrown
                .iter()
                .any(|refusal| refusal
                    .contains("is 9 body lines, above the 7 it measured in the base")),
            "{regrown:?}"
        );
    let deleted = workspace(&[("lib.rs", CITED)]);
    let recreated = land(&published, Some(&published), &deleted, &tree(8)).violations;
    assert!(
        recreated
            .iter()
            .any(|refusal| refusal.contains("above the 0 it measured in the base")),
        "{recreated:?}"
    );
    // A newly uncited root refuses, whatever the count.
    let held = policy("[header_uncited]\nfix = 2");
    let cited = workspace(&[("lib.rs", &lib), ("a.rs", "")]);
    let uncited = workspace(&[("lib.rs", "mod a;\n"), ("a.rs", "")]);
    assert!(
        land(&held, Some(&held), &cited, &uncited)
            .violations
            .iter()
            .any(|refusal| refusal.contains("has new folder roots whose //! header cites no"))
    );
}

// Critic: a ghost crate in a collision row waives nothing; a real joining
// crate records the collision it brings.
#[test]
fn a_ghost_home_waives_no_collision() {
    let fix = [
        ("lib.rs", &*format!("{CITED}mod a;\nmod plan;\n")),
        ("a.rs", &*format!("{CITED}mod plan;\n")),
        ("a/plan.rs", ""),
        ("plan.rs", ""),
    ];
    let tree = two_crates(&fix, &[("lib.rs", CITED)]);
    let published = policy("");
    let ghost = policy(
        "name = [{ name = 'plan', ruling = 'collision', why = 'two plans', homes = ['fix::a::plan', 'fix::plan', 'ghost::plan'] }]",
    );
    assert!(
        refusals(&ghost, Some(&published), &tree)
            .iter()
            .any(|refusal| refusal.contains("new collision row"))
    );
    let one = [("lib.rs", &*format!("{CITED}mod plan;\n")), ("plan.rs", "")];
    let extra = [("lib.rs", &*format!("{CITED}mod plan;\n")), ("plan.rs", "")];
    let joined_tree = two_crates(&one, &extra);
    let joined = section(
        JOINED,
        "name = [{ name = 'plan', ruling = 'collision', why = 'two plans', homes = ['extra::plan', 'fix::plan'] }]",
    );
    let verdict = land(&joined, Some(&published), &joined_tree, &joined_tree);
    assert!(verdict.violations.is_empty(), "{:?}", verdict.violations);
    // The joining home must pre-exist: created this landing, it is new.
    let base = two_crates(&one, &[("lib.rs", CITED)]);
    assert!(
        land(&joined, Some(&published), &base, &joined_tree)
            .violations
            .iter()
            .any(|refusal| refusal.contains("new collision row"))
    );
}

// A row stays with its path: a renamed oversized file is a new file over
// the limit, and the refusal names the split; a row never rises.
#[test]
fn a_rename_of_an_oversized_file_refuses_with_the_split_hint() {
    let published = policy("file = [{ path = 'crates/fix/src/a.rs', lines = 6 }]");
    let base = workspace(&[("lib.rs", &format!("{CITED}mod a;\n")), ("a.rs", &lines(6))]);
    let renamed = workspace(&[("lib.rs", &format!("{CITED}mod b;\n")), ("b.rs", &lines(6))]);
    let verdict = land(&published, Some(&published), &base, &renamed);
    assert_eq!(verdict.violations.len(), 1, "{:?}", verdict.violations);
    assert!(
        verdict.violations[0].starts_with("new file `crates/fix/src/b.rs`")
            && verdict.violations[0].contains("split it into pieces of at most 5 body lines"),
        "{:?}",
        verdict.violations
    );
    // Writing a row for the new path does not admit it.
    let rowed = policy("file = [{ path = 'crates/fix/src/b.rs', lines = 6 }]");
    assert!(
        land(&rowed, Some(&published), &base, &renamed)
            .violations
            .iter()
            .any(|row| row.contains("split it into pieces of at most 5 body lines"))
    );
    let raised = policy("file = [{ path = 'crates/fix/src/a.rs', lines = 7 }]");
    let grown = workspace(&[("lib.rs", &format!("{CITED}mod a;\n")), ("a.rs", &lines(7))]);
    assert!(
        land(&raised, Some(&published), &base, &grown)
            .violations
            .iter()
            .any(|row| row.contains("rose from 6 to 7"))
    );
}

// A split into pieces within the limit passes; each piece carries its
// own header and imports, which are navigation, not body.
#[test]
fn a_split_into_pieces_within_the_limit_passes() {
    let imports = "use std::collections::BTreeMap;\nuse std::fmt::{self, Write};\nuse crate::shape::{\n    Shape,\n    Space,\n};\n";
    let module = |header: &str, body: usize| {
        format!("//! {header} (`.design/x.md`).\n{imports}{}", lines(body))
    };
    let published =
        committed_limits("file = [{ path = 'crates/fix/src/construction.rs', lines = 4189 }]");
    let base = workspace(&[
        ("lib.rs", &format!("{CITED}mod construction;\nmod shape;\n")),
        ("shape.rs", ""),
        ("construction.rs", &module("Construction", 4189)),
    ]);
    let split = workspace(&[
        (
            "lib.rs",
            &format!("{CITED}mod schedule;\nmod regions;\nmod shape;\n"),
        ),
        ("shape.rs", ""),
        ("schedule.rs", &module("The schedule", 2000)),
        ("regions.rs", &module("The regions", 2000)),
        ("residue.rs", &module("The residue", 189)),
    ]);
    let verdict = land(&published, Some(&published), &base, &split);
    assert!(verdict.violations.is_empty(), "{:?}", verdict.violations);
    assert_eq!(verdict.report["file"]["over_after"], 0);
    assert!(
        verdict.report["hints"][0]
            .as_str()
            .is_some_and(|hint| hint.contains("can be deleted"))
    );
}

// Critic probes: emptying a listed file into small pieces frees nothing
// for new code, and one file's entry and file rows are never spent on
// another path.
#[test]
fn a_published_row_funds_no_other_path() {
    let published = policy("file = [{ path = 'crates/fix/src/a.rs', lines = 10 }]");
    let lib = format!("{CITED}mod a;\nmod b;\nmod c;\nmod d;\n");
    let base = workspace(&[("lib.rs", &lib), ("a.rs", &lines(10))]);
    let pieces = workspace(&[
        ("lib.rs", &lib),
        ("a.rs", ""),
        ("b.rs", &lines(4)),
        ("c.rs", &lines(4)),
        ("d.rs", &lines(10)),
    ]);
    let rows = policy(
        "file = [{ path = 'crates/fix/src/a.rs', lines = 10 }, { path = 'crates/fix/src/d.rs', lines = 10 }]",
    );
    assert!(
        land(&rows, Some(&published), &base, &pieces)
            .violations
            .iter()
            .any(|row| row.contains("new concentration file row `crates/fix/src/d.rs`"))
    );
    let both = committed_limits(
        "entry = [{ path = 'crates/fix/src/x.rs', lines = 400 }]\nfile = [{ path = 'crates/fix/src/x.rs', lines = 2400 }]",
    );
    let base = workspace(&[
        ("lib.rs", &format!("{CITED}mod x;\n")),
        ("x.rs", &format!("{CITED}mod part;\n{}", lines(2400))),
        ("x/part.rs", ""),
    ]);
    let moved = workspace(&[
        ("lib.rs", &format!("{CITED}mod y;\n")),
        ("y.rs", &format!("{CITED}mod part;\n{}", lines(2400))),
        ("y/part.rs", ""),
    ]);
    let spent = committed_limits(
        "entry = [{ path = 'crates/fix/src/y.rs', lines = 400 }]\nfile = [{ path = 'crates/fix/src/y.rs', lines = 2400 }]",
    );
    let refused = land(&spent, Some(&both), &base, &moved).violations;
    assert!(
        refused
            .iter()
            .any(|row| row.contains("new concentration entry row `crates/fix/src/y.rs`"))
            && refused
                .iter()
                .any(|row| row.contains("new concentration file row `crates/fix/src/y.rs`")),
        "{refused:?}"
    );
}

// A collision cannot move: a moved home refuses; a layout change keeps
// the module id; renaming one home ends the collision, and its row goes.
#[test]
fn a_collision_home_moves_only_by_keeping_its_module_id() {
    let published = policy(
        "name = [{ name = 'plan', ruling = 'collision', why = 'two plans', homes = ['fix::a::plan', 'fix::plan'] }]",
    );
    let lib = format!("{CITED}mod a;\nmod b;\nmod plan;\n");
    let base = workspace(&[
        ("lib.rs", &lib),
        ("a.rs", &format!("{CITED}mod plan;\n")),
        ("a/plan.rs", ""),
        ("b.rs", CITED),
        ("plan.rs", ""),
    ]);
    let moved = workspace(&[
        ("lib.rs", &lib),
        ("a.rs", CITED),
        ("b.rs", &format!("{CITED}mod plan;\n")),
        ("b/plan.rs", ""),
        ("plan.rs", ""),
    ]);
    let rowed = policy(
        "name = [{ name = 'plan', ruling = 'collision', why = 'two plans', homes = ['fix::b::plan', 'fix::plan'] }]",
    );
    assert!(
        land(&rowed, Some(&published), &base, &moved)
            .violations
            .iter()
            .any(|row| row.contains("gained fix::b::plan"))
    );
    let relaid = workspace(&[
        ("lib.rs", &lib),
        ("a.rs", &format!("{CITED}mod plan;\n")),
        ("a/plan.rs", ""),
        ("b.rs", CITED),
        ("plan/mod.rs", &format!("{CITED}mod part;\n")),
        ("plan/part.rs", ""),
    ]);
    let verdict = land(&published, Some(&published), &base, &relaid);
    assert!(verdict.violations.is_empty(), "{:?}", verdict.violations);
    let renamed = workspace(&[
        ("lib.rs", &lib),
        ("a.rs", &format!("{CITED}mod schedule;\n")),
        ("a/schedule.rs", ""),
        ("b.rs", CITED),
        ("plan.rs", ""),
    ]);
    let verdict = land(&policy(""), Some(&published), &base, &renamed);
    assert!(verdict.violations.is_empty(), "{:?}", verdict.violations);
}

#[test]
fn a_moved_alias_is_a_new_alias() {
    let rename = "pub use std::fmt::Write as FmtWrite;\n";
    let published = policy(
        "alias = [{ module = 'fix::a', kind = 'rename', binding = 'FmtWrite', source = 'std::fmt::Write' }]",
    );
    let base = workspace(&[("lib.rs", &format!("{CITED}mod a;\n")), ("a.rs", rename)]);
    let moved = workspace(&[("lib.rs", &format!("{CITED}mod b;\n")), ("b.rs", rename)]);
    let rowed = policy(
        "alias = [{ module = 'fix::b', kind = 'rename', binding = 'FmtWrite', source = 'std::fmt::Write' }]",
    );
    assert!(
        land(&rowed, Some(&published), &base, &moved)
            .violations
            .iter()
            .any(|row| row.contains("listed once and only go away"))
    );
    assert!(
        land(&published, Some(&published), &base, &moved)
            .violations
            .iter()
            .any(|row| row.starts_with("new `fix::b`"))
    );
}

// Only the land gate's citation grammar counts: prose, `Seat:` and
// `Ruled-by:` mentions cite nothing.
// Native a_ruling_is_cited_only_by_the_land_grammar stays with its project parser.

// Critic: an entry row holds its path whatever its kind. Losing the
// child and growing refuses with the row kept and with it deleted; losing
// the child at the same size keeps the row and passes.
#[test]
fn an_entry_row_holds_its_path_whatever_its_kind() {
    let published = committed_limits("entry = [{ path = 'crates/fix/src/x.rs', lines = 400 }]");
    let lib = format!("{CITED}mod x;\n");
    let base = workspace(&[
        ("lib.rs", &lib),
        ("x.rs", &format!("{CITED}mod part;\n{}", lines(400))),
        ("x/part.rs", ""),
    ]);
    let leaf = |count: usize| {
        workspace(&[
            ("lib.rs", &lib),
            ("x.rs", &format!("{CITED}{}", lines(count))),
        ])
    };
    let kept = land(&published, Some(&published), &base, &leaf(2000)).violations;
    assert!(
        kept.iter()
            .any(|row| row.contains("`crates/fix/src/x.rs` is 2000 body lines, above its row 400")),
        "{kept:?}"
    );
    let dropped = land(&committed_limits(""), Some(&published), &base, &leaf(2000)).violations;
    assert!(
        dropped
            .iter()
            .any(|row| row.contains("its entry row stays while the path does")),
        "{dropped:?}"
    );
    // As a leaf it is still capped by its base size.
    let small = workspace(&[
        ("lib.rs", &lib),
        (
            "x.rs",
            &format!(
                "{CITED}mod part;
{}",
                lines(350)
            ),
        ),
        ("x/part.rs", ""),
    ]);
    assert!(
        land(&published, Some(&published), &small, &leaf(380))
            .violations
            .iter()
            .any(|row| row.contains("is 380 body lines, above the 350 it measured in the base"))
    );
    let same = land(&published, Some(&published), &base, &leaf(400));
    assert!(same.violations.is_empty(), "{:?}", same.violations);
    assert!(same.report["hints"].as_array().is_some_and(Vec::is_empty));
}

// Critic: citing an old root does not pay for a new uncited one.
#[test]
fn a_new_folder_root_must_cite() {
    let held = policy("[header_uncited]\nfix = 1");
    let base = workspace(&[("lib.rs", "//! Fixture.\nmod a;\n"), ("a.rs", "")]);
    let swapped = workspace(&[
        ("lib.rs", &format!("{CITED}mod a;\nmod b;\n")),
        ("a.rs", ""),
        ("b.rs", "//! A new folder.\nmod c;\n"),
        ("b/c.rs", ""),
    ]);
    let refused = land(&held, Some(&held), &base, &swapped).violations;
    assert_eq!(refused.len(), 1, "{refused:?}");
    assert!(refused[0].contains("has new folder roots whose //! header cites no formal-model section or .design spec (crates/fix/src/b.rs)"));
}

// A production `#[path]` hides a module from the scan; test-only and
// test-support uses stay allowed.
#[test]
fn a_production_path_attribute_refuses() {
    let tree = |lib: &str| workspace(&[("lib.rs", lib), ("elsewhere.rs", "")]);
    let remapped = tree(&format!("{CITED}#[path = \"elsewhere.rs\"]\nmod hidden;\n"));
    let refused = refusals(&policy(""), None, &remapped);
    assert!(
        refused.iter().any(|row| row
            .starts_with("crates/fix/src/lib.rs:3 declares a production module with `#[path]`")),
        "{refused:?}"
    );
    for gate in [
        "#[cfg(test)]",
        "#[cfg(any(test, feature = \"test-strategies\"))]",
    ] {
        let gated = tree(&format!(
            "{CITED}{gate}\n#[path = \"elsewhere.rs\"]\nmod support;\n"
        ));
        assert!(
            !refusals(&policy(""), None, &gated)
                .iter()
                .any(|row| row.contains("#[path]")),
            "{gate}"
        );
    }
}

#[test]
fn root_relays_shadows_internal_uses_and_renames_are_aliases() {
    let lib = "mod a;\nmod parse;\npub use a::inner;\npub use a::inner::parse;\npub(crate) use a::Thing;\npub use a::Thing as Other;\nmacro_rules! m { () => {} }\npub(crate) use m;\npub use a::Same;\n";
    let tree = workspace(&[
        ("lib.rs", lib),
        (
            "a.rs",
            "pub mod inner;\npub(crate) use super::parse::X as Y;\n",
        ),
        ("a/inner.rs", ""),
        ("parse.rs", ""),
    ]);
    let kinds: Vec<(String, AliasKind)> = measured(&tree, &policy(""))
        .aliases
        .into_iter()
        .map(|alias| (alias.binding, alias.kind))
        .collect();
    assert_eq!(
        kinds,
        vec![
            ("inner".to_owned(), AliasKind::ModuleRelay),
            ("parse".to_owned(), AliasKind::RootShadow),
            ("Thing".to_owned(), AliasKind::RootRestricted),
            ("Other".to_owned(), AliasKind::Rename),
            ("Y".to_owned(), AliasKind::Rename),
        ]
    );
}

#[test]
fn an_uncited_folder_root_counts_and_the_count_only_goes_down() {
    let tree = workspace(&[("lib.rs", "//! Fixture.\nmod a;\n"), ("a.rs", "")]);
    let refused = refusals(&policy(""), None, &tree);
    assert!(
        refused[0].contains("1 folder roots whose //! header cites no"),
        "{refused:?}"
    );
    let held = policy("[header_uncited]\nfix = 1");
    assert!(refusals(&held, None, &tree).is_empty());
    let published = policy("[header_uncited]\nfix = 0");
    let cited = workspace(&[("lib.rs", &format!("{CITED}mod a;\n")), ("a.rs", "")]);
    assert!(refusals(&published, None, &cited).is_empty());
    assert!(
        refusals(&held, Some(&published), &tree)
            .iter()
            .any(|row| row.contains("rose from 0 to 1"))
    );
}
