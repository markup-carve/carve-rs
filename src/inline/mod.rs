//! The inline pass (C2-I): one content at a time, a scan that emits events,
//! and the builder that turns them into inline nodes.

mod abbr;
mod atoms;
mod build;
mod content;
mod delim;
mod direct;
mod events;
mod scan;
mod smart;
mod token;

pub(crate) use atoms::{Angle, Label, angle, code_span, comment_start, label};
pub(crate) use direct::{Seg, line_segments};

use crate::Arena;
use crate::ast::Node;
use crate::source::Source;
use std::collections::HashSet;

/// How a piece joins the piece before it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Join {
    /// A line break of the content (C2-I-003).
    Line,
    /// A continuation row fragment of a table cell (C2-B-184): a space in
    /// text, a line feed inside a node whose value is its source.
    Cell,
}

/// One source span of a content (C2-I-002). The block pass has removed
/// prefixes, markers and leading indent; trailing spaces and tabs are still
/// there and the inline pass drops them (C2-I-002).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Piece {
    pub start: usize,
    pub end: usize,
    pub join: Join,
}

/// The leaf a content belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentKind {
    Paragraph,
    Heading,
    Term,
    /// A caption: the first bare `#` is the caption number mark (C2-B-207).
    Caption,
    /// A table cell: padding trimmed at both ends, a backslash at the end is a hard break (C2-I-102).
    Cell,
    /// One line of a line block: leading spaces are content (C2-B-130), `%%` is text (C2-B-144).
    LineBlockLine,
}

/// What the inline pass needs from the rest of the document.
pub struct InlineEnv<'e, 'a> {
    pub src: &'e Source<'a>,
    /// Where the nodes and the strings that are no slice of the source go.
    pub arena: &'a Arena,
    /// Folded keys of the link definitions of the document (C2-I-072, C2-I-125).
    pub link_keys: &'e HashSet<String>,
    /// Abbreviation terms and expansions, first definition of a term only
    /// (C2-I-098), sorted by term: the matcher binary-searches it.
    pub abbreviations: &'e [(String, String)],
    pub kind: ContentKind,
    /// Inside a hard-break block: every line break is a hard break (C2-A-068).
    pub hard_breaks: bool,
}

/// [`InlineEnv`] with the definitions of the documents around this one kept
/// apart from its own.
pub(crate) struct Env<'e, 'a> {
    pub src: &'e Source<'a>,
    pub arena: &'a Arena,
    pub link_keys: Keys<'e>,
    pub abbreviations: Terms<'e>,
    pub kind: ContentKind,
    pub hard_breaks: bool,
    pub abbreviation_remaining: Option<&'e std::cell::Cell<usize>>,
}

/// The link keys of a document and of the documents around it, read in
/// place so that a small parse inside a large scope copies none of them.
#[derive(Clone, Copy)]
pub(crate) struct Keys<'e> {
    pub own: &'e HashSet<String>,
    pub outer: Option<&'e HashSet<String>>,
}

impl Keys<'_> {
    #[must_use]
    pub(crate) fn contains(&self, key: &str) -> bool {
        self.own.contains(key) || self.outer.is_some_and(|o| o.contains(key))
    }
}

/// The abbreviations of a document, then those of the documents around it
/// whose term it does not define itself. Both lists are sorted by term.
#[derive(Clone, Copy)]
pub(crate) struct Terms<'e> {
    pub own: &'e [(String, String)],
    pub outer: &'e [(String, String)],
}

impl<'e> Terms<'e> {
    #[must_use]
    pub(crate) fn is_empty(&self) -> bool {
        self.own.is_empty() && self.outer.is_empty()
    }

    /// The index of `term`: own terms first, the outer ones after them.
    #[must_use]
    pub(crate) fn find(&self, term: &str) -> Option<usize> {
        let by = |(t, _): &(String, String)| t.as_str().cmp(term);
        match self.own.binary_search_by(by) {
            Ok(i) => Some(i),
            Err(_) => self
                .outer
                .binary_search_by(by)
                .ok()
                .map(|j| self.own.len() + j),
        }
    }

    #[must_use]
    pub(crate) fn get(&self, idx: usize) -> Option<&'e (String, String)> {
        match idx.checked_sub(self.own.len()) {
            None => self.own.get(idx),
            Some(j) => self.outer.get(j),
        }
    }
}

/// The buffers of one inline pass, kept for the next content of the document.
#[derive(Default)]
pub struct Scratch<'a> {
    text: String,
    segs: Vec<content::Seg>,
    joins: Vec<(usize, Join)>,
    chars: Vec<usize>,
    toks: Vec<token::Tok<'a>>,
    evs: Vec<events::Ev<'a>>,
    /// Child lists of closed inline frames.
    lists: Vec<Vec<Node<'a>>>,
    /// Text pieces gathered for one text node.
    join: String,
    outs: Vec<smart::Out>,
    frames: Vec<build::Frame<'a>>,
    scan: scan::Bufs,
    stacks: Vec<Vec<delim::Entry>>,
    direct: Vec<direct::Seg>,
    /// Holds the buffers of the thread's spare, returned on drop.
    borrowed: bool,
}

/// A buffer this long is not kept: one huge content must not pin its memory.
const KEEP: usize = 1 << 16;

impl Scratch<'_> {
    fn trim(&mut self) {
        if self.text.capacity() > KEEP
            || self.toks.capacity() > KEEP / 16
            || self.evs.capacity() > KEEP / 16
            || self.direct.capacity() > KEEP / 16
        {
            // Dropped, not returned to the spare.
            self.borrowed = false;
            *self = Self::default();
        }
    }

    /// Takes the buffers of the full pass from the last tree of the thread,
    /// so a tree does not grow them from empty again. Only the full pass
    /// borrows: a tree whose contents all take the direct route pays nothing.
    fn borrow_spare(&mut self) {
        if self.borrowed {
            return;
        }
        self.borrowed = true;
        let _ = SPARE.try_with(|spare| {
            if let Ok(mut spare) = spare.try_borrow_mut() {
                self.exchange(&mut spare);
            }
        });
    }

    /// Whether any buffer is larger than the spare keeps: a tree may grow one
    /// that `trim` does not bound, and the spare outlives the tree.
    fn exceeds_keep(&self) -> bool {
        let cap = KEEP / 16;
        self.text.capacity() > KEEP
            || self.join.capacity() > KEEP
            || self.segs.capacity() > cap
            || self.joins.capacity() > cap
            || self.chars.capacity() > cap
            || self.toks.capacity() > cap
            || self.evs.capacity() > cap
            || self.outs.capacity() > cap
            || self.frames.capacity() > cap
            || self.direct.capacity() > cap
            || self.scan.exceeds(cap)
            || self.lists.len() > cap
            || self.lists.iter().any(|l| l.capacity() > cap)
            || self.stacks.len() > cap
            || self.stacks.iter().any(|l| l.capacity() > cap)
    }

    /// Swaps the buffers with `other`; those typed by the arena are emptied.
    /// Field by field: moving the whole scratch cost more than a small tree saves.
    fn exchange(&mut self, other: &mut Scratch<'_>) {
        use std::mem::{swap, take};
        swap(&mut self.text, &mut other.text);
        swap(&mut self.segs, &mut other.segs);
        swap(&mut self.joins, &mut other.joins);
        swap(&mut self.chars, &mut other.chars);
        swap(&mut self.join, &mut other.join);
        swap(&mut self.outs, &mut other.outs);
        swap(&mut self.scan, &mut other.scan);
        swap(&mut self.stacks, &mut other.stacks);
        let toks = recycle(take(&mut self.toks));
        self.toks = recycle(std::mem::replace(&mut other.toks, toks));
        let evs = recycle(take(&mut self.evs));
        self.evs = recycle(std::mem::replace(&mut other.evs, evs));
        let frames = recycle(take(&mut self.frames));
        self.frames = recycle(std::mem::replace(&mut other.frames, frames));
        let lists = take(&mut self.lists).into_iter().map(recycle).collect();
        self.lists = std::mem::replace(&mut other.lists, lists)
            .into_iter()
            .map(recycle)
            .collect();
    }
}

impl Drop for Scratch<'_> {
    fn drop(&mut self) {
        if !self.borrowed || self.exceeds_keep() {
            return;
        }
        let _ = SPARE.try_with(|spare| {
            if let Ok(mut spare) = spare.try_borrow_mut() {
                self.exchange(&mut spare);
            }
        });
    }
}

thread_local! {
    static SPARE: std::cell::RefCell<Scratch<'static>> = std::cell::RefCell::default();
}

/// An emptied vector with the allocation of `v`: the collect runs in place
/// when `T` and `U` have the same layout, as they do for one type under two
/// lifetimes.
fn recycle<T, U>(mut v: Vec<T>) -> Vec<U> {
    if v.capacity() == 0 {
        return Vec::new();
    }
    v.clear();
    v.into_iter().filter_map(|_| None).collect()
}

/// Parses one content into inline nodes with positions.
#[must_use]
pub fn parse<'a>(
    env: &InlineEnv<'_, 'a>,
    pieces: &[Piece],
    s: &mut Scratch<'a>,
) -> &'a mut [Node<'a>] {
    let env = Env {
        src: env.src,
        arena: env.arena,
        link_keys: Keys {
            own: env.link_keys,
            outer: None,
        },
        abbreviations: Terms {
            own: env.abbreviations,
            outer: &[],
        },
        kind: env.kind,
        hard_breaks: env.hard_breaks,
        abbreviation_remaining: None,
    };
    parse_env(&env, pieces, s)
}

/// [`parse`] of an [`Env`].
pub(crate) fn parse_env<'a>(
    env: &Env<'_, 'a>,
    pieces: &[Piece],
    s: &mut Scratch<'a>,
) -> &'a mut [Node<'a>] {
    let out = parse_in(env, pieces, s);
    s.trim();
    out
}

fn parse_in<'a>(env: &Env<'_, 'a>, pieces: &[Piece], s: &mut Scratch<'a>) -> &'a mut [Node<'a>] {
    #[cfg(debug_assertions)]
    let before = env.abbreviation_remaining.map(std::cell::Cell::get);
    match direct::route(env, pieces, &mut s.direct) {
        Ok(nodes) => {
            #[cfg(debug_assertions)]
            {
                let after = env.abbreviation_remaining.map(std::cell::Cell::get);
                if let (Some(remaining), Some(before)) = (env.abbreviation_remaining, before) {
                    remaining.set(before);
                }
                let full = full(env, pieces, s, Known::default());
                assert_eq!(
                    &*nodes, &*full,
                    "direct inline route differs from the full pass"
                );
                if let (Some(remaining), Some(after)) = (env.abbreviation_remaining, after) {
                    remaining.set(after);
                }
            }
            nodes
        }
        Err(known) => {
            let out = full(env, pieces, s, known);
            #[cfg(debug_assertions)]
            {
                let after = env.abbreviation_remaining.map(std::cell::Cell::get);
                if let (Some(remaining), Some(before)) = (env.abbreviation_remaining, before) {
                    remaining.set(before);
                }
                assert_eq!(
                    &*out,
                    &*full(env, pieces, s, Known::default()),
                    "the known plain prefix changes the full pass"
                );
                if let (Some(remaining), Some(after)) = (env.abbreviation_remaining, after) {
                    remaining.set(after);
                }
            }
            out
        }
    }
}

/// The full inline pass (C2-I-004).
fn full<'a>(
    env: &Env<'_, 'a>,
    pieces: &[Piece],
    s: &mut Scratch<'a>,
    known: Known,
) -> &'a mut [Node<'a>] {
    s.borrow_spare();
    let content = content::Content::new_in(env, pieces, s);
    let scan::Scanned { mut toks, comments } = scan::scan(
        &content,
        env,
        std::mem::take(&mut s.toks),
        &mut s.scan,
        known.scan,
    );
    let ctx = delim::Ctx {
        t: &content.text,
        comments: &comments,
    };
    delim::resolve(&mut toks, &ctx, env.arena, &mut s.stacks);
    let out = if env.abbreviations.is_empty() && comments.is_empty() {
        build::build(
            &content,
            &ctx,
            env,
            s,
            events::Stream::new(&mut toks),
            &[],
            known.smart,
        )
    } else {
        let mut evs = std::mem::take(&mut s.evs);
        events::from_tokens(&mut toks, &mut evs);
        let next_flow = build::next_flow(ctx.t, &evs);
        let out = build::build(
            &content,
            &ctx,
            env,
            s,
            evs.drain(..),
            &next_flow,
            known.smart,
        );
        s.evs = evs;
        out
    };
    s.toks = toks;
    s.scan.comments = comments;
    content.recycle(s);
    out
}

/// How far the start of a content is known to hold no form, in content
/// offsets: the scan starts its first text run at `scan`, smart typography
/// at `smart`.
#[derive(Clone, Copy, Default)]
struct Known {
    scan: usize,
    smart: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recycle_keeps_the_allocation() {
        let mut v: Vec<token::Tok<'static>> = Vec::with_capacity(64);
        v.push(token::Tok::new(0, 1, token::K::Nbsp));
        let at = v.as_ptr().cast::<u8>();
        let u: Vec<token::Tok<'_>> = recycle(v);
        assert!(u.is_empty());
        assert_eq!(u.capacity(), 64);
        assert_eq!(u.as_ptr().cast::<u8>(), at);
    }

    #[test]
    fn a_tree_starts_with_the_buffers_of_the_last_tree() {
        let src = "{^a {^b {^c {^d {^e {^f^}^}^}^}^}^} and more text\n";
        let arena = crate::Arena::new();
        let _ = crate::parse_resolved_in(src, &crate::Options::default(), &arena);
        let mut next = Scratch::default();
        next.borrow_spare();
        assert!(next.toks.capacity() > 0);
        assert!(next.frames.capacity() > 0);
        assert!(next.toks.is_empty() && next.frames.is_empty());
    }

    #[test]
    fn the_spare_drops_a_buffer_above_the_cap() {
        let mut big = Scratch::default();
        big.borrow_spare();
        big.segs = Vec::with_capacity(KEEP);
        drop(big);
        let mut next = Scratch::default();
        next.borrow_spare();
        assert!(next.segs.capacity() <= KEEP / 16);
    }
}
