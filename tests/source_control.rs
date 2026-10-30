//! Executable source witnesses qualify the structural observations, not feasibility inference.
mod support;
use atlas::{diagram::Diagram, discovery, workflow};
use std::collections::BTreeSet;
use support::Fixture;

fn inspect(files: &[(&str, &str)], function: &str, depth: u8) -> (Fixture, Diagram) {
    let f = Fixture::new(files);
    let m = cargo_metadata::MetadataCommand::new()
        .current_dir(&f.0)
        .no_deps()
        .exec()
        .expect("test fixture prerequisite must succeed");
    let graph = discovery::scan_metadata(&m).expect("test fixture prerequisite must succeed");
    let (_, d) = workflow::expanded(&f.0, &m, &graph, "src/lib.rs", function, depth)
        .expect("test fixture prerequisite must succeed");
    d.validate()
        .expect("test fixture prerequisite must succeed");
    (f, d)
}
fn reachable(d: &Diagram, from: &str, to: &str) -> bool {
    let mut todo = vec![from.to_owned()];
    let mut seen = BTreeSet::new();
    while let Some(n) = todo.pop() {
        if n == to {
            return true;
        }
        if seen.insert(n.clone()) {
            todo.extend(d.edges.iter().filter(|e| e.from == n).map(|e| e.to.clone()));
        }
    }
    false
}
const CONTROL: &str = r#"
use std::sync::atomic::{AtomicUsize,Ordering};
static GUARDS:AtomicUsize=AtomicUsize::new(0);
fn g()->Result<usize,()>{Err(())}
fn h()->usize{panic!("h must not run after g's residual")}
fn combine(_:usize,_:usize){panic!("combine must not run after g's residual")}
fn nested()->Result<(),()>{combine(g()?,h());Ok(())}
fn predicate(_:usize)->bool{GUARDS.fetch_add(1,Ordering::SeqCst);true}
fn guarded(x:Option<usize>)->usize{match x{Some(v) if predicate(v)=>v,_=>0}}
fn main(){assert_eq!(nested(),Err(()));assert_eq!(guarded(None),0);assert_eq!(GUARDS.load(Ordering::SeqCst),0);}
"#;
#[test]
fn nested_residual_and_pattern_failure_skip_their_following_work() {
    let (f, d) = inspect(&[("src/lib.rs", CONTROL)], "nested", 1);
    let binary = f.0.join("control");
    let compile = std::process::Command::new("rustc")
        .args(["--edition=2024"])
        .arg(f.0.join("src/lib.rs"))
        .arg("-o")
        .arg(&binary)
        .output()
        .unwrap();
    assert!(
        compile.status.success(),
        "{}",
        String::from_utf8_lossy(&compile.stderr)
    );
    assert!(
        std::process::Command::new(binary)
            .status()
            .unwrap()
            .success()
    );
    let residual = d
        .nodes
        .iter()
        .find(|n| n.label == "? returns early (residual)")
        .unwrap();
    for skipped in d
        .nodes
        .iter()
        .filter(|n| n.label.starts_with("h()") || n.label == "combine(g()?,h())")
    {
        assert!(!reachable(&d, &residual.id, &skipped.id));
    }
    let root = d.nodes.iter().find(|n| n.label == "fn nested").unwrap();
    let h = d.nodes.iter().find(|n| n.label.starts_with("h()")).unwrap();
    assert!(
        reachable(&d, &root.id, &h.id),
        "success branch remains a source possibility; no constant-result inference"
    );
    let (_, d) = inspect(&[("src/lib.rs", CONTROL)], "guarded", 1);
    let pattern = d
        .nodes
        .iter()
        .find(|n| n.label == "pattern: Some(v)")
        .unwrap();
    let failed = d
        .edges
        .iter()
        .find(|e| e.from == pattern.id && e.label.as_deref() == Some("does not match"))
        .unwrap();
    assert!(
        d.nodes.iter().any(|n| n.label == "fn predicate"),
        "guard call resolves through the shared index"
    );
    let guard = d
        .nodes
        .iter()
        .find(|n| n.label.starts_with("predicate(v)"))
        .unwrap();
    assert!(!reachable(&d, &failed.to, &guard.id));
    let result = d.nodes.iter().find(|n| n.label == "guard result").unwrap();
    assert!(
        d.edges.iter().any(|e| e.from == result.id
            && e.to == failed.to
            && e.label.as_deref() == Some("false"))
    );
}
#[test]
fn repeated_cross_file_calls_have_distinct_matched_returns() {
    let (_, d) = inspect(
        &[
            (
                "src/lib.rs",
                "mod child; pub fn run(){child::work();child::work();}",
            ),
            ("src/child.rs", "pub fn work(){}"),
        ],
        "run",
        1,
    );
    let returns: Vec<_> = d
        .edges
        .iter()
        .filter(|e| {
            e.label
                .as_deref()
                .is_some_and(|s| s.starts_with("return to"))
        })
        .collect();
    assert_eq!(returns.len(), 2);
    assert_ne!(returns[0].from, returns[1].from);
    assert_ne!(returns[0].to, returns[1].to);
    for edge in returns {
        let exit = d.nodes.iter().find(|n| n.id == edge.from).unwrap();
        assert_eq!(exit.group, "src/child.rs");
        let continuation = d.nodes.iter().find(|n| n.id == edge.to).unwrap();
        assert_eq!(continuation.group, "src/lib.rs");
        assert_eq!(d.edges.iter().filter(|e| e.from == edge.from).count(), 1);
    }
}
#[test]
fn loop_forms_preserve_exhaustion_condition_rechecks_and_absence_of_infinite_exit() {
    let (_, d) = inspect(
        &[(
            "src/lib.rs",
            "pub fn run(){loop {continue;} after();} fn after(){}",
        )],
        "run",
        1,
    );
    let entry = d.nodes.iter().find(|n| n.label == "fn run").unwrap();
    let after = d.nodes.iter().find(|n| n.label == "after()").unwrap();
    assert!(!reachable(&d, &entry.id, &after.id));
    let (_, d) = inspect(
        &[(
            "src/lib.rs",
            "pub fn run(){while ready(){continue;} for x in values(){consume(x);} } fn ready()->bool{true} fn values()->[u8;0]{[]} fn consume(_:u8){}",
        )],
        "run",
        1,
    );
    let head = d.nodes.iter().find(|n| n.label == "while ready()").unwrap();
    let condition = d
        .nodes
        .iter()
        .find(|n| n.label.starts_with("ready()"))
        .unwrap();
    let jump = d.nodes.iter().find(|n| n.label == "continue").unwrap();
    assert!(d.edges.iter().any(|e| e.from == jump.id && e.to == head.id));
    assert!(reachable(&d, &head.id, &condition.id));
    assert_eq!(
        d.nodes
            .iter()
            .filter(|n| n.label.starts_with("values()"))
            .count(),
        1
    );
    assert!(
        d.edges
            .iter()
            .any(|e| e.label.as_deref() == Some("exhausted"))
    );
}
#[test]
fn recursion_depth_unknowns_and_return_operands_are_explicit() {
    let (_, d) = inspect(
        &[(
            "src/lib.rs",
            "pub fn recur(x:u8)->u8{if x==0{return external::value();}recur(x-1)}",
        )],
        "recur",
        3,
    );
    assert!(
        d.nodes
            .iter()
            .any(|n| n.label.contains("recursion boundary"))
    );
    assert!(
        d.nodes
            .iter()
            .any(|n| n.label.contains("not expanded:") && !n.label.contains("recursion"))
    );
    let call = d
        .nodes
        .iter()
        .find(|n| n.label.starts_with("external::value()"))
        .unwrap();
    let transfer = d
        .nodes
        .iter()
        .find(|n| n.label == "return external::value()")
        .unwrap();
    assert!(reachable(&d, &call.id, &transfer.id));
}

#[test]
fn large_empty_match_arms_refuse_the_node_budget_instead_of_evading_it() {
    use std::fmt::Write as _;
    let mut arms = String::new();
    for index in 0..210 {
        write!(arms, "{index}=>{{}},").expect("formatting into a String cannot fail");
    }
    let source = format!("pub fn run(x:u16){{match x{{{arms}_=>{{}}}}}}");
    let f = Fixture::new(&[("src/lib.rs", &source)]);
    let m = cargo_metadata::MetadataCommand::new()
        .current_dir(&f.0)
        .no_deps()
        .exec()
        .unwrap();
    let graph = discovery::scan_metadata(&m).unwrap();
    let error = workflow::expanded(&f.0, &m, &graph, "src/lib.rs", "run", 1).unwrap_err();
    assert!(error.to_string().contains("600"));
}
