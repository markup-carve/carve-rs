//! The token list the scan builds and the delimiter pass rewrites in place.
//! Offsets are content offsets (`Content::text`).

use crate::ast::{LinkRefForm, Node};
use crate::attr::AttrBlock;

/// A container node with inline children.
#[derive(Debug, Clone, PartialEq)]
pub enum Kind {
    /// A delimiter span; the node type name from `tables::DELIMITERS`.
    Styled(&'static str),
    Span,
    Link {
        href: Option<String>,
        tooltip: Option<String>,
        key: Option<String>,
        form: Option<LinkRefForm>,
    },
    InlineFootnote,
    Extension(String),
}

/// How an opener formed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sort {
    Bracket,
    Bare,
    Braced,
}

/// A delimiter that the delimiter pass has not decided yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delim {
    Bare(char),
    BracedOpen(char),
    /// A braced closer; `alone` is false when marks of its run stand before it (C2-I-057).
    BracedClose {
        mark: char,
        alone: bool,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum K {
    /// Text; `val` is `None` when the value is the content slice. `plain`
    /// text is open to smart typography and abbreviations.
    Text {
        val: Option<String>,
        plain: bool,
    },
    Nbsp,
    Break {
        hard: bool,
    },
    /// A continuation-row join (C2-B-184): a space in text.
    CellJoin,
    /// An atom node. `transparent` is the braced comment (C2-I-026).
    Leaf {
        node: Box<Node>,
        host: bool,
        transparent: bool,
    },
    Attr(Box<AttrBlock>),
    Delim(Delim),
    /// A bracket opener on the stack, not decided yet.
    Pending,
    Open {
        kind: Kind,
        sort: Sort,
        close: usize,
    },
    Close {
        open: usize,
        attrs: Option<Box<AttrBlock>>,
    },
    CaptionMark,
    Skip,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Tok {
    pub v0: usize,
    pub v1: usize,
    pub k: K,
}

impl Tok {
    #[must_use]
    pub const fn new(v0: usize, v1: usize, k: K) -> Self {
        Self { v0, v1, k }
    }

    /// Turns the token into text of its own source.
    pub fn make_text(&mut self, plain: bool) {
        self.k = K::Text { val: None, plain };
    }
}
