//! Character classes the clauses name.

use crate::tables::{UNICODE_CF, UNICODE_L, UNICODE_M, UNICODE_N};

#[inline]
fn in_ranges(c: char, ranges: &[(u32, u32)]) -> bool {
    let code = u32::from(c);
    let at = ranges.partition_point(|&(start, _)| start <= code);
    at.checked_sub(1)
        .and_then(|i| ranges.get(i))
        .is_some_and(|&(_, end)| code <= end)
}

/// A word character: a Unicode letter or number, category L or N (inline preliminaries).
#[must_use]
pub fn is_word(c: char) -> bool {
    if c.is_ascii() {
        return c.is_ascii_alphanumeric();
    }
    in_ranges(c, UNICODE_L) || in_ranges(c, UNICODE_N)
}

/// A Unicode number, category N (C2-H-148).
#[must_use]
pub fn is_number(c: char) -> bool {
    if c.is_ascii() {
        c.is_ascii_digit()
    } else {
        in_ranges(c, UNICODE_N)
    }
}

/// Whitespace for the tests of the character before or after a position:
/// space, tab, line break, and the no-break space (inline preliminaries).
#[must_use]
pub const fn is_flank_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\u{a0}')
}

/// A combining mark, a variation selector or a zero-width joiner (C2-I-050).
#[must_use]
pub fn is_joining(c: char) -> bool {
    // Category M starts at U+0300.
    if c.is_ascii() {
        return false;
    }
    matches!(c, '\u{200D}' | '\u{FE00}'..='\u{FE0F}' | '\u{E0100}'..='\u{E01EF}')
        || in_ranges(c, UNICODE_M)
}

/// A Unicode letter, number or mark, category L, N or M (C2-I-092).
#[must_use]
pub fn is_word_or_mark(c: char) -> bool {
    is_word(c) || !c.is_ascii() && in_ranges(c, UNICODE_M)
}

/// A control (Cc) or format (Cf) character (C2-I-122).
#[must_use]
pub fn is_control_or_format(c: char) -> bool {
    if c <= '\u{009f}' {
        c <= '\u{001f}' || c >= '\u{007f}'
    } else {
        in_ranges(c, UNICODE_CF)
    }
}

/// Space or tab.
#[must_use]
pub const fn is_blank(c: char) -> bool {
    c == ' ' || c == '\t'
}

/// Whitespace in the sense of the preliminaries: space, tab or line break.
#[must_use]
pub const fn is_ws(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r')
}

/// A line that holds only spaces and tabs.
#[must_use]
pub fn is_blank_line(s: &str) -> bool {
    s.bytes().all(|b| b == b' ' || b == b'\t')
}

/// The lookup key of a label (C2-I-079): runs of spaces, tabs and line breaks
/// fold to one space, the ends are trimmed, case is kept.
#[must_use]
pub fn fold_label(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut space = false;
    for c in s.chars() {
        if matches!(c, ' ' | '\t' | '\n') {
            space = true;
        } else {
            if space && !out.is_empty() {
                out.push(' ');
            }
            space = false;
            out.push(c);
        }
    }
    out
}

/// A set of bytes as a table, for scans that test every byte.
pub struct ByteSet([bool; 256]);

impl ByteSet {
    /// The bytes of `a` and of `b`, plus every byte from `from` up when `from` is set.
    // Evaluated at compile time only: an index out of range is a build error, not a panic.
    #[allow(clippy::indexing_slicing)]
    #[must_use]
    pub const fn new(a: &[u8], b: &[u8], from: Option<u8>) -> Self {
        let mut t = [false; 256];
        let mut i = 0;
        while i < a.len() {
            t[a[i] as usize] = true;
            i += 1;
        }
        i = 0;
        while i < b.len() {
            t[b[i] as usize] = true;
            i += 1;
        }
        if let Some(f) = from {
            i = f as usize;
            while i < 256 {
                t[i] = true;
                i += 1;
            }
        }
        Self(t)
    }

    #[inline]
    #[must_use]
    pub fn has(&self, b: u8) -> bool {
        self.0.get(usize::from(b)) == Some(&true)
    }

    /// The offset of the first byte of `s` in the set.
    #[inline]
    #[must_use]
    pub fn find(&self, s: &[u8]) -> Option<usize> {
        s.iter().position(|&b| self.has(b))
    }
}
