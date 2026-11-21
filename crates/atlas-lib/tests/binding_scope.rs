//! A local binding types a receiver only inside its lexical scope: the
//! block, closure body, match arm or conditional branch it is bound in.
//! The closure, async, alias and initializer cases are the 2026-11-21
//! review's fixture.
mod support;
use atlas_lib::{analysis::Analysis, discovery};
use std::collections::BTreeMap;
use support::Fixture;

const LIB: &str = "struct A;
impl A { fn run(&self) {} }
struct B;
impl B { fn run(&self) {} }

pub fn closure_scope() {
    let _closure = |receiver: B| receiver.run();
    {
        static RECEIVER: A = A;
        use RECEIVER as receiver;
        receiver.run();
    }
}

pub async fn async_scope() {
    let _future = async { let receiver: B = B; receiver.run(); };
    {
        static RECEIVER: A = A;
        use RECEIVER as receiver;
        receiver.run();
    }
}

pub fn branches(flag: Option<B>) {
    if let Some(receiver) = flag {
        let typed: B = receiver;
        typed.run();
    } else {
        typed.run();
    }
    match 1 {
        0 => { let other: A = A; other.run(); }
        _ => other.run(),
    }
    {
        let first: A = A;
        first.run();
    }
    {
        let first: B = B;
        first.run();
    }
}

pub fn block_type_shadow() {
    type A = B;
    let value: A = B;
    value.run();
}

fn make() -> A { A }
pub fn initializer_shadow() {
    fn make() -> B { B }
    let value = make();
    value.run();
}
pub fn value_initializer() {
    let make = || B;
    let value = make();
    value.run();
}
";

/// Each call by line: its target's name, or `?` and why not.
fn calls() -> BTreeMap<usize, String> {
    let f = Fixture::new(&[
        (
            "Cargo.toml",
            "[package]\nname=\"scope\"\nversion=\"0.1.0\"\nedition=\"2024\"\n",
        ),
        ("src/lib.rs", LIB),
    ]);
    let metadata = cargo_metadata::MetadataCommand::new()
        .current_dir(f.path())
        .no_deps()
        .exec()
        .expect("metadata");
    let snapshot = discovery::snapshot_metadata(&metadata).expect("snapshot");
    let facts = Analysis::new(&snapshot)
        .expect("analysis")
        .representations()
        .expect("facts");
    let names: BTreeMap<_, _> = facts
        .functions
        .iter()
        .map(|f| (f.id.as_str(), f.name.as_str()))
        .collect();
    let mut out = BTreeMap::new();
    for call in &facts.calls {
        out.insert(call.site.line, names[call.to.as_str()].to_owned());
    }
    for unknown in &facts.unknown_calls {
        out.insert(unknown.call.site.line, format!("? {}", unknown.reason));
    }
    out
}

#[test]
fn a_binding_types_a_receiver_only_inside_its_scope() {
    let calls = calls();
    let at = |line: usize| calls.get(&line).map_or("absent", String::as_str);
    // Inside the closure and the async block: their own binding.
    assert_eq!(at(7), "B::run", "{calls:#?}");
    assert_eq!(at(16), "B::run", "{calls:#?}");
    // After them, a block of its own: the closure's and async block's
    // bindings are gone, and the `use` there is not a typed binding.
    assert!(at(11).starts_with("? "), "{calls:#?}");
    assert!(at(20).starts_with("? "), "{calls:#?}");
    // The then branch's binding is not the else branch's; one arm's is not
    // the next arm's.
    assert_eq!(at(27), "B::run", "{calls:#?}");
    assert!(at(29).starts_with("? "), "{calls:#?}");
    assert_eq!(at(32), "A::run", "{calls:#?}");
    assert!(at(33).starts_with("? "), "{calls:#?}");
    // Two blocks binding one name: each call sees only its own.
    assert_eq!(at(37), "A::run", "{calls:#?}");
    assert_eq!(at(41), "B::run", "{calls:#?}");
    // A body-local alias, function or value names the receiver's type, not
    // the module's item of that name: never the module's edge.
    for line in [48, 55, 60] {
        assert!(
            at(line).starts_with("? receiver value"),
            "{line}: {calls:#?}"
        );
        assert!(
            at(line).contains("shadows the module's"),
            "{line}: {calls:#?}"
        );
    }
}
