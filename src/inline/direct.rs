//! A direct route for short contents: plain runs, bare `*` and `/` pairs
//! with plain text inside, and code spans, on one line each. The first byte
//! the route cannot decide sends the content to the full pass, which is the
//! reference: debug builds run both and compare the trees.

use super::{ContentKind, Env, Join, Known, Piece};
use crate::ast::{Code, Emphasis, Node, SoftBreak, Strong, Text};

/// Longer contents go to the full pass: a late byte the route cannot
/// decide would cost the scan of everything before it.
const SHORT: usize = 96;

/// One part of a piece, in source offsets.
#[derive(Clone, Copy)]
pub(super) enum Seg {
    Text(usize, usize),
    /// A bare pair: the mark, the offset of the opener and the closer.
    Pair(u8, usize, usize),
    /// A code span: its start and end, then the value.
    Code(usize, usize, usize, usize),
    Break(usize, usize),
}

/// Bytes the route reads as text; `.` only outside a `..` run.
static TEXT: crate::chars::ByteSet = crate::chars::ByteSet::new(
    b"0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ",
    b" .,;?)",
    None,
);

/// Every byte the route reads.
static READ: crate::chars::ByteSet = crate::chars::ByteSet::new(
    b"0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ",
    b" .,;?)*/`",
    None,
);

/// Bytes that may follow a closer: they are no word character and start nothing.
fn ends_pair(b: Option<&u8>) -> bool {
    b.is_none_or(|&c| matches!(c, b' ' | b'.' | b',' | b';' | b'?' | b')'))
}

/// The nodes of a content the route decides. Otherwise how much of the
/// start of the content is known to hold no form, for the full pass.
pub(super) fn route<'a>(
    env: &Env<'_, 'a>,
    pieces: &[Piece],
    segs: &mut Vec<Seg>,
) -> Result<&'a mut [Node<'a>], Known> {
    let cell = env.kind == ContentKind::Cell;
    if !env.abbreviations.is_empty()
        || pieces.is_empty()
        || (pieces.len() > 1 && (env.hard_breaks || cell))
        || pieces.iter().any(|p| p.join != Join::Line)
    {
        return Err(Known::default());
    }
    let short = pieces.iter().map(|p| p.end - p.start).sum::<usize>() <= SHORT;
    let src = env.src;
    segs.clear();
    let mut prev_end = None;
    let mut first = Known::default();
    for (n, p) in pieces.iter().enumerate() {
        let Some(t) = src.borrowed(p.start, p.end) else {
            return Err(Known::default());
        };
        let mut b = t.as_bytes();
        let mut start = p.start;
        while let [rest @ .., b' ' | b'\t'] = b {
            b = rest;
        }
        if cell {
            while let [b' ' | b'\t', rest @ ..] = b {
                b = rest;
                start += 1;
            }
        }
        match b.first() {
            Some(c) if c.is_ascii_alphanumeric() => {}
            // A content that starts with a form is mostly no simple one:
            // every byte is tested before any other work.
            Some(b'*' | b'/' | b'`') if short && b.iter().all(|&c| READ.has(c)) => {}
            _ => return Err(first),
        }
        if let Some(e) = prev_end {
            segs.push(Seg::Break(e, start));
        }
        let (stop, comma) = prefix(b);
        if n == 0 {
            // The content text starts at the first piece; a line block keeps its indent.
            if env.kind != ContentKind::LineBlockLine
                && b.first().is_some_and(u8::is_ascii_alphanumeric)
                && stop < b.len()
            {
                first = known(b, stop, comma);
            }
            if !later_lines_open(src, pieces, short) {
                return Err(first);
            }
        }
        if piece(b, start, segs, short, stop).is_none() {
            return Err(first);
        }
        prev_end = Some(start + b.len());
    }
    Ok(nodes(env, segs))
}

/// Whether every piece after the first starts with a byte the route reads
/// there, checked before the first piece is read.
fn later_lines_open(src: &crate::source::Source<'_>, pieces: &[Piece], short: bool) -> bool {
    pieces.iter().skip(1).all(|p| {
        src.borrowed(p.start, p.end)
            .and_then(|t| t.as_bytes().first())
            .is_some_and(|&c| {
                c.is_ascii_alphanumeric() || (short && matches!(c, b'*' | b'/' | b'`'))
            })
    })
}

/// Where the plain text at the start of `b` ends: at the first byte that is
/// no text, or behind the first dot of a `..`; and its first comma.
fn prefix(b: &[u8]) -> (usize, Option<usize>) {
    let mut comma = None;
    for (i, &c) in b.iter().enumerate() {
        if !TEXT.has(c) {
            return (i, comma);
        }
        if c == b'.' && b.get(i + 1) == Some(&b'.') {
            return (i + 1, comma);
        }
        if c == b',' && comma.is_none() {
            comma = Some(i);
        }
    }
    (b.len(), comma)
}

/// What the full pass may skip of a first piece whose plain text ends at
/// `stop`: `.` starts only the `...` run (smart typography), `,` a braced
/// mark before `}`.
fn known(b: &[u8], stop: usize, comma: Option<usize>) -> Known {
    Known {
        scan: comma.unwrap_or(stop),
        smart: if stop > 0 && b.get(stop - 1) == Some(&b'.') {
            stop - 1
        } else {
            stop
        },
    }
}

/// The parts of one trimmed piece starting at source offset `at`, whose
/// plain text ends at `stop`; pairs and code spans only in a short content.
fn piece(b: &[u8], at: usize, segs: &mut Vec<Seg>, short: bool, stop: usize) -> Option<()> {
    if b.get(stop).is_some_and(|&c| TEXT.has(c)) {
        // The second dot of a `..`.
        return None;
    }
    let mut i = stop;
    let mut text_from = 0;
    while let Some(&c) = b.get(i) {
        if TEXT.has(c) {
            if c == b'.' && b.get(i + 1) == Some(&b'.') {
                return None;
            }
            i += 1;
            continue;
        }
        let end = match c {
            b'`' if short => code(b, i, at, segs, text_from)?,
            b'*' | b'/' if short => pair(b, i, at, segs, text_from)?,
            _ => return None,
        };
        i = end;
        text_from = end;
    }
    if text_from < b.len() {
        segs.push(Seg::Text(at + text_from, at + b.len()));
    }
    Some(())
}

fn flush(segs: &mut Vec<Seg>, at: usize, from: usize, to: usize) {
    if from < to {
        segs.push(Seg::Text(at + from, at + to));
    }
}

/// A code span at `i` closed on the same piece (C2-I-015, C2-I-016).
fn code(b: &[u8], i: usize, at: usize, segs: &mut Vec<Seg>, from: usize) -> Option<usize> {
    let run = |k: usize| {
        b.get(k..)
            .map_or(0, |s| s.iter().take_while(|&&x| x == b'`').count())
    };
    let n = run(i);
    let mut j = i + n;
    while j < b.len() {
        if b.get(j) == Some(&b'`') {
            let m = run(j);
            if m == n {
                let (mut v0, mut v1) = (i + n, j);
                let inner = b.get(v0..v1)?;
                if inner.len() >= 2
                    && inner.first() == Some(&b' ')
                    && inner.last() == Some(&b' ')
                    && inner.iter().any(|&x| x != b' ')
                {
                    v0 += 1;
                    v1 -= 1;
                }
                flush(segs, at, from, i);
                segs.push(Seg::Code(at + i, at + j + m, at + v0, at + v1));
                return Some(j + m);
            }
            j += m;
        } else {
            j += 1;
        }
    }
    None
}

/// A single bare mark at `i` opening a pair whose content is plain text
/// starting and ending with a word character (C2-I-047, C2-I-048).
fn pair(b: &[u8], i: usize, at: usize, segs: &mut Vec<Seg>, from: usize) -> Option<usize> {
    let c = *b.get(i)?;
    if i > 0 && b.get(i - 1) != Some(&b' ') {
        return None;
    }
    if !b.get(i + 1)?.is_ascii_alphanumeric() {
        return None;
    }
    let mut j = i + 1;
    while let Some(&x) = b.get(j) {
        if !TEXT.has(x) {
            break;
        }
        if x == b'.' && b.get(j + 1) == Some(&b'.') {
            return None;
        }
        j += 1;
    }
    if b.get(j) != Some(&c) || !b.get(j - 1)?.is_ascii_alphanumeric() || !ends_pair(b.get(j + 1)) {
        return None;
    }
    flush(segs, at, from, i);
    segs.push(Seg::Pair(c, at + i, at + j));
    Some(j + 1)
}

fn nodes<'a>(env: &Env<'_, 'a>, segs: &[Seg]) -> &'a mut [Node<'a>] {
    let (src, a) = (env.src, env.arena);
    let text = |s: usize, e: usize| {
        Node::Text(a.alloc(Text {
            value: src.borrowed(s, e).unwrap_or_default(),
            pos: Some(src.pos(s, e)),
            ..Default::default()
        }))
    };
    a.fill(segs.iter().map(|&seg| match seg {
        Seg::Text(s, e) => text(s, e),
        Seg::Pair(c, o, k) => {
            let children = a.one(text(o + 1, k));
            let pos = Some(src.pos(o, k + 1));
            if c == b'*' {
                Node::Strong(a.alloc(Strong {
                    children,
                    pos,
                    ..Default::default()
                }))
            } else {
                Node::Emphasis(a.alloc(Emphasis {
                    children,
                    pos,
                    ..Default::default()
                }))
            }
        }
        Seg::Code(s, e, v0, v1) => Node::Code(a.alloc(Code {
            value: src.borrowed(v0, v1).unwrap_or_default(),
            pos: Some(src.pos(s, e)),
            ..Default::default()
        })),
        Seg::Break(s, e) => Node::SoftBreak(a.alloc(SoftBreak {
            pos: Some(src.pos(s, e)),
            ..Default::default()
        })),
    }))
}

#[cfg(test)]
mod tests {
    use super::super::{Keys, Scratch, Terms, full};
    use super::*;
    use crate::Arena;
    use crate::source::Source;
    use std::collections::HashSet;

    #[test]
    fn short_contents_take_the_route_and_match_the_full_pass() {
        let keys = HashSet::new();
        for (input, kind, routed) in [
            ("item 0 alpha delta *em* `c0`", ContentKind::Paragraph, true),
            (
                "a /b c/ and ` x ` then `` a`b ``.",
                ContentKind::Paragraph,
                true,
            ),
            ("*n* `3`", ContentKind::Cell, true),
            ("plain words, more words.", ContentKind::Heading, true),
            ("a*b* c", ContentKind::Paragraph, false),
            ("a *b", ContentKind::Paragraph, false),
            ("*a:* b", ContentKind::Paragraph, false),
            ("a **b**", ContentKind::Paragraph, false),
            ("see [x](y)", ContentKind::Paragraph, false),
            ("wait... yes", ContentKind::Paragraph, false),
            ("`open", ContentKind::Paragraph, false),
        ] {
            let src = Source::new(input);
            let arena = Arena::new();
            let env = Env {
                src: &src,
                arena: &arena,
                link_keys: Keys {
                    own: &keys,
                    outer: None,
                },
                abbreviations: Terms {
                    own: &[],
                    outer: &[],
                },
                kind,
                hard_breaks: false,
            };
            let pieces = [Piece {
                start: 0,
                end: input.len(),
                join: Join::Line,
            }];
            let got = route(&env, &pieces, &mut Vec::new());
            assert_eq!(got.is_ok(), routed, "{input}");
            if let Ok(nodes) = got {
                let reference = full(&env, &pieces, &mut Scratch::default(), Known::default());
                assert_eq!(&*nodes, &*reference, "{input}");
            }
        }
    }
}
