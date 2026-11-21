//! Calls and explicit bindings from the same AST as the ordered body projection.
use super::{Binding, Call, Declared, parse::Reader};
use std::collections::{BTreeMap, BTreeSet};
use syn::{Expr, FnArg, Item, Pat, Signature, spanned::Spanned, visit::Visit};

pub(crate) fn imports(
    items: &[Item],
    selection: crate::population::SyntaxSelection<'_>,
) -> BTreeMap<String, Vec<Vec<String>>> {
    bindings(items, selection, false)
}
/// Paths, relative to the file's module, of the types it declares: structs,
/// enums, unions, aliases and traits, through inline modules. An impl whose
/// owner is not among a workspace's declared types is for a foreign type.
pub(super) fn types(
    items: &[Item],
    module: &[String],
    selection: crate::population::SyntaxSelection<'_>,
    out: &mut super::ParsedFile,
) {
    for item in items.iter().filter(|item| selection.admits(item.span())) {
        let ident = match item {
            Item::Struct(item) => &item.ident,
            Item::Enum(item) => &item.ident,
            Item::Union(item) => &item.ident,
            Item::Trait(item) => &item.ident,
            Item::Type(alias) => {
                let mut path = module.to_vec();
                path.push(alias.ident.to_string());
                out.aliases.push(super::Alias {
                    path,
                    target: type_path(&alias.ty),
                    imports: imports(items, selection),
                });
                continue;
            }
            Item::Mod(item) => {
                if let Some((_, items)) = &item.content {
                    let mut child = module.to_vec();
                    child.push(item.ident.to_string());
                    types(items, &child, selection, out);
                }
                continue;
            }
            _ => continue,
        };
        let mut path = module.to_vec();
        path.push(ident.to_string());
        out.types.push(path);
    }
}
/// A type's path as written, without generic arguments, through references.
fn type_path(ty: &syn::Type) -> Option<Vec<String>> {
    match ty {
        syn::Type::Path(p) if p.qself.is_none() => Some(
            p.path
                .segments
                .iter()
                .map(|s| s.ident.to_string())
                .collect(),
        ),
        syn::Type::Reference(r) => type_path(&r.elem),
        syn::Type::Paren(p) => type_path(&p.elem),
        _ => None,
    }
}
/// A module's re-exports: every `pub` `use`, restricted ones (`pub(crate)`,
/// `pub(super)`, `pub(in path)`) included. Code that names an item through a
/// restricted re-export compiles only where the restriction allows it, so
/// following one never resolves a path the compiler would not; a private
/// `use` is an import, not an export.
pub(super) fn exports(
    items: &[Item],
    selection: crate::population::SyntaxSelection<'_>,
) -> BTreeMap<String, Vec<Vec<String>>> {
    bindings(items, selection, true)
}
fn bindings(
    items: &[Item],
    selection: crate::population::SyntaxSelection<'_>,
    exported_only: bool,
) -> BTreeMap<String, Vec<Vec<String>>> {
    let mut out = BTreeMap::<String, Vec<Vec<String>>>::new();
    for item in items {
        if let Item::Use(item) = item
            && selection.admits(item.span())
            && (!exported_only || !matches!(item.vis, syn::Visibility::Inherited))
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
) -> (Vec<Call>, Vec<Binding>, Vec<String>, Vec<Declared>) {
    let mut facts = Facts {
        reader,
        calls: vec![],
        bindings: vec![],
        items: BTreeSet::new(),
        declared: vec![],
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
    (
        facts.calls,
        facts.bindings,
        facts.items.into_iter().collect(),
        facts.declared,
    )
}
struct Facts<'a, 'b> {
    reader: &'a Reader<'b>,
    calls: Vec<Call>,
    bindings: Vec<Binding>,
    /// Names bound by items and `use` declarations inside the body.
    items: BTreeSet<String>,
    declared: Vec<Declared>,
}
/// The names a body-local item binds; `*` for a glob import. A macro
/// definition binds no call path.
fn item_names(item: &Item) -> Vec<String> {
    let ident = match item {
        Item::Use(item) => {
            return crate::scan::item_use_bindings(item)
                .into_iter()
                .map(|binding| binding.local_name)
                .filter(|name| name != "_")
                .collect();
        }
        Item::ForeignMod(block) => {
            return block
                .items
                .iter()
                .filter_map(|item| match item {
                    syn::ForeignItem::Fn(f) => Some(f.sig.ident.to_string()),
                    syn::ForeignItem::Static(s) => Some(s.ident.to_string()),
                    syn::ForeignItem::Type(t) => Some(t.ident.to_string()),
                    _ => None,
                })
                .collect();
        }
        Item::ExternCrate(item) => item.rename.as_ref().map_or(&item.ident, |(_, name)| name),
        Item::Fn(item) => &item.sig.ident,
        Item::Const(item) => &item.ident,
        Item::Static(item) => &item.ident,
        Item::Struct(item) => &item.ident,
        Item::Enum(item) => &item.ident,
        Item::Union(item) => &item.ident,
        Item::Type(item) => &item.ident,
        Item::Trait(item) => &item.ident,
        Item::TraitAlias(item) => &item.ident,
        Item::Mod(item) => &item.ident,
        _ => return Vec::new(),
    };
    vec![ident.to_string()]
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
    /// A closure's body is part of this function: its calls are this
    /// function's calls. Its parameters bind names inside it: a typed one
    /// with its type, an untyped one with none, so a method called on it
    /// stays unresolved rather than guessed.
    fn visit_expr_closure(&mut self, closure: &'ast syn::ExprClosure) {
        for input in &closure.inputs {
            if let Pat::Type(typed) = input
                && let Some(name) = pattern(&typed.pat)
            {
                self.bindings.push(Binding {
                    name,
                    ty: Some(self.reader.text(typed.ty.span())),
                    initializer: None,
                    site: self.reader.site(input.span()),
                });
            } else {
                self.visit_pat(input);
            }
        }
        self.visit_expr(&closure.body);
    }
    // An `async` block's body is part of this function too (the default
    // visit walks it), as a closure's is.
    fn visit_expr_macro(&mut self, _: &'ast syn::ExprMacro) {}
    fn visit_expr_const(&mut self, _: &'ast syn::ExprConst) {}
    /// A nested item's body is its own scope, but its name shadows the
    /// module's throughout the enclosing block.
    fn visit_item(&mut self, item: &'ast Item) {
        self.items.extend(item_names(item));
        // A function is named by its signature, not its whole body.
        let header = match item {
            Item::Fn(f) => f.sig.span(),
            _ => item.span(),
        };
        self.declared.push(Declared {
            label: self.reader.text(header),
            site: self.reader.site(item.span()),
        });
    }
    fn visit_arm(&mut self, arm: &'ast syn::Arm) {
        self.visit_pat(&arm.pat);
        self.visit_expr(&arm.body);
    }
}
