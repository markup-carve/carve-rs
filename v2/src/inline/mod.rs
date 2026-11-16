//! The inline pass (C2-I): one content at a time, a scan that emits events,
//! and the builder that turns them into inline nodes.

mod abbr;
mod atoms;
mod build;
mod content;
mod delim;
mod events;
mod scan;
mod smart;
mod token;

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
    /// Open block containers around the content, for the depth cap (C2-I-118).
    pub depth: usize,
    pub kind: ContentKind,
    /// Inside a hard-break block: every line break is a hard break (C2-A-068).
    pub hard_breaks: bool,
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
}

/// A buffer this long is not kept: one huge content must not pin its memory.
const KEEP: usize = 1 << 16;

impl Scratch<'_> {
    fn trim(&mut self) {
        if self.text.capacity() > KEEP
            || self.toks.capacity() > KEEP / 16
            || self.evs.capacity() > KEEP / 16
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
    let out = parse_in(env, pieces, s);
    s.trim();
    out
}

fn parse_in<'a>(
    env: &InlineEnv<'_, 'a>,
    pieces: &[Piece],
    s: &mut Scratch<'a>,
) -> &'a mut [Node<'a>] {
    let content = content::Content::new_in(env, pieces, s);
    let scanned = scan::scan(&content, env, std::mem::take(&mut s.toks));
    let mut toks = scanned.toks;
    let ctx = delim::Ctx {
        t: &content.text,
        comments: &scanned.comments,
        depth: env.depth,
    };
    delim::resolve(&mut toks, &ctx, env.arena);
    events::from_tokens(&mut toks, &mut s.evs);
    s.toks = toks;
    let out = build::build(&content, &ctx, env, s);
    content.recycle(s);
    out
}
