//! Where a call's callee name is written, as a SCIP position: the last name
//! segment of a path call, or a method call's method name where the parser
//! saw it. A call whose name cannot be found at exactly the written text,
//! or a method call read without its name's site, gets no position.
use crate::workflow::Call;

/// The callee name and its zero-based line and UTF-8 byte column.
pub(super) struct Callee {
    pub(super) name: String,
    pub(super) line: u32,
    pub(super) column: u32,
}

/// The byte offset of each line's start in one file, read once per file.
pub(super) struct Lines(Vec<usize>);
impl Lines {
    pub(super) fn of(text: &str) -> Self {
        Self(
            std::iter::once(0)
                .chain(text.match_indices('\n').map(|(i, _)| i + 1))
                .collect(),
        )
    }
}

/// Locate `call`'s callee in `text`, the captured source of its file, whose
/// line starts are `lines`.
pub(super) fn callee(text: &str, lines: &Lines, call: &Call) -> Option<Callee> {
    let start = offset(text, lines, call.site.line, call.site.column)?;
    let (at, name) = match &call.method {
        None if call.receiver.is_none() => path_name(text, start, &call.spelling)?,
        // A method name is found where the parser saw it, never by
        // searching the text: a comment or the receiver's own calls can
        // stand between the receiver and the method.
        None => return None,
        Some(method) => {
            let at = offset(text, lines, method.line, method.column)?;
            written(text, at, &call.spelling).then(|| (at, call.spelling.clone()))?
        }
    };
    // The line holding `at`: the last line start at or before it.
    let line = lines.0.partition_point(|&line_start| line_start <= at) - 1;
    Some(Callee {
        name,
        line: u32::try_from(line).ok()?,
        column: u32::try_from(at - lines.0[line]).ok()?,
    })
}

/// Byte offset of a one-based line and zero-based character column.
fn offset(text: &str, lines: &Lines, line: usize, column: usize) -> Option<usize> {
    let start = *lines.0.get(line.checked_sub(1)?)?;
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

    fn site(line: usize, column: usize) -> Site {
        Site {
            path: "a.rs".into(),
            line,
            column,
            end_line: line,
            end_column: column,
        }
    }
    fn call(line: usize, column: usize, spelling: &str, receiver: Option<&str>) -> Call {
        Call {
            spelling: spelling.into(),
            receiver: receiver.map(Into::into),
            arguments: vec![],
            site: site(line, column),
            method: None,
        }
    }
    /// A method call whose method name the parser saw at `name`.
    fn method(line: usize, column: usize, spelling: &str, receiver: &str, name: usize) -> Call {
        Call {
            method: Some(site(line, name)),
            ..call(line, column, spelling, Some(receiver))
        }
    }

    #[test]
    fn path_calls_name_their_last_segment_and_skip_turbofish() {
        let text =
            "fn f() {\n    Vec::<u8>::new();\n    ParameterValues::<Real>::block_samples();\n}\n";
        let c = callee(text, &Lines::of(text), &call(2, 4, "Vec::<u8>::new", None)).unwrap();
        assert_eq!((c.name.as_str(), c.line, c.column), ("new", 1, 15));
        let c = callee(
            text,
            &Lines::of(text),
            &call(3, 4, "ParameterValues::<Real>::block_samples", None),
        )
        .unwrap();
        assert_eq!((c.name.as_str(), c.line), ("block_samples", 2));
    }

    #[test]
    fn method_calls_name_the_method_where_it_was_parsed_in_bytes() {
        let text = "fn f() { é.len().len(); }\n";
        // The second `len` is at character 17, byte 18.
        let c = callee(text, &Lines::of(text), &method(1, 9, "len", "é.len()", 17)).unwrap();
        assert_eq!(c.column, 18, "byte column of the second len");
        assert!(callee(text, &Lines::of(text), &method(1, 9, "missing", "é", 11)).is_none());
        // Without the name's site a method call gets no position.
        assert!(callee(text, &Lines::of(text), &call(1, 9, "len", Some("é.len()"))).is_none());
    }

    /// The 2026-11-21 review's case: a comment between the receiver and the
    /// method made a text search take the receiver's own `foo`.
    #[test]
    fn a_comment_before_the_method_does_not_move_it() {
        let text = "pub fn chain(a: A) { a.foo() /* comment */ .foo(); }\n";
        let outer = text.rfind("foo").unwrap();
        let c = callee(
            text,
            &Lines::of(text),
            &method(1, 21, "foo", "a.foo()", outer),
        )
        .unwrap();
        assert_eq!(c.column as usize, outer);
    }
}
