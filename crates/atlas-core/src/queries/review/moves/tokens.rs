//! Native token readings, with exact and explicitly approximate rerooted modes.
use super::Body;
use proc_macro2::{Delimiter, Spacing, TokenStream, TokenTree};
use std::collections::BTreeSet;

/// A declaration span's two token readings.
pub(super) fn tokens(text: &str, roots: &BTreeSet<String>) -> Body {
    let Ok(stream) = text.parse::<TokenStream>() else {
        return Body::Unreadable;
    };
    readings(stream, roots)
}
pub(super) fn readings(stream: TokenStream, roots: &BTreeSet<String>) -> Body {
    Body::Tokens {
        exact: normalized(stream.clone(), None),
        rerooted: normalized(stream, Some(roots)),
    }
}

/// Stop at the impl body, not a brace group inside a const-generic argument.
pub(crate) fn impl_header(text: &str) -> Option<TokenStream> {
    let stream = text.parse::<TokenStream>().ok()?;
    let mut header = TokenStream::new();
    let mut angles = 0usize;
    let mut previous = None;
    for token in stream {
        match &token {
            TokenTree::Group(group) if angles == 0 && group.delimiter() == Delimiter::Brace => {
                return Some(header);
            }
            TokenTree::Punct(punct) => {
                match punct.as_char() {
                    '<' => angles += 1,
                    '>' if previous != Some('-') => angles = angles.saturating_sub(1),
                    _ => {}
                }
                previous = Some(punct.as_char());
            }
            _ => previous = None,
        }
        header.extend([token]);
    }
    None
}

fn punct(token: Option<&TokenTree>, ch: char) -> bool {
    matches!(token, Some(TokenTree::Punct(p)) if p.as_char() == ch)
}

/// Whether `tokens[index..]` opens a `::` path separator.
fn path_separator(tokens: &[TokenTree], index: usize) -> bool {
    matches!(tokens.get(index), Some(TokenTree::Punct(p)) if p.as_char() == ':' && p.spacing() == Spacing::Joint)
        && punct(tokens.get(index + 1), ':')
}

/// A span's tokens with docs, visibility and layout removed: doc attributes,
/// `pub` and `pub(..)`, the trailing comma before a closing brace/bracket, and
/// the braces rustfmt adds around a closure's entire single-expression body.
/// With `roots` (see [`super::path_roots`]), a path whose first segment is a root
/// also loses it and the lowercase module segments after it
/// (`crate::state::Slot` and `dispatch::Slot` both read `Slot`). Primitive
/// types, method names and other module paths are kept: `u32::MAX` and
/// `u64::MAX` stay different. Macro input groups retain their tokens unchanged.
fn normalized(stream: TokenStream, roots: Option<&BTreeSet<String>>) -> Vec<String> {
    fn push(stream: TokenStream, roots: Option<&BTreeSet<String>>, out: &mut Vec<String>) {
        let tokens: Vec<TokenTree> = stream.into_iter().collect();
        let mut index = 0;
        while index < tokens.len() {
            let skip = match &tokens[index] {
                TokenTree::Punct(hash) if hash.as_char() == '#' => {
                    let at = index + 1 + usize::from(punct(tokens.get(index + 1), '!'));
                    let doc = matches!(tokens.get(at), Some(TokenTree::Group(group))
                        if group.delimiter() == Delimiter::Bracket
                            && group.stream().into_iter().next().is_some_and(|head| head.to_string() == "doc"));
                    if doc { at + 1 - index } else { 0 }
                }
                TokenTree::Ident(ident) if ident == "pub" => {
                    let restricted = matches!(tokens.get(index + 1), Some(TokenTree::Group(group))
                        if group.delimiter() == Delimiter::Parenthesis);
                    1 + usize::from(restricted)
                }
                TokenTree::Ident(root)
                    if roots.is_some_and(|roots| roots.contains(&root.to_string()))
                        && path_separator(&tokens, index + 1)
                        && !(index >= 2 && path_separator(&tokens, index - 2)) =>
                {
                    let mut end = index + 3;
                    while let Some(TokenTree::Ident(segment)) = tokens.get(end)
                        && segment
                            .to_string()
                            .starts_with(|ch: char| ch.is_ascii_lowercase() || ch == '_')
                        && path_separator(&tokens, end + 1)
                    {
                        end += 3;
                    }
                    end - index
                }
                _ => 0,
            };
            if skip > 0 {
                index += skip;
                continue;
            }
            match &tokens[index] {
                TokenTree::Group(group) => {
                    let invocation = index > 0 && punct(tokens.get(index - 1), '!');
                    let definition = index >= 3
                        && tokens[index - 3].to_string() == "macro_rules"
                        && punct(tokens.get(index - 2), '!');
                    let attribute = group.delimiter() == Delimiter::Bracket
                        && ((index > 0 && punct(tokens.get(index - 1), '#'))
                            || (index >= 2
                                && punct(tokens.get(index - 2), '#')
                                && punct(tokens.get(index - 1), '!')));
                    if invocation || definition || attribute {
                        raw(&tokens[index], out);
                        index += 1;
                        continue;
                    }
                    let mut inner = Vec::new();
                    push(group.stream(), roots, &mut inner);
                    // Parentheses can form a one-element tuple: `(x,)` differs
                    // from `(x)`. Without a syntax classification, keep its comma.
                    if group.delimiter() != Delimiter::Parenthesis
                        && inner.last().is_some_and(|last| last == ",")
                    {
                        inner.pop();
                    }
                    // The braces are the closure's whole body only when the
                    // group ends the argument, statement or enclosing group.
                    let ends_body = tokens
                        .get(index + 1)
                        .is_none_or(|next| punct(Some(next), ',') || punct(Some(next), ';'));
                    let single_expression_closure = group.delimiter() == Delimiter::Brace
                        && out.last().is_some_and(|last| last == "|")
                        && ends_body
                        && !inner.is_empty()
                        && !group
                            .stream()
                            .into_iter()
                            .any(|token| punct(Some(&token), ';'));
                    let (open, close) = match group.delimiter() {
                        _ if single_expression_closure => ("", ""),
                        Delimiter::Parenthesis => ("(", ")"),
                        Delimiter::Brace => ("{", "}"),
                        Delimiter::Bracket => ("[", "]"),
                        Delimiter::None => ("", ""),
                    };
                    if !open.is_empty() {
                        out.push(open.to_owned());
                    }
                    out.extend(inner);
                    if !close.is_empty() {
                        out.push(close.to_owned());
                    }
                }
                other => out.push(other.to_string()),
            }
            index += 1;
        }
    }
    let mut out = Vec::new();
    push(stream, roots, &mut out);
    out
}

/// Macro matchers can distinguish visibility words, doc attributes, commas and
/// punctuation spacing. Preserve that syntax rather than treating it as Rust layout.
fn raw(token: &TokenTree, out: &mut Vec<String>) {
    match token {
        TokenTree::Group(group) => {
            out.push(format!("raw-open:{:?}", group.delimiter()));
            for token in group.stream() {
                raw(&token, out);
            }
            out.push("raw-close".to_owned());
        }
        TokenTree::Punct(punct) => out.push(format!(
            "raw-punct:{}:{:?}",
            punct.as_char(),
            punct.spacing()
        )),
        other => out.push(format!("raw:{other}")),
    }
}
