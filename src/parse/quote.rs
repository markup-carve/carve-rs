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
            while let Some(after) = strip_blockquote_prefix(peeled) {
                peeled = after;
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
            } else if strip_definition_marker(content).is_some() {
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
        let chain = &mut self.chain;
        let mut text = lines[at].text.as_str();
        // A lazy line was taken only because a paragraph was open, and it
        // continues that paragraph.
        if text.starts_with(LAZY) {
            return;
        }
        let mut level = 0;
        loop {
            if let Some((open, floor, opener, payload)) = chain[level].host_fence {
                if !is_blank_line(text) && indent_columns(text) < floor {
                    chain[level].host_fence = None;
                } else {
                    // A block closer reaches the host or opener column.
                    // Payload spans pair at any column still owned by the item.
                    let at = indent_columns(text);
                    if (payload || at == floor || at == opener)
                        && is_fence_close(trim_ascii_start(text), open)
                    {
                        chain[level].host_fence = None;
                    }
                    chain[level].by_fence = true;
                    chain[level].para = chain[level].host_verse.is_some() && !is_blank_line(text);
                    chain[level].colon_absorption = false;
                    chain.truncate(level + 1);
                    return;
                }
            }
            if let Some(open) = chain[level].fence {
                if is_fence_close(text, open) {
                    chain[level].fence = None;
                    chain[level].by_fence = true;
                }
                chain.truncate(level + 1);
                return;
            }
            // Comment contents are not syntax. A comment never overrides the
            // one-line reading, so it leaves `by_fence` alone.
            if let Some(len) = chain[level].comment {
                if is_comment_fence_close(text, len) {
                    chain[level].comment = None;
                    chain[level].by_comment = true;
                }
                chain.truncate(level + 1);
                return;
            }
            // A collected comment is opaque. A closer beyond the collected
            // item leaves a line comment, so its following blocks stay visible.
            if let Some((width, column, provisional)) = chain[level].host_comment {
                if !is_blank_line(text) && indent_columns(text) < column {
                    chain[level].host_comment = None;
                } else {
                    if is_comment_fence_close(trim_ascii_start(text), width) {
                        chain[level].host_comment = None;
                        chain[level].by_comment = chain[level]
                            .hosts
                            .last()
                            .is_some_and(|&(_, floor, _)| column < floor);
                    }
                    if !provisional {
                        chain.truncate(level + 1);
                        return;
                    }
                }
            }
            if let Some((width, floor, column)) = chain[level].host_verse {
                if !is_blank_line(text) && indent_columns(text) < floor {
                    chain[level].host_verse = None;
                } else {
                    if let Some(comment) = detect_comment_fence_line(trim_ascii_start(text)) {
                        self.literal_comment_openers.insert(at);
                        let len = comment.fence_len;
                        let col = indent_columns(text);
                        let collected = self
                            .colon_body_comments
                            .entry(level)
                            .or_default()
                            .closes_after(lines, at, level, len);
                        if collected || future_quoted_comment_closes(cur, level + 1, col, len) {
                            chain[level].host_comment = Some((len, col, !collected));
                        }
                    }
                    if let Some(open) = detect_fence_open(trim_ascii_start(text)) {
                        let opener_column = indent_columns(text);
                        if literal_code_closes(
                            lines,
                            at,
                            level,
                            floor,
                            opener_column,
                            open,
                            &mut self.literal_code_lookup,
                        ) {
                            chain[level].host_fence = Some((open, floor, opener_column, true));
                        }
                    }
                    if (indent_columns(text) == floor || indent_columns(text) == column)
                        && exact_colon_fence_len(trim_ascii_start(text)) == Some(width)
                    {
                        chain[level].host_verse = None;
                        chain[level].by_colon = true;
                        chain[level].para = false;
                        chain[level].colon_absorption = false;
                        chain.truncate(level + 1);
                        return;
                    }
                    chain[level].para = !is_blank_line(text) && !is_line_comment_any_column(text);
                    chain[level].colon_absorption = false;
                    chain[level].by_colon = true;
                    chain.truncate(level + 1);
                    return;
                }
            }
            if let Some(rest) = strip_blockquote_prefix(text) {
                chain[level].hosts.clear();
                chain[level].term_columns.clear();
                chain[level].colons.clear();
                chain[level].para = false;
                chain[level].colon_absorption = false;
                chain[level].by_fence = false;
                if chain.len() == level + 1 {
                    chain.push(NestedLevel::default());
                }
                text = rest;
                level += 1;
                continue;
            }
            if level + 1 == chain.len()
                && (!chain[level].colons.is_empty() || chain[level].by_colon)
                && is_line_comment_any_column(text)
            {
                if let Some(open) = detect_comment_fence_line_any_column(text) {
                    let column = indent_columns(text);
                    if closer_ahead(lines, at, level, |line| {
                        is_comment_fence_close_any_column(line, open.fence_len)
                    }) {
                        chain[level].host_comment = Some((open.fence_len, column, false));
                    }
                }
                chain[level].by_comment = false;
                chain[level].para = false;
                chain[level].colon_absorption = false;
                return;
            }
            // A line short of the deepest quote continues that quote's open
            // paragraph unless it interrupts one.
            let deepest_open = chain.last().is_some_and(|l| l.para && l.fence.is_none());
            if chain.len() > level + 1 && deepest_open && !is_blank_line(text) {
                let interrupts = match detect_fence_open(text) {
                    Some(open) => closer_ahead(lines, at, level, |l| is_fence_close(l, open)),
                    None => interrupts_paragraph_with_rest(text, &[]),
                };
                if !interrupts {
                    return;
                }
            }
            chain.truncate(level + 1);
            let here = &mut chain[level];
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
            let starts_sibling = detect_list_marker_full(trim_ascii_start(text)).is_some()
                && marker_ends_replayed_item(&here.hosts, indent_columns(text));
            let hosted_colon_opener = here
                .hosts
                .iter()
                .any(|&(_, column, _)| column <= indent_columns(text))
                && (!here.colon_absorption
                    || exact_colon_fence_len(trim_ascii_start(text)).is_none())
                && (detect_container_open(trim_ascii_start(text)).is_some()
                    || detect_line_block_open(trim_ascii_start(text)).is_some()
                    || detect_hardbreaks_block_open(trim_ascii_start(text)).is_some()
                    || detect_quote_block_open(trim_ascii_start(text)).is_some());
            // Replay the same handoff before an unquoted line asks whether
            // the quoted list left a paragraph open.
            let lazy_paragraph = here.para
                && !starts_sibling
                && !hosted_colon_opener
                && !(here
                    .hosts
                    .last()
                    .is_some_and(|&(_, floor, _)| indent_columns(text) < floor)
                    && detect_fence_open(trim_ascii_start(text)).is_some_and(|open| {
                        indexed_code_closes(
                            lines,
                            at,
                            level,
                            indent_columns(text),
                            None,
                            open,
                            &mut self.literal_code_lookup,
                        )
                    }))
                && !match detect_fence_open(text) {
                    Some(open) => closer_ahead(lines, at, level, |line| is_fence_close(line, open)),
                    None => interrupts_paragraph_with_rest(text, &[]),
                };
            if !is_blank_line(text) && !lazy_paragraph {
                while here
                    .colons
                    .last()
                    .is_some_and(|&(_, floor, _)| floor > indent_columns(text))
                {
                    here.colons.pop();
                }
            }
            {
                let column = indent_columns(text);
                if let Some(width) = exact_colon_fence_len(trim_ascii_start(text)) {
                    if let Some(index) = here.colons.iter().rposition(|&(seen, floor, authored)| {
                        seen == width && (floor == column || authored == column)
                    }) {
                        here.colons.truncate(index);
                        here.para = false;
                        here.colon_absorption = false;
                        here.by_colon = true;
                        return;
                    }
                }
            }
            let mut absorption_text = text;
            if !is_blank_line(text) {
                let column = indent_columns(text);
                let sibling = marker_ends_replayed_item(&here.hosts, column);
                if !lazy_paragraph {
                    while here.hosts.last().is_some_and(|&(_, col, _)| col > column) {
                        if let Some((_, column, _)) = here.hosts.pop() {
                            here.term_columns.remove(&column);
                        }
                    }
                }
                let inside_item = here.hosts.last().is_some_and(|&(_, _, item)| item);
                let mut inner = trim_ascii_start(text);
                let mut col = column;
                let mut block_start = false;
                loop {
                    if let Some(marker) = detect_list_marker_full(inner) {
                        let below_host = here
                            .hosts
                            .last()
                            .is_some_and(|&(_, content, _)| col < content);
                        if here.para && (!inside_item || below_host) && !sibling {
                            break;
                        }
                        let next = col + marker_content_col(inner).expect("matched list marker");
                        here.hosts.push((col, next, true));
                        if is_definition_list_start(trim_ascii_start(marker.content)) {
                            here.term_columns.insert(next);
                        }
                        col = next + indent_columns(marker.content);
                        inner = trim_ascii_start(marker.content);
                        block_start = true;
                        continue;
                    }
                    if parse_footnote_def_line(inner).is_some() {
                        here.hosts.push((col, col + 2, false));
                    }
                    break;
                }
                absorption_text = inner;
                if block_start && is_line_comment_any_column(inner) {
                    here.para = false;
                    here.colon_absorption = false;
                    here.by_colon = true;
                }
                if block_start {
                    here.colon_absorption = false;
                }
                let floor = here.hosts.last().map_or(0, |&(_, col, _)| col);
                if floor == 0 && col == 0 {
                    if let Some(width) = detect_line_block_open(inner) {
                        here.host_verse = Some((width, 0, col));
                        here.para = false;
                        here.colon_absorption = false;
                        here.by_colon = true;
                        return;
                    }
                }
                if floor > 0
                    && col >= floor
                    && (col == floor
                        || exact_colon_fence_len(inner).is_none()
                        || here.colons.is_empty())
                {
                    if let Some(comment) = detect_comment_fence_line(inner) {
                        let len = comment.fence_len;
                        let collected = closer_ahead(lines, at, level, |line| {
                            is_comment_fence_close(trim_ascii_start(line), len)
                        });
                        if collected || future_quoted_comment_closes(cur, level + 1, col, len) {
                            here.host_comment = Some((len, col, !collected));
                            here.para = false;
                            here.colon_absorption = false;
                            return;
                        }
                    }
                    if detect_line_block_open(inner).is_some()
                        && here
                            .colons
                            .last()
                            .is_some_and(|&(_, base, authored)| floor == base && col > authored)
                    {
                        here.para = true;
                        here.colon_absorption = true;
                        return;
                    }
                    if let Some(width) = detect_line_block_open(inner) {
                        here.host_verse = Some((width, floor, col));
                        here.para = false;
                        here.colon_absorption = false;
                        here.by_colon = !block_start;
                        return;
                    }
                    if let Some(width) = detect_container_open(inner)
                        .filter(|_| {
                            !here.colon_absorption || exact_colon_fence_len(inner).is_none()
                        })
                        .map(|open| open.fence_len)
                        .or_else(|| detect_hardbreaks_block_open(inner))
                        .or_else(|| detect_quote_block_open(inner))
                    {
                        here.colons.push((width, floor, col));
                        here.by_colon = !block_start;
                        here.para = false;
                        here.colon_absorption = false;
                        return;
                    }
                }
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
                    here.term_columns.remove(&floor);
                }
                if floor > 0
                    && col >= floor
                    && !here.term_columns.contains(&floor)
                    && (row
                        || detect_heading(leaf).is_some()
                        || detect_thematic_break(leaf)
                        || (!block_start && detect_prepass_def_term(inner).is_some())
                        || standalone_attrs_block_len(&[leaf]) == Some(1)
                        || parse_link_def_line(leaf).is_some()
                        || (block_start && is_line_comment_any_column(leaf)))
                {
                    here.para = false;
                    here.colon_absorption = false;
                    here.by_colon = true;
                    return;
                }
                if let Some(open) = detect_fence_open(inner).filter(|_| floor > 0) {
                    // The host holds the fence whether the opener sits AT its
                    // content column or past it. A fence after an open paragraph
                    // still needs a closer ahead (section 10). Inside a colon
                    // paragraph it must reach the opener's authored column.
                    if block_start
                        || !here.para
                        || indexed_code_closes(
                            lines,
                            at,
                            level,
                            col,
                            here.colons.is_empty().then_some(floor),
                            open,
                            &mut self.literal_code_lookup,
                        )
                    {
                        self.fence_count += 1;
                        here.fence_ordinal = self.fence_count;
                        here.host_fence = Some((open, floor, col, false));
                        here.para = false;
                        here.colon_absorption = false;
                        here.by_fence = true;
                        return;
                    }
                    here.para = true;
                    return;
                }
                if block_start && is_line_comment_any_column(inner) {
                    here.para = false;
                    here.colon_absorption = false;
                    here.by_colon = true;
                    return;
                }
            }
            if let Some(open) = detect_fence_open(text) {
                // At block start a fence opens with or without a closer; after
                // a paragraph it needs one (§10), or it is inline verbatim.
                if !here.para
                    || indexed_code_closes(
                        lines,
                        at,
                        level,
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
            if let Some(comment) = detect_comment_fence_line(text) {
                let len = comment.fence_len;
                if closer_ahead(lines, at, level, |l| is_comment_fence_close(l, len)) {
                    here.comment = Some(len);
                }
            }
            if exact_colon_fence_len(trim_ascii_start(text)).is_some() && !here.colons.is_empty() {
                here.para = true;
                here.colon_absorption = false;
                return;
            }
            if after_comment
                && !here.para
                && !here.colon_absorption
                && here.colons.is_empty()
                && exact_colon_fence_len(trim_ascii_start(text)).is_some()
            {
                here.by_colon = true;
                return;
            }
            if is_line_comment_any_column(text) {
                here.by_comment = here
                    .hosts
                    .last()
                    .is_some_and(|&(_, column, _)| indent_columns(text) < column);
            }
            if here.para
                && indent_columns(text) > 0
                && exact_colon_fence_len(trim_ascii_start(text)).is_some()
            {
                return;
            }
            let mut paragraph = ParaOpen::from_line(text, here.colon_absorption);
            here.para = paragraph.get();
            here.colon_absorption = here.para
                && (here.colon_absorption
                    || is_invalid_colon_fence_opener_text(trim_ascii_start(absorption_text)));
            return;
        }
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
    let mut fence_closers = QuotedFenceClosers::default();
    let mut parts = Vec::new();
    let span_start = cur.pos;
    let mut inner = LineBuffer::default();
    let mut para_open = ParaOpen::Closed;
    let mut comment_run = None;
    let mut comment_lazy_lines: Vec<(usize, String, Option<isize>)> = Vec::new();
    let mut in_fence: Option<FenceOpen> = None;
    let mut opaque_tail = false;
    let mut fence_count = 0;
    // What the quote's own lines have left open, for the lazy branch below.
    // ADVANCED ON DEMAND, never per line: `strip_blockquote_prefix` is counted
    // and a depth ladder must not pay per level per line
    // (`quote_prefix_calls`). Only a `:`-shaped lazy line asks, and the scan
    // resumes where it stopped, so the whole walk is one pass over the lines.
    let mut open_entry = QuotedEntryScan::default();
    let mut may_have_term = false;
    // THE QUOTE'S OWN LAST BLOCK IS WHAT S4 ASKS ABOUT, and a continuation row
    // belongs to the table above it (§5 T6, markup-carve/carve#1348). `ParaOpen`
    // is handed ONE line and a `+ b |` line reads as prose on its own, so a
    // quote ending on a continuation row recorded an open paragraph and the
    // flush-left line below folded in - while the SAME table ending on a
    // standard row closed the quote. One question answered two ways by a
    // spelling. This carries the run so the two spellings answer alike.
    let mut table = TableRun::default();
    // ONE BLOCK IS ONE BLOCK HOWEVER MANY LINES IT TAKES (§15 A5). A wrapped
    // attribute block (`{.k` / `#x}`) closes the quoted paragraph exactly as the
    // single-line `{.k}` does, and the lines after its opener are its own
    // content rather than paragraph text. `ParaOpen` decides from ONE line, so
    // the block's extent is tracked here beside the code fence's - the same
    // shape, for the same reason (markup-carve/carve-rs#1050).
    let mut attrs_block_rest: usize = 0;
    // The first line index the block lookahead is still worth running on. A scan
    // that walked to a blank line, or to the end of the quoted run, without
    // meeting a line that could CLOSE an attribute block has proved the same for
    // every line it passed - see `quoted_attrs_block_len`.
    let mut attrs_scan_floor: usize = 0;
    // The last quoted line reached a nested quote, so `para_open` may have read a
    // fence one level down as prose; see `NestedFenceReplay`.
    let mut last_nested = false;
    let mut nested_verdict: Option<Option<bool>> = None;
    let mut nested_replay = NestedFenceReplay::default();
    // No replay without a fence to find: every level of a quote ladder would
    // otherwise walk its whole body once.
    let mut may_hold_fence = false;
    // The last quoted line was read on a description body's base.
    let mut description_line = false;
    let mut literal_quote_body = None;
    let mut last_quoted_at = None;
    // The quoted line that opened a list item, not text in a paragraph or a fence.
    let mut item_opened_at = None;
    // The quoted line that opened a block at any depth, not text in a paragraph or a fence.
    let mut block_opened_at = None;
    let mut list_open = false;
    while let Some(line) = cur.peek() {
        let quote_line = if cur.colon_view.is_some_and(|view| view.marker_hosted) {
            trim_ascii_start(line)
        } else {
            line
        };
        if let Some(stripped) = strip_blockquote_prefix(quote_line) {
            let marker = detect_list_marker_full(trim_ascii_start(stripped)).is_some();
            // Resolved only for a list or quote line ending in `+`, so plain quoted prose
            // pays no paragraph resolution here.
            let plus_line = trim_ascii_end(stripped).ends_with('+')
                && (marker || stripped.starts_with('>'))
                && in_fence.is_none();
            let opens =
                (marker || plus_line) && in_fence.is_none() && (!para_open.get() || list_open);
            block_opened_at = (opens && plus_line).then_some(cur.pos);
            item_opened_at = (opens && marker).then_some(cur.pos);
            // A sibling item may follow an item's own line, whose lead leaves a paragraph open.
            list_open = item_opened_at.is_some();
            if !may_have_term && stripped.contains(":: ") {
                let trimmed = trim_ascii_start(stripped);
                may_have_term = (inner.lines.is_empty()
                    || detect_list_marker_full(trimmed).is_some())
                    && (trimmed.starts_with('>')
                        || innermost_marker_content(trimmed).starts_with(":: "));
            }
            if stripped.contains("%%%") {
                let (comment_depth, _, comment_text) = prepass_quote_scope(stripped);
                if let Some((run, depth, _)) = comment_run {
                    if comment_depth == depth
                        && detect_list_marker_full(trim_ascii_start(stripped)).is_none()
                        && is_comment_fence_close_any_column(comment_text, run)
                    {
                        for (index, text, column) in comment_lazy_lines.drain(..) {
                            inner.lines[index].text = text;
                            inner.lines[index].stripped = column;
                        }
                        comment_run = None;
                    }
                } else if in_fence.is_none() {
                    comment_run = detect_comment_fence_line_any_column(comment_text)
                        .map(|open| (open.fence_len, comment_depth, inner.lines.len()));
                }
            }
            let source_line = cur.source_line(cur.pos);
            let at = cur.pos;
            cur.consume();
            // Keep the prior quoted line by index. Only a line ending in `+`
            // asks for nested depth, so a deep quote does not rescan its chain
            // on every ordinary line.
            let prior_quoted_at = last_quoted_at.replace(at);
            if strip_blockquote_prefix(stripped).is_none()
                && prior_quoted_at.is_some_and(|prior| {
                    let mut previous = strip_blockquote_prefix(cur.lines.at(prior)).unwrap_or("");
                    let mut nested = false;
                    while let Some(rest) = strip_blockquote_prefix(previous) {
                        previous = rest;
                        nested = true;
                    }
                    nested
                        && is_plus_marker(innermost_marker_content(previous))
                        && cur
                            .continuation_decision(prior)
                            .is_some_and(|decision| decision.refusal.is_none())
                })
            {
                inner.push_synthetic_blank();
            }
            if cur.carried_reach(at) == Some(true)
                && is_plus_marker(stripped)
                && cur.continuation_decision(at).is_some_and(|decision| {
                    decision.refusal.is_none()
                        && decision.owner != Some(continuation_marker::Kind::Quote)
                })
                && inner
                    .lines
                    .last()
                    .is_some_and(|last| strip_blockquote_prefix(&last.text).is_some())
            {
                continue;
            }
            // Pass a marker at this quote's content column to the nested
            // quote's collector. It owns the extent of the attached block.
            if cur.carried_reach(at) != Some(false)
                && is_plus_marker(stripped)
                && cur.continuation_decision(at).is_some_and(|decision| {
                    decision.owner == Some(continuation_marker::Kind::Quote)
                        && decision.refusal.is_none()
                })
                && prior_quoted_at.is_some()
                && inner.lines.last().is_some_and(|last| {
                    strip_blockquote_prefix(&last.text).is_some()
                        || is_definition_placeholder(&last.text)
                })
            {
                if cur
                    .peek()
                    .and_then(strip_blockquote_prefix)
                    .is_some_and(|next| !is_blank_line(next))
                    && inner.lines.last().is_some_and(|last| {
                        is_definition_placeholder(innermost_marker_content(&last.text))
                    })
                {
                    cur.consume();
                    continue;
                }
                if cur.carried_reach(at) == Some(true) {
                    inner.push_reached_at("+".to_string(), source_line, Some(0));
                } else {
                    inner.push_at("+".to_string(), source_line, Some(0));
                }
                para_open = ParaOpen::Closed;
                continue;
            }
            let first_block = cur.continuation_decision(at).is_some_and(|decision| {
                decision.owner == Some(continuation_marker::Kind::Quote)
                    && decision.refusal.is_none()
            });
            let mut deepest = stripped;
            let mut levels: usize = 0;
            if first_block {
                while let Some(next) = strip_blockquote_prefix(deepest) {
                    deepest = next;
                    levels += 1;
                }
            }
            let prior_levels = if first_block && levels > 0 {
                prior_quoted_at.map_or(0, |prior| {
                    let mut content = strip_blockquote_prefix(cur.lines.at(prior)).unwrap_or("");
                    let mut depth = 0;
                    while let Some(next) = strip_blockquote_prefix(content) {
                        content = next;
                        depth += 1;
                    }
                    depth
                })
            } else {
                0
            };
            if first_block
                && is_plus_marker(deepest)
                && (levels > prior_levels || (levels == 0 && inner.lines.is_empty()))
            {
                // Preserve a nested quote marker for its own parse. The
                // innermost quote consumes the `+` and takes the block.
                let follower_nested_levels = cur.peek().map_or(0, |next| {
                    let mut rest = strip_blockquote_prefix(next).unwrap_or(next);
                    let mut levels = 0;
                    while let Some(inner) = strip_blockquote_prefix(rest) {
                        rest = inner;
                        levels += 1;
                    }
                    levels
                });
                let quoted_follower = cur
                    .peek()
                    .is_some_and(|next| strip_blockquote_prefix(next).is_some());
                let keep_levels = if prior_levels > 0 {
                    levels
                } else if quoted_follower {
                    levels.saturating_sub(follower_nested_levels + 1)
                } else {
                    levels
                };
                if deepest != stripped && keep_levels > 0 {
                    let mut kept = stripped;
                    for _ in keep_levels..levels {
                        kept = strip_blockquote_prefix(kept).unwrap_or(kept);
                    }
                    let column = stripped_col(cur.source_col(at), line, kept);
                    if cur.carried_reach(at) == Some(true) {
                        inner.push_reached_at(kept.to_string(), source_line, column);
                    } else {
                        inner.push_at(kept.to_string(), source_line, column);
                    }
                }
                let end = attached_block_lines_classified(
                    cur.lines,
                    cur.pos,
                    &mut cur.comment_closer_last_index,
                    &mut cur.code_closer_last_index,
                    options,
                    &mut |next, _| is_blank_line(next) || is_plus_marker(next),
                );
                while cur.pos < end {
                    let next = cur.lines.at(cur.pos);
                    let kept = if prior_levels > 0 {
                        strip_blockquote_prefix(next).unwrap_or(next)
                    } else {
                        next
                    };
                    inner.push_at(
                        kept.to_string(),
                        cur.source_line(cur.pos),
                        stripped_col(cur.source_col(cur.pos), next, kept),
                    );
                    cur.pos += 1;
                }
                inner.push_synthetic_blank();
                para_open = ParaOpen::Closed;
                continue;
            }
            nested_verdict = None;
            description_line = false;
            let after_nested = last_nested && may_hold_fence;
            last_nested = in_fence.is_none() && (stripped == ">" || stripped.starts_with("> "));
            may_hold_fence = may_hold_fence
                || stripped.contains("```")
                || stripped.contains("~~~")
                || stripped.contains(":::")
                || stripped.contains("- ")
                || stripped.contains("* ")
                || stripped.contains(". ")
                || stripped.contains(") ");
            // The quote marker (and its optional space) is a pure prefix, so the
            // quoted line's columns are knowable in the document.
            let stripped_at = stripped_col(cur.source_col(at), line, stripped);
            if let Some(width) = literal_quote_body {
                if exact_colon_fence_len(stripped) == Some(width) {
                    literal_quote_body = None;
                    para_open = ParaOpen::Closed;
                } else {
                    para_open = if comment_run.is_some() {
                        ParaOpen::Closed
                    } else {
                        ParaOpen::from_line("x", false)
                    };
                }
                opaque_tail = false;
                table = TableRun::default();
            } else if let Some(width) = detect_line_block_open(stripped) {
                literal_quote_body = Some(width);
                para_open = ParaOpen::from_line("x", false);
                opaque_tail = false;
                table = TableRun::default();
            } else if attrs_block_rest > 0 {
                opaque_tail = false;
                // Inside a wrapped attribute block the opener already closed the
                // paragraph; its remaining lines close nothing further and open
                // nothing either.
                attrs_block_rest -= 1;
                para_open = ParaOpen::Closed;
                table = TableRun::default();
            } else if let Some(open) = in_fence {
                opaque_tail = true;
                if is_fence_close(stripped, open) {
                    in_fence = None;
                    opaque_tail = fence_count == 1;
                }
                para_open = ParaOpen::Closed;
                table = TableRun::default();
                // The absorption belongs to ONE paragraph: whatever closed it
                // -- a blank line, a real opener, a code fence -- ends the
                // absorption too. Spelled at each site that closes the
                // paragraph rather than once after the chain, because after the
                // chain it would have to READ `para_open`, and reading it on
                // every quoted line is the walk this defers.
            } else if let Some(open) = detect_fence_open(stripped) {
                table = TableRun::default();
                // The line above reached a nested quote, so its fence may be
                // what the paragraph ended on.
                let fenced_below = if after_nested {
                    nested_replay.verdict(&inner.lines, cur)
                } else {
                    None
                };
                if !fenced_below.unwrap_or_else(|| para_open.get()) {
                    // Fence at block start opens (unterminated renders to end).
                    opaque_tail = true;
                    fence_count += 1;
                    in_fence = Some(open);
                    para_open = ParaOpen::Closed;
                } else {
                    // After an open paragraph a fence interrupts only with a
                    // matching closer ahead (§10); else it is inline verbatim.
                    let has_closer = fence_closers.contains(cur.lines, cur.pos, open);
                    if has_closer {
                        opaque_tail = true;
                        fence_count += 1;
                        in_fence = Some(open);
                        para_open = ParaOpen::Closed;
                    }
                    // No closer: the fence is inline verbatim and the paragraph
                    // (already resolved OPEN by the test above) stays open, so
                    // the absorption survives - as it did when the reset was
                    // written `if !para_open`.
                }
            } else {
                opaque_tail = false;
                // Whether this line leaves a paragraph open is decided by
                // `ParaOpen::get`, which walks down to the innermost quoted
                // content. Record the line rather than the answer: the walk
                // costs one strip per remaining quote marker, and answering
                // here answers a question nothing may ask
                // (markup-carve/carve-rs#731). The absorption state the line
                // inherits goes WITH it, so §12 is decided by the same walk
                // instead of forcing one of its own.
                let inherited_absorption = if para_open.may_carry_absorption() {
                    para_open.absorption()
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
                if table.observe(stripped) {
                    para_open = ParaOpen::Closed;
                    if cur.carried_reach(at) == Some(true) {
                        inner.push_reached_at(stripped.to_string(), source_line, stripped_at);
                    } else {
                        inner.push_at(stripped.to_string(), source_line, stripped_at);
                    }
                    continue;
                }
                let nested_item_closes = !stripped.starts_with([' ', '\t'])
                    && detect_list_marker_full(stripped).is_some()
                    && !para_open.get()
                    && detect_fence_open(innermost_marker_content(stripped)).is_none()
                    && !body_ends_with_open_paragraph(stripped, options);
                // A nested quote can carry the same description body base.
                let mut description_content = stripped;
                if may_have_term {
                    while let Some(next) = strip_blockquote_prefix(description_content) {
                        description_content = next;
                    }
                }
                let in_description =
                    may_have_term && description_content.starts_with([' ', '\t']) && {
                        open_entry.advance(&inner.lines);
                        open_entry
                            .description_col
                            .is_some_and(|base| indent_columns(description_content) >= base)
                    };
                description_line = in_description;
                para_open = if nested_item_closes {
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
                    para_open = ParaOpen::Closed;
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
                if at >= attrs_scan_floor {
                    match quoted_attrs_block_len(
                        stripped,
                        cur.lines.range(cur.pos..cur.lines.len()),
                    ) {
                        QuotedAttrsBlock::Block(len) => {
                            para_open = ParaOpen::Closed;
                            attrs_block_rest = len - 1;
                        }
                        // The scan already proved no block can start inside the
                        // window it walked, so the next lines skip it. That is
                        // what keeps a run of brace-shaped lines that close
                        // nothing linear instead of scanned once per line.
                        QuotedAttrsBlock::NoneWithin(window) => {
                            attrs_scan_floor = at + window;
                        }
                        QuotedAttrsBlock::No => {}
                    }
                }
            }
            if cur.carried_reach(at) == Some(true) {
                inner.push_reached_at(stripped.to_string(), source_line, stripped_at);
            } else {
                inner.push_at(stripped.to_string(), source_line, stripped_at);
            }
            continue;
        }
        if cur.carried_reach(cur.pos).is_none()
            && inner.lines.last().is_some_and(|last| last.text == "+")
            && inner
                .lines
                .last()
                .and_then(|last| last.source_line())
                .and_then(continuation_marker::decision)
                .is_some_and(|decision| decision.refusal.is_none())
        {
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
                inner.push_at(
                    cur.lines.at(cur.pos).to_string(),
                    cur.source_line(cur.pos),
                    cur.source_col(cur.pos),
                );
                cur.consume();
            }
            if end > start {
                inner.push_synthetic_blank();
                continue;
            }
        }
        // An accepted physical marker attaches one block to the surviving quote.
        if trim_ascii(line) == "+"
            && indent_columns(line) == 0
            && (!cur.in_item_body || cur.continuation_decision(cur.pos).is_some())
            && (cur.continuation_decision(cur.pos).is_some()
                || cur.carried_reach(cur.pos) != Some(false))
            && cur.continuation_decision(cur.pos).is_some_and(|decision| {
                matches!(
                    decision.owner,
                    Some(continuation_marker::Kind::Quote | continuation_marker::Kind::List)
                ) && decision.refusal.is_none()
            })
        {
            opaque_tail = false;
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
                if in_fence.take().is_some() {
                    parts.push(std::mem::take(&mut inner));
                    comment_run = None;
                    comment_lazy_lines.clear();
                }
                // `inner` always holds the quote's first content line, so a
                // leading blank separates the attached block from it.
                inner.push_synthetic_blank();
                inner.lines.extend(attached.lines);
                inner.push_synthetic_blank();
                para_open = ParaOpen::Closed;
                table = TableRun::default();
                last_nested = false;
            }
            continue;
        }
        // A hosted stacked `- > +` attaches a non-list block at column 0 before the
        // host quote ends (carve#3421): it joins the body as the unhosted twin reads.
        if last_quoted_at == cur.pos.checked_sub(1)
            && item_opened_at == last_quoted_at
            && !line.starts_with([' ', '\t'])
            && item_block_opener(line)
            && detect_list_marker_full(line).is_none()
            && last_quoted_at.is_some_and(|prior| {
                strip_blockquote_prefix(cur.lines.at(prior))
                    .is_some_and(|body| detect_list_marker_full(trim_ascii_start(body)).is_some())
                    && cur.continuation_decision(prior).is_some_and(|decision| {
                        decision.first_block
                            && decision.refusal.is_none()
                            && decision.owner == Some(continuation_marker::Kind::Quote)
                    })
            })
        {
            let end = attached_block_lines_classified(
                cur.lines,
                cur.pos,
                &mut cur.comment_closer_last_index,
                &mut cur.code_closer_last_index,
                options,
                &mut |next, _| is_blank_line(next) || is_plus_marker(next),
            )
            .max(cur.pos + 1);
            while cur.pos < end {
                let text = cur.lines.at(cur.pos).to_string();
                inner.push_at(text, cur.source_line(cur.pos), cur.source_col(cur.pos));
                cur.pos += 1;
            }
            para_open = ParaOpen::Closed;
            table = TableRun::default();
            last_nested = false;
            continue;
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
        let fenced_below = if may_hold_fence {
            *nested_verdict.get_or_insert_with(|| nested_replay.verdict(&inner.lines, cur))
        } else {
            None
        };
        if inner.lines.last().is_some_and(|last| {
            detect_list_marker_full(&last.text)
                .is_some_and(|marker| marker.checked.is_some() && is_plus_marker(marker.content))
        }) && (line_starts_paragraph(line) || detect_list_marker_full(line).is_some())
        {
            break;
        }
        // A first-block list `+` (`- +`, `- - +`) leaves no paragraph open, so a line
        // without the quote's marker ends the quote (carve#3421).
        if last_quoted_at == cur.pos.checked_sub(1)
            && block_opened_at == last_quoted_at
            && inner.lines.last().is_some_and(|last| {
                let mut body = trim_ascii_start(&last.text);
                while let Some(rest) = strip_blockquote_prefix(body) {
                    body = trim_ascii_start(rest);
                }
                detect_list_marker_full(body).is_some()
                    && is_plus_marker(innermost_marker_content(body))
            })
        {
            break;
        }
        let term_open = if may_have_term
            && !is_blank_line(line)
            && !is_definition_list_start(trim_ascii_start(line))
            && strip_description_prefix(trim_ascii_start(line)).is_none()
            && (!item_block_opener(trim_ascii_start(line))
                || indent_columns(line) > 0
                || detect_fence_open(trim_ascii_start(line)).is_some())
        {
            open_entry.advance(&inner.lines);
            open_entry.half == Some(DefHalf::Term)
        } else {
            false
        };
        // The replay reads a description body as quoted text; its own base
        // closes the paragraph too.
        let quoted_open = if description_line {
            fenced_below.unwrap_or(true) && para_open.get()
        } else {
            fenced_below.unwrap_or_else(|| para_open.get())
        };
        let empty_marker_leaf = inner.lines.len() == 1
            && inner.lines.first().is_some_and(|source| {
                let mut leaf = innermost_marker_content(&source.text);
                let mut nested_quote = false;
                while let Some(content) = strip_blockquote_prefix(leaf) {
                    nested_quote = true;
                    leaf = content;
                }
                nested_quote && (is_blank_line(leaf) || detect_fence_open(leaf).is_some())
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
            break;
        }
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
        // A flush-left description ends an open term. A description first
        // supplied by a lazy line cannot itself keep the quote open for another
        // flush-left description.
        if indent_columns(line) == 0 && strip_definition_marker(line).is_some() {
            open_entry.advance(&inner.lines);
            if open_entry.half == Some(DefHalf::Term) {
                break;
            }
        }
        if comment_run.is_some()
            && (!comment_run
                .is_some_and(|(_, _, at)| nested_replay.literal_comment_openers.contains(&at))
                || nested_replay.chain.last().is_some_and(|level| {
                    level.comment.is_some()
                        || level
                            .host_comment
                            .is_some_and(|(_, _, provisional)| !provisional)
                }))
        {
            comment_lazy_lines.push((inner.lines.len(), line.to_string(), source_col));
        }
        let trimmed = trim_ascii_start(line);
        let is_term = is_definition_list_start(trimmed);
        let is_desc = strip_definition_marker(trimmed).is_some();
        let (frame, dedent) = if is_term || is_desc {
            open_entry.advance(&inner.lines);
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
            && matches!(open_entry.half, Some(DefHalf::Term))
            && indent_columns(line) > 0
        {
            // An indented unmarked term is plain lazy text of the open term.
            (format!("{LAZY} {trimmed}"), leading_ws(line))
        } else if line.starts_with(LAZY) {
            (line.to_string(), 0)
        } else {
            (format!("{LAZY}{}", trimmed), leading_ws(line))
        };
        opaque_tail = false;
        cur.consume();
        inner.push_at(
            kept,
            source_line,
            source_col.map(|col| col + stripped_cols as isize),
        );
    }
    let opaque_tail = if nested_verdict.is_some() {
        nested_replay.marker_fence_tail()
    } else {
        opaque_tail
    };
    parts.push(inner);
    (span_start, parts, opaque_tail)
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
