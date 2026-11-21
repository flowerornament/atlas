//! The measure's rules, each on a small written case.
use super::*;

fn measured(text: &str) -> Vec<FunctionMeasure> {
    functions(&syn::parse_file(text).expect("fixture parses"), text)
}
fn one(text: &str, function: &str) -> FunctionMeasure {
    measured(text)
        .into_iter()
        .find(|m| m.function == function)
        .unwrap_or_else(|| panic!("{function} in {:?}", measured(text)))
}

#[test]
fn lines_count_code_outside_comments_and_braces() {
    let text = "fn f() {\n    let a = 1;\n\n    // a comment\n    /* a block\n       comment */ let b = 2;\n    let c = \"/* opens\";\n    let d = 4;\n}\n";
    // a, the line closing the comment (it holds b), and c; d's line is in
    // the phantom comment the string opened.
    assert_eq!(one(text, "f").lines, 3);
    assert_eq!(one("fn f() { g(); }\n", "f").lines, 1);
    assert_eq!(one("fn f() {}\n", "f").lines, 0);
}

#[test]
fn an_async_fn_counts_its_brace_lines() {
    assert_eq!(one("async fn f() {\n    g();\n}\n", "f").lines, 3);
}

#[test]
fn test_gated_statements_closures_and_nested_fns_count_as_text() {
    let text = "fn f() {\n    #[cfg(test)]\n    g();\n    let c = || {\n        h();\n    };\n    fn inner() {\n        i();\n    }\n}\n";
    assert_eq!(one(text, "f").lines, 8);
    assert_eq!(one(text, "f::inner").lines, 1);
}

#[test]
fn nesting_counts_blocks_and_scopes_from_the_file_root() {
    assert_eq!(one("fn f() {}\n", "f").nesting, 1);
    assert_eq!(one("fn f() { if a { b(); } }\n", "f").nesting, 2);
    assert_eq!(one("impl T { fn f() { loop { } } }\n", "T::f").nesting, 3);
    assert_eq!(
        one("mod m { trait Q { fn f() { } } }\n", "m::Q::f").nesting,
        3
    );
    // match arms written `=> {}`, block closures, async and let-else count:
    // body 1, arm 2, closure 3, async 4, else 5.
    let text = "fn f() { match x { _ => { let g = || { async { let Some(_) = y else { return }; }; }; } } }\n";
    assert_eq!(one(text, "f").nesting, 5);
    // match itself, arm expressions, else-if and expression closures do not.
    let flat =
        "fn f() { match x { _ => a(), }; if a { } else if b { } else { }; let g = |x| x + 1; }\n";
    assert_eq!(one(flat, "f").nesting, 2);
}

#[test]
fn unsafe_labelled_and_test_gated_blocks_are_not_entered() {
    let text = "fn f() { unsafe { { { { } } } } 'a: { { { } } } #[cfg(test)] { { { } } } }\n";
    assert_eq!(one(text, "f").nesting, 1);
}

#[test]
fn a_nested_fn_raises_its_outer_and_macro_arguments_count_nothing() {
    let text = "fn f() { fn inner() { if a { } } }\n";
    assert_eq!(one(text, "f").nesting, 3);
    assert_eq!(one(text, "f::inner").nesting, 3);
    assert_eq!(one("fn f() { assert!({ { true } }); }\n", "f").nesting, 1);
}

#[test]
fn identity_is_flows_spelling_with_generics_and_repeats_numbered() {
    let text = "struct T;\nimpl From<A> for T { fn from(_: A) -> Self { T } }\nimpl From<B> for T { fn from(_: B) -> Self { T } }\ntrait Q { fn d() { } fn r(); }\n#[cfg(unix)]\nfn twin() {}\n#[cfg(windows)]\nfn twin() {}\nmod m { impl Vec < u8 > { fn method(&self) {} } }\n";
    let names: Vec<_> = measured(text).into_iter().map(|m| m.function).collect();
    assert_eq!(
        names,
        [
            "<T as From<A>>::from",
            "<T as From<B>>::from",
            "Q::d",
            "twin",
            "twin#2",
            "m::Vec < u8 >::method"
        ]
    );
}

#[test]
fn the_fingerprint_ignores_comments_and_spacing_but_not_code() {
    let a = one("fn f() {\n    g(); // why\n}\n", "f").fingerprint;
    let b = one("fn f() { g(); }\n", "f").fingerprint;
    let c = one("fn f() { h(); }\n", "f").fingerprint;
    assert_eq!(a, b);
    assert_ne!(a, c);
}
