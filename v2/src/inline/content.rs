//! One content as the scan reads it (C2-I-002, C2-I-003): the pieces joined
//! by one line feed each, with the map from content offsets back to source
//! offsets.

use super::{ContentKind, InlineEnv, Join, Piece};
use crate::ast::Pos;
use crate::source::Source;

#[derive(Debug, Clone, Copy)]
struct Seg {
    /// Offset of the piece in the content text.
    v: usize,
    /// Offset of the piece in the source.
    s: usize,
    len: usize,
    /// Source end of the piece before its trailing spaces were dropped.
    raw_end: usize,
}

/// The joined content text and its source map.
pub struct Content<'a> {
    pub text: String,
    segs: Vec<Seg>,
    /// Content offset of each join character and its kind, in order.
    joins: Vec<(usize, Join)>,
    src: &'a Source,
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

impl<'a> Content<'a> {
    /// Joins the pieces of one content.
    #[must_use]
    pub fn new(env: &InlineEnv<'a>, pieces: &[Piece]) -> Self {
        let full = env.src.text();
        let b = full.as_bytes();
        let mut text = String::new();
        let mut segs = Vec::with_capacity(pieces.len());
        let mut joins = Vec::new();
        for (n, p) in pieces.iter().enumerate() {
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
            if n > 0 && p.join == Join::Cell && start == end {
                // C2-B-184: an empty fragment adds nothing.
                continue;
            }
            if !segs.is_empty() {
                joins.push((text.len(), p.join));
                text.push('\n');
            }
            let piece = full.get(start..end).unwrap_or("");
            segs.push(Seg {
                v: text.len(),
                s: start,
                len: piece.len(),
                raw_end,
            });
            text.push_str(piece);
        }
        Self {
            text,
            segs,
            joins,
            src: env.src,
        }
    }

    /// Source offset of a content offset. The offset of a join character
    /// maps to the end of the piece before it.
    #[must_use]
    pub fn src_off(&self, v: usize) -> usize {
        let i = self.segs.partition_point(|s| s.v <= v).saturating_sub(1);
        self.segs
            .get(i)
            .map_or(0, |s| s.s + (v.saturating_sub(s.v)).min(s.len))
    }

    /// The tree position of a content range.
    #[must_use]
    pub fn pos(&self, v0: usize, v1: usize) -> Pos {
        self.src.pos(self.src_off(v0), self.src_off(v1))
    }

    /// The source end of the line before the join at `v`, its trailing
    /// spaces included; `src_off(v)` when `v` is no join.
    fn raw_line_end(&self, v: usize) -> usize {
        let i = self.segs.partition_point(|s| s.v <= v).saturating_sub(1);
        match self.segs.get(i) {
            Some(s) if s.v + s.len == v && self.join_at(v).is_some() => s.raw_end,
            _ => self.src_off(v),
        }
    }

    /// GAP INL-011: a line comment before a join owns the trailing spaces of its line.
    #[must_use]
    pub fn pos_to_line_end(&self, v0: usize, v1: usize) -> Pos {
        self.src.pos(self.src_off(v0), self.raw_line_end(v1))
    }

    /// The break after such a comment starts behind those spaces.
    #[must_use]
    pub fn pos_from_line_end(&self, v0: usize, v1: usize) -> Pos {
        self.src.pos(self.raw_line_end(v0), self.src_off(v1))
    }

    /// The join kind of the line feed at `v`, when `v` is a join.
    #[must_use]
    pub fn join_at(&self, v: usize) -> Option<Join> {
        let i = self.joins.partition_point(|&(p, _)| p < v);
        self.joins.get(i).filter(|&&(p, _)| p == v).map(|&(_, j)| j)
    }

    /// True when a continuation row joined a fragment (C2-B-184).
    #[must_use]
    pub fn cell_joined(&self) -> bool {
        self.joins.iter().any(|&(_, j)| j == Join::Cell)
    }

    /// True when the source of `v0..v1` is one contiguous slice.
    #[must_use]
    pub fn contiguous(&self, v0: usize, v1: usize) -> bool {
        let i = self.joins.partition_point(|&(p, _)| p < v0);
        self.joins.get(i).is_none_or(|&(p, _)| p >= v1)
    }
}
