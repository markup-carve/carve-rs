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

pub(crate) use atoms::{Label, code_span, label};
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
            *self = Self::default();
        }
    }
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
