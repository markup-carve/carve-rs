//! The block pass (C2-B): reads the source line by line, keeps the stack of
//! open containers and the open leaf, and emits events.

pub mod events;
pub mod include;
pub mod line;
mod open;
pub mod shapes;
pub mod table;

use crate::attr::AttrBlock;
use crate::hash::AddrMap;
use crate::inline::{Join, Piece};
use crate::source::Source;
use events::{ContainerKind, Event, LeafKind, LineBlockEntry, Role};
use line::Cursor;
use shapes::{FenceInfo, MarkerKind};
use table::{Pieces, TableData};

/// The result of the block pass.
#[derive(Debug, Default)]
pub struct BlockOutput {
    pub events: Vec<Event>,
    /// Findings of the parse channel, in the order they were made.
    pub findings: Vec<Finding>,
    /// For a block that took attribute lines: its start, which is the start
    /// of the first attribute line, mapped to the start of its own markup.
    pub own_starts: Vec<(usize, usize)>,
}

/// A finding of the block pass: its code, which message of the code, its
/// source range and the values its message names.
#[derive(Debug, Clone)]
pub struct Finding {
    pub code: &'static str,
    pub message: usize,
    pub start: usize,
    pub end: usize,
    pub values: Vec<(&'static str, String)>,
}

/// An open container.
#[derive(Debug, Clone)]
struct Container {
    kind: Kind,
    start: usize,
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
        pieces: Pieces,
    },
    Heading {
        level: u8,
        marker_col: usize,
        pieces: Pieces,
    },
    Term {
        marker_col: usize,
        pieces: Pieces,
    },
    Caption {
        marker_col: usize,
        pieces: Pieces,
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
        held: Vec<String>,
    },
    Frontmatter {
        format: String,
        value: String,
        /// The opener line; attribute lines above it start the block.
        opener: usize,
    },
    LineBlock {
        len: usize,
        entries: Vec<LineBlockEntry>,
        comment: Option<LineCommentFence>,
        stanza_open: bool,
        /// Entries scanned for the last line, and its index, so lazy comments stay linear.
        last_line: (usize, Option<usize>),
        /// Blank lines held back (gap G-025).
        held: Vec<(usize, usize)>,
    },
    Table {
        data: TableData,
    },
}

#[derive(Debug, Clone)]
struct LineCommentFence {
    len: usize,
    start: usize,
    end: usize,
    lines: Vec<String>,
}

impl LineCommentFence {
    /// C2-B-108: text behind the run is the first line of the value.
    fn open(k: &Cursor<'_>, run: usize, end: usize) -> Self {
        let first = k
            .rest()
            .get(run..)
            .unwrap_or("")
            .trim_start_matches([' ', '\t']);
        Self {
            len: run,
            start: k.offset(),
            end,
            lines: (!first.is_empty())
                .then(|| first.to_string())
                .into_iter()
                .collect(),
        }
    }

    /// C2-B-113: an unclosed body drops its trailing blank lines.
    fn unclosed(mut self) -> LineBlockEntry {
        while self
            .lines
            .last()
            .is_some_and(|l| crate::chars::is_blank_line(l))
        {
            self.lines.pop();
        }
        LineBlockEntry::BlockComment {
            start: self.start,
            end: self.end,
            value: self.lines.join("\n"),
        }
    }
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
pub(super) struct Closed {
    pub(super) depth: usize,
    pub(super) target: bool,
}

/// The result of reading the prefixes of a line.
#[derive(Debug, Clone, Copy)]
struct Walk<'a> {
    /// Containers of the stack that own the line, from the outside.
    matched: usize,
    cur: Cursor<'a>,
    /// C2-B-029: source after quote marks, before an item strips to its content column.
    body: Cursor<'a>,
    /// Column behind the innermost matched quote mark (C2-B-003).
    qbase: usize,
    /// The index of the first quote whose mark the line lacks.
    missing_quote: Option<usize>,
}

/// The block parser state.
pub struct Parser<'a> {
    src: &'a Source<'a>,
    events: Vec<Event>,
    stack: Vec<Container>,
    leaf: Option<OpenLeaf>,
    pending: Option<Pending>,
    own_starts: AddrMap<usize, usize>,
    blank_run: usize,
    closed: Option<Closed>,
    line_no: usize,
    /// The end of the line being read.
    line_end: usize,
    findings: Vec<Finding>,
    /// True while a fence leaf closes at its closing fence.
    fence_closes: bool,
    /// Whether the line above was blank, for a typed metadata opener (C2-B-232).
    prev_blank: bool,
    /// The definition the next indented line may continue (C2-B-083, C2-B-084).
    def_tail: Option<DefTail>,
}

/// An open definition: its event and, for a link, the tail parts its first
/// line left out (C2-B-001).
#[derive(Debug, Clone, Copy)]
struct DefTail {
    event: usize,
    link: Option<TailParts>,
}

#[derive(Debug, Clone, Copy)]
struct TailParts {
    title: bool,
}

/// Merges a tail line into a link definition (C2-B-083): the title, then the
/// attribute block, at most one of each. Returns the part a title-only line
/// leaves to the next line, or `None` when the line is no tail.
fn take_link_tail(
    parts: TailParts,
    (more_title, more_attrs): (Option<String>, Option<AttrBlock>),
    title: &mut Option<String>,
    attrs: &mut Option<Box<AttrBlock>>,
) -> Option<Option<TailParts>> {
    if parts.title && more_title.is_some() {
        return None;
    }
    let rest = more_attrs.is_none().then_some(TailParts { title: true });
    if more_title.is_some() {
        *title = more_title;
    }
    if let Some(a) = more_attrs {
        match attrs {
            Some(own) => own.merge(a),
            None => *attrs = Some(Box::new(a)),
        }
    }
    Some(rest)
}

/// Runs the block pass over a source.
#[must_use]
pub fn scan(src: &Source) -> BlockOutput {
    scan_with_frontmatter(src, true)
}

pub(crate) fn scan_with_frontmatter(src: &Source, allow_frontmatter: bool) -> BlockOutput {
    let mut p = Parser {
        src,
        events: Vec::new(),
        stack: vec![Container {
            kind: Kind::Document,
            start: 0,
            end: 0,
        }],
        leaf: None,
        pending: None,
        own_starts: AddrMap::default(),
        line_end: 0,
        blank_run: 0,
        closed: None,
        line_no: 0,
        findings: Vec::new(),
        fence_closes: false,
        prev_blank: false,
        def_tail: None,
    };
    let n = src.line_count();
    for i in 0..n {
        p.line_no = i;
        let (s, e) = src.line_range(i);
        if i + 1 == n && s == e && i > 0 {
            break;
        }
        p.line_with_frontmatter(s, e, allow_frontmatter);
    }
    p.finish();
    BlockOutput {
        events: p.events,
        findings: p.findings,
        own_starts: p.own_starts.into_iter().collect(),
    }
}

impl<'a> Parser<'a> {
    fn depth_used(&self) -> usize {
        self.stack.iter().filter(|c| c.kind.counts()).count()
    }

    fn line_with_frontmatter(&mut self, s: usize, e: usize, allow_frontmatter: bool) {
        self.line_end = e;
        let text = self.src.slice(s, e);
        let cur = Cursor::new(text, s);
        let blank_above = std::mem::replace(&mut self.prev_blank, false);
        let def_tail = self.def_tail.take();
        if self.frontmatter(text, s, e, allow_frontmatter) {
            return;
        }
        let mut walk = self.walk(cur, false);
        if walk.cur.is_blank() {
            self.prev_blank = true;
            let walk = self.walk(cur, true);
            self.blank_line(&walk, s, e);
            return;
        }
        if self.metadata_block(&walk, text, s, e, blank_above) {
            return;
        }
        if walk.matched > self.stack.len() {
            walk = self.walk(cur, false);
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
            // C2-B-052, C2-B-062: blank lines in an opaque body are its content.
            self.blank_run = 0;
            return;
        }
        if self.region_marker(&walk) {
            return;
        }
        if let Some(t) = def_tail
            && self.definition_tail(t, &walk, e)
        {
            return;
        }
        if matches!(
            self.leaf.as_ref().map(|l| &l.leaf),
            Some(
                Leaf::Heading { .. }
                    | Leaf::Term { .. }
                    | Leaf::Caption { .. }
                    | Leaf::Table { .. }
            )
        ) && self.comment_only(&walk, e)
        {
            return;
        }
        if self.continue_leaf(&walk, e) {
            return;
        }
        if self.comment_line(&walk, e, had_opaque, blank_above) {
            return;
        }
        self.blank_run_reset_after(&walk, e);
    }

    /// A line indented past column 0 below a definition continues it
    /// (C2-B-083, C2-B-084). A comment-only line ends it (C2-B-137).
    fn definition_tail(&mut self, t: DefTail, walk: &Walk<'a>, e: usize) -> bool {
        let k = walk.cur.after_blanks();
        if !self.may_continue_definition(t, &k) {
            return false;
        }
        let Some(Event::Leaf {
            kind, end, attrs, ..
        }) = self.events.get_mut(t.event)
        else {
            return false;
        };
        let next = match (kind, t.link) {
            (LeafKind::LinkDef { title, .. }, Some(parts)) => {
                let Some(tail) = shapes::link_def_tail(k.rest()) else {
                    return false;
                };
                let Some(rest) = take_link_tail(parts, tail, title, attrs) else {
                    return false;
                };
                rest.map(|parts| DefTail {
                    event: t.event,
                    link: Some(parts),
                })
            }
            (LeafKind::AbbrDef { expansion, .. }, None) => {
                expansion.push(' ');
                expansion.push_str(k.rest().trim_end_matches([' ', '\t']));
                Some(t)
            }
            _ => return false,
        };
        *end = e;
        self.def_tail = next;
        if let Some(top) = self.stack.last_mut() {
            top.end = top.end.max(e);
        }
        true
    }

    /// Whether a line may continue the open definition: indented, in the
    /// document, no comment-only line, and for an abbreviation no line that
    /// ends a heading continuation (C2-B-084).
    fn may_continue_definition(&self, t: DefTail, k: &Cursor<'_>) -> bool {
        if self.stack.len() != 1
            || self.leaf.is_some()
            || k.col == 0
            || shapes::comment_only(k.rest())
        {
            return false;
        }
        t.link.is_some()
            || !(self.fence_interrupts(k)
                || shapes::table_row_shape(k).is_some()
                || shapes::thematic_break(k))
    }

    fn comment_line(
        &mut self,
        walk: &Walk<'a>,
        e: usize,
        had_opaque: bool,
        blank_above: bool,
    ) -> bool {
        // C2-B-135: a line that ended an opaque body is not a skipped comment line.
        if had_opaque {
            return self.leaf.is_none() && self.comment_ends_body(walk, e);
        }
        if !self.comment_only(walk, e) {
            return false;
        }
        // C2-B-137: a skipped comment line is not blank and not content, so
        // the blank line above it stays the one C2-B-232 reads.
        self.prev_blank = blank_above;
        true
    }

    /// C2-B-137: a comment-only line that ends an opaque body closes the body's
    /// container, with its list, and nothing more (carve#6439).
    fn comment_ends_body(&mut self, walk: &Walk<'a>, e: usize) -> bool {
        if !shapes::comment_only(walk.cur.after_blanks().rest()) {
            return false;
        }
        let is_body_container = |c: &Container| {
            matches!(
                c.kind,
                Kind::Item { .. } | Kind::Desc { .. } | Kind::Footnote { .. } | Kind::Quote
            )
        };
        let Some(index) = (walk.matched..self.stack.len())
            .rev()
            .find(|&i| i == walk.matched || self.stack.get(i).is_some_and(is_body_container))
        else {
            return false;
        };
        let keep = match self.stack.get(index.saturating_sub(1)).map(|c| &c.kind) {
            Some(Kind::List { .. } | Kind::DescList { .. }) => index.saturating_sub(1),
            _ => index,
        };
        self.close_to(keep);
        self.closed = None;
        self.blank_run = 0;
        self.comment_only(walk, e)
    }

    fn region_marker(&mut self, walk: &Walk<'a>) -> bool {
        let k = walk.cur.after_blanks();
        if !crate::include::is_carve_region_marker(k.rest()) {
            return false;
        }
        self.close_leaf();
        self.close_to(walk.matched);
        while self
            .stack
            .last()
            .is_some_and(|c| matches!(c.kind, Kind::List { .. } | Kind::DescList { .. }))
        {
            self.pop_container();
        }
        self.closed = None;
        self.blank_run = 0;
        self.push_leaf_event(
            LeafKind::Comment {
                block: false,
                value: shapes::line_comment_value(k.rest()),
            },
            k.offset(),
            k.offset() + k.rest().len(),
            None,
        );
        true
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

    /// C2-B-232, C2-B-233: after line 1 a typed opener at column 0 opens a
    /// top-level metadata block below a blank line or top-level attribute
    /// lines. A column-0 opener first closes the lists and description lists it
    /// stands after.
    fn metadata_block(
        &mut self,
        walk: &Walk<'a>,
        text: &str,
        s: usize,
        e: usize,
        blank_above: bool,
    ) -> bool {
        if !text.starts_with("---") || !shapes::typed_frontmatter_opener(text) {
            return false;
        }
        let lists = |c: &[Container]| {
            c.iter().all(|c| {
                matches!(
                    c.kind,
                    Kind::List { .. }
                        | Kind::Item { .. }
                        | Kind::DescList { .. }
                        | Kind::Desc { .. }
                )
            })
        };
        // An open leaf above means no separation: the line continues it or
        // closes the lists as any column-0 line does (C2-B-232).
        if self.leaf.is_some() {
            return false;
        }
        if self.stack.len() > 1 && lists(self.stack.get(1..).unwrap_or(&[])) {
            self.close_to(1);
        }
        let matched = self
            .stack
            .get(1..walk.matched.min(self.stack.len()))
            .unwrap_or(&[]);
        if !lists(matched) {
            return false;
        }
        let attrs = self.pending.as_ref().map(|p| p.depth);
        if attrs.map_or(!blank_above, |depth| depth != 1) {
            return false;
        }
        let Some(format) = shapes::frontmatter_opener(text) else {
            return false;
        };
        self.close_to(1);
        let (start, attrs) = self.take_pending(s, None);
        self.closed = None;
        self.blank_run = 0;
        self.leaf = Some(OpenLeaf {
            leaf: Leaf::Frontmatter {
                format,
                value: String::new(),
                opener: s,
            },
            start,
            end: e,
            attrs,
            depth: 1,
        });
        true
    }

    /// C2-B-161 to C2-B-165.
    fn frontmatter(&mut self, text: &str, s: usize, e: usize, allow_opener: bool) -> bool {
        if self.line_no == 0 && self.leaf.is_none() {
            if !allow_opener {
                return false;
            }
            if let Some(format) = shapes::frontmatter_opener(text) {
                let start = s;
                self.leaf = Some(OpenLeaf {
                    leaf: Leaf::Frontmatter {
                        format,
                        value: String::new(),
                        opener: s,
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
            self.fence_closes = true;
            self.close_leaf();
            self.fence_closes = false;
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
            body: cur,
            qbase: 0,
            missing_quote: None,
        };
        for (i, c) in self.stack.iter().enumerate().skip(1) {
            match &c.kind {
                Kind::Quote => {
                    let k = w.cur.after_blanks();
                    if let Some(after) = shapes::quote_mark(&k) {
                        w.cur = after;
                        w.body = after;
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
    fn opaque_blank(&mut self, walk: &Walk<'a>, s: usize, e: usize) -> bool {
        let owned = self.leaf.as_ref().is_some_and(|l| l.depth <= walk.matched);
        let Some(open) = &mut self.leaf else {
            return false;
        };
        if !owned {
            return false;
        }
        match &mut open.leaf {
            Leaf::Code { indent, held, .. } | Leaf::CommentBlock { indent, held, .. } => {
                let mut k = walk.body;
                k.strip_to(walk.qbase + *indent);
                held.push(k.rest().to_string());
                true
            }
            Leaf::LineBlock {
                held,
                comment,
                stanza_open,
                ..
            } => {
                if let Some(fence) = comment {
                    fence.lines.push(String::new());
                    fence.end = e;
                    return true;
                }
                // C2-B-024: a blank line ends the stanza, so no lazy line follows it.
                *stanza_open = false;
                // Gap G-025: as in the oracle, trailing blank lines of an
                // unclosed line block are dropped, as C2-B-113 does for a fence.
                let _ = s;
                let k = walk.cur;
                held.push((k.offset(), e.max(k.offset())));
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
        if matches!(open.leaf, Leaf::LineBlock { .. }) {
            return self.line_block_line(walk, e);
        }
        let k = walk.cur.after_blanks();
        if let Leaf::CommentBlock { len, .. } = open.leaf {
            if shapes::bare_run(&k, b'%') >= len {
                if let Some(leaf) = &mut self.leaf {
                    leaf.end = e;
                    flush_held(&mut leaf.leaf);
                }
                self.fence_closes = true;
                self.close_leaf();
                self.fence_closes = false;
                return true;
            }
        }
        if open.depth > walk.matched || walk.matched < self.stack.len() {
            // C2-B-111: the container ends, and the fence with it.
            self.close_leaf();
            return false;
        }
        let Some(OpenLeaf { leaf, end, .. }) = &mut self.leaf else {
            return false;
        };
        let closes = match leaf {
            Leaf::Code { len, .. } => shapes::bare_run(&k, b'`') >= *len,
            Leaf::CommentBlock { len, .. } => shapes::bare_run(&k, b'%') >= *len,
            _ => false,
        };
        *end = e;
        if closes {
            // C2-B-113 drops trailing blank lines of an unclosed body only.
            flush_held(leaf);
            let target = matches!(leaf, Leaf::Code { info, .. } if info.raw.is_none());
            self.fence_closes = true;
            self.close_leaf();
            self.fence_closes = false;
            self.closed = Some(Closed {
                depth: self.stack.len(),
                target,
            });
            return true;
        }
        opaque_push(leaf, walk);
        true
    }

    /// A stanza can keep lazy text and a comment fence (C2-B-024, C2-B-123).
    fn line_block_line(&mut self, walk: &Walk<'a>, e: usize) -> bool {
        let k = walk.cur.after_blanks();
        let owned = self.owns_line(walk);
        let lazy_end = self.lazy_stanza_ends(walk, &k);
        let Some(OpenLeaf {
            leaf:
                Leaf::LineBlock {
                    len,
                    comment,
                    entries,
                    held,
                    stanza_open,
                    last_line,
                },
            end,
            ..
        }) = &mut self.leaf
        else {
            return false;
        };
        let found = (self.src, &mut self.findings);
        let blank_held = end_unowned_comment(
            comment,
            entries,
            &k,
            owned,
            !*stanza_open || lazy_end,
            found,
        );
        let no_stanza = !*stanza_open || blank_held;
        if comment.is_some() {
            let found = (self.src, &mut self.findings);
            let parent_closes =
                line_block_comment_line(comment, entries, (*len, walk, &k, e, owned), found);
            *end = e;
            if parent_closes {
                self.fence_closes = true;
                self.close_leaf();
                self.fence_closes = false;
                self.closed = Some(Closed {
                    depth: self.stack.len(),
                    target: false,
                });
            }
            return true;
        }
        if owned && shapes::bare_run(&k, b':') >= *len {
            *end = e;
            entries.extend(held.drain(..).map(|(start, end)| LineBlockEntry::Line {
                piece: Piece {
                    start,
                    end,
                    join: Join::Line,
                },
                comments: Vec::new(),
            }));
            self.fence_closes = true;
            self.close_leaf();
            self.fence_closes = false;
            self.closed = Some(Closed {
                depth: self.stack.len(),
                target: false,
            });
            return true;
        }
        let run = shapes::fence_run(&k, b'%');
        if owned && run >= 3 {
            entries.extend(held.drain(..).map(|(start, end)| LineBlockEntry::Line {
                piece: Piece {
                    start,
                    end,
                    join: Join::Line,
                },
                comments: Vec::new(),
            }));
            *comment = Some(LineCommentFence::open(&k, run, e));
            *end = e;
            return true;
        }
        if !owned {
            // C2-B-024: a line that ends the stanza, as a region marker does, is no lazy comment.
            let ends = no_stanza || lazy_end;
            if shapes::comment_only(k.rest()) && !ends {
                lazy_comment(entries, last_line, &k, e);
                *end = e;
                return true;
            }
            if ends {
                self.close_leaf();
                return false;
            }
        }
        entries.extend(held.drain(..).map(|(start, end)| LineBlockEntry::Line {
            piece: Piece {
                start,
                end,
                join: Join::Line,
            },
            comments: Vec::new(),
        }));
        let body = walk.cur;
        entries.push(LineBlockEntry::Line {
            piece: Piece {
                start: body.offset(),
                end: body.base + body.text.len(),
                join: Join::Line,
            },
            comments: Vec::new(),
        });
        *stanza_open = true;
        *end = e;
        true
    }

    fn lazy_stanza_ends(&self, walk: &Walk<'_>, k: &Cursor<'_>) -> bool {
        shapes::heading(k).is_some()
            || shapes::quote_mark(k).is_some()
            || shapes::attribute_line(k).is_some()
            || shapes::footnote_def(k.rest()).is_some()
            || shapes::link_def(k.rest()).is_some()
            || shapes::abbr_def(k.rest()).is_some()
            || crate::include::is_carve_region_marker(k.rest())
            || self.interrupts(walk)
    }

    fn owns_line(&self, walk: &Walk<'_>) -> bool {
        self.leaf
            .as_ref()
            .is_some_and(|open| open.depth <= walk.matched)
    }

    /// Continues the open paragraph with a lazy line, or a label (heading,
    /// term, caption) with an indented line, unless the line interrupts
    /// (C2-B-012, C2-B-234).
    fn continue_leaf(&mut self, walk: &Walk<'a>, e: usize) -> bool {
        let Some(open) = &self.leaf else { return false };
        let k = walk.cur.after_blanks();
        let lazy = match &open.leaf {
            Leaf::Para { .. } => true,
            Leaf::Heading { marker_col, .. }
            | Leaf::Term { marker_col, .. }
            | Leaf::Caption { marker_col, .. } => {
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
                | Leaf::Term { pieces, .. }
                | Leaf::Caption { pieces, .. }
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
        let value = shapes::line_comment_value(k.rest());
        let start = k.offset();
        let end = start + k.rest().len();
        if let Some(OpenLeaf {
            leaf:
                Leaf::Heading { pieces, .. } | Leaf::Term { pieces, .. } | Leaf::Caption { pieces, .. },
            end: hend,
            ..
        }) = &mut self.leaf
        {
            pieces.push(Piece {
                start,
                end,
                join: Join::Line,
            });
            *hend = end;
            return true;
        }
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
            attrs: attrs.map(Box::new),
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
        let mut pieces = Pieces::default();
        for &(s, e) in &p.lines {
            pieces.push(Piece {
                start: s,
                end: e,
                join: Join::Line,
            });
        }
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
        if !self.fence_closes {
            self.unclosed(&open);
        }
        // C2-B-111: a comment block still open in a line block ends with it.
        if let Leaf::LineBlock {
            comment: Some(f), ..
        } = &open.leaf
        {
            self.findings.push(unclosed_comment(self.src, f));
        }
        let (kind, target) = self.finish_leaf(open.leaf, open.start);
        let end = if let LeafKind::Paragraph { pieces, .. }
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

    /// C2-B-113: a fence body that ends without its closing fence.
    fn unclosed(&mut self, open: &OpenLeaf) {
        let (code, ch) = match open.leaf {
            Leaf::Code { .. } => ("unclosed-code-fence", b'`'),
            Leaf::CommentBlock { .. } => ("unclosed-comment-fence", b'%'),
            Leaf::Frontmatter { .. } => ("unclosed-frontmatter", b'-'),
            Leaf::LineBlock { .. } => ("unclosed-container-fence", b':'),
            _ => return,
        };
        let start = match open.leaf {
            Leaf::Frontmatter { opener, .. } => opener,
            _ => open.start,
        };
        let line_block = matches!(open.leaf, Leaf::LineBlock { .. });
        let f = fence_finding(self.src, (code, ch), start, open.end, line_block);
        self.findings.push(f);
    }

    /// C2-B-122: further words after an opener's parts are not read.
    /// `fence` is where the fence run starts, after any container prefix and list marker;
    /// `anonymous` marks a div opener with no name (C2-B-119).
    pub(super) fn further_words(
        &mut self,
        fence: usize,
        start: usize,
        end: usize,
        anonymous: bool,
    ) {
        let text = self.src.text();
        let line = text.get(fence..end).unwrap_or("").trim_start();
        let opener = text.get(fence..start).unwrap_or("").trim();
        let respaced = line.split_whitespace().collect::<Vec<_>>().join(" ");
        let (message, values) = if line.starts_with("```") {
            let run = line.bytes().take_while(|&b| b == b'`').count();
            let rest = line.get(run..).unwrap_or("");
            if rest.trim_start().starts_with('{') && rest.starts_with("  ") {
                (4, vec![("correctedOpener", opener.to_string())])
            } else if rest.starts_with("  ") || rest.starts_with('\t') && rest.trim().is_empty() {
                (5, vec![("respaced", respaced)])
            } else if rest.contains('\t') {
                (6, vec![("respaced", respaced)])
            } else if rest.contains('"') {
                (1, Vec::new())
            } else {
                (4, vec![("correctedOpener", opener.to_string())])
            }
        } else if anonymous {
            (0, Vec::new())
        } else {
            (4, vec![("correctedOpener", opener.to_string())])
        };
        self.findings.push(Finding {
            code: "fence-title-syntax",
            message,
            start,
            end,
            values,
        });
    }

    pub(super) fn fence_opener_fallback(&mut self, start: usize, end: usize) {
        self.findings.push(Finding {
            code: "fence-opener-fallback",
            message: 0,
            start,
            end,
            values: Vec::new(),
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
            Leaf::Term { pieces, .. } => (LeafKind::Term { pieces }, false),
            Leaf::Caption { pieces, .. } => (LeafKind::Caption { pieces }, false),
            Leaf::Code { info, value, .. } => {
                let target = info.raw.is_none();
                (
                    LeafKind::Code {
                        value,
                        info: Box::new(info),
                    },
                    target,
                )
            }
            Leaf::CommentBlock { lines, .. } => (
                LeafKind::Comment {
                    block: true,
                    value: lines.join("\n"),
                },
                false,
            ),
            Leaf::Frontmatter { format, value, .. } => {
                (LeafKind::Frontmatter { format, value }, false)
            }
            Leaf::LineBlock {
                mut entries,
                comment,
                ..
            } => {
                // C2-B-111: an unclosed comment block ends with the line block.
                if let Some(fence) = comment {
                    entries.push(fence.unclosed());
                }
                (LeafKind::LineBlock { entries }, false)
            }
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
        if !self.fence_closes
            && let Kind::Div { len, .. } = c.kind
        {
            self.findings.push(Finding {
                code: "unclosed-container-fence",
                message: 0,
                start: c.start,
                end: c.end,
                values: vec![("width", len.to_string()), ("kind", "container".into())],
            });
        }
        let loose = match c.kind {
            Kind::List { loose, .. } => Some(loose),
            _ => None,
        };
        let nested_figure = self
            .stack
            .iter()
            .any(|frame| matches!(frame.kind, Kind::Div { figure: true, .. }));
        let target = matches!(
            c.kind,
            Kind::Quote
                | Kind::Div {
                    sigil: Some('>'),
                    ..
                }
        ) || matches!(c.kind, Kind::Div { figure: true, .. }) && !nested_figure;
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
            attrs: attrs.map(Box::new),
        });
        self.stack.push(Container { kind, start, end });
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

/// The finding of a fence that ends without its closing fence (C2-B-113).
fn fence_finding(
    src: &Source,
    (code, ch): (&'static str, u8),
    start: usize,
    end: usize,
    line_block: bool,
) -> Finding {
    let text = src.text();
    let run: String = text
        .get(start..)
        .unwrap_or("")
        .bytes()
        .take_while(|&b| b == ch)
        .map(char::from)
        .collect();
    let to_end = text.get(end..).is_none_or(|r| r.trim().is_empty());
    let ending = if to_end {
        "it runs to the end of the document".to_string()
    } else {
        format!(
            "it ends at the end of its container on line {}",
            src.pos(end, end).end_line
        )
    };
    Finding {
        code,
        message: 0,
        start,
        end,
        values: if line_block {
            vec![
                ("width", run.len().to_string()),
                ("kind", "line block".into()),
            ]
        } else {
            vec![("marker", run.clone()), ("opener", run), ("ending", ending)]
        },
    }
}

/// C2-B-111: a comment block in a line block that its container ends.
fn unclosed_comment(src: &Source, f: &LineCommentFence) -> Finding {
    fence_finding(src, ("unclosed-comment-fence", b'%'), f.start, f.end, false)
}

/// C2-B-123: a comment block hides its body but does not end the stanza.
fn line_block_comment_line(
    comment: &mut Option<LineCommentFence>,
    entries: &mut Vec<LineBlockEntry>,
    (opener_len, walk, k, end, owned): (usize, &Walk<'_>, &Cursor<'_>, usize, bool),
    (src, findings): (&Source, &mut Vec<Finding>),
) -> bool {
    let Some(fence) = comment.as_mut() else {
        return false;
    };
    let inner_closes = shapes::bare_run(k, b'%') >= fence.len;
    let parent_closes = owned && shapes::bare_run(k, b':') >= opener_len;
    if inner_closes || parent_closes {
        entries.push(LineBlockEntry::BlockComment {
            start: fence.start,
            end: if inner_closes { end } else { fence.end },
            value: fence.lines.join("\n"),
        });
        if let Some(fence) = comment.take().filter(|_| !inner_closes) {
            findings.push(unclosed_comment(src, &fence));
        }
        return parent_closes;
    }
    // C2-B-123, C2-B-130: the body is stripped by the container only, and a straddling tab is kept (C2-B-007).
    fence.lines.push(walk.cur.rest().to_string());
    fence.end = end;
    false
}

/// C2-B-024: a lazy comment-only line ends the stanza line above it; a comment
/// block between keeps that line (C2-B-123).
fn lazy_comment(
    entries: &mut [LineBlockEntry],
    last_line: &mut (usize, Option<usize>),
    k: &Cursor<'_>,
    e: usize,
) {
    let (scanned, at) = *last_line;
    let at = entries
        .get(scanned..)
        .unwrap_or_default()
        .iter()
        .rposition(|entry| matches!(entry, LineBlockEntry::Line { .. }))
        .map(|i| i + scanned)
        .or(at);
    *last_line = (entries.len(), at);
    if let Some(LineBlockEntry::Line { comments, .. }) = at.and_then(|i| entries.get_mut(i)) {
        comments.push((k.offset(), e, shapes::line_comment_value(k.rest())));
    }
}

/// An unowned line while a line block's comment block is open. C2-B-137: the closer
/// is tested before ownership. C2-B-024: only an open stanza takes a lazy line, and
/// not after a blank line; otherwise the comment block ends with its container
/// (C2-B-111). Returns whether a blank line was held.
fn end_unowned_comment(
    comment: &mut Option<LineCommentFence>,
    entries: &mut Vec<LineBlockEntry>,
    k: &Cursor<'_>,
    owned: bool,
    no_stanza: bool,
    (src, findings): (&Source, &mut Vec<Finding>),
) -> bool {
    let Some(fence) = comment.as_ref().filter(|_| !owned) else {
        return false;
    };
    let blank_held = fence
        .lines
        .last()
        .is_some_and(|l| crate::chars::is_blank_line(l));
    if shapes::bare_run(k, b'%') < fence.len
        && (no_stanza || blank_held)
        && let Some(fence) = comment.take()
    {
        findings.push(unclosed_comment(src, &fence));
        entries.push(fence.unclosed());
    }
    blank_held
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
        Leaf::LineBlock { entries, held, .. } => {
            entries.extend(held.drain(..).map(|(start, end)| LineBlockEntry::Line {
                piece: Piece {
                    start,
                    end,
                    join: Join::Line,
                },
                comments: Vec::new(),
            }));
        }
        Leaf::CommentBlock { lines, held, .. } => {
            lines.append(held);
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
            let mut k = walk.body;
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
            lines.append(held);
            let mut k = walk.body;
            k.strip_to(walk.qbase + *indent);
            lines.push(k.rest().to_string());
        }
        Leaf::LineBlock { entries, held, .. } => {
            entries.extend(held.drain(..).map(|(start, end)| LineBlockEntry::Line {
                piece: Piece {
                    start,
                    end,
                    join: Join::Line,
                },
                comments: Vec::new(),
            }));
            let k = walk.cur;
            entries.push(LineBlockEntry::Line {
                piece: Piece {
                    start: k.offset(),
                    end: k.base + k.text.len(),
                    join: Join::Line,
                },
                comments: Vec::new(),
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
