//! Native stage/path/interface regression examples.

use super::policy::StageOrder;
use super::*;

fn crate_map(files: &[(&str, &str)]) -> CrateMap {
    CrateMap {
        name: "murail-fix".to_owned(),
        src_rel: "crates/murail-fix/src".to_owned(),
        mounted: Vec::new(),
        files: files
            .iter()
            .map(|(rel, text)| {
                FileMap::of(format!("crates/murail-fix/src/{rel}"), (*text).to_owned())
            })
            .collect(),
    }
}

fn root_of(files: &[(&str, &str)]) -> RootTable {
    RootTable::of(&crate_map(files), TestSelection::Production)
}

/// A row reading `reads` whole and owing one item of each `debt` stage.
fn row(reads: &[&str], debt: &[&str]) -> policy::StageReads {
    policy::StageReads {
        reads: reads.iter().map(|s| (*s).to_owned()).collect(),
        input: Vec::new(),
        debt: policy::StageDebt::Rows(
            debt.iter()
                .map(|stage| {
                    let row: policy::DebtRow =
                        toml::from_str("items = 1\nowner = 'murail-zhe09.29'")
                            .expect("fixture row");
                    ((*stage).to_owned(), row)
                })
                .collect(),
        ),
    }
}

fn order() -> Vec<String> {
    ["load", "schedule", "engine"].map(str::to_owned).to_vec()
}

fn forward(report: &StageReport) -> Vec<(&str, &str)> {
    report
        .forward
        .iter()
        .map(|edge| (edge.from.as_str(), edge.to.as_str()))
        .collect()
}

fn unjudged(report: &StageReport) -> Vec<(&str, Unjudged)> {
    report
        .unjudged
        .iter()
        .map(|path| (path.path.as_str(), path.why))
        .collect()
}

#[test]
fn paths_resolve_from_the_declaring_module() {
    let root = root_of(&[("load.rs", ""), ("schedule.rs", ""), ("engine.rs", "")]);
    let none = BTreeSet::new();
    let read = |module: &[&str], path: &str| read_path(module, path, PathForm::Use, &root, &none);
    assert_eq!(
        read(&["load", "plan"], "super::super::schedule::X"),
        Ok(vec![vec!["schedule".into(), "X".into()]])
    );
    assert_eq!(
        read(&["load", "plan"], "super::Cursor"),
        Ok(vec![vec!["load".into(), "Cursor".into()]])
    );
    assert_eq!(
        read(&["load"], "crate::engine::Engine"),
        Ok(vec![vec!["engine".into(), "Engine".into()]])
    );
    assert_eq!(read(&["load"], "std::fmt"), Ok(vec![]));
    assert_eq!(
        inline_paths("let x = crate::schedule::plan(super::y);", &none),
        vec!["crate::schedule::plan", "super::y"]
    );
}

#[test]
fn backward_imports_are_judged_and_pass() {
    let report = StageReport::of(
        &crate_map(&[
            ("load.rs", "use crate::host::Clock;\n"),
            ("schedule/plan.rs", "use crate::load::{Loaded, self};\n"),
            ("engine.rs", "fn run() { crate::schedule::plan::run(); }\n"),
            ("host.rs", "use crate::engine::Engine;\n"),
        ]),
        &order(),
    );
    assert!(report.forward.is_empty());
    assert_eq!(report.judged, 2);
    assert_eq!(report.unplaced, BTreeSet::from(["host".to_owned()]));
    assert!(report.absent.is_empty());
}

#[test]
fn a_stage_interface_refuses_only_an_undeclared_read() {
    // engine reads schedule (declared) and load (debt); schedule reads
    // load (undeclared): the one refusal is schedule → load.
    let mut report = StageReport::of(
        &crate_map(&[
            ("load.rs", ""),
            ("schedule.rs", "use crate::load::Cursor;\n"),
            (
                "engine.rs",
                "use crate::schedule::Plan;\nuse crate::load::Loaded;\n",
            ),
        ]),
        &order(),
    );
    let declared = row;
    report.interfaces = Some(BTreeMap::from([
        ("engine".to_owned(), declared(&["schedule"], &["load"])),
        ("load".to_owned(), declared(&["schedule"], &[])),
    ]));
    let undeclared: Vec<(&str, &str)> = report
        .undeclared_reads()
        .into_iter()
        .map(|edge| (edge.from.as_str(), edge.to.as_str()))
        .collect();
    assert_eq!(undeclared, [("schedule", "load")]);
    assert_eq!(
        report.unused_reads(),
        [("load".to_owned(), "schedule".to_owned())]
    );
    assert_eq!(report.refusals("murail-fix").len(), 1);
    // A crate that declares no interfaces is not judged.
    report.interfaces = None;
    assert!(report.undeclared_reads().is_empty());
    assert!(report.refusals("murail-fix").is_empty());
}

#[test]
fn a_stage_interface_row_the_orders_cannot_read_is_refused() {
    let orders = vec![StageOrder {
        crate_name: "murail-fix".to_owned(),
        stages: order(),
    }];
    let good = BTreeMap::from([(
        "murail-fix".to_owned(),
        BTreeMap::from([("engine".to_owned(), row(&["schedule"], &["load"]))]),
    )]);
    let unseamed = BTreeMap::new();
    assert!(interface_policy_violations(&orders, &good, &unseamed, None).is_empty());
    for bad in [
        BTreeMap::from([("murail-fx".to_owned(), BTreeMap::new())]),
        BTreeMap::from([(
            "murail-fix".to_owned(),
            BTreeMap::from([("engin".to_owned(), row(&[], &[]))]),
        )]),
        BTreeMap::from([(
            "murail-fix".to_owned(),
            BTreeMap::from([("engine".to_owned(), row(&["shedule"], &[]))]),
        )]),
        BTreeMap::from([(
            "murail-fix".to_owned(),
            BTreeMap::from([("load".to_owned(), row(&["engine"], &[]))]),
        )]),
        BTreeMap::from([(
            "murail-fix".to_owned(),
            BTreeMap::from([("engine".to_owned(), row(&["load"], &["load"]))]),
        )]),
    ] {
        assert_eq!(
            interface_policy_violations(&orders, &bad, &unseamed, None).len(),
            1,
            "{bad:?}"
        );
    }
}

#[test]
fn a_seamed_row_reads_only_the_floor_whole_and_names_its_debt_in_rows() {
    let orders = vec![StageOrder {
        crate_name: "murail-fix".to_owned(),
        stages: order(),
    }];
    let seams: BTreeMap<String, policy::StageSeams> = BTreeMap::from([(
        "murail-fix".to_owned(),
        toml::from_str("floor = ['load']").expect("fixture seams"),
    )]);
    let rows = |text: &str| {
        BTreeMap::from([(
            "murail-fix".to_owned(),
            toml::from_str::<BTreeMap<String, policy::StageReads>>(text).expect("fixture rows"),
        )])
    };
    // An input may owe a row for its interior.
    let good = rows(
        "engine = { input = ['schedule'], debt = { schedule = { items = 2, owner = 'murail-zhe09.29' } } }",
    );
    assert!(interface_policy_violations(&orders, &good, &seams, None).is_empty());
    for bad in [
        "engine = { reads = ['schedule'] }",
        "engine = { input = ['load'] }",
        "engine = { debt = ['schedule'] }",
        "engine = { input = ['schedule'], reads = [] , debt = { shedule = { items = 1, owner = 'murail-zhe09.29' } } }",
    ] {
        assert_eq!(
            interface_policy_violations(&orders, &rows(bad), &seams, None).len(),
            1,
            "{bad}"
        );
    }
    // A floor stage reads only the floor.
    let schedule_floor: BTreeMap<String, policy::StageSeams> = BTreeMap::from([(
        "murail-fix".to_owned(),
        toml::from_str("floor = ['schedule']").expect("fixture seams"),
    )]);
    let floor_reads = interface_policy_violations(
        &orders,
        &rows("schedule = { input = ['load'] }"),
        &schedule_floor,
        None,
    );
    assert_eq!(floor_reads.len(), 1, "{floor_reads:#?}");
    assert!(floor_reads[0].contains("a floor stage reads only the floor"));
    let unknown_floor = BTreeMap::from([(
        "murail-fix".to_owned(),
        toml::from_str::<policy::StageSeams>("floor = ['lod']").expect("fixture seams"),
    )]);
    assert_eq!(
        interface_policy_violations(&orders, &BTreeMap::new(), &unknown_floor, None).len(),
        1
    );
}

#[test]
fn a_forward_import_is_reported_with_its_site() {
    // The check can go red: the load stage reaching into the engine.
    let report = StageReport::of(
        &crate_map(&[
            (
                "load/cursor.rs",
                "use std::fmt;\nuse super::super::{\n    engine::Engine,\n};\n",
            ),
            (
                "load.rs",
                "#[cfg(test)]\nmod tests {\n    use crate::engine::Engine;\n}\n",
            ),
            ("engine.rs", "pub struct Engine;\n"),
        ]),
        &order(),
    );
    let sites: Vec<_> = report
        .forward
        .iter()
        .map(|edge| {
            (
                edge.file.as_str(),
                edge.line,
                edge.from.as_str(),
                edge.to.as_str(),
            )
        })
        .collect();
    // Test-only imports stay outside the production reading.
    assert_eq!(
        sites,
        vec![("crates/murail-fix/src/load/cursor.rs", 2, "load", "engine")]
    );
    assert_eq!(report.absent, vec!["schedule".to_owned()]);
}

#[test]
fn a_root_re_export_cannot_hide_a_forward_edge() {
    // `crate::Engine` resolves through lib.rs's `pub use` to the engine stage.
    let report = StageReport::of(
        &crate_map(&[
            (
                "lib.rs",
                "pub use engine::Engine;\nmod engine;\nmod load;\n",
            ),
            ("load.rs", "use crate::Engine;\n"),
            ("engine.rs", "pub struct Engine;\n"),
        ]),
        &order(),
    );
    assert_eq!(forward(&report), vec![("load", "engine")]);
}

#[test]
fn a_root_glob_is_reported_not_skipped() {
    let report = StageReport::of(
        &crate_map(&[
            ("load.rs", "use crate::*;\n"),
            ("schedule/plan.rs", "use super::super::*;\nuse super::*;\n"),
        ]),
        &order(),
    );
    let sites: Vec<_> = report
        .unjudged
        .iter()
        .map(|path| (path.file.as_str(), path.line, path.why))
        .collect();
    assert_eq!(
        sites,
        vec![
            ("crates/murail-fix/src/load.rs", 1, Unjudged::RootGlob),
            (
                "crates/murail-fix/src/schedule/plan.rs",
                1,
                Unjudged::RootGlob
            ),
        ]
    );
}

#[test]
fn a_forward_edge_through_an_unplaced_module_is_reported() {
    // load reads witness, which reads engine: load reaches engine in two hops.
    let report = StageReport::of(
        &crate_map(&[
            ("load.rs", "use crate::witness::Map;\n"),
            ("witness.rs", "use crate::engine::Engine;\n"),
            ("engine.rs", "use crate::witness::Map;\n"),
        ]),
        &order(),
    );
    assert!(report.forward.is_empty());
    let hops: Vec<_> = report
        .through_unplaced
        .iter()
        .map(|edge| {
            (
                edge.reader.from.as_str(),
                edge.via.join(" → "),
                edge.onward.to.as_str(),
            )
        })
        .collect();
    assert_eq!(hops, vec![("load", "witness".to_owned(), "engine")]);
}

// murail-7soto.4: unplaced → unplaced → later is followed to its end.
#[test]
fn a_forward_edge_through_an_unplaced_chain_is_reported() {
    let report = StageReport::of(
        &crate_map(&[
            ("load.rs", "use crate::witness::Map;\n"),
            ("witness.rs", "use crate::ledger::Row;\n"),
            ("ledger.rs", "use crate::engine::Engine;\n"),
            ("engine.rs", "pub struct Engine;\n"),
        ]),
        &order(),
    );
    let hops: Vec<_> = report
        .through_unplaced
        .iter()
        .map(|edge| {
            (
                edge.reader.from.as_str(),
                edge.via.join(" → "),
                edge.onward.to.as_str(),
            )
        })
        .collect();
    assert_eq!(
        hops,
        vec![("load", "witness → ledger".to_owned(), "engine")]
    );
}

// murail-7soto.4: `pub(crate) use alias::X as Y;` where `alias` is itself
// a root re-export of a module is followed to the module's stage.
#[test]
fn a_multi_step_root_alias_cannot_hide_a_forward_edge() {
    let report = StageReport::of(
        &crate_map(&[
            (
                "lib.rs",
                "mod engine;\nmod load;\npub(crate) use engine::program_map;\npub(crate) use program_map::ProgramMap as Chained;\npub(crate) use Chained as Twice;\n",
            ),
            ("load.rs", "use crate::Twice;\n"),
            ("engine/program_map.rs", "pub struct ProgramMap;\n"),
        ]),
        &order(),
    );
    assert_eq!(forward(&report), vec![("load", "engine")]);
    assert!(report.unjudged.is_empty());
}

// murail-7soto.2: an alias of the crate root carries its paths with it.
#[test]
fn a_crate_alias_cannot_hide_a_forward_edge() {
    let report = StageReport::of(
        &crate_map(&[
            (
                "lib.rs",
                "mod engine;\nmod load;\nmod schedule;\npub(crate) use crate as root;\n",
            ),
            (
                "load.rs",
                "use crate as c;\nfn run() { let _ = c::engine::Engine; }\n",
            ),
            (
                "schedule/plan.rs",
                "use super::super as up;\nuse up::engine::Engine;\nuse crate::root::engine::Engine;\n",
            ),
            ("engine.rs", "pub struct Engine;\n"),
        ]),
        &order(),
    );
    let sites: Vec<_> = report
        .forward
        .iter()
        .map(|edge| (edge.from.as_str(), edge.line, edge.path.as_str()))
        .collect();
    assert_eq!(
        sites,
        vec![
            ("load", 2, "c::engine::Engine"),
            ("schedule", 2, "up::engine::Engine"),
            ("schedule", 3, "crate::root::engine::Engine"),
        ]
    );
}

// murail-7soto.2: a root name a lib.rs glob may supply is unjudged.
#[test]
fn a_name_under_a_root_glob_re_export_is_unjudged() {
    let files = [
        (
            "lib.rs",
            "mod engine;\nmod load;\npub use engine::program::*;\npub use std::fmt::*;\npub fn helper() {}\n",
        ),
        ("load.rs", "use crate::Engine;\nuse crate::helper;\n"),
        ("engine/program.rs", "pub struct Engine;\n"),
    ];
    let report = StageReport::of(&crate_map(&files), &order());
    assert_eq!(
        unjudged(&report),
        vec![("crate::Engine", Unjudged::GlobbedRootName)]
    );
    // Without the crate-internal glob the name is unknown, still unjudged.
    let report = StageReport::of(
        &crate_map(&[
            (
                "lib.rs",
                "mod engine;\nmod load;\npub use std::fmt::*;\npub fn helper() {}\n",
            ),
            files[1],
            files[2],
        ]),
        &order(),
    );
    assert_eq!(
        unjudged(&report),
        vec![("crate::Engine", Unjudged::UnknownRootName)]
    );
}

// murail-7soto.5: a root re-export that reuses a module's name reads as
// the re-export when the path ends (a call), the module when it continues.
#[test]
fn a_re_export_named_like_a_module_resolves_by_namespace() {
    let report = StageReport::of(
        &crate_map(&[
            (
                "lib.rs",
                "mod engine;\nmod load;\nmod schedule;\npub use engine::compile::load;\n",
            ),
            ("load.rs", "pub struct Cursor;\n"),
            (
                "schedule.rs",
                "fn run() { let _ = crate::load::Cursor; crate::load(1); }\n",
            ),
            ("engine/compile.rs", "pub fn load(_: u8) {}\n"),
        ]),
        &order(),
    );
    let sites: Vec<_> = report
        .forward
        .iter()
        .map(|edge| (edge.from.as_str(), edge.to.as_str(), edge.path.as_str()))
        .collect();
    assert_eq!(sites, vec![("schedule", "engine", "crate::load")]);
    assert_eq!(report.judged, 2);
}

#[test]
fn a_use_of_a_re_export_named_like_a_module_imports_both() {
    let report = StageReport::of(
        &crate_map(&[
            (
                "lib.rs",
                "mod engine;\nmod load;\nmod schedule;\npub use engine::compile::load;\n",
            ),
            ("load.rs", ""),
            ("schedule.rs", "use crate::load;\n"),
            ("engine/compile.rs", "pub fn load() {}\n"),
        ]),
        &order(),
    );
    assert_eq!(forward(&report), vec![("schedule", "engine")]);
    assert_eq!(report.judged, 2);
}

#[test]
fn unnamed_modules_and_inline_root_modules_are_reported_test_files_are_not() {
    let report = StageReport::of(
            &crate_map(&[
                (
                    "lib.rs",
                    "mod load;\nmod extra;\nmod inline {\n    pub fn f() {}\n}\n#[cfg(test)]\nmod fixtures;\n#[cfg(test)]\nmod tests {\n}\n",
                ),
                ("load.rs", ""),
                ("extra.rs", ""),
                ("fixtures.rs", "mod nested;\n"),
                ("fixtures/nested.rs", "use crate::engine::Engine;\n"),
            ])
            .with_test_files(),
            &order(),
        );
    assert_eq!(report.unplaced, BTreeSet::from(["extra".to_owned()]));
    assert_eq!(report.inline_modules, BTreeSet::from(["inline".to_owned()]));
    assert!(report.forward.is_empty() && report.unjudged.is_empty());
}

#[test]
fn two_globs_in_one_use_tree_stay_two_bindings() {
    let root = root_of(&[
        (
            "lib.rs",
            "mod engine;\nmod load;\npub use {engine::*, load::*, std::fmt::*};\n",
        ),
        ("load.rs", ""),
        ("engine.rs", ""),
    ]);
    assert_eq!(
        root.globs,
        vec!["engine::*".to_owned(), "load::*".to_owned()]
    );
}

const SHARED_LIB: &str = "mod engine;\nmod load;\nmod schedule;\npub use engine::compile::load;\n";

// The critic's canonical case: a braced `self` imports only the module.
#[test]
fn a_braced_self_imports_only_the_module() {
    let report = StageReport::of(
        &crate_map(&[
            ("lib.rs", SHARED_LIB),
            ("load.rs", ""),
            ("schedule.rs", "use crate::load::{self};\n"),
            ("engine/compile.rs", "pub fn load() {}\n"),
        ]),
        &order(),
    );
    assert!(report.forward.is_empty());
    assert_eq!(report.judged, 1);
}

#[test]
fn a_shared_name_refusal_names_the_braced_self_remedy() {
    let report = StageReport::of(
        &crate_map(&[
            ("lib.rs", SHARED_LIB),
            ("load.rs", ""),
            ("schedule.rs", "use crate::load;\n"),
            ("engine/compile.rs", "pub fn load() {}\n"),
        ]),
        &order(),
    );
    let refusals = report.refusals("murail-fix");
    assert_eq!(refusals.len(), 1);
    assert!(refusals[0].contains("amend the stage_order row if the order is wrong"));
    assert!(refusals[0].contains("write `use crate::load::{self};`"));
}

#[test]
fn a_test_gated_item_is_exempt_and_an_ungated_one_is_refused() {
    let gated = "#[cfg(test)]\nuse crate::engine::Engine;\npub struct Cursor;\nimpl Cursor {\n    #[cfg(test)]\n    fn probe() {\n        crate::engine::run();\n    }\n}\n#[cfg(any(test, feature = \"strategies\"))]\nuse crate::engine::Strategy;\n";
    let report = StageReport::of(
        &crate_map(&[("load.rs", gated), ("engine.rs", "")]),
        &order(),
    );
    // `any(test, feature)` also compiles for the feature: production.
    let sites: Vec<_> = report.forward.iter().map(|edge| edge.line).collect();
    assert_eq!(sites, vec![11]);
    let report = StageReport::of(
        &crate_map(&[
            ("load.rs", "use crate::engine::Engine;\n"),
            ("engine.rs", ""),
        ]),
        &order(),
    );
    assert_eq!(forward(&report), vec![("load", "engine")]);
}

#[test]
fn a_lib_type_alias_or_item_body_cannot_launder_an_edge() {
    let lib = "mod engine;\nmod load;\nmod schedule;\npub type Alias = engine::Engine;\npub type Boxed = Box<engine::Engine>;\npub fn helper() -> u8 {\n    engine::run()\n}\npub const LIMIT: usize = engine::LIMIT;\npub static TABLE: [u8; 1] = [schedule::ROW];\npub fn quiet() {}\n";
    let report = StageReport::of(
        &crate_map(&[
            ("lib.rs", lib),
            (
                "load.rs",
                "use crate::Alias;\nuse crate::Boxed;\nfn f() { crate::helper(); crate::quiet(); }\nconst L: usize = crate::LIMIT;\nuse crate::TABLE;\n",
            ),
            ("schedule.rs", ""),
            ("engine.rs", ""),
        ]),
        &order(),
    );
    let sites: Vec<_> = report
        .forward
        .iter()
        .map(|edge| (edge.path.as_str(), edge.to.as_str()))
        .collect();
    assert_eq!(
        sites,
        vec![
            ("crate::Alias", "engine"),
            ("crate::Boxed", "engine"),
            ("crate::helper", "engine"),
            ("crate::LIMIT", "engine"),
            ("crate::TABLE", "schedule"),
        ]
    );
    assert!(report.unjudged.is_empty());
}

#[test]
fn a_lib_type_or_impl_cannot_launder_an_edge() {
    let lib = "mod engine;\nmod load;\nmod schedule;\npub struct W(pub engine::Engine);\npub trait T {\n    fn e(&self) -> &schedule::Plan;\n}\npub struct V;\nimpl V {\n    pub fn new() -> engine::Engine {\n        todo!()\n    }\n}\npub struct A(pub Box<B>);\npub struct B(pub Option<A>);\npub enum Quiet {\n    One,\n}\n";
    let report = StageReport::of(
        &crate_map(&[
            ("lib.rs", lib),
            (
                "load.rs",
                "use crate::W;\nuse crate::T;\nuse crate::V;\nuse crate::A;\nuse crate::Quiet;\n",
            ),
            ("schedule.rs", ""),
            ("engine.rs", ""),
        ]),
        &order(),
    );
    let sites: Vec<_> = report
        .forward
        .iter()
        .map(|edge| (edge.path.as_str(), edge.to.as_str()))
        .collect();
    assert_eq!(
        sites,
        vec![
            ("crate::W", "engine"),
            ("crate::T", "schedule"),
            ("crate::V", "engine"),
        ]
    );
    // Mutually naming items end the walk; they are judged, not unjudged.
    assert!(report.unjudged.is_empty());
}

#[test]
fn an_exported_macro_ranks_by_its_defining_module() {
    let report = StageReport::of(
        &crate_map(&[
            (
                "lib.rs",
                "mod engine;\nmod load;\nmacro_rules! local {\n    () => {};\n}\n",
            ),
            (
                "load.rs",
                "fn f() {\n    crate::knock!();\n    crate::local!();\n}\n",
            ),
            (
                "engine/macros.rs",
                "#[macro_export]\nmacro_rules! knock {\n    () => {};\n}\n",
            ),
        ]),
        &order(),
    );
    assert_eq!(forward(&report), vec![("load", "engine")]);
    assert!(report.unjudged.is_empty());
}

#[test]
fn each_unjudged_refusal_names_its_own_remedy() {
    let report = StageReport::of(
        &crate_map(&[
            (
                "lib.rs",
                "mod engine;\nmod load;\npub use engine::program::*;\n",
            ),
            ("load.rs", "use crate::Engine;\nuse crate::*;\n"),
            ("engine/program.rs", ""),
        ]),
        &order(),
    );
    let refusals = report.refusals("murail-fix");
    assert_eq!(refusals.len(), 2);
    assert!(refusals[0].contains("replace the lib.rs glob with named re-exports"));
    assert!(refusals[1].contains("import each name by its true path"));
}

impl CrateMap {
    fn with_test_files(mut self) -> Self {
        let flags = crate::discovery::classify_test_fixture(
            self.files
                .iter()
                .map(|file| (file.rel.clone(), std::sync::Arc::clone(&file.source)))
                .collect(),
        );
        for (file, test_only) in self.files.iter_mut().zip(flags) {
            file.test_file = test_only;
        }
        self
    }
}
#[test]
fn imported_module_and_value_names_are_distinct_when_declarations_prove_it() {
    let map = crate_map(&[
        (
            "lib.rs",
            "pub use crate::load::choose as Shared;\npub use crate::engine::shared as Shared;\n",
        ),
        ("load.rs", "pub fn choose() {}\n"),
        ("engine/shared.rs", "pub fn work() {}\n"),
    ]);
    let root = RootTable::of(&map, TestSelection::Production);
    assert_eq!(
        root.read(&["Shared"], PathForm::Inline, 0).unwrap(),
        [vec!["load".to_owned(), "choose".to_owned()]]
    );
    assert_eq!(
        root.read(&["Shared", "work"], PathForm::Inline, 0).unwrap(),
        [vec![
            "engine".to_owned(),
            "shared".to_owned(),
            "work".to_owned()
        ]]
    );
    let imports = root.read(&["Shared"], PathForm::Use, 0).unwrap();
    assert_eq!(imports.len(), 2);
    assert!(imports.iter().any(|path| path == &["load", "choose"]));
    assert!(imports.iter().any(|path| path == &["engine", "shared"]));
}
#[test]
fn external_or_chained_aliases_do_not_invent_a_value_namespace() {
    for value in ["external::choose", "crate::Alias"] {
        let lib = format!(
            "pub use {value} as Shared;\npub use crate::engine::shared as Shared;\npub use crate::load::choose as Alias;\n"
        );
        let root = root_of(&[
            ("lib.rs", &lib),
            ("load.rs", "pub fn choose() {}\n"),
            ("engine/shared.rs", "pub fn work() {}\n"),
        ]);
        assert_eq!(
            root.read(&["Shared"], PathForm::Inline, 0).unwrap_err(),
            Unjudged::AmbiguousRootName
        );
    }
}
#[test]
fn a_test_gated_value_proves_only_the_test_population_namespace() {
    let map = crate_map(&[
        (
            "lib.rs",
            "pub use crate::load::choose as Shared;\npub use crate::engine::shared as Shared;\n",
        ),
        ("load.rs", "#[cfg(test)]\npub fn choose() {}\n"),
        ("engine/shared.rs", "pub fn work() {}\n"),
    ]);
    assert_eq!(
        RootTable::of(&map, TestSelection::Production)
            .read(&["Shared"], PathForm::Inline, 0)
            .unwrap_err(),
        Unjudged::AmbiguousRootName
    );
    assert!(
        RootTable::of(&map, TestSelection::All)
            .read(&["Shared"], PathForm::Inline, 0)
            .is_ok()
    );
}
#[test]
fn an_import_that_already_names_both_namespaces_cannot_prove_a_pair() {
    let root = root_of(&[
        (
            "lib.rs",
            "pub use crate::engine::shared as Shared;\npub use crate::load::choose as Shared;\n",
        ),
        ("engine/mod.rs", "pub fn shared() {}\n"),
        ("engine/shared.rs", "pub fn work() {}\n"),
        ("load.rs", "pub fn choose() {}\n"),
    ]);
    assert_eq!(
        root.read(&["Shared"], PathForm::Inline, 0).unwrap_err(),
        Unjudged::AmbiguousRootName
    );
}
