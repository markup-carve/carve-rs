//! A cursor over one line: byte position and visual column (C2-B-003 to C2-B-007).

use crate::tables::TAB_STOP;

/// A line of the source with a cursor. Columns count code points from the
/// start of the line, a tab advancing to the next multiple of the tab stop
/// counted from the start of the line (C2-B-004, C2-B-005).
#[derive(Debug, Clone, Copy)]
pub struct Cursor<'a> {
    pub text: &'a str,
    /// Byte offset of the line in the source.
    pub base: usize,
    pub pos: usize,
    pub col: usize,
}

impl<'a> Cursor<'a> {
    #[must_use]
    pub const fn new(text: &'a str, base: usize) -> Self {
        Self { text, base, pos: 0, col: 0 }
    }

    /// The rest of the line from the cursor.
    #[must_use]
    pub fn rest(&self) -> &'a str {
        self.text.get(self.pos..).unwrap_or("")
    }

    /// Source offset of the cursor.
    #[must_use]
    pub const fn offset(&self) -> usize {
        self.base + self.pos
    }

    #[must_use]
    pub fn peek(&self) -> Option<char> {
        self.rest().chars().next()
    }

    #[must_use]
    pub fn peek_byte(&self) -> Option<u8> {
        self.text.as_bytes().get(self.pos).copied()
    }

    /// Column after the character at the cursor.
    #[must_use]
    pub const fn col_after(col: usize, c: char) -> usize {
        if c == '\t' { (col / TAB_STOP + 1) * TAB_STOP } else { col + 1 }
    }

    /// Advances over one character.
    pub fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.col = Self::col_after(self.col, c);
        self.pos += c.len_utf8();
        Some(c)
    }

    /// Advances `n` bytes of ASCII.
    pub fn bump_n(&mut self, n: usize) {
        for _ in 0..n {
            if self.bump().is_none() {
                break;
            }
        }
    }

    /// Advances over spaces and tabs.
    pub fn skip_blanks(&mut self) {
        while matches!(self.peek_byte(), Some(b' ' | b'\t')) {
            self.bump();
        }
    }

    /// The cursor advanced over spaces and tabs, without moving this one.
    #[must_use]
    pub fn after_blanks(&self) -> Self {
        let mut c = *self;
        c.skip_blanks();
        c
    }

    /// True when only spaces and tabs remain.
    #[must_use]
    pub fn is_blank(&self) -> bool {
        self.rest().bytes().all(|b| b == b' ' || b == b'\t')
    }

    /// Strips whole spaces and tabs while they end at or before column
    /// `limit` (C2-B-007: a tab that straddles the strip column is kept).
    pub fn strip_to(&mut self, limit: usize) {
        while let Some(c @ (' ' | '\t')) = self.peek() {
            if Self::col_after(self.col, c) > limit {
                break;
            }
            self.bump();
        }
    }
}
