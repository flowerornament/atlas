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
/// The names each module binds for a glob of it, through inline modules.
/// Items a macro writes are not seen, so a glob can bind a name such an
/// item would shadow.
pub(super) fn modules(
    items: &[Item],
    module: &[String],
    selection: crate::population::SyntaxSelection<'_>,
    out: &mut Vec<super::ModuleNames>,
) {
    let mut names = super::ModuleNames {
        path: module.to_vec(),
        items: BTreeMap::new(),
        imports: imports(items, selection),
    };
    for item in items {
        if !selection.admits(item.span()) {
            continue;
        }
        let (ident, vis) = match item {
            Item::Fn(f) => (&f.sig.ident, &f.vis),
            Item::Struct(s) => (&s.ident, &s.vis),
            Item::Enum(e) => (&e.ident, &e.vis),
            Item::Union(u) => (&u.ident, &u.vis),
            Item::Trait(t) => (&t.ident, &t.vis),
            Item::Type(t) => (&t.ident, &t.vis),
            Item::Const(c) => (&c.ident, &c.vis),
            Item::Static(s) => (&s.ident, &s.vis),
            Item::Mod(m) => {
                if let Some((_, inner)) = &m.content {
                    let mut child = module.to_vec();
                    child.push(m.ident.to_string());
                    modules(inner, &child, selection, out);
                }
                (&m.ident, &m.vis)
            }
            _ => continue,
        };
        names
            .items
            .insert(ident.to_string(), matches!(vis, syn::Visibility::Inherited));
    }
    out.push(names);
}
/// A type's path as written, without generic arguments, through references.
/// A path's segment names, as the index reads them. A leading `::` is kept
/// as an empty first segment, as a `use` binding's source path splits: the
/// segment after it names a crate, never a module in scope.
pub(crate) fn path_segments(path: &syn::Path) -> Vec<String> {
    let root = path.leading_colon.map(|_| String::new());
    root.into_iter()
        .chain(path.segments.iter().map(|s| s.ident.to_string()))
        .collect()
}
/// A type's path segments, through references and parentheses.
pub(crate) fn type_path(ty: &syn::Type) -> Option<Vec<String>> {
    match ty {
        syn::Type::Path(p) if p.qself.is_none() => Some(path_segments(&p.path)),
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
                // A glob is kept under `*`, its module's path; the index
                // follows it only into workspace modules.
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
        // Parameters are visible in the whole body.
        region: Some(reader.site(block.span())),
    };
    for arg in &signature.inputs {
        if let FnArg::Typed(arg) = arg
            && let Some(name) = pattern(&arg.pat)
        {
            facts.bind(
                name,
                Some(reader.text(arg.ty.span())),
                None,
                reader.site(arg.span()),
            );
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
    /// Where a name bound now is visible: the innermost block, closure
    /// body, arm or branch being read.
    region: Option<super::Site>,
}
impl Facts<'_, '_> {
    fn bind(
        &mut self,
        name: String,
        ty: Option<String>,
        initializer: Option<String>,
        site: super::Site,
    ) {
        self.bindings.push(Binding {
            name,
            ty,
            initializer,
            site,
            scope: self.region.clone(),
        });
    }
    /// Read `visit` with names bound in it visible only within `span`.
    fn within(&mut self, span: proc_macro2::Span, visit: impl FnOnce(&mut Self)) {
        let outer = self.region.replace(self.reader.site(span));
        visit(self);
        self.region = outer;
    }
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
            self.bind(
                name.to_string(),
                None,
                None,
                self.reader.site(assignment.span()),
            );
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
            self.bind(name, ty, call, self.reader.site(local.span()));
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
        self.bind(
            pat.ident.to_string(),
            None,
            None,
            self.reader.site(pat.span()),
        );
        syn::visit::visit_pat_ident(self, pat);
    }
    fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
        self.visit_expr(&call.func);
        for arg in &call.args {
            self.visit_expr(arg);
        }
        if !self.reader.selection.admits(call.span()) {
            return;
        }
        self.calls.push(Call {
            test_only: self.reader.selection.test_site(call.span()),
            spelling: self.reader.text(call.func.span()),
            method: None,
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
        if !self.reader.selection.admits(call.span()) {
            return;
        }
        self.calls.push(Call {
            test_only: self.reader.selection.test_site(call.span()),
            spelling: call.method.to_string(),
            method: Some(self.reader.site(call.method.span())),
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
        self.within(closure.body.span(), |facts| {
            for input in &closure.inputs {
                if let Pat::Type(typed) = input
                    && let Some(name) = pattern(&typed.pat)
                {
                    let ty = facts.reader.text(typed.ty.span());
                    let site = facts.reader.site(input.span());
                    facts.bind(name, Some(ty), None, site);
                } else {
                    facts.visit_pat(input);
                }
            }
            facts.visit_expr(&closure.body);
        });
    }
    /// A block's `let` bindings are visible to its end, and no further.
    fn visit_block(&mut self, block: &'ast syn::Block) {
        self.within(block.span(), |facts| syn::visit::visit_block(facts, block));
    }
    /// `if let` (and a `let` chain) binds in the then branch only.
    fn visit_expr_if(&mut self, expr: &'ast syn::ExprIf) {
        self.within(expr.then_branch.span(), |facts| {
            facts.visit_expr(&expr.cond);
        });
        self.visit_block(&expr.then_branch);
        if let Some((_, otherwise)) = &expr.else_branch {
            self.visit_expr(otherwise);
        }
    }
    fn visit_expr_while(&mut self, expr: &'ast syn::ExprWhile) {
        self.within(expr.body.span(), |facts| facts.visit_expr(&expr.cond));
        self.visit_block(&expr.body);
    }
    fn visit_expr_for_loop(&mut self, expr: &'ast syn::ExprForLoop) {
        self.visit_expr(&expr.expr);
        self.within(expr.body.span(), |facts| facts.visit_pat(&expr.pat));
        self.visit_block(&expr.body);
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
        self.within(arm.span(), |facts| {
            facts.visit_pat(&arm.pat);
            facts.visit_expr(&arm.body);
        });
    }
}
