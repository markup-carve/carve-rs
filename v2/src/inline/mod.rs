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

pub(crate) use atoms::{Label, code_span, comment_start, label};

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
    frames: Vec<build::Frame<'a>>,
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
    if let Some(nodes) = plain_piece(env, pieces) {
        return nodes;
    }
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

/// Plain lines (`plain_span`) with no abbreviation defined and no hard break
/// block: text nodes with a soft break between them, as the full pass builds.
fn plain_piece<'a>(env: &InlineEnv<'_, 'a>, pieces: &[Piece]) -> Option<&'a mut [Node<'a>]> {
    let multi = pieces.len() > 1;
    if !env.abbreviations.is_empty()
        || (multi && (env.hard_breaks || env.kind == ContentKind::Cell))
        || pieces.iter().any(|p| p.join != Join::Line)
    {
        return None;
    }
    let mut spans = [(0, ""); 8];
    if pieces.is_empty() || pieces.len() > spans.len() {
        return None;
    }
    for (slot, p) in spans.iter_mut().zip(pieces) {
        *slot = plain_span(env, p)?;
    }
    let spans = spans.get(..pieces.len())?;
    let (src, a) = (env.src, env.arena);
    Some(a.fill((0..spans.len() * 2 - 1).map(|i| {
        let (start, t) = spans.get(i / 2).copied().unwrap_or_default();
        let end = start + t.len();
        if i % 2 == 0 {
            let pos = Some(src.pos(start, end));
            Node::Text(a.alloc(crate::ast::Text {
                value: t,
                pos,
                ..Default::default()
            }))
        } else {
            let next = spans.get(i / 2 + 1).map_or(end, |s| s.0);
            let pos = Some(src.pos(end, next));
            Node::SoftBreak(a.alloc(crate::ast::SoftBreak {
                pos,
                ..Default::default()
            }))
        }
    })))
}

/// The start and text of a piece of ASCII letters, digits, inner spaces and
/// a few punctuation marks: nothing in it starts a construct.
fn plain_span<'a>(env: &InlineEnv<'_, 'a>, p: &Piece) -> Option<(usize, &'a str)> {
    let t = env.src.borrowed(p.start, p.end)?;
    let trimmed = if env.kind == ContentKind::Cell {
        t.trim_matches([' ', '\t'])
    } else {
        t.trim_end_matches([' ', '\t'])
    };
    let b = trimmed.as_bytes();
    // `.` starts only the `...` run (smart typography); the others start nothing.
    let plain =
        |c: &u8| c.is_ascii_alphanumeric() || matches!(c, b' ' | b'.' | b',' | b';' | b'?' | b')');
    if !b.first().is_some_and(u8::is_ascii_alphanumeric)
        || b.last() == Some(&b' ')
        || !b.iter().all(plain)
        || trimmed.contains("..")
    {
        return None;
    }
    Some((
        p.start + (t.len() - t.trim_start_matches([' ', '\t']).len()),
        trimmed,
    ))
}
