//! Conservative cfg implication over syn Meta, independent of host configuration.
use syn::{Meta, Token, parse::Parser, punctuated::Punctuated};

pub fn attributes_require_test_or_features(text: &str, features: &[&str]) -> bool {
    let parser = |input: syn::parse::ParseStream<'_>| {
        let attrs = syn::Attribute::parse_outer(input)?;
        let _: proc_macro2::TokenStream = input.parse()?;
        Ok(attrs)
    };
    parser
        .parse_str(text)
        .is_ok_and(|attrs| requires(&attrs, features))
}
pub(super) fn requires(attrs: &[syn::Attribute], features: &[&str]) -> bool {
    attrs.iter().any(|attr| {
        attr.path().is_ident("test")
            || (attr.path().is_ident("cfg")
                && attr
                    .parse_args::<Meta>()
                    .is_ok_and(|meta| implies(&meta, features)))
    })
}
pub(crate) fn implies(meta: &Meta, features: &[&str]) -> bool {
    match meta {
        Meta::Path(path) => path.is_ident("test"),
        Meta::NameValue(value) if value.path.is_ident("feature") => {
            matches!(&value.value, syn::Expr::Lit(expr) if matches!(&expr.lit,
                syn::Lit::Str(lit) if features.contains(&lit.value().as_str())))
        }
        Meta::List(list) if list.path.is_ident("all") || list.path.is_ident("any") => {
            let Ok(children) =
                list.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
            else {
                return false;
            };
            if list.path.is_ident("all") {
                children.iter().any(|m| implies(m, features))
            } else {
                children.iter().all(|m| implies(m, features))
            }
        }
        _ => false,
    }
}
