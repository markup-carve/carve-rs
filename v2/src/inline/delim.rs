//! Step three and four of the scan (C2-I-004): the delimiter stack inside
//! each settled bracket construct and at top level (C2-I-007), then
//! attachment of attribute blocks (C2-I-009) and the caption mark (C2-B-207).

use super::scan::is_space;
use super::token::{Delim, K, Kind, Sort, Tok};
use crate::ast::{CaptionNumber, Node};
use crate::attr::AttrBlock;
use crate::chars::is_word;
use crate::tables;

/// What the delimiter pass reads besides the tokens.
pub struct Ctx<'a> {
    pub t: &'a str,
    pub comments: &'a [(usize, usize)],
    pub depth: usize,
}

impl Ctx<'_> {
    /// The character before `i`, braced comments skipped (C2-I-026).
    #[must_use]
    pub fn before(&self, i: usize) -> Option<char> {
        char_before(self.t, self.comments, i)
    }

    /// The character at `i`, braced comments skipped; a backslash before a
    /// space reads as the no-break space (C2-I-011).
    #[must_use]
    pub fn after(&self, mut i: usize) -> Option<char> {
        loop {
            let k = self.comments.partition_point(|&(s, _)| s < i);
            match self.comments.get(k) {
                Some(&(s, e)) if s == i => i = e,
                _ => break,
            }
        }
        let mut cs = self.t.get(i..)?.chars();
        let c = cs.next()?;
        Some(if c == '\\' && cs.next() == Some(' ') {
            '\u{a0}'
        } else {
            c
        })
    }
}

/// The character before `i` in `t`, braced comments (sorted ranges) skipped.
#[must_use]
pub fn char_before(t: &str, comments: &[(usize, usize)], mut i: usize) -> Option<char> {
    loop {
        let k = comments.partition_point(|&(_, e)| e < i);
        match comments.get(k) {
            Some(&(s, e)) if e == i => i = s,
            _ => break,
        }
    }
    t.get(..i).and_then(|s| s.chars().next_back())
}

#[derive(Clone, Copy)]
struct Entry {
    tok: usize,
    mark: char,
    braced: bool,
}

/// Pairs delimiters, attaches attribute blocks and settles the caption mark.
pub fn resolve(toks: &mut [Tok], ctx: &Ctx<'_>) {
    group(toks, 0, toks.len(), ctx.depth, ctx);
    attach(toks);
    caption(toks);
}

fn group(toks: &mut [Tok], lo: usize, hi: usize, base: usize, ctx: &Ctx<'_>) {
    let mut stack: Vec<Entry> = Vec::new();
    let mut k = lo;
    while k < hi {
        match toks.get(k).map(|t| &t.k) {
            Some(K::Open {
                sort: Sort::Bracket,
                close,
                ..
            }) => {
                let close = *close;
                group(toks, k + 1, close.min(hi), base + 1, ctx);
                k = close.max(k) + 1;
                continue;
            }
            Some(K::Delim(d)) => {
                let d = *d;
                delimiter(toks, &mut stack, k, d, base, ctx);
            }
            _ => {}
        }
        k += 1;
    }
    for e in stack {
        text(toks, e.tok);
    }
}

fn text(toks: &mut [Tok], k: usize) {
    if let Some(t) = toks.get_mut(k) {
        t.make_text(true);
    }
}

fn node_name(mark: char, braced: bool) -> Option<&'static str> {
    let form = if braced { "braced" } else { "bare" };
    tables::DELIMITERS
        .iter()
        .find(|&&(m, _, f)| m == mark && f == form)
        .map(|&(_, n, _)| n)
}

fn delimiter(
    toks: &mut [Tok],
    stack: &mut Vec<Entry>,
    k: usize,
    d: Delim,
    base: usize,
    ctx: &Ctx<'_>,
) {
    let room = base + stack.len() < tables::DEPTH_CAP;
    match d {
        Delim::Bare(c) => bare(toks, stack, k, c, room, ctx),
        Delim::BracedOpen(c) if room => stack.push(Entry {
            tok: k,
            mark: c,
            braced: true,
        }),
        Delim::BracedOpen(_) => text(toks, k),
        Delim::BracedClose { mark, alone } => {
            if let Some(j) = stack.iter().rposition(|e| e.braced && e.mark == mark) {
                let glued = stack.get(j).and_then(|e| toks.get(e.tok)).map(|t| t.v1)
                    == toks.get(k).map(|t| t.v0);
                if glued {
                    // C2-I-061: an opener glued to its own closer forms nothing.
                    let e = stack.remove(j);
                    text(toks, e.tok);
                    text(toks, k);
                } else {
                    close(toks, stack, j, k, Sort::Braced);
                }
            } else if alone && node_name(mark, false).is_some() {
                // C2-I-057: the mark is read by the bare rules and the brace is text.
                if let Some(t) = toks.get_mut(k) {
                    t.v1 = t.v0 + 1;
                    t.k = K::Delim(Delim::Bare(mark));
                }
                text(toks, k + 1);
                bare(toks, stack, k, mark, room, ctx);
            } else {
                text(toks, k);
            }
        }
    }
}

/// C2-I-047, C2-I-048, C2-I-051, C2-I-053.
fn bare(toks: &mut [Tok], stack: &mut Vec<Entry>, k: usize, c: char, room: bool, ctx: &Ctx<'_>) {
    let Some(&Tok { v0, v1, .. }) = toks.get(k) else {
        return;
    };
    let before = ctx.before(v0);
    let after = ctx.after(v1);
    let can_open = before.is_none_or(|b| !is_word(b)) && !is_space(after);
    let can_close = !is_space(before) && after.is_none_or(|a| !is_word(a));
    if can_close && let Some(j) = stack.iter().rposition(|e| !e.braced && e.mark == c) {
        let open = stack.get(j).map_or(k, |e| e.tok);
        if holds_own_mark(toks, open, k, c, ctx) {
            stack.remove(j);
            text(toks, open);
            text(toks, k);
        } else {
            close(toks, stack, j, k, Sort::Bare);
        }
        return;
    }
    if can_open && room {
        stack.push(Entry {
            tok: k,
            mark: c,
            braced: false,
        });
    } else {
        text(toks, k);
    }
}

/// C2-I-051, C2-I-059: the pair closes, every entry above it is text.
fn close(toks: &mut [Tok], stack: &mut Vec<Entry>, j: usize, k: usize, sort: Sort) {
    for e in stack.drain(j + 1..) {
        text(toks, e.tok);
    }
    let Some(e) = stack.pop() else { return };
    let Some(name) = node_name(e.mark, sort == Sort::Braced) else {
        text(toks, e.tok);
        text(toks, k);
        return;
    };
    if let Some(t) = toks.get_mut(e.tok) {
        t.k = K::Open {
            kind: Kind::Styled(name),
            sort,
            close: k,
        };
    }
    if let Some(t) = toks.get_mut(k) {
        t.k = K::Close {
            open: e.tok,
            attrs: None,
        };
    }
}

/// C2-I-053: the span's own content holds its mark between two word
/// characters; atoms, bracket constructs and braced spans do not count.
fn holds_own_mark(toks: &[Tok], open: usize, close: usize, c: char, ctx: &Ctx<'_>) -> bool {
    let mut m = open + 1;
    while m < close {
        let Some(t) = toks.get(m) else { break };
        match &t.k {
            K::Open {
                sort: Sort::Bracket | Sort::Braced,
                close: e,
                ..
            } => {
                m = (*e).max(m);
            }
            K::Text { val: None, .. } | K::Delim(_) if mark_between_words(ctx.t, t.v0, t.v1, c) => {
                return true;
            }
            _ => {}
        }
        m += 1;
    }
    false
}

fn mark_between_words(t: &str, v0: usize, v1: usize, c: char) -> bool {
    let Some(s) = t.get(v0..v1) else { return false };
    s.match_indices(c).any(|(p, _)| {
        let p = v0 + p;
        let before = t.get(..p).and_then(|x| x.chars().next_back());
        let after = t.get(p + c.len_utf8()..).and_then(|x| x.chars().next());
        before.is_some_and(is_word) && after.is_some_and(is_word)
    })
}

fn is_host(toks: &[Tok], p: usize) -> bool {
    match toks.get(p).map(|t| &t.k) {
        Some(K::Close { .. }) => true,
        Some(K::Leaf { host, .. }) => *host,
        _ => false,
    }
}

/// C2-I-036, C2-I-039: glued blocks attach to the host they follow.
fn attach(toks: &mut [Tok]) {
    let mut prev: Option<usize> = None;
    for k in 0..toks.len() {
        let Some(tok) = toks.get(k) else { break };
        match &tok.k {
            K::Skip => continue,
            K::Attr(_) => {
                let v0 = tok.v0;
                let v1 = tok.v1;
                let glued = prev.and_then(|p| toks.get(p)).is_some_and(|t| t.v1 == v0);
                if let Some(p) = prev.filter(|&p| glued && is_host(toks, p)) {
                    let Some(t) = toks.get_mut(k) else { break };
                    let block = match std::mem::replace(&mut t.k, K::Skip) {
                        K::Attr(b) => *b,
                        _ => AttrBlock::default(),
                    };
                    if let Some(host) = toks.get_mut(p) {
                        add_attrs(host, block);
                        host.v1 = v1;
                    }
                    continue;
                }
                // C2-I-035: an unattached block is printed as written.
                if let Some(t) = toks.get_mut(k) {
                    t.make_text(false);
                }
            }
            _ => {}
        }
        prev = Some(k);
    }
}

fn add_attrs(host: &mut Tok, block: AttrBlock) {
    match &mut host.k {
        K::Close { attrs, .. } => match attrs {
            Some(a) => a.merge(block),
            None => *attrs = Some(Box::new(block)),
        },
        K::Leaf { node, .. } => {
            if let Some(slot) = node.attrs_slot() {
                let mut a = slot.take().unwrap_or_default();
                a.merge(block.attrs);
                *slot = (!a.is_empty()).then_some(a);
            }
            if block.lang.is_some() {
                super::build::set_lang(node, block.lang);
            }
        }
        _ => {}
    }
}

/// C2-B-207: the first bare `#` outside inline markup is the caption mark.
fn caption(toks: &mut [Tok]) {
    let mut depth = 0usize;
    let mut done = false;
    for t in toks.iter_mut() {
        match t.k {
            K::Open { .. } => depth += 1,
            K::Close { .. } => depth = depth.saturating_sub(1),
            K::CaptionMark if depth == 0 && !done => {
                done = true;
                t.k = K::Leaf {
                    node: Box::new(Node::CaptionNumber(CaptionNumber::default())),
                    host: false,
                    transparent: false,
                };
            }
            K::CaptionMark => t.make_text(true),
            _ => {}
        }
    }
}
