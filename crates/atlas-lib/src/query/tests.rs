//! The engine against brute force on small generated graphs, and each kind
//! of refusal with where it points.
use super::*;
use crate::relations::{Relation, schema};
use std::collections::{BTreeMap, BTreeSet};

/// A deterministic generator, so a failing seed can be replayed.
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 33
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
}

fn node(n: u64) -> String {
    format!("src/m{}.rs:1:{n}", n % 3)
}

/// A random directed graph as `function` and `call` rows: node ids, and
/// edges with a witness id each (an edge may repeat with a new witness).
fn graph(seed: u64) -> (Relations, u64, Vec<(u64, u64)>) {
    let mut rng = Lcg(seed);
    let nodes = 2 + rng.below(12);
    let edges = rng.below(nodes * 3);
    let mut function = Relation {
        schema: schema::relation("function").unwrap(),
        rows: Vec::new(),
    };
    for n in 0..nodes {
        function.rows.push(vec![
            Cell::Text(node(n)),
            Cell::Text(format!("src/m{}.rs", n % 3)),
            Cell::Integer(1),
            Cell::Text(format!("f{n}")),
            Cell::Text(format!("crate::m{}::f{n}", n % 3)),
            Cell::Text(String::new()),
            Cell::Boolean(n % 2 == 0),
            Cell::Boolean(false),
            Cell::Text(String::new()),
        ]);
    }
    let mut call = Relation {
        schema: schema::relation("call").unwrap(),
        rows: Vec::new(),
    };
    let mut pairs = Vec::new();
    for e in 0..edges {
        let (a, b) = (rng.below(nodes), rng.below(nodes));
        pairs.push((a, b));
        call.rows.push(vec![
            Cell::Text(format!("call:{e}")),
            Cell::Text(node(a)),
            Cell::Text(node(b)),
            Cell::Text("src/m.rs".into()),
            Cell::Integer(e),
            Cell::Integer(1),
        ]);
    }
    let relations = Relations {
        schema: schema::VERSION,
        families: vec![Family::Qualified],
        limitations: Vec::new(),
        relations: vec![function, call],
    };
    (relations, nodes, pairs)
}

fn ask(source: &str, relations: &Relations) -> Result<Answer, Error> {
    let program = parse(source)?;
    let checked = check(&program, &relations.families)?;
    run(&checked, relations, Limits::default())
}

fn pairs_of(table: &Table) -> BTreeSet<(String, String)> {
    table
        .rows
        .iter()
        .map(|row| {
            (
                row[0].text().unwrap_or_default().to_owned(),
                row[1].text().unwrap_or_default().to_owned(),
            )
        })
        .collect()
}

fn closure(nodes: u64, edges: &[(u64, u64)]) -> BTreeSet<(u64, u64)> {
    let mut reach = BTreeSet::new();
    for start in 0..nodes {
        let mut seen = BTreeSet::new();
        let mut todo = vec![start];
        while let Some(n) = todo.pop() {
            for &(a, b) in edges {
                if a == n && seen.insert(b) {
                    todo.push(b);
                }
            }
        }
        reach.extend(seen.into_iter().map(|b| (start, b)));
    }
    reach
}

const LINEAR: &str = "
reach{from: A, to: B} :- call{from: A, to: B}.
reach{from: A, to: C} :- reach{from: A, to: B}, call{from: B, to: C}.
";
const DOUBLING: &str = "
reach{from: A, to: B} :- call{from: A, to: B}.
reach{from: A, to: C} :- reach{from: A, to: B}, reach{from: B, to: C}.
";

#[test]
fn recursion_matches_a_breadth_first_closure() {
    for seed in 0..60 {
        let (relations, nodes, edges) = graph(seed);
        let expected: BTreeSet<(String, String)> = closure(nodes, &edges)
            .into_iter()
            .map(|(a, b)| (node(a), node(b)))
            .collect();
        for rules in [LINEAR, DOUBLING] {
            let answer = ask(&format!("{rules}?- reach{{from: A, to: B}}."), &relations).unwrap();
            assert_eq!(
                pairs_of(&answer.tables[0]),
                expected,
                "seed {seed}\n{rules}"
            );
        }
    }
}

#[test]
fn negation_matches_the_complement_of_the_closure() {
    for seed in 0..60 {
        let (relations, nodes, edges) = graph(seed);
        let reach = closure(nodes, &edges);
        let expected: BTreeSet<(String, String)> = (0..nodes)
            .flat_map(|a| (0..nodes).map(move |b| (a, b)))
            .filter(|p| !reach.contains(p))
            .map(|(a, b)| (node(a), node(b)))
            .collect();
        let source = format!(
            "{LINEAR}unreached{{from: A, to: B}} :- function{{id: A}}, function{{id: B}}, !reach{{from: A, to: B}}.\n?- unreached{{from: A, to: B}}."
        );
        let answer = ask(&source, &relations).unwrap();
        assert_eq!(pairs_of(&answer.tables[0]), expected, "seed {seed}");
    }
}

#[test]
fn count_matches_distinct_bindings_per_group() {
    for seed in 0..60 {
        let (relations, _, edges) = graph(seed);
        let mut targets: BTreeMap<String, BTreeSet<u64>> = BTreeMap::new();
        let mut witnesses: BTreeMap<String, u64> = BTreeMap::new();
        for &(a, b) in &edges {
            targets.entry(node(a)).or_default().insert(b);
            *witnesses.entry(node(a)).or_default() += 1;
        }
        let answer = ask(
            "out{node: A, targets: count(B)} :- call{from: A, to: B}.
             calls{node: A, n: count(C)} :- call{id: C, from: A}.
             ?- out{node: A, targets: T}.
             ?- calls{node: A, n: N}.",
            &relations,
        )
        .unwrap();
        let got: BTreeMap<String, u64> = answer.tables[0]
            .rows
            .iter()
            .map(|r| (r[0].text().unwrap().to_owned(), r[1].integer().unwrap()))
            .collect();
        let expected: BTreeMap<String, u64> = targets
            .into_iter()
            .map(|(k, v)| (k, u64::try_from(v.len()).unwrap()))
            .collect();
        assert_eq!(got, expected, "seed {seed}");
        let got: BTreeMap<String, u64> = answer.tables[1]
            .rows
            .iter()
            .map(|r| (r[0].text().unwrap().to_owned(), r[1].integer().unwrap()))
            .collect();
        assert_eq!(got, witnesses, "seed {seed}");
    }
}

#[test]
fn comparisons_tests_order_and_limit() {
    let (relations, nodes, _) = graph(7);
    let answer = ask(
        r#"?- function{id: F, path: P, name: N, qualified: Q, public: true},
              under(P, "src"), last_segment(Q, S), S = N, starts_with(N, "f")
              order by N desc limit 2."#,
        &relations,
    )
    .unwrap();
    let table = &answer.tables[0];
    assert_eq!(table.columns, ["F", "P", "N", "Q", "S"]);
    let public = nodes.div_ceil(2);
    assert_eq!(table.total, usize::try_from(public).unwrap());
    assert_eq!(table.rows.len(), 2.min(table.total));
    let names: Vec<&str> = table.rows.iter().map(|r| r[2].text().unwrap()).collect();
    let mut sorted = names.clone();
    sorted.sort_unstable_by(|a, b| b.cmp(a));
    assert_eq!(names, sorted);
    let answer = ask(
        "?- call{from: A, line: L}, L >= 2, L < 4, A != \"x\".",
        &relations,
    )
    .unwrap();
    assert!(answer.tables[0].rows.iter().all(|r| {
        let l = r[1].integer().unwrap();
        (2..4).contains(&l)
    }));
}

#[test]
fn facts_and_constants_in_heads() {
    let (relations, _, _) = graph(3);
    let answer = ask(
        r#"scope{dir: "src"}.
           scope{dir: "tests"}.
           tagged{id: F, tag: "public"} :- function{id: F, public: true}.
           ?- scope{dir: D}.
           ?- tagged{id: _F, tag: T}."#,
        &relations,
    )
    .unwrap();
    assert_eq!(answer.tables[0].rows.len(), 2);
    assert_eq!(answer.tables[1].columns, ["T"]);
    assert_eq!(
        answer.tables[1].rows,
        vec![vec![Cell::Text("public".into())]]
    );
}

fn refusal(source: &str) -> Error {
    let (relations, _, _) = graph(1);
    ask(source, &relations).unwrap_err()
}

#[test]
fn refusals_say_what_and_where() {
    let cases: &[(&str, ErrorKind, &str, (usize, usize))] = &[
        (
            "?- cal{from: A}.",
            ErrorKind::Schema,
            "no relation `cal`; closest: `call`",
            (1, 4),
        ),
        (
            "?- call{form: A}.",
            ErrorKind::Schema,
            "has no field `form`; its fields, closest first: `from`",
            (1, 9),
        ),
        (
            "?- call{line: \"7\"}.",
            ErrorKind::Schema,
            "holds a number, given text",
            (1, 15),
        ),
        (
            "?- call{from: A, line: A}.",
            ErrorKind::Schema,
            "variable `A` is text in `call.from` but a number in `call.line`",
            (1, 18),
        ),
        (
            "x{a: A} :- call{from: A, to: B}.\n?- x{a: A}.",
            ErrorKind::Safety,
            "`B` appears only once",
            (1, 30),
        ),
        (
            "?- call{from: A}, !call{to: B}.",
            ErrorKind::Safety,
            "`B` is not bound: a negation reads it",
            (1, 29),
        ),
        (
            "?- call{from: A}, B > 3.",
            ErrorKind::Safety,
            "`B` is not bound",
            (1, 19),
        ),
        (
            "x{a: A} :- call{from: A}, !y{a: A}.\ny{a: A} :- call{from: A}, !x{a: A}.\n?- x{a: A}.",
            ErrorKind::Stratification,
            "depends on itself through a negation",
            (0, 0),
        ),
        (
            "call{from: \"a\"}.\n?- call{from: A}.",
            ErrorKind::Schema,
            "is a stored relation",
            (1, 1),
        ),
        (
            "x{a: A} :- call{from: A}.\nx{b: A} :- call{from: A}.\n?- x{a: A}.",
            ErrorKind::Schema,
            "gives the same fields",
            (2, 1),
        ),
        (
            "x{a: A} :- call{from: A}.",
            ErrorKind::Syntax,
            "has no query",
            (0, 0),
        ),
    ];
    for (source, kind, message, (line, column)) in cases {
        let error = refusal(source);
        assert_eq!(error.kind, *kind, "{source}: {error}");
        assert!(error.message.contains(message), "{source}: {error}");
        if *line > 0 {
            assert_eq!(
                error.location,
                Some(Location {
                    line: *line,
                    column: *column
                }),
                "{source}: {error}"
            );
        }
    }
}

#[test]
fn families_name_what_a_program_reads() {
    let program = parse("?- call{from: A}, mention{source: A}.").unwrap();
    let checked = check(&program, &Family::ALL).unwrap();
    assert_eq!(checked.families(), [Family::Lexical, Family::Qualified]);
    assert_eq!(checked.reads().collect::<Vec<_>>(), ["mention", "call"]);
    let error = check(&program, &[Family::Qualified]).unwrap_err();
    assert!(error.message.contains("lexical family"), "{error}");
}

#[test]
fn only_the_rules_a_query_needs_are_kept() {
    let program = parse(
        "a{x: X} :- call{from: X}.
         b{x: X} :- mention{source: X}.
         ?- a{x: X}.",
    )
    .unwrap();
    let checked = check(&program, &Family::ALL).unwrap();
    assert_eq!(checked.families(), [Family::Qualified]);
}
