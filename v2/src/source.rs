//! The source as the parser reads it (C2-B-010, C2-B-165, C2-B-218) and the
//! conversion from byte offsets to tree positions (C2-A-140, C2-A-141).

use crate::ast::Pos;

/// The normalized source: U+0000 read as U+FFFD, the byte-order mark kept in
/// place but skipped by the line scan. Offsets into `text` are converted back
/// to offsets of the original bytes when a position is built.
#[derive(Debug)]
pub struct Source {
    text: String,
    /// Byte offset of the first character of each line in `text`.
    line_starts: Vec<usize>,
    /// Offsets in `text` of each U+FFFD that stood for a U+0000 (two extra bytes each).
    nul_at: Vec<usize>,
    /// Length of the byte-order mark at the start, 0 or 3.
    bom: usize,
}

impl Source {
    /// Reads `input` (C2-B-218: U+0000 is read as U+FFFD).
    #[must_use]
    pub fn new(input: &str) -> Self {
        let mut nul_at = Vec::new();
        let text = if input.contains('\0') {
            let mut t = String::with_capacity(input.len() + 16);
            for c in input.chars() {
                if c == '\0' {
                    nul_at.push(t.len());
                    t.push('\u{FFFD}');
                } else {
                    t.push(c);
                }
            }
            t
        } else {
            input.to_owned()
        };
        let bom = if text.starts_with('\u{FEFF}') { 3 } else { 0 };
        let mut line_starts = vec![bom];
        for (i, b) in text.bytes().enumerate() {
            if b == b'\n' {
                line_starts.push(i + 1);
            }
        }
        Self { text, line_starts, nul_at, bom }
    }

    /// The normalized text.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Length of a leading byte-order mark (C2-B-165).
    #[must_use]
    pub const fn bom_len(&self) -> usize {
        self.bom
    }

    /// Number of lines; a source that ends with LF has an empty last line.
    #[must_use]
    pub fn line_count(&self) -> usize {
        self.line_starts.len()
    }

    /// Byte range of line `i` (0-based) without its LF and without a CR
    /// directly before the LF (C2-B-010).
    #[must_use]
    pub fn line_range(&self, i: usize) -> (usize, usize) {
        let start = self.line_starts.get(i).copied().unwrap_or(self.text.len());
        match self.line_starts.get(i + 1) {
            Some(&next) => {
                let lf = next - 1;
                let cr = lf > start && self.text.as_bytes().get(lf - 1) == Some(&b'\r');
                (start, if cr { lf - 1 } else { lf })
            }
            None => (start, self.text.len()),
        }
    }

    /// The text of line `i` as `line_range` gives it.
    #[must_use]
    pub fn line(&self, i: usize) -> &str {
        let (s, e) = self.line_range(i);
        self.text.get(s..e).unwrap_or("")
    }

    /// Byte offset of the end of line `i` including its line ending.
    #[must_use]
    pub fn line_end_with_break(&self, i: usize) -> usize {
        self.line_starts.get(i + 1).copied().unwrap_or(self.text.len())
    }

    /// A slice of the normalized text, empty when the range is out of bounds.
    #[must_use]
    pub fn slice(&self, start: usize, end: usize) -> &str {
        self.text.get(start..end).unwrap_or("")
    }

    /// Offset in the original bytes for an offset into `text`.
    #[must_use]
    pub fn original_offset(&self, at: usize) -> usize {
        if self.nul_at.is_empty() {
            return at;
        }
        let before = self.nul_at.partition_point(|&n| n < at);
        at - 2 * before
    }

    /// 0-based line index of a byte offset.
    #[must_use]
    pub fn line_of(&self, at: usize) -> usize {
        self.line_starts.partition_point(|&s| s <= at).saturating_sub(1)
    }

    /// 1-based line and column (code points) of an offset (C2-A-141).
    #[must_use]
    pub fn line_col(&self, at: usize) -> (usize, usize) {
        let line = self.line_of(at);
        let start = self.line_starts.get(line).copied().unwrap_or(0);
        let col = self.text.get(start..at.max(start)).map_or(0, |s| s.chars().count());
        (line + 1, col + 1)
    }

    /// The position of a byte range of `text`.
    #[must_use]
    pub fn pos(&self, start: usize, end: usize) -> Pos {
        let (start_line, start_column) = self.line_col(start);
        let (end_line, end_column) = self.line_col(end);
        Pos {
            start_line,
            end_line,
            start_column,
            end_column,
            start_offset: self.original_offset(start),
            end_offset: self.original_offset(end),
        }
    }

    /// Byte length of the original source (`srcByteLength`, C2-A-010).
    #[must_use]
    pub fn original_len(&self) -> usize {
        self.original_offset(self.text.len())
    }
}
