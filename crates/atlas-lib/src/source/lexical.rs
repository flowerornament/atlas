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
#[non_exhaustive]
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
        if bytes[index] == b'\n' {
            lines.push(ProjectedLine {
                code: String::from_utf8(line).expect("masked source remains UTF-8"),
                starts_in_code,
            });
            line = Vec::new();
            starts_in_code = matches!(state, LexicalState::Code);
            index += 1;
            continue;
        }
        (state, index) = state.mask(text, index, &mut line);
    }
    if !line.is_empty() {
        lines.push(ProjectedLine {
            code: String::from_utf8(line).expect("masked source remains UTF-8"),
            starts_in_code,
        });
    }
    lines
}

impl LexicalState {
    /// Mask the lexical step at `index` (never a newline) onto `line`: code
    /// bytes are copied, comment and string bytes become spaces, one output
    /// byte per input byte. Returns the next state and index.
    fn mask(self, text: &str, index: usize, line: &mut Vec<u8>) -> (Self, usize) {
        let bytes = text.as_bytes();
        let byte = bytes[index];
        match self {
            Self::Code => mask_code(text, index, line),
            Self::BlockComment(depth) if bytes[index..].starts_with(b"/*") => {
                line.extend_from_slice(b"  ");
                (Self::BlockComment(depth + 1), index + 2)
            }
            Self::BlockComment(depth) if bytes[index..].starts_with(b"*/") => {
                line.extend_from_slice(b"  ");
                let state = if depth == 1 {
                    Self::Code
                } else {
                    Self::BlockComment(depth - 1)
                };
                (state, index + 2)
            }
            Self::BlockComment(_) => {
                line.push(b' ');
                (self, index + 1)
            }
            Self::String { escaped } => {
                line.push(b' ');
                let state = if escaped {
                    Self::String { escaped: false }
                } else if byte == b'\\' {
                    Self::String { escaped: true }
                } else if byte == b'"' {
                    Self::Code
                } else {
                    self
                };
                (state, index + 1)
            }
            Self::RawString { hashes } => {
                let closes = byte == b'"'
                    && bytes
                        .get(index + 1..index + 1 + hashes)
                        .is_some_and(|suffix| suffix.iter().all(|byte| *byte == b'#'));
                line.push(b' ');
                if closes {
                    line.extend(std::iter::repeat_n(b' ', hashes));
                    (Self::Code, index + 1 + hashes)
                } else {
                    (self, index + 1)
                }
            }
        }
    }
}

/// One step in code: a line comment is masked to the end of its line; a
/// block comment, string or raw string opens; a char literal is masked
/// whole; any other byte is copied.
fn mask_code(text: &str, mut index: usize, line: &mut Vec<u8>) -> (LexicalState, usize) {
    let bytes = text.as_bytes();
    let byte = bytes[index];
    if bytes[index..].starts_with(b"//") {
        while index < bytes.len() && bytes[index] != b'\n' {
            line.push(b' ');
            index += 1;
        }
        (LexicalState::Code, index)
    } else if bytes[index..].starts_with(b"/*") {
        line.extend_from_slice(b"  ");
        (LexicalState::BlockComment(1), index + 2)
    } else if let Some((prefix_len, hashes)) = raw_string_open(&bytes[index..]) {
        line.extend(std::iter::repeat_n(b' ', prefix_len));
        (LexicalState::RawString { hashes }, index + prefix_len)
    } else if let Some(literal_len) = (byte == b'\'')
        .then(|| char_literal_len(&text[index..]))
        .flatten()
    {
        line.extend(std::iter::repeat_n(b' ', literal_len));
        (LexicalState::Code, index + literal_len)
    } else if byte == b'"' {
        line.push(b' ');
        (LexicalState::String { escaped: false }, index + 1)
    } else {
        line.push(byte);
        (LexicalState::Code, index + 1)
    }
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
