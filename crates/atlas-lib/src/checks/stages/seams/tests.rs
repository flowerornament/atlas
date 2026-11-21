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
    let fixture = crate::test_support::Fixture::new(&[]);
    let root = fixture.path();
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
    crate::discovery::snapshot_workspace(root).unwrap()
}
fn crate_map(files: &[(&str, String)]) -> CrateMap {
    supported_map(files, &[])
}
/// The crate, with `features` declared as test support.
fn supported_map(files: &[(&str, String)], features: &[&str]) -> CrateMap {
    let features: Vec<String> = features.iter().map(|f| (*f).to_owned()).collect();
    Workspace::of(&snapshot(files))
        .unwrap()
        .with_test_support(&features)
        .crates
        .remove(0)
}

// A fixture wrapper supplies interpreted names. Real project interpretation
// and citation tests belong to project wrappers, outside this generic mechanism.
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
            let listed: Vec<String> = text
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
                BoundaryDeclaration::new(
                    file.rel.clone(),
                    crate::discovery::fingerprint(file.source.text().as_bytes()),
                )
                .with_listed(listed),
            ))
        })
        .collect()
}

fn order() -> StageOrder {
    StageOrder {
        crate_name: "fix".to_owned(),
        stages: STAGES.map(str::to_owned).to_vec(),
        pipeline: Vec::new(),
    }
}

/// middle reads front's boundary and owes a row for front's interior; back
/// reads middle's boundary and owes a row for front.
const POLICY: &str = r#"
    front = {}
    middle = { input = ["front"], debt = { front = { items = 1, owner = "issue-29" } } }
    back = { input = ["middle"], debt = { front = { items = 2, owner = "issue-29" } } }
"#;
const SEAMS: &str = r#"
    floor = ["vocabulary"]
    test_coupling = { back = { pairs = 1, owner = "issue-29" } }
"#;

const FRONT_HEADER: &str = "//! Front.\n//!\n//! Boundary (`spec/core.md` §Definition: core terms): `Presentation`,\n//! `Certified`.\n";
const FRONT_BODY: &str = "pub(crate) mod check;\nmod work;\npub(crate) use check::Certified;\npub(crate) struct Presentation;\n";
const MIDDLE: &str = "//! Boundary (SPEC-1-2): `Graph`.\nuse crate::front::Presentation;\nuse crate::front::check::Certified;\nuse crate::vocabulary::Name;\nuse crate::front::work;\npub(crate) struct Graph;\nfn lower() { work::interior(); }\n";
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
    read_supported(files, policy, seams, &[]).expect("a complete seam reading")
}
/// The reading with `features` declared as test support, or what made it
/// unavailable.
fn read_supported(
    files: &[(&str, String)],
    policy: &str,
    seams: &str,
    features: &[&str],
) -> Result<(SeamReading, StageReport), Vec<crate::model::Diagnostic>> {
    read_declared(files, policy, seams, features, |_| {})
}
/// As `read_supported`, with the fixture wrapper's declarations adjusted.
fn read_declared(
    files: &[(&str, String)],
    policy: &str,
    seams: &str,
    features: &[&str],
    adjust: impl FnOnce(&mut BTreeMap<String, BoundaryDeclaration>),
) -> Result<(SeamReading, StageReport), Vec<crate::model::Diagnostic>> {
    let map = supported_map(files, features);
    let mut report = StageReport::of(&map, &order().stages);
    report.interfaces = Some(toml::from_str(policy).expect("fixture rows"));
    let seams: policy::StageSeams = toml::from_str(seams).expect("fixture seams");
    report.floor.clone_from(&seams.floor);
    SeamReading::of(
        &map,
        &order(),
        &report,
        &seams,
        &{
            let mut declared = declarations(&map);
            adjust(&mut declared);
            declared
        },
        &RootTable::of(&map, TestSelection::Production),
    )
    .map(|reading| (reading, report))
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
        r#"back = { input = ["middle"], debt = { front = { items = 2, owner = "issue-29" } } }"#,
        r#"back = { input = ["middle"], debt = { front = { items = 2, owner = "issue-29" }, middle = { items = 1, owner = "issue-29" } } }"#,
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
    let raised = format!("{SEAMS}\n[exports]\nfront = {{ items = 1, owner = \"issue-29\" }}\n");
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
owner = "issue-34"
test_sites = {}
production_sites = {}
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
    assert!(
        found[0]
            .contains("back builds or changes `Arena` at 1 production sites, above the recorded 0")
    );
    let recorded = seams.replace("production_sites = {}", "production_sites = { back = 1 }");
    let (now, report) = read(&files, POLICY, &recorded);
    let found = now.refusals(None, &report, &published(POLICY, &recorded));
    assert!(found.is_empty(), "{found:#?}");
    // A recorded count cannot rise from the published one, and an owner
    // the published row does not name recorded 0.
    let found = now.refusals(None, &report, &published(POLICY, &seams));
    assert_eq!(found.len(), 1, "{found:#?}");
    assert!(found[0].contains("back's recorded production `Arena` writer sites rose from 0 to 1"));
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
    // Its gate admits production under `strategies`: undeclared, it is a
    // production mount the stage reading cannot read.
    let Err(unread) = read_supported(&files, POLICY, SEAMS, &[]) else {
        panic!("a production mount outside src must make the reading unavailable");
    };
    assert_eq!(unread.len(), 2, "{unread:#?}");
    assert!(unread.iter().all(
        |d| d.code == crate::model::DiagnosticCode::ProductionMountUnread
            && d.message.contains("--test-support-features")
    ));
    let (reading, _) =
        read_supported(&files, POLICY, SEAMS, &["strategies"]).expect("declared support");
    let pairs: Vec<&str> = reading.tests["test_support"]
        .keys()
        .map(|(_, item)| item.as_str())
        .collect();
    assert_eq!(pairs, ["front::other", "front::interior"]);
    let base = read(&tree(), POLICY, SEAMS).0;
    let (now, report) =
        read_supported(&files, POLICY, SEAMS, &["strategies"]).expect("declared support");
    let found = now.refusals(Some(&base), &report, &published(POLICY, SEAMS));
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
    let spelled: Vec<&str> = census.production["back"]
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
    let row = "\n[writers.Graph]\nstage = \"vocabulary\"\nreason = \"one object every later stage reads\"\nproducers = [\"middle\"]\nowner = \"issue-30\"\ntest_sites = {}\nproduction_sites = {}\n";
    let seams = format!("{SEAMS}{row}");
    let (now, report) = read(&files, POLICY, &seams);
    let found = now.refusals(None, &report, &published(POLICY, &seams));
    assert_eq!(found.len(), 1, "{found:#?}");
    assert!(
        found[0]
            .contains("back builds or changes `Graph` at 1 production sites, above the recorded 0")
    );
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

fn final_refusals(files: &[(&str, String)], policy: &str, finals: &str) -> Vec<String> {
    let seams = format!("{SEAMS}\n    final_stages = {finals}\n");
    refusals(files, policy, &seams)
        .into_iter()
        .filter(|refusal| refusal.starts_with("stage final"))
        .collect()
}

#[test]
fn a_final_stage_is_read_by_no_stage_but_the_composition() {
    // Only back, the composition, reads middle.
    assert_eq!(
        final_refusals(&tree(), POLICY, r#"["middle"]"#),
        Vec::<String>::new()
    );
}

#[test]
fn an_observed_final_stage_read_is_refused_at_its_site() {
    let found = final_refusals(&tree(), POLICY, r#"["front"]"#);
    assert_eq!(found.len(), 1, "{found:#?}");
    assert!(
        found[0].starts_with("stage final `fix`: crates/fix/src/middle/mod.rs:2 middle reads front (`crate::front::Presentation`), a final stage only back reads"),
        "{found:#?}"
    );
    // A row naming the final stage is a read even with no path observed.
    let quiet = with(
        MIDDLE_ROOT,
        "//! Boundary (SPEC-1-2): `Graph`.\npub(crate) struct Graph;\n",
    );
    let found = final_refusals(&quiet, POLICY, r#"["front"]"#);
    assert_eq!(found.len(), 1, "{found:#?}");
    assert!(
        found[0].contains("middle's stage_reads row names front"),
        "{found:#?}"
    );
}

#[test]
fn a_final_stage_is_a_staged_non_floor_stage_other_than_the_composition() {
    let seams: policy::StageSeams = toml::from_str(&format!(
        "{SEAMS}\n    final_stages = [\"nowhere\", \"vocabulary\", \"back\", \"middle\", \"middle\"]\n    amended_by = {{ bead = \"x-1\", covers = [\"population\"] }}\n"
    ))
    .expect("seams");
    let found = final_stage_shape(&order(), &seams);
    for problem in [
        "`nowhere`, which is not a stage",
        "`vocabulary`, which is a floor stage",
        "`back`, which is the composition",
        "`middle`, which is listed twice",
        "amended_by covers only `final_stages`",
    ] {
        assert!(
            found.iter().any(|v| v.contains(problem)),
            "{problem}: {found:#?}"
        );
    }
    assert_eq!(found.len(), 5, "{found:#?}");
}

/// A whole policy whose crate `fix` runs `stages` and writes `seams`.
fn list_policy(stages: &str, seams: &str) -> policy::Policy {
    policy::Policy::parse(&format!(
        "schema=1\nprovenance='authored'\nsurface={{fix=50}}\nauthority=[{{fact='f',home='src/lib.rs::X'}}]\nstage_order=[{{crate='fix',stages={stages}}}]\n[population]\ncore=['fix']\n[size]\ncore_token_lines=1000\ncrate={{fix=1000}}\n[crate_graph]\nrule=[{{name='fix',allowed=[],law='leaf'}}]\n[stage_seams.fix]\nfloor=[]\n{seams}\n"
    ))
    .expect("fixture policy")
}
fn list_law(published: &policy::Policy, candidate: &policy::Policy, cited: &[&str]) -> Vec<String> {
    let description = format!("change ({})", cited.join(", "));
    let citations = crate::checks::concentration::Citations {
        description_fingerprint: crate::discovery::fingerprint(description.as_bytes()),
        beads: cited.iter().map(|bead| (*bead).to_owned()).collect(),
    };
    let mut unavailable = Vec::new();
    let found = final_stage_list_refusals(
        published,
        candidate,
        &description,
        Some(&citations),
        &mut unavailable,
    );
    assert!(unavailable.is_empty());
    found
}

#[test]
fn the_final_stage_list_is_held_to_the_published_one() {
    let stages = "['a','b','c']";
    let at = |seams: &str| list_policy(stages, seams);
    let one = at("final_stages=['a']");
    // absent → anything passes; the same set passes.
    assert!(list_law(&at(""), &one, &[]).is_empty());
    assert!(list_law(&at(""), &at("final_stages=[]"), &[]).is_empty());
    assert!(list_law(&one, &one, &[]).is_empty());
    for (candidate, refusal) in [
        ("final_stages=['a','b']", "b is not a published final stage"),
        ("final_stages=[]", "the published final stage a is gone"),
        ("", "the published final_stages list is gone"),
    ] {
        let found = list_law(&one, &at(candidate), &[]);
        assert!(
            found.iter().any(|v| v.contains(refusal)),
            "{candidate}: {found:#?}"
        );
    }
    let found = list_law(&at("final_stages=[]"), &at("final_stages=['a']"), &[]);
    assert!(
        found
            .iter()
            .any(|v| v.contains("a is not a published final stage")),
        "{found:#?}"
    );
}

#[test]
fn a_cited_amendment_changes_the_list_and_nothing_else_does() {
    let stages = "['a','b','c']";
    let one = list_policy(stages, "final_stages=['a']");
    let amended = list_policy(
        stages,
        "final_stages=['a','b']\namended_by={bead='x-1',covers=['final_stages']}",
    );
    assert_eq!(list_law(&one, &amended, &["x-1"]), Vec::<String>::new());
    let uncited = list_law(&one, &amended, &[]);
    assert!(
        uncited.iter().any(|v| v
            .contains("amended_by names `x-1`, which this landing's description does not cite")),
        "{uncited:#?}"
    );
    let idle = list_policy(
        stages,
        "final_stages=['a']\namended_by={bead='x-1',covers=['final_stages']}",
    );
    let found = list_law(&one, &idle, &["x-1"]);
    assert!(
        found
            .iter()
            .any(|v| v.contains("does not change the published list")),
        "{found:#?}"
    );
}

#[test]
fn the_composition_cannot_move_while_final_stages_are_published() {
    let one = list_policy("['a','b','c']", "final_stages=['a']");
    let moved = list_policy("['a','c','b']", "final_stages=['a']");
    let found = list_law(&one, &moved, &[]);
    assert!(
        found
            .iter()
            .any(|v| v.contains("the composition moves from c to b")),
        "{found:#?}"
    );
    let unpublished = list_policy("['a','b','c']", "");
    assert!(list_law(&unpublished, &list_policy("['a','c','b']", ""), &[]).is_empty());
}

/// `front` declaring `Arena` with its `intern` writer, listed in front's
/// boundary; `back` written by `back_root` and its tests by `back_tests`.
fn arena_tree(back_root: &str, back_tests: &str) -> Vec<(&'static str, String)> {
    let header = FRONT_HEADER.replace("`Certified`.", "`Certified`, `Arena`.");
    let mut files = with(
        FRONT,
        &format!("{header}{FRONT_BODY}pub(crate) struct Arena;\n"),
    );
    files[3].1.push_str("pub(crate) fn remap() {}\n");
    files[BACK_ROOT].1 = format!("{BACK}{back_root}");
    files[6].1 = format!("{BACK_TESTS}{back_tests}");
    files
}
fn writer_refusals(files: &[(&str, String)], candidate: &str, published_row: &str) -> Vec<String> {
    let (now, report) = read(files, POLICY, &format!("{SEAMS}{candidate}"));
    now.refusals(
        None,
        &report,
        &published(POLICY, &format!("{SEAMS}{published_row}")),
    )
    .into_iter()
    .filter(|refusal| refusal.starts_with("stage writers"))
    .collect()
}
const TEST_CALL: &str = "fn t(a: &mut u8) {\n    a.intern(1);\n    a.intern(2);\n}\n";
const PRODUCTION_CALL: &str = "fn f(a: &mut u8) { a.intern(3); }\n";

#[test]
fn a_production_writer_is_not_hidden_by_a_test_count() {
    let row = WRITERS.replace("test_sites = {}", "test_sites = { back = 2 }");
    assert_eq!(
        writer_refusals(&arena_tree("", TEST_CALL), &row, &row),
        Vec::<String>::new()
    );
    let found = writer_refusals(&arena_tree(PRODUCTION_CALL, TEST_CALL), &row, &row);
    assert_eq!(found.len(), 1, "{found:#?}");
    assert!(
        found[0]
            .contains("back builds or changes `Arena` at 1 production sites, above the recorded 0"),
        "{found:#?}"
    );
}

#[test]
fn a_production_writer_cannot_replace_a_test_fixture_through_a_new_key() {
    let published_row = WRITERS.to_owned();
    let row = WRITERS.replace("production_sites = {}", "production_sites = { back = 1 }");
    let found = writer_refusals(&arena_tree(PRODUCTION_CALL, ""), &row, &published_row);
    assert_eq!(found.len(), 1, "{found:#?}");
    assert!(found[0].contains("rose from 0 to 1"), "{found:#?}");
}

#[test]
fn an_owned_writer_count_must_equal_its_measure() {
    let row = WRITERS.replace("test_sites = {}", "test_sites = { back = 3 }");
    let found = writer_refusals(&arena_tree("", TEST_CALL), &row, &row);
    assert_eq!(found.len(), 1, "{found:#?}");
    assert!(found[0].contains("test_sites records 3 for back, which has 2; it is owned by `issue-34`, and an owned row must equal its measure: write back = 2"), "{found:#?}");
    let gone = writer_refusals(&arena_tree("", ""), &row, &row);
    assert!(gone[0].contains("delete `back`"), "{gone:#?}");
}

#[test]
fn a_writers_row_records_both_populations_and_never_the_combined_sites() {
    let seams: policy::StageSeams = toml::from_str(&format!(
        "{SEAMS}{}",
        WRITERS.replace("production_sites = {}\n", "sites = { back = 1 }\n")
    ))
    .expect("seams");
    let found = writers_shape("fix", &seams);
    assert_eq!(found.len(), 2, "{found:#?}");
    assert!(found[0].contains("writes `sites`, which counts test and production together; write `test_sites` and `production_sites`"), "{found:#?}");
    assert!(
        found[1].contains(
            "records no `production_sites`; record it per owner, `production_sites = {}` for none"
        ),
        "{found:#?}"
    );
}

#[test]
fn a_combined_published_count_bounds_each_population_by_the_base_tree() {
    let legacy = WRITERS.replace(
        "test_sites = {}\nproduction_sites = {}\n",
        "sites = { back = 2 }\n",
    );
    let row = WRITERS.replace("production_sites = {}", "production_sites = { back = 1 }");
    let candidate = arena_tree(PRODUCTION_CALL, "");
    let (now, report) = read(&candidate, POLICY, &format!("{SEAMS}{row}"));
    let published_stages = published(POLICY, &format!("{SEAMS}{legacy}"));
    // The base held two test writers and no production one: the combined 2
    // cannot fund a production writer.
    let base_files = arena_tree("", TEST_CALL);
    let (base, _) = read(&base_files, POLICY, &format!("{SEAMS}{row}"));
    let found: Vec<String> = now
        .refusals(Some(&base), &report, &published_stages)
        .into_iter()
        .filter(|refusal| refusal.starts_with("stage writers"))
        .collect();
    assert_eq!(found.len(), 1, "{found:#?}");
    assert!(found[0].contains("(1) exceed 0: the published row counted both populations together (2) and the base tree has 0 production sites"), "{found:#?}");
    // A base that held the production writer keeps it.
    let (base, _) = read(&candidate, POLICY, &format!("{SEAMS}{row}"));
    let kept: Vec<String> = now
        .refusals(Some(&base), &report, &published_stages)
        .into_iter()
        .filter(|refusal| refusal.starts_with("stage writers"))
        .collect();
    assert_eq!(kept, Vec::<String>::new());
}

#[test]
fn a_test_gated_mount_and_its_children_are_test_code_and_a_twice_mounted_file_counts_once_per_population()
 {
    let mut files = arena_tree("", "");
    files[BACK_ROOT].1 = format!(
        "{BACK}#[path = \"../../shared/mod.rs\"]\nmod shared;\n#[cfg(test)]\n#[path = \"../../shared/mod.rs\"]\nmod shared_tests;\n"
    );
    files.push((
        "../shared/mod.rs",
        format!("{PRODUCTION_CALL}mod deeper;\n"),
    ));
    files.push((
        "../shared/deeper.rs",
        "fn g(a: &mut u8) { a.intern(4); }\n".to_owned(),
    ));
    let map = crate_map(&files);
    let populations: Vec<(&str, &str, bool)> = map
        .mounted
        .iter()
        .map(|mount| {
            (
                mount.module.as_str(),
                mount.file.rel.as_str(),
                mount.file.test_file,
            )
        })
        .collect();
    assert_eq!(
        populations,
        [
            ("back::shared::deeper", "crates/fix/shared/deeper.rs", false),
            (
                "back::shared_tests::deeper",
                "crates/fix/shared/deeper.rs",
                true
            ),
            ("back::shared", "crates/fix/shared/mod.rs", false),
            ("back::shared_tests", "crates/fix/shared/mod.rs", true),
        ],
        "{populations:#?}"
    );
}

/// A boundary may expose a child module wholesale: its public items are
/// boundary items, read freely, while its private ones stay interior.
#[test]
fn a_listed_module_exposes_its_public_items() {
    let (plain, _) = read(&tree(), POLICY, SEAMS);
    assert!(
        plain.debt["middle"]["front"].contains_key("interior"),
        "the fixture reads work's interior"
    );
    let (listed, report) = read_declared(&tree(), POLICY, SEAMS, &[], |declared| {
        let front = declared.remove("front").expect("front's declaration");
        declared.insert("front".into(), front.with_listed_modules(["work"]));
    })
    .expect("a reading");
    let front = listed
        .debt
        .get("middle")
        .and_then(|reads| reads.get("front"));
    assert!(
        front.is_none_or(|items| !items.contains_key("interior")),
        "{:?}",
        front.map(|items| items.keys().collect::<Vec<_>>())
    );
    // back's read of `work::interior` is free too, so its debt row is slack.
    let found = listed.refusals(None, &report, &published(POLICY, SEAMS));
    assert!(!found.iter().any(|r| r.contains("interior")), "{found:#?}");
}

/// A glob over the crate root in a test file brings the root's names in:
/// each use of one is read where it is written, so the file is judged, not
/// unavailable (atlas-3n1.4).
#[test]
fn a_root_glob_in_a_test_file_is_read_through_its_names() {
    for glob in ["use crate::*;\n", "use super::super::*;\n"] {
        let files = with(6, &format!("{glob}fn t() {{ front::check::helper(); }}\n"));
        let (reading, _) = read_supported(&files, POLICY, SEAMS, &[])
            .unwrap_or_else(|unavailable| panic!("{glob}: {unavailable:#?}"));
        let pairs: Vec<&str> = reading.tests["back"]
            .keys()
            .map(|(_, item)| item.as_str())
            .collect();
        assert_eq!(pairs, ["front::helper"], "{glob}");
    }
}
