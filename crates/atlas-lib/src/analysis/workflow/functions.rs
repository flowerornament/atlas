//! Discover selectable function bodies in written modules, impls and traits.
//! Qualified selectors preserve duplicate-name refusals; this is not a declaration census.
//! A trait's default body is selectable for `flow` but never a call target:
//! an impl may override it.
use super::{Workflow, parse::Reader};
use crate::Result;
use std::fmt::Write as _;
use syn::{Item, spanned::Spanned};

pub(super) struct Function<'a> {
    name: String,
    /// `<Type as Trait>::method` for a trait impl's method.
    trait_name: Option<String>,
    /// A trait's default body: selectable, but not a call target.
    pub(super) default_body: bool,
    module: Vec<String>,
    owner: Option<String>,
    parameters: Vec<String>,
    imports: std::collections::BTreeMap<String, Vec<Vec<String>>>,
    signature: &'a syn::Signature,
    body: &'a syn::Block,
}
/// Every selectable function body among `items`, in `module`: free
/// functions, inline modules' bodies, impl methods and trait default
/// bodies.
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
    let scope = Scope {
        module,
        prefix: &prefix,
        imports: &imports,
    };
    for item in items {
        if !reader.selection.admits(item.span()) {
            continue;
        }
        match item {
            Item::Fn(f) => out.push(Function {
                name: format!("{prefix}{}", f.sig.ident),
                trait_name: None,
                default_body: false,
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
            Item::Impl(i) => impl_methods(i, &scope, reader, out),
            Item::Trait(t) => trait_defaults(t, &scope, reader, out),
            _ => {}
        }
    }
}

/// Where a run of items is written: its module, the prefix its names take,
/// and the imports in scope there.
struct Scope<'s> {
    module: &'s [String],
    prefix: &'s str,
    imports: &'s std::collections::BTreeMap<String, Vec<Vec<String>>>,
}

/// An impl's methods, named `Type::method` (and `<Type as Trait>::method`
/// for a trait impl).
fn impl_methods<'a>(
    i: &'a syn::ItemImpl,
    scope: &Scope<'_>,
    reader: &Reader<'_>,
    out: &mut Vec<Function<'a>>,
) {
    let prefix = scope.prefix;
    let owner = reader.text(i.self_ty.span());
    let implemented = i.trait_.as_ref().map(|(path, _)| reader.text(path.span()));
    for member in &i.items {
        if let syn::ImplItem::Fn(f) = member
            && reader.selection.admits(f.span())
        {
            out.push(Function {
                name: format!("{prefix}{owner}::{}", f.sig.ident),
                trait_name: implemented.as_ref().map(|implemented| {
                    format!("{prefix}<{owner} as {implemented}>::{}", f.sig.ident)
                }),
                default_body: false,
                module: scope.module.to_vec(),
                owner: Some(owner.clone()),
                parameters: i
                    .generics
                    .type_params()
                    .chain(f.sig.generics.type_params())
                    .map(|p| p.ident.to_string())
                    .collect(),
                imports: scope.imports.clone(),
                signature: &f.sig,
                body: &f.block,
            });
        }
    }
}

/// A trait's default method bodies, named `Trait::method`; `Self` is
/// whichever type implements the trait, so they have no owner.
fn trait_defaults<'a>(
    t: &'a syn::ItemTrait,
    scope: &Scope<'_>,
    reader: &Reader<'_>,
    out: &mut Vec<Function<'a>>,
) {
    for member in &t.items {
        if let syn::TraitItem::Fn(f) = member
            && let Some(body) = &f.default
            && reader.selection.admits(f.span())
        {
            out.push(Function {
                name: format!("{}{}::{}", scope.prefix, t.ident, f.sig.ident),
                trait_name: None,
                default_body: true,
                module: scope.module.to_vec(),
                owner: None,
                parameters: t
                    .generics
                    .type_params()
                    .chain(f.sig.generics.type_params())
                    .map(|p| p.ident.to_string())
                    .collect(),
                imports: scope.imports.clone(),
                signature: &f.sig,
                body,
            });
        }
    }
}

/// A `LINE` or `LINE:COLUMN` selector names the function whose name starts there.
fn site_selector(selector: &str) -> Option<(usize, Option<usize>)> {
    let (line, column) = match selector.split_once(':') {
        Some((line, column)) => (line, Some(column.parse().ok()?)),
        None => (selector, None),
    };
    Some((line.parse().ok()?, column))
}
impl Reader<'_> {
    /// Select by name, qualified name, `<Type as Trait>::method`, or the
    /// `LINE` / `LINE:COLUMN` where the function's name is written.
    pub(super) fn select(&self, functions: &[Function<'_>], selector: &str) -> Result<Workflow> {
        Ok(self.build(self.find(functions, selector)?))
    }
    /// The one function `selector` names, or every candidate with the
    /// selector that picks it.
    pub(super) fn find<'f, 'a>(
        &self,
        functions: &'f [Function<'a>],
        selector: &str,
    ) -> Result<&'f Function<'a>> {
        let site = |f: &Function<'_>| self.site(f.signature.ident.span());
        let found: Vec<_> = functions
            .iter()
            .filter(|f| match site_selector(selector) {
                Some((line, column)) => {
                    let at = site(f);
                    at.line == line && column.is_none_or(|column| at.column + 1 == column)
                }
                None => {
                    f.name == selector
                        || f.trait_name.as_deref() == Some(selector)
                        || (!selector.contains("::")
                            && f.name.rsplit("::").next() == Some(selector))
                }
            })
            .collect();
        let [function] = found.as_slice() else {
            let path = self.path;
            let count = found.len();
            let mut message = format!(
                "workflow {path}::{selector}: expected one function, found {count}; use its qualified module/impl name, <Type as Trait>::method, or the LINE where its name is written"
            );
            for f in &found {
                let at = site(f);
                let on_line = functions.iter().filter(|g| site(g).line == at.line).count();
                let by_site = if on_line == 1 {
                    at.line.to_string()
                } else {
                    format!("{}:{}", at.line, at.column + 1)
                };
                let by_name = f.trait_name.as_deref().unwrap_or(&f.name);
                let unique = functions
                    .iter()
                    .filter(|g| g.name == by_name || g.trait_name.as_deref() == Some(by_name))
                    .count()
                    == 1;
                let pick = if unique {
                    format!("'{by_name}' or {by_site}")
                } else {
                    by_site
                };
                write!(
                    message,
                    "\n  {path}:{}: {} (--function {pick})",
                    at.line, f.name
                )
                .expect("formatting into a String cannot fail");
            }
            return Err(message.into());
        };
        Ok(function)
    }
    /// The selected function's body as written, for a check that reads it
    /// itself.
    pub(super) fn selected<'a>(
        &self,
        functions: &[Function<'a>],
        selector: &str,
    ) -> Result<super::Selected<'a>> {
        let function = self.find(functions, selector)?;
        Ok(super::Selected {
            site: self.site(function.signature.ident.span()),
            block: function.body,
        })
    }
    pub(super) fn build(&self, function: &Function<'_>) -> Workflow {
        let (calls, bindings, local_names, declarations) =
            super::facts::body(self, function.signature, function.body);
        let limits = [
            "Calls stay at their call sites; names and dispatch are not resolved or expanded.",
            "Operands are evaluated before their enclosing operation; labels over 110 characters are shortened, source spans remain exact.",
            "Branches and loops show syntax, not feasibility, iteration counts or a control-flow proof.",
            "Each ? is a distinct residual decision after its operand, including conditions and return expressions.",
            "Macros, closures, deferred bodies and unsupported expressions are opaque; their effects are not inferred.",
        ];

        Workflow {
            schema: 2,
            function: function.name.clone(),
            module: function.module.clone(),
            owner: function.owner.clone(),
            trait_impl: function.trait_name.is_some(),
            type_parameters: function.parameters.clone(),
            site: self.site(function.signature.span()),
            steps: self.block(function.body),
            calls,
            bindings,
            local_names,
            declarations,
            imports: function.imports.clone(),
            output: match &function.signature.output {
                syn::ReturnType::Default => "()".into(),
                syn::ReturnType::Type(_, ty) => self.text(ty.span()),
            },
            limits: limits.into_iter().map(str::to_owned).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::workflow::parse::parse;

    const SOURCE: &str = "\
pub trait Defaulted { fn perform(&self) { first(); } fn required(&self); }
pub struct Both;
pub trait One { fn run(&self); }
pub trait Two { fn run(&self); }
impl One for Both { fn run(&self) { first(); } }
impl Two for Both { fn run(&self) { second(); } }
#[cfg(unix)]
pub fn entry() { first(); }
#[cfg(not(unix))]
pub fn entry() { second(); }
";

    fn calls(selector: &str) -> Vec<String> {
        parse(SOURCE, "lib.rs", selector)
            .unwrap_or_else(|e| panic!("{selector}: {e}"))
            .calls
            .into_iter()
            .map(|call| call.spelling)
            .collect()
    }

    #[test]
    fn a_trait_default_body_is_selectable() {
        assert_eq!(calls("perform"), ["first"]);
        assert_eq!(calls("Defaulted::perform"), ["first"]);
        assert!(parse(SOURCE, "lib.rs", "required").is_err());
    }

    #[test]
    fn trait_qualified_and_site_selectors_pick_one_body() {
        assert_eq!(calls("<Both as Two>::run"), ["second"]);
        assert_eq!(calls("8"), ["first"]);
        assert_eq!(calls("10:8"), ["second"]);
    }

    #[test]
    fn an_ambiguous_selector_lists_each_candidate_and_its_selector() {
        let error = parse(SOURCE, "lib.rs", "Both::run")
            .unwrap_err()
            .to_string();
        assert!(error.contains("found 2"), "{error}");
        assert!(
            error.contains("lib.rs:5: Both::run (--function '<Both as One>::run' or 5)"),
            "{error}"
        );
        assert!(error.contains("lib.rs:6: Both::run (--function '<Both as Two>::run' or 6)"));
        let error = parse(SOURCE, "lib.rs", "entry").unwrap_err().to_string();
        assert!(error.contains("lib.rs:8: entry (--function 8)"), "{error}");
        assert!(
            error.contains("lib.rs:10: entry (--function 10)"),
            "{error}"
        );
    }

    #[test]
    fn a_trait_default_body_is_not_a_call_target() {
        let parsed = crate::workflow::parse::parse_all(SOURCE, "lib.rs").expect("parse");
        assert!(
            !parsed
                .functions
                .iter()
                .any(|flow| flow.function.ends_with("perform"))
        );
    }
}
