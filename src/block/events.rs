//! The events of the block pass: the boundary between block parsing and the
//! tree builder. Containers enter and exit; leaves arrive whole when they
//! close, with the facts set at close (C2-B-191).

use super::shapes::{DivInfo, FenceInfo, MarkerKind};
use super::table::TableData;
use crate::attr::AttrBlock;
use crate::inline::Piece;

/// What a container is.
#[derive(Debug, Clone)]
pub enum ContainerKind {
    Quote,
    List {
        kind: MarkerKind,
        start: Option<u64>,
    },
    Item {
        task: Option<char>,
    },
    DescList,
    Desc,
    Footnote {
        label: String,
    },
    Div {
        info: Box<DivInfo>,
        sigil: Option<char>,
    },
}

/// A leaf block, complete.
#[derive(Debug, Clone)]
pub enum LeafKind {
    Paragraph {
        pieces: Vec<Piece>,
        role: Option<Role>,
    },
    Heading {
        level: u8,
        pieces: Vec<Piece>,
    },
    Term {
        pieces: Vec<Piece>,
    },
    /// A caption attaches to the block that closed directly above (C2-B-146).
    Caption {
        pieces: Vec<Piece>,
    },
    Code {
        value: String,
        info: Box<FenceInfo>,
    },
    /// `%%%` block or a comment-only line outside a leaf (C2-A-074).
    Comment {
        block: bool,
        value: String,
    },
    Thematic,
    Table(Box<TableData>),
    LinkDef {
        label: String,
        href: String,
        title: Option<String>,
    },
    AbbrDef {
        key: String,
        expansion: String,
    },
    Frontmatter {
        format: String,
        value: String,
    },
    Include {
        path: String,
        section: Option<String>,
        options: serde_json::Map<String, serde_json::Value>,
        invalid: Vec<String>,
    },
    /// One entry per body line; an empty piece for an empty line (C2-A-069).
    LineBlock {
        entries: Vec<LineBlockEntry>,
    },
}

#[derive(Debug, Clone)]
pub enum LineBlockEntry {
    Line {
        piece: Piece,
        comments: Vec<(usize, usize, String)>,
    },
    BlockComment {
        start: usize,
        end: usize,
        value: String,
    },
}

/// A fact set when a paragraph closes (C2-B-157, C2-B-158).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Image,
    Math,
}

/// One event of the block pass.
#[derive(Debug, Clone)]
pub enum Event {
    Enter {
        kind: ContainerKind,
        start: usize,
        attrs: Option<Box<AttrBlock>>,
    },
    /// `loose` is the tightness of a list (C2-B-062); `None` for other containers.
    Exit { end: usize, loose: Option<bool> },
    Leaf {
        kind: LeafKind,
        start: usize,
        end: usize,
        attrs: Option<Box<AttrBlock>>,
    },
}
