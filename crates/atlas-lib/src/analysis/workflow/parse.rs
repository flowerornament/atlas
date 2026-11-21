//! The Rust AST determines nesting; source spans supply labels and links.
use super::functions::collect;
use super::{Arm, Site, Step, Workflow};
use crate::Result;
use proc_macro2::Span;
use syn::{Expr, Stmt, spanned::Spanned};

#[cfg(test)]
pub(super) fn parse(source: &str, path: &str, selector: &str) -> Result<Workflow> {
    parse_many(source, path, &[selector])?
        .pop()
        .ok_or_else(|| "workflow result missing".into())
}
pub(super) fn parse_many(source: &str, path: &str, selectors: &[&str]) -> Result<Vec<Workflow>> {
    let ast =
        crate::source::parse_file(source).map_err(|e| format!("parse workflow {path}: {e}"))?;
    let reader = Reader {
        source,
        path,
        selection: crate::population::SyntaxSelection::All,
    };
    let mut functions = Vec::new();
    collect(&ast.items, &[], &reader, &mut functions);
    selectors
        .iter()
        .map(|selector| reader.select(&functions, selector))
        .collect()
}

pub(super) fn parse_all(source: &str, path: &str) -> Result<super::ParsedFile> {
    let ast =
        crate::source::parse_file(source).map_err(|e| format!("parse workflow {path}: {e}"))?;
    Ok(parse_ast(
        source,
        path,
        &ast,
        crate::population::SyntaxSelection::All,
    ))
}
pub(super) fn parse_ast(
    source: &str,
    path: &str,
    ast: &syn::File,
    selection: crate::population::SyntaxSelection<'_>,
) -> super::ParsedFile {
    let reader = Reader {
        source,
        path,
        selection,
    };
    let mut functions = Vec::new();
    collect(&ast.items, &[], &reader, &mut functions);
    let functions = functions
        .iter()
        .filter(|function| !function.default_body)
        .map(|function| reader.build(function))
        .collect();
    let mut parsed = super::ParsedFile {
        functions,
        exports: super::facts::exports(&ast.items, reader.selection),
        types: Vec::new(),
        aliases: Vec::new(),
        modules: Vec::new(),
    };
    super::facts::types(&ast.items, &[], reader.selection, &mut parsed);
    super::facts::modules(&ast.items, &[], reader.selection, &mut parsed.modules);
    parsed
}

pub(super) struct Reader<'a> {
    pub(super) source: &'a str,
    pub(super) path: &'a str,
    pub(super) selection: crate::population::SyntaxSelection<'a>,
}
impl Reader<'_> {
    pub(super) fn site(&self, span: Span) -> Site {
        Site::at(self.path, span)
    }

    pub(super) fn text(&self, span: Span) -> String {
        let text = crate::source_text::text(self.source, span);
        if text.chars().count() > 110 {
            format!("{}...", text.chars().take(107).collect::<String>())
        } else {
            text
        }
    }
    fn action(&self, expr: &Expr) -> Step {
        Step::Action {
            binding: None,
            label: self.text(expr.span()),
            fallible: false,
            site: self.site(expr.span()),
        }
    }
    fn opaque(&self, expr: &Expr) -> Step {
        Step::Opaque {
            label: self.text(expr.span()),
            fallible: false,
            site: self.site(expr.span()),
        }
    }
    pub(super) fn block(&self, block: &syn::Block) -> Vec<Step> {
        let mut out = Vec::new();
        for stmt in &block.stmts {
            let steps = self.statement(stmt);
            out.extend(self.cfg_gated(stmt, steps));
        }
        out
    }
    fn statement(&self, stmt: &Stmt) -> Vec<Step> {
        let mut out = Vec::new();
        match stmt {
            Stmt::Local(local) => {
                if let Some(init) = &local.init {
                    let mut steps = self.expr(&init.expr);
                    if let Some(step) = steps.last_mut() {
                        if let Step::Action { binding, .. } = step {
                            *binding = Some(self.text(local.pat.span()));
                        }
                        let label = step.label_mut();
                        *label = format!("{} = {label}", self.text(local.pat.span()));
                    }
                    out.extend(steps);
                    if let Some((_, diverge)) = &init.diverge {
                        out.push(Step::Branch {
                            label: "let-else (pattern does not match)".into(),
                            arms: vec![
                                Arm {
                                    label: "else".into(),
                                    steps: self.expr(diverge),
                                },
                                Arm {
                                    label: "pattern matches".into(),
                                    steps: vec![],
                                },
                            ],
                            site: self.site(local.span()),
                        });
                    }
                }
            }
            Stmt::Expr(expr, _) => out.extend(self.expr(expr)),
            Stmt::Macro(m) => out.push(Step::Opaque {
                label: self.text(m.span()),
                fallible: false,
                site: self.site(m.span()),
            }),
            // Declarations run nothing; Workflow::declarations lists them.
            Stmt::Item(_) => {}
        }
        out
    }
    /// A statement under `#[cfg(...)]` (or a `cfg_attr` that adds one) is
    /// compiled only where its predicate holds. The predicate is not
    /// evaluated: the steps become one arm of a branch named by the
    /// attributes as written, beside an empty arm.
    fn cfg_gated(&self, stmt: &Stmt, steps: Vec<Step>) -> Vec<Step> {
        let attrs = match stmt {
            Stmt::Local(local) => local.attrs.as_slice(),
            Stmt::Expr(expr, _) => expr_attrs(expr),
            Stmt::Macro(m) => m.attrs.as_slice(),
            Stmt::Item(_) => &[],
        };
        let gates: Vec<_> = attrs
            .iter()
            .filter(|attr| {
                attr.path().is_ident("cfg")
                    || (attr.path().is_ident("cfg_attr") && self.text(attr.span()).contains("cfg("))
            })
            .map(|attr| self.text(attr.span()))
            .collect();
        if gates.is_empty() || steps.is_empty() {
            return steps;
        }
        vec![Step::Branch {
            label: format!("{} (predicate not evaluated)", gates.join(" ")),
            arms: vec![
                Arm {
                    label: "compiled where the predicate holds".into(),
                    steps,
                },
                Arm {
                    label: "not compiled otherwise".into(),
                    steps: vec![],
                },
            ],
            site: self.site(stmt.span()),
        }]
    }
    fn operand(&self, expr: &Expr) -> Vec<Step> {
        if matches!(expr, Expr::Path(_) | Expr::Lit(_)) {
            vec![]
        } else {
            self.expr(expr)
        }
    }
    fn with_operand(&self, expr: &Expr, operand: &Expr) -> Vec<Step> {
        let mut steps = self.operand(operand);
        steps.push(self.action(expr));
        steps
    }
    /// An expression's steps, in evaluation order as written.
    fn expr(&self, expr: &Expr) -> Vec<Step> {
        let site = self.site(expr.span());
        match expr {
            Expr::Block(b) => match &b.label {
                None => self.block(&b.block),
                Some(label) => vec![Step::Block {
                    label: format!("'{}: block", label.name.ident),
                    body: self.block(&b.block),
                    target: label.name.ident.to_string(),
                    site,
                }],
            },
            Expr::Unsafe(e) => self.unsafe_block(e, site),
            Expr::Paren(e) => self.expr(&e.expr),
            Expr::Group(e) => self.expr(&e.expr),
            Expr::Try(e) => {
                let mut steps = self.expr(&e.expr);
                steps.push(Step::Action {
                    binding: None,
                    label: "propagate ?".into(),
                    fallible: true,
                    site,
                });
                steps
            }
            Expr::If(e) => self.if_expr(e, site),
            Expr::Match(e) => self.match_expr(e, site),
            Expr::ForLoop(_) | Expr::While(_) | Expr::Loop(_) => self.loop_expr(expr, site),
            Expr::Return(_) | Expr::Break(_) | Expr::Continue(_) => self.exit_expr(expr, site),
            Expr::Binary(e) if matches!(e.op, syn::BinOp::And(_) | syn::BinOp::Or(_)) => {
                self.short_circuit(expr, e, site)
            }
            Expr::Binary(e) if compound(&e.op) => self.compound_assign(expr, e, site),
            Expr::Call(e) => self.evaluated(expr, std::iter::once(&*e.func).chain(&e.args)),
            Expr::MethodCall(e) => {
                self.evaluated(expr, std::iter::once(&*e.receiver).chain(&e.args))
            }
            Expr::Binary(e) => self.evaluated(expr, [&*e.left, &*e.right]),
            Expr::Assign(e) => self.evaluated(expr, [&*e.right, &*e.left]),
            Expr::Reference(e) => {
                let mut steps = self.operand(&e.expr);
                steps.push(Step::Borrow {
                    label: self.text(expr.span()),
                    site,
                });
                steps
            }
            Expr::Unary(e) => self.with_operand(expr, &e.expr),
            Expr::Field(e) => self.with_operand(expr, &e.base),
            Expr::Index(e) => self.evaluated(expr, [&*e.expr, &*e.index]),
            Expr::Cast(e) => self.with_operand(expr, &e.expr),
            Expr::Struct(e) => self.evaluated(
                expr,
                e.fields.iter().map(|f| &f.expr).chain(e.rest.as_deref()),
            ),
            Expr::Tuple(e) => self.evaluated(expr, &e.elems),
            Expr::Array(e) => self.evaluated(expr, &e.elems),
            // The operand runs, then the await; when and how often the
            // future is polled is not shown.
            Expr::Await(e) => self.with_operand(expr, &e.base),
            // The element is evaluated once; the length is a constant.
            Expr::Repeat(e) => self.with_operand(expr, &e.expr),
            // A range evaluates its start, then its end.
            Expr::Range(e) => self.evaluated(
                expr,
                [&e.start, &e.end].into_iter().flatten().map(|end| &**end),
            ),
            Expr::Let(e) => self.with_operand(expr, &e.expr),
            Expr::Lit(_) | Expr::Path(_) => vec![self.action(expr)],
            _ => vec![self.opaque(expr)],
        }
    }
    /// Each operand in order, then the expression's own action.
    fn evaluated<'e>(
        &self,
        expr: &Expr,
        operands: impl IntoIterator<Item = &'e Expr>,
    ) -> Vec<Step> {
        let mut steps: Vec<Step> = operands.into_iter().flat_map(|x| self.operand(x)).collect();
        steps.push(self.action(expr));
        steps
    }
    /// The steps run as written; the marker says the compiler does not
    /// check the operations through the block's closing line.
    fn unsafe_block(&self, e: &syn::ExprUnsafe, site: Site) -> Vec<Step> {
        let mut steps = vec![Step::Action {
            binding: None,
            label: format!("unsafe block, through line {}", site.end_line),
            fallible: false,
            site,
        }];
        steps.extend(self.block(&e.block));
        steps
    }
    fn if_expr(&self, e: &syn::ExprIf, site: Site) -> Vec<Step> {
        let mut steps = self.operand(&e.cond);
        steps.push(Step::Branch {
            label: format!("if {}", self.text(e.cond.span())),
            arms: vec![
                Arm {
                    label: "then (true / pattern matches)".into(),
                    steps: self.block(&e.then_branch),
                },
                Arm {
                    label: "else (false / pattern does not match)".into(),
                    steps: e
                        .else_branch
                        .as_ref()
                        .map_or_else(Vec::new, |(_, x)| self.expr(x)),
                },
            ],
            site,
        });
        steps
    }
    fn match_expr(&self, e: &syn::ExprMatch, site: Site) -> Vec<Step> {
        let mut steps = self.operand(&e.expr);
        steps.push(Step::Match {
            label: format!("match {}", self.text(e.expr.span())),
            arms: e
                .arms
                .iter()
                .map(|arm| super::MatchArm {
                    pattern: self.text(match &arm.pat {
                        syn::Pat::Guard(g) => g.pat.span(),
                        p => p.span(),
                    }),
                    guard: match &arm.pat {
                        syn::Pat::Guard(g) => self.expr(&g.guard),
                        _ => vec![],
                    },
                    body: self.expr(&arm.body),
                })
                .collect(),
            site,
        });
        steps
    }
    /// A `for` (its iterator first), `while` (its condition as the header)
    /// or `loop`.
    fn loop_expr(&self, expr: &Expr, site: Site) -> Vec<Step> {
        let target = |label: &Option<syn::Label>| label.as_ref().map(|l| l.name.ident.to_string());
        match expr {
            Expr::ForLoop(e) => {
                let mut steps = self.operand(&e.expr);
                steps.push(Step::Loop {
                    label: format!(
                        "for {} in {}",
                        self.text(e.pat.span()),
                        self.text(e.expr.span())
                    ),
                    form: super::LoopKind::For,
                    head: vec![],
                    body: self.block(&e.body),
                    target: target(&e.label),
                    site,
                });
                steps
            }
            Expr::While(e) => vec![Step::Loop {
                label: format!("while {}", self.text(e.cond.span())),
                form: super::LoopKind::While,
                head: self.operand(&e.cond),
                body: self.block(&e.body),
                target: target(&e.label),
                site,
            }],
            Expr::Loop(e) => vec![Step::Loop {
                label: "loop".into(),
                form: super::LoopKind::Infinite,
                head: vec![],
                body: self.block(&e.body),
                target: target(&e.label),
                site,
            }],
            _ => vec![self.opaque(expr)],
        }
    }
    /// A `return` or `break` (its value first) or a `continue`.
    fn exit_expr(&self, expr: &Expr, site: Site) -> Vec<Step> {
        let label = |l: &Option<syn::Lifetime>| l.as_ref().map(|l| l.ident.to_string());
        let (value, control) = match expr {
            Expr::Return(e) => (e.expr.as_deref(), super::ExitKind::Return),
            Expr::Break(e) => (e.expr.as_deref(), super::ExitKind::Break(label(&e.label))),
            Expr::Continue(e) => (None, super::ExitKind::Continue(label(&e.label))),
            _ => return vec![self.opaque(expr)],
        };
        let mut steps = value.map_or_else(Vec::new, |x| self.operand(x));
        steps.push(Step::Exit {
            label: self.text(expr.span()),
            control,
            site,
        });
        steps
    }
    /// `&&` and `||`: the left operand, then a branch that evaluates or
    /// skips the right.
    fn short_circuit(&self, expr: &Expr, e: &syn::ExprBinary, site: Site) -> Vec<Step> {
        let mut steps = self.operand(&e.left);
        let is_and = matches!(e.op, syn::BinOp::And(_));
        steps.push(Step::Branch {
            label: format!("short-circuit {}", self.text(expr.span())),
            arms: vec![
                Arm {
                    label: format!(
                        "left {}: evaluate right",
                        if is_and { "true" } else { "false" }
                    ),
                    steps: self.expr(&e.right),
                },
                Arm {
                    label: format!("left {}: skip right", if is_and { "false" } else { "true" }),
                    steps: vec![],
                },
            ],
            site,
        });
        steps
    }
    /// Compound assignment. Built-in operators evaluate the right operand
    /// first, overloaded ones the left; the type is not inferred, so when
    /// both sides have steps, both orders are shown, and the operator's own
    /// effect is opaque.
    fn compound_assign(&self, expr: &Expr, e: &syn::ExprBinary, site: Site) -> Vec<Step> {
        let (left, right) = (self.operand(&e.left), self.operand(&e.right));
        let mut steps = if left.is_empty() || right.is_empty() {
            [right, left].concat()
        } else {
            vec![Step::Branch {
                label: format!(
                    "operand order of {} (depends on the operand types)",
                    self.text(expr.span())
                ),
                arms: vec![
                    Arm {
                        label: "built-in operator: right operand first".into(),
                        steps: [right.clone(), left.clone()].concat(),
                    },
                    Arm {
                        label: "overloaded operator: left operand first".into(),
                        steps: [left, right].concat(),
                    },
                ],
                site,
            }]
        };
        steps.push(self.opaque(expr));
        steps
    }
}

/// Whether `op` is a compound assignment, whose operand order depends on
/// the operand types.
const fn compound(op: &syn::BinOp) -> bool {
    matches!(
        op,
        syn::BinOp::AddAssign(_)
            | syn::BinOp::SubAssign(_)
            | syn::BinOp::MulAssign(_)
            | syn::BinOp::DivAssign(_)
            | syn::BinOp::RemAssign(_)
            | syn::BinOp::BitXorAssign(_)
            | syn::BinOp::BitAndAssign(_)
            | syn::BinOp::BitOrAssign(_)
            | syn::BinOp::ShlAssign(_)
            | syn::BinOp::ShrAssign(_)
    )
}

/// An expression's outer attributes, as written.
fn expr_attrs(expr: &Expr) -> &[syn::Attribute] {
    macro_rules! attrs {
        ($($variant:ident),* $(,)?) => {
            match expr {
                $(Expr::$variant(e) => &e.attrs,)*
                _ => &[],
            }
        };
    }
    attrs!(
        Array, Assign, Async, Await, Binary, Block, Break, Call, Cast, Closure, Const, Continue,
        Field, ForLoop, Group, If, Index, Infer, Let, Lit, Loop, Macro, Match, MethodCall, Paren,
        Path, Range, RawAddr, Reference, Repeat, Return, Struct, Try, TryBlock, Tuple, Unary,
        Unsafe, While, Yield,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(steps: &[Step]) -> Vec<String> {
        steps
            .iter()
            .map(|step| match step {
                Step::Action { label, .. } => label.clone(),
                Step::Opaque { label, .. } => format!("[opaque] {label}"),
                other => format!("{other:?}"),
            })
            .collect()
    }

    #[test]
    fn compound_assignment_shows_its_operands_and_leaves_their_order_open() {
        let flow = parse("fn f() { total += next()?; }", "lib.rs", "f").unwrap();
        assert_eq!(
            labels(&flow.steps),
            ["next()", "propagate ?", "[opaque] total += next()?"]
        );
        let flow = parse("fn f() { slot()[i()] *= next()?; }", "lib.rs", "f").unwrap();
        let Step::Branch { label, arms, .. } = &flow.steps[0] else {
            panic!("{:?}", flow.steps)
        };
        assert!(label.contains("depends on the operand types"), "{label}");
        assert_eq!(
            labels(&arms[0].steps),
            ["next()", "propagate ?", "slot()", "i()", "slot()[i()]"]
        );
        assert_eq!(
            labels(&arms[1].steps),
            ["slot()", "i()", "slot()[i()]", "next()", "propagate ?"]
        );
        assert_eq!(
            labels(&flow.steps[1..]),
            ["[opaque] slot()[i()] *= next()?"]
        );
    }

    #[test]
    fn body_local_items_are_declarations_not_steps() {
        let source = "fn f() {\n    use std::mem::swap as exchange;\n    work();\n    fn helper(x: u8) -> u8 { x }\n    if ready() { struct Inner; }\n}";
        let flow = parse(source, "lib.rs", "f").unwrap();
        assert_eq!(labels(&flow.steps[..1]), ["work()"]);
        let declared: Vec<_> = flow
            .declarations
            .iter()
            .map(|d| (d.label.as_str(), d.site.line))
            .collect();
        assert_eq!(
            declared,
            [
                ("use std::mem::swap as exchange;", 2),
                ("fn helper(x: u8) -> u8", 4),
                ("struct Inner;", 5),
            ]
        );
        assert!(!flow.ascii().contains("[opaque]"), "{}", flow.ascii());
        assert!(
            flow.ascii()
                .contains("declares (not a step): fn helper(x: u8) -> u8 [lib.rs:4]")
        );
        let notes = flow.diagram().notes;
        assert!(
            notes
                .iter()
                .any(|n| n.contains("f declares struct Inner; at lib.rs:5")),
            "{notes:?}"
        );
    }

    #[test]
    fn an_await_shows_its_operand_then_the_await() {
        let flow = parse(
            "async fn f() { let x = fetch(id()).await?; done(x); }",
            "lib.rs",
            "f",
        )
        .unwrap();
        assert_eq!(
            labels(&flow.steps),
            [
                "id()",
                "fetch(id())",
                "fetch(id()).await",
                "x = propagate ?",
                "done(x)"
            ]
        );
    }

    #[test]
    fn an_array_repeat_evaluates_its_element_once() {
        let flow = parse("fn f() { fill([seed(); 4]); }", "lib.rs", "f").unwrap();
        assert_eq!(
            labels(&flow.steps),
            ["seed()", "[seed(); 4]", "fill([seed(); 4])"]
        );
    }

    #[test]
    fn an_unsafe_block_shows_its_steps_between_markers() {
        let flow = parse(
            "fn f() { let x = unsafe { read(p()) }; use_it(x); }",
            "lib.rs",
            "f",
        )
        .unwrap();
        assert_eq!(
            labels(&flow.steps),
            [
                "unsafe block, through line 1",
                "p()",
                "x = read(p())",
                "use_it(x)"
            ]
        );
    }

    #[test]
    fn a_cfg_statement_is_conditional_on_its_predicate_as_written() {
        let source = "fn f() {\n    #[cfg(unix)]\n    unsafe { signal(); }\n    #[cfg(not(test))]\n    let x = load();\n    #[cfg_attr(feature = \"slow\", cfg(any()))]\n    trace();\n    #[allow(unused)]\n    plain();\n}";
        let flow = parse(source, "lib.rs", "f").unwrap();
        let gated: Vec<_> = flow
            .steps
            .iter()
            .filter_map(|step| match step {
                Step::Branch { label, arms, .. } => Some((
                    label.as_str(),
                    labels(&arms[0].steps),
                    arms[1].steps.is_empty(),
                )),
                _ => None,
            })
            .collect();
        assert_eq!(gated.len(), 3, "{:?}", flow.steps);
        assert_eq!(gated[0].0, "#[cfg(unix)] (predicate not evaluated)");
        assert_eq!(gated[0].1[0], "unsafe block, through line 3");
        assert_eq!(gated[1].0, "#[cfg(not(test))] (predicate not evaluated)");
        assert_eq!(gated[1].1, ["x = load()"]);
        assert!(
            gated[2]
                .0
                .starts_with("#[cfg_attr(feature = \"slow\", cfg(any()))]")
        );
        assert!(gated.iter().all(|(_, _, empty)| *empty));
        // Other attributes leave a statement as it is (its label is its
        // written text, attributes included).
        assert_eq!(labels(&flow.steps[3..]), ["#[allow(unused)] plain()"]);
        assert!(
            flow.ascii().contains("| not compiled otherwise"),
            "{}",
            flow.ascii()
        );
    }

    #[test]
    fn a_range_evaluates_its_start_then_its_end() {
        let source = "fn f() { take(start()..end()); take(..=end()); take(start()..); }";
        let flow = parse(source, "lib.rs", "f").unwrap();
        assert_eq!(
            labels(&flow.steps),
            [
                "start()",
                "end()",
                "start()..end()",
                "take(start()..end())",
                "end()",
                "..=end()",
                "take(..=end())",
                "start()",
                "start()..",
                "take(start()..)",
            ]
        );
    }
    #[test]
    fn retains_order_fallibility_branches_loops_and_sites() {
        let source = "fn build() {\n let x = parse()?;\n for item in values() {\n match check(item) { Ok(v) => save(v), Err(e) => return fail(e) }\n }\n finish(x);\n}";
        let flow = parse(source, "src/lib.rs", "build").unwrap();
        assert!(
            matches!(&flow.steps[0],Step::Action{label,fallible:false,site,..} if label=="parse()" && site.line==2 && site.column==9)
        );
        let Step::Loop { body, .. } = &flow.steps[3] else {
            panic!("loop")
        };
        let Step::Match { arms, .. } = &body[1] else {
            panic!("match")
        };
        assert_eq!(arms.len(), 2);
        assert!(matches!(arms[1].body.last(), Some(Step::Exit { .. })));
        assert!(flow.ascii().find("save(v)").unwrap() < flow.ascii().find("finish(x)").unwrap());
    }
    #[test]
    fn deferred_and_macro_bodies_are_opaque() {
        let flow = parse(
            "fn f(){let later=|| hidden(); launch!{ secret() };}",
            "lib.rs",
            "f",
        )
        .unwrap();
        assert_eq!(flow.steps.len(), 2);
        assert!(flow.steps.iter().all(|s| matches!(s, Step::Opaque { .. })));
    }
    #[test]
    fn ambiguous_function_names_refuse() {
        let src = "mod one {fn go(){}} mod two {fn go(){}}";
        assert!(parse(src, "lib.rs", "go").is_err());
        assert_eq!(parse(src, "lib.rs", "one::go").unwrap().function, "one::go");
    }
    #[test]
    fn short_circuit_does_not_flatten_conditional_call() {
        let flow = parse("fn f(){let x = ready() && expensive();}", "lib.rs", "f").unwrap();
        assert!(
            flow.steps
                .iter()
                .any(|s| matches!(s,Step::Branch{label, ..} if label.contains("short-circuit")))
        );
    }
    #[test]
    fn unicode_before_call_has_valid_source_span() {
        let flow = parse("fn f(){let café = get(); done(café);}", "lib.rs", "f").unwrap();
        assert!(flow.ascii().contains("café = get()"));
    }
    #[test]
    fn call_argument_deferred_body_is_explicitly_opaque() {
        let flow = parse("fn f(){items.map(|x| hidden(x));}", "lib.rs", "f").unwrap();
        assert!(flow.steps.iter().any(|s| matches!(s, Step::Opaque { .. })));
    }
    #[test]
    fn nested_question_mark_retains_possible_return() {
        let flow = parse("fn f(){consume(read()?);}", "lib.rs", "f").unwrap();
        assert!(matches!(
            &flow.steps[1],
            Step::Action { fallible: true, .. }
        ));
    }
    #[test]
    fn opaque_fallible_initializer_keeps_its_binding() {
        let flow = parse("fn f(){let x = read().map_err(|e| e)?;}", "lib.rs", "f").unwrap();
        assert!(flow.steps.iter().any(|s| matches!(s, Step::Opaque { .. })));
        assert!(
            matches!(flow.steps.last(),Some(Step::Action{label,fallible:true,..}) if label=="x = propagate ?")
        );
    }
}

#[cfg(test)]
mod selector_tests {
    #[test]
    fn batch_selection_preserves_requested_order() {
        let flows = super::parse_many(
            "fn first(){a();} fn second(){b();}",
            "lib.rs",
            &["second", "first"],
        )
        .unwrap();
        assert_eq!(
            flows
                .iter()
                .map(|f| f.function.as_str())
                .collect::<Vec<_>>(),
            ["second", "first"]
        );
    }
    #[test]
    fn qualified_selector_is_exact_even_with_nested_suffix() {
        let source = "mod one {fn go(){}} mod outer {mod one {fn go(){}}}";
        assert_eq!(
            super::parse(source, "lib.rs", "one::go").unwrap().function,
            "one::go"
        );
        assert!(super::parse(source, "lib.rs", "go").is_err());
        assert!(super::parse("mod outer {mod one {fn go(){}}}", "lib.rs", "one::go").is_err());
    }
}
