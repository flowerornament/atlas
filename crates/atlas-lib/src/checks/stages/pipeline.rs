//! `stage_order.pipeline`: a function that runs a crate's stages makes
//! exactly its listed calls, in straight-line code.
//!
//! The function is chosen with flow's selector over the file's production
//! syntax, and its body is read as written: a call is spelled by its path
//! (`stage::run`, `Some`), `.method` or `macro!`, never resolved, so an
//! alias is another spelling. Every call counts wherever it appears
//! (arguments, receivers, `let` initializers, `for` heads); a `?` on a call
//! belongs to it, and one outer `Ok(..)` around the body's final expression
//! is not a call (what it wraps is). A macro's tokens are read as
//! expressions, so the calls in its arguments count; its expansion is not
//! seen, so listing a macro trusts it as a step. Branches, loops other than
//! `for` (allowed by `loops = true`, each body read by these same rules),
//! early exits, closures and `async` blocks, `&`/`|` merges, struct update
//! `..`, `cfg`-gated statements and syntax it cannot read as calls refuse.
//! Items written inside the body are declarations and are not read.
use crate::analysis::{Analysis, workflow::Site};
use crate::checks::policy::{MissingInput, Pipeline, StageOrder};
use std::collections::BTreeMap;
use syn::{
    BinOp, Expr, Stmt,
    parse::Parser as _,
    punctuated::Punctuated,
    spanned::Spanned,
    visit::{self, Visit},
};

/// What the pipelines were found to break, and what could not be read.
#[derive(Default)]
pub(in crate::checks) struct Judgement {
    pub violations: Vec<String>,
    pub unavailable: Vec<MissingInput>,
}

/// One body's calls by spelling, with their sites, and what it refuses.
#[derive(Debug, Default)]
struct Reading {
    calls: BTreeMap<String, Vec<Site>>,
    refusals: Vec<(Site, Refusal)>,
}

/// A construct a pipeline may not hold.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Refusal {
    Branch(&'static str),
    Loop(&'static str),
    ForWithoutLoops,
    Exit(&'static str),
    Deferred(&'static str),
    Merge(&'static str),
    StructUpdate,
    UnreadableCall,
    UnreadableMacro(String),
    TryOnValue,
    CfgGated,
    Unsupported(&'static str),
}
impl Refusal {
    /// The message and its fix.
    fn explain(&self) -> (String, &'static str) {
        match self {
            Self::Branch(kind) => (
                format!("holds a branch (`{kind}`)"),
                "move the decision into a stage",
            ),
            Self::Loop(kind) => (format!("holds a loop (`{kind}`)"), "iterate inside a stage"),
            Self::ForWithoutLoops => (
                "holds `for`; its row does not set `loops`".into(),
                "set `loops = true` if iterating a product is the composition, else move it into a stage",
            ),
            Self::Exit(kind) => (
                format!("holds an early exit (`{kind}`)"),
                "end the body with its result; use `?` on a call",
            ),
            Self::Deferred(kind) => (
                format!("holds a {kind} (deferred body)"),
                "name the step as a function and call it",
            ),
            Self::Merge(op) => (format!("holds a `{op}` merge"), "merge inside a stage"),
            Self::StructUpdate => (
                "holds a struct-update merge `..`".into(),
                "build the value in a stage",
            ),
            Self::UnreadableCall => (
                "holds a call it cannot read as written".into(),
                "drop the turbofish (annotate the binding), or name the callee",
            ),
            Self::UnreadableMacro(name) => (
                format!("holds a macro it cannot read (`{name}`)"),
                "call a function instead",
            ),
            Self::TryOnValue => (
                "holds `?` on a value, not a call".into(),
                "`?` the call that produces it",
            ),
            Self::CfgGated => (
                "holds a cfg-gated statement (a branch)".into(),
                "gate the whole function",
            ),
            Self::Unsupported(kind) => (
                format!("holds syntax this rule cannot read (`{kind}`)"),
                "rewrite as calls",
            ),
        }
    }
}

/// The body visitor: calls as written, and refusals.
struct Composition<'p> {
    path: &'p str,
    loops: bool,
    reading: Reading,
}
impl Composition<'_> {
    fn refuse(&mut self, span: proc_macro2::Span, refusal: Refusal) {
        self.reading
            .refusals
            .push((Site::at(self.path, span), refusal));
    }
    fn call(&mut self, spelling: String, span: proc_macro2::Span) {
        self.reading
            .calls
            .entry(spelling)
            .or_default()
            .push(Site::at(self.path, span));
    }
    /// The body: its final expression may be wrapped once in `Ok(..)`.
    fn body(&mut self, block: &syn::Block) {
        let Some((Stmt::Expr(tail, None), rest)) = block.stmts.split_last() else {
            return self.visit_block(block);
        };
        for stmt in rest {
            self.visit_stmt(stmt);
        }
        match tail {
            Expr::Call(call) if is_ok(&call.func) && call.args.len() == 1 => {
                for arg in &call.args {
                    self.visit_expr(arg);
                }
            }
            tail => self.visit_expr(tail),
        }
    }
    fn path_call(&mut self, call: &syn::ExprCall) {
        match &*call.func {
            Expr::Path(path)
                if path.qself.is_none()
                    && path.path.segments.iter().all(|s| s.arguments.is_none()) =>
            {
                self.call(spelling(&path.path), call.func.span());
            }
            func => self.refuse(func.span(), Refusal::UnreadableCall),
        }
        for arg in &call.args {
            self.visit_expr(arg);
        }
    }
    fn method_call(&mut self, call: &syn::ExprMethodCall) {
        if call.turbofish.is_some() {
            self.refuse(call.method.span(), Refusal::UnreadableCall);
        } else {
            self.call(format!(".{}", call.method), call.method.span());
        }
        self.visit_expr(&call.receiver);
        for arg in &call.args {
            self.visit_expr(arg);
        }
    }
    /// A macro is a call; its tokens are read as a comma list of
    /// expressions (`name = expr` included) or `expr; expr`.
    fn macro_call(&mut self, mac: &syn::Macro) {
        let name = format!(
            "{}!",
            mac.path
                .segments
                .iter()
                .map(|s| s.ident.to_string())
                .collect::<Vec<_>>()
                .join("::")
        );
        self.call(name.clone(), mac.path.span());
        let list = Punctuated::<Expr, syn::Token![,]>::parse_terminated.parse2(mac.tokens.clone());
        let repeat = |input: syn::parse::ParseStream<'_>| {
            let value: Expr = input.parse()?;
            input.parse::<syn::Token![;]>()?;
            let count: Expr = input.parse()?;
            Ok(vec![value, count])
        };
        let exprs = match list {
            Ok(list) => list.into_iter().collect(),
            Err(_) => match repeat.parse2(mac.tokens.clone()) {
                Ok(exprs) => exprs,
                Err(_) => return self.refuse(mac.path.span(), Refusal::UnreadableMacro(name)),
            },
        };
        for expr in &exprs {
            self.visit_expr(expr);
        }
    }
    /// The refusal an expression's own form makes, if any (its operands are
    /// read either way, except a deferred body's).
    fn form(&self, expr: &Expr) -> Option<Refusal> {
        Some(match expr {
            Expr::If(_) => Refusal::Branch("if"),
            Expr::Match(_) => Refusal::Branch("match"),
            Expr::While(_) => Refusal::Loop("while"),
            Expr::Loop(_) => Refusal::Loop("loop"),
            Expr::ForLoop(_) if !self.loops => Refusal::ForWithoutLoops,
            Expr::Return(_) => Refusal::Exit("return"),
            Expr::Break(_) => Refusal::Exit("break"),
            Expr::Continue(_) => Refusal::Exit("continue"),
            Expr::Binary(binary) => return merge(&binary.op).map(Refusal::Merge),
            Expr::Struct(literal) if literal.rest.is_some() => Refusal::StructUpdate,
            Expr::Try(attempt) if !is_call(&attempt.expr) => Refusal::TryOnValue,
            Expr::Block(block) if block.label.is_some() => Refusal::Unsupported("labelled block"),
            Expr::TryBlock(_) => Refusal::Unsupported("try"),
            Expr::Const(_) => Refusal::Unsupported("const"),
            Expr::Verbatim(_) => Refusal::Unsupported("verbatim"),
            _ => return None,
        })
    }
}
impl<'ast> Visit<'ast> for Composition<'_> {
    fn visit_expr(&mut self, expr: &'ast Expr) {
        if let Some(refusal) = self.form(expr) {
            self.refuse(expr.span(), refusal);
        }
        match expr {
            Expr::Call(call) => self.path_call(call),
            Expr::MethodCall(call) => self.method_call(call),
            Expr::Macro(mac) => self.macro_call(&mac.mac),
            Expr::Closure(closure) => self.refuse(closure.span(), Refusal::Deferred("closure")),
            Expr::Async(block) => self.refuse(block.span(), Refusal::Deferred("`async` block")),
            _ => visit::visit_expr(self, expr),
        }
    }
    fn visit_stmt(&mut self, stmt: &'ast Stmt) {
        let attrs = match stmt {
            Stmt::Local(local) => &local.attrs,
            Stmt::Macro(mac) => &mac.attrs,
            Stmt::Expr(expr, _) => attributes(expr),
            Stmt::Item(_) => return,
        };
        if attrs.iter().any(gates) {
            self.refuse(stmt.span(), Refusal::CfgGated);
        }
        match stmt {
            Stmt::Macro(mac) => self.macro_call(&mac.mac),
            stmt => visit::visit_stmt(self, stmt),
        }
    }
    fn visit_local(&mut self, local: &'ast syn::Local) {
        if let Some(init) = &local.init
            && init.diverge.is_some()
        {
            self.refuse(local.span(), Refusal::Branch("let-else"));
        }
        visit::visit_local(self, local);
    }
    fn visit_pat_or(&mut self, pattern: &'ast syn::PatOr) {
        self.refuse(pattern.span(), Refusal::Merge("|"));
        visit::visit_pat_or(self, pattern);
    }
    fn visit_item(&mut self, _: &'ast syn::Item) {}
}

/// A path as written: its segments joined by `::`, with a leading `::`.
fn spelling(path: &syn::Path) -> String {
    let joined = path
        .segments
        .iter()
        .map(|segment| segment.ident.to_string())
        .collect::<Vec<_>>()
        .join("::");
    if path.leading_colon.is_some() {
        format!("::{joined}")
    } else {
        joined
    }
}
fn is_ok(func: &Expr) -> bool {
    matches!(func, Expr::Path(path) if path.qself.is_none() && path.path.leading_colon.is_none() && path.path.segments.len() == 1 && path.path.segments[0].ident == "Ok" && path.path.segments[0].arguments.is_none())
}
/// Whether `?`'s operand, past parentheses and `.await`, is a call.
fn is_call(expr: &Expr) -> bool {
    match expr {
        Expr::Paren(inner) => is_call(&inner.expr),
        Expr::Await(inner) => is_call(&inner.base),
        Expr::Call(_) | Expr::MethodCall(_) | Expr::Macro(_) => true,
        _ => false,
    }
}
const fn merge(op: &BinOp) -> Option<&'static str> {
    match op {
        BinOp::BitAnd(_) | BinOp::BitAndAssign(_) => Some("&"),
        BinOp::And(_) => Some("&&"),
        BinOp::BitOr(_) | BinOp::BitOrAssign(_) => Some("|"),
        BinOp::Or(_) => Some("||"),
        _ => None,
    }
}
fn attributes(expr: &Expr) -> &[syn::Attribute] {
    expr_attrs(expr).map_or(&[], Vec::as_slice)
}
fn expr_attrs(expr: &Expr) -> Option<&Vec<syn::Attribute>> {
    Some(match expr {
        Expr::Call(e) => &e.attrs,
        Expr::MethodCall(e) => &e.attrs,
        Expr::Macro(e) => &e.attrs,
        Expr::Assign(e) => &e.attrs,
        Expr::Block(e) => &e.attrs,
        Expr::Unsafe(e) => &e.attrs,
        Expr::ForLoop(e) => &e.attrs,
        Expr::If(e) => &e.attrs,
        Expr::Match(e) => &e.attrs,
        Expr::Try(e) => &e.attrs,
        Expr::Await(e) => &e.attrs,
        Expr::Struct(e) => &e.attrs,
        Expr::Path(e) => &e.attrs,
        _ => return None,
    })
}
/// `#[cfg(..)]`, or a `cfg_attr` that applies one.
fn gates(attr: &syn::Attribute) -> bool {
    if attr.path().is_ident("cfg") {
        return true;
    }
    attr.path().is_ident("cfg_attr")
        && attr.meta.require_list().is_ok_and(|list| {
            let tokens = list.tokens.to_string();
            tokens.split_once(',').is_some_and(|(_, applied)| {
                applied
                    .split(|c: char| !c.is_alphanumeric() && c != '_')
                    .any(|word| word == "cfg")
            })
        })
}

/// Judge every `stage_order` pipeline row.
pub(in crate::checks) fn judge(analysis: &Analysis<'_>, orders: &[StageOrder]) -> Judgement {
    let mut out = Judgement::default();
    for order in orders {
        for (index, row) in order.pipeline.iter().enumerate() {
            let field = format!("stage_order.{}.pipeline.{index}", order.crate_name);
            judge_row(analysis, &order.crate_name, row, &field, &mut out);
        }
    }
    out
}
fn judge_row(
    analysis: &Analysis<'_>,
    crate_name: &str,
    row: &Pipeline,
    field: &str,
    out: &mut Judgement,
) {
    let at = format!("{field} ({}::{})", row.file, row.function);
    let snapshot = analysis.snapshot();
    let ours = snapshot
        .graph()
        .files
        .iter()
        .any(|file| file.path == row.file && file.crate_name == crate_name && !file.test_only);
    if !ours {
        out.violations.push(format!(
            "{at}: file `{}` is not a production source of package `{crate_name}`",
            row.file
        ));
        return;
    }
    let (ast, source) = match (analysis.syntax(&row.file), snapshot.source_facts(&row.file)) {
        (Ok(ast), Some(source)) => (ast, source),
        (Err(error), _) => {
            return out.unavailable.push(MissingInput {
                field: field.to_owned(),
                reason: error.to_string(),
            });
        }
        (_, None) => {
            return out.unavailable.push(MissingInput {
                field: field.to_owned(),
                reason: format!("captured facts absent for {}", row.file),
            });
        }
    };
    let selected = match crate::analysis::workflow::body(source, &row.file, ast, &row.function) {
        Ok(selected) => selected,
        Err(error) => return out.violations.push(format!("{at}: {error}")),
    };
    let mut composition = Composition {
        path: &row.file,
        loops: row.loops,
        reading: Reading::default(),
    };
    composition.body(selected.block);
    verdicts(
        &at,
        row,
        &selected.site,
        &composition.reading,
        &mut out.violations,
    );
}
/// Each refusal, unlisted call and listed call not made.
fn verdicts(at: &str, row: &Pipeline, function: &Site, reading: &Reading, out: &mut Vec<String>) {
    let file = &row.file;
    for (site, refusal) in &reading.refusals {
        let (message, fix) = refusal.explain();
        out.push(format!("{at} at {file}:{}: {message}; {fix}", site.line));
    }
    for (spelling, sites) in &reading.calls {
        if !row.calls.contains(spelling) {
            out.push(format!(
                "{at} at {file}:{}: calls `{spelling}`, which its row does not list; list it, or move it into a stage",
                sites[0].line
            ));
        }
    }
    for listed in row
        .calls
        .iter()
        .filter(|listed| !reading.calls.contains_key(*listed))
    {
        out.push(format!(
            "{at} at {file}:{}: lists `{listed}`, which its body does not call; remove it from `calls`",
            function.line
        ));
    }
}

/// The shape of a crate's pipeline rows, before any source is read.
pub(in crate::checks) fn validate(order: &StageOrder) -> crate::Result<()> {
    for (index, row) in order.pipeline.iter().enumerate() {
        let at = format!("policy: stage_order.{}.pipeline.{index}", order.crate_name);
        crate::architecture::relative_path(&row.file).map_err(|error| format!("{at}: {error}"))?;
        let function = row.function.trim();
        if function.is_empty() {
            return Err(format!("{at} needs a function").into());
        }
        if function
            .split(':')
            .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
        {
            return Err(format!(
                "{at} selects `{function}` by line; lines move, so name the function (`name`, `Type::method` or `<Type as Trait>::method`)"
            )
            .into());
        }
        let mut seen = std::collections::BTreeSet::new();
        for call in &row.calls {
            if !well_spelled(call) {
                return Err(format!(
                    "{at}: call `{call}` must be written as a path (`stage::run`), `.method` or `macro!`"
                )
                .into());
            }
            if !seen.insert(call.as_str()) {
                return Err(format!("{at} lists `{call}` twice").into());
            }
        }
    }
    Ok(())
}
fn well_spelled(call: &str) -> bool {
    let ident = |part: &str| {
        part.chars()
            .next()
            .is_some_and(|c| c.is_alphabetic() || c == '_')
            && part.chars().all(|c| c.is_alphanumeric() || c == '_')
    };
    let path = |text: &str| {
        text.strip_prefix("::")
            .unwrap_or(text)
            .split("::")
            .all(ident)
    };
    if let Some(method) = call.strip_prefix('.') {
        ident(method)
    } else if let Some(name) = call.strip_suffix('!') {
        name.split("::").all(ident)
    } else {
        path(call)
    }
}

#[cfg(test)]
mod tests;
