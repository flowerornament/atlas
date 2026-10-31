//! Calls and explicit bindings from the same AST as the ordered body projection.
use super::{Binding, Call, parse::Reader};
use std::collections::BTreeMap;
use syn::{Expr, FnArg, Item, Pat, Signature, spanned::Spanned, visit::Visit};

pub(crate) fn imports(
    items: &[Item],
    selection: crate::population::SyntaxSelection<'_>,
) -> BTreeMap<String, Vec<Vec<String>>> {
    bindings(items, selection, false)
}
pub(super) fn exports(
    items: &[Item],
    selection: crate::population::SyntaxSelection<'_>,
) -> BTreeMap<String, Vec<Vec<String>>> {
    bindings(items, selection, true)
}
fn bindings(
    items: &[Item],
    selection: crate::population::SyntaxSelection<'_>,
    public_only: bool,
) -> BTreeMap<String, Vec<Vec<String>>> {
    let mut out = BTreeMap::<String, Vec<Vec<String>>>::new();
    for item in items {
        if let Item::Use(item) = item
            && selection.admits(item.span())
            && (!public_only || matches!(item.vis, syn::Visibility::Public(_)))
        {
            for binding in crate::scan::item_use_bindings(item) {
                // A glob remains an unresolved import; qualification never expands it.
                let path = binding
                    .source_path
                    .split("::")
                    .filter(|part| *part != "*")
                    .map(str::to_owned)
                    .collect();
                out.entry(binding.local_name).or_default().push(path);
            }
        }
    }
    out
}
fn pattern(pat: &Pat) -> Option<String> {
    match pat {
        Pat::Ident(i) => Some(i.ident.to_string()),
        Pat::Type(t) => pattern(&t.pat),
        _ => None,
    }
}
pub(super) fn body(
    reader: &Reader<'_>,
    signature: &Signature,
    block: &syn::Block,
) -> (Vec<Call>, Vec<Binding>) {
    let mut facts = Facts {
        reader,
        calls: vec![],
        bindings: vec![],
    };
    for arg in &signature.inputs {
        if let FnArg::Typed(arg) = arg
            && let Some(name) = pattern(&arg.pat)
        {
            facts.bindings.push(Binding {
                name,
                ty: Some(reader.text(arg.ty.span())),
                initializer: None,
                site: reader.site(arg.span()),
            });
        } else if let FnArg::Typed(arg) = arg {
            facts.visit_pat(&arg.pat);
        }
    }
    facts.visit_block(block);
    (facts.calls, facts.bindings)
}
struct Facts<'a, 'b> {
    reader: &'a Reader<'b>,
    calls: Vec<Call>,
    bindings: Vec<Binding>,
}
fn initializer(expr: &Expr) -> Option<&Expr> {
    match expr {
        Expr::Paren(e) => initializer(&e.expr),
        Expr::Reference(e) => initializer(&e.expr),
        Expr::Call(_) => Some(expr),
        _ => None,
    }
}
impl<'ast> Visit<'ast> for Facts<'_, '_> {
    fn visit_expr_assign(&mut self, assignment: &'ast syn::ExprAssign) {
        // An overwritten constructor no longer establishes the receiver's type.
        if let Expr::Path(left) = assignment.left.as_ref()
            && let Some(name) = left.path.get_ident()
        {
            self.bindings.push(Binding {
                name: name.to_string(),
                ty: None,
                initializer: None,
                site: self.reader.site(assignment.span()),
            });
        }
        syn::visit::visit_expr_assign(self, assignment);
    }

    fn visit_local(&mut self, local: &'ast syn::Local) {
        if let Some(name) = pattern(&local.pat) {
            let ty = if let Pat::Type(p) = &local.pat {
                Some(self.reader.text(p.ty.span()))
            } else {
                None
            };
            let call = local
                .init
                .as_ref()
                .and_then(|i| initializer(&i.expr))
                .and_then(|e| {
                    if let Expr::Call(c) = e {
                        Some(self.reader.text(c.func.span()))
                    } else {
                        None
                    }
                });
            self.bindings.push(Binding {
                name,
                ty,
                initializer: call,
                site: self.reader.site(local.span()),
            });
        } else {
            self.visit_pat(&local.pat);
        }
        if let Some(init) = &local.init {
            self.visit_expr(&init.expr);
            if let Some((_, diverge)) = &init.diverge {
                self.visit_expr(diverge);
            }
        }
    }
    fn visit_pat_ident(&mut self, pat: &'ast syn::PatIdent) {
        self.bindings.push(Binding {
            name: pat.ident.to_string(),
            ty: None,
            initializer: None,
            site: self.reader.site(pat.span()),
        });
        syn::visit::visit_pat_ident(self, pat);
    }
    fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
        self.visit_expr(&call.func);
        for arg in &call.args {
            self.visit_expr(arg);
        }
        self.calls.push(Call {
            spelling: self.reader.text(call.func.span()),
            receiver: None,
            arguments: call
                .args
                .iter()
                .map(|a| self.reader.text(a.span()))
                .collect(),
            site: self.reader.site(call.span()),
        });
    }
    fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
        self.visit_expr(&call.receiver);
        for arg in &call.args {
            self.visit_expr(arg);
        }
        self.calls.push(Call {
            spelling: call.method.to_string(),
            receiver: Some(self.reader.text(call.receiver.span())),
            arguments: call
                .args
                .iter()
                .map(|a| self.reader.text(a.span()))
                .collect(),
            site: self.reader.site(call.span()),
        });
    }
    fn visit_expr_closure(&mut self, _: &'ast syn::ExprClosure) {}
    fn visit_expr_async(&mut self, _: &'ast syn::ExprAsync) {}
    fn visit_expr_macro(&mut self, _: &'ast syn::ExprMacro) {}
    fn visit_expr_const(&mut self, _: &'ast syn::ExprConst) {}
    fn visit_item(&mut self, _: &'ast Item) {}
    fn visit_arm(&mut self, arm: &'ast syn::Arm) {
        self.visit_pat(&arm.pat);
        self.visit_expr(&arm.body);
    }
}
