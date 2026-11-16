//! Interrupters (C2-B-013 to C2-B-024) and the openers of a line.

use super::events::{ContainerKind, LeafKind};
use super::line::Cursor;
use super::shapes::{self, ListMarker, MarkerKind};
use super::table::{self, Row};
use super::{Closed, Container, Kind, Leaf, OpenLeaf, Parser, Pending, Walk};
use crate::attr::AttrBlock;
use crate::inline::{Join, Piece};
use crate::tables::{BLOCK_DEPTH, LIST_END_BLANK_LINES};

/// The lists, description lists and marker owners a line can reach:
/// those inside the innermost quote whose mark it carries and outside every
/// quote whose mark it lacks (C2-B-020).
struct Frame {
    lists: Vec<usize>,
    desc_lists: Vec<usize>,
    owner: bool,
    footnote: bool,
}

impl Parser<'_> {
    fn frame(&self, walk: &Walk<'_>, m: usize) -> Frame {
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
        let mut f = Frame {
            lists: Vec::new(),
            desc_lists: Vec::new(),
            owner: false,
            footnote: false,
        };
        for c in self.stack.get(lo..hi).unwrap_or(&[]) {
            match &c.kind {
                Kind::List { col, .. } => f.lists.push(*col),
                Kind::DescList { col, .. } => f.desc_lists.push(*col),
                k => {
                    if let Some((marker, _)) = k.indent_cols() {
                        if m > marker {
                            f.owner = true;
                        }
                    }
                }
            }
        }
        f.footnote = self
            .stack
            .iter()
            .any(|c| matches!(c.kind, Kind::Footnote { .. }));
        f
    }

    /// Whether the line ends the open paragraph-like leaf (the six interrupters).
    pub(super) fn interrupts(&self, walk: &Walk<'_>) -> bool {
        let k = walk.cur.after_blanks();
        if fence_line(&k) || shapes::table_row(&k).is_some() || shapes::thematic_break(&k) {
            return true;
        }
        let m = k.col - walk.qbase;
        if shapes::caption(&k).is_some() {
            return self.caption_interrupts(walk);
        }
        let f = self.frame(walk, m);
        if let Some(marker) = shapes::list_marker(&k) {
            // C2-B-190: a nested marker that cannot open is paragraph text.
            let can_replace = f.lists.iter().any(|&col| m <= col);
            if !can_replace && !self.room(2) {
                return false;
            }
            return list_interrupts(&f, m, &marker);
        }
        if let Some((_, _, _)) = shapes::desc_mark(&k) {
            return f.desc_lists.contains(&m);
        }
        walk.qbase == 0 && k.col == 0 && f.footnote && is_definition(k.rest())
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
            if let Some(parsed) = shapes::table_row(&k) {
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
                    pieces: vec![piece],
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
    fn take_pending(
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
        let mut value = match mk.kind {
            MarkerKind::Ordered { value, .. } => Some(value.unwrap_or(1)),
            MarkerKind::Bullet(_) => None,
        };
        if let (Some(v), MarkerKind::Ordered { style, .. }) = (&mut value, mk.kind) {
            start_attribute(attrs.as_mut(), style, v);
        }
        let list = Kind::List {
            kind: mk.kind,
            col: m,
            loose: false,
            items: 1,
        };
        self.push_container(
            list,
            ContainerKind::List {
                kind: mk.kind,
                start: value,
            },
            start,
            attrs,
        );
        self.push_item(k, m, mk)
    }

    /// The leaf openers, and the paragraph when nothing else opens.
    fn open_leaf(&mut self, k: &Cursor<'_>, m: usize, e: usize, first: bool) {
        let r = k.rest();
        let start = k.offset();
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
                    pieces: vec![piece],
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
            self.closed = None;
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
        let piece = Piece {
            start,
            end: e,
            join: Join::Line,
        };
        self.set_leaf(
            Leaf::Para {
                pieces: vec![piece],
            },
            start,
            e,
            None,
        );
    }

    fn open_fence(&mut self, k: &Cursor<'_>, m: usize, e: usize) -> bool {
        let start = k.offset();
        if let Some((len, info)) = shapes::code_fence_opener(k) {
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
                    lines: Vec::new(),
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
            self.pop_container();
            return true;
        }
        false
    }

    fn open_table(&mut self, k: &Cursor<'_>, e: usize) -> bool {
        let Some(parsed) = shapes::table_row(k) else {
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
                pieces: vec![piece],
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
            let (s, attrs) = self.take_pending(start, None);
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
    if f.lists.contains(&m) {
        return true;
    }
    if let Some(&inner) = f.lists.last() {
        if m < inner {
            return true;
        }
    }
    if f.desc_lists.contains(&m) {
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
fn fence_line(k: &Cursor<'_>) -> bool {
    shapes::code_fence_opener(k).is_some()
        || shapes::fence_run(k, b'%') >= 3
        || shapes::div_opener(k).is_some()
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

fn bracket_end(s: &str, start: usize) -> Option<usize> {
    let b = s.as_bytes();
    let mut depth = 0usize;
    let mut i = start;
    while let Some(&c) = b.get(i) {
        match c {
            b'\\' => i += 1,
            b'[' => depth += 1,
            b']' if depth == 0 => return Some(i),
            b']' => depth -= 1,
            b'%' if crate::inline::comment_start(s, i) => return None,
            b'`' => {
                let span = crate::inline::code_span(s, i);
                if !span.closed {
                    return None;
                }
                i = span.end;
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    None
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

fn trailing_attrs(mut s: &str) -> bool {
    while s.starts_with('{') {
        match crate::attr::parse_block(s, 0) {
            Some((_, end)) => s = s.get(end..).unwrap_or(""),
            None => return false,
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
fn table_row_data(
    text: &str,
    k: &Cursor<'_>,
    (s, z, attrs, comment): (usize, usize, Option<crate::attr::AttrBlock>, Option<usize>),
) -> Row {
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
        cells: table::parse_cells(text, k.base + s, k.base + z),
        start: k.offset(),
        end: comment.map_or(line_end, |at| k.base + at),
        attrs,
        comments,
        ..Row::default()
    }
}
