//! A closure's body is part of the function that writes it: its calls are
//! that function's calls, resolved with the function's bindings and the
//! closure's typed parameters, and unknown (never missing) otherwise.
mod support;
use std::process::Command;
use support::Fixture;

const LIB: &str = "pub struct Prices;
impl Prices {
    pub fn bound(&self) -> u32 { 1 }
    pub fn least(&self) -> u32 { 2 }
}
pub fn helper() -> u32 { 3 }
pub fn direct(prices: &Prices) -> u32 {
    let f = |x: u32| prices.bound() + x + helper();
    f(1)
}
pub fn mapped() -> u32 {
    (0..3).map(|i| helper() + i).sum::<u32>()
}
pub fn nested() -> u32 {
    let outer = || (|| helper())();
    outer()
}
pub fn typed(all: &[Prices]) -> u32 {
    all.iter().map(|p: &Prices| p.least()).sum::<u32>()
}
pub fn untyped(all: &[Prices]) -> u32 {
    all.iter().map(|p| p.bound()).sum::<u32>()
}
#[derive(Default)]
pub struct Graph {
    pub n: u8,
}
pub fn builds() -> Vec<Graph> {
    let literal = || Graph { n: 1 };
    let called = || Graph::default();
    vec![literal(), called()]
}
pub fn later(prices: &Prices) -> impl std::future::Future<Output = (u32, Graph)> + '_ {
    async move { (prices.bound() + helper(), Graph { n: 2 }) }
}
";

fn json(f: &Fixture, args: &[&str]) -> serde_json::Value {
    let output = Command::new(env!("CARGO_BIN_EXE_cargo-atlas"))
        .current_dir(f.path())
        .env("ATLAS_CACHE", "off")
        .env("ATLAS_NOTICES", "off")
        .args(args)
        .args(["--as", "json"])
        .output()
        .expect("run Atlas");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("JSON")
}
/// The calling functions of a resolved row list, by name.
fn names(rows: &serde_json::Value, field: &str) -> Vec<String> {
    let mut names: Vec<String> = rows
        .as_array()
        .expect("rows")
        .iter()
        .map(|row| row[field].as_str().expect("name").to_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn calls_inside_closures_belong_to_the_enclosing_function() {
    let f = Fixture::new(&[("src/lib.rs", LIB)]);
    // A free function called from a direct, a mapped and a nested closure.
    let helper = json(&f, &["callers", "helper"]);
    assert_eq!(
        names(&helper["answers"][0]["resolved"], "from_name"),
        ["direct", "later", "mapped", "nested"]
    );
    // A method on the enclosing function's typed parameter, and on the
    // closure's own typed parameter.
    let bound = json(&f, &["callers", "Prices::bound"]);
    assert_eq!(
        names(&bound["answers"][0]["resolved"], "from_name"),
        ["direct", "later"]
    );
    let least = json(&f, &["callers", "Prices::least"]);
    assert_eq!(
        names(&least["answers"][0]["resolved"], "from_name"),
        ["typed"]
    );
    // An untyped closure parameter is not guessed: its call is unknown,
    // recorded with its kind, and so a possible caller.
    let possibly = &bound["answers"][0]["possibly"];
    assert_eq!(possibly.as_array().map(Vec::len), Some(1), "{possibly}");
    assert_eq!(possibly[0]["spelling"], "bound");
    assert_eq!(possibly[0]["kind"], "receiver-initializer-unknown");
    // The closure value's own call (`f(1)`, `outer()`) is a value call, not a
    // call of a function named f.
    let direct = json(&f, &["callees", "direct"]);
    let kinds = &direct["answers"][0]["unresolved_by_kind"];
    assert_eq!(kinds["value-call"], 1, "{kinds}");
}

/// A literal and a `default()` call inside a closure are both construction
/// sites of the function that writes the closure.
#[test]
fn constructions_inside_closures_belong_to_the_enclosing_function() {
    let f = Fixture::new(&[("src/lib.rs", LIB)]);
    let types = json(&f, &["types", "Graph"]);
    let text = types.to_string();
    let facts = &types["facts"];
    let literal = facts["constructors"]
        .as_array()
        .expect("constructors")
        .iter()
        .filter(|c| {
            c["from"]
                .as_str()
                .is_some_and(|f| f.starts_with("src/lib.rs:28:"))
        })
        .count();
    let called = facts["default_calls"]
        .as_array()
        .expect("default calls")
        .iter()
        .filter(|c| {
            c["from"]
                .as_str()
                .is_some_and(|f| f.starts_with("src/lib.rs:28:"))
        })
        .count();
    assert_eq!((literal, called), (1, 1), "{text}");
}

/// An `async` block's body is part of the function that writes it, as a
/// closure's is: its calls and literals are that function's.
#[test]
fn calls_and_constructions_inside_async_blocks_belong_to_the_enclosing_function() {
    let f = Fixture::new(&[("src/lib.rs", LIB)]);
    let helper = json(&f, &["callers", "helper"]);
    assert!(
        names(&helper["answers"][0]["resolved"], "from_name").contains(&"later".to_owned()),
        "{helper}"
    );
    let bound = json(&f, &["callers", "Prices::bound"]);
    assert!(
        names(&bound["answers"][0]["resolved"], "from_name").contains(&"later".to_owned()),
        "{bound}"
    );
    let types = json(&f, &["types", "Graph"]);
    let later = types["facts"]["functions"]
        .as_array()
        .expect("functions")
        .iter()
        .find(|function| function["name"] == "later")
        .and_then(|function| function["id"].as_str())
        .expect("later")
        .to_owned();
    assert!(
        types["facts"]["constructors"]
            .as_array()
            .expect("constructors")
            .iter()
            .any(|c| c["from"] == later.as_str()),
        "{types}"
    );
}
