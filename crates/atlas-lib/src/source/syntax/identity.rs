//! A function's identity, as `flow --function` prints it and policy rows
//! name it: an impl's self type and trait by their last segment, with
//! their type arguments, lifetimes dropped and spacing normalized, so a row
//! does not orphan when a lifetime is renamed or an import respelled
//! (`<Graph<Cell> as From<Seed>>::from`). Distinct type arguments stay
//! distinct (`From<A>` and `From<B>`).
use quote::ToTokens;

/// An impl's segment in a function's identity: its self type, or
/// `<SelfType as Trait>` for a trait impl.
pub(crate) fn impl_segment(item: &syn::ItemImpl) -> String {
    let owner = type_name(&item.self_ty);
    match &item.trait_ {
        Some((path, _)) => format!("<{owner} as {}>", path_name(path)),
        None => owner,
    }
}
/// A trait path's name in an identity: its last segment with its type
/// arguments.
pub(crate) fn path_name(path: &syn::Path) -> String {
    path.segments
        .last()
        .map(|segment| format!("{}{}", segment.ident, arguments(&segment.arguments)))
        .unwrap_or_default()
}
/// A type's name in an identity: a path by its last segment with its type
/// arguments; any other type as written, lifetimes dropped.
pub(crate) fn type_name(ty: &syn::Type) -> String {
    match ty {
        syn::Type::Path(path) if path.qself.is_none() => path_name(&path.path),
        syn::Type::Reference(reference) => format!(
            "&{}{}",
            if reference.mutability.is_some() {
                "mut "
            } else {
                ""
            },
            type_name(&reference.elem)
        ),
        syn::Type::Slice(slice) => format!("[{}]", type_name(&slice.elem)),
        syn::Type::Array(array) => {
            format!("[{}; {}]", type_name(&array.elem), tokens(&array.len))
        }
        syn::Type::Tuple(tuple) => format!(
            "({})",
            tuple
                .elems
                .iter()
                .map(type_name)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        syn::Type::Paren(inner) => type_name(&inner.elem),
        syn::Type::Group(inner) => type_name(&inner.elem),
        other => tokens(other),
    }
}
fn arguments(arguments: &syn::PathArguments) -> String {
    let syn::PathArguments::AngleBracketed(angle) = arguments else {
        return String::new();
    };
    let kept: Vec<String> = angle
        .args
        .iter()
        .filter_map(|argument| match argument {
            syn::GenericArgument::Lifetime(_) => None,
            syn::GenericArgument::Type(ty) => Some(type_name(ty)),
            syn::GenericArgument::AssocType(assoc) => {
                Some(format!("{} = {}", assoc.ident, type_name(&assoc.ty)))
            }
            other => Some(tokens(other)),
        })
        .collect();
    if kept.is_empty() {
        String::new()
    } else {
        format!("<{}>", kept.join(", "))
    }
}
/// Tokens as written, without lifetimes, one space between words.
fn tokens(node: &impl ToTokens) -> String {
    node.to_token_stream()
        .to_string()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// A selector or policy row in the same form, so an older spelling (written
/// spacing, lifetimes, a qualified trait or `<Type as Trait>` self type)
/// names the same function. Text that does not parse as a path, and a
/// trailing `#N`, are kept as written; a plain path keeps every segment,
/// since modules and an impl's self type cannot be told apart in it.
#[must_use]
pub fn normalize_selector(selector: &str) -> String {
    let (path, repeat) = match selector.rsplit_once('#') {
        Some((path, number)) if number.bytes().all(|b| b.is_ascii_digit()) => (path, Some(number)),
        _ => (selector, None),
    };
    let Ok(syn::Type::Path(written)) = syn::parse_str::<syn::Type>(path) else {
        return selector.to_owned();
    };
    let rest = |segments: &mut dyn Iterator<Item = &syn::PathSegment>| {
        segments
            .map(|segment| format!("{}{}", segment.ident, arguments(&segment.arguments)))
            .collect::<Vec<_>>()
    };
    let mut parts = Vec::new();
    match &written.qself {
        Some(qself) => {
            let position = qself.position;
            let trait_path = syn::Path {
                leading_colon: None,
                segments: written
                    .path
                    .segments
                    .iter()
                    .take(position)
                    .cloned()
                    .collect(),
            };
            parts.push(format!(
                "<{} as {}>",
                type_name(&qself.ty),
                path_name(&trait_path)
            ));
            parts.extend(rest(&mut written.path.segments.iter().skip(position)));
        }
        None => parts.extend(rest(&mut written.path.segments.iter())),
    }
    let normalized = parts.join("::");
    repeat.map_or(normalized.clone(), |number| {
        format!("{normalized}#{number}")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn segment(text: &str) -> String {
        let file: syn::File = syn::parse_str(text).expect("parses");
        let syn::Item::Impl(item) = &file.items[0] else {
            panic!("an impl");
        };
        impl_segment(item)
    }

    #[test]
    fn an_impl_is_named_by_last_segments_with_arguments_and_no_lifetimes() {
        assert_eq!(
            segment("impl<'a> crate::a::Graph<'a, Cell> {}"),
            "Graph<Cell>"
        );
        assert_eq!(
            segment("impl<'a> std::convert::From<&'a Seed> for g::Graph<'a> {}"),
            "<Graph as From<&Seed>>"
        );
        assert_ne!(
            segment("impl From<A> for T {}"),
            segment("impl From<B> for T {}")
        );
        assert_eq!(segment("impl Vec < u8 > {}"), "Vec<u8>");
    }

    #[test]
    fn an_older_spelling_normalizes_to_the_identity() {
        for (written, identity) in [
            ("<T as From<A>>::from", "<T as From<A>>::from"),
            (
                "<g::Graph<'a> as std::convert::From<&'a Seed>>::from",
                "<Graph as From<&Seed>>::from",
            ),
            ("m::Vec < u8 >::method", "m::Vec<u8>::method"),
            ("Cache<'a>::get#2", "Cache::get#2"),
            ("outer::inner", "outer::inner"),
        ] {
            assert_eq!(normalize_selector(written), identity, "{written}");
        }
    }
}
