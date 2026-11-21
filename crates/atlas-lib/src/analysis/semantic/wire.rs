//! A dependency-free reader for the subset of the SCIP protobuf schema that
//! call evidence needs: tool metadata, document paths and position encodings,
//! occurrences (range, symbol, roles) and symbol kinds.
//!
//! Unknown fields are skipped, so a newer indexer stays readable. Malformed or
//! truncated input refuses with the reason; it never yields a partial index.
//! Field numbers follow `scip.proto` (sourcegraph/scip).

/// The fields of one SCIP `Index` this reader keeps.
#[derive(Debug, Default)]
pub(super) struct Index {
    pub(super) tool: String,
    pub(super) version: String,
    pub(super) project_root: String,
    pub(super) documents: Vec<Document>,
}
#[derive(Debug, Default)]
pub(super) struct Document {
    pub(super) relative_path: String,
    pub(super) position_encoding: u64,
    /// The document's text, when the indexer wrote it (`Document.text`).
    pub(super) text: Option<String>,
    pub(super) occurrences: Vec<Occurrence>,
    pub(super) symbols: Vec<Symbol>,
}
/// One occurrence. `range` is normalised to `[start_line, start_character,
/// end_line, end_character]`, zero-based, in the document's encoding.
#[derive(Debug, Default)]
pub(super) struct Occurrence {
    pub(super) range: [u32; 4],
    pub(super) symbol: String,
    pub(super) roles: u64,
}
#[derive(Debug, Default)]
pub(super) struct Symbol {
    pub(super) symbol: String,
    pub(super) kind: u64,
}

/// `SymbolRole.Definition`.
pub(super) const DEFINITION: u64 = 1;
/// `PositionEncoding.UTF8CodeUnitOffsetFromLineStart`.
pub(super) const UTF8_FROM_LINE_START: u64 = 1;

type Read<T> = Result<T, String>;

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}
impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, at: 0 }
    }
    fn done(&self) -> bool {
        self.at >= self.bytes.len()
    }
    fn varint(&mut self) -> Read<u64> {
        let mut value = 0u64;
        for shift in (0..64).step_by(7) {
            let byte = *self
                .bytes
                .get(self.at)
                .ok_or_else(|| format!("truncated varint at byte {}", self.at))?;
            self.at += 1;
            value |= u64::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                return Ok(value);
            }
        }
        Err(format!("varint longer than 64 bits at byte {}", self.at))
    }
    fn slice(&mut self) -> Read<&'a [u8]> {
        let len = usize::try_from(self.varint()?).map_err(|_| "field length overflows")?;
        let end = self
            .at
            .checked_add(len)
            .filter(|end| *end <= self.bytes.len())
            .ok_or_else(|| format!("truncated field at byte {}", self.at))?;
        let out = &self.bytes[self.at..end];
        self.at = end;
        Ok(out)
    }
    fn string(&mut self) -> Read<String> {
        String::from_utf8(self.slice()?.to_vec()).map_err(|e| format!("string is not UTF-8: {e}"))
    }
    fn advance(&mut self, n: usize) -> Read<()> {
        self.at = self
            .at
            .checked_add(n)
            .filter(|end| *end <= self.bytes.len())
            .ok_or_else(|| format!("truncated fixed-width field at byte {}", self.at))?;
        Ok(())
    }
    fn skip(&mut self, wire: u64) -> Read<()> {
        match wire {
            0 => self.varint().map(drop),
            1 => self.advance(8),
            2 => self.slice().map(drop),
            5 => self.advance(4),
            other => Err(format!("unsupported wire type {other}")),
        }
    }
    /// Visit each field of this message with the reader at its value. A
    /// visitor returns `false` for a field it does not read, which is skipped.
    fn fields(&mut self, mut each: impl FnMut(&mut Self, u64, u64) -> Read<bool>) -> Read<()> {
        while !self.done() {
            let key = self.varint()?;
            let (field, wire) = (key >> 3, key & 7);
            if field == 0 {
                return Err("field number 0".into());
            }
            if !each(self, field, wire)? {
                self.skip(wire)?;
            }
        }
        Ok(())
    }
}

/// Read one serialized SCIP `Index`.
pub(super) fn parse(bytes: &[u8]) -> Read<Index> {
    let mut index = Index::default();
    Reader::new(bytes).fields(|r, field, wire| {
        Ok(match (field, wire) {
            (1, 2) => {
                metadata(r.slice()?, &mut index)?;
                true
            }
            (2, 2) => {
                index.documents.push(document(r.slice()?)?);
                true
            }
            _ => false,
        })
    })?;
    Ok(index)
}

fn metadata(bytes: &[u8], index: &mut Index) -> Read<()> {
    Reader::new(bytes).fields(|r, field, wire| {
        Ok(match (field, wire) {
            (2, 2) => {
                Reader::new(r.slice()?).fields(|t, field, wire| {
                    Ok(match (field, wire) {
                        (1, 2) => {
                            index.tool = t.string()?;
                            true
                        }
                        (2, 2) => {
                            index.version = t.string()?;
                            true
                        }
                        _ => false,
                    })
                })?;
                true
            }
            (3, 2) => {
                index.project_root = r.string()?;
                true
            }
            _ => false,
        })
    })
}

fn document(bytes: &[u8]) -> Read<Document> {
    let mut d = Document::default();
    Reader::new(bytes).fields(|r, field, wire| {
        Ok(match (field, wire) {
            (1, 2) => {
                d.relative_path = r.string()?;
                true
            }
            (2, 2) => {
                d.occurrences.push(occurrence(r.slice()?)?);
                true
            }
            (3, 2) => {
                d.symbols.push(symbol(r.slice()?)?);
                true
            }
            (5, 2) => {
                d.text = Some(r.string()?);
                true
            }
            (6, 0) => {
                d.position_encoding = r.varint()?;
                true
            }
            _ => false,
        })
    })?;
    if d.relative_path.is_empty() {
        return Err("document without a relative path".into());
    }
    Ok(d)
}

fn occurrence(bytes: &[u8]) -> Read<Occurrence> {
    let mut o = Occurrence::default();
    let mut range = Vec::new();
    Reader::new(bytes).fields(|r, field, wire| {
        Ok(match (field, wire) {
            // `range`, packed or not.
            (1, 2) => {
                let mut packed = Reader::new(r.slice()?);
                while !packed.done() {
                    range.push(packed.varint()?);
                }
                true
            }
            (1, 0) => {
                range.push(r.varint()?);
                true
            }
            // `single_line_range` and `multi_line_range`, the newer form.
            // Their scalar fields are omitted when zero, so read them by number.
            (8 | 9, 2) => {
                let width = if field == 8 { 3 } else { 4 };
                let mut parts = vec![0; width];
                Reader::new(r.slice()?).fields(|p, part, wire| {
                    let slot = usize::try_from(part)
                        .ok()
                        .filter(|n| (1..=width).contains(n));
                    match (slot, wire) {
                        (Some(n), 0) => {
                            parts[n - 1] = p.varint()?;
                            Ok(true)
                        }
                        _ => Ok(false),
                    }
                })?;
                range = parts;
                true
            }
            (2, 2) => {
                o.symbol = r.string()?;
                true
            }
            (3, 0) => {
                o.roles = r.varint()?;
                true
            }
            _ => false,
        })
    })?;
    let range: Vec<u32> = range
        .into_iter()
        .map(|v| u32::try_from(v).map_err(|_| format!("occurrence position {v} out of range")))
        .collect::<Read<_>>()?;
    o.range = match range[..] {
        [line, start, end] => [line, start, line, end],
        [start_line, start, end_line, end] => [start_line, start, end_line, end],
        _ => return Err(format!("occurrence range has {} elements", range.len())),
    };
    Ok(o)
}

fn symbol(bytes: &[u8]) -> Read<Symbol> {
    let mut s = Symbol::default();
    Reader::new(bytes).fields(|r, field, wire| {
        Ok(match (field, wire) {
            (1, 2) => {
                s.symbol = r.string()?;
                true
            }
            (5, 0) => {
                s.kind = r.varint()?;
                true
            }
            _ => false,
        })
    })?;
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn varint(mut v: u64, out: &mut Vec<u8>) {
        loop {
            let byte = u8::try_from(v & 0x7f).expect("seven bits");
            v >>= 7;
            if v == 0 {
                out.push(byte);
                return;
            }
            out.push(byte | 0x80);
        }
    }
    fn bytes(field: u64, value: &[u8], out: &mut Vec<u8>) {
        varint(field << 3 | 2, out);
        varint(value.len() as u64, out);
        out.extend_from_slice(value);
    }
    fn number(field: u64, value: u64, out: &mut Vec<u8>) {
        varint(field << 3, out);
        varint(value, out);
    }
    fn one_document() -> Vec<u8> {
        let mut occurrence = Vec::new();
        let mut range = Vec::new();
        for v in [3, 4, 9] {
            varint(v, &mut range);
        }
        bytes(1, &range, &mut occurrence);
        bytes(2, b"scheme manager pkg 1 lib/f().", &mut occurrence);
        number(3, 1, &mut occurrence);
        let mut document = Vec::new();
        bytes(1, b"src/lib.rs", &mut document);
        bytes(2, &occurrence, &mut document);
        number(6, 1, &mut document);
        let mut index = Vec::new();
        bytes(2, &document, &mut index);
        index
    }

    #[test]
    fn reads_the_fields_it_needs() {
        let index = parse(&one_document()).unwrap();
        let document = &index.documents[0];
        assert_eq!(document.relative_path, "src/lib.rs");
        assert_eq!(document.position_encoding, UTF8_FROM_LINE_START);
        assert_eq!(document.occurrences[0].range, [3, 4, 3, 9]);
        assert_eq!(document.occurrences[0].roles, DEFINITION);
    }

    #[test]
    fn unknown_fields_of_every_wire_type_are_skipped() {
        let mut input = Vec::new();
        number(15, 7, &mut input);
        varint(16 << 3 | 1, &mut input);
        input.extend_from_slice(&[0; 8]);
        varint(17 << 3 | 5, &mut input);
        input.extend_from_slice(&[0; 4]);
        bytes(18, b"opaque", &mut input);
        input.extend(one_document());
        assert_eq!(parse(&input).unwrap().documents.len(), 1);
    }

    #[test]
    fn every_truncation_refuses_rather_than_reading_part_of_an_index() {
        let input = one_document();
        for len in 1..input.len() {
            assert!(
                parse(&input[..len]).is_err(),
                "a {len}-byte prefix of {} must not parse",
                input.len()
            );
        }
    }

    #[test]
    fn malformed_input_refuses_with_its_reason() {
        let error = parse(&[0x0f]).unwrap_err();
        assert!(error.contains("wire type 7"), "{error}");
        let error = parse(&[0x00]).unwrap_err();
        assert!(error.contains("field number 0"), "{error}");
        let mut long = vec![0x10];
        long.extend([0xff; 11]);
        assert!(parse(&long).unwrap_err().contains("varint"));
        let mut bad_range = Vec::new();
        let mut occurrence = Vec::new();
        bytes(1, &[1, 2], &mut occurrence);
        let mut document = Vec::new();
        bytes(1, b"a.rs", &mut document);
        bytes(2, &occurrence, &mut document);
        bytes(2, &document, &mut bad_range);
        assert!(parse(&bad_range).unwrap_err().contains("2 elements"));
    }
}
