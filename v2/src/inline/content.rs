//! One content as the scan reads it (C2-I-002, C2-I-003): the pieces joined
//! by one line feed each, with the map from content offsets back to source
//! offsets.

use super::{ContentKind, InlineEnv, Join, Piece};
use crate::Arena;
use crate::ast::Pos;
use crate::source::Source;

#[derive(Debug, Clone, Copy)]
pub(super) struct Seg {
    /// Offset of the piece in the content text.
    v: usize,
    /// Offset of the piece in the source.
    s: usize,
    len: usize,
    /// Source end of the piece before its trailing spaces were dropped.
    raw_end: usize,
    /// 1-based line and column of `s`.
    line: usize,
    col: usize,
}

/// The joined content text and its source map.
pub struct Content<'e, 'a> {
    pub text: String,
    segs: Vec<Seg>,
    /// Content offset of each join character and its kind, in order.
    joins: Vec<(usize, Join)>,
    /// Number of characters in `text[..i]`, for columns in linear time;
    /// empty when the text is ASCII and the count is `i`.
    chars: Vec<usize>,
    /// A continuation row joined a fragment, computed once (linear time).
    cell_joined: bool,
    src: &'e Source<'a>,
    arena: &'a Arena,
}

const fn is_pad(b: u8) -> bool {
    b == b' ' || b == b'\t'
}

/// C2-I-002: trailing spaces and tabs go, but a space a backslash escapes stays.
fn trim_end(b: &[u8], start: usize, mut end: usize) -> usize {
    let full = end;
    while end > start && b.get(end - 1).copied().is_some_and(is_pad) {
        end -= 1;
    }
    if end < full && b.get(end) == Some(&b' ') {
        let mut slashes = 0;
        while end > start + slashes && b.get(end - 1 - slashes) == Some(&b'\\') {
            slashes += 1;
        }
        if slashes % 2 == 1 {
            end += 1;
        }
    }
    end
}

#[inline]
fn piece_bounds(env: &InlineEnv<'_, '_>, p: &Piece, full: &str) -> (usize, usize, usize) {
    let b = full.as_bytes();
    let end = p.end.min(full.len());
    let mut start = p.start.min(end);
    if env.kind != ContentKind::LineBlockLine {
        while start < end && b.get(start).copied().is_some_and(is_pad) {
            start += 1;
        }
    }
    let raw_end = end;
    let end = if env.kind == ContentKind::Cell {
        // C2-I-002: the cell padding goes first, so a backslash before it is a hard break (GAP INL-017).
        let mut e = end;
        while e > start && b.get(e - 1).copied().is_some_and(is_pad) {
            e -= 1;
        }
        e
    } else {
        trim_end(b, start, end)
    };
    (start, end, raw_end)
}

impl<'e, 'a> Content<'e, 'a> {
    /// Joins the pieces of one content, in the buffers of `s`.
    pub(super) fn new_in(
        env: &InlineEnv<'e, 'a>,
        pieces: &[Piece],
        s: &mut super::Scratch<'a>,
    ) -> Self {
        let full = env.src.text();
        let mut text = std::mem::take(&mut s.text);
        text.clear();
        text.reserve(
            pieces
                .iter()
                .map(|p| p.end.saturating_sub(p.start) + 1)
                .sum(),
        );
        let mut segs = std::mem::take(&mut s.segs);
        segs.clear();
        let mut joins = std::mem::take(&mut s.joins);
        joins.clear();
        for (n, p) in pieces.iter().enumerate() {
            let (start, end, raw_end) = piece_bounds(env, p, full);
            if n > 0 && p.join == Join::Cell && start == end {
                // C2-B-184: an empty fragment adds nothing.
                continue;
            }
            if p.join == Join::Cell && text.is_empty() {
                // C2-B-184: an empty cell has no text for the space to follow.
                segs.clear();
            }
            if !segs.is_empty() {
                joins.push((text.len(), p.join));
                text.push('\n');
            }
            let piece = full.get(start..end).unwrap_or("");
            let (line, col) = env.src.line_col(start);
            segs.push(Seg {
                v: text.len(),
                s: start,
                len: piece.len(),
                raw_end,
                line,
                col,
            });
            text.push_str(piece);
        }
        let mut chars = std::mem::take(&mut s.chars);
        chars.clear();
        if !text.is_ascii() {
            chars.reserve(text.len() + 1);
            let mut n = 0;
            chars.push(0);
            for b in text.bytes() {
                if b & 0xC0 != 0x80 {
                    n += 1;
                }
                chars.push(n);
            }
        }
        let cell_joined = joins.iter().any(|&(_, j)| j == Join::Cell);
        Self {
            text,
            segs,
            joins,
            chars,
            cell_joined,
            src: env.src,
            arena: env.arena,
        }
    }

    /// Hands the buffers back for the next content.
    pub(super) fn recycle(self, s: &mut super::Scratch<'a>) {
        s.text = self.text;
        s.segs = self.segs;
        s.joins = self.joins;
        s.chars = self.chars;
    }

    /// Line, column and source offset of a content offset.
    fn point(&self, v: usize) -> (usize, usize, usize) {
        let i = self.segs.partition_point(|s| s.v <= v).saturating_sub(1);
        let Some(seg) = self.segs.get(i) else {
            return (1, 1, 0);
        };
        let d = v.saturating_sub(seg.v).min(seg.len);
        let count = |x: usize| {
            if !self.chars.is_empty() {
                self.chars.get(x).copied().unwrap_or(0)
            } else if x <= self.text.len() {
                x
            } else {
                0
            }
        };
        (
            seg.line,
            seg.col + count(seg.v + d) - count(seg.v),
            seg.s + d,
        )
    }

    /// The point at the end of the line before the join at `v`, its
    /// trailing spaces included; `point(v)` when `v` is no join.
    fn raw_point(&self, v: usize) -> (usize, usize, usize) {
        let i = self.segs.partition_point(|s| s.v <= v).saturating_sub(1);
        let (line, col, off) = self.point(v);
        match self.segs.get(i) {
            Some(s) if s.v + s.len == v && (self.join_at(v).is_some() || v == self.text.len()) => {
                (line, col + (s.raw_end - off), s.raw_end)
            }
            _ => (line, col, off),
        }
    }

    fn make(&self, a: (usize, usize, usize), b: (usize, usize, usize)) -> Pos {
        Pos {
            start_line: Pos::at(a.0),
            end_line: Pos::at(b.0),
            start_column: Pos::at(a.1),
            end_column: Pos::at(b.1),
            start_offset: Pos::at(self.src.original_offset(a.2)),
            end_offset: Pos::at(self.src.original_offset(b.2)),
        }
    }

    /// The tree position of a content range.
    #[must_use]
    pub fn pos(&self, v0: usize, v1: usize) -> Pos {
        self.make(self.point(v0), self.point(v1))
    }

    /// GAP INL-011: a line comment before a join owns the trailing spaces of its line.
    #[must_use]
    pub fn pos_to_line_end(&self, v0: usize, v1: usize) -> Pos {
        self.make(self.point(v0), self.raw_point(v1))
    }

    /// The break after such a comment starts behind those spaces.
    #[must_use]
    pub fn pos_from_line_end(&self, v0: usize, v1: usize) -> Pos {
        self.make(self.raw_point(v0), self.point(v1))
    }

    /// The join kind of the line feed at `v`, when `v` is a join.
    #[must_use]
    pub fn join_at(&self, v: usize) -> Option<Join> {
        let i = self.joins.partition_point(|&(p, _)| p < v);
        self.joins.get(i).filter(|&&(p, _)| p == v).map(|&(_, j)| j)
    }

    /// True when a continuation row joined a fragment (C2-B-184).
    #[must_use]
    pub const fn cell_joined(&self) -> bool {
        self.cell_joined
    }

    /// A string that lives as long as the tree: the source itself when `s`
    /// is a slice of the content text that lies in one piece, else a copy.
    #[must_use]
    pub fn keep(&self, s: &str) -> &'a str {
        self.borrow(s).unwrap_or_else(|| self.arena.str(s))
    }

    pub(super) fn borrow(&self, s: &str) -> Option<&'a str> {
        let v = s.as_ptr().addr().checked_sub(self.text.as_ptr().addr())?;
        let end = v + s.len();
        if end > self.text.len() {
            return None;
        }
        let i = self.segs.partition_point(|g| g.v <= v).checked_sub(1)?;
        let g = self.segs.get(i)?;
        if end > g.v + g.len {
            return None;
        }
        let at = g.s + (v - g.v);
        self.src.borrowed(at, at + s.len())
    }

    /// The arena of the tree.
    #[must_use]
    pub const fn arena(&self) -> &'a Arena {
        self.arena
    }

    /// The source text of `v0..v1` as `keep` gives it.
    #[must_use]
    pub fn slice(&self, v0: usize, v1: usize) -> &'a str {
        self.keep(self.text.get(v0..v1).unwrap_or(""))
    }

    /// True when the source of `v0..v1` is one contiguous slice.
    #[must_use]
    pub fn contiguous(&self, v0: usize, v1: usize) -> bool {
        let i = self.joins.partition_point(|&(p, _)| p < v0);
        self.joins.get(i).is_none_or(|&(p, _)| p >= v1)
    }
}
