//! Fallible compatibility readers for written Rust fragments; production uses captured facts.
use super::super::{Field, Visibility};
use syn::{parse::Parser, spanned::Spanned};

pub(super) fn visibility(vis: &syn::Visibility) -> Visibility {
    match vis {
        syn::Visibility::Public(_) => Visibility::Pub,
        syn::Visibility::Inherited => Visibility::Private,
        syn::Visibility::Restricted(r) if r.path.is_ident("crate") => Visibility::PubCrate,
        syn::Visibility::Restricted(r) if r.path.is_ident("super") => Visibility::PubSuper,
        syn::Visibility::Restricted(r) if r.path.is_ident("self") => Visibility::Private,
        syn::Visibility::Restricted(_) => Visibility::PubIn,
    }
}
pub(super) fn restriction(text: &str, vis: &syn::Visibility) -> Option<String> {
    match vis {
        syn::Visibility::Restricted(r) => Some(
            text[r.path.span().byte_range()]
                .split_whitespace()
                .collect(),
        ),
        _ => None,
    }
}
/// Parse a visibility prefix; only pub(in ...) has a restricted-path comparison key.
pub fn restricted_path(text: &str) -> Result<Option<String>, syn::Error> {
    let parser = |input: syn::parse::ParseStream<'_>| {
        let vis: syn::Visibility = input.parse()?;
        let _: proc_macro2::TokenStream = input.parse()?;
        Ok(vis)
    };
    let vis = parser.parse_str(text)?;
    Ok((visibility(&vis) == Visibility::PubIn)
        .then(|| restriction(text, &vis))
        .flatten())
}
/// A complete impl or a header ending in its opening brace. No item-body search.
pub fn impl_trait(header: &str) -> Result<Option<&str>, syn::Error> {
    let complete;
    let text = if header.trim_end().ends_with('{') {
        complete = format!("{header}}}");
        &complete
    } else {
        header
    };
    let item: syn::ItemImpl = syn::parse_str(text)?;
    Ok(item
        .trait_
        .as_ref()
        .and_then(|(p, _)| p.segments.last())
        .and_then(|p| header.get(p.ident.span().byte_range())))
}
/// Struct fields as (line offset, name, written visibility, pub(in) target).
pub fn struct_fields(text: &str) -> Result<Vec<Field>, syn::Error> {
    let item: syn::ItemStruct = syn::parse_str(text)?;
    Ok(fields(
        text,
        &item.fields,
        item.struct_token.span.start().line,
    ))
}
pub(super) fn fields(text: &str, fields: &syn::Fields, line: usize) -> Vec<Field> {
    fields
        .iter()
        .enumerate()
        .map(|(i, f)| {
            let start = if matches!(f.vis, syn::Visibility::Inherited) {
                f.ident
                    .as_ref()
                    .map_or_else(|| f.ty.span(), syn::Ident::span)
            } else {
                f.vis.span()
            };
            Field {
                line_offset: start.start().line.saturating_sub(line),
                name: f
                    .ident
                    .as_ref()
                    .map_or_else(|| i.to_string(), ToString::to_string),
                visibility: visibility(&f.vis),
                restricted: (visibility(&f.vis) == Visibility::PubIn)
                    .then(|| restriction(text, &f.vis))
                    .flatten(),
            }
        })
        .collect()
}
