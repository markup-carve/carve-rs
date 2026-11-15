//! The block pass (C2-B): reads the source line by line, keeps the stack of
//! open containers and the open leaf, and emits events.

pub mod events;
pub mod include;
pub mod line;
mod open;
pub mod shapes;
pub mod table;

use crate::attr::AttrBlock;
use crate::inline::{Join, Piece};
use crate::source::Source;
use events::{ContainerKind, Event, LeafKind, Role};
use line::Cursor;
use shapes::{FenceInfo, MarkerKind};
use table::TableData;

/// The result of the block pass.
#[derive(Debug, Default)]
pub struct BlockOutput {
    pub events: Vec<Event>,
}

/// An open container.
#[derive(Debug, Clone)]
struct Container {
    kind: Kind,
    /// Offset of the end of the last thing it holds, for its position.
    end: usize,
}

#[derive(Debug, Clone)]
enum Kind {
    Document,
    Quote,
    /// `col` is the marker column of the first item, relative to the quote base.
    List {
        kind: MarkerKind,
        col: usize,
        loose: bool,
        items: usize,
    },
    Item {
        marker_col: usize,
        content_col: usize,
    },
    DescList {
        col: usize,
        has_term: bool,
    },
    Desc {
        marker_col: usize,
        content_col: usize,
    },
    Footnote {
        content_col: usize,
    },
    Div {
        len: usize,
        sigil: Option<char>,
        figure: bool,
    },
}

impl Kind {
    /// Counts toward the depth cap (C2-B-190).
    const fn counts(&self) -> bool {
        !matches!(self, Self::Document)
    }

    /// Marker and content column of an indent-owned container.
    const fn indent_cols(&self) -> Option<(usize, usize)> {
        match self {
            Self::Item {
                marker_col,
                content_col,
            }
            | Self::Desc {
                marker_col,
                content_col,
            } => Some((*marker_col, *content_col)),
            Self::Footnote { content_col } => Some((0, *content_col)),
            _ => None,
        }
    }
}

/// The open leaf.
#[derive(Debug, Clone)]
enum Leaf {
    Para {
        pieces: Vec<Piece>,
    },
    Heading {
        level: u8,
        marker_col: usize,
        pieces: Vec<Piece>,
    },
    Term {
        pieces: Vec<Piece>,
    },
    Caption {
        pieces: Vec<Piece>,
    },
    Code {
        len: usize,
        indent: usize,
        info: FenceInfo,
        value: String,
        held: Vec<String>,
    },
    CommentBlock {
        len: usize,
        indent: usize,
        lines: Vec<String>,
        held: usize,
    },
    Frontmatter {
        format: String,
        value: String,
    },
    LineBlock {
        len: usize,
        lines: Vec<Piece>,
    },
    Table {
        data: TableData,
    },
}

#[derive(Debug, Clone)]
struct OpenLeaf {
    leaf: Leaf,
    start: usize,
    end: usize,
    attrs: Option<AttrBlock>,
    /// Stack depth of the container that holds the leaf.
    depth: usize,
}

/// Attribute lines waiting for their block (C2-B-092).
#[derive(Debug, Clone)]
struct Pending {
    attrs: AttrBlock,
    start: usize,
    end: usize,
    lines: Vec<(usize, usize)>,
    depth: usize,
    /// The event index at the first line: comment lines between the
    /// attribute lines and the block emit before the block does (C2-B-137).
    at: usize,
}

/// What closed directly above, for a caption (C2-B-146).
#[derive(Debug, Clone, Copy)]
struct Closed {
    depth: usize,
    target: bool,
}

/// The result of reading the prefixes of a line.
#[derive(Debug, Clone, Copy)]
struct Walk<'a> {
    /// Containers of the stack that own the line, from the outside.
    matched: usize,
    cur: Cursor<'a>,
    /// Column behind the innermost matched quote mark (C2-B-003).
    qbase: usize,
    /// The index of the first quote whose mark the line lacks.
    missing_quote: Option<usize>,
}

/// The block parser state.
pub struct Parser<'a> {
    src: &'a Source,
    events: Vec<Event>,
    stack: Vec<Container>,
    leaf: Option<OpenLeaf>,
    pending: Option<Pending>,
    blank_run: usize,
    closed: Option<Closed>,
    line_no: usize,
    /// The end of the line being read.
    line_end: usize,
}

/// Runs the block pass over a source.
#[must_use]
pub fn scan(src: &Source) -> BlockOutput {
    let mut p = Parser {
        src,
        events: Vec::new(),
        stack: vec![Container {
            kind: Kind::Document,
            end: 0,
        }],
        leaf: None,
        pending: None,
        line_end: 0,
        blank_run: 0,
        closed: None,
        line_no: 0,
    };
    let n = src.line_count();
    for i in 0..n {
        p.line_no = i;
        let (s, e) = src.line_range(i);
        if i + 1 == n && s == e && i > 0 {
            break;
        }
        p.line(s, e);
    }
    p.finish();
    BlockOutput { events: p.events }
}

impl<'a> Parser<'a> {
    fn depth_used(&self) -> usize {
        self.stack.iter().filter(|c| c.kind.counts()).count()
    }

    fn line(&mut self, s: usize, e: usize) {
        self.line_end = e;
        let text = self.src.slice(s, e);
        let cur = Cursor::new(text, s);
        if self.frontmatter(text, s, e) {
            return;
        }
        let walk = self.walk(cur, false);
        if walk.cur.is_blank() {
            let walk = self.walk(cur, true);
            self.blank_line(&walk, s, e);
            return;
        }
        // G-011: a quote owns each non-blank line its mark stands on, up to the line end.
        for c in self.stack.iter_mut().take(walk.matched) {
            if matches!(c.kind, Kind::Quote) {
                c.end = c.end.max(e);
            }
        }
        let had_opaque = matches!(
            self.leaf.as_ref().map(|l| &l.leaf),
            Some(Leaf::Code { .. } | Leaf::CommentBlock { .. } | Leaf::LineBlock { .. })
        );
        if self.opaque_line(&walk, e) {
            return;
        }
        if self.continue_leaf(&walk, e) {
            return;
        }
        // C2-B-135: a line that ended an opaque body is not a skipped comment line.
        if !had_opaque && self.comment_only(&walk, e) {
            return;
        }
        self.blank_run_reset_after(&walk, e);
    }

    /// The rest of a line that no open leaf took: close what does not own it,
    /// then open blocks (C2-B-025, C2-B-026).
    fn blank_run_reset_after(&mut self, walk: &Walk<'a>, e: usize) {
        self.settle_pending(walk);
        self.close_leaf();
        self.close_to(walk.matched);
        let blanks = self.blank_run;
        self.blank_run = 0;
        self.open_line(walk.cur, walk.qbase, e, blanks);
    }

    /// C2-B-161 to C2-B-165.
    fn frontmatter(&mut self, text: &str, s: usize, e: usize) -> bool {
        if self.line_no == 0 && self.leaf.is_none() {
            if let Some(format) = shapes::frontmatter_opener(text) {
                let start = s;
                self.leaf = Some(OpenLeaf {
                    leaf: Leaf::Frontmatter {
                        format,
                        value: String::new(),
                    },
                    start,
                    end: e,
                    attrs: None,
                    depth: 1,
                });
                return true;
            }
            return false;
        }
        let Some(OpenLeaf {
            leaf: Leaf::Frontmatter { value, .. },
            end,
            ..
        }) = &mut self.leaf
        else {
            return false;
        };
        *end = e;
        if shapes::frontmatter_closer(text) {
            self.close_leaf();
        } else {
            value.push_str(text);
            value.push('\n');
        }
        true
    }

    /// Reads the prefixes of the open containers, outside in (C2-B-008).
    fn walk(&self, cur: Cursor<'a>, blank: bool) -> Walk<'a> {
        let mut w = Walk {
            matched: 1,
            cur,
            qbase: 0,
            missing_quote: None,
        };
        for (i, c) in self.stack.iter().enumerate().skip(1) {
            match &c.kind {
                Kind::Quote => {
                    let k = w.cur.after_blanks();
                    if let Some(after) = shapes::quote_mark(&k) {
                        w.cur = after;
                        w.qbase = after.col;
                    } else {
                        w.missing_quote = Some(i);
                        break;
                    }
                }
                Kind::Item { .. } | Kind::Desc { .. } | Kind::Footnote { .. } => {
                    let Some((marker, content)) = c.kind.indent_cols() else {
                        break;
                    };
                    if !blank {
                        let k = w.cur.after_blanks();
                        if k.col - w.qbase <= marker || k.is_blank() {
                            break;
                        }
                        w.cur.strip_to(w.qbase + content);
                    }
                }
                Kind::List { .. } | Kind::DescList { .. } | Kind::Div { .. } | Kind::Document => {}
            }
            w.matched = i + 1;
        }
        w
    }

    /// A blank line (C2-B-009, C2-B-032).
    fn blank_line(&mut self, walk: &Walk<'a>, s: usize, e: usize) {
        if let Some(q) = walk.missing_quote {
            self.close_leaf();
            self.close_to(q);
        }
        if self.opaque_blank(walk, s, e) {
            self.blank_run += 1;
            return;
        }
        if let Some(p) = self.pending.take() {
            self.pending_to_paragraph(&p);
        }
        self.close_leaf();
        self.blank_run += 1;
        self.closed = None;
    }

    /// A blank line inside a fence body is held back (C2-B-113).
    fn opaque_blank(&mut self, walk: &Walk<'a>, s: usize, _e: usize) -> bool {
        let owned = self.leaf.as_ref().is_some_and(|l| l.depth <= walk.matched);
        let Some(open) = &mut self.leaf else {
            return false;
        };
        if !owned {
            return false;
        }
        match &mut open.leaf {
            Leaf::Code { indent, held, .. } => {
                let mut k = walk.cur;
                k.strip_to(walk.qbase + *indent);
                held.push(k.rest().to_string());
                true
            }
            Leaf::CommentBlock { held, .. } => {
                *held += 1;
                true
            }
            Leaf::LineBlock { lines, .. } => {
                lines.push(Piece {
                    start: s,
                    end: s,
                    join: Join::Line,
                });
                true
            }
            _ => false,
        }
    }

    /// A line in an opaque body or a line block (C2-B-112, C2-B-123).
    fn opaque_line(&mut self, walk: &Walk<'a>, e: usize) -> bool {
        let Some(open) = &self.leaf else { return false };
        if !matches!(
            open.leaf,
            Leaf::Code { .. } | Leaf::CommentBlock { .. } | Leaf::LineBlock { .. }
        ) {
            return false;
        }
        if open.depth > walk.matched || walk.matched < self.stack.len() {
            // C2-B-111: the container ends, and the fence with it.
            self.close_leaf();
            return false;
        }
        let k = walk.cur.after_blanks();
        let Some(OpenLeaf { leaf, end, .. }) = &mut self.leaf else {
            return false;
        };
        let closes = match leaf {
            Leaf::Code { len, .. } => shapes::bare_run(&k, b'`') >= *len,
            Leaf::CommentBlock { len, .. } => shapes::bare_run(&k, b'%') >= *len,
            Leaf::LineBlock { len, .. } => shapes::bare_run(&k, b':') >= *len,
            _ => false,
        };
        *end = e;
        if closes {
            // C2-B-113 drops trailing blank lines of an unclosed body only.
            flush_held(leaf);
            let target = matches!(leaf, Leaf::Code { .. });
            self.close_leaf();
            self.closed = Some(Closed {
                depth: self.stack.len(),
                target,
            });
            return true;
        }
        opaque_push(leaf, walk);
        true
    }

    /// Continues the open paragraph-like leaf with a lazy line, or a heading
    /// with an indented line, unless the line interrupts (C2-B-012, C2-B-154).
    fn continue_leaf(&mut self, walk: &Walk<'a>, e: usize) -> bool {
        let Some(open) = &self.leaf else { return false };
        let k = walk.cur.after_blanks();
        let lazy = match &open.leaf {
            Leaf::Para { .. } | Leaf::Term { .. } | Leaf::Caption { .. } => true,
            Leaf::Heading { marker_col, .. } => {
                walk.matched == self.stack.len()
                    && walk.missing_quote.is_none()
                    && k.col - walk.qbase > *marker_col
            }
            Leaf::Table { .. } => return self.table_line(walk, e),
            _ => false,
        };
        if !lazy || self.interrupts(walk) {
            return false;
        }
        let piece = Piece {
            start: k.offset(),
            end: e,
            join: Join::Line,
        };
        if let Some(OpenLeaf {
            leaf:
                Leaf::Para { pieces }
                | Leaf::Term { pieces }
                | Leaf::Caption { pieces }
                | Leaf::Heading { pieces, .. },
            end,
            ..
        }) = &mut self.leaf
        {
            pieces.push(piece);
            *end = e;
        }
        self.closed = None;
        true
    }

    /// A comment-only line outside a leaf is skipped by ownership and gives a
    /// comment node of the innermost open container (C2-B-137, C2-B-138).
    fn comment_only(&mut self, walk: &Walk<'a>, e: usize) -> bool {
        let k = walk.cur.after_blanks();
        if !shapes::comment_only(k.rest()) {
            return false;
        }
        if matches!(
            self.leaf.as_ref().map(|l| &l.leaf),
            Some(Leaf::Heading { .. })
        ) {
            self.close_leaf();
        }
        let value = shapes::line_comment_value(k.rest());
        let start = k.offset();
        let end = start + k.rest().len();
        if let Some(OpenLeaf {
            leaf: Leaf::Table { data },
            end: tend,
            ..
        }) = &mut self.leaf
        {
            if let Some(row) = data.rows.last_mut() {
                row.comments.push((start, end, value));
                *tend = end;
                return true;
            }
        }
        let _ = e;
        self.push_leaf_event(
            LeafKind::Comment {
                block: false,
                value,
            },
            start,
            end,
            None,
        );
        true
    }

    fn push_leaf_event(
        &mut self,
        kind: LeafKind,
        start: usize,
        end: usize,
        attrs: Option<AttrBlock>,
    ) {
        if let Some(top) = self.stack.last_mut() {
            top.end = top.end.max(end);
        }
        self.events.push(Event::Leaf {
            kind,
            start,
            end,
            attrs,
        });
    }

    /// Pending attribute lines stay pending only for a block of their own
    /// container (C2-B-091, C2-B-092).
    fn settle_pending(&mut self, walk: &Walk<'a>) {
        let Some(p) = &self.pending else { return };
        if walk.matched < p.depth {
            if let Some(p) = self.pending.take() {
                self.pending_to_paragraph(&p);
            }
        }
    }

    /// C2-B-192: attribute lines no block takes become one closed paragraph.
    fn pending_to_paragraph(&mut self, p: &Pending) {
        while self.stack.len() > p.depth {
            self.pop_container();
        }
        let pieces = p
            .lines
            .iter()
            .map(|&(s, e)| Piece {
                start: s,
                end: e,
                join: Join::Line,
            })
            .collect();
        self.push_leaf_event(
            LeafKind::Paragraph { pieces, role: None },
            p.start,
            p.end,
            None,
        );
        // The paragraph goes before the comment lines that followed it.
        if let Some(ev) = self.events.pop() {
            self.events.insert(p.at.min(self.events.len()), ev);
        }
        self.closed = None;
    }

    /// Closes the open leaf and emits it (facts set at close, C2-B-191).
    fn close_leaf(&mut self) {
        let Some(open) = self.leaf.take() else { return };
        let (kind, target) = self.finish_leaf(open.leaf, open.start);
        let end = if let LeafKind::Paragraph { pieces, .. }
        | LeafKind::Heading { pieces, .. }
        | LeafKind::Term { pieces }
        | LeafKind::Caption { pieces } = &kind
        {
            pieces.last().map_or(open.end, |p| p.end)
        } else {
            open.end
        };
        let attrs = open.attrs;
        while self.stack.len() > open.depth {
            self.pop_container();
        }
        self.push_leaf_event(kind, open.start, end, attrs);
        self.closed = Some(Closed {
            depth: self.stack.len(),
            target,
        });
    }

    fn finish_leaf(&self, leaf: Leaf, start: usize) -> (LeafKind, bool) {
        match leaf {
            Leaf::Para { pieces } => {
                let role = paragraph_role(self.src, &pieces);
                let target = role.is_some();
                (LeafKind::Paragraph { pieces, role }, target)
            }
            Leaf::Heading { level, pieces, .. } => (LeafKind::Heading { level, pieces }, false),
            Leaf::Term { pieces } => (LeafKind::Term { pieces }, false),
            Leaf::Caption { pieces } => (LeafKind::Caption { pieces }, false),
            Leaf::Code { info, value, .. } => (LeafKind::Code { value, info }, true),
            Leaf::CommentBlock { lines, .. } => (
                LeafKind::Comment {
                    block: true,
                    value: lines.join("\n"),
                },
                false,
            ),
            Leaf::Frontmatter { format, value } => (LeafKind::Frontmatter { format, value }, false),
            Leaf::LineBlock { lines, .. } => (LeafKind::LineBlock { lines }, false),
            Leaf::Table { data } => {
                let _ = start;
                (LeafKind::Table(Box::new(data)), true)
            }
        }
    }

    /// Closes containers until `keep` remain (C2-B-026).
    fn close_to(&mut self, keep: usize) {
        if self.stack.len() > keep {
            self.close_leaf();
        }
        while self.stack.len() > keep.max(1) {
            self.pop_container();
        }
    }

    fn pop_container(&mut self) {
        if self
            .leaf
            .as_ref()
            .is_some_and(|l| l.depth >= self.stack.len())
        {
            self.close_leaf();
        }
        let Some(c) = self.stack.pop() else { return };
        if matches!(c.kind, Kind::Document) {
            self.stack.push(c);
            return;
        }
        let loose = match c.kind {
            Kind::List { loose, .. } => Some(loose),
            _ => None,
        };
        let target = matches!(
            c.kind,
            Kind::Quote
                | Kind::Div {
                    sigil: Some('>'),
                    ..
                }
                | Kind::Div { figure: true, .. }
        );
        self.events.push(Event::Exit { end: c.end, loose });
        if let Some(top) = self.stack.last_mut() {
            top.end = top.end.max(c.end);
        }
        self.closed = Some(Closed {
            depth: self.stack.len(),
            target,
        });
    }

    fn push_container(
        &mut self,
        kind: Kind,
        ev: ContainerKind,
        start: usize,
        attrs: Option<AttrBlock>,
    ) {
        let end = start;
        self.events.push(Event::Enter {
            kind: ev,
            start,
            attrs,
        });
        self.stack.push(Container { kind, end });
        self.closed = None;
    }

    fn finish(&mut self) {
        if let Some(p) = self.pending.take() {
            self.pending_to_paragraph(&p);
        }
        self.close_leaf();
        while self.stack.len() > 1 {
            self.pop_container();
        }
    }
}

/// Moves the blank lines held back in a fence body into it.
fn flush_held(leaf: &mut Leaf) {
    match leaf {
        Leaf::Code { value, held, .. } => {
            for h in held.drain(..) {
                value.push_str(&h);
                value.push('\n');
            }
        }
        Leaf::CommentBlock { lines, held, .. } => {
            for _ in 0..*held {
                lines.push(String::new());
            }
            *held = 0;
        }
        _ => {}
    }
}

/// Appends a body line of an opaque leaf, stripped by the opener indent
/// (C2-B-029) or, in a line block, by the container only (C2-B-130).
fn opaque_push(leaf: &mut Leaf, walk: &Walk<'_>) {
    match leaf {
        Leaf::Code {
            indent,
            value,
            held,
            ..
        } => {
            for h in held.drain(..) {
                value.push_str(&h);
                value.push('\n');
            }
            let mut k = walk.cur;
            k.strip_to(walk.qbase + *indent);
            value.push_str(k.rest());
            value.push('\n');
        }
        Leaf::CommentBlock {
            indent,
            lines,
            held,
            ..
        } => {
            for _ in 0..*held {
                lines.push(String::new());
            }
            *held = 0;
            let mut k = walk.cur;
            k.strip_to(walk.qbase + *indent);
            lines.push(k.rest().to_string());
        }
        Leaf::LineBlock { lines, .. } => {
            let k = walk.cur;
            lines.push(Piece {
                start: k.offset(),
                end: k.base + k.text.len(),
                join: Join::Line,
            });
        }
        _ => {}
    }
}

/// End of a piece without its trailing spaces and tabs (C2-I-002), keeping
/// an escaped space.
pub(crate) fn trim_end(src: &Source, s: usize, e: usize) -> usize {
    let b = src.text().as_bytes();
    let mut z = e;
    while z > s && matches!(b.get(z - 1), Some(b' ' | b'\t')) {
        z -= 1;
    }
    if z < e && z > s && b.get(z - 1) == Some(&b'\\') {
        let run = b
            .get(s..z)
            .map_or(0, |r| r.iter().rev().take_while(|&&c| c == b'\\').count());
        if run % 2 == 1 {
            z += 1;
        }
    }
    z
}

/// C2-B-157, C2-B-158: a paragraph of exactly one block image line or one
/// display math line.
fn paragraph_role(src: &Source, pieces: &[Piece]) -> Option<Role> {
    let first = pieces.first()?;
    let rest_comments = pieces
        .iter()
        .skip(1)
        .all(|p| shapes::comment_only(src.slice(p.start, p.end).trim_start_matches([' ', '\t'])));
    if !rest_comments {
        return None;
    }
    let line = src.slice(first.start, first.end);
    if open::block_image_line(line) {
        return Some(Role::Image);
    }
    if open::display_math_line(line) {
        return Some(Role::Math);
    }
    None
}
