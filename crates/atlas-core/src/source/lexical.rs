//! Exact source bytes and a comment/string mask. This does not parse Rust items.
use super::{Item, SyntaxError, TestRegions};
use std::path::Path;
#[derive(Debug)]
struct ProjectedLine {
    code: String,
    /// Whether this line begins outside a continued string or block comment.
    starts_in_code: bool,
}

/// One lexical view of Rust source shared by every line-based scan.
pub struct RustSourceProjection {
    text: String,
    lines: Vec<ProjectedLine>,
}

impl RustSourceProjection {
    pub fn of(text: String) -> Self {
        Self {
            lines: code_projection(&text),
            text,
        }
    }
    pub fn text(&self) -> &str {
        &self.text
    }
    /// Structural compatibility entry; malformed Rust returns its syntax error.
    pub fn outline(&self, file: &Path) -> Result<Vec<Item>, SyntaxError> {
        Ok(super::syntax::read(file, self)?.outline)
    }
    pub fn test_regions(&self) -> Result<TestRegions, SyntaxError> {
        Ok(super::syntax::read(Path::new("input.rs"), self)?.regions)
    }
    /// Declared items with their source lines, independent of layout.
    pub fn declaration_lines(&self) -> Result<Vec<(usize, Item)>, SyntaxError> {
        Ok(self
            .outline(Path::new("input.rs"))?
            .into_iter()
            .filter(|item| item.kind.declares())
            .map(|item| (item.line, item))
            .collect())
    }
    /// Full-line comments in code, excluding comment-shaped string contents.
    pub fn line_comment_lines(&self) -> impl Iterator<Item = (usize, &str)> {
        self.text
            .lines()
            .enumerate()
            .filter(|(index, line)| {
                self.lines[*index].starts_in_code && line.trim_start().starts_with("//")
            })
            .map(|(index, line)| (index + 1, line))
    }
    /// Whether this zero-based line starts in lexical code.
    pub fn starts_in_code(&self, index: usize) -> bool {
        self.lines
            .get(index)
            .is_some_and(|line| line.starts_in_code)
    }
    pub fn code_lines(&self) -> impl Iterator<Item = &str> {
        self.lines.iter().map(|line| line.code.as_str())
    }

    /// Lexical //! documentation, with one-based line witnesses.
    pub fn module_header_sites(&self) -> impl Iterator<Item = (usize, &str)> {
        self.text
            .lines()
            .zip(&self.lines)
            .enumerate()
            .filter(|(_, (_, projected))| projected.starts_in_code)
            .filter_map(|(index, (line, _))| {
                line.trim_start()
                    .strip_prefix("//!")
                    .map(|text| (index + 1, text.trim()))
            })
    }

    /// Lexical //! documentation lines, including nested module docs.
    pub fn module_header_lines(&self) -> Vec<String> {
        self.module_header_sites()
            .map(|(_, text)| text.to_owned())
            .collect()
    }
}

#[derive(Debug, Clone, Copy)]
enum LexicalState {
    Code,
    BlockComment(usize),
    String { escaped: bool },
    RawString { hashes: usize },
}

/// Project Rust source onto code bytes while preserving line and byte offsets.
///
/// This is deliberately smaller than a parser: it recognizes only comments
/// and string literals, the two lexical forms that can counterfeit declaration
/// lines or braces for the line grammar.
fn code_projection(text: &str) -> Vec<ProjectedLine> {
    let bytes = text.as_bytes();
    let mut state = LexicalState::Code;
    let mut starts_in_code = true;
    let mut line = Vec::new();
    let mut lines = Vec::new();
    let mut index = 0usize;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte == b'\n' {
            lines.push(ProjectedLine {
                code: String::from_utf8(line).expect("masked source remains UTF-8"),
                starts_in_code,
            });
            line = Vec::new();
            starts_in_code = matches!(state, LexicalState::Code);
            index += 1;
            continue;
        }
        match state {
            LexicalState::Code if bytes[index..].starts_with(b"//") => {
                while index < bytes.len() && bytes[index] != b'\n' {
                    line.push(b' ');
                    index += 1;
                }
            }
            LexicalState::Code if bytes[index..].starts_with(b"/*") => {
                line.extend_from_slice(b"  ");
                state = LexicalState::BlockComment(1);
                index += 2;
            }
            LexicalState::Code => {
                if let Some((prefix_len, hashes)) = raw_string_open(&bytes[index..]) {
                    line.extend(std::iter::repeat_n(b' ', prefix_len));
                    state = LexicalState::RawString { hashes };
                    index += prefix_len;
                } else if let Some(literal_len) = (byte == b'\'')
                    .then(|| char_literal_len(&text[index..]))
                    .flatten()
                {
                    line.extend(std::iter::repeat_n(b' ', literal_len));
                    index += literal_len;
                } else if byte == b'"' {
                    line.push(b' ');
                    state = LexicalState::String { escaped: false };
                    index += 1;
                } else {
                    line.push(byte);
                    index += 1;
                }
            }
            LexicalState::BlockComment(depth) if bytes[index..].starts_with(b"/*") => {
                line.extend_from_slice(b"  ");
                state = LexicalState::BlockComment(depth + 1);
                index += 2;
            }
            LexicalState::BlockComment(depth) if bytes[index..].starts_with(b"*/") => {
                line.extend_from_slice(b"  ");
                state = if depth == 1 {
                    LexicalState::Code
                } else {
                    LexicalState::BlockComment(depth - 1)
                };
                index += 2;
            }
            LexicalState::BlockComment(_) => {
                line.push(b' ');
                index += 1;
            }
            LexicalState::String { escaped } => {
                line.push(b' ');
                state = if escaped {
                    LexicalState::String { escaped: false }
                } else if byte == b'\\' {
                    LexicalState::String { escaped: true }
                } else if byte == b'"' {
                    LexicalState::Code
                } else {
                    state
                };
                index += 1;
            }
            LexicalState::RawString { hashes } => {
                let closes = byte == b'"'
                    && bytes
                        .get(index + 1..index + 1 + hashes)
                        .is_some_and(|suffix| suffix.iter().all(|byte| *byte == b'#'));
                line.push(b' ');
                index += 1;
                if closes {
                    line.extend(std::iter::repeat_n(b' ', hashes));
                    index += hashes;
                    state = LexicalState::Code;
                }
            }
        }
    }
    if !line.is_empty() {
        lines.push(ProjectedLine {
            code: String::from_utf8(line).expect("masked source remains UTF-8"),
            starts_in_code,
        });
    }
    lines
}

fn raw_string_open(bytes: &[u8]) -> Option<(usize, usize)> {
    let prefix = if bytes.starts_with(b"br") {
        2
    } else if bytes.starts_with(b"r") {
        1
    } else {
        return None;
    };
    let hashes = bytes[prefix..]
        .iter()
        .take_while(|byte| **byte == b'#')
        .count();
    (bytes.get(prefix + hashes) == Some(&b'"')).then_some((prefix + hashes + 1, hashes))
}

fn char_literal_len(text: &str) -> Option<usize> {
    let body = text.strip_prefix('\'')?;
    let content_len = if let Some(escaped) = body.strip_prefix('\\') {
        if let Some(unicode) = escaped.strip_prefix("u{") {
            unicode.find('}')? + 4
        } else if escaped.starts_with('x') {
            4
        } else {
            escaped.chars().next()?.len_utf8() + 1
        }
    } else {
        body.chars().next()?.len_utf8()
    };
    (body.as_bytes().get(content_len) == Some(&b'\'')).then_some(content_len + 2)
}
