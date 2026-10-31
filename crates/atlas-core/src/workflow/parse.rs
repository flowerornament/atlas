//! The Rust AST determines nesting; source spans supply labels and links.
use super::{Arm, Site, Step, Workflow};
use crate::Result;
use proc_macro2::Span;
use syn::{Expr, Item, Stmt, spanned::Spanned};

#[cfg(test)]
pub(super) fn parse(source: &str, path: &str, selector: &str) -> Result<Workflow> {
    parse_many(source, path, &[selector])?
        .pop()
        .ok_or_else(|| "workflow result missing".into())
}
pub(super) fn parse_many(source: &str, path: &str, selectors: &[&str]) -> Result<Vec<Workflow>> {
    let ast = syn::parse_file(source).map_err(|e| format!("parse workflow {path}: {e}"))?;
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
    let ast = syn::parse_file(source).map_err(|e| format!("parse workflow {path}: {e}"))?;
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
        .map(|function| reader.build(function))
        .collect();
    super::ParsedFile {
        functions,
        exports: super::facts::exports(&ast.items, reader.selection),
    }
}

struct Function<'a> {
    name: String,
    module: Vec<String>,
    owner: Option<String>,
    parameters: Vec<String>,
    imports: std::collections::BTreeMap<String, Vec<Vec<String>>>,
    signature: &'a syn::Signature,
    body: &'a syn::Block,
}
fn collect<'a>(
    items: &'a [Item],
    module: &[String],
    reader: &Reader<'_>,
    out: &mut Vec<Function<'a>>,
) {
    let imports = super::facts::imports(items, reader.selection);
    let prefix = if module.is_empty() {
        String::new()
    } else {
        format!("{}::", module.join("::"))
    };
    for item in items {
        if !reader.selection.admits(item.span()) {
            continue;
        }
        match item {
            Item::Fn(f) => out.push(Function {
                name: format!("{prefix}{}", f.sig.ident),
                module: module.to_vec(),
                owner: None,
                parameters: f
                    .sig
                    .generics
                    .type_params()
                    .map(|p| p.ident.to_string())
                    .collect(),
                imports: imports.clone(),
                signature: &f.sig,
                body: &f.block,
            }),
            Item::Mod(m) => {
                if let Some((_, items)) = &m.content {
                    let mut child = module.to_vec();
                    child.push(m.ident.to_string());
                    collect(items, &child, reader, out);
                }
            }
            Item::Impl(i) => {
                let owner = reader.text(i.self_ty.span());
                for member in &i.items {
                    if let syn::ImplItem::Fn(f) = member
                        && reader.selection.admits(f.span())
                    {
                        out.push(Function {
                            name: format!("{prefix}{owner}::{}", f.sig.ident),
                            module: module.to_vec(),
                            owner: Some(owner.clone()),
                            parameters: i
                                .generics
                                .type_params()
                                .chain(f.sig.generics.type_params())
                                .map(|p| p.ident.to_string())
                                .collect(),
                            imports: imports.clone(),
                            signature: &f.sig,
                            body: &f.block,
                        });
                    }
                }
            }
            _ => {}
        }
    }
}
pub(super) struct Reader<'a> {
    pub(super) source: &'a str,
    pub(super) path: &'a str,
    selection: crate::population::SyntaxSelection<'a>,
}
impl Reader<'_> {
    fn select(&self, functions: &[Function<'_>], selector: &str) -> Result<Workflow> {
        let found: Vec<_> = functions
            .iter()
            .filter(|f| {
                f.name == selector
                    || (!selector.contains("::") && f.name.rsplit("::").next() == Some(selector))
            })
            .collect();
        let [function] = found.as_slice() else {
            return Err(format!("workflow {}::{selector}: expected one function, found {}; use its qualified module/impl name",self.path,found.len()).into());
        };
        Ok(self.build(function))
    }
    fn build(&self, function: &Function<'_>) -> Workflow {
        let (calls, bindings) = super::facts::body(self, function.signature, function.body);
        Workflow {
            schema: 2,
            function: function.name.clone(),
            module: function.module.clone(),
            owner: function.owner.clone(),
            type_parameters:function.parameters.clone(),
            site: self.site(function.signature.span()),
            steps: self.block(function.body),
            calls, bindings, imports: function.imports.clone(),
            output: match &function.signature.output { syn::ReturnType::Default => "()".into(), syn::ReturnType::Type(_, ty) => self.text(ty.span()) },
            limits: [
                "Calls stay at their call sites; names and dispatch are not resolved or expanded.",
                "Operands are evaluated before their enclosing operation; labels over 110 characters are shortened, source spans remain exact.",
                "Branches and loops show syntax, not feasibility, iteration counts or a control-flow proof.",
                "Each ? is a distinct residual decision after its operand, including conditions and return expressions.",
                "Macros, closures, labelled blocks, deferred bodies and unsupported expressions are opaque; their effects are not inferred.",
            ].into_iter().map(str::to_owned).collect(),
        }
    }
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
    fn block(&self, block: &syn::Block) -> Vec<Step> {
        let mut out = Vec::new();
        for stmt in &block.stmts {
            match stmt {
                Stmt::Local(local) => {
                    if let Some(init) = &local.init {
                        let mut steps = self.expr(&init.expr);
                        if let Some(step) = steps.last_mut() {
                            if let Step::Action { binding, .. } = step {
                                *binding = Some(self.text(local.pat.span()));
                            }
                            let label = match step {
                                Step::Borrow { label, .. }
                                | Step::Action { label, .. }
                                | Step::Match { label, .. }
                                | Step::Branch { label, .. }
                                | Step::Loop { label, .. }
                                | Step::Exit { label, .. }
                                | Step::Opaque { label, .. } => label,
                            };
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
                Stmt::Item(item) => out.push(Step::Opaque {
                    label: format!("local declaration: {}", self.text(item.span())),
                    fallible: false,
                    site: self.site(item.span()),
                }),
            }
        }
        out
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
    fn expr(&self, expr: &Expr) -> Vec<Step> {
        let site = self.site(expr.span());
        match expr {
            Expr::Block(b) if b.label.is_none() => self.block(&b.block),
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
            Expr::If(e) => {
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
            Expr::Match(e) => {
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
                    target: e.label.as_ref().map(|l| l.name.ident.to_string()),
                    site,
                });
                steps
            }
            Expr::While(e) => vec![Step::Loop {
                label: format!("while {}", self.text(e.cond.span())),
                form: super::LoopKind::While,
                head: self.operand(&e.cond),
                body: self.block(&e.body),
                target: e.label.as_ref().map(|l| l.name.ident.to_string()),
                site,
            }],
            Expr::Loop(e) => vec![Step::Loop {
                label: "loop".into(),
                form: super::LoopKind::Infinite,
                head: vec![],
                body: self.block(&e.body),
                target: e.label.as_ref().map(|l| l.name.ident.to_string()),
                site,
            }],
            Expr::Return(e) => {
                let mut steps = e.expr.as_ref().map_or_else(Vec::new, |x| self.operand(x));
                steps.push(Step::Exit {
                    label: self.text(expr.span()),
                    control: super::ExitKind::Return,
                    site,
                });
                steps
            }
            Expr::Break(e) => {
                let mut steps = e.expr.as_ref().map_or_else(Vec::new, |x| self.operand(x));
                steps.push(Step::Exit {
                    label: self.text(expr.span()),
                    control: super::ExitKind::Break(e.label.as_ref().map(|l| l.ident.to_string())),
                    site,
                });
                steps
            }
            Expr::Continue(e) => vec![Step::Exit {
                label: self.text(expr.span()),
                control: super::ExitKind::Continue(
                    e.label.as_ref().map(|label| label.ident.to_string()),
                ),
                site,
            }],
            Expr::Binary(e) if matches!(e.op, syn::BinOp::And(_) | syn::BinOp::Or(_)) => {
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
                            label: format!(
                                "left {}: skip right",
                                if is_and { "false" } else { "true" }
                            ),
                            steps: vec![],
                        },
                    ],
                    site,
                });
                steps
            }
            Expr::Call(e) => {
                let mut steps = self.operand(&e.func);
                for arg in &e.args {
                    steps.extend(self.operand(arg));
                }
                steps.push(self.action(expr));
                steps
            }
            Expr::MethodCall(e) => {
                let mut steps = self.operand(&e.receiver);
                for arg in &e.args {
                    steps.extend(self.operand(arg));
                }
                steps.push(self.action(expr));
                steps
            }
            Expr::Binary(e) => {
                // Compound assignment order depends on type/overloading, which this reader does not infer.
                if matches!(
                    e.op,
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
                ) {
                    return vec![self.opaque(expr)];
                }
                let mut steps = self.operand(&e.left);
                steps.extend(self.operand(&e.right));
                steps.push(self.action(expr));
                steps
            }
            Expr::Assign(e) => {
                let mut steps = self.operand(&e.right);
                steps.extend(self.operand(&e.left));
                steps.push(self.action(expr));
                steps
            }
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
            Expr::Index(e) => {
                let mut steps = self.operand(&e.expr);
                steps.extend(self.operand(&e.index));
                steps.push(self.action(expr));
                steps
            }
            Expr::Cast(e) => self.with_operand(expr, &e.expr),
            Expr::Struct(e) => {
                let mut steps = Vec::new();
                for f in &e.fields {
                    steps.extend(self.operand(&f.expr));
                }
                if let Some(rest) = &e.rest {
                    steps.extend(self.operand(rest));
                }
                steps.push(self.action(expr));
                steps
            }
            Expr::Tuple(e) => {
                let mut steps = Vec::new();
                for x in &e.elems {
                    steps.extend(self.operand(x));
                }
                steps.push(self.action(expr));
                steps
            }
            Expr::Array(e) => {
                let mut steps = Vec::new();
                for x in &e.elems {
                    steps.extend(self.operand(x));
                }
                steps.push(self.action(expr));
                steps
            }
            Expr::Let(e) => self.with_operand(expr, &e.expr),
            Expr::Lit(_) | Expr::Path(_) => vec![self.action(expr)],
            _ => vec![self.opaque(expr)],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
