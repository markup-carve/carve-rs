//! The source as the parser reads it (C2-B-010, C2-B-165, C2-B-218) and the
//! conversion from byte offsets to tree positions (C2-A-140, C2-A-141).

use crate::ast::Pos;
use std::borrow::Cow;

/// The normalized source: U+0000 read as U+FFFD, the byte-order mark kept in
/// place but skipped by the line scan. Offsets into `text` are converted back
/// to offsets of the original bytes when a position is built.
#[derive(Debug)]
pub struct Source<'a> {
    /// The input itself unless it holds U+0000.
    text: Cow<'a, str>,
    /// Byte offset of the first character of each line in `text`.
    line_starts: Vec<usize>,
    /// Offsets in `text` of each U+FFFD that stood for a U+0000 (two extra bytes each).
    nul_at: Vec<usize>,
    /// Length of the byte-order mark at the start, 0 or 3.
    bom: usize,
    /// For each block of `MARK` bytes, its first character boundary and the
    /// column there, so a column costs at most one block of counting.
    marks: Vec<(usize, usize)>,
}

const MARK: usize = 256;

/// The first character boundary of each `MARK` block with its 0-based
/// column, counted in code points from the start of its line.
fn column_marks(text: &str) -> Vec<(usize, usize)> {
    let mut marks = Vec::with_capacity(text.len() / MARK + 1);
    let mut col = 0;
    for (i, c) in text.char_indices() {
        while marks.len() * MARK <= i {
            marks.push((i, col));
        }
        // Line 0 starts behind a byte-order mark (`line_starts`).
        col = match c {
            '\n' => 0,
            '\u{FEFF}' if i == 0 => 0,
            _ => col + 1,
        };
    }
    marks
}

impl<'a> Source<'a> {
    /// Reads `input` (C2-B-218: U+0000 is read as U+FFFD).
    #[must_use]
    pub fn new(input: &'a str) -> Self {
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
            Cow::Owned(t)
        } else {
            Cow::Borrowed(input)
        };
        let bom = if text.starts_with('\u{FEFF}') { 3 } else { 0 };
        let mut line_starts = Vec::with_capacity(text.len() / 32 + 1);
        line_starts.push(bom);
        line_starts.extend(text.match_indices('\n').map(|(i, _)| i + 1));
        let marks = if text.is_ascii() {
            Vec::new()
        } else {
            column_marks(&text)
        };
        Self {
            text,
            line_starts,
            nul_at,
            bom,
            marks,
        }
    }

    /// The normalized text.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// A slice of the normalized text that lives as long as the input: none
    /// when the range is out of bounds or the text is a normalized copy.
    #[must_use]
    pub fn borrowed(&self, start: usize, end: usize) -> Option<&'a str> {
        match self.text {
            Cow::Borrowed(t) => t.get(start..end),
            Cow::Owned(_) => None,
        }
    }

    /// The offset of `s` in the text, when `s` is a slice of a borrowed text.
    #[must_use]
    pub fn offset_of(&self, s: &str) -> Option<usize> {
        let Cow::Borrowed(t) = self.text else {
            return None;
        };
        let at = s.as_ptr().addr().checked_sub(t.as_ptr().addr())?;
        (at + s.len() <= t.len()).then_some(at)
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
        self.line_starts
            .get(i + 1)
            .copied()
            .unwrap_or(self.text.len())
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
        self.line_starts
            .partition_point(|&s| s <= at)
            .saturating_sub(1)
    }

    /// 1-based line and column (code points) of an offset (C2-A-141).
    #[must_use]
    pub fn line_col(&self, at: usize) -> (usize, usize) {
        let line = self.line_of(at);
        let start = self.line_starts.get(line).copied().unwrap_or(0);
        let at = at.max(start);
        let (from, base) = if self.marks.is_empty() {
            (at, at - start)
        } else {
            let k = at / MARK;
            match self
                .marks
                .get(k)
                .filter(|m| m.0 <= at)
                .or_else(|| k.checked_sub(1).and_then(|j| self.marks.get(j)))
            {
                Some(&(b, c)) if b >= start && b <= at => (b, c),
                _ => (start, 0),
            }
        };
        let col = base + self.text.get(from..at).map_or(0, |s| s.chars().count());
        (line + 1, col + 1)
    }

    /// The position of a byte range of `text`.
    #[must_use]
    pub fn pos(&self, start: usize, end: usize) -> Pos {
        let (start_line, start_column) = self.line_col(start);
        let (end_line, end_column) = self.line_col(end);
        Pos {
            start_line: Pos::at(start_line),
            end_line: Pos::at(end_line),
            start_column: Pos::at(start_column),
            end_column: Pos::at(end_column),
            start_offset: Pos::at(self.original_offset(start)),
            end_offset: Pos::at(self.original_offset(end)),
        }
    }

    /// Byte length of the original source (`srcByteLength`, C2-A-010).
    #[must_use]
    pub fn original_len(&self) -> usize {
        self.original_offset(self.text.len())
    }
}

#[cfg(test)]
mod tests {
    use super::Source;

    fn slow(text: &str, start: usize, at: usize) -> usize {
        text.get(start..at).map_or(0, |s| s.chars().count()) + 1
    }

    #[test]
    fn c2_a_141_columns_count_code_points_on_long_lines() {
        let line = "é".repeat(300) + "a" + &"ü".repeat(200);
        let text = format!("\u{FEFF}x{line}\n{line}\nab");
        let src = Source::new(&text);
        for (at, _) in text.char_indices().filter(|&(i, _)| i >= 3) {
            let line = src.line_of(at);
            let start = src.line_range(line).0;
            assert_eq!(
                src.line_col(at),
                (line + 1, slow(&text, start, at)),
                "at {at}"
            );
        }
    }
}
