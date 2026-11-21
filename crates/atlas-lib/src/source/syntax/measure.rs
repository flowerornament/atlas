//! Each function's length and nesting, read from the syntax tree the way
//! Clippy's `too_many_lines` and `excessive_nesting` read them, once per
//! file, so the concentration law and the facts columns share one number.
//!
//! Lines: the body's text without its outer braces (an `async fn` keeps
//! them, as Clippy reads its desugared body), trimmed, and each line that
//! holds code outside a comment counted by Clippy's own loop, quirks
//! included (a `/*` inside a string opens a comment). Nesting: the deepest
//! brace block inside the function, its nested items included, counted
//! from the root of the file: `impl`, `trait` and inline `mod` add a level,
//! as does every `{}` block; `match`, arm expressions, struct literals,
//! `else if` and expression closures do not; `unsafe {}` and labelled
//! blocks, and test-gated statements, are neither counted nor entered.
//!
//! Limits: Atlas expands no macro, so a function a `macro_rules!` writes
//! and a function in macro input are not measured, and a block inside a
//! macro's arguments is not counted (Clippy counts those that expand into
//! blocks, so Atlas can read lower there). A function without a body is
//! not measured. Every cfg alternative is measured.
use quote::ToTokens;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use syn::{
    spanned::Spanned,
    visit::{self, Visit},
};

/// One function's measures and identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct FunctionMeasure {
    /// As `flow --function` prints it (`m::Owner::method`,
    /// `<Owner as Trait>::method`, `Trait::method` for a default body,
    /// `outer::inner` for a nested fn), with `#2`, `#3`, ... for a spelling
    /// repeated in the file, in source order.
    pub function: String,
    /// The signature's start (one-based line, zero-based column): the site
    /// a facts `function.id` names.
    pub line: usize,
    pub column: usize,
    /// The item's first byte, attributes included, for population choice.
    pub start: usize,
    /// Code lines in the body, as `too_many_lines` counts them.
    pub lines: usize,
    /// The deepest level a block in it reaches, as `excessive_nesting`
    /// counts it.
    pub nesting: usize,
    /// A digest of the signature's and body's tokens: comments, spacing and
    /// positions do not change it.
    pub fingerprint: String,
}

/// Measure every function with a body in `file`, whose text is `text`.
pub(super) fn functions(file: &syn::File, text: &str) -> Vec<FunctionMeasure> {
    let mut reader = Reader {
        text,
        path: Vec::new(),
        level: 0,
        open: Vec::new(),
        out: Vec::new(),
    };
    reader.visit_file(file);
    number_repeats(&mut reader.out);
    reader.out
}

/// A second (third, ...) function with one spelling in a file is `name#2`.
fn number_repeats(out: &mut [FunctionMeasure]) {
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    for measure in out {
        let count = seen.entry(measure.function.clone()).or_default();
        *count += 1;
        if *count > 1 {
            measure.function = format!("{}#{count}", measure.function);
        }
    }
}

struct Reader<'a> {
    text: &'a str,
    /// The enclosing modules, owners, traits and functions, as spelled in
    /// a function's identity.
    path: Vec<String>,
    /// The current nesting level, from the root of the file.
    level: usize,
    /// Indexes into `out` of the functions being read, outermost first.
    open: Vec<usize>,
    out: Vec<FunctionMeasure>,
}

impl Reader<'_> {
    /// Read one function item: record it, then read its signature and body
    /// with its name on the path, raising its nesting and every enclosing
    /// function's to each block it holds.
    fn function(&mut self, attrs: &[syn::Attribute], sig: &syn::Signature, block: &syn::Block) {
        let start = attrs
            .first()
            .map_or_else(|| sig.span(), Spanned::span)
            .byte_range()
            .start;
        let at = sig.span().start();
        let name = sig.ident.to_string();
        let function = self
            .path
            .iter()
            .cloned()
            .chain([name.clone()])
            .collect::<Vec<_>>()
            .join("::");
        self.out.push(FunctionMeasure {
            function,
            line: at.line,
            column: at.column,
            start,
            lines: code_lines(
                &self.text[block.span().byte_range()],
                sig.asyncness.is_some(),
            ),
            nesting: self.level + 1,
            fingerprint: fingerprint(sig, block),
        });
        self.open.push(self.out.len() - 1);
        self.path.push(name);
        self.visit_signature(sig);
        self.visit_block(block);
        self.path.pop();
        self.open.pop();
    }
    /// Read an item that adds a level and a path segment.
    fn scope(&mut self, segment: String, read: impl FnOnce(&mut Self)) {
        self.path.push(segment);
        self.level += 1;
        read(self);
        self.level -= 1;
        self.path.pop();
    }
}

impl<'ast> Visit<'ast> for Reader<'_> {
    fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
        self.function(&item.attrs, &item.sig, &item.block);
    }
    fn visit_impl_item_fn(&mut self, item: &'ast syn::ImplItemFn) {
        self.function(&item.attrs, &item.sig, &item.block);
    }
    fn visit_trait_item_fn(&mut self, item: &'ast syn::TraitItemFn) {
        if let Some(block) = &item.default {
            self.function(&item.attrs, &item.sig, block);
        }
    }
    fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
        if item.content.is_some() {
            self.scope(item.ident.to_string(), |reader| {
                visit::visit_item_mod(reader, item);
            });
        }
    }
    fn visit_item_impl(&mut self, item: &'ast syn::ItemImpl) {
        let owner = written(self.text, item.self_ty.span());
        let segment = match &item.trait_ {
            Some((path, _)) => format!("<{owner} as {}>", written(self.text, path.span())),
            None => owner,
        };
        self.scope(segment, |reader| visit::visit_item_impl(reader, item));
    }
    fn visit_item_trait(&mut self, item: &'ast syn::ItemTrait) {
        self.scope(item.ident.to_string(), |reader| {
            visit::visit_item_trait(reader, item);
        });
    }
    /// A brace block is one level deeper than what holds it.
    fn visit_block(&mut self, block: &'ast syn::Block) {
        self.level += 1;
        for &open in &self.open {
            let measure = &mut self.out[open];
            measure.nesting = measure.nesting.max(self.level);
        }
        for stmt in &block.stmts {
            if !test_gated(stmt) {
                self.visit_stmt(stmt);
            }
        }
        self.level -= 1;
    }
    /// `unsafe {}` is neither a level nor entered, as Clippy reads it.
    fn visit_expr_unsafe(&mut self, _: &'ast syn::ExprUnsafe) {}
    /// Nor is a labelled block (`'a: {}`).
    fn visit_expr_block(&mut self, expr: &'ast syn::ExprBlock) {
        if expr.label.is_none() {
            self.visit_block(&expr.block);
        }
    }
}

/// A statement under a test gate is stripped before Clippy reads nesting.
fn test_gated(stmt: &syn::Stmt) -> bool {
    let attrs: &[syn::Attribute] = match stmt {
        syn::Stmt::Local(local) => &local.attrs,
        syn::Stmt::Item(item) => item_attrs(item),
        syn::Stmt::Expr(expr, _) => expr_attrs(expr),
        syn::Stmt::Macro(mac) => &mac.attrs,
    };
    super::attributes::requires(attrs, &[])
}
fn item_attrs(item: &syn::Item) -> &[syn::Attribute] {
    match item {
        syn::Item::Fn(item) => &item.attrs,
        syn::Item::Const(item) => &item.attrs,
        syn::Item::Static(item) => &item.attrs,
        syn::Item::Struct(item) => &item.attrs,
        syn::Item::Enum(item) => &item.attrs,
        syn::Item::Impl(item) => &item.attrs,
        syn::Item::Mod(item) => &item.attrs,
        syn::Item::Use(item) => &item.attrs,
        _ => &[],
    }
}
fn expr_attrs(expr: &syn::Expr) -> &[syn::Attribute] {
    match expr {
        syn::Expr::Block(expr) => &expr.attrs,
        syn::Expr::Call(expr) => &expr.attrs,
        syn::Expr::MethodCall(expr) => &expr.attrs,
        syn::Expr::If(expr) => &expr.attrs,
        syn::Expr::Match(expr) => &expr.attrs,
        syn::Expr::Macro(expr) => &expr.attrs,
        syn::Expr::Assign(expr) => &expr.attrs,
        _ => &[],
    }
}

/// The text at `span` with its whitespace collapsed to single spaces.
fn written(text: &str, span: proc_macro2::Span) -> String {
    text[span.byte_range()]
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Clippy's `too_many_lines` count of a body's text: the outer braces
/// dropped (kept for an `async fn`), trimmed, then each line counted that
/// holds code outside `//` and `/* */` comments.
fn code_lines(body: &str, asyncness: bool) -> usize {
    let inner = if !asyncness && body.starts_with('{') && body.ends_with('}') {
        &body[1..body.len() - 1]
    } else {
        body
    };
    let mut count = 0;
    let mut in_comment = false;
    for mut line in inner.trim().lines() {
        let mut code = false;
        loop {
            line = line.trim_start();
            if line.is_empty() {
                break;
            }
            if in_comment {
                if let Some(end) = line.find("*/") {
                    line = &line[end + 2..];
                    in_comment = false;
                    continue;
                }
            } else {
                let multi = line.find("/*").unwrap_or(line.len());
                let single = line.find("//").unwrap_or(line.len());
                code |= multi > 0 && single > 0;
                if multi < single {
                    line = &line[multi + 2..];
                    in_comment = true;
                    continue;
                }
            }
            break;
        }
        count += usize::from(code);
    }
    count
}

/// A digest of the signature and body tokens, positions dropped.
fn fingerprint(sig: &syn::Signature, block: &syn::Block) -> String {
    let mut tokens = sig.to_token_stream();
    block.to_tokens(&mut tokens);
    crate::discovery::fingerprint(tokens.to_string().as_bytes())
}

#[cfg(test)]
mod tests;
