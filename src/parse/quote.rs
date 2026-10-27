use super::*;

pub(in crate::parse) fn parse_blockquote(
    cur: &mut LineCursor,
    options: &Options<'_>,
) -> Box<BlockNode> {
    let (span_start, parts, _) = collect_blockquote_body(cur, options);
    let mut children = Vec::new();
    for part in parts {
        let source = part.into_source();
        let blocks = parse_quoted_mapped_source(&source, options);
        drop(source);
        children.extend(blocks);
    }
    if options.positions {
        let original = cur.lines.at(span_start);
        if let Some(after_outer) = strip_blockquote_prefix(original) {
            if strip_blockquote_prefix(after_outer).is_some_and(is_plus_marker) {
                if let (Some(line), Some(column), Some(BlockNode::BlockQuote(inner))) = (
                    cur.source_line(span_start),
                    cur.source_col(span_start),
                    children.first_mut(),
                ) {
                    if let Some(pos) = inner.pos.as_mut() {
                        if pos.start_line > line {
                            let outer_width = original[..original.len() - after_outer.len()]
                                .chars()
                                .count();
                            pos.start_line = line;
                            pos.start_column = document_column(column, outer_width);
                        }
                    }
                }
            }
        }
    }
    finish_blockquote(cur, options, span_start, children)
}

#[inline(never)]
pub(in crate::parse) fn finish_blockquote(
    cur: &mut LineCursor,
    options: &Options<'_>,
    span_start: usize,
    children: Vec<BlockNode>,
) -> Box<BlockNode> {
    let quote_pos = span_of(cur, span_start, cur.pos, options);
    // PART 9 §4d: a quote built out of nothing but structural-only `>` lines is
    // not a captionable host. It holds nothing the caption could be about, and
    // taking one binds the caption across the container change that opened the
    // quote - which is how `| a |` / blank / `>` / blank / `^ cap` captioned an
    // empty quote nobody wrote the caption under (markup-carve/carve#2892).
    let caption_after_blank_reaches =
        cur.peek().is_some_and(is_blank_line) && cur.carried_reach(cur.pos + 1) == Some(true);
    let caption = if children.is_empty()
        || (cur.carried_reach(cur.pos) == Some(false) && !caption_after_blank_reaches)
    {
        None
    } else {
        consume_caption(cur, options)
    };
    let figure_pos = span_of(cur, span_start, cur.pos, options);
    boxed_blockquote_node(children, quote_pos, caption, figure_pos, false)
}

/// Which half of a definition-list entry the quote's own lines last left open.
///
/// A lazy `:`-shaped line attaches a fresh description to an open TERM, and is
/// the open DESCRIPTION's own continuation when there is one - two answers for
/// one shape, which is why the half has to be tracked rather than the mere
/// presence of a list (markup-carve/carve-js#1608 measured the same split).
#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::parse) enum DefHalf {
    Term,
    Description,
}

/// What a quote's OWN lines have left open for a lazy line below them: which
/// half of a definition entry, and the content column of the innermost
/// container.
///
/// ADVANCED ON DEMAND. `strip_blockquote_prefix` is counted, and a depth ladder
/// must not pay a walk per level per line (`quote_prefix_calls`), so nothing
/// here runs until a `:`-shaped lazy line actually asks - and the scan resumes
/// from where it stopped, so the whole walk is one pass over the quote's lines
/// however many times it is asked.
#[derive(Default)]
pub(in crate::parse) struct QuotedEntryScan {
    scanned: usize,
    quote_depth: usize,
    half: Option<DefHalf>,
    content_col: Option<usize>,
    description_col: Option<usize>,
    // A BLOCK closed the innermost entry, so no paragraph is open below it. This
    // is what separates the two `half == None` states: no entry ever opened
    // (a paragraph is still open and a lazy marker folds into it) versus an
    // entry a block ended (a lazy marker below the content column is the
    // enclosing item's own text - carve-rs#1526's below-column row, #1538).
    closed_by_block: bool,
}

impl QuotedEntryScan {
    fn advance(&mut self, lines: &[SourceLine]) {
        while self.scanned < lines.len() {
            let line = &lines[self.scanned].text;
            self.scanned += 1;
            // A BLANK ONLY LOOSENS THE LIST, so the open half survives it and
            // a `:` below is still the open description's own continuation.
            // Resetting here put 12 documents of the blank-then-reopen band on
            // the wrong side of the oracle.
            if is_blank_line(line) {
                continue;
            }
            // PEEL NESTED QUOTE MARKERS. A line still carrying a `>` opens the
            // INNER quote whose entry a lazy line below is continuing - `> > -
            // :: t` collected at the OUTER quote keeps one `>`, and without
            // peeling it the term went unseen and a lazy `:: u` below reported no
            // open half, so it framed and escaped instead of opening a second
            // term (markup-carve/carve-rs#1538). The single-quote sweeps did not
            // exercise this, which is why the earlier no-peel form measured clean.
            let mut peeled = line.as_str();
            let mut quote_depth = 0;
            while let Some(after) = strip_blockquote_prefix(peeled) {
                peeled = after;
                quote_depth += 1;
            }
            if quote_depth != self.quote_depth {
                self.half = None;
                self.description_col = None;
                self.content_col = None;
                self.closed_by_block = false;
                self.quote_depth = quote_depth;
            }
            let column = indent_columns(peeled);
            let rest = trim_ascii_start(peeled);
            if !line.starts_with(LAZY) && self.description_col.is_some_and(|base| column < base) {
                self.description_col = None;
            }
            let ladder = innermost_marker_content_col(rest);
            if ladder > 0 {
                self.content_col = Some(column + ladder);
            }
            let content = trim_ascii_start(innermost_marker_content(rest));
            if is_definition_list_start(content) {
                self.half = Some(DefHalf::Term);
                self.description_col = None;
                self.closed_by_block = false;
            } else if self.half.is_some() && strip_definition_marker(content).is_some() {
                self.half = Some(DefHalf::Description);
                self.description_col =
                    definition_body_content_col(content).map(|body_col| column + ladder + body_col);
                self.closed_by_block = false;
            } else if item_block_opener(content) {
                // A block written inside the entry closes it, so a lazy line
                // below has no open half to continue.
                self.half = None;
                // ONLY A LEAF block leaves no open paragraph below it. A quote or
                // a nested list is a CONTAINER whose own innermost paragraph a
                // lazy marker folds into, so it does not make the marker the
                // enclosing item's loose text - `> - > x` / ` :: t` folds `:: t`
                // into the quoted paragraph, not into the item (carve-rs#1538).
                self.closed_by_block = detect_heading(content).is_some()
                    || thematic_break_marker(content).is_some()
                    || detect_fence_open(content).is_some()
                    || detect_comment_fence_line(content).is_some()
                    || is_table_start(content);
            }
        }
    }
}

/// Whether a fence leaves the innermost quote of a collected body without an
/// open paragraph, or `None` when no fence decided it.
///
/// `ParaOpen` reads one line on its own. At the quote's own level `in_fence`
/// carries the fence, but one level down a fence opener read alone looks like
/// paragraph text whenever no closer follows it: a fence at the start of a
/// nested quote was taken for inline verbatim, and the line below folded into
/// the outer quote (markup-carve/carve-rs#2117). This replays the body with a
/// fence and paragraph state per nesting level, asked only for a lazy line
/// after a nested quoted line.
///
/// INCREMENTAL. Each line is replayed once however often a lazy line asks. That
/// is exact because a closer lookahead stops at the first line that leaves the
/// quote, and the line that asks is such a line.
#[derive(Default)]
pub(in crate::parse) struct NestedFenceReplay {
    scanned: usize,
    chain: Vec<NestedLevel>,
    fence_count: usize,
    colon_body_comments: HashMap<usize, QuotedBodyComments>,
    literal_code_lookup: LiteralCodeLookup,
    literal_comment_openers: HashSet<usize>,
}

#[derive(Default)]
pub(in crate::parse) struct NestedLevel {
    hosts: Vec<(usize, usize, bool)>,
    term_columns: HashSet<usize>,
    /// The fence a quoted host holds open, as (opener, the host's content column,
    /// the opener's own column, and whether this is a payload span). The two columns differ when the opener is
    /// SHIFTED past its host, and a closer at either one ends it.
    ///
    /// A shifted opener used to be parked in a `pending` slot instead, which left
    /// the host's paragraph open over the payload, so a flush-left line folded
    /// into it (carve-rs#2170). Section 24 C3 asks a processor to NAME such an
    /// opener rather than refuse it, so the host holds its payload exactly as it
    /// holds a canonical one. Ported from carve-js#2356, which landed first.
    host_fence: Option<(FenceOpen, usize, usize, bool)>,
    fence: Option<FenceOpen>,
    comment: Option<usize>,
    by_comment: bool,
    host_comment: Option<(usize, usize, bool)>,
    para: bool,
    by_fence: bool,
    colons: Vec<(usize, usize, usize)>,
    by_colon: bool,
    colon_absorption: bool,
    host_verse: Option<(usize, usize, usize)>,
    fence_ordinal: usize,
}

impl NestedFenceReplay {
    fn marker_fence_tail(&self) -> bool {
        self.chain.last().is_some_and(|level| {
            level.fence.is_some()
                || level.host_fence.is_some()
                || (level.by_fence && level.fence_ordinal == 1)
        })
    }

    fn verdict(&mut self, lines: &[SourceLine], cur: &LineCursor<'_>) -> Option<bool> {
        if self.chain.is_empty() {
            self.chain.push(NestedLevel::default());
        }
        while self.scanned < lines.len() {
            let at = self.scanned;
            self.scanned += 1;
            self.replay(lines, at, cur);
        }
        let deepest = self.chain.last()?;
        if deepest.host_verse.is_some() && deepest.para && deepest.host_comment.is_none() {
            return Some(true);
        }
        if deepest.para
            && deepest.fence.is_none()
            && deepest.host_fence.is_none()
            && !deepest.by_fence
        {
            return Some(true);
        }
        (deepest.fence.is_some()
            || deepest.host_fence.is_some()
            || deepest.by_fence
            || deepest.by_colon
            || deepest.host_comment.is_some())
        .then_some(false)
    }

    fn replay(&mut self, lines: &[SourceLine], at: usize, cur: &LineCursor<'_>) {
        let mut text = lines[at].text.as_str();
        // A lazy line was taken only because a paragraph was open, and it
        // continues that paragraph.
        if text.starts_with(LAZY) {
            return;
        }
        let mut pos = ReplayAt {
            lines,
            at,
            level: 0,
            cur,
        };
        loop {
            if self.replay_held(pos, text) {
                return;
            }
            let Some(rest) = strip_blockquote_prefix(text) else {
                break;
            };
            self.descend(pos.level);
            text = rest;
            pos.level += 1;
        }
        self.replay_line(pos, text);
    }

    /// Feeds `text` to a block that `pos.level` already holds open. True when
    /// that block took the line.
    fn replay_held(&mut self, pos: ReplayAt<'_, '_>, text: &str) -> bool {
        self.replay_host_fence(pos.level, text)
            || self.replay_fence_body(pos.level, text)
            || self.replay_host_comment(pos.level, text)
            || self.replay_host_verse(pos, text)
    }

    fn replay_host_fence(&mut self, level: usize, text: &str) -> bool {
        let here = &mut self.chain[level];
        let Some((open, floor, opener, payload)) = here.host_fence else {
            return false;
        };
        if !is_blank_line(text) && indent_columns(text) < floor {
            here.host_fence = None;
            return false;
        }
        // A block closer reaches the host or opener column.
        // Payload spans pair at any column still owned by the item.
        let at = indent_columns(text);
        if (payload || at == floor || at == opener) && is_fence_close(trim_ascii_start(text), open)
        {
            here.host_fence = None;
        }
        here.by_fence = true;
        here.para = here.host_verse.is_some() && !is_blank_line(text);
        here.colon_absorption = false;
        self.chain.truncate(level + 1);
        true
    }

    fn replay_fence_body(&mut self, level: usize, text: &str) -> bool {
        let here = &mut self.chain[level];
        if let Some(open) = here.fence {
            if is_fence_close(text, open) {
                here.fence = None;
                here.by_fence = true;
            }
        } else if let Some(len) = here.comment {
            // Comment contents are not syntax. A comment never overrides the
            // one-line reading, so it leaves `by_fence` alone.
            if is_comment_fence_close(text, len) {
                here.comment = None;
                here.by_comment = true;
            }
        } else {
            return false;
        }
        self.chain.truncate(level + 1);
        true
    }

    // A collected comment is opaque. A closer beyond the collected
    // item leaves a line comment, so its following blocks stay visible.
    fn replay_host_comment(&mut self, level: usize, text: &str) -> bool {
        let here = &mut self.chain[level];
        let Some((width, column, provisional)) = here.host_comment else {
            return false;
        };
        if !is_blank_line(text) && indent_columns(text) < column {
            here.host_comment = None;
            return false;
        }
        if is_comment_fence_close(trim_ascii_start(text), width) {
            here.host_comment = None;
            here.by_comment = here
                .hosts
                .last()
                .is_some_and(|&(_, floor, _)| column < floor);
        }
        if provisional {
            return false;
        }
        self.chain.truncate(level + 1);
        true
    }

    fn replay_host_verse(&mut self, pos: ReplayAt<'_, '_>, text: &str) -> bool {
        let level = pos.level;
        let Some((width, floor, column)) = self.chain[level].host_verse else {
            return false;
        };
        if !is_blank_line(text) && indent_columns(text) < floor {
            self.chain[level].host_verse = None;
            return false;
        }
        let trimmed = trim_ascii_start(text);
        let indent = indent_columns(text);
        if let Some(comment) = detect_comment_fence_line(trimmed) {
            self.literal_comment_openers.insert(pos.at);
            let len = comment.fence_len;
            let collected = self
                .colon_body_comments
                .entry(level)
                .or_default()
                .closes_after(pos.lines, pos.at, level, len);
            if collected || future_quoted_comment_closes(pos.cur, level + 1, indent, len) {
                self.chain[level].host_comment = Some((len, indent, !collected));
            }
        }
        if let Some(open) = detect_fence_open(trimmed) {
            if literal_code_closes(
                pos.lines,
                pos.at,
                level,
                floor,
                indent,
                open,
                &mut self.literal_code_lookup,
            ) {
                self.chain[level].host_fence = Some((open, floor, indent, true));
            }
        }
        let here = &mut self.chain[level];
        if (indent == floor || indent == column) && exact_colon_fence_len(trimmed) == Some(width) {
            here.host_verse = None;
            here.para = false;
        } else {
            here.para = !is_blank_line(text) && !is_line_comment_any_column(text);
        }
        here.colon_absorption = false;
        here.by_colon = true;
        self.chain.truncate(level + 1);
        true
    }

    fn descend(&mut self, level: usize) {
        let here = &mut self.chain[level];
        here.hosts.clear();
        here.term_columns.clear();
        here.colons.clear();
        here.para = false;
        here.colon_absorption = false;
        here.by_fence = false;
        if self.chain.len() == level + 1 {
            self.chain.push(NestedLevel::default());
        }
    }

    /// Replays a line at the deepest quote level it reaches, past every block
    /// that level held open.
    fn replay_line(&mut self, pos: ReplayAt<'_, '_>, text: &str) {
        let level = pos.level;
        if level + 1 == self.chain.len()
            && (!self.chain[level].colons.is_empty() || self.chain[level].by_colon)
            && is_line_comment_any_column(text)
        {
            self.chain[level].replay_colon_comment(pos, text);
            return;
        }
        if self.continues_deepest(pos, text) {
            return;
        }
        self.chain.truncate(level + 1);
        let here = &mut self.chain[level];
        here.by_fence = false;
        here.by_colon = false;
        let after_comment = std::mem::take(&mut here.by_comment);
        if after_comment
            && here.hosts.last().is_some_and(|&(marker, column, _)| {
                indent_columns(text) > marker && indent_columns(text) < column
            })
            && !is_blank_line(text)
            && !interrupts_paragraph_with_rest(text, &[])
        {
            here.para = true;
        }
        let lazy_paragraph = self.lazy_paragraph(pos, text);
        let here = &mut self.chain[level];
        if !is_blank_line(text) && !lazy_paragraph {
            while here
                .colons
                .last()
                .is_some_and(|&(_, floor, _)| floor > indent_columns(text))
            {
                here.colons.pop();
            }
        }
        if here.closes_colon(text) {
            return;
        }
        let absorption_text = if is_blank_line(text) {
            text
        } else {
            match self.replay_hosted(pos, text, lazy_paragraph) {
                Some(inner) => inner,
                None => return,
            }
        };
        self.replay_unhosted(pos, text, after_comment, absorption_text);
    }

    /// A line short of the deepest quote continues that quote's open
    /// paragraph unless it interrupts one.
    fn continues_deepest(&self, pos: ReplayAt<'_, '_>, text: &str) -> bool {
        let deepest_open = self
            .chain
            .last()
            .is_some_and(|l| l.para && l.fence.is_none());
        if self.chain.len() <= pos.level + 1 || !deepest_open || is_blank_line(text) {
            return false;
        }
        let interrupts = match detect_fence_open(text) {
            Some(open) => closer_ahead(pos.lines, pos.at, pos.level, |l| is_fence_close(l, open)),
            None => interrupts_paragraph_with_rest(text, &[]),
        };
        !interrupts
    }

    // Replay the same handoff before an unquoted line asks whether
    // the quoted list left a paragraph open.
    fn lazy_paragraph(&mut self, pos: ReplayAt<'_, '_>, text: &str) -> bool {
        let here = &self.chain[pos.level];
        let column = indent_columns(text);
        let trimmed = trim_ascii_start(text);
        let starts_sibling = detect_list_marker_full(trimmed).is_some()
            && marker_ends_replayed_item(&here.hosts, column);
        let hosted_colon_opener = here.hosts.iter().any(|&(_, host, _)| host <= column)
            && (!here.colon_absorption || exact_colon_fence_len(trimmed).is_none())
            && (detect_container_open(trimmed).is_some()
                || detect_line_block_open(trimmed).is_some()
                || detect_hardbreaks_block_open(trimmed).is_some()
                || detect_quote_block_open(trimmed).is_some());
        here.para
            && !starts_sibling
            && !hosted_colon_opener
            && !(here
                .hosts
                .last()
                .is_some_and(|&(_, floor, _)| column < floor)
                && detect_fence_open(trimmed).is_some_and(|open| {
                    indexed_code_closes(
                        pos.lines,
                        pos.at,
                        pos.level,
                        column,
                        None,
                        open,
                        &mut self.literal_code_lookup,
                    )
                }))
            && !match detect_fence_open(text) {
                Some(open) => closer_ahead(pos.lines, pos.at, pos.level, |line| {
                    is_fence_close(line, open)
                }),
                None => interrupts_paragraph_with_rest(text, &[]),
            }
    }

    /// Replays a non-blank line against the list and footnote hosts of its
    /// level. `None` when a hosted block took the line; otherwise the content
    /// left after the line's own markers.
    fn replay_hosted<'t>(
        &mut self,
        pos: ReplayAt<'_, '_>,
        text: &'t str,
        lazy_paragraph: bool,
    ) -> Option<&'t str> {
        let column = indent_columns(text);
        let here = &mut self.chain[pos.level];
        let sibling = marker_ends_replayed_item(&here.hosts, column);
        if !lazy_paragraph {
            while here.hosts.last().is_some_and(|&(_, col, _)| col > column) {
                if let Some((_, column, _)) = here.hosts.pop() {
                    here.term_columns.remove(&column);
                }
            }
        }
        let (inner, col, block_start) = here.open_hosts(text, column, sibling);
        if block_start && is_line_comment_any_column(inner) {
            here.para = false;
            here.colon_absorption = false;
            here.by_colon = true;
        }
        if block_start {
            here.colon_absorption = false;
        }
        let floor = here.hosts.last().map_or(0, |&(_, col, _)| col);
        let line = HostedLine {
            inner,
            col,
            floor,
            block_start,
        };
        if here.opens_hosted_block(pos, line) || here.ends_on_leaf_block(line) {
            return None;
        }
        if let Some(open) = detect_fence_open(inner).filter(|_| floor > 0) {
            self.replay_hosted_fence(pos, line, open);
            return None;
        }
        if block_start && is_line_comment_any_column(inner) {
            let here = &mut self.chain[pos.level];
            here.para = false;
            here.colon_absorption = false;
            here.by_colon = true;
            return None;
        }
        Some(inner)
    }

    fn replay_hosted_fence(
        &mut self,
        pos: ReplayAt<'_, '_>,
        line: HostedLine<'_>,
        open: FenceOpen,
    ) {
        let here = &mut self.chain[pos.level];
        // A below-column code run continues an open item paragraph as text.
        if here.para && line.col < line.floor {
            return;
        }
        // The host holds the fence whether the opener sits AT its
        // content column or past it. A fence after an open paragraph
        // still needs a closer ahead (section 10). Inside a colon
        // paragraph it must reach the opener's authored column.
        if line.block_start
            || !here.para
            || indexed_code_closes(
                pos.lines,
                pos.at,
                pos.level,
                line.col,
                here.colons.is_empty().then_some(line.floor),
                open,
                &mut self.literal_code_lookup,
            )
        {
            self.fence_count += 1;
            here.fence_ordinal = self.fence_count;
            here.host_fence = Some((open, line.floor, line.col, false));
            here.para = false;
            here.colon_absorption = false;
            here.by_fence = true;
            return;
        }
        here.para = true;
    }

    fn replay_unhosted(
        &mut self,
        pos: ReplayAt<'_, '_>,
        text: &str,
        after_comment: bool,
        absorption_text: &str,
    ) {
        let here = &mut self.chain[pos.level];
        if let Some(open) = detect_fence_open(text) {
            // At block start a fence opens with or without a closer; after
            // a paragraph it needs one (§10), or it is inline verbatim.
            if !here.para
                || indexed_code_closes(
                    pos.lines,
                    pos.at,
                    pos.level,
                    0,
                    None,
                    open,
                    &mut self.literal_code_lookup,
                )
            {
                self.fence_count += 1;
                here.fence_ordinal = self.fence_count;
                here.fence = Some(open);
                here.para = false;
                here.colon_absorption = false;
                here.by_fence = true;
                return;
            }
            here.para = true;
            return;
        }
        here.replay_text(pos, text, after_comment, absorption_text);
    }
}

/// The line a [`NestedFenceReplay`] is replaying, and the quote level reached.
#[derive(Clone, Copy)]
struct ReplayAt<'a, 'c> {
    lines: &'a [SourceLine],
    at: usize,
    level: usize,
    cur: &'a LineCursor<'c>,
}

/// A replayed line past the list and footnote markers that open hosts on it.
#[derive(Clone, Copy)]
struct HostedLine<'t> {
    inner: &'t str,
    col: usize,
    floor: usize,
    block_start: bool,
}

impl NestedLevel {
    fn replay_colon_comment(&mut self, pos: ReplayAt<'_, '_>, text: &str) {
        if let Some(open) = detect_comment_fence_line_any_column(text) {
            let column = indent_columns(text);
            if closer_ahead(pos.lines, pos.at, pos.level, |line| {
                is_comment_fence_close_any_column(line, open.fence_len)
            }) {
                self.host_comment = Some((open.fence_len, column, false));
            }
        }
        self.by_comment = false;
        self.para = false;
        self.colon_absorption = false;
    }

    fn closes_colon(&mut self, text: &str) -> bool {
        let column = indent_columns(text);
        let Some(width) = exact_colon_fence_len(trim_ascii_start(text)) else {
            return false;
        };
        let Some(index) = self.colons.iter().rposition(|&(seen, floor, authored)| {
            seen == width && (floor == column || authored == column)
        }) else {
            return false;
        };
        self.colons.truncate(index);
        self.para = false;
        self.colon_absorption = false;
        self.by_colon = true;
        true
    }

    /// Pushes a host for each list or footnote marker that opens on the line.
    /// Returns the content after them, its column, and whether any opened.
    fn open_hosts<'t>(
        &mut self,
        text: &'t str,
        column: usize,
        sibling: bool,
    ) -> (&'t str, usize, bool) {
        let inside_item = self.hosts.last().is_some_and(|&(_, _, item)| item);
        let mut inner = trim_ascii_start(text);
        let mut col = column;
        let mut block_start = false;
        loop {
            if let Some(marker) = detect_list_marker_full(inner) {
                let below_host = self
                    .hosts
                    .last()
                    .is_some_and(|&(_, content, _)| col < content);
                if self.para && (!inside_item || below_host) && !sibling {
                    break;
                }
                let next = col + marker_content_col(inner).expect("matched list marker");
                self.hosts.push((col, next, true));
                if is_definition_list_start(trim_ascii_start(marker.content)) {
                    self.term_columns.insert(next);
                }
                col = next + indent_columns(marker.content);
                inner = trim_ascii_start(marker.content);
                block_start = true;
                continue;
            }
            if parse_footnote_def_line(inner).is_some() {
                self.hosts.push((col, col + 2, false));
            }
            break;
        }
        (inner, col, block_start)
    }

    /// Opens a verse, comment or colon block the line's host holds.
    fn opens_hosted_block(&mut self, pos: ReplayAt<'_, '_>, line: HostedLine<'_>) -> bool {
        let HostedLine {
            inner,
            col,
            floor,
            block_start,
        } = line;
        if floor == 0 && col == 0 {
            if let Some(width) = detect_line_block_open(inner) {
                self.host_verse = Some((width, 0, col));
                self.para = false;
                self.colon_absorption = false;
                self.by_colon = true;
                return true;
            }
        }
        if !(floor > 0
            && col >= floor
            && (col == floor || exact_colon_fence_len(inner).is_none() || self.colons.is_empty()))
        {
            return false;
        }
        if let Some(comment) = detect_comment_fence_line(inner) {
            let len = comment.fence_len;
            let collected = closer_ahead(pos.lines, pos.at, pos.level, |line| {
                is_comment_fence_close(trim_ascii_start(line), len)
            });
            if collected || future_quoted_comment_closes(pos.cur, pos.level + 1, col, len) {
                self.host_comment = Some((len, col, !collected));
                self.para = false;
                self.colon_absorption = false;
                return true;
            }
        }
        if detect_line_block_open(inner).is_some()
            && self
                .colons
                .last()
                .is_some_and(|&(_, base, authored)| floor == base && col > authored)
        {
            self.para = true;
            self.colon_absorption = true;
            return true;
        }
        if let Some(width) = detect_line_block_open(inner) {
            self.host_verse = Some((width, floor, col));
            self.para = false;
            self.colon_absorption = false;
            self.by_colon = !block_start;
            return true;
        }
        if let Some(width) = detect_container_open(inner)
            .filter(|_| !self.colon_absorption || exact_colon_fence_len(inner).is_none())
            .map(|open| open.fence_len)
            .or_else(|| detect_hardbreaks_block_open(inner))
            .or_else(|| detect_quote_block_open(inner))
        {
            self.colons.push((width, floor, col));
            self.by_colon = !block_start;
            self.para = false;
            self.colon_absorption = false;
            return true;
        }
        false
    }

    /// Closes the paragraph on a one-line leaf block inside the line's host.
    fn ends_on_leaf_block(&mut self, line: HostedLine<'_>) -> bool {
        let HostedLine {
            inner,
            col,
            floor,
            block_start,
        } = line;
        let mut leaf = inner;
        if block_start {
            loop {
                if detect_thematic_break(leaf) {
                    break;
                }
                if let Some(rest) = strip_blockquote_prefix(leaf) {
                    leaf = trim_ascii_start(rest);
                } else if let Some(marker) = detect_list_marker_full(leaf) {
                    leaf = trim_ascii_start(marker.content);
                } else {
                    break;
                }
            }
        }
        let row = is_table_start(leaf);
        if !block_start && col == floor {
            self.term_columns.remove(&floor);
        }
        if floor > 0
            && col >= floor
            && !self.term_columns.contains(&floor)
            && (row
                || detect_heading(leaf).is_some()
                || detect_thematic_break(leaf)
                || (!block_start && detect_prepass_def_term(inner).is_some())
                || standalone_attrs_block_len(&[leaf]) == Some(1)
                || parse_link_def_line(leaf).is_some()
                || (block_start && is_line_comment_any_column(leaf)))
        {
            self.para = false;
            self.colon_absorption = false;
            self.by_colon = true;
            return true;
        }
        false
    }

    /// Replays a line no fence or host took: comments, colon fences, and
    /// finally the one-line paragraph reading.
    fn replay_text(
        &mut self,
        pos: ReplayAt<'_, '_>,
        text: &str,
        after_comment: bool,
        absorption_text: &str,
    ) {
        if let Some(comment) = detect_comment_fence_line(text) {
            let len = comment.fence_len;
            if closer_ahead(pos.lines, pos.at, pos.level, |l| {
                is_comment_fence_close(l, len)
            }) {
                self.comment = Some(len);
            }
        }
        if exact_colon_fence_len(trim_ascii_start(text)).is_some() && !self.colons.is_empty() {
            self.para = true;
            self.colon_absorption = false;
            return;
        }
        if after_comment
            && !self.para
            && !self.colon_absorption
            && self.colons.is_empty()
            && exact_colon_fence_len(trim_ascii_start(text)).is_some()
        {
            self.by_colon = true;
            return;
        }
        if is_line_comment_any_column(text) {
            self.by_comment = self
                .hosts
                .last()
                .is_some_and(|&(_, column, _)| indent_columns(text) < column);
        }
        if self.para
            && indent_columns(text) > 0
            && exact_colon_fence_len(trim_ascii_start(text)).is_some()
        {
            return;
        }
        let mut paragraph = ParaOpen::from_line(text, self.colon_absorption);
        self.para = paragraph.get();
        self.colon_absorption = self.para
            && (self.colon_absorption
                || is_invalid_colon_fence_opener_text(trim_ascii_start(absorption_text)));
    }
}

pub(in crate::parse) fn marker_ends_replayed_item(
    hosts: &[(usize, usize, bool)],
    column: usize,
) -> bool {
    hosts.iter().enumerate().any(|(index, &(marker, _, item))| {
        item && column <= marker && (index == 0 || column >= hosts[index - 1].1)
    })
}

pub(in crate::parse) struct FutureQuotedComments {
    closers: ContainerCommentClosers,
    blanks: Vec<usize>,
    dedents: HashMap<(usize, usize), (usize, usize)>,
}

pub(in crate::parse) fn future_quoted_comment_closes(
    cur: &LineCursor<'_>,
    depth: usize,
    column: usize,
    width: usize,
) -> bool {
    let mut cached = cur.future_quoted_comments.borrow_mut();
    let indexed = cached.get_or_insert_with(|| {
        let source: Vec<_> = cur.lines.range(0..cur.lines.len()).collect();
        let blanks = source
            .iter()
            .enumerate()
            .filter_map(|(at, line)| is_blank_line(line).then_some(at))
            .collect();
        FutureQuotedComments {
            closers: ContainerCommentClosers::build(&source),
            blanks,
            dedents: HashMap::new(),
        }
    });
    let before = cur.pos.saturating_sub(1);
    let blank = indexed
        .blanks
        .get(indexed.blanks.partition_point(|at| *at < cur.pos))
        .copied()
        .unwrap_or(cur.lines.len());
    let key = (depth, column);
    let dedent = match indexed.dedents.get(&key).copied() {
        Some((from, at)) if from <= cur.pos && cur.pos < at => at,
        _ => {
            let mut boundary = cur.lines.len();
            for (offset, line) in cur.lines.range(cur.pos..cur.lines.len()).enumerate() {
                let mut text = line;
                let mut quoted = true;
                for _ in 0..depth {
                    if let Some(rest) = strip_blockquote_prefix(text) {
                        text = rest;
                    } else {
                        quoted = false;
                        break;
                    }
                }
                if quoted && !is_blank_line(text) && indent_columns(text) < column {
                    boundary = cur.pos + offset;
                    break;
                }
            }
            indexed.dedents.insert(key, (cur.pos, boundary));
            boundary
        }
    };
    indexed
        .closers
        .next_closer(before, width, depth)
        .is_some_and(|at| {
            if at >= blank || at >= dedent {
                return false;
            }
            let mut text = cur.lines.at(at);
            for _ in 0..depth {
                let Some(rest) = strip_blockquote_prefix(text) else {
                    return false;
                };
                text = rest;
            }
            indent_columns(text) >= column
        })
}

#[derive(Default)]
pub(in crate::parse) struct QuotedBodyComments {
    scanned: usize,
    scope_start: Option<usize>,
    scope_end: Option<usize>,
    closers: HashMap<usize, Vec<usize>>,
    boundaries: Vec<usize>,
}

impl QuotedBodyComments {
    pub(in crate::parse) fn closes_after(
        &mut self,
        lines: &[SourceLine],
        from: usize,
        level: usize,
        width: usize,
    ) -> bool {
        if self.scope_start.is_some_and(|start| from < start)
            || self.scope_end.is_some_and(|end| from >= end)
        {
            self.closers.clear();
            self.boundaries.clear();
            self.scope_end = None;
            self.scanned = from + 1;
            self.scope_start = Some(from);
        }
        if self.scope_start.is_none() {
            self.scope_start = Some(from);
            self.scanned = from + 1;
        }
        let scan_end = self.scope_end.unwrap_or(lines.len());
        for (at, line) in lines.iter().enumerate().take(scan_end).skip(self.scanned) {
            #[cfg(test)]
            COMMENT_INDEX_LINES.with(|work| work.set(work.get() + 1));
            let mut text = line.text.as_str();
            let mut boundary = text.starts_with(LAZY);
            for _ in 0..level {
                if let Some(rest) = strip_blockquote_prefix(text) {
                    text = rest;
                } else {
                    boundary = true;
                    break;
                }
            }
            if boundary {
                self.boundaries.push(at);
                self.scope_end = Some(at);
                break;
            } else if let Some(open) = detect_comment_fence_line(trim_ascii_start(text)) {
                if is_comment_fence_close_any_column(text, open.fence_len) {
                    self.closers.entry(open.fence_len).or_default().push(at);
                }
            }
        }
        self.scanned = self.scope_end.map_or(lines.len(), |end| end + 1);
        let Some(closers) = self.closers.get(&width) else {
            return false;
        };
        let Some(&close) = closers.get(closers.partition_point(|&at| at <= from)) else {
            return false;
        };
        let boundary = self.boundaries.partition_point(|&at| at <= from);
        self.boundaries
            .get(boundary)
            .map_or(true, |&end| close < end)
    }
}

#[derive(Default)]
pub(in crate::parse) struct LiteralCodeLookup {
    indices: HashMap<usize, LiteralCodeIndex>,
}

#[derive(Default)]
pub(in crate::parse) struct LiteralCodeIndex {
    scanned: usize,
    boundaries: Vec<usize>,
    scope_end: Option<usize>,
    columns: HashMap<(u8, usize), LiteralCodeColumn>,
    payloads: HashMap<u8, LiteralCodeColumn>,
    departures: LiteralCodeColumn,
}

// Store each run once; the maximum tree finds a sufficiently wide later closer.
#[derive(Default)]
pub(in crate::parse) struct LiteralCodeColumn {
    pub(in crate::parse) positions: Vec<usize>,
    pub(in crate::parse) maxima: Vec<usize>,
    capacity: usize,
}

impl LiteralCodeColumn {
    pub(in crate::parse) fn push(&mut self, position: usize, width: usize) {
        if self.positions.len() == self.capacity {
            let old_capacity = self.capacity;
            self.capacity = old_capacity.max(1) * 2;
            let mut maxima = vec![0; self.capacity * 2];
            maxima[self.capacity..self.capacity + self.positions.len()]
                .copy_from_slice(&self.maxima[old_capacity..old_capacity + self.positions.len()]);
            for index in (1..self.capacity).rev() {
                maxima[index] = maxima[index * 2].max(maxima[index * 2 + 1]);
            }
            self.maxima = maxima;
        }
        let mut index = self.capacity + self.positions.len();
        self.positions.push(position);
        self.maxima[index] = width;
        while index > 1 {
            index /= 2;
            self.maxima[index] = self.maxima[index * 2].max(self.maxima[index * 2 + 1]);
        }
    }

    pub(in crate::parse) fn next(&self, from: usize, width: usize) -> Option<usize> {
        let start = self.positions.partition_point(|&position| position <= from);
        self.find(1, 0, self.capacity, start, width)
            .and_then(|index| self.positions.get(index).copied())
    }

    fn find(
        &self,
        node: usize,
        left: usize,
        right: usize,
        start: usize,
        width: usize,
    ) -> Option<usize> {
        if right <= start || self.maxima.get(node).copied().unwrap_or(0) < width {
            return None;
        }
        if right - left == 1 {
            return Some(left);
        }
        let middle = left + (right - left) / 2;
        self.find(node * 2, left, middle, start, width)
            .or_else(|| self.find(node * 2 + 1, middle, right, start, width))
    }
}

impl LiteralCodeIndex {
    fn refresh(&mut self, lines: &[SourceLine], level: usize, from: usize) {
        self.refresh_text(lines.len(), level, from, |at| lines[at].text.as_str());
    }

    fn refresh_cursor(&mut self, lines: CursorLines<'_>, from: usize) {
        self.refresh_text(lines.len(), 0, from, |at| lines.at(at));
    }

    fn refresh_text<'a>(
        &mut self,
        length: usize,
        level: usize,
        from: usize,
        line_at: impl Fn(usize) -> &'a str,
    ) {
        if let Some(end) = self.scope_end {
            if from < end {
                return;
            }
            self.columns.clear();
            self.payloads.clear();
            self.departures = LiteralCodeColumn::default();
            self.boundaries.clear();
            self.scope_end = None;
        }
        self.scanned = self.scanned.max(from + 1);
        for index in self.scanned..length {
            let mut text = line_at(index);
            #[cfg(test)]
            LITERAL_CODE_LINES.with(|work| work.set(work.get() + 1));
            let mut boundary = text.starts_with(LAZY);
            for _ in 0..level {
                let Some(rest) = strip_blockquote_prefix(text) else {
                    boundary = true;
                    break;
                };
                text = rest;
            }
            if boundary {
                self.boundaries.push(index);
                self.scope_end = Some(index);
                self.scanned = index + 1;
                return;
            }
            let column = indent_columns(text);
            if !is_blank_line(text) {
                self.departures.push(index, usize::MAX - column);
            }
            let bytes = trim_ascii_start(text).as_bytes();
            let Some(&character @ (b'`' | b'~')) = bytes.first() else {
                continue;
            };
            let width = bytes.iter().take_while(|&&byte| byte == character).count();
            if !bytes[width..]
                .iter()
                .all(|&byte| byte == b' ' || byte == b'\t')
            {
                continue;
            }
            self.columns
                .entry((character, column))
                .or_default()
                .push(index, width);
            self.payloads
                .entry(character)
                .or_default()
                .push(index, width);
        }
        self.scanned = length;
    }

    fn payload_closer(&self, from: usize, open: FenceOpen) -> Option<usize> {
        self.payloads
            .get(&open.fence_char)?
            .next(from, open.fence_len)
    }

    fn next(&self, from: usize, column: usize, open: FenceOpen) -> Option<usize> {
        self.columns
            .get(&(open.fence_char, column))?
            .next(from, open.fence_len)
    }
}

pub(in crate::parse) fn indexed_code_closes(
    lines: &[SourceLine],
    from: usize,
    level: usize,
    column: usize,
    alternate_column: Option<usize>,
    open: FenceOpen,
    lookup: &mut LiteralCodeLookup,
) -> bool {
    let index = lookup.indices.entry(level).or_default();
    index.refresh(lines, level, from);
    let closer = index
        .next(from, column, open)
        .into_iter()
        .chain(alternate_column.and_then(|column| index.next(from, column, open)))
        .min();
    closer.is_some_and(|close| {
        index
            .boundaries
            .get(index.boundaries.partition_point(|&at| at <= from))
            .map_or(true, |&end| close < end)
    })
}

pub(in crate::parse) fn literal_code_closes_cursor(
    cur: &mut LineCursor<'_>,
    floor: usize,
    _column: usize,
    open: FenceOpen,
) -> bool {
    let from = cur.pos;
    let index = cur.literal_code.indices.entry(0).or_default();
    index.refresh_cursor(cur.lines, from);
    let closer = index.payload_closer(from, open);
    let Some(closer) = closer else {
        return false;
    };
    if index
        .boundaries
        .get(index.boundaries.partition_point(|&at| at <= from))
        .is_some_and(|&at| at <= closer)
    {
        return false;
    }
    floor == 0
        || index
            .departures
            .next(from, usize::MAX - floor.saturating_sub(1))
            .map_or(true, |end| closer < end)
}

pub(in crate::parse) fn literal_code_closes(
    lines: &[SourceLine],
    from: usize,
    level: usize,
    floor: usize,
    _column: usize,
    open: FenceOpen,
    lookup: &mut LiteralCodeLookup,
) -> bool {
    let index = lookup.indices.entry(level).or_default();
    index.refresh(lines, level, from);
    let Some(closer) = index.payload_closer(from, open) else {
        return false;
    };
    if index
        .boundaries
        .get(index.boundaries.partition_point(|&at| at <= from))
        .is_some_and(|&at| at <= closer)
    {
        return false;
    }
    floor == 0
        || index
            .departures
            .next(from, usize::MAX - floor.saturating_sub(1))
            .map_or(true, |end| closer < end)
}

#[cfg(test)]
pub(in crate::parse) fn quoted_colon_body_follows(
    lines: &[SourceLine],
    from: usize,
    level: usize,
    floor: usize,
    comments: &mut QuotedBodyComments,
) -> bool {
    let mut comment = None;
    for (at, line) in lines.iter().enumerate().skip(from + 1) {
        let mut text = line.text.as_str();
        if text.starts_with(LAZY) {
            return false;
        }
        for _ in 0..level {
            let Some(rest) = strip_blockquote_prefix(text) else {
                return false;
            };
            text = rest;
        }
        if let Some(width) = comment {
            if is_comment_fence_close_any_column(text, width) {
                comment = None;
            }
            continue;
        }
        if let Some(open) = detect_comment_fence_line(text) {
            if comments.closes_after(lines, at, level, open.fence_len) {
                comment = Some(open.fence_len);
                continue;
            }
        }
        if is_blank_line(text) || trim_ascii_start(text).starts_with("%%") {
            continue;
        }
        return indent_columns(text) >= floor;
    }
    false
}

/// Does a line matching `closes` follow line `from` at nesting `level`, before
/// the quote at that level ends?
pub(in crate::parse) fn closer_ahead(
    lines: &[SourceLine],
    from: usize,
    level: usize,
    closes: impl Fn(&str) -> bool,
) -> bool {
    for line in &lines[from + 1..] {
        let mut text = line.text.as_str();
        if text.starts_with(LAZY) {
            return false;
        }
        for _ in 0..level {
            match strip_blockquote_prefix(text) {
                Some(rest) => text = rest,
                None => return false,
            }
        }
        if closes(text) {
            return true;
        }
    }
    false
}

#[derive(Default)]
pub(in crate::parse) struct QuotedFenceClosers {
    end: usize,
    backticks: Vec<(usize, usize)>,
    tildes: Vec<(usize, usize)>,
}

impl QuotedFenceClosers {
    fn contains(&mut self, lines: CursorLines<'_>, from: usize, open: FenceOpen) -> bool {
        if from >= self.end {
            self.backticks.clear();
            self.tildes.clear();
            self.end = from;
            for (offset, line) in lines.range(from..lines.len()).enumerate() {
                let index = from + offset;
                let Some(body) = strip_blockquote_prefix(line) else {
                    break;
                };
                self.end = index + 1;
                let trimmed = trim_ascii_end(body);
                let Some(marker @ (b'`' | b'~')) = trimmed.as_bytes().first().copied() else {
                    continue;
                };
                if trimmed.len() >= 3 && trimmed.bytes().all(|byte| byte == marker) {
                    let entries = if marker == b'`' {
                        &mut self.backticks
                    } else {
                        &mut self.tildes
                    };
                    entries.push((index, trimmed.len()));
                }
            }
            for entries in [&mut self.backticks, &mut self.tildes] {
                let mut maximum = 0;
                for (_, width) in entries.iter_mut().rev() {
                    maximum = maximum.max(*width);
                    *width = maximum;
                }
            }
        }
        let entries = if open.fence_char == b'`' {
            &self.backticks
        } else {
            &self.tildes
        };
        let index = entries.partition_point(|(line, _)| *line < from);
        entries
            .get(index)
            .is_some_and(|(_, width)| *width >= open.fence_len)
    }
}

#[inline(never)]
pub(in crate::parse) fn collect_blockquote_body(
    cur: &mut LineCursor,
    options: &Options<'_>,
) -> (usize, Vec<LineBuffer>, bool) {
    let mut body = QuoteBody::new(cur.pos);
    while let Some(line) = cur.peek() {
        let quote_line = if cur.colon_view.is_some_and(|view| view.marker_hosted) {
            trim_ascii_start(line)
        } else {
            line
        };
        if let Some(stripped) = strip_blockquote_prefix(quote_line) {
            body.quoted_line(cur, options, line, stripped);
            continue;
        }
        if body.attach_after_quoted_plus(cur, options)
            || body.attach_physical_plus(cur, options, line)
            || body.pull_hosted_block(cur, options, line)
        {
            continue;
        }
        if body.lazy_ends_quote(cur, line) {
            break;
        }
        body.fold_lazy_line(cur, line);
    }
    body.finish()
}

/// Strips every quote prefix from `text`, returning the rest and how many.
fn strip_all_quotes(text: &str) -> (&str, usize) {
    let mut rest = text;
    let mut levels = 0;
    while let Some(next) = strip_blockquote_prefix(rest) {
        rest = next;
        levels += 1;
    }
    (rest, levels)
}

/// Whether the quoted line at `prior` ended a nested quote on an accepted `+`.
fn nested_plus_accepted(cur: &LineCursor<'_>, prior: usize) -> bool {
    let (previous, levels) =
        strip_all_quotes(strip_blockquote_prefix(cur.lines.at(prior)).unwrap_or(""));
    levels > 0
        && is_plus_marker(innermost_marker_content(previous))
        && cur
            .continuation_decision(prior)
            .is_some_and(|decision| decision.refusal.is_none())
}

/// A quoted line [`collect_blockquote_body`] has consumed.
#[derive(Clone, Copy)]
struct QuotedLine<'a> {
    line: &'a str,
    stripped: &'a str,
    at: usize,
    source_line: Option<usize>,
    prior: Option<usize>,
}

/// What [`collect_blockquote_body`] carries from one line of the quote to the
/// next.
struct QuoteBody<'a> {
    fence_closers: QuotedFenceClosers,
    parts: Vec<LineBuffer>,
    span_start: usize,
    inner: LineBuffer,
    para_open: ParaOpen<'a>,
    comment_run: Option<(usize, usize, usize)>,
    comment_lazy_lines: Vec<(usize, String, Option<isize>)>,
    in_fence: Option<FenceOpen>,
    opaque_tail: bool,
    fence_count: usize,
    // What the quote's own lines have left open, for the lazy branch below.
    // ADVANCED ON DEMAND, never per line: `strip_blockquote_prefix` is counted
    // and a depth ladder must not pay per level per line
    // (`quote_prefix_calls`). Only a `:`-shaped lazy line asks, and the scan
    // resumes where it stopped, so the whole walk is one pass over the lines.
    open_entry: QuotedEntryScan,
    may_have_term: bool,
    // THE QUOTE'S OWN LAST BLOCK IS WHAT S4 ASKS ABOUT, and a continuation row
    // belongs to the table above it (§5 T6, markup-carve/carve#1348). `ParaOpen`
    // is handed ONE line and a `+ b |` line reads as prose on its own, so a
    // quote ending on a continuation row recorded an open paragraph and the
    // flush-left line below folded in - while the SAME table ending on a
    // standard row closed the quote. One question answered two ways by a
    // spelling. This carries the run so the two spellings answer alike.
    table: TableRun,
    // ONE BLOCK IS ONE BLOCK HOWEVER MANY LINES IT TAKES (§15 A5). A wrapped
    // attribute block (`{.k` / `#x}`) closes the quoted paragraph exactly as the
    // single-line `{.k}` does, and the lines after its opener are its own
    // content rather than paragraph text. `ParaOpen` decides from ONE line, so
    // the block's extent is tracked here beside the code fence's - the same
    // shape, for the same reason (markup-carve/carve-rs#1050).
    attrs_block_rest: usize,
    attrs_nested_run: bool,
    attrs_closed_last: bool,
    unclosed_nested_attribute: bool,
    // The first line index the block lookahead is still worth running on. A scan
    // that walked to a blank line, or to the end of the quoted run, without
    // meeting a line that could CLOSE an attribute block has proved the same for
    // every line it passed - see `quoted_attrs_block_len`.
    attrs_scan_floor: usize,
    // The last quoted line reached a nested quote, so `para_open` may have read a
    // fence one level down as prose; see `NestedFenceReplay`.
    last_nested: bool,
    nested_verdict: Option<Option<bool>>,
    nested_replay: NestedFenceReplay,
    // No replay without a fence to find: every level of a quote ladder would
    // otherwise walk its whole body once.
    may_hold_fence: bool,
    // The last quoted line was read on a description body's base.
    description_line: bool,
    literal_quote_body: Option<usize>,
    last_quoted_at: Option<usize>,
    // The quoted line that opened a list item, not text in a paragraph or a fence.
    item_opened_at: Option<usize>,
    // The quoted line that opened a block at any depth, not text in a paragraph or a fence.
    block_opened_at: Option<usize>,
    list_open: bool,
    list_content_columns: Vec<usize>,
}

impl<'a> QuoteBody<'a> {
    fn new(span_start: usize) -> Self {
        Self {
            fence_closers: QuotedFenceClosers::default(),
            parts: Vec::new(),
            span_start,
            inner: LineBuffer::default(),
            para_open: ParaOpen::Closed,
            comment_run: None,
            comment_lazy_lines: Vec::new(),
            in_fence: None,
            opaque_tail: false,
            fence_count: 0,
            open_entry: QuotedEntryScan::default(),
            may_have_term: false,
            table: TableRun::default(),
            attrs_block_rest: 0,
            attrs_nested_run: false,
            attrs_closed_last: false,
            unclosed_nested_attribute: false,
            attrs_scan_floor: 0,
            last_nested: false,
            nested_verdict: None,
            nested_replay: NestedFenceReplay::default(),
            may_hold_fence: false,
            description_line: false,
            literal_quote_body: None,
            last_quoted_at: None,
            item_opened_at: None,
            block_opened_at: None,
            list_open: false,
            list_content_columns: Vec::new(),
        }
    }

    fn finish(mut self) -> (usize, Vec<LineBuffer>, bool) {
        let opaque_tail = if self.nested_verdict.is_some() {
            self.nested_replay.marker_fence_tail()
        } else {
            self.opaque_tail
        };
        self.parts.push(self.inner);
        (self.span_start, self.parts, opaque_tail)
    }

    fn push_carried(
        &mut self,
        cur: &LineCursor<'_>,
        at: usize,
        text: String,
        source_line: Option<usize>,
        column: Option<isize>,
    ) {
        if cur.carried_reach(at) == Some(true) {
            self.inner.push_reached_at(text, source_line, column);
        } else {
            self.inner.push_at(text, source_line, column);
        }
    }

    fn quoted_line(
        &mut self,
        cur: &mut LineCursor<'a>,
        options: &Options<'_>,
        line: &'a str,
        stripped: &'a str,
    ) {
        self.attrs_closed_last = false;
        self.unclosed_nested_attribute = false;
        self.note_openers(cur.pos, stripped);
        let source_line = cur.source_line(cur.pos);
        let at = cur.pos;
        cur.consume();
        // Keep the prior quoted line by index. Only a line ending in `+`
        // asks for nested depth, so a deep quote does not rescan its chain
        // on every ordinary line.
        let prior = self.last_quoted_at.replace(at);
        let quoted = QuotedLine {
            line,
            stripped,
            at,
            source_line,
            prior,
        };
        if strip_blockquote_prefix(stripped).is_none()
            && prior.is_some_and(|prior| nested_plus_accepted(cur, prior))
        {
            self.inner.push_synthetic_blank();
        }
        if self.skips_carried_plus(cur, quoted)
            || self.hands_plus_to_nested(cur, quoted)
            || self.attaches_first_block(cur, options, quoted)
        {
            return;
        }
        self.read_quoted_line(cur, options, quoted);
    }

    /// Records which list, block and comment openers the quoted line carries,
    /// before it is consumed.
    fn note_openers(&mut self, at: usize, stripped: &str) {
        let marker = detect_list_marker_full(trim_ascii_start(stripped)).is_some();
        let column = indent_columns(stripped);
        if !is_blank_line(stripped)
            && !marker
            && !trim_ascii_start(stripped).starts_with('{')
            && self.in_fence.is_none()
        {
            while self
                .list_content_columns
                .last()
                .is_some_and(|owner| column < *owner)
            {
                self.list_content_columns.pop();
            }
        }
        // Resolved only for a list or quote line ending in `+`, so plain quoted prose
        // pays no paragraph resolution here.
        let plus_line = trim_ascii_end(stripped).ends_with('+')
            && (marker || stripped.starts_with('>'))
            && self.in_fence.is_none();
        let opens = (marker || plus_line)
            && self.in_fence.is_none()
            && (!self.para_open.get() || self.list_open);
        self.block_opened_at = (opens && plus_line).then_some(at);
        self.item_opened_at = (opens && marker).then_some(at);
        // A sibling item may follow its list's lines, whose lead leaves a paragraph open,
        // until a block or paragraph at the quote's column ends the list.
        if self.item_opened_at.is_some() {
            self.list_open = true;
            while self
                .list_content_columns
                .last()
                .is_some_and(|owner| column < *owner)
            {
                self.list_content_columns.pop();
            }
            self.list_content_columns
                .push(innermost_marker_content_col(stripped));
        } else if self.in_fence.is_none()
            && !is_blank_line(stripped)
            && !stripped.starts_with([' ', '\t'])
            && (!self.list_open || !self.para_open.get())
        {
            self.list_open = false;
            self.list_content_columns.clear();
        }
        if !self.may_have_term && stripped.contains(":: ") {
            let trimmed = trim_ascii_start(stripped);
            self.may_have_term = (self.inner.lines.is_empty()
                || detect_list_marker_full(trimmed).is_some())
                && (trimmed.starts_with('>')
                    || innermost_marker_content(trimmed).starts_with(":: "));
        }
        if stripped.contains("%%%") {
            self.note_comment_run(stripped);
        }
    }

    fn note_comment_run(&mut self, stripped: &str) {
        let (comment_depth, _, comment_text) = prepass_quote_scope(stripped);
        if let Some((run, depth, _)) = self.comment_run {
            if comment_depth == depth
                && detect_list_marker_full(trim_ascii_start(stripped)).is_none()
                && is_comment_fence_close_any_column(comment_text, run)
            {
                for (index, text, column) in self.comment_lazy_lines.drain(..) {
                    self.inner.lines[index].text = text;
                    self.inner.lines[index].stripped = column;
                }
                self.comment_run = None;
            }
        } else if self.in_fence.is_none() {
            self.comment_run = detect_comment_fence_line_any_column(comment_text)
                .map(|open| (open.fence_len, comment_depth, self.inner.lines.len()));
        }
    }

    fn skips_carried_plus(&self, cur: &LineCursor<'_>, quoted: QuotedLine<'_>) -> bool {
        cur.carried_reach(quoted.at) == Some(true)
            && is_plus_marker(quoted.stripped)
            && cur
                .continuation_decision(quoted.at)
                .is_some_and(|decision| {
                    decision.refusal.is_none()
                        && decision.owner != Some(continuation_marker::Kind::Quote)
                })
            && self
                .inner
                .lines
                .last()
                .is_some_and(|last| strip_blockquote_prefix(&last.text).is_some())
    }

    // Pass a marker at this quote's content column to the nested
    // quote's collector. It owns the extent of the attached block.
    fn hands_plus_to_nested(&mut self, cur: &mut LineCursor<'a>, quoted: QuotedLine<'_>) -> bool {
        let at = quoted.at;
        if !(cur.carried_reach(at) != Some(false)
            && is_plus_marker(quoted.stripped)
            && cur.continuation_decision(at).is_some_and(|decision| {
                decision.owner == Some(continuation_marker::Kind::Quote)
                    && decision.refusal.is_none()
            })
            && quoted.prior.is_some()
            && self.inner.lines.last().is_some_and(|last| {
                strip_blockquote_prefix(&last.text).is_some()
                    || is_definition_placeholder(&last.text)
            }))
        {
            return false;
        }
        if cur
            .peek()
            .and_then(strip_blockquote_prefix)
            .is_some_and(|next| !is_blank_line(next))
            && self
                .inner
                .lines
                .last()
                .is_some_and(|last| is_definition_placeholder(innermost_marker_content(&last.text)))
        {
            cur.consume();
            return true;
        }
        self.push_carried(cur, at, "+".to_string(), quoted.source_line, Some(0));
        self.para_open = ParaOpen::Closed;
        true
    }

    fn attaches_first_block(
        &mut self,
        cur: &mut LineCursor<'a>,
        options: &Options<'_>,
        quoted: QuotedLine<'_>,
    ) -> bool {
        let first_block = cur
            .continuation_decision(quoted.at)
            .is_some_and(|decision| {
                decision.owner == Some(continuation_marker::Kind::Quote)
                    && decision.refusal.is_none()
            });
        if !first_block {
            return false;
        }
        let (deepest, levels) = strip_all_quotes(quoted.stripped);
        let prior_levels = if levels > 0 {
            quoted.prior.map_or(0, |prior| {
                strip_all_quotes(strip_blockquote_prefix(cur.lines.at(prior)).unwrap_or("")).1
            })
        } else {
            0
        };
        if !(is_plus_marker(deepest)
            // With no line left to take, the `+` stays text (carve#3421).
            && !cur.eof()
            && (levels > prior_levels || (levels == 0 && self.inner.lines.is_empty())))
        {
            return false;
        }
        self.attach_first_block(cur, options, quoted, levels, prior_levels);
        true
    }

    fn attach_first_block(
        &mut self,
        cur: &mut LineCursor<'a>,
        options: &Options<'_>,
        quoted: QuotedLine<'_>,
        levels: usize,
        prior_levels: usize,
    ) {
        // Preserve a nested quote marker for its own parse. The
        // innermost quote consumes the `+` and takes the block.
        let follower_nested_levels = cur.peek().map_or(0, |next| {
            strip_all_quotes(strip_blockquote_prefix(next).unwrap_or(next)).1
        });
        let quoted_follower = cur
            .peek()
            .is_some_and(|next| strip_blockquote_prefix(next).is_some());
        // A follower carrying just this quote's own marker joins the innermost quote
        // behind every level (`> > > > +` / `> b`, carve#3421).
        let own_marker_follower =
            prior_levels == 0 && quoted_follower && follower_nested_levels == 0 && levels > 2;
        let strip_follower_marker = prior_levels > 0 || own_marker_follower;
        let keep_levels = if strip_follower_marker {
            levels
        } else if quoted_follower {
            levels.saturating_sub(follower_nested_levels + 1)
        } else {
            levels
        };
        if levels > 0 && keep_levels > 0 {
            let mut kept = quoted.stripped;
            for _ in keep_levels..levels {
                kept = strip_blockquote_prefix(kept).unwrap_or(kept);
            }
            let column = stripped_col(cur.source_col(quoted.at), quoted.line, kept);
            self.push_carried(cur, quoted.at, kept.to_string(), quoted.source_line, column);
        }
        let end = attached_block_lines_classified(
            cur.lines,
            cur.pos,
            &mut cur.comment_closer_last_index,
            &mut cur.code_closer_last_index,
            options,
            &mut |next, _| is_blank_line(next) || is_plus_marker(next),
        );
        let attached_start = cur.pos;
        while cur.pos < end {
            let next = cur.lines.at(cur.pos);
            let kept = if strip_follower_marker {
                strip_blockquote_prefix(next).unwrap_or(next)
            } else {
                next
            };
            self.inner.push_at(
                kept.to_string(),
                cur.source_line(cur.pos),
                stripped_col(cur.source_col(cur.pos), next, kept),
            );
            cur.pos += 1;
        }
        self.keep_unclosed_fence_line(cur, attached_start, end, strip_follower_marker);
        self.inner.push_synthetic_blank();
        self.para_open = ParaOpen::Closed;
    }

    // An unclosed fence the `+` attached keeps an empty last line, as the spec
    // model and carve-js read it, unless the document ends on its last line
    // (carve#3421).
    fn keep_unclosed_fence_line(
        &mut self,
        cur: &LineCursor<'_>,
        attached_start: usize,
        end: usize,
        strip_follower_marker: bool,
    ) {
        // Lines kept with a nested quote's marker carry it on the empty line too.
        let nested_marker = attached_start < end
            && !strip_follower_marker
            && strip_blockquote_prefix(cur.lines.at(attached_start)).is_some();
        let attached_text = |at: usize| {
            let line = cur.lines.at(at);
            trim_ascii_start(if strip_follower_marker || nested_marker {
                strip_blockquote_prefix(line).unwrap_or(line)
            } else {
                line
            })
        };
        if attached_start < end
            && !crate::parse::is_unterminated_source_line(cur.source_line(end - 1))
            && detect_fence_open(attached_text(attached_start)).is_some_and(|open| {
                !(attached_start + 1..end).any(|at| is_fence_close(attached_text(at), open))
            })
        {
            let empty = if nested_marker { ">" } else { "" };
            self.inner.push_at(empty.to_string(), None, None);
        }
    }

    fn read_quoted_line(
        &mut self,
        cur: &mut LineCursor<'a>,
        options: &Options<'_>,
        quoted: QuotedLine<'a>,
    ) {
        let stripped = quoted.stripped;
        self.nested_verdict = None;
        self.description_line = false;
        let after_nested = self.last_nested && self.may_hold_fence;
        self.last_nested =
            self.in_fence.is_none() && (stripped == ">" || stripped.starts_with("> "));
        self.may_hold_fence = self.may_hold_fence
            || stripped.contains("```")
            || stripped.contains("~~~")
            || stripped.contains(":::")
            || stripped.contains("- ")
            || stripped.contains("* ")
            || stripped.contains(". ")
            || stripped.contains(") ");
        // The quote marker (and its optional space) is a pure prefix, so the
        // quoted line's columns are knowable in the document.
        let stripped_at = stripped_col(cur.source_col(quoted.at), quoted.line, stripped);
        self.track_quoted_block(cur, options, quoted, after_nested);
        let kept_at = if self.unclosed_nested_attribute {
            stripped_at.map(|column| column + leading_ws(stripped) as isize)
        } else {
            stripped_at
        };
        self.push_carried(
            cur,
            quoted.at,
            if self.unclosed_nested_attribute {
                format!("{LAZY}{}", trim_ascii_start(stripped))
            } else {
                stripped.to_string()
            },
            quoted.source_line,
            kept_at,
        );
    }

    /// Advances the fence, verse, attribute and paragraph state over a quoted
    /// line that is kept as quote content.
    fn track_quoted_block(
        &mut self,
        cur: &mut LineCursor<'a>,
        options: &Options<'_>,
        quoted: QuotedLine<'a>,
        after_nested: bool,
    ) {
        let stripped = quoted.stripped;
        if let Some(width) = self.literal_quote_body {
            if exact_colon_fence_len(stripped) == Some(width) {
                self.literal_quote_body = None;
                self.para_open = ParaOpen::Closed;
            } else {
                self.para_open = if self.comment_run.is_some() {
                    ParaOpen::Closed
                } else {
                    ParaOpen::from_line("x", false)
                };
            }
            self.opaque_tail = false;
            self.table = TableRun::default();
        } else if let Some(width) = detect_line_block_open(stripped) {
            self.literal_quote_body = Some(width);
            self.para_open = ParaOpen::from_line("x", false);
            self.opaque_tail = false;
            self.table = TableRun::default();
        } else if self.attrs_block_rest > 0 {
            self.opaque_tail = false;
            // Inside a wrapped attribute block the opener already closed the
            // paragraph; its remaining lines close nothing further and open
            // nothing either.
            self.attrs_block_rest -= 1;
            self.attrs_closed_last = self.attrs_block_rest == 0 && self.attrs_nested_run;
            self.para_open = ParaOpen::Closed;
            self.table = TableRun::default();
        } else if let Some(open) = self.in_fence {
            self.opaque_tail = true;
            if is_fence_close(stripped, open) {
                self.in_fence = None;
                self.opaque_tail = self.fence_count == 1;
            }
            self.para_open = ParaOpen::Closed;
            self.table = TableRun::default();
            // The absorption belongs to ONE paragraph: whatever closed it
            // -- a blank line, a real opener, a code fence -- ends the
            // absorption too. Spelled at each site that closes the
            // paragraph rather than once after the chain, because after the
            // chain it would have to READ `para_open`, and reading it on
            // every quoted line is the walk this defers.
        } else if let Some(open) = detect_fence_open(stripped) {
            self.fence_line(cur, open, after_nested);
        } else {
            self.ordinary_line(cur, options, quoted);
        }
    }

    fn fence_line(&mut self, cur: &LineCursor<'_>, open: FenceOpen, after_nested: bool) {
        self.table = TableRun::default();
        // The line above reached a nested quote, so its fence may be
        // what the paragraph ended on.
        let fenced_below = if after_nested {
            self.nested_replay.verdict(&self.inner.lines, cur)
        } else {
            None
        };
        if !fenced_below.unwrap_or_else(|| self.para_open.get()) {
            // Fence at block start opens (unterminated renders to end).
            self.opaque_tail = true;
            self.fence_count += 1;
            self.in_fence = Some(open);
            self.para_open = ParaOpen::Closed;
        } else {
            // After an open paragraph a fence interrupts only with a
            // matching closer ahead (§10); else it is inline verbatim.
            let has_closer = self.fence_closers.contains(cur.lines, cur.pos, open);
            if has_closer {
                self.opaque_tail = true;
                self.fence_count += 1;
                self.in_fence = Some(open);
                self.para_open = ParaOpen::Closed;
            }
            // No closer: the fence is inline verbatim and the paragraph
            // (already resolved OPEN by the test above) stays open, so
            // the absorption survives - as it did when the reset was
            // written `if !para_open`.
        }
    }

    fn ordinary_line(
        &mut self,
        cur: &mut LineCursor<'a>,
        options: &Options<'_>,
        quoted: QuotedLine<'a>,
    ) {
        let QuotedLine { stripped, at, .. } = quoted;
        self.opaque_tail = false;
        // Whether this line leaves a paragraph open is decided by
        // `ParaOpen::get`, which walks down to the innermost quoted
        // content. Record the line rather than the answer: the walk
        // costs one strip per remaining quote marker, and answering
        // here answers a question nothing may ask
        // (markup-carve/carve-rs#731). The absorption state the line
        // inherits goes WITH it, so §12 is decided by the same walk
        // instead of forcing one of its own.
        let inherited_absorption = if self.para_open.may_carry_absorption() {
            self.para_open.absorption()
        } else {
            false
        };
        // The run is advanced on every ordinary quoted line, so the
        // answer is about the BLOCK this line lands in rather than
        // about the line. A continuation row appends to the table above
        // it and opens no paragraph, exactly as the row it appends to
        // does; anything else resets the run, which is what keeps a
        // `+ b |` with no table above it ordinary prose
        // (markup-carve/carve#1345).
        if self.table.observe(stripped) {
            self.para_open = ParaOpen::Closed;
            return;
        }
        let nested_item_closes = !stripped.starts_with([' ', '\t'])
            && detect_list_marker_full(stripped).is_some()
            && !self.para_open.get()
            && detect_fence_open(innermost_marker_content(stripped)).is_none()
            && !body_ends_with_open_paragraph(stripped, options);
        // A nested quote can carry the same description body base.
        let mut description_content = stripped;
        if self.may_have_term {
            description_content = strip_all_quotes(description_content).0;
        }
        let in_description =
            self.may_have_term && description_content.starts_with([' ', '\t']) && {
                self.open_entry.advance(&self.inner.lines);
                self.open_entry
                    .description_col
                    .is_some_and(|base| indent_columns(description_content) >= base)
            };
        self.description_line = in_description;
        let nested_attribute = self.list_content_columns.last().copied()
            == Some(indent_columns(stripped))
            && trim_ascii_start(stripped).starts_with('{');
        let prior_paragraph = nested_attribute
            && self.para_open.get()
            && self
                .inner
                .lines
                .last()
                .is_some_and(|line| !is_blank_line(&line.text));
        self.para_open = if nested_item_closes {
            ParaOpen::Closed
        } else if in_description {
            ParaOpen::from_line(trim_ascii_start(description_content), inherited_absorption)
        } else if is_plus_marker(innermost_marker_content(stripped))
            && cur
                .continuation_decision(at)
                .is_some_and(|decision| decision.refusal.is_some())
        {
            ParaOpen::from_line("x", inherited_absorption)
        } else {
            ParaOpen::from_line(stripped, inherited_absorption)
        };
        if detect_list_marker_full(stripped).is_some()
            && is_plus_marker(innermost_marker_content(stripped))
        {
            self.para_open = ParaOpen::Closed;
        }
        self.scan_attrs_block(cur, at, stripped, prior_paragraph);
        if self.unclosed_nested_attribute {
            // A closer without this quote's marker cannot finish the attribute.
            // The opener remains text in the list paragraph, which can still
            // take a lazy continuation below the quote.
            self.para_open = ParaOpen::from_line("x", inherited_absorption);
        }
    }

    // A WRAPPED ATTRIBUTE BLOCK CLOSES THE PARAGRAPH TOO, and only
    // the lines after its opener can tell it apart from prose that
    // happens to start with a brace. `ParaOpen` is handed ONE line
    // and passes an empty rest slice to
    // `interrupts_paragraph_with_rest`, so the single-line `{.k}`
    // closed the quoted paragraph while `{.k` / `#x}` did not: the
    // quote went on collecting, the column-0 line folded in, and the
    // author's attributes landed on it INSIDE the container they
    // were written to end (markup-carve/carve-rs#1050). §15 A5 makes
    // one block one block however many lines it takes, so the two
    // spellings answer alike.
    fn scan_attrs_block(
        &mut self,
        cur: &LineCursor<'a>,
        at: usize,
        stripped: &'a str,
        prior_paragraph: bool,
    ) {
        if at < self.attrs_scan_floor {
            return;
        }
        let nested = self.list_content_columns.last().copied() == Some(indent_columns(stripped))
            && stripped.starts_with([' ', '\t']);
        let opener = if nested {
            trim_ascii_start(stripped)
        } else {
            stripped
        };
        match quoted_attrs_block_len(opener, cur.lines.range(cur.pos..cur.lines.len())) {
            QuotedAttrsBlock::Block(len) => {
                self.para_open = ParaOpen::Closed;
                self.attrs_block_rest = len - 1;
                self.attrs_nested_run = nested;
            }
            // The scan already proved no block can start inside the
            // window it walked, so the next lines skip it. That is
            // what keeps a run of brace-shaped lines that close
            // nothing linear instead of scanned once per line.
            QuotedAttrsBlock::NoneWithin(window) => {
                self.attrs_scan_floor = at + window;
                self.unclosed_nested_attribute = trim_ascii_start(stripped).starts_with('{')
                    && (nested && prior_paragraph
                        || !nested
                            && cur
                                .peek()
                                .is_some_and(|next| strip_blockquote_prefix(next).is_none()));
            }
            QuotedAttrsBlock::No => {}
        }
    }

    /// Takes the block a refusal-free `+` the quote kept as its last line
    /// attaches. False when that block is empty or no such `+` is there.
    fn attach_after_quoted_plus(
        &mut self,
        cur: &mut LineCursor<'a>,
        options: &Options<'_>,
    ) -> bool {
        if !(cur.carried_reach(cur.pos).is_none()
            && self.inner.lines.last().is_some_and(|last| last.text == "+")
            && self
                .inner
                .lines
                .last()
                .and_then(|last| last.source_line())
                .and_then(continuation_marker::decision)
                .is_some_and(|decision| decision.refusal.is_none()))
        {
            return false;
        }
        let end = attached_block_lines_classified(
            cur.lines,
            cur.pos,
            &mut cur.comment_closer_last_index,
            &mut cur.code_closer_last_index,
            options,
            &mut |next, _| is_blank_line(next),
        );
        let start = cur.pos;
        while cur.pos < end {
            self.inner.push_at(
                cur.lines.at(cur.pos).to_string(),
                cur.source_line(cur.pos),
                cur.source_col(cur.pos),
            );
            cur.consume();
        }
        if end > start {
            self.inner.push_synthetic_blank();
            return true;
        }
        false
    }

    // An accepted physical marker attaches one block to the surviving quote.
    fn attach_physical_plus(
        &mut self,
        cur: &mut LineCursor<'a>,
        options: &Options<'_>,
        line: &str,
    ) -> bool {
        if !(trim_ascii(line) == "+"
            && indent_columns(line) == 0
            && (!cur.in_item_body || cur.continuation_decision(cur.pos).is_some())
            && (cur.continuation_decision(cur.pos).is_some()
                || cur.carried_reach(cur.pos) != Some(false))
            && cur.continuation_decision(cur.pos).is_some_and(|decision| {
                matches!(
                    decision.owner,
                    Some(continuation_marker::Kind::Quote | continuation_marker::Kind::List)
                ) && decision.refusal.is_none()
            }))
        {
            return false;
        }
        self.opaque_tail = false;
        cur.consume();
        let mut attached = LineBuffer::default();
        let cursor_lines = cur.lines;
        let mut attach_end = attached_block_lines_classified(
            cursor_lines,
            cur.pos,
            &mut cur.comment_closer_last_index,
            &mut cur.code_closer_last_index,
            options,
            &mut |next, _| {
                is_blank_line(next) || (trim_ascii(next) == "+" && indent_columns(next) == 0)
            },
        );
        if cursor_lines
            .line(cur.pos)
            .is_some_and(is_line_comment_any_column)
            && cursor_lines
                .line(attach_end)
                .is_some_and(|next| indent_columns(next) == 0 && line_starts_paragraph(next))
        {
            attach_end = attached_block_lines_classified(
                cursor_lines,
                attach_end,
                &mut cur.comment_closer_last_index,
                &mut cur.code_closer_last_index,
                options,
                &mut |next, _| is_blank_line(next) || is_plus_marker(next),
            );
        }
        while cur.pos < attach_end {
            let next = cur.lines.at(cur.pos);
            // Attached lines are spliced in verbatim, so the container took
            // nothing beyond whatever an outer one already had.
            attached.push_at(
                next.to_string(),
                cur.source_line(cur.pos),
                cur.source_col(cur.pos),
            );
            cur.pos += 1;
        }
        if !attached.lines.is_empty() {
            if self.in_fence.take().is_some() {
                self.parts.push(std::mem::take(&mut self.inner));
                self.comment_run = None;
                self.comment_lazy_lines.clear();
            }
            // `inner` always holds the quote's first content line, so a
            // leading blank separates the attached block from it.
            self.inner.push_synthetic_blank();
            self.inner.lines.extend(attached.lines);
            self.inner.push_synthetic_blank();
            self.para_open = ParaOpen::Closed;
            self.table = TableRun::default();
            self.last_nested = false;
        }
        true
    }

    // A hosted stacked `- > +` attaches a non-list block at column 0 before the
    // host quote ends (carve#3421): it joins the body as the unhosted twin reads.
    // In a nested quote (`> > - > +`) the block joins the nested body behind its markers.
    fn pull_hosted_block(
        &mut self,
        cur: &mut LineCursor<'a>,
        options: &Options<'_>,
        line: &str,
    ) -> bool {
        let follows_quoted = self.last_quoted_at == cur.pos.checked_sub(1);
        let pull_shape = follows_quoted
            && !line.starts_with([' ', '\t'])
            && item_block_opener(line)
            && detect_list_marker_full(line).is_none();
        let nested_prefix =
            self.last_quoted_at
                .filter(|_| pull_shape)
                .map_or(usize::MAX, |prior| {
                    let (body, levels) = strip_all_quotes(
                        strip_blockquote_prefix(cur.lines.at(prior)).unwrap_or(""),
                    );
                    if detect_list_marker_full(trim_ascii_start(body)).is_some() {
                        levels
                    } else {
                        usize::MAX
                    }
                });
        if !(follows_quoted
            && nested_prefix != usize::MAX
            && if nested_prefix == 0 {
                self.item_opened_at == self.last_quoted_at
            } else {
                self.block_opened_at == self.last_quoted_at
            }
            && !line.starts_with([' ', '\t'])
            && item_block_opener(line)
            && detect_list_marker_full(line).is_none()
            && self.last_quoted_at.is_some_and(|prior| {
                cur.continuation_decision(prior).is_some_and(|decision| {
                    decision.first_block
                        && decision.refusal.is_none()
                        && decision.owner == Some(continuation_marker::Kind::Quote)
                })
            }))
        {
            return false;
        }
        let end = attached_block_lines_classified(
            cur.lines,
            cur.pos,
            &mut cur.comment_closer_last_index,
            &mut cur.code_closer_last_index,
            options,
            &mut |next, _| is_blank_line(next) || is_plus_marker(next),
        )
        .max(cur.pos + 1);
        let prefix = "> ".repeat(nested_prefix);
        while cur.pos < end {
            let text = format!("{prefix}{}", cur.lines.at(cur.pos));
            let column = cur
                .source_col(cur.pos)
                .map(|col| col - prefix.len() as isize);
            self.inner.push_at(text, cur.source_line(cur.pos), column);
            cur.pos += 1;
        }
        self.para_open = ParaOpen::Closed;
        self.table = TableRun::default();
        self.last_nested = false;
        true
    }

    // Lazy continuation: a non-`>` line folds into an OPEN paragraph. A
    // blank line, a caption, or a line that starts a block ends the quote.
    // A list marker FOLDS into the open quoted paragraph as literal text --
    // the quoted paragraph follows the same rule as a top-level paragraph,
    // where a list marker does not interrupt (it needs a blank line before
    // it). `interrupts_paragraph` is the shared predicate for that decision,
    // and it already returns false for bullet/task/ordered markers, so we
    // simply defer to it. A heading is the sole construct a list marker
    // would otherwise end, and headings still interrupt via that predicate.
    fn lazy_ends_quote(&mut self, cur: &mut LineCursor<'a>, line: &str) -> bool {
        if self.attrs_closed_last {
            return true;
        }
        let fenced_below = if self.may_hold_fence {
            *self
                .nested_verdict
                .get_or_insert_with(|| self.nested_replay.verdict(&self.inner.lines, cur))
        } else {
            None
        };
        if self.inner.lines.last().is_some_and(|last| {
            detect_list_marker_full(&last.text)
                .is_some_and(|marker| marker.checked.is_some() && is_plus_marker(marker.content))
        }) && (line_starts_paragraph(line) || detect_list_marker_full(line).is_some())
        {
            return true;
        }
        // A first-block list `+` (`- +`, `- - +`) leaves no paragraph open, so a line
        // without the quote's marker ends the quote (carve#3421).
        if self.last_quoted_at == cur.pos.checked_sub(1)
            && self.block_opened_at == self.last_quoted_at
            && self.inner.lines.last().is_some_and(|last| {
                let mut body = trim_ascii_start(&last.text);
                while let Some(rest) = strip_blockquote_prefix(body) {
                    body = trim_ascii_start(rest);
                }
                detect_list_marker_full(body).is_some()
                    && is_plus_marker(innermost_marker_content(body))
            })
        {
            return true;
        }
        let term_open = if self.may_have_term
            && !is_blank_line(line)
            && !is_definition_list_start(trim_ascii_start(line))
            && strip_description_prefix(trim_ascii_start(line)).is_none()
            && (!item_block_opener(trim_ascii_start(line))
                || indent_columns(line) > 0
                || detect_fence_open(trim_ascii_start(line)).is_some())
        {
            self.open_entry.advance(&self.inner.lines);
            self.open_entry.half == Some(DefHalf::Term)
        } else {
            false
        };
        // The replay reads a description body as quoted text; its own base
        // closes the paragraph too.
        let quoted_open = if self.description_line {
            fenced_below.unwrap_or(true) && self.para_open.get()
        } else {
            fenced_below.unwrap_or_else(|| self.para_open.get())
        };
        let empty_marker_leaf = self.inner.lines.len() == 1
            && self.inner.lines.first().is_some_and(|source| {
                let (leaf, levels) = strip_all_quotes(innermost_marker_content(&source.text));
                levels > 0 && (is_blank_line(leaf) || detect_fence_open(leaf).is_some())
            });
        if !(term_open || quoted_open)
            || empty_marker_leaf
            || is_blank_line(line)
            || (term_open && detect_list_marker_full(line).is_some_and(|marker| marker.indent == 0))
            || (caption_content(line).is_some() && cur.carried_reach(cur.pos) != Some(false))
            || {
                let line_owned = line.to_string();
                interrupts_lazy_continuation(cur, &line_owned)
            }
        {
            return true;
        }
        // A flush-left description ends an open term. A description first
        // supplied by a lazy line cannot itself keep the quote open for another
        // flush-left description.
        if indent_columns(line) == 0 && strip_definition_marker(line).is_some() {
            self.open_entry.advance(&self.inner.lines);
            if self.open_entry.half == Some(DefHalf::Term) {
                return true;
            }
        }
        false
    }

    fn fold_lazy_line(&mut self, cur: &mut LineCursor<'a>, line: &str) {
        let source_line = cur.source_line(cur.pos);
        // A lazy continuation line carries no quote marker, so nothing was
        // stripped from it beyond what an outer container already took.
        let source_col = cur.source_col(cur.pos);
        // A LAZY LINE HAS NO COLUMN INSIDE THE QUOTE (PART 0). It supplies no
        // `>`, so it is not the quote's content at any column - it reached the
        // innermost open paragraph by the fold, and its indentation inside the
        // quote body means nothing. The collectors below read the quote's inner
        // lines BY COLUMN, so an unmarked `:  a` must stay framed as lazy text
        // in the open term. Only a marked line has a quote-local column that
        // can open a description (CARVE-P2-028, markup-carve/carve#3451).
        //
        // ONE SHAPE, DELIBERATELY. Every lazy line has this much in common, and
        // stripping them all is the general port carve-js#1609 measured
        // separately: it moves fence-shaped lines off the oracle's answer.
        // UNLESS A DESCRIPTION BODY IS ALREADY OPEN, where the same line is that
        // body's own lazy continuation rather than a second entry - and only for
        // a line that REACHED the innermost container's content column, because
        // one below it reached no container at all and is already lazy text
        // wherever it sits.
        // FRAMING vs #1526's DEFINITION-LIST HANDLING turns on whether the item
        // already has an open definition entry. A `:` description or `::` term
        // folded into a quoted item is that item's own definition list ONLY when
        // one is open; with a paragraph open instead, the same line is lazy text
        // and folds into the paragraph. `open_entry` tracks which
        // (markup-carve/carve-rs#1526).
        //
        //   half == None (a paragraph is open)  -> FRAME, fold as text
        //   half == Term, unmarked `:` -> FRAME, fold as term text
        //   half == Term, `::` -> a second term: keep AS-IS (the item re-parse
        //                         dedents it to its own column)
        //   half == Description, `::` -> a new term: keep AS-IS
        //   half == Description, flush-left `:` -> escaped lazy text in the `<dd>`
        //   half == Description, indented `:` -> keep its authored column
        //   below the content column -> reached no entry: keep AS-IS as lazy text
        //
        // Every line that is NOT a definition marker is framed so its
        // indentation cannot re-open a block on re-parse (carve-rs#1538).
        if self.comment_run.is_some()
            && (!self
                .comment_run
                .is_some_and(|(_, _, at)| self.nested_replay.literal_comment_openers.contains(&at))
                || self.nested_replay.chain.last().is_some_and(|level| {
                    level.comment.is_some()
                        || level
                            .host_comment
                            .is_some_and(|(_, _, provisional)| !provisional)
                }))
        {
            self.comment_lazy_lines
                .push((self.inner.lines.len(), line.to_string(), source_col));
        }
        let trimmed = trim_ascii_start(line);
        let is_term = is_definition_list_start(trimmed);
        let is_desc = strip_definition_marker(trimmed).is_some();
        let (frame, dedent) = if is_term || is_desc {
            self.open_entry.advance(&self.inner.lines);
            lazy_marker_frame(&self.open_entry, line, is_desc)
        } else {
            (true, false)
        };
        // FRAME ONCE. A nested quote folds a line its outer quote ALREADY
        // folded, and framing it twice leaves a frame behind after the one strip
        // downstream - `> > - x` with any following line put a literal
        // `U+0000 L U+0000` on the page. The executable spec carries the same
        // guard at its own item-fold for the same reason.
        let (kept, stripped_cols) = if !frame {
            if dedent {
                (trimmed.to_string(), leading_ws(line))
            } else {
                (line.to_string(), 0)
            }
        } else if is_term
            && matches!(self.open_entry.half, Some(DefHalf::Term))
            && indent_columns(line) > 0
        {
            // An indented unmarked term is plain lazy text of the open term.
            (format!("{LAZY} {trimmed}"), leading_ws(line))
        } else if line.starts_with(LAZY) {
            (line.to_string(), 0)
        } else {
            (format!("{LAZY}{}", trimmed), leading_ws(line))
        };
        self.opaque_tail = false;
        cur.consume();
        self.inner.push_at(
            kept,
            source_line,
            source_col.map(|col| col + stripped_cols as isize),
        );
    }
}

/// Whether a lazy `:`/`::` line is framed as text, and whether it is dedented.
fn lazy_marker_frame(open_entry: &QuotedEntryScan, line: &str, is_desc: bool) -> (bool, bool) {
    // A LAZY LINE HAS NO COLUMN (PART 0), so a `:`/`::` marker continues
    // the innermost open definition entry wherever it was written - the
    // column it reached does not gate the recognition. Only the OPEN HALF
    // decides.
    match open_entry.half {
        // No open entry, and the marker sits BELOW the innermost content
        // column: it reached no container at all and is the enclosing
        // item's lazy text wherever it sits, so it is kept as-is. Pulling
        // it to column 0 (frame) would eject it from a nested item -
        // `> - - :: t` / `>     # h` / ` :  a` folds the `:` into the
        // outer item instead of escaping to a top-level paragraph
        // (markup-carve/carve-rs#1526's below-column row, #1538).
        None if open_entry.closed_by_block
            && open_entry
                .content_col
                .is_some_and(|col| indent_columns(line) < col) =>
        {
            (false, false)
        }
        // A paragraph is open, no entry: the marker is lazy text of that
        // paragraph, framed so it folds in rather than re-opening a list.
        None => (true, false),
        // An unmarked lazy `:` has no column in the quote. It folds
        // into the open term as text (CARVE-P2-028); only a quoted
        // separator at the term's column can open a description.
        Some(DefHalf::Term) if is_desc => (true, false),
        // A `::` opens a second term under the existing quote claim.
        Some(DefHalf::Term) => (true, false),
        // A flush-left `:` below an explicitly quoted description is
        // lazy text in that description. It cannot open another entry.
        Some(DefHalf::Description) if is_desc && indent_columns(line) == 0 => (true, false),
        // A `::` opens a new term; an indented `:` keeps its column.
        Some(DefHalf::Description) => (false, false),
    }
}

#[inline(never)]
pub(in crate::parse) fn boxed_blockquote_node(
    children: Vec<BlockNode>,
    quote_pos: Option<Pos>,
    caption: Option<Vec<InlineNode>>,
    figure_pos: Option<Pos>,
    fenced: bool,
) -> Box<BlockNode> {
    let quote = BlockQuote {
        pos: quote_pos,
        attrs: None,
        children,
        fenced,
    };
    if let Some(caption) = caption {
        Box::new(BlockNode::Figure(Figure {
            attrs: None,
            target: Box::new(FigureTarget::BlockQuote(quote)),
            rendered_target: None,
            caption,
            short_caption: None,
            pos: figure_pos,
        }))
    } else {
        Box::new(BlockNode::BlockQuote(quote))
    }
}
