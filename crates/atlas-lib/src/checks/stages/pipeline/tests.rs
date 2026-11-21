//! Each pipeline refusal goes red on its own construct, and the listed
//! straight-line composition stays green.
use super::*;

const STAGES: &str = "pub mod stage {
    pub fn one(x: u32) -> Result<u32, ()> { Ok(x) }
    pub fn two(x: u32, _o: u32) -> Result<u32, ()> { Ok(x) }
    pub fn three(x: u32) -> Result<u32, ()> { Ok(x) }
}
";

/// The violations and unavailable fields for `run` in a crate whose
/// `src/lib.rs` is `STAGES` plus `body`, under `row` (TOML table fields).
fn judged(body: &str, row: &str) -> (Vec<String>, Vec<String>) {
    let fixture = crate::test_support::Fixture::new(&[
        (
            "Cargo.toml",
            "[package]\nname='fix'\nversion='0.0.0'\nedition='2024'\n[workspace]\n",
        ),
        ("src/lib.rs", &format!("{STAGES}{body}")),
    ]);
    let snapshot = crate::discovery::snapshot_workspace(fixture.path()).expect("snapshot");
    let analysis = Analysis::new(&snapshot).expect("analysis");
    let order: StageOrder = toml::from_str(&format!(
        "crate = 'fix'\nstages = ['stage']\npipeline = [{{ {file}{row} }}]",
        file = if row.contains("file =") {
            ""
        } else {
            "file = 'src/lib.rs', "
        }
    ))
    .expect("fixture order");
    let found = judge(&analysis, &[order]);
    let unavailable = found.unavailable.into_iter().map(|m| m.reason).collect();
    (found.violations, unavailable)
}
fn violations(body: &str, row: &str) -> Vec<String> {
    let (violations, unavailable) = judged(body, row);
    assert!(unavailable.is_empty(), "{unavailable:?}");
    violations
}
const THREE: &str = "function = 'run', calls = ['stage::one', 'stage::two', 'stage::three']";
const RUN: &str = "pub fn run(x: u32, opts: u32) -> Result<u32, ()> {
    let a = stage::one(x)?;
    let b = stage::two(a, opts)?;
    Ok(stage::three(b)?)
}
";
fn assert_one(found: &[String], needle: &str) {
    assert!(
        found.iter().any(|v| v.contains(needle)),
        "no `{needle}` in {found:#?}"
    );
}

#[test]
fn listed_stage_calls_pass() {
    assert_eq!(violations(RUN, THREE), Vec::<String>::new());
}

#[test]
fn an_unlisted_timer_call_refuses() {
    let timed = RUN.replace(
        "    let b =",
        "    let t = std::time::Instant::now();\n    let _us = t.elapsed().as_micros();\n    let b =",
    );
    let found = violations(&timed, THREE);
    for call in ["std::time::Instant::now", ".elapsed", ".as_micros"] {
        assert_one(
            &found,
            &format!("calls `{call}`, which its row does not list"),
        );
    }
    assert!(
        found[0].starts_with("stage_order.fix.pipeline.0 (src/lib.rs::run) at src/lib.rs:"),
        "{found:?}"
    );
}

#[test]
fn a_branch_refuses_even_when_every_call_is_listed() {
    let branched = RUN.replace(
        "    let b = stage::two(a, opts)?;",
        "    let b = if a > 1 { stage::two(a, opts)? } else { stage::two(a, 0)? };",
    );
    assert_one(
        &violations(&branched, THREE),
        "holds a branch (`if`); move the decision into a stage",
    );
    let matched = RUN.replace(
        "    let b = stage::two(a, opts)?;",
        "    let b = match a { 0 => 0, _ => stage::two(a, opts)? };",
    );
    assert_one(&violations(&matched, THREE), "holds a branch (`match`)");
    let otherwise = RUN.replace(
        "    let a = stage::one(x)?;",
        "    let Ok(a) = stage::one(x) else { panic!() };",
    );
    assert_one(
        &violations(&otherwise, THREE),
        "holds a branch (`let-else`)",
    );
}

#[test]
fn a_closure_merge_refuses_and_async_blocks_too() {
    let merged = RUN.replace(
        "    let b = stage::two(a, opts)?;",
        "    let b = Some(a).map(|a| stage::two(a, opts)).unwrap()?;",
    );
    let found = violations(&merged, THREE);
    assert_one(&found, "holds a closure (deferred body)");
    assert_one(&found, "calls `.map`");
    let deferred = RUN.replace("    let b =", "    let _f = async { 1 };\n    let b =");
    assert_one(&violations(&deferred, THREE), "holds a `async` block");
}

#[test]
fn a_listed_call_no_longer_made_refuses() {
    let found = violations(RUN, &THREE.replace(']', ", 'stage::four']"));
    assert_one(
        &found,
        "lists `stage::four`, which its body does not call; remove it from `calls`",
    );
}

const PRODUCT: &str = "pub struct Out { pub frames: Vec<f32>, pub n: usize }
pub fn run(n: usize, c: usize) -> Out {
    let mut frames = vec![0.0; n * c];
    let mut count = 0;
    for frame in frames.chunks_mut(c) {
        stage::fill(frame);
        count += 1;
    }
    for _ in 0..2 {
        count += 1;
    }
    Out { frames, n: count }
}
pub mod stage_fill {}
";
fn product() -> String {
    PRODUCT
        .replace("stage::fill(frame)", "fill(frame)")
        .replace("pub mod stage_fill {}", "pub fn fill(_f: &mut [f32]) {}")
}
const PRODUCT_ROW: &str = "function = 'run', calls = ['vec!', '.chunks_mut', 'fill'], loops = true";

#[test]
fn a_product_loop_passes_with_loops_and_several_fors() {
    assert_eq!(violations(&product(), PRODUCT_ROW), Vec::<String>::new());
}

#[test]
fn a_for_refuses_without_loops() {
    let found = violations(&product(), &PRODUCT_ROW.replace(", loops = true", ""));
    assert_one(&found, "holds `for`; its row does not set `loops`");
}

#[test]
fn an_early_exit_refuses_even_with_loops() {
    for exit in ["return Out { frames, n: 0 };", "break;", "continue;"] {
        let early = product().replace(
            "        count += 1;\n    }\n    for",
            &format!("        if false {{ {exit} }}\n        count += 1;\n    }}\n    for"),
        );
        let keyword = exit
            .trim_end_matches(';')
            .split(' ')
            .next()
            .unwrap_or_default();
        assert_one(
            &violations(&early, PRODUCT_ROW),
            &format!("holds an early exit (`{keyword}`)"),
        );
    }
}

#[test]
fn a_struct_update_merge_refuses() {
    let updated = product().replace(
        "    Out { frames, n: count }",
        "    let base = Out { frames: Vec::new(), n: 0 };\n    Out { frames, ..base }",
    );
    let found = violations(
        &updated,
        &PRODUCT_ROW.replace("'fill'", "'fill', 'Vec::new'"),
    );
    assert_one(&found, "holds a struct-update merge `..`");
    assert!(
        !found.iter().any(|v| v.contains("count")),
        "a range or count is not a merge: {found:?}"
    );
}

#[test]
fn a_binary_and_or_refuses_but_a_reference_passes() {
    for (merge, op) in [
        ("a & 1", "`&` merge"),
        ("(a > 0) && (opts > 0)", "`&&` merge"),
        ("a | 1", "`|` merge"),
        ("(a > 0) || true", "`||` merge"),
    ] {
        let merged = RUN.replace(
            "    let b = stage::two(a, opts)?;",
            &format!("    let _m = {merge};\n    let b = stage::two(a, opts)?;"),
        );
        assert_one(&violations(&merged, THREE), &format!("holds a {op}"));
    }
    let assigned = RUN.replace(
        "    let b = stage::two(a, opts)?;",
        "    let mut m = a;\n    m &= 1;\n    m |= 2;\n    let b = stage::two(m, opts)?;",
    );
    let found = violations(&assigned, THREE);
    assert_one(&found, "holds a `&` merge");
    assert_one(&found, "holds a `|` merge");
    let referenced = RUN.replace(
        "    let b = stage::two(a, opts)?;",
        "    let r = &a;\n    let b = stage::two(*r, opts)?;",
    );
    assert_eq!(violations(&referenced, THREE), Vec::<String>::new());
}

#[test]
fn a_missing_or_ambiguous_function_refuses() {
    let found = violations(RUN, &THREE.replace("'run'", "'absent'"));
    assert_one(&found, "expected one function, found 0");
    let twice = format!("{RUN}pub mod again {{ pub fn run() {{}} }}\n");
    let found = violations(&twice, THREE);
    assert_one(&found, "expected one function, found 2");
    assert_one(&found, "again::run");
    // The qualified name picks one.
    assert_one(
        &violations(&twice, "function = 'again::run', calls = ['x']"),
        "lists `x`",
    );
}

#[test]
fn a_qualified_method_is_selectable() {
    let method = "pub struct P;\nimpl P {\n    pub fn run(&self, x: u32) -> Result<u32, ()> {\n        let a = stage::one(x)?;\n        Ok(stage::three(a)?)\n    }\n}\n";
    assert_eq!(
        violations(
            method,
            "function = 'P::run', calls = ['stage::one', 'stage::three']"
        ),
        Vec::<String>::new()
    );
}

#[test]
fn a_turbofish_call_refuses() {
    let fished = RUN.replace("    let b = stage::two(a, opts)?;", "    let _v = Vec::<u32>::new();\n    let _w = [1u32].iter().copied().collect::<Vec<_>>();\n    let b = stage::two(a, opts)?;");
    let found = violations(&fished, THREE);
    assert_eq!(
        found
            .iter()
            .filter(|v| v.contains("holds a call it cannot read as written"))
            .count(),
        2,
        "{found:#?}"
    );
}

#[test]
fn nested_argument_calls_each_count_and_ok_is_free_only_around_the_tail() {
    let nested = "pub fn run(x: u32) -> Result<u32, ()> {\n    let a = stage::two(stage::one(x)?, Ok::<u32, ()>(1)?)?;\n    Ok(stage::three(a)?)\n}\n";
    let found = violations(
        nested,
        "function = 'run', calls = ['stage::one', 'stage::two', 'stage::three']",
    );
    assert_one(&found, "holds a call it cannot read as written");
    let plain_ok = nested.replace("Ok::<u32, ()>(1)?", "Ok(1)?");
    let found = violations(
        &plain_ok,
        "function = 'run', calls = ['stage::one', 'stage::two', 'stage::three']",
    );
    assert_one(&found, "calls `Ok`, which its row does not list");
    assert_eq!(found.len(), 1, "{found:#?}");
}

#[test]
fn question_mark_on_a_value_refuses() {
    let valued = RUN.replace(
        "    let b = stage::two(a, opts)?;",
        "    let r = stage::two(a, opts);\n    let b = r?;",
    );
    assert_one(
        &violations(&valued, THREE),
        "holds `?` on a value, not a call",
    );
}

#[test]
fn macros_are_read_as_expressions_or_refused() {
    let formatted = RUN.replace(
        "    let b =",
        "    let _s = format!(\"{} {n}\", stage::one(x)?, n = opts);\n    let b =",
    );
    let found = violations(&formatted, &THREE.replace(']', ", 'format!']"));
    assert_eq!(found, Vec::<String>::new());
    let unlisted = violations(&formatted, THREE);
    assert_one(&unlisted, "calls `format!`");
    let odd = RUN.replace("    let b =", "    odd!(x => y);\n    let b =");
    let found = violations(
        &format!("macro_rules! odd {{ ($a:ident => $b:ident) => {{}} }}\n{odd}"),
        &THREE.replace(']', ", 'odd!']"),
    );
    assert_one(
        &found,
        "holds a macro it cannot read (`odd!`); call a function instead",
    );
}

#[test]
fn a_cfg_statement_refuses() {
    let gated = RUN.replace(
        "    let b =",
        "    #[cfg(feature = \"x\")]\n    let _y = 1;\n    let b =",
    );
    assert_one(
        &violations(&gated, THREE),
        "holds a cfg-gated statement (a branch)",
    );
}

#[test]
fn shape_rules_refuse_line_selectors_odd_spellings_and_duplicates() {
    for (row, error) in [
        ("function = '12', calls = []", "selects `12` by line"),
        (
            "function = 'run', calls = ['a b']",
            "must be written as a path",
        ),
        ("function = 'run', calls = ['.m', '.m']", "lists `.m` twice"),
    ] {
        let order: StageOrder = toml::from_str(&format!(
            "crate = 'fix'\nstages = ['stage']\npipeline = [{{ file = 'src/lib.rs', {row} }}]"
        ))
        .expect("fixture order");
        let found = validate(&order).unwrap_err().to_string();
        assert!(found.contains(error), "{row}: {found}");
    }
}

#[test]
fn a_test_function_or_another_file_is_never_a_pipeline() {
    let tested = "#[cfg(test)]\nmod tests {\n    pub fn run() {}\n}\n";
    assert_one(&violations(tested, THREE), "expected one function, found 0");
    let (found, _) = judged(RUN, "file = 'src/other.rs', function = 'run', calls = []");
    assert_one(
        &found,
        "file `src/other.rs` is not a production source of package `fix`",
    );
}
