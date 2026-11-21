//! The function measure against Clippy itself: on a fixture crate with low
//! thresholds, the functions Clippy's `too_many_lines` and
//! `excessive_nesting` flag are exactly those the measure puts over the
//! same limits. It runs Clippy, so it is ignored by default: CI runs it,
//! and `just clippy-golden` runs it locally.
mod support;
use std::{collections::BTreeSet, process::Command};
use support::Fixture;

const LINES: usize = 3;
const NESTING: usize = 3;

/// Functions near both thresholds, written with each construct the measure
/// reads: comments, a string that opens a phantom comment, async, closures,
/// arms, unsafe and labelled blocks, impls, inline modules, nested fns.
const LIB: &str = r#"#![allow(dead_code, unused)]
fn three_lines() {
    let a = 1;
    // a comment
    let b = 2;
    let c = 3;
}
fn four_lines() {
    let a = 1;
    let b = 2;
    let c = 3;
    let d = 4;
}
fn phantom_comment() {
    let a = "/* opens";
    let b = 2;
    let c = 3;
    let d = 4;
}
async fn async_two() {
    let a = 1;
    let b = 2;
}
fn two_deep() {
    if true {
        let a = 1;
    }
}
fn three_deep() {
    if true {
        loop {
            break;
        }
    }
}
fn four_deep() {
    if true {
        loop {
            match 1 {
                _ => {}
            }
        }
    }
}
fn unsafe_is_flat() {
    unsafe {
        if true {
            loop {
                if true {}
            }
        }
    }
}
fn labelled_is_flat() {
    'a: {
        if true {
            loop {
                if true {}
            }
        }
    }
}
fn closure_block() {
    let f = || {
        let g = || {
            if true {}
        };
    };
}
struct S;
impl S {
    fn method_two() {
        if true {}
    }
    fn method_three() {
        if true {
            loop {
                break;
            }
        }
    }
}
mod inner {
    fn in_module() {
        if true {
            loop {
                break;
            }
        }
    }
}
fn outer() {
    fn nested() {
        if true {
            loop {
                break;
            }
        }
    }
}
"#;

/// The functions Clippy flags for `lint`, by the line their span starts at,
/// and for nesting, every function whose span holds a flagged block.
fn clippy(root: &std::path::Path) -> (BTreeSet<usize>, BTreeSet<usize>) {
    let output = Command::new("cargo")
        .current_dir(root)
        .env("CARGO_TARGET_DIR", root.join("target"))
        .args([
            "clippy",
            "--quiet",
            "--message-format=json",
            "--",
            "-A",
            "clippy::all",
            "-W",
            "clippy::too_many_lines",
            "-W",
            "clippy::excessive_nesting",
        ])
        .output()
        .expect("run clippy");
    let mut lines = BTreeSet::new();
    let mut blocks = BTreeSet::new();
    for message in String::from_utf8_lossy(&output.stdout).lines() {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(message) else {
            continue;
        };
        let message = &value["message"];
        let line = message["spans"][0]["line_start"]
            .as_u64()
            .and_then(|line| usize::try_from(line).ok())
            .unwrap_or(0);
        match message["code"]["code"].as_str() {
            Some("clippy::too_many_lines") => lines.insert(line),
            Some("clippy::excessive_nesting") => blocks.insert(line),
            _ => false,
        };
    }
    (lines, blocks)
}

#[test]
#[ignore = "runs Clippy; CI and `just clippy-golden` run it"]
fn the_measure_flags_what_clippy_flags() {
    let f = Fixture::new(&[
        (
            "Cargo.toml",
            "[package]\nname = \"golden\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[workspace]\n",
        ),
        (
            "clippy.toml",
            &format!(
                "too-many-lines-threshold = {LINES}\nexcessive-nesting-threshold = {NESTING}\n"
            ),
        ),
        ("src/lib.rs", LIB),
    ]);
    let (long, deep_blocks) = clippy(f.path());
    let ast = syn::parse_file(LIB).expect("fixture parses");
    let source = atlas_lib::source::Source::of(std::path::Path::new("src/lib.rs"), LIB.to_owned())
        .expect("fixture source");
    let spans = function_spans(&ast);
    let mut measured_long = BTreeSet::new();
    let mut measured_deep = BTreeSet::new();
    let mut clippy_deep = BTreeSet::new();
    for measure in source.functions() {
        let (start, end) = spans[&measure.line];
        if measure.lines > LINES {
            measured_long.insert(measure.line);
        }
        if measure.nesting > NESTING {
            measured_deep.insert(measure.line);
        }
        if deep_blocks.iter().any(|line| (start..=end).contains(line)) {
            clippy_deep.insert(measure.line);
        }
    }
    assert!(
        !long.is_empty() && !clippy_deep.is_empty(),
        "Clippy flagged nothing: is it installed?"
    );
    assert_eq!(measured_long, long, "too_many_lines");
    assert_eq!(measured_deep, clippy_deep, "excessive_nesting");
}

/// Each function's signature line and its last line.
fn function_spans(file: &syn::File) -> std::collections::BTreeMap<usize, (usize, usize)> {
    use syn::{spanned::Spanned, visit::Visit};
    struct Spans(std::collections::BTreeMap<usize, (usize, usize)>);
    impl<'ast> Visit<'ast> for Spans {
        fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
            self.0.insert(
                item.sig.span().start().line,
                (item.sig.span().start().line, item.span().end().line),
            );
            syn::visit::visit_item_fn(self, item);
        }
        fn visit_impl_item_fn(&mut self, item: &'ast syn::ImplItemFn) {
            self.0.insert(
                item.sig.span().start().line,
                (item.sig.span().start().line, item.span().end().line),
            );
            syn::visit::visit_impl_item_fn(self, item);
        }
    }
    let mut spans = Spans(std::collections::BTreeMap::new());
    spans.visit_file(file);
    spans.0
}
