//! Each seam ratchet goes red on its own knockout, and a boundary read stays
//! green.

use std::collections::BTreeMap;

use super::super::super::{CrateMap, module_path_of};
use super::*;
use crate::checks::workspace::Workspace;

const STAGES: [&str; 4] = ["vocabulary", "front", "middle", "back"];

/// The fixture crate. An entry under `../` is a file outside `src`, found
/// only if a `#[path]` declaration mounts it.
fn snapshot(files: &[(&str, String)]) -> crate::discovery::Snapshot {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let root = std::env::temp_dir().join(format!(
        "atlas-seams-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&root).unwrap();
    std::fs::create_dir_all(root.join("crates/fix/src")).unwrap();
    std::fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nmembers=['crates/fix']\nresolver='3'\n",
    )
    .unwrap();
    std::fs::write(
        root.join("crates/fix/Cargo.toml"),
        "[package]\nname='fix'\nversion='0.0.0'\nedition='2024'\n",
    )
    .unwrap();
    std::fs::write(
        root.join("crates/fix/src/lib.rs"),
        "mod vocabulary;\nmod front;\nmod middle;\nmod back;\n",
    )
    .unwrap();
    for (path, text) in files {
        let path = root.join("crates/fix/src").join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    let snapshot = crate::discovery::snapshot_workspace(&root).unwrap();
    std::fs::remove_dir_all(root).unwrap();
    snapshot
}
fn crate_map(files: &[(&str, String)]) -> CrateMap {
    Workspace::of(&snapshot(files)).unwrap().crates.remove(0)
}

// A fixture wrapper supplies interpreted names. Real project FM interpretation
// and citation tests stay native, outside this generic mechanism.
fn declarations(map: &CrateMap) -> BTreeMap<String, BoundaryDeclaration> {
    map.files
        .iter()
        .filter_map(|file| {
            let module = module_path_of(&map.src_rel, &file.rel)?;
            if module.contains("::") {
                return None;
            }
            let header = file.source.projection().module_header_lines();
            let start = header.iter().position(|line| line.starts_with("Boundary"));
            let text = start
                .map(|start| {
                    header[start..]
                        .iter()
                        .take_while(|line| !line.is_empty())
                        .map(String::as_str)
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .unwrap_or_default();
            let listed = text
                .split('`')
                .skip(1)
                .step_by(2)
                .filter(|name| {
                    name.starts_with(|c: char| c.is_ascii_alphabetic())
                        && name.chars().all(|c| c == '_' || c.is_ascii_alphanumeric())
                })
                .map(str::to_owned)
                .collect();
            Some((
                module,
                BoundaryDeclaration {
                    file: file.rel.clone(),
                    content_fingerprint: crate::discovery::fingerprint(
                        file.source.text().as_bytes(),
                    ),
                    listed,
                    citation_problems: Vec::new(),
                },
            ))
        })
        .collect()
}

fn order() -> StageOrder {
    StageOrder {
        crate_name: "fix".to_owned(),
        stages: STAGES.map(str::to_owned).to_vec(),
    }
}

/// middle reads front's boundary and owes a row for front's interior; back
/// reads middle's boundary and owes a row for front.
const POLICY: &str = r#"
    front = {}
    middle = { input = ["front"], debt = { front = { items = 1, owner = "murail-zhe09.29" } } }
    back = { input = ["middle"], debt = { front = { items = 2, owner = "murail-zhe09.29" } } }
"#;
const SEAMS: &str = r#"
    floor = ["vocabulary"]
    test_coupling = { back = { pairs = 1, owner = "murail-zhe09.29" } }
"#;

const FRONT_HEADER: &str = "//! Front.\n//!\n//! Boundary (`fm/core.md` §Definition: core terms): `Presentation`,\n//! `Certified`.\n";
const FRONT_BODY: &str = "pub(crate) mod check;\nmod work;\npub(crate) use check::Certified;\npub(crate) struct Presentation;\n";
const MIDDLE: &str = "//! Boundary (FM-1-2): `Graph`.\nuse crate::front::Presentation;\nuse crate::front::check::Certified;\nuse crate::vocabulary::Name;\nuse crate::front::work;\npub(crate) struct Graph;\nfn lower() { work::interior(); }\n";
const BACK: &str =
    "use crate::middle::Graph;\nuse crate::front::work::interior;\n#[cfg(test)]\nmod tests;\n";
const BACK_TESTS: &str = "use crate::middle::Graph;\nuse crate::front::check::helper;\n";

/// The fixture tree, one file per entry, each replaceable by index.
fn tree() -> Vec<(&'static str, String)> {
    vec![
        ("vocabulary/mod.rs", "pub(crate) struct Name;\n".to_owned()),
        ("front/mod.rs", format!("{FRONT_HEADER}{FRONT_BODY}")),
        (
            "front/check.rs",
            "pub(crate) struct Certified;\npub(crate) fn helper() {}\npub(crate) fn other() {}\n"
                .to_owned(),
        ),
        (
            "front/work.rs",
            "pub(crate) fn interior() {}\npub(crate) fn scratch() {}\n".to_owned(),
        ),
        ("middle/mod.rs", MIDDLE.to_owned()),
        ("back/mod.rs", BACK.to_owned()),
        ("back/tests.rs", BACK_TESTS.to_owned()),
    ]
}

const FRONT: usize = 1;
const MIDDLE_ROOT: usize = 4;
const BACK_ROOT: usize = 5;
const BACK_TEST_FILE: usize = 6;

fn read(files: &[(&str, String)], policy: &str, seams: &str) -> (SeamReading, StageReport) {
    let map = crate_map(files);
    let mut report = StageReport::of(&map, &order().stages);
    report.interfaces = Some(toml::from_str(policy).expect("fixture rows"));
    let seams: policy::StageSeams = toml::from_str(seams).expect("fixture seams");
    report.floor.clone_from(&seams.floor);
    (
        SeamReading::of(
            &map,
            &order(),
            &report,
            &seams,
            &declarations(&map),
            &RootTable::of(&map, TestSelection::Production),
        )
        .unwrap(),
        report,
    )
}

fn published(policy: &str, seams: &str) -> policy::PublishedStages {
    policy::PublishedStages {
        orders: vec![order()],
        reads: BTreeMap::from([("fix".to_owned(), toml::from_str(policy).expect("rows"))]),
        seams: BTreeMap::from([("fix".to_owned(), toml::from_str(seams).expect("seams"))]),
    }
}

/// The candidate's refusals against the fixture tree as its base.
fn refusals(files: &[(&str, String)], policy: &str, seams: &str) -> Vec<String> {
    let base = read(&tree(), POLICY, SEAMS).0;
    let (now, report) = read(files, policy, seams);
    now.refusals(Some(&base), &report, &published(POLICY, SEAMS))
}

fn with(index: usize, text: &str) -> Vec<(&'static str, String)> {
    let mut files = tree();
    files[index].1 = text.to_owned();
    files
}

#[test]
fn a_boundary_read_is_free_and_an_interior_read_is_debt() {
    let (reading, report) = read(&tree(), POLICY, SEAMS);
    // The root's own item and its listed re-export, at either spelling, are
    // the boundary; the floor is free; the alias-carried interior read is debt.
    let owed: Vec<&str> = reading.debt["middle"]["front"]
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(owed, ["interior"]);
    let tests: Vec<&str> = reading.tests["back"]
        .keys()
        .map(|(_, item)| item.as_str())
        .collect();
    assert_eq!(tests, ["front::helper"]);
    assert!(
        reading
            .refusals(None, &report, &published(POLICY, SEAMS))
            .is_empty()
    );
}

#[test]
fn a_new_interior_read_is_refused_by_count_and_by_its_row() {
    let grown = format!("{MIDDLE}fn more() {{ crate::front::check::helper(); }}\n");
    let found = refusals(&with(MIDDLE_ROOT, &grown), POLICY, SEAMS);
    assert_eq!(found.len(), 2, "{found:#?}");
    assert!(
        found[0].contains("middle reads 2 items of front past its boundary, above the row's 1")
    );
    assert!(found[1].contains("middle newly reads `helper` of front"));
    // A swap keeps the count and is still refused by name.
    let swapped = MIDDLE.replace("work::interior()", "crate::front::check::helper()");
    let found = refusals(&with(MIDDLE_ROOT, &swapped), POLICY, SEAMS);
    assert_eq!(found.len(), 1, "{found:#?}");
    assert!(found[0].contains("newly reads `helper`"));
}

#[test]
fn a_name_read_from_another_stage_cannot_use_a_rows_slack() {
    // back reads `scratch` of middle; reading front's `scratch` as well fits
    // the front row's slack, but the (back, front) row never read it.
    let middle = format!("{MIDDLE}pub(crate) mod scratch_home;\n");
    let mut files = with(MIDDLE_ROOT, &middle);
    files.push((
        "middle/scratch_home.rs",
        "pub(crate) fn scratch() {}\n".to_owned(),
    ));
    let policy = POLICY.replace(
        r#"back = { input = ["middle"], debt = { front = { items = 2, owner = "murail-zhe09.29" } } }"#,
        r#"back = { input = ["middle"], debt = { front = { items = 2, owner = "murail-zhe09.29" }, middle = { items = 1, owner = "murail-zhe09.29" } } }"#,
    );
    files[BACK_ROOT].1 = format!("{BACK}use crate::middle::scratch_home::scratch;\n");
    let base = read(&files, &policy, SEAMS).0;
    files[BACK_ROOT].1 = format!(
        "{BACK}use crate::middle::scratch_home::scratch;\nfn f() {{ crate::front::work::scratch(); }}\n"
    );
    let (now, report) = read(&files, &policy, SEAMS);
    let found = now.refusals(Some(&base), &report, &published(&policy, SEAMS));
    assert_eq!(found.len(), 1, "{found:#?}");
    assert!(found[0].contains("back newly reads `scratch` of front"));
}

#[test]
fn a_cross_stage_glob_counts_as_its_module() {
    // A glob into a second front module is a new item, not a repeat of `*`.
    let back = format!("{BACK}use crate::front::work::*;\n");
    let (reading, _) = read(&with(BACK_ROOT, &back), POLICY, SEAMS);
    let owed: Vec<&str> = reading.debt["back"]["front"]
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(owed, ["interior", "work::*"]);
    let found = refusals(&with(BACK_ROOT, &back), POLICY, SEAMS);
    assert_eq!(found.len(), 1, "{found:#?}");
    assert!(found[0].contains("back newly reads `work::*` of front"));
    let tests = format!("{BACK_TESTS}use crate::front::check::*;\n");
    let found = refusals(&with(BACK_TEST_FILE, &tests), POLICY, SEAMS);
    assert!(
        found
            .iter()
            .any(|r| r.contains("newly name `front::check::*`")),
        "{found:#?}"
    );
}

#[test]
fn an_input_interior_read_without_a_row_is_refused() {
    let middle = format!("{MIDDLE}pub(crate) mod scratch;\n");
    let mut files = with(MIDDLE_ROOT, &middle);
    files.push(("middle/scratch.rs", String::new()));
    files[BACK_ROOT].1 = format!("{BACK}use crate::middle::scratch;\n");
    // A module import is no read by itself; an item through it is.
    let (now, report) = read(&files, POLICY, SEAMS);
    assert!(
        now.refusals(None, &report, &published(POLICY, SEAMS))
            .is_empty()
    );
    files[BACK_ROOT].1 =
        format!("{BACK}use crate::middle::scratch;\nfn f() {{ scratch::step(); }}\n");
    let (now, report) = read(&files, POLICY, SEAMS);
    let found = now.refusals(None, &report, &published(POLICY, SEAMS));
    assert_eq!(found.len(), 1, "{found:#?}");
    assert!(found[0].contains("back reads `step`"));
    assert!(found[0].contains("inside its input middle"));
}

#[test]
fn a_test_item_its_owner_never_named_is_refused_even_by_a_swap() {
    // Same count, new item: refused. Same item, new file: passes.
    let swapped = BACK_TESTS.replace("check::helper", "check::other");
    let found = refusals(&with(BACK_TEST_FILE, &swapped), POLICY, SEAMS);
    assert_eq!(found.len(), 1, "{found:#?}");
    assert!(found[0].contains("back's tests newly name `front::other`"));
    let mut moved = with(BACK_TEST_FILE, "use crate::middle::Graph;\n");
    moved[BACK_ROOT].1 = format!("{BACK}#[cfg(test)]\nmod more_tests;\n");
    moved.push(("back/more_tests.rs", BACK_TESTS.to_owned()));
    assert!(refusals(&moved, POLICY, SEAMS).is_empty());
}

#[test]
fn a_coupled_test_beyond_the_count_is_refused() {
    // An inline test module's `super::super` walks from the module it sits
    // in, so the read lands in front rather than past the root.
    let middle = format!(
        "{MIDDLE}#[cfg(test)]\nmod probe {{\n    use super::super::front::work::interior;\n}}\n"
    );
    let mut files = with(MIDDLE_ROOT, &middle);
    files[BACK_TEST_FILE].1 = format!("{BACK_TESTS}use crate::front::work::interior;\n");
    let found = refusals(&files, POLICY, SEAMS);
    assert!(
        found
            .iter()
            .any(|r| r.contains("back's tests name 2 (test file, item) pairs")),
        "{found:#?}"
    );
    assert!(
        found
            .iter()
            .any(|r| r.contains("back's tests newly name `front::interior`")),
        "{found:#?}"
    );
    assert!(
        found.iter().any(|r| r.contains("middle's tests name 1")),
        "{found:#?}"
    );
}

#[test]
fn a_root_export_is_boundary_only_when_its_header_lists_it() {
    let header = FRONT_HEADER.replace("`Presentation`,\n//! `Certified`.", "`Presentation`.");
    let found = refusals(
        &with(FRONT, &format!("{header}{FRONT_BODY}")),
        POLICY,
        SEAMS,
    );
    assert!(
        found
            .iter()
            .any(|r| r.contains("middle newly reads `Certified` of front")),
        "{found:#?}"
    );
    assert!(
        found.iter().any(|r| r.contains("front's root exports 1 items its //! Boundary paragraph does not list, above the recorded 0")),
        "{found:#?}"
    );
}

#[test]
fn a_newly_listed_name_is_reported_for_the_landing() {
    let base = read(&tree(), POLICY, SEAMS).0;
    let middle = MIDDLE.replace("`Graph`.", "`Graph`, `lower`.");
    let (now, _) = read(&with(MIDDLE_ROOT, &middle), POLICY, SEAMS);
    assert_eq!(
        now.listing_changes(Some(&base)),
        (vec!["middle::lower".to_owned()], vec![])
    );
    let (now, _) = read(
        &with(MIDDLE_ROOT, &MIDDLE.replace("`Graph`", "Graph")),
        POLICY,
        SEAMS,
    );
    assert_eq!(
        now.listing_changes(Some(&base)),
        (vec![], vec!["middle::Graph".to_owned()])
    );
}

#[test]
fn a_wildcard_and_an_item_macro_in_a_root_are_refused() {
    let front = format!("{FRONT_HEADER}{FRONT_BODY}pub(crate) use check::*;\n");
    let found = refusals(&with(FRONT, &front), POLICY, SEAMS);
    assert_eq!(found.len(), 1, "{found:#?}");
    assert!(found[0].contains("front's root re-exports a wildcard"));
    let front = format!("{FRONT_HEADER}{FRONT_BODY}generate_items!(rows);\n");
    let found = refusals(&with(FRONT, &front), POLICY, SEAMS);
    assert_eq!(found.len(), 1, "{found:#?}");
    assert!(found[0].contains("front's root invokes `generate_items!` at item level"));
}

#[test]
fn a_new_unlisted_export_is_refused_and_a_raised_row_cannot_bless_it() {
    let front = format!("{FRONT_HEADER}{FRONT_BODY}pub(crate) fn extra() {{}}\n");
    let found = refusals(&with(FRONT, &front), POLICY, SEAMS);
    assert_eq!(found.len(), 2, "{found:#?}");
    assert!(found[0].contains("front's root exports 1 items"));
    assert!(found[1].contains("root newly exports `extra`"));
    let raised =
        format!("{SEAMS}\n[exports]\nfront = {{ items = 1, owner = \"murail-zhe09.29\" }}\n");
    let found = refusals(&with(FRONT, &front), POLICY, &raised);
    assert_eq!(found.len(), 1, "{found:#?}");
    assert!(found[0].contains("root newly exports `extra`"));
    // Against a published row of 0, the raised row is itself refused.
    let (now, report) = read(&with(FRONT, &front), POLICY, &raised);
    let zero = raised.replace("items = 1", "items = 0");
    let found = now.refusals(None, &report, &published(POLICY, &zero));
    assert_eq!(found.len(), 1, "{found:#?}");
    assert!(found[0].contains("front's recorded unlisted exports rose from 0 to 1"));
}

/// front lists `Arena` whole; middle produces it.
const WRITERS: &str = r#"
[writers.Arena]
stage = "front"
reason = "read access until the view exists"
methods = ["intern"]
functions = ["remap"]
producers = ["front", "middle"]
owner = "murail-zhe09.34"
"#;

#[test]
fn a_writer_call_outside_the_producers_is_counted_and_held() {
    let header = FRONT_HEADER.replace("`Certified`.", "`Certified`, `Arena`.");
    let front = format!("{header}{FRONT_BODY}pub(crate) struct Arena;\n");
    let mut files = with(FRONT, &front);
    files[3].1.push_str("pub(crate) fn remap() {}\n");
    // middle, a producer, calls both and reads `remap` free.
    files[MIDDLE_ROOT].1 =
        format!("{MIDDLE}fn build(a: &mut u8) {{ a.intern(1); crate::front::work::remap(); }}\n");
    let seams = format!("{SEAMS}{WRITERS}");
    let (now, report) = read(&files, POLICY, &seams);
    assert!(!now.debt["middle"]["front"].contains_key("remap"));
    assert!(
        now.refusals(None, &report, &published(POLICY, &seams))
            .is_empty()
    );
    // back is not a producer: its call is refused until recorded.
    files[BACK_ROOT].1 = format!("{BACK}fn f(a: &mut u8) {{ a.intern(2); }}\n");
    let (now, report) = read(&files, POLICY, &seams);
    let found = now.refusals(None, &report, &published(POLICY, &seams));
    assert_eq!(found.len(), 1, "{found:#?}");
    assert!(found[0].contains("back builds or changes `Arena` at 1 sites, above the recorded 0"));
    let recorded = format!("{seams}sites = {{ back = 1 }}\n");
    let (now, report) = read(&files, POLICY, &recorded);
    let found = now.refusals(None, &report, &published(POLICY, &recorded));
    assert!(found.is_empty(), "{found:#?}");
    // A recorded count cannot rise from the published one.
    let zero = format!("{seams}sites = {{ back = 0 }}\n");
    let found = now.refusals(None, &report, &published(POLICY, &zero));
    assert_eq!(found.len(), 1, "{found:#?}");
    assert!(found[0].contains("back's recorded `Arena` writer sites rose from 0 to 1"));
    // A writers row for a type its stage neither declares nor lists is refused.
    let (now, report) = read(&tree(), POLICY, &seams);
    let found = now.refusals(None, &report, &published(POLICY, &seams));
    assert_eq!(found.len(), 2, "{found:#?}");
    assert!(found[0].contains("which declares no `Arena`"));
    assert!(found[1].contains("does not list it"));
}

#[test]
fn a_mounted_support_file_is_read_as_its_modules_tests() {
    let mut files = tree();
    files.push((
        "lib.rs",
        "#[cfg(any(test, feature = \"strategies\"))]\n#[path = \"../tests/support/mod.rs\"]\nmod test_support;\n"
            .to_owned(),
    ));
    files.push((
        "../tests/support/mod.rs",
        "use crate::front::work::interior;\nmod deeper;\n".to_owned(),
    ));
    files.push((
        "../tests/support/deeper.rs",
        "use crate::front::check::other;\n".to_owned(),
    ));
    let (reading, _) = read(&files, POLICY, SEAMS);
    let pairs: Vec<&str> = reading.tests["test_support"]
        .keys()
        .map(|(_, item)| item.as_str())
        .collect();
    assert_eq!(pairs, ["front::other", "front::interior"]);
    let found = refusals(&files, POLICY, SEAMS);
    assert!(
        found
            .iter()
            .any(|r| r.contains("test_support's tests name 2 (test file, item) pairs")),
        "{found:#?}"
    );
}

#[test]
fn a_listed_item_must_live_in_the_listing_stage() {
    let middle = MIDDLE.replace("`Graph`.", "`Graph`, `interior`.");
    let mut files = with(
        MIDDLE_ROOT,
        &format!("{middle}pub(crate) use crate::front::work::interior;\n"),
    );
    let found = refusals(&files, POLICY, SEAMS);
    assert!(
        found.iter().any(|r| r
            .contains("lists `interior`, whose item is `front::work::interior` in another stage")),
        "{found:#?}"
    );
    files[MIDDLE_ROOT].1 =
        format!("{middle}pub(crate) type interior = crate::front::work::Thing;\n");
    let found = refusals(&files, POLICY, SEAMS);
    assert!(
        found
            .iter()
            .any(|r| r.contains("whose item is `front::work::Thing` in another stage")),
        "{found:#?}"
    );
}

/// front declares `Arena` with writer methods derived from its impl.
const ARENA: &str = "pub(crate) struct Arena;\npub(crate) struct Compacted { arena: Arena }\nimpl Arena {\n    pub(crate) fn new() -> Self {\n        Self\n    }\n    pub(crate) fn intern(&mut self, _: u8) {}\n    pub(crate) fn read(&self) -> u8 {\n        0\n    }\n    pub(crate) fn compact(&self) -> Compacted {\n        Compacted { arena: Arena }\n    }\n}\n";

#[test]
fn writer_methods_are_derived_from_the_type_and_its_aliases() {
    let header = FRONT_HEADER.replace("`Certified`.", "`Certified`, `Arena`.");
    let mut files = with(
        FRONT,
        &format!("{header}{FRONT_BODY}pub(crate) mod arena;\n"),
    );
    files.push(("front/arena.rs", ARENA.to_owned()));
    files[BACK_ROOT].1 = format!(
        "{BACK}type Pool = crate::front::arena::Arena;\nfn f(a: &mut Pool) {{\n    let _ = a.read();\n    let _ = Pool::new();\n    let _ = <Arena>::new();\n    let _ = a.compact();\n    a.intern(1);\n}}\n"
    );
    let row = WRITERS.replace("methods = [\"intern\"]\n", "");
    let seams = format!("{SEAMS}{row}");
    let (now, _) = read(&files, POLICY, &seams);
    let census = &now.writer_census["Arena"];
    assert_eq!(
        census
            .methods
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["compact", "intern", "new"]
    );
    assert_eq!(
        census
            .aliases
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["Pool"]
    );
    // The read is not a writer; each other spelling is, through the alias too.
    let spelled: Vec<&str> = census.calls["back"]
        .iter()
        .map(|call| call.site.path.as_str())
        .collect();
    assert_eq!(
        spelled,
        ["Pool::new(", "<Arena>::new(", ".compact(", ".intern("]
    );
}

#[test]
fn a_writers_row_and_the_floor_only_tighten() {
    let header = FRONT_HEADER.replace("`Certified`.", "`Certified`, `Arena`.");
    let mut files = with(
        FRONT,
        &format!("{header}{FRONT_BODY}pub(crate) mod arena;\n"),
    );
    files.push(("front/arena.rs", ARENA.to_owned()));
    let seams = format!("{SEAMS}{WRITERS}");
    let policy_refusals = |candidate: &str| {
        let (now, report) = read(&files, POLICY, candidate);
        now.refusals(None, &report, &published(POLICY, &seams))
    };
    assert!(
        policy_refusals(&seams).is_empty(),
        "{:#?}",
        policy_refusals(&seams)
    );
    // The row removed, a method dropped, a producer added: each refused.
    for (candidate, refusal) in [
        (
            SEAMS.to_owned(),
            "the published writers row for `Arena` is gone",
        ),
        (
            seams.replace("methods = [\"intern\"]\n", ""),
            "it no longer counts intern",
        ),
        (
            seams.replace(
                "producers = [\"front\", \"middle\"]",
                "producers = [\"front\", \"middle\", \"back\"]",
            ),
            "it adds producers back",
        ),
    ] {
        let found = policy_refusals(&candidate);
        assert!(
            found.iter().any(|r| r.contains(refusal)),
            "{refusal}: {found:#?}"
        );
    }
    // An existing stage moving into the floor is refused, naming its list.
    let floored = seams.replace(
        "floor = [\"vocabulary\"]",
        "floor = [\"vocabulary\", \"front\"]",
    );
    let found = policy_refusals(&floored);
    assert!(
        found
            .iter()
            .any(|r| r.contains("front moves into the floor") && r.contains("Presentation")),
        "{found:#?}"
    );
}

#[test]
fn a_floor_type_takes_a_writers_row_without_a_listing() {
    // W's shape: the cell graph in the floor, written by its producers only.
    let mut files = tree();
    files[0].1 =
        "pub(crate) struct Graph;\nimpl Graph {\n    pub(crate) fn rewrite(&mut self) {}\n}\n"
            .to_owned();
    files[BACK_ROOT].1 =
        format!("{BACK}fn f(g: &mut crate::vocabulary::Graph) {{ g.rewrite(); }}\n");
    let row = "\n[writers.Graph]\nstage = \"vocabulary\"\nreason = \"one object every later stage reads\"\nproducers = [\"middle\"]\nowner = \"murail-zhe09.30\"\n";
    let seams = format!("{SEAMS}{row}");
    let (now, report) = read(&files, POLICY, &seams);
    let found = now.refusals(None, &report, &published(POLICY, &seams));
    assert_eq!(found.len(), 1, "{found:#?}");
    assert!(found[0].contains("back builds or changes `Graph` at 1 sites, above the recorded 0"));
}

fn source_policy() -> policy::Policy {
    policy::Policy::parse(&format!(
        "schema=1\nprovenance='authored'\nsurface={{}}\nauthority=[]\nstage_order=[{{crate='fix',stages=['vocabulary','front','middle','back']}}]\n[population]\ncore=['fix']\n[size]\ncore_token_lines=100\ncrate={{fix=100}}\n[crate_graph]\nrule=[]\n[stage_reads.fix]\n{POLICY}\n[stage_seams.fix]\n{SEAMS}\n"
    )).unwrap()
}

#[test]
fn missing_or_stale_boundary_evidence_never_constructs_a_seam_reading() {
    let snapshot = snapshot(&tree());
    let map = Workspace::of(&snapshot).unwrap().crates.remove(0);
    let policy = source_policy();
    let mut inputs = BTreeMap::from([("fix".to_owned(), declarations(&map))]);
    let valid = crate::checks::seams(&snapshot, &policy, &inputs, None).unwrap();
    assert!(valid.unavailable.is_empty(), "{:?}", valid.unavailable);
    assert!(valid.crates.contains_key("fix"));
    let declared = inputs.get_mut("fix").unwrap().remove("front").unwrap();
    let missing = crate::checks::seams(&snapshot, &policy, &inputs, None).unwrap();
    assert!(!missing.crates.contains_key("fix"));
    assert!(
        missing
            .unavailable
            .iter()
            .any(|issue| issue.message.contains("declaration unavailable for front"))
    );
    inputs.get_mut("fix").unwrap().insert(
        "front".into(),
        BoundaryDeclaration {
            content_fingerprint: "stale".into(),
            ..declared
        },
    );
    let stale = crate::checks::seams(&snapshot, &policy, &inputs, None).unwrap();
    assert!(!stale.crates.contains_key("fix"));
    assert!(
        stale
            .unavailable
            .iter()
            .any(|issue| issue.message.contains("does not match captured source"))
    );
}

#[test]
fn a_missing_mounted_test_or_unreadable_test_import_makes_the_census_unavailable() {
    for test in [
        "#[path=\"../../missing.rs\"]\nmod support;\n",
        "use crate::front::{;\n",
    ] {
        let snapshot = snapshot(&with(BACK_TEST_FILE, test));
        let map = Workspace::of(&snapshot).unwrap().crates.remove(0);
        let inputs = BTreeMap::from([("fix".to_owned(), declarations(&map))]);
        let report = crate::checks::seams(&snapshot, &source_policy(), &inputs, None).unwrap();
        assert!(report.crates.is_empty());
        assert!(!report.unavailable.is_empty(), "{test}");
    }
}

#[test]
fn test_only_root_aliases_resolve_in_the_test_population_and_unknowns_do_not_disappear() {
    let mut files = tree();
    files.push(("lib.rs","mod vocabulary;\nmod front;\nmod middle;\nmod back;\n#[cfg(test)]\npub use front::check::helper as Helper;\n".into()));
    files[BACK_TEST_FILE].1 = "use crate::Helper;\n".into();
    let (reading, _) = read(&files, POLICY, SEAMS);
    assert!(
        reading.tests["back"]
            .keys()
            .any(|(_, item)| item == "front::helper")
    );
    files[BACK_TEST_FILE].1 = "use crate::UnknownAlias;\n".into();
    let snapshot = snapshot(&files);
    let map = Workspace::of(&snapshot).unwrap().crates.remove(0);
    let inputs = BTreeMap::from([("fix".to_owned(), declarations(&map))]);
    let unknown = crate::checks::seams(&snapshot, &source_policy(), &inputs, None).unwrap();
    assert!(unknown.crates.is_empty());
    assert!(
        unknown
            .unavailable
            .iter()
            .any(|issue| issue.message.contains("UnknownAlias")
                && issue.message.contains("unjudged"))
    );
}

#[test]
fn project_citation_findings_reach_the_ratchet_without_becoming_core_interpretation() {
    let map = crate_map(&tree());
    let mut inputs = declarations(&map);
    inputs
        .get_mut("front")
        .unwrap()
        .citation_problems
        .push("project resolver rejected this citation".into());
    let mut report = StageReport::of(&map, &order().stages);
    report.interfaces = Some(toml::from_str(POLICY).unwrap());
    let held: policy::StageSeams = toml::from_str(SEAMS).unwrap();
    let reading = SeamReading::of(
        &map,
        &order(),
        &report,
        &held,
        &inputs,
        &RootTable::of(&map, TestSelection::Production),
    )
    .unwrap();
    assert!(
        reading
            .refusals(None, &report, &published(POLICY, SEAMS))
            .iter()
            .any(|refusal| refusal.contains("project resolver rejected this citation"))
    );
}

#[test]
fn a_test_only_stage_is_absent_from_production_paths_and_boundaries() {
    let mut files = tree();
    files.push((
        "lib.rs",
        "mod vocabulary;\n#[cfg(test)]\nmod front;\nmod middle;\nmod back;\n".into(),
    ));
    let snapshot = snapshot(&files);
    let map = Workspace::of(&snapshot).unwrap().crates.remove(0);
    let report = StageReport::of(&map, &order().stages);
    assert!(report.absent.iter().any(|stage| stage == "front"));
    assert!(
        report
            .unjudged
            .iter()
            .any(|issue| issue.path.contains("front::Presentation"))
    );
    assert!(!report.backward.iter().any(|edge| edge.to == "front"));
    let held: policy::StageSeams = toml::from_str(SEAMS).unwrap();
    let root = RootTable::of(&map, TestSelection::Production);
    let reading =
        SeamReading::of(&map, &order(), &report, &held, &declarations(&map), &root).unwrap();
    assert!(reading.json()["boundaries"].get("front").is_none());
}
