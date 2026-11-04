//! Discover selectable function bodies in written modules and impls.
//! Qualified selectors preserve duplicate-name refusals; this is not a declaration census.
use super::{Workflow, parse::Reader};
use crate::Result;
use syn::{Item, spanned::Spanned};

pub(super) struct Function<'a> {
    name: String,
    module: Vec<String>,
    owner: Option<String>,
    parameters: Vec<String>,
    imports: std::collections::BTreeMap<String, Vec<Vec<String>>>,
    signature: &'a syn::Signature,
    body: &'a syn::Block,
}
pub(super) fn collect<'a>(
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
impl Reader<'_> {
    pub(super) fn select(&self, functions: &[Function<'_>], selector: &str) -> Result<Workflow> {
        let found: Vec<_> = functions
            .iter()
            .filter(|f| {
                f.name == selector
                    || (!selector.contains("::") && f.name.rsplit("::").next() == Some(selector))
            })
            .collect();
        let [function] = found.as_slice() else {
            let path = self.path;
            let count = found.len();
            return Err(format!(
                "workflow {path}::{selector}: expected one function, found {count}; use its qualified module/impl name"
            ).into());
        };
        Ok(self.build(function))
    }
    pub(super) fn build(&self, function: &Function<'_>) -> Workflow {
        let (calls, bindings) = super::facts::body(self, function.signature, function.body);
        let limits = [
            "Calls stay at their call sites; names and dispatch are not resolved or expanded.",
            "Operands are evaluated before their enclosing operation; labels over 110 characters are shortened, source spans remain exact.",
            "Branches and loops show syntax, not feasibility, iteration counts or a control-flow proof.",
            "Each ? is a distinct residual decision after its operand, including conditions and return expressions.",
            "Macros, closures, labelled blocks, deferred bodies and unsupported expressions are opaque; their effects are not inferred.",
        ];

        Workflow {
            schema: 2,
            function: function.name.clone(),
            module: function.module.clone(),
            owner: function.owner.clone(),
            type_parameters: function.parameters.clone(),
            site: self.site(function.signature.span()),
            steps: self.block(function.body),
            calls,
            bindings,
            imports: function.imports.clone(),
            output: match &function.signature.output {
                syn::ReturnType::Default => "()".into(),
                syn::ReturnType::Type(_, ty) => self.text(ty.span()),
            },
            limits: limits.into_iter().map(str::to_owned).collect(),
        }
    }
}
