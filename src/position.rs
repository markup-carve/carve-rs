//! Source positions for AST nodes (spec PART 12 §4).
//!
//! §4 pins the unit: lines are 1-based, columns are 1-based, offsets are
//! 0-based, and BOTH columns and offsets count UNICODE CODEPOINTS. Not bytes,
//! not UTF-16 code units.
//!
//! That matters here more than in the other engines. Rust indexes `&str` by
//! BYTE, so every offset this parser knows is a byte offset, and the three
//! units agree on ASCII - which means a byte-indexed position passes every test
//! that does not contain an astral character. The spec measures the difference
//! on `\u{1F600} *b*`, where the delimiter sits at codepoint 2, UTF-16 unit 3
//! and byte 5.
//!
//! So the conversion happens once per document, in [`PositionIndex`], and every
//! lookup after that is arithmetic on precomputed tables.

/// A span of source, in codepoints.
///
/// `end_column` and `end_offset` are EXCLUSIVE, matching §4's example.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pos {
    pub start_line: usize,
    pub end_line: usize,
    pub start_column: usize,
    pub end_column: usize,
    pub start_offset: usize,
    pub end_offset: usize,
}

/// Byte-to-codepoint tables for one document.
///
/// Built in a single pass. For a document with no multi-byte character the
/// conversion is the identity, which is the common case and costs one `Vec`.
pub struct PositionIndex {
    /// Codepoint offset for each BYTE offset that begins a character, plus a
    /// final entry for end-of-source so an exclusive end is representable.
    codepoint_of_byte: Vec<usize>,
    /// Byte offset where each line starts, 0-based line index.
    line_start_byte: Vec<usize>,
}

impl PositionIndex {
    pub fn new(source: &str) -> Self {
        let mut codepoint_of_byte = vec![0; source.len() + 1];
        let mut codepoints = 0;
        for (byte, _) in source.char_indices() {
            codepoint_of_byte[byte] = codepoints;
            codepoints += 1;
        }
        codepoint_of_byte[source.len()] = codepoints;

        // A byte in the MIDDLE of a character never begins one, so `char_indices`
        // leaves it 0. Carrying the previous value forward means a stray
        // interior byte resolves to the character containing it rather than to
        // the start of the document - wrong either way, but not catastrophically
        // so, and callers are expected to pass character boundaries.
        let mut last = 0;
        for slot in codepoint_of_byte.iter_mut() {
            if *slot == 0 && last != 0 {
                *slot = last;
            } else {
                last = *slot;
            }
        }

        let mut line_start_byte = vec![0];
        for (byte, ch) in source.char_indices() {
            if ch == '\n' {
                line_start_byte.push(byte + 1);
            }
        }

        PositionIndex {
            codepoint_of_byte,
            line_start_byte,
        }
    }

    /// Codepoint offset for a byte offset.
    pub fn offset(&self, byte: usize) -> usize {
        *self
            .codepoint_of_byte
            .get(byte)
            .unwrap_or_else(|| self.codepoint_of_byte.last().unwrap_or(&0))
    }

    /// 1-based codepoint column for a byte offset on a 1-based line.
    ///
    /// Returns `None` when the line is not in this document, so a caller that
    /// has lost track of where it is cannot silently report column 1.
    pub fn column(&self, line: usize, byte: usize) -> Option<usize> {
        let start = *self.line_start_byte.get(line.checked_sub(1)?)?;
        if byte < start {
            return None;
        }

        Some(self.offset(byte) - self.offset(start) + 1)
    }

    /// Byte offset where a 1-based line begins.
    pub fn line_start(&self, line: usize) -> Option<usize> {
        self.line_start_byte.get(line.checked_sub(1)?).copied()
    }

    /// A span from a byte range, given the lines its ends fall on.
    ///
    /// `None` when either end cannot be placed - §4 forbids emitting a position
    /// with invented values, so a span that cannot be computed is not emitted.
    pub fn span(
        &self,
        start_byte: usize,
        end_byte: usize,
        start_line: usize,
        end_line: usize,
    ) -> Option<Pos> {
        if end_byte < start_byte || end_line < start_line {
            return None;
        }

        Some(Pos {
            start_line,
            end_line,
            start_column: self.column(start_line, start_byte)?,
            end_column: self.column(end_line, end_byte)?,
            start_offset: self.offset(start_byte),
            end_offset: self.offset(end_byte),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_offsets_are_the_identity() {
        let index = PositionIndex::new("abc\ndef\n");
        assert_eq!(index.offset(0), 0);
        assert_eq!(index.offset(4), 4);
        assert_eq!(index.offset(8), 8);
    }

    #[test]
    fn an_astral_character_separates_codepoints_from_bytes() {
        // The exact measurement PART 12 §4 uses. The emoji is 4 bytes and 1
        // codepoint, so the `*` after the space is byte 5 and codepoint 2.
        let source = "\u{1F600} *b*";
        let index = PositionIndex::new(source);

        assert_eq!(source.find('*'), Some(5));
        assert_eq!(index.offset(5), 2);
    }

    #[test]
    fn a_bmp_character_also_shifts_the_offset() {
        // Two bytes, one codepoint - so byte and codepoint offsets diverge well
        // before the astral plane, which is why an engine cannot rely on ASCII
        // tests to prove the unit.
        let index = PositionIndex::new("é x");
        assert_eq!(index.offset(2), 1);
    }

    #[test]
    fn columns_count_codepoints_from_the_line_start() {
        let source = "a\n\u{1F600}b";
        let index = PositionIndex::new(source);

        // `b` is byte 6 (1 + 1 + 4), on line 2, one codepoint past the emoji.
        assert_eq!(index.column(2, 6), Some(2));
    }

    #[test]
    fn a_column_on_an_unknown_line_is_none() {
        let index = PositionIndex::new("a\n");
        assert_eq!(index.column(9, 0), None);
        assert_eq!(index.column(0, 0), None);
    }

    #[test]
    fn a_span_carries_codepoint_ends() {
        let source = "\u{1F600} *b*\n";
        let index = PositionIndex::new(source);
        let pos = index.span(5, 8, 1, 1).expect("span");

        assert_eq!(pos.start_offset, 2);
        assert_eq!(pos.end_offset, 5);
        assert_eq!(pos.start_column, 3);
        assert_eq!(pos.end_column, 6);
    }

    #[test]
    fn a_backwards_span_is_refused() {
        // Absent beats wrong: §4 forbids inventing values, and a span whose end
        // precedes its start is one no consumer can act on.
        let index = PositionIndex::new("abc\n");
        assert_eq!(index.span(2, 1, 1, 1), None);
        assert_eq!(index.span(0, 1, 2, 1), None);
    }

    #[test]
    fn the_end_of_source_is_addressable() {
        // An exclusive end offset has to be able to name the position one past
        // the last character.
        let source = "ab";
        let index = PositionIndex::new(source);
        assert_eq!(index.offset(source.len()), 2);
    }
}
