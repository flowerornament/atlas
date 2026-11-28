//! Recognize the written initializer-free linkme declaration, not its expansion.
use proc_macro2::TokenStream;
use syn::{Attribute, Ident, Meta, Token, Type, Visibility, parse::Parse};

pub(super) struct Declaration {
    pub attrs: Vec<Attribute>,
    pub vis: Visibility,
    pub name: Ident,
}

impl Parse for Declaration {
    fn parse(input: syn::parse::ParseStream<'_>) -> syn::Result<Self> {
        let attrs = Attribute::parse_outer(input)?;
        let vis = input.parse()?;
        let _: Token![static] = input.parse()?;
        let name = input.parse()?;
        let _: Token![:] = input.parse()?;
        let ty: Type = input.parse()?;
        let _: Token![;] = input.parse()?;
        let marker = attrs.iter().any(|attr| {
            let Meta::Path(path) = &attr.meta else {
                return false;
            };
            let names: Vec<_> = path.segments.iter().map(|s| s.ident.to_string()).collect();
            names == ["distributed_slice"] || names == ["linkme", "distributed_slice"]
        });
        if !marker || !matches!(ty, Type::Slice(_)) {
            return Err(input.error("not a written distributed slice declaration"));
        }
        Ok(Self { attrs, vis, name })
    }
}

pub(super) fn parse(tokens: TokenStream) -> Option<Declaration> {
    syn::parse2(tokens).ok()
}
