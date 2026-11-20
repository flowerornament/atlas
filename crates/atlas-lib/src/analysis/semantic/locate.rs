//! Where a call's callee name is written, as a SCIP position: the last name
//! segment of a path call, or the method name after the receiver. A call
//! whose name cannot be found at exactly the written text gets no position.
use crate::workflow::Call;

/// The callee name and its zero-based line and UTF-8 byte column.
pub(super) struct Callee {
    pub(super) name: String,
    pub(super) line: u32,
    pub(super) column: u32,
}

/// Locate `call`'s callee in `text`, the captured source of its file.
pub(super) fn callee(text: &str, call: &Call) -> Option<Callee> {
    let start = offset(text, call.site.line, call.site.column)?;
    let (at, name) = match call.receiver.as_deref() {
        None => path_name(text, start, &call.spelling)?,
        Some(receiver) => method_name(text, start, receiver, &call.spelling)?,
    };
    let line_start = text[..at].rfind('\n').map_or(0, |i| i + 1);
    Some(Callee {
        name,
        line: u32::try_from(text[..at].matches('\n').count()).ok()?,
        column: u32::try_from(at - line_start).ok()?,
    })
}

/// Byte offset of a one-based line and zero-based character column.
fn offset(text: &str, line: usize, column: usize) -> Option<usize> {
    let mut start = 0;
    for _ in 1..line {
        start += text[start..].find('\n')? + 1;
    }
    let rest = &text[start..];
    let line_end = rest.find('\n').unwrap_or(rest.len());
    rest[..line_end]
        .char_indices()
        .map(|(i, _)| i)
        .chain(std::iter::once(line_end))
        .nth(column)
        .map(|i| start + i)
}

fn ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// The last segment of a path call outside angle brackets; a turbofish
/// segment (`::<T>`) is not a name.
fn path_name(text: &str, start: usize, spelling: &str) -> Option<(usize, String)> {
    let bytes = spelling.as_bytes();
    let (mut depth, mut starts, mut i) = (0i32, vec![0usize], 0);
    while i < bytes.len() {
        match bytes[i] {
            b'<' => depth += 1,
            b'>' => depth -= 1,
            b':' if depth == 0 && bytes.get(i + 1) == Some(&b':') => {
                starts.push(i + 2);
                i += 1;
            }
            _ => {}
        }
        i += 1;
    }
    let segment = starts.into_iter().rev().find(|&s| {
        spelling[s..]
            .trim_start()
            .chars()
            .next()
            .is_some_and(|c| ident(c) && !c.is_ascii_digit())
    })?;
    let lead = spelling[segment..].len() - spelling[segment..].trim_start().len();
    let name: String = spelling[segment + lead..]
        .chars()
        .take_while(|&c| ident(c))
        .collect();
    let at = start + segment + lead;
    written(text, at, &name).then_some((at, name))
}

/// The method name after `receiver`: first where the receiver's written
/// length ends, then the first `.name` that follows the call's start.
fn method_name(text: &str, start: usize, receiver: &str, name: &str) -> Option<(usize, String)> {
    let after_dot = |dot: usize| {
        let rest = text.get(dot + 1..)?;
        let at = dot + 1 + (rest.len() - rest.trim_start().len());
        written(text, at, name).then_some(at)
    };
    let expected = text
        .get(start + receiver.len()..)
        .and_then(|rest| rest.trim_start().starts_with('.').then_some(rest))
        .map(|rest| start + receiver.len() + (rest.len() - rest.trim_start().len()))
        .and_then(after_dot);
    let at = expected.or_else(|| {
        text.get(start..)?
            .match_indices('.')
            .find_map(|(dot, _)| after_dot(start + dot))
    })?;
    Some((at, name.to_owned()))
}

/// `name` is written at `at` as a whole identifier.
fn written(text: &str, at: usize, name: &str) -> bool {
    !name.is_empty()
        && text.get(at..at + name.len()) == Some(name)
        && !text[..at].chars().next_back().is_some_and(ident)
        && !text[at + name.len()..].chars().next().is_some_and(ident)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::Site;

    fn call(line: usize, column: usize, spelling: &str, receiver: Option<&str>) -> Call {
        Call {
            spelling: spelling.into(),
            receiver: receiver.map(Into::into),
            arguments: vec![],
            site: Site {
                path: "a.rs".into(),
                line,
                column,
                end_line: line,
                end_column: column,
            },
        }
    }

    #[test]
    fn path_calls_name_their_last_segment_and_skip_turbofish() {
        let text =
            "fn f() {\n    Vec::<u8>::new();\n    ParameterValues::<Real>::block_samples();\n}\n";
        let c = callee(text, &call(2, 4, "Vec::<u8>::new", None)).unwrap();
        assert_eq!((c.name.as_str(), c.line, c.column), ("new", 1, 15));
        let c = callee(
            text,
            &call(3, 4, "ParameterValues::<Real>::block_samples", None),
        )
        .unwrap();
        assert_eq!((c.name.as_str(), c.line), ("block_samples", 2));
    }

    #[test]
    fn method_calls_name_the_method_after_the_receiver_in_bytes() {
        let text = "fn f() { é.len().len(); }\n";
        let c = callee(text, &call(1, 9, "len", Some("é.len()"))).unwrap();
        assert_eq!(c.column, 18, "byte column of the second len");
        assert!(callee(text, &call(1, 9, "missing", Some("é"))).is_none());
    }
}
