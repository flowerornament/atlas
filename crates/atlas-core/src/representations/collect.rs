//! Written structs, enums, signatures and immediate literal initializers.
use super::types::{Constant, Literal};
use proc_macro2::Span;
use std::collections::{BTreeMap, BTreeSet};
use syn::{spanned::Spanned, visit::Visit};
#[derive(Clone)]
pub(super) struct Record {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) file: String,
    pub(super) module: Vec<String>,
    pub(super) imports: BTreeMap<String, Vec<Vec<String>>>,
    pub(super) line: usize,
    pub(super) body_end_line: usize,
    pub(super) column: usize,
    pub(super) parameters: BTreeSet<String>,
    pub(super) public: bool,
    pub(super) trait_impl: bool,
    pub(super) signature: Option<syn::Signature>,
    pub(super) owner: Option<syn::Type>,
    pub(super) fields: Vec<(String, syn::Type)>,
    pub(super) text: String,
}
pub(super) struct Collector<'a> {
    pub(super) file: &'a str,
    pub(super) source: &'a str,
    pub(super) selection: crate::population::SyntaxSelection<'a>,
    pub(super) trait_impl: bool,
    pub(super) parameters: BTreeSet<String>,
    pub(super) module: Vec<String>,
    pub(super) imports: BTreeMap<String, Vec<Vec<String>>>,
    pub(super) owner: Option<syn::Type>,
    pub(super) function: Option<String>,
    pub(super) records: Vec<Record>,
    pub(super) constants: Vec<Constant>,
    pub(super) constructors: Vec<(String, syn::Path, Option<syn::Type>, usize)>,
}
impl Collector<'_> {
    fn text(&self, span: Span) -> String {
        crate::source_text::text(self.source, span)
    }
    fn record(
        &mut self,
        name: String,
        span: Span,
        sig: Option<syn::Signature>,
        fields: Vec<(String, syn::Type)>,
    ) -> String {
        let p = span.start();
        let id = format!("{}:{}:{}", self.file, p.line, p.column);
        self.records.push(Record {
            id: id.clone(),
            name,
            file: self.file.into(),
            module: self.module.clone(),
            imports: self.imports.clone(),
            line: p.line,
            body_end_line: span.end().line,
            column: p.column,
            parameters: self
                .parameters
                .iter()
                .cloned()
                .chain(
                    sig.iter()
                        .flat_map(|s| s.generics.type_params().map(|p| p.ident.to_string())),
                )
                .collect(),
            public: false,
            trait_impl: self.trait_impl,
            signature: sig,
            owner: self.owner.clone(),
            fields,
            text: self.text(span),
        });
        id
    }
    fn function(&mut self, sig: &syn::Signature, body: &syn::Block, public: bool) {
        let name = self.owner.as_ref().map_or_else(
            || sig.ident.to_string(),
            |o| format!("{}::{}", self.text(o.span()), sig.ident),
        );
        let id = self.record(name, sig.span(), Some(sig.clone()), vec![]);
        if let Some(record) = self.records.last_mut() {
            record.public = public;
            record.body_end_line = body.span().end().line;
        }
        let old = self.function.replace(id);
        self.visit_block(body);
        self.function = old;
    }
}
impl<'ast> Visit<'ast> for Collector<'_> {
    fn visit_item_const(&mut self, c: &'ast syn::ItemConst) {
        if self.function.is_none() && self.owner.is_none() && self.selection.admits(c.span()) {
            self.constants.push(Constant {
                name: c.ident.to_string(),
                module: self.module.clone(),
                site: crate::workflow::Site::at(self.file, c.span()),
                value: literal(self.source, &c.expr),
            });
        }
        // Const evaluation is not execution of its enclosing function.
    }

    fn visit_item_mod(&mut self, m: &'ast syn::ItemMod) {
        if !self.selection.admits(m.span()) {
            return;
        }
        if let Some((_, items)) = &m.content {
            let imports = std::mem::replace(
                &mut self.imports,
                crate::workflow::facts::imports(items, self.selection),
            );
            self.module.push(m.ident.to_string());
            for item in items {
                self.visit_item(item);
            }
            self.module.pop();
            self.imports = imports;
        }
    }
    fn visit_item_struct(&mut self, s: &'ast syn::ItemStruct) {
        if self.function.is_some() || !self.selection.admits(s.span()) {
            return;
        }
        let fields = s
            .fields
            .iter()
            .enumerate()
            .map(|(i, f)| {
                (
                    f.ident
                        .as_ref()
                        .map_or_else(|| i.to_string(), ToString::to_string),
                    f.ty.clone(),
                )
            })
            .collect();
        self.record(s.ident.to_string(), s.ident.span(), None, fields);
        if let Some(record) = self.records.last_mut() {
            record
                .parameters
                .extend(s.generics.type_params().map(|p| p.ident.to_string()));
        }
    }
    fn visit_item_enum(&mut self, e: &'ast syn::ItemEnum) {
        if self.function.is_some() || !self.selection.admits(e.span()) {
            return;
        }
        let fields = e
            .variants
            .iter()
            .flat_map(|v| {
                v.fields.iter().enumerate().map(move |(i, f)| {
                    (
                        format!(
                            "{}::{}",
                            v.ident,
                            f.ident
                                .as_ref()
                                .map_or_else(|| i.to_string(), ToString::to_string)
                        ),
                        f.ty.clone(),
                    )
                })
            })
            .collect();
        self.record(e.ident.to_string(), e.ident.span(), None, fields);
        if let Some(record) = self.records.last_mut() {
            record
                .parameters
                .extend(e.generics.type_params().map(|p| p.ident.to_string()));
        }
    }
    fn visit_item_impl(&mut self, i: &'ast syn::ItemImpl) {
        if !self.selection.admits(i.span()) {
            return;
        }
        let old = self.owner.replace(*i.self_ty.clone());
        let old_trait = std::mem::replace(&mut self.trait_impl, i.trait_.is_some());
        let old_parameters = self.parameters.clone();
        self.parameters
            .extend(i.generics.type_params().map(|p| p.ident.to_string()));
        syn::visit::visit_item_impl(self, i);
        self.owner = old;
        self.trait_impl = old_trait;
        self.parameters = old_parameters;
    }
    fn visit_item_fn(&mut self, f: &'ast syn::ItemFn) {
        if self.function.is_none() && self.selection.admits(f.span()) {
            self.function(
                &f.sig,
                &f.block,
                matches!(f.vis, syn::Visibility::Public(_)),
            );
        }
    }
    fn visit_impl_item_fn(&mut self, f: &'ast syn::ImplItemFn) {
        if self.selection.admits(f.span()) {
            self.function(
                &f.sig,
                &f.block,
                matches!(f.vis, syn::Visibility::Public(_)),
            );
        }
    }
    fn visit_expr_struct(&mut self, s: &'ast syn::ExprStruct) {
        if let Some(f) = &self.function {
            self.constructors.push((
                f.clone(),
                s.path.clone(),
                self.owner.clone(),
                s.span().start().line,
            ));
        }
        syn::visit::visit_expr_struct(self, s);
    }
    // Deferred bodies have their own execution; do not attribute their construction to a call of this function.
    fn visit_expr_closure(&mut self, _: &'ast syn::ExprClosure) {}
    fn visit_expr_async(&mut self, _: &'ast syn::ExprAsync) {}
}

fn literal(source: &str, e: &syn::Expr) -> Literal {
    match e {
        syn::Expr::Reference(e) => literal(source, &e.expr),
        syn::Expr::Paren(e) => literal(source, &e.expr),
        syn::Expr::Group(e) => literal(source, &e.expr),
        syn::Expr::Array(e) => Literal::Array(e.elems.iter().map(|e| literal(source, e)).collect()),
        syn::Expr::Tuple(e) => Literal::Tuple(e.elems.iter().map(|e| literal(source, e)).collect()),
        syn::Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Str(s),
            ..
        }) => Literal::String(s.value()),
        syn::Expr::Path(e)
            if e.qself.is_none()
                && e.path
                    .segments
                    .iter()
                    .all(|s| matches!(s.arguments, syn::PathArguments::None)) =>
        {
            Literal::Path(
                e.path
                    .segments
                    .iter()
                    .map(|s| s.ident.to_string())
                    .collect(),
            )
        }
        _ => Literal::Opaque(crate::source_text::text(source, e.span())),
    }
}
