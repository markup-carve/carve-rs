//! Interrupters (C2-B-013 to C2-B-024) and the openers of a line.

use super::events::{ContainerKind, LeafKind};
use super::line::Cursor;
use super::shapes::{self, ListMarker, MarkerKind};
use super::table::{self, Pieces, Row};
use super::{Closed, Container, Kind, Leaf, OpenLeaf, Parser, Pending, Walk};
use crate::attr::AttrBlock;
use crate::inline::{Join, Piece};
use crate::tables::{BLOCK_DEPTH, LIST_END_BLANK_LINES};

/// The lists, description lists and marker owners a line can reach:
/// those inside the innermost quote whose mark it carries and outside every
/// quote whose mark it lacks (C2-B-020).
struct Frame<'s> {
    reach: &'s [Container],
    owner: bool,
    footnote: bool,
    /// The line lacks the mark of a quote that holds the open leaf.
    lazy: bool,
}

impl Frame<'_> {
    fn lists(&self) -> impl DoubleEndedIterator<Item = usize> + '_ {
        self.reach.iter().filter_map(|c| match c.kind {
            Kind::List { col, .. } => Some(col),
            _ => None,
        })
    }

    fn desc_lists(&self) -> impl Iterator<Item = usize> + '_ {
        self.reach.iter().filter_map(|c| match c.kind {
            Kind::DescList { col, .. } => Some(col),
            _ => None,
        })
    }
}

impl Parser<'_> {
    /// A refused div opener is text at the depth cap (C2-B-190), so it cannot
    /// end the paragraph above it. A bare run that closes an open div can.
    fn fence_interrupts(&self, k: &Cursor<'_>) -> bool {
        if shapes::code_fence_opener(k).is_some() || shapes::fence_run(k, b'%') >= 3 {
            return true;
        }
        let Some((len, info)) = shapes::div_opener(k) else {
            return false;
        };
        self.room(1)
            || (k.rest().get(len..).is_some_and(crate::chars::is_blank_line)
                && info.name.is_none()
                && info.sigil.is_none()
                && info.title.is_none()
                && info.label.is_none()
                && self
                    .stack
                    .iter()
                    .rev()
                    .take_while(|c| matches!(c.kind, Kind::Div { .. }))
                    .any(|c| matches!(c.kind, Kind::Div { len: open_len, .. } if open_len <= len)))
    }

    fn frame(&self, walk: &Walk<'_>, m: usize) -> Frame<'_> {
        let lo = self
            .stack
            .iter()
            .take(walk.matched)
            .rposition(|c| matches!(c.kind, Kind::Quote))
            .map_or(1, |q| q + 1);
        let hi = self
            .stack
            .iter()
            .enumerate()
            .skip(walk.matched)
            .find(|(_, c)| matches!(c.kind, Kind::Quote))
            .map_or(self.stack.len(), |(i, _)| i);
        let reach = self.stack.get(lo..hi).unwrap_or(&[]);
        // C2-B-015: a nested list opens below a paragraph only past the marker of
        // the item, description or footnote body that holds the paragraph; inside
        // a div the marker is a lazy line (C2-B-024).
        let lazy = walk.missing_quote.is_some();
        let in_div = !lazy && matches!(self.stack.last().map(|c| &c.kind), Some(Kind::Div { .. }));
        let owner = !in_div
            && reach.iter().any(|c| {
                !matches!(c.kind, Kind::List { .. } | Kind::DescList { .. })
                    && c.kind.indent_cols().is_some_and(|(marker, _)| m > marker)
            });
        Frame {
            reach,
            owner,
            footnote: self
                .stack
                .iter()
                .any(|c| matches!(c.kind, Kind::Footnote { .. })),
            lazy,
        }
    }

    /// Whether the line ends the open paragraph-like leaf (the six interrupters).
    pub(super) fn interrupts(&self, walk: &Walk<'_>) -> bool {
        let k = walk.cur.after_blanks();
        if !may_open(&k) {
            let m = k.col - walk.qbase;
            return shapes::list_marker(&k)
                .is_some_and(|marker| self.marker_interrupts(&self.frame(walk, m), m, &marker));
        }
        if self.fence_interrupts(&k)
            || shapes::table_row_shape(&k).is_some()
            || shapes::thematic_break(&k)
        {
            return true;
        }
        let m = k.col - walk.qbase;
        if shapes::caption(&k).is_some() {
            return self.caption_interrupts(walk);
        }
        if let Some(marker) = shapes::list_marker(&k) {
            return self.marker_interrupts(&self.frame(walk, m), m, &marker);
        }
        if let Some((term, _, _)) = shapes::desc_mark(&k) {
            let f = self.frame(walk, m);
            // C2-B-190: a description that cannot open is text of the term.
            let replaces = matches!(
                self.stack.last(),
                Some(Container {
                    kind: Kind::Desc { .. },
                    ..
                })
            );
            return f.desc_lists().any(|c| c == m) && (term || replaces || self.room(1));
        }
        walk.qbase == 0 && k.col == 0 && is_definition(k.rest()) && self.frame(walk, m).footnote
    }

    fn marker_interrupts(&self, f: &Frame, column: usize, marker: &ListMarker) -> bool {
        if matches!(
            self.leaf,
            Some(OpenLeaf {
                leaf: Leaf::Term { .. },
                ..
            })
        ) && if f.lazy {
            f.desc_lists().any(|owner| column >= owner)
        } else {
            self.term_keeps(column)
        } {
            return false;
        }
        // C2-B-190: a nested marker that cannot open is paragraph text.
        let can_replace = f.lists().any(|owner| column <= owner);
        (can_replace || self.room(2)) && list_interrupts(f, column, marker)
    }

    /// C2-B-069, C2-B-075: below an open term, a list marker that the container
    /// around the term's description list owns (C2-B-025) is term text; only a
    /// marker that closes that container can end the term.
    fn term_keeps(&self, column: usize) -> bool {
        let mut outside = self
            .stack
            .iter()
            .rev()
            .take_while(|c| !matches!(c.kind, Kind::Quote))
            .skip_while(|c| !matches!(c.kind, Kind::DescList { .. }));
        let desc_column = outside.next().and_then(|c| match c.kind {
            Kind::DescList { col, .. } => Some(col),
            _ => None,
        });
        let outer_marker = outside.find_map(|c| c.kind.indent_cols().map(|(marker, _)| marker));
        if let (Some(desc), Some(marker)) = (desc_column, outer_marker) {
            if marker < column && column < desc {
                return false;
            }
        }
        outer_marker.is_none_or(|marker| column > marker)
    }

    /// Interrupter (d) and Exception C2-B-018.
    fn caption_interrupts(&self, walk: &Walk<'_>) -> bool {
        if let Some(q) = walk.missing_quote {
            // Gap G-026: C2-B-018 interrupts an open paragraph; an open term
            // takes the caret line as a lazy line, as in the oracle.
            let lazy = matches!(
                self.leaf,
                Some(OpenLeaf {
                    leaf: Leaf::Term { .. } | Leaf::Caption { .. },
                    ..
                })
            );
            if walk.matched == q && !lazy {
                return true;
            }
        }
        // C2-B-146: a caret line the paragraph's container does not own is no
        // caption line, so it is a lazy line (as in C2-B-018).
        if walk.matched < self.stack.len() {
            return false;
        }
        let Some(OpenLeaf {
            leaf: Leaf::Para { pieces },
            ..
        }) = &self.leaf
        else {
            return false;
        };
        super::paragraph_role(self.src, pieces).is_some()
    }

    /// A line while a table is open: a row or a continuation row continues it
    /// (C2-B-170, C2-B-175); a comment-only line is left to the comment rule.
    pub(super) fn table_line(&mut self, walk: &Walk<'_>, _e: usize) -> bool {
        let k = walk.cur.after_blanks();
        if shapes::comment_only(k.rest()) && walk.matched == self.stack.len() {
            return false;
        }
        let owned = walk.matched == self.stack.len();
        let text = self.src.text();
        if owned {
            if let Some(parsed) = shapes::table_row_shape(&k) {
                let line_end = k.offset() + k.rest().len();
                let row = table_row_data(text, &k, parsed);
                if let Some(OpenLeaf {
                    leaf: Leaf::Table { data },
                    end,
                    ..
                }) = &mut self.leaf
                {
                    data.rows.push(row);
                    *end = line_end;
                }
                self.closed = None;
                return true;
            }
            if let Some(frags) = continuation(&k) {
                if let Some(OpenLeaf {
                    leaf: Leaf::Table { data },
                    end,
                    ..
                }) = &mut self.leaf
                {
                    if let Some(row) = data.rows.last_mut() {
                        table::join_continuation(row, text, &frags);
                        row.end = (k.offset() + k.rest().len()).max(row.end);
                        *end = row.end.max(*end);
                    }
                }
                return true;
            }
        }
        self.close_leaf();
        false
    }

    /// Opens the blocks of a line in the innermost open container.
    pub(super) fn open_line(&mut self, cur: Cursor<'_>, qbase: usize, e: usize, blanks: usize) {
        let mut cur = cur;
        let mut qbase = qbase;
        let mut first = true;
        loop {
            let k = cur.after_blanks();
            if k.is_blank() {
                break;
            }
            let m = k.col - qbase;
            if first {
                if let Some(next) = self.sibling(&k, m, blanks, e) {
                    cur = next;
                    first = false;
                    continue;
                }
            }
            let Some((next, base)) = self.open_container(&k, m, first) else {
                self.open_leaf(&k, m, e, first);
                break;
            };
            cur = next;
            if let Some(b) = base {
                qbase = b;
            }
            first = false;
        }
    }

    /// Settles a list or description list in the innermost context: a
    /// matching marker is a sibling, anything else closes the list.
    fn sibling<'c>(
        &mut self,
        k: &Cursor<'c>,
        m: usize,
        blanks: usize,
        e: usize,
    ) -> Option<Cursor<'c>> {
        loop {
            match self.stack.last().map(|c| c.kind.clone()) {
                Some(Kind::List { kind, .. }) => {
                    let marker =
                        shapes::list_marker(k).filter(|mk| shapes::marker_matches(kind, mk.kind));
                    match marker {
                        Some(mk) if blanks < LIST_END_BLANK_LINES => {
                            return Some(self.sibling_item(k, m, &mk, blanks));
                        }
                        _ => self.pop_container(),
                    }
                }
                Some(Kind::DescList { col, has_term }) => match shapes::desc_mark(k) {
                    // C2-B-052: three blank lines end a description list too.
                    Some((term, pos, ccol))
                        if col == m && (term || has_term) && blanks < LIST_END_BLANK_LINES =>
                    {
                        // C2-B-190: a description that cannot open is text.
                        if !term && !self.room(1) {
                            return None;
                        }
                        return Some(self.desc_entry(k, m, term, pos, ccol, e));
                    }
                    _ => self.pop_container(),
                },
                _ => return None,
            }
        }
    }

    fn sibling_item<'c>(
        &mut self,
        k: &Cursor<'c>,
        m: usize,
        mk: &ListMarker,
        blanks: usize,
    ) -> Cursor<'c> {
        if let Some(Kind::Item { .. }) = self.stack.last().map(|c| &c.kind) {
            self.pop_container();
        }
        if let Some(c) = self.stack.last_mut() {
            if let Kind::List {
                loose, items, col, ..
            } = &mut c.kind
            {
                // C2-B-062, C2-B-065: a blank line between two items.
                if blanks > 0 && *items > 0 {
                    *loose = true;
                }
                *items += 1;
                // Gap G-021: a sibling left of the list's column (C2-B-030)
                // moves the column to its marker.
                *col = (*col).min(m);
            }
        }
        self.push_item(k, m, mk)
    }

    fn push_item<'c>(&mut self, k: &Cursor<'c>, m: usize, mk: &ListMarker) -> Cursor<'c> {
        let start = k.offset();
        let ev = ContainerKind::Item { task: mk.task };
        let item = Kind::Item {
            marker_col: m,
            content_col: mk.content_col - (k.col - m),
        };
        self.push_container(item, ev, start, mk.attrs.clone());
        let mut c = *k;
        c.pos = mk.content;
        c.col = mk.content_col;
        c
    }

    fn desc_entry<'c>(
        &mut self,
        k: &Cursor<'c>,
        m: usize,
        term: bool,
        pos: usize,
        ccol: usize,
        e: usize,
    ) -> Cursor<'c> {
        if let Some(Kind::Desc { .. }) = self.stack.last().map(|c| &c.kind) {
            self.pop_container();
        }
        let mut c = *k;
        c.pos = pos;
        c.col = ccol;
        if term {
            if let Some(Container {
                kind: Kind::DescList { has_term, .. },
                ..
            }) = self.stack.last_mut()
            {
                *has_term = true;
            }
            let piece = Piece {
                start: c.offset(),
                end: e,
                join: Join::Line,
            };
            self.set_leaf(
                Leaf::Term {
                    pieces: Pieces::One(Some(piece)),
                },
                k.offset(),
                e,
                None,
            );
            c.pos = c.text.len();
            return c;
        }
        self.push_container(
            Kind::Desc {
                marker_col: m,
                content_col: ccol - (k.col - m),
            },
            ContainerKind::Desc,
            k.offset(),
            None,
        );
        c
    }

    fn set_leaf(&mut self, leaf: Leaf, start: usize, end: usize, attrs: Option<AttrBlock>) {
        let (start, attrs) = self.take_pending(start, attrs);
        self.leaf = Some(OpenLeaf {
            leaf,
            start,
            end,
            attrs,
            depth: self.stack.len(),
        });
        self.closed = None;
    }

    /// The pending attribute lines go to the first block of the line
    /// (C2-B-091); its own attributes merge after them.
    pub(super) fn take_pending(
        &mut self,
        start: usize,
        attrs: Option<AttrBlock>,
    ) -> (usize, Option<AttrBlock>) {
        match self.pending.take() {
            Some(p) => {
                let mut a = p.attrs;
                if let Some(own) = attrs {
                    a.merge(own);
                }
                self.own_starts.insert(p.start, start);
                (p.start, Some(a))
            }
            None => (start, attrs),
        }
    }

    fn room(&self, n: usize) -> bool {
        self.depth_used() + n <= BLOCK_DEPTH
    }

    /// A container opener: a quote mark, a list marker, a term or a
    /// description mark, a footnote definition, a div opener.
    fn open_container<'c>(
        &mut self,
        k: &Cursor<'c>,
        m: usize,
        first: bool,
    ) -> Option<(Cursor<'c>, Option<usize>)> {
        let _ = first;
        if !may_open(k) {
            let mk = shapes::list_marker(k)?;
            return self.room(2).then(|| (self.open_list(k, m, &mk), None));
        }
        if let Some(after) = shapes::quote_mark(k) {
            if !self.room(1) {
                return None;
            }
            let (start, attrs) = self.take_pending(k.offset(), None);
            self.push_container(Kind::Quote, ContainerKind::Quote, start, attrs);
            // G-011: an empty quote still spans the line of its first mark.
            if let Some(c) = self.stack.last_mut() {
                c.end = self.line_end;
            }
            return Some((after, Some(after.col)));
        }
        if shapes::thematic_break(k) {
            return None;
        }
        if let Some(mk) = shapes::list_marker(k) {
            if !self.room(2) {
                return None;
            }
            return Some((self.open_list(k, m, &mk), None));
        }
        if let Some((true, pos, ccol)) = shapes::desc_mark(k) {
            if !self.room(1) {
                return None;
            }
            let (start, attrs) = self.take_pending(k.offset(), None);
            self.push_container(
                Kind::DescList {
                    col: m,
                    has_term: true,
                },
                ContainerKind::DescList,
                start,
                attrs,
            );
            let e = k.base + k.text.len();
            return Some((self.desc_entry(k, m, true, pos, ccol, e), None));
        }
        if self.stack.len() == 1 && k.col == 0 {
            if let Some((label, content)) = shapes::footnote_def(k.rest()) {
                let (start, attrs) = self.take_pending(k.offset(), None);
                let mut c = *k;
                c.bump_n(content);
                self.push_container(
                    Kind::Footnote { content_col: c.col },
                    ContainerKind::Footnote { label },
                    start,
                    attrs,
                );
                return Some((c, None));
            }
        }
        None
    }

    fn open_list<'c>(&mut self, k: &Cursor<'c>, m: usize, mk: &ListMarker) -> Cursor<'c> {
        let (start, mut attrs) = self.take_pending(k.offset(), None);
        // C2-B-042: the first marker fixes the style without reading a later line.
        let kind = mk.kind;
        let mut value = match kind {
            MarkerKind::Ordered { value, .. } => Some(value.unwrap_or(1)),
            MarkerKind::Bullet(_) => None,
        };
        if let (Some(v), MarkerKind::Ordered { style, .. }) = (&mut value, kind) {
            start_attribute(attrs.as_mut(), style, v);
        }
        let list = Kind::List {
            kind,
            col: m,
            loose: false,
            items: 1,
        };
        self.push_container(
            list,
            ContainerKind::List { kind, start: value },
            start,
            attrs,
        );
        self.push_item(k, m, mk)
    }

    /// The leaf openers, and the paragraph when nothing else opens.
    fn open_leaf(&mut self, k: &Cursor<'_>, m: usize, e: usize, first: bool) {
        let r = k.rest();
        let start = k.offset();
        if !may_open(k) {
            self.open_paragraph(start, e);
            return;
        }
        if shapes::thematic_break(k) {
            let (s, attrs) = self.take_pending(start, None);
            self.push_leaf_event(LeafKind::Thematic, s, start + r.len(), attrs);
            self.closed = None;
            return;
        }
        if let Some((level, content)) = shapes::heading(k) {
            let piece = Piece {
                start: k.base + content,
                end: e,
                join: Join::Line,
            };
            self.set_leaf(
                Leaf::Heading {
                    level,
                    marker_col: m,
                    pieces: Pieces::One(Some(piece)),
                },
                start,
                e,
                None,
            );
            return;
        }
        if let Some(inc) = r
            .starts_with("{{")
            .then(|| super::include::parse(r))
            .flatten()
        {
            let (s, attrs) = self.take_pending(start, None);
            let end = start + r.len();
            // C2-B-147: a code include, whose valid options hold `code`, takes a caption.
            let target = inc.options.contains_key("code");
            self.push_leaf_event(
                LeafKind::Include {
                    path: inc.path,
                    section: inc.section,
                    options: inc.options,
                    invalid: inc.invalid,
                },
                s,
                end,
                attrs,
            );
            self.closed = Some(super::Closed {
                depth: self.stack.len(),
                target,
            });
            return;
        }
        if self.open_fence(k, m, e)
            || self.open_table(k, e)
            || self.open_attr_line(k, e)
            || self.open_caption(k, e)
        {
            return;
        }
        if self.stack.len() == 1 && k.col == 0 && self.open_definition(r, start, e) {
            return;
        }
        if shapes::comment_only(r) {
            let _ = first;
            // C2-B-136: a comment after a marker or a quote mark is content.
            let value = shapes::line_comment_value(r);
            let end = start + r.len();
            self.push_leaf_event(
                LeafKind::Comment {
                    block: false,
                    value,
                },
                start,
                end,
                None,
            );
            return;
        }
        self.open_paragraph(start, e);
    }

    fn open_paragraph(&mut self, start: usize, e: usize) {
        let piece = Piece {
            start,
            end: e,
            join: Join::Line,
        };
        self.set_leaf(
            Leaf::Para {
                pieces: Pieces::One(Some(piece)),
            },
            start,
            e,
            None,
        );
    }

    fn open_fence(&mut self, k: &Cursor<'_>, m: usize, e: usize) -> bool {
        let start = k.offset();
        if let Some((len, info)) = shapes::code_fence_opener(k) {
            if let Some((begin, end)) = info.glued {
                self.fence_opener_fallback(begin, end);
            }
            if let Some(begin) = info.further {
                self.further_words(begin, e);
            }
            self.set_leaf(
                Leaf::Code {
                    len,
                    indent: m,
                    info,
                    value: String::new(),
                    held: Vec::new(),
                },
                start,
                e,
                None,
            );
            return true;
        }
        let n = shapes::fence_run(k, b'%');
        if n >= 3 {
            let first = k
                .rest()
                .get(n..)
                .unwrap_or("")
                .trim_start_matches([' ', '\t']);
            let lines = if first.is_empty() {
                Vec::new()
            } else {
                vec![first.to_string()]
            };
            self.set_leaf(
                Leaf::CommentBlock {
                    len: n,
                    indent: m,
                    lines,
                    held: Vec::new(),
                },
                start,
                e,
                None,
            );
            return true;
        }
        self.open_div(k, e)
    }

    /// Div openers and bare colon runs (C2-B-119, C2-B-125, C2-B-127).
    fn open_div(&mut self, k: &Cursor<'_>, e: usize) -> bool {
        let Some((len, info)) = shapes::div_opener(k) else {
            return false;
        };
        let start = k.offset();
        if let Some((begin, end)) = info.glued {
            self.fence_opener_fallback(begin, end);
        }
        if let Some(begin) = info.further {
            self.further_words(begin, e);
        }
        let bare = k.rest().get(len..).is_some_and(crate::chars::is_blank_line)
            && info.name.is_none()
            && info.sigil.is_none()
            && info.title.is_none()
            && info.label.is_none();
        if bare && self.close_bare_div(k, len, start) {
            return true;
        }
        if !self.room(1) {
            return false;
        }
        if info.sigil == Some('|') {
            let (s, attrs) = self.take_pending(start, None);
            self.leaf = Some(OpenLeaf {
                leaf: Leaf::LineBlock {
                    len,
                    entries: Vec::new(),
                    comment: None,
                    stanza_open: false,
                    held: Vec::new(),
                },
                start: s,
                end: e,
                attrs,
                depth: self.stack.len(),
            });
            return true;
        }
        let (s, attrs) = self.take_pending(start, info.attrs.clone());
        let figure =
            info.name.as_deref() == Some("figure") && info.title.is_none() && info.label.is_none();
        let sigil = info.sigil;
        self.push_container(
            Kind::Div { len, sigil, figure },
            ContainerKind::Div {
                info: Box::new(info),
                sigil,
            },
            s,
            attrs,
        );
        if let Some(top) = self.stack.last_mut() {
            top.end = start + k.rest().len();
        }
        true
    }

    fn close_bare_div(&mut self, k: &Cursor<'_>, len: usize, start: usize) -> bool {
        let mut reachable = self
            .stack
            .iter()
            .rev()
            .take_while(|c| matches!(c.kind, Kind::Div { .. }));
        if let Some(depth) = reachable
            .position(|c| matches!(c.kind, Kind::Div { len: open_len, .. } if open_len <= len))
        {
            let i = self.stack.len() - depth - 1;
            // C2-B-192: attribute lines left in the div close inside it.
            if let Some(p) = self.pending.take() {
                self.pending_to_paragraph(&p);
            }
            self.close_to(i + 1);
            if let Some(top) = self.stack.last_mut() {
                top.end = start + k.rest().len();
            }
            self.fence_closes = true;
            self.pop_container();
            self.fence_closes = false;
            return true;
        }
        false
    }

    fn open_table(&mut self, k: &Cursor<'_>, e: usize) -> bool {
        let Some(parsed) = shapes::table_row_shape(k) else {
            return false;
        };
        let _ = e;
        let text = self.src.text();
        let line_end = k.offset() + k.rest().len();
        let row = table_row_data(text, k, parsed);
        let data = table::TableData { rows: vec![row] };
        self.set_leaf(Leaf::Table { data }, k.offset(), line_end, None);
        true
    }

    /// C2-B-090, C2-B-102: attribute lines merge and wait for the next line.
    fn open_attr_line(&mut self, k: &Cursor<'_>, e: usize) -> bool {
        let Some(block) = shapes::attribute_line(k) else {
            return false;
        };
        let start = k.offset();
        let end = start + k.rest().len();
        let _ = e;
        match &mut self.pending {
            Some(p) => {
                p.attrs.merge(block);
                p.end = end;
                p.lines.push((start, e));
            }
            None => {
                self.pending = Some(Pending {
                    attrs: block,
                    start,
                    end,
                    lines: vec![(start, e)],
                    depth: self.stack.len(),
                    at: self.events.len(),
                });
            }
        }
        true
    }

    /// C2-B-146: a caption directly below its block in the same container.
    fn open_caption(&mut self, k: &Cursor<'_>, e: usize) -> bool {
        let Some(content) = shapes::caption(k) else {
            return false;
        };
        if let Some(p) = self.pending.take() {
            // C2-B-096: a caption takes no attribute line.
            self.pending_to_paragraph(&p);
            return false;
        }
        let ok = matches!(self.closed, Some(Closed { depth, target: true }) if depth == self.stack.len());
        if !ok {
            return false;
        }
        let piece = Piece {
            start: k.base + content,
            end: e,
            join: Join::Line,
        };
        self.leaf = Some(OpenLeaf {
            leaf: Leaf::Caption {
                pieces: Pieces::One(Some(piece)),
            },
            start: k.offset(),
            end: e,
            attrs: None,
            depth: self.stack.len(),
        });
        true
    }

    /// Link and abbreviation definitions at column 0 (C2-B-081 to C2-B-084).
    fn open_definition(&mut self, r: &str, start: usize, e: usize) -> bool {
        if let Some(d) = shapes::link_def(r) {
            let (s, attrs) = self.take_pending(start, d.attrs);
            let end = start + r.len();
            let _ = e;
            self.push_leaf_event(
                LeafKind::LinkDef {
                    label: d.label,
                    href: d.href,
                    title: d.title,
                },
                s,
                end,
                attrs,
            );
            self.closed = None;
            return true;
        }
        if let Some((key, expansion)) = shapes::abbr_def(r) {
            let (s, attrs) = self.take_pending(start, None);
            let end = start + r.len();
            self.push_leaf_event(LeafKind::AbbrDef { key, expansion }, s, end, attrs);
            self.closed = None;
            return true;
        }
        false
    }
}

/// Interrupter (b) on the frame of the line (C2-B-015, C2-B-054, C2-B-055, C2-B-056).
fn list_interrupts(f: &Frame, m: usize, mk: &ListMarker) -> bool {
    if f.lists().any(|c| c == m) {
        return true;
    }
    if let Some(inner) = f.lists().next_back() {
        if m < inner {
            return true;
        }
    }
    if f.desc_lists().any(|c| c == m) {
        // C2-B-022: without a list to close, a description stays lazy.
        return false;
    }
    if !f.owner {
        return false;
    }
    match mk.kind {
        MarkerKind::Bullet(_) => true,
        MarkerKind::Ordered { value, .. } => value.is_none_or(|v| v == 1),
    }
}

/// C2-B-045, C2-B-219: `start` on the attribute line of an ordered list sets
/// its first number and is not kept as a key; another value stays a key.
fn start_attribute(attrs: Option<&mut AttrBlock>, style: shapes::Style, value: &mut u64) {
    let Some(a) = attrs else { return };
    let Some(i) = a
        .attrs
        .key_values
        .iter()
        .position(|(k, _)| k.eq_ignore_ascii_case("start"))
    else {
        return;
    };
    let Some((_, v)) = a.attrs.key_values.get(i) else {
        return;
    };
    let ok = !v.is_empty() && v.len() <= 9 && v.bytes().all(|b| b.is_ascii_digit());
    let Some(n) = v.parse::<u64>().ok().filter(|_| ok) else {
        return;
    };
    if n == 0 && style != shapes::Style::Decimal {
        return;
    }
    *value = n;
    a.attrs.key_values.remove(i);
}

/// Interrupter (a): a fence line (C2-B-014).
/// The first bytes of every block shape but a list marker: fences, tables,
/// thematic breaks, captions, quotes, headings, attribute and include lines,
/// definitions and description marks.
static OPENER_START: crate::chars::ByteSet = crate::chars::ByteSet::new(b"`%:|*^[>=#{", b"", None);

/// False when no shape but a list marker can start at `k`.
fn may_open(k: &Cursor<'_>) -> bool {
    k.peek_byte().is_some_and(|b| OPENER_START.has(b))
}

fn is_definition(r: &str) -> bool {
    shapes::footnote_def(r).is_some()
        || shapes::link_def(r).is_some()
        || shapes::abbr_def(r).is_some()
}

/// A continuation row `+ ... |` (C2-B-175): the fragment areas.
fn continuation(k: &Cursor<'_>) -> Option<Vec<(usize, usize)>> {
    let r = k.rest();
    let body = r.strip_prefix('+')?;
    let seps = table::separators(body);
    let last = *seps.last()?;
    if !body
        .get(last + 1..)
        .is_some_and(crate::chars::is_blank_line)
    {
        return None;
    }
    let base = k.offset() + 1;
    let mut frags = Vec::new();
    let mut s = 0;
    for &p in &seps {
        frags.push((base + s, base + p));
        s = p + 1;
    }
    Some(frags)
}

/// C2-B-197: one image form, glued attribute blocks, trailing spaces.
#[must_use]
pub fn block_image_line(line: &str) -> bool {
    if !line.starts_with("![") {
        return false;
    }
    let Some(close) = bracket_end(line, 2) else {
        return false;
    };
    let Some(after) = line.get(close + 1..) else {
        return false;
    };
    let after = match after.as_bytes().first() {
        Some(b'(') => paren_tail(after),
        Some(b'[') => label_tail(line, close + 1),
        _ => None,
    };
    after.is_some_and(trailing_attrs)
}

/// C2-I-006, C2-I-074: the `]` closing the image bracket, paired as the inline
/// scan pairs: atoms first, a note closes with no tail and holds `[^` and `^[`
/// as plain brackets (C2-I-087, C2-I-089), a nested pair takes its tail.
fn bracket_end(s: &str, start: usize) -> Option<usize> {
    AltScan::new(s).close(start)
}

/// One scan of one line. The memos keep every search linear in the line: a
/// failed search from one offset fails from every later one too.
struct AltScan<'s> {
    s: &'s str,
    /// Inner open brackets: `None` for an inline footnote, else where the text starts.
    open: Vec<Option<usize>>,
    /// The inline footnotes on `open`.
    notes: usize,
    /// The marks of the open braced spans.
    braced: Vec<u8>,
    /// Backtick run lengths that have no closing run further on.
    dead_runs: Vec<usize>,
    /// No `%}` follows this offset.
    no_comment_close: usize,
    /// The last label scan: (from, stop, closed); a scan from inside it stops alike.
    label_memo: Option<(usize, usize, bool)>,
    /// Per `(` offset, where its link destination ends.
    parens: Option<Vec<Paren>>,
}

impl<'s> AltScan<'s> {
    fn new(s: &'s str) -> Self {
        Self {
            s,
            open: Vec::new(),
            notes: 0,
            braced: Vec::new(),
            dead_runs: Vec::new(),
            no_comment_close: usize::MAX,
            label_memo: None,
            parens: None,
        }
    }

    fn close(&mut self, start: usize) -> Option<usize> {
        let b = self.s.as_bytes();
        let mut i = start;
        while let Some(&c) = b.get(i) {
            if let Some(next) = self.atom_end(i) {
                i = next;
                continue;
            }
            i = match c {
                b'^' if b.get(i + 1) == Some(&b'[') => self.note_open(i),
                // C2-I-087: the bracket of a nested image opener is no footnote reference, `![^]` included.
                b'!' if b.get(i + 1) == Some(&b'[') => {
                    self.open.push(Some(i + 2));
                    i + 2
                }
                b'[' => self.bracket_open(i),
                b']' => match self.open.pop() {
                    None => return Some(i),
                    Some(None) => {
                        self.notes -= 1;
                        i + 1
                    }
                    Some(Some(text)) => self.nested_tail_end(text, i),
                },
                _ => self.braced_step(i),
            };
        }
        None
    }

    /// C2-I-005, C2-I-074: behind the atom at `i`, if one starts there.
    fn atom_end(&mut self, i: usize) -> Option<usize> {
        let b = self.s.as_bytes();
        match b.get(i)? {
            b'\\' => Some(i + 2),
            b'`' => Some(self.code_end(i)),
            b'<' => Some(angle_end(self.s, i)),
            b'{' if b.get(i + 1) == Some(&b'%') => Some(self.comment_end(i)),
            _ => None,
        }
    }

    /// C2-I-089: `^[` opens a note; inside one the caret is text.
    fn note_open(&mut self, i: usize) -> usize {
        if self.notes > 0 {
            return i + 1;
        }
        self.open.push(None);
        self.notes += 1;
        i + 2
    }

    /// C2-I-087: a footnote reference is one atom outside a note; else `[` opens.
    fn bracket_open(&mut self, i: usize) -> usize {
        if self.notes == 0 {
            if let Some(end) = self.footnote_ref_end(i) {
                return end;
            }
        }
        self.open.push(Some(i + 1));
        i + 1
    }

    /// C2-I-043: a braced mark that opens or closes a span at `i`, else one byte.
    fn braced_step(&mut self, i: usize) -> usize {
        let b = self.s.as_bytes();
        if b.get(i) == Some(&b'{') {
            if let Some(mark) = braced_open_mark(b, i) {
                self.braced.push(mark);
                return i + 2;
            }
        }
        if self
            .braced
            .last()
            .is_some_and(|&m| b.get(i) == Some(&m) && b.get(i + 1) == Some(&b'}'))
        {
            self.braced.pop();
            return i + 2;
        }
        i + 1
    }

    /// C2-I-014, C2-I-119: behind the code span, or behind its run when unclosed.
    fn code_end(&mut self, i: usize) -> usize {
        let run = self
            .s
            .as_bytes()
            .get(i..)
            .map_or(0, |r| r.iter().take_while(|&&c| c == b'`').count());
        if self.dead_runs.contains(&run) {
            return i + run;
        }
        let span = crate::inline::code_span(self.s, i);
        if span.closed {
            return span.end;
        }
        self.dead_runs.push(run);
        i + run
    }

    /// C2-I-025: behind the braced comment at `i`, or `i + 1` when it does not close.
    fn comment_end(&mut self, i: usize) -> usize {
        if i >= self.no_comment_close {
            return i + 1;
        }
        if let Some(k) = self.s.get(i + 2..).and_then(|r| r.find("%}")) {
            i + 2 + k + 2
        } else {
            self.no_comment_close = i;
            i + 1
        }
    }

    /// C2-I-076, C2-I-077: the `]` ending a label that starts at `from`, or
    /// `None` when a line comment or the end of the line comes first.
    fn label_close(&mut self, from: usize) -> Option<usize> {
        if let Some((at, stop, closed)) = self.label_memo {
            if at <= from && from <= stop {
                return closed.then_some(stop);
            }
        }
        let b = self.s.as_bytes();
        let mut j = from;
        let (stop, closed) = loop {
            match b.get(j) {
                None => break (j, false),
                Some(b'\\') => j += 2,
                Some(b']') => break (j, true),
                Some(b'%') if crate::inline::comment_start(self.s, j) => break (j, false),
                Some(_) => j += 1,
            }
        };
        self.label_memo = Some((from, stop, closed));
        closed.then_some(stop)
    }

    /// C2-I-087: behind the footnote reference at `i`, `Some(i + 1)` for a
    /// label-less `[^` (text, no bracket), `None` when `[` opens a plain bracket.
    fn footnote_ref_end(&mut self, i: usize) -> Option<usize> {
        if self.s.as_bytes().get(i + 1) != Some(&b'^') {
            return None;
        }
        Some(match self.label_close(i + 2) {
            Some(close)
                if !self
                    .s
                    .get(i + 2..close)
                    .is_some_and(|k| k.bytes().all(|c| matches!(c, b' ' | b'\t' | b'\n'))) =>
            {
                close + 1
            }
            _ => i + 1,
        })
    }

    /// C2-I-066 to C2-I-072, C2-I-078: behind what is glued to the `]` at `i`
    /// of a nested pair whose text starts at `text`; a tail that forms nothing
    /// leaves the scan behind the `]`, as in the inline scan.
    fn nested_tail_end(&mut self, text: usize, i: usize) -> usize {
        let b = self.s.as_bytes();
        let keyed = |a: usize, z: usize| {
            self.s
                .get(a..z)
                .is_some_and(|k| !crate::chars::fold_label(k).is_empty())
        };
        match (b.get(i + 1), b.get(i + 2)) {
            (Some(b'('), _) => self.paren_end(i + 1).unwrap_or(i + 1),
            (Some(b'{'), _) => attr_run_end(self.s, i + 1).unwrap_or(i + 1),
            (Some(b'['), Some(b']')) if keyed(text, i) => i + 3,
            (Some(b'['), Some(b']' | b'@' | b'^')) => i + 1,
            (Some(b'['), _) => match self.label_close(i + 2) {
                Some(close) if keyed(i + 2, close) => close + 1,
                _ => i + 1,
            },
            _ => i + 1,
        }
    }

    /// `paren_tail` from the `(` at `q`, read off the paren matches so that
    /// repeated tails stay linear.
    fn paren_end(&mut self, q: usize) -> Option<usize> {
        let parens = self.parens.get_or_insert_with(|| paren_matches(self.s));
        match parens.get(q)? {
            Paren::Close(close) => Some(close + 1),
            // Balanced up to a space: only a title can end the tail.
            Paren::Space(at) => paren_title_tail(self.s, at + 1).map(|t| self.s.len() - t.len()),
            Paren::Dead => None,
        }
    }
}

#[derive(Clone, Copy)]
enum Paren {
    Dead,
    Close(usize),
    Space(usize),
}

/// For every `(`, how `paren_tail` ends its destination: at the matching `)`,
/// at a space with the inner parens balanced, or not at all (C2-I-013, C2-I-066).
fn paren_matches(s: &str) -> Vec<Paren> {
    let b = s.as_bytes();
    let mut out = vec![Paren::Dead; b.len()];
    let mut stack: Vec<usize> = Vec::new();
    let mut i = 0;
    while let Some(&c) = b.get(i) {
        match c {
            b'\\' if matches!(b.get(i + 1), Some(b'(' | b')' | b'\\')) => i += 1,
            b'(' => stack.push(i),
            b')' => {
                if let Some(slot) = stack.pop().and_then(|o| out.get_mut(o)) {
                    *slot = Paren::Close(i);
                }
            }
            b' ' | b'\t' => {
                if c == b' ' {
                    if let Some(slot) = stack.last().and_then(|&o| out.get_mut(o)) {
                        *slot = Paren::Space(i);
                    }
                }
                stack.clear();
            }
            _ => {}
        }
        i += 1;
    }
    out
}

/// C2-I-073, C2-I-096: behind the autolink or cross-reference, or `i + 1`.
fn angle_end(s: &str, i: usize) -> usize {
    match crate::inline::angle(s, i) {
        Some(
            crate::inline::Angle::Xref { end, .. } | crate::inline::Angle::Autolink { end, .. },
        ) => end,
        None => i + 1,
    }
}

/// Behind a run of glued non-empty attribute blocks from `at`, if any.
fn attr_run_end(s: &str, mut at: usize) -> Option<usize> {
    let mut end = None;
    while s.as_bytes().get(at) == Some(&b'{') {
        match crate::attr::parse_block(s, at) {
            Some((block, next)) if !block.empty => {
                end = Some(next);
                at = next;
            }
            _ => break,
        }
    }
    end
}

fn braced_open_mark(bytes: &[u8], i: usize) -> Option<u8> {
    bytes
        .get(i + 1)
        .copied()
        .filter(|mark| crate::tables::BRACED_MARKS.as_bytes().contains(mark))
}

fn label_tail(s: &str, start: usize) -> Option<&str> {
    let b = s.as_bytes();
    let alt = s.get(2..start.saturating_sub(1))?;
    if b.get(start + 1) == Some(&b']') {
        return (!crate::chars::fold_label(alt).is_empty())
            .then(|| s.get(start + 2..))
            .flatten();
    }
    if matches!(b.get(start + 1), Some(b'@' | b'^')) {
        return None;
    }
    let crate::inline::Label::Ok(close, end) = crate::inline::label(s, start + 1, true) else {
        return None;
    };
    let key = s.get(start + 1..close)?;
    (!crate::chars::fold_label(key).is_empty())
        .then(|| s.get(end..))
        .flatten()
}

/// `(destination "title")`: a destination has no space (`data/line-shapes.json`
/// part `destination`), balanced parentheses, and an optional quoted title.
fn paren_tail(s: &str) -> Option<&str> {
    let b = s.as_bytes();
    let mut depth = 0usize;
    let mut i = 1;
    loop {
        match *b.get(i)? {
            b'\\' => {
                if matches!(b.get(i + 1), Some(b'(' | b')' | b'\\')) {
                    i += 1;
                }
            }
            b'(' => depth += 1,
            b')' if depth == 0 => return s.get(i + 1..),
            b')' => depth -= 1,
            b' ' | b'\t' => break,
            c if c >= 0x80 => {
                let ch = s.get(i..)?.chars().next()?;
                i += ch.len_utf8() - 1;
            }
            _ => {}
        }
        i += 1;
    }
    (depth == 0 && b.get(i) == Some(&b' '))
        .then(|| paren_title_tail(s, i + 1))
        .flatten()
}

fn paren_title_tail(s: &str, mut i: usize) -> Option<&str> {
    let b = s.as_bytes();
    let q = *b.get(i).filter(|&&c| c == b'"' || c == b'\'')?;
    i += 1;
    loop {
        match *b.get(i)? {
            b'\\' => i += 1,
            c if c == q => break,
            _ => {}
        }
        i += 1;
    }
    i += 1;
    while b.get(i) == Some(&b' ') {
        i += 1;
    }
    (b.get(i) == Some(&b')')).then(|| s.get(i + 1..)).flatten()
}

/// Glued attribute blocks and trailing spaces; an empty `{}` here is text.
fn trailing_attrs(mut s: &str) -> bool {
    while s.starts_with('{') {
        match crate::attr::parse_block(s, 0) {
            Some((block, end)) if !block.empty => s = s.get(end..).unwrap_or(""),
            _ => return false,
        }
    }
    crate::chars::is_blank_line(s)
}

/// C2-B-198, C2-I-121: `$$`, a code span closed on the line, attributes, spaces.
#[must_use]
pub fn display_math_line(line: &str) -> bool {
    let Some(rest) = line.strip_prefix("$$") else {
        return false;
    };
    let n = rest.bytes().take_while(|&b| b == b'`').count();
    if n == 0 {
        return false;
    }
    let b = rest.as_bytes();
    let mut i = n;
    while i < b.len() {
        if b.get(i) == Some(&b'`') {
            let m = b
                .get(i..)
                .map_or(0, |r| r.iter().take_while(|&&c| c == b'`').count());
            if m == n {
                return rest.get(i + m..).is_some_and(trailing_attrs);
            }
            i += m;
        } else {
            i += 1;
        }
    }
    false
}

/// C2-A-086: a row's trailing comment is a sibling after the row.
fn table_row_data(text: &str, k: &Cursor<'_>, shape: shapes::TableRowShape) -> Row {
    let shapes::TableRowShape {
        attrs,
        comment,
        separators,
    } = shape;
    let line_end = k.offset() + k.rest().len();
    let comments = comment.map_or_else(Vec::new, |at| {
        let value = shapes::line_comment_value(
            k.text
                .get(at..)
                .unwrap_or("")
                .trim_start_matches([' ', '\t']),
        );
        vec![(k.base + at, line_end, value)]
    });
    Row {
        cells: table::parse_cells_known(text, k.offset(), &separators),
        start: k.offset(),
        end: comment.map_or(line_end, |at| k.base + at),
        attrs,
        comments,
        ..Row::default()
    }
}
