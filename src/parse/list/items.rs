use super::*;
use std::ops::ControlFlow;

const NEXT: ControlFlow<()> = ControlFlow::Continue(());
const END: ControlFlow<()> = ControlFlow::Break(());

#[inline(never)]
pub(crate) fn parse_list(
    cur: &mut LineCursor,
    options: &Options<'_>,
    deferred: &mut Vec<ItemBody>,
) -> Box<BlockNode> {
    // Where this list's own chunks start in the caller's vector, so the span
    // rule below can reach back for them once it knows which spelling it used.
    let deferred_start = deferred.len();
    let span_start = cur.pos;
    let first = cur.peek().unwrap();
    let first_marker = detect_list_marker_full(first).unwrap();
    let base_indent = first_marker.indent;
    // §11 ambiguous-letter tie-break: a single roman-letter first marker is
    // roman or alpha depending on its consecutive sibling. Resolve it against
    // the next sibling marker before fixing the list's type.
    let (start, ol_type) = resolve_ordered_first(&first_marker, cur, base_indent);
    let mut scan = ListScan::new(cur, options, deferred, first_marker, ol_type);
    scan.run();
    let ListScan {
        cur,
        deferred,
        first_marker,
        items,
        tight,
        pending_attrs,
        pending_attrs_pos,
        ..
    } = scan;
    finish_list(
        cur,
        options,
        deferred,
        ListFinish {
            deferred_start,
            span_start,
            first_marker,
            start,
            ol_type,
            tight,
            pending_attrs,
            pending_attrs_pos,
        },
        items,
    )
}

/// The list loop's state: the list's fixed shape, the items built so far, and
/// what the current (last) item still holds open.
struct ListScan<'a, 'l, 'o> {
    cur: &'a mut LineCursor<'l>,
    options: &'o Options<'o>,
    deferred: &'a mut Vec<ItemBody>,
    first_marker: ListMarker<'l>,
    base_indent: usize,
    is_task: bool,
    is_ordered: bool,
    first_dialect: OlDialect,
    items: Vec<ListItem>,
    tight: bool,
    pending_blank: bool,
    // How many blank lines this loop has consumed since the last line with
    // content. PART 9 §11 N1a's hard boundary is a run of THREE OR MORE before
    // a compatible sibling marker, so the count matters and `pending_blank`
    // alone -- which is a flag -- cannot carry it.
    blank_run: usize,
    // A block-attribute block that ended a continuation chunk, waiting for the
    // SUB-LIST it was written in front of. The chunk boundary is what separated
    // them (see `split_trailing_attrs`), so this carries them across it.
    //
    // It survives blank lines, because a blank does not break attachment
    // anywhere else either - `{.x}` / blank / `- b` at document level attaches.
    // Everything that opens an item or attaches a block of its own clears it,
    // so a set of attributes can only ever reach the sub-list that directly
    // follows it, and one that reaches nothing is dropped exactly as before.
    pending_attrs: Option<Attrs>,
    pending_attr_slots: Option<HashSet<AttrSlot>>,
    // Where that set was written, for §15 A4's diagnostic. This slot holds
    // attributes LIFTED off a chunk (`split_trailing_attrs`), so the span comes
    // from the chunk's own maps rather than from the cursor.
    pending_attrs_pos: Option<Pos>,
    // The current item's content column (where its content begins after the
    // marker). Nested content and sub-blocks of the last item dedent by this, so
    // it persists across iterations and is updated as each item is opened.
    //
    // A PLACEHOLDER, NOT A RULE, until the first item opens. `parse_list` is
    // only entered on a marker, and the sibling branch assigns before anything
    // reads this, so the initial value is dead - measured, by setting it to 999
    // and finding the corpus and every list test green. It is deliberately not
    // spelled as the real computation: a third copy of the column rule that
    // nothing can exercise is exactly what let the constant drift from the two
    // live ones.
    content_col: usize,
    // The current item's own fenced code block, still OPEN. A FENCED BODY IS
    // NOT A PARAGRAPH, so nothing below the content column folds into the item
    // while one is open: PART 9 §24's S1 stops at the ITEM, S2 never fires, and
    // S4's lazy branch wants a paragraph a verbatim body is not - the item and
    // the list close, and the residue re-parses in the surviving context
    // (markup-carve/carve#950). It lives here, beside `content_col`, because the
    // guard is on the OPEN FENCE rather than on where the fence was opened: the
    // opener may be the MARKER line, which the collectors never see, or a later
    // CONTINUATION line, after the item's paragraph state has reopened.
    item_open_fence: Option<FenceOpen>,
    // Does the current item still hold an OPEN PARAGRAPH for a lazy line?
    //
    // The comment exception needs it. Section 24 C3 says a comment does not
    // close the item, and that is right - but it does not reach the item
    // either, so once the item holds no paragraph, PART 1 S4's otherwise has
    // already ended it and both the comment and the line under it belong to the
    // document. Asked unconditionally, the exception revived an item its own
    // block had closed: `- # h` / `%% c` / `tail`, and the same over a fence, a
    // table and an indented comment (markup-carve/carve-rs#2109, corpus
    // 506-comment-columns-and-surviving-list-items-6).
    //
    // True at the start because the lead-paragraph path is what the loop resumes
    // from, and that path leaves a paragraph - which is exactly why that path has
    // to SAY so rather than inherit: an item after a marker-line heading read the
    // heading's answer and refused a lazy line of its own (carve-rs#2120).
    item_paragraph_open: ParagraphOpen<'o>,
    // A refused fence stays paragraph text but still affects lazy-line
    // ownership. Its blank separators count when the second paragraph reaches
    // the item's content column.
    item_unopened_fence_span: bool,
    item_unclosed_fence_base: Option<usize>,
    item_rejected_fence_owner: bool,
    // Whether the current item has shown anything yet: a blank after an invisible
    // lead separates nothing, so it does not loosen (carve#4639).
    item_shows: bool,
    // An empty first-block item has no paragraph for a below-column line to continue.
    last_item_bare_continuation: bool,
    // The same S4 state for an item whose marker-line content is only a
    // floating attribute block: it has no paragraph for a below-column line to
    // continue, and the pending attribute cannot pull that line into its scope.
    last_item_marker_attr_only: bool,
}

/// A sibling marker that has just opened an item.
struct ItemOpen<'l> {
    marker: ListMarker<'l>,
    attrs: Option<Attrs>,
    at: usize,
    source_line: Option<usize>,
}

/// A following-line sub-list chunk, collected for the last item.
struct SublistChunk {
    nested: MappedSource,
    closed_marker_comment_span: bool,
    stopped_fence: bool,
    descendant_text_after_fence: bool,
    nested_bare_ends_item: bool,
    band_line_ends_the_chunk: bool,
}

/// A marker-line sub-list lead (`- - a`) and the innermost item it opens.
struct MarkerSublist<'l> {
    content: &'l str,
    innermost_content: &'l str,
    inner_lead: &'l str,
    ladder_columns: Vec<usize>,
    nested_content_col: usize,
    nested_definition_ended_paragraph: bool,
    nested_lead_is_continuation: bool,
    before_block: usize,
}

fn marker_ladder_columns(mut content: &str, initial: usize) -> Vec<usize> {
    let mut columns = vec![initial];
    while let Some(marker) = detect_list_marker_full(content) {
        let Some(width) = marker_content_col(content) else {
            break;
        };
        columns.push(columns.last().unwrap() + width);
        content = marker.content;
    }
    columns
}

/// What a marker-line block lead left open once its body was collected.
struct BlockLead {
    leaves_content_open: bool,
    retained_marker_comment: bool,
    marker_colon_closed: bool,
    marker_colon_open: bool,
    swallowed_blank_separator: bool,
    literal_comment_handoff: bool,
    definition_comment_handoff: bool,
}

/// The item's lead paragraph while its lazy lines are gathered.
struct LeadParagraph {
    lines: Vec<String>,
    suppress_colon_interrupt: bool,
    // The first raw line a failed wrapped-attribute lookahead has not
    // already proved cannot start one. Without this floor, a run of
    // unclosed `{.a` lines scans the whole remaining item once per line.
    attrs_scan_floor: usize,
    anchors: Option<Vec<Option<(usize, isize)>>>,
    caption_below_content_column: bool,
    // CARVE-P2-021 item 4: once a lone image's caption slot is filled, a
    // further `^ ` line begins its own paragraph.
    image_lead: bool,
    caption_claimed: bool,
    // An inline image's figure is known before resolution; a reference
    // image's is not, and its unpromoted text is trimmed.
    inline_image_lead: bool,
    literal_colon_opener: Option<usize>,
}

impl<'a, 'l, 'o> ListScan<'a, 'l, 'o> {
    fn new(
        cur: &'a mut LineCursor<'l>,
        options: &'o Options<'o>,
        deferred: &'a mut Vec<ItemBody>,
        first_marker: ListMarker<'l>,
        ol_type: Option<OrderedListType>,
    ) -> Self {
        let base_indent = first_marker.indent;
        Self {
            cur,
            options,
            deferred,
            base_indent,
            item_shows: !invisible_item_lead(first_marker.content),
            is_task: first_marker.checked.is_some(),
            is_ordered: first_marker.ordered,
            first_dialect: ol_dialect(ol_type),
            first_marker,
            items: Vec::new(),
            tight: true,
            pending_blank: false,
            blank_run: 0,
            pending_attrs: None,
            pending_attr_slots: None,
            pending_attrs_pos: None,
            content_col: base_indent + 2,
            item_open_fence: None,
            item_paragraph_open: ParagraphOpen::Known(true),
            item_unopened_fence_span: false,
            item_unclosed_fence_base: None,
            item_rejected_fence_owner: false,
            last_item_bare_continuation: false,
            last_item_marker_attr_only: false,
        }
    }

    fn run(&mut self) {
        while let Some(line) = self.cur.peek() {
            if is_blank_line(line) {
                // A blank alone does not loosen the list; it loosens only when the
                // next line is a sibling item (handled at the marker branch via
                // pending_blank) or an indented second paragraph (below). A blank
                // before any other indented block keeps it compact (#74).
                self.pending_blank = true;
                self.blank_run += 1;
                self.cur.consume();
                continue;
            }
            if is_attached_nonparagraph_definition_placeholder(line) {
                self.pending_blank = false;
                self.blank_run = 0;
            }
            // Every nonblank line ends the preceding blank run, except a
            // comment outside every item: PART 0 S1 erases it from layout
            // (CARVE-P9-014). One inside an item is that item's content.
            let blank_run_before = if trim_ascii_start(line).starts_with("%%")
                && indent_columns(line) < self.content_col
                && !self.cur.in_item_body
            {
                self.blank_run
            } else {
                std::mem::take(&mut self.blank_run)
            };
            let flow = if self.owns_plus_marker(line) {
                self.plus_continuation(line)
            } else if let Some(marker) = detect_list_marker_full(line) {
                self.marker_line(marker, blank_run_before)
            } else {
                self.non_marker_line(line)
            };
            if flow.is_break() {
                break;
            }
        }
    }

    fn push_item(&mut self, item: ItemOpen<'l>, children: Vec<BlockNode>) {
        let pos = span_of(self.cur, item.at, self.cur.pos, self.options);
        self.items.push(ListItem {
            attrs: item.attrs,
            checked: item.marker.checked,
            task_state: item.marker.task_state,
            children,
            pos,
        });
    }

    fn place_in_last_item(&mut self, blocks: Vec<BlockNode>) {
        if let Some(last_item) = self.items.len().checked_sub(1) {
            place_item_chunk(self.deferred, last_item, blocks, Vec::new());
        }
    }

    fn drop_pending_attrs(&mut self) {
        note_unattached_block_attrs(
            self.pending_attrs_pos
                .take()
                .filter(|_| self.pending_attrs.is_some()),
        );
        self.pending_attrs = None;
    }

    fn previous_line_blank(&self) -> bool {
        self.cur.pos > 0 && is_blank_line(self.cur.lines.at(self.cur.pos - 1))
    }

    /// Physical ownership survives every enclosing collector's dedent.
    fn owns_plus_marker(&self, line: &str) -> bool {
        is_plus_marker(trim_ascii_start(line))
            && matches!(indent_columns(line), column if column == self.base_indent || column == self.content_col)
            && self
                .cur
                .continuation_decision(self.cur.pos)
                .is_some_and(|decision| decision.owner == Some(continuation_marker::Kind::List))
    }

    fn plus_continuation(&mut self, line: &str) -> ControlFlow<()> {
        let (base_indent, content_col, options) =
            (self.base_indent, self.content_col, self.options);
        let marker_column = indent_columns(line);
        self.cur.consume();
        self.pending_blank = false;
        // A marker is not the block these were written for either (§15 A4).
        self.drop_pending_attrs();
        // Only a synthetic separator can stand between an accepted `+` and its block.
        while self.cur.peek().is_some_and(is_blank_line)
            && self.cur.continuation_decision(self.cur.pos - 1).is_some()
        {
            self.cur.consume();
        }
        let attached_comment = self.cur.peek().is_some_and(is_line_comment_any_column);
        let attached = parse_continuation_block(
            self.cur,
            options,
            base_indent,
            Some(content_col),
            marker_column,
        );
        if attached.is_none()
            && self.cur.peek().is_some_and(is_plus_marker)
            && self
                .cur
                .continuation_decision(self.cur.pos)
                .is_some_and(|decision| {
                    decision.owner == Some(continuation_marker::Kind::List)
                        && self.cur.lines.line(self.cur.pos + 1).is_some_and(|line| {
                            detect_list_marker_full(line)
                                .is_some_and(|marker| marker.indent <= base_indent)
                        })
                })
        {
            let literal = MappedSource::new_line_at(
                self.cur.lines.at(self.cur.pos).to_string(),
                self.cur.source_line(self.cur.pos),
                self.cur.source_col(self.cur.pos),
            );
            self.cur.consume();
            if let Some(last_item) = self.items.len().checked_sub(1) {
                item_body(self.deferred, last_item, literal, None, false, options);
            }
        }
        if let Some(block) = attached {
            self.item_paragraph_open =
                ParagraphOpen::Known(block_ends_with_open_paragraph(Some(&block), 0, false));
            // Hoisted definitions leave no paragraph node behind (carve#801).
            let is_empty_paragraph = matches!(
                &block,
                BlockNode::Paragraph(p) if p.children.is_empty()
            );
            if !self.items.is_empty() && !is_empty_paragraph {
                // Keep the attachment after any deferred item body.
                self.place_in_last_item(vec![block]);
            }
        }
        if attached_comment
            && self.cur.peek().is_some_and(|next| {
                indent_columns(next) == base_indent
                    && line_starts_paragraph(next)
                    && !item_block_opener(next)
            })
        {
            if let Some(block) = parse_continuation_block(
                self.cur,
                options,
                base_indent,
                Some(content_col),
                base_indent,
            ) {
                self.place_in_last_item(vec![block]);
            }
        }
        if options.extensions.is_empty()
            && self
                .cur
                .peek()
                .is_some_and(|line| below_item_definition(line, base_indent, content_col))
        {
            return END;
        }
        if self.cur.peek().is_some_and(|line| {
            let indent = indent_columns(line);
            indent > base_indent && indent < content_col
        }) && !self.item_paragraph_open.get()
        {
            return END;
        }
        NEXT
    }

    fn non_marker_line(&mut self, line: &str) -> ControlFlow<()> {
        let indent = indent_columns(line);
        if indent < self.content_col
            && is_line_comment_any_column(line)
            && !line.starts_with(LAZY)
            && !self.item_paragraph_open.get()
            && (self.items.is_empty() || !self.comment_precedes_a_sibling(line))
        {
            return END;
        }
        if indent > self.base_indent {
            if let Some(flow) = self.indented_continuation(line, indent) {
                return flow;
            }
        }
        // Rule A (carve#3885): a line below the content column that no
        // branch above took closes the item. Items still group by the
        // container they open in, so a comment between two siblings keeps the
        // list; it joins the item it follows, as an invisible line.
        if self.items.is_empty() || !self.comment_precedes_a_sibling(line) {
            return END;
        }
        let last_item = self.items.len() - 1;
        let nested = MappedSource::new_line_at(
            line.to_string(),
            self.cur.source_line(self.cur.pos),
            self.cur.source_col(self.cur.pos),
        );
        let extent = self.invisible_extent(self.cur.pos, line).unwrap_or(1);
        for _ in 0..extent {
            self.cur.consume();
        }
        let children = item_body(self.deferred, last_item, nested, None, false, self.options);
        self.items[last_item].children.extend(children);
        NEXT
    }

    /// How many lines the comment at `at` covers: a closed `%%%` span is one extent.
    /// An empty span is `None`: the Djot importer writes it to split a list.
    fn invisible_extent(&self, at: usize, line: &str) -> Option<usize> {
        if let Some(open) = detect_comment_fence_line_any_column(line) {
            let mut end = at + 1;
            while let Some(next) = self.cur.lines.line(end) {
                if is_comment_fence_close_any_column(next, open.fence_len) {
                    return (end > at + 1).then_some(end + 1 - at);
                }
                end += 1;
            }
            return Some(1);
        }
        (is_line_comment_any_column(line) && !is_definition_placeholder(line)).then_some(1)
    }

    fn comment_precedes_a_sibling(&mut self, line: &str) -> bool {
        if indent_columns(line) > self.base_indent {
            return false;
        }
        let Some(extent) = self.invisible_extent(self.cur.pos, line) else {
            return false;
        };
        let mut at = self.cur.pos + extent;
        while let Some(next) = self.cur.lines.line(at) {
            if is_blank_line(next) {
                at += 1;
                continue;
            }
            if indent_columns(next) <= self.base_indent {
                if let Some(extent) = self.invisible_extent(at, next) {
                    at += extent;
                    continue;
                }
            }
            return detect_list_marker_full(next)
                .is_some_and(|marker| marker.indent == self.base_indent);
        }
        false
    }

    /// A line indented past the list's base. `None` when no item holds it yet.
    fn indented_continuation(&mut self, line: &str, indent: usize) -> Option<ControlFlow<()>> {
        let content_col = self.content_col;
        // After a blank line, lazy continuation no longer applies: a line
        // must reach the item's content column to keep belonging to it
        // (PART 9 §24 C3). A line BELOW content_column ends the list and
        // parses at document level (corpus 81-list-lazy-5). content_col
        // is the item's true column (`- `=2, `1. `=3, `10. `=4), NOT a
        // fixed base+2 -- an ordered item's body sits deeper.
        if self.pending_blank && indent < content_col {
            return Some(END);
        }
        // The item holds an OPEN fenced body and this line is below its
        // content column, so §24 closes the item and the list here
        // rather than folding the line in (#950). The collector below
        // stops on the same state and would otherwise return nothing,
        // leaving the cursor where it is.
        if self.item_open_fence.is_some() && indent < content_col && !line.starts_with(LAZY) {
            return Some(END);
        }
        // An unattached first-block marker cannot admit a lazy continuation.
        if (self.last_item_bare_continuation || self.last_item_marker_attr_only)
            && indent < content_col
        {
            return Some(END);
        }
        if self
            .items
            .last()
            .is_some_and(|item| item.children.iter().all(crate::ast::publishes_nothing))
            && indent < content_col
        {
            return Some(END);
        }
        if self.items.is_empty() {
            return None;
        }
        Some(self.continuation_chunk(line, indent))
    }

    fn continuation_chunk(&mut self, line: &str, indent: usize) -> ControlFlow<()> {
        let (content_col, options) = (self.content_col, self.options);
        let last_item = self.items.len() - 1;
        let chunk_floor = self.continuation_chunk_floor(line, indent);
        let mut nested = collect_item_continuation_block_mapped(
            self.cur,
            chunk_floor,
            content_col,
            &mut self.item_open_fence,
            options,
        );
        let collected_tail_blank = self.item_open_fence.is_some() && self.previous_line_blank();
        self.fold_continuation_chunk(&mut nested, collected_tail_blank);
        let definition_ended_paragraph = nested.source.lines().any(|line| {
            is_collected_definition_placeholder(line) || is_quoted_definition_placeholder(line)
        });
        let degraded_comment_fence = chunk_ends_in_degraded_comment_fence(self.cur, &nested);
        let closed_comment_span = chunk_ends_in_closed_comment_span(&nested);
        // Walk the chunk before peeling trailing attribute lines: the peel
        // only recognizes a flush attribute block, and its target may live in
        // the following chunk (notably a sub-list).
        walk_chunk(&mut nested);
        // Deferred: the probe re-parses the whole collected body, and
        // the body's own sub-lists probe theirs, so an eager probe here
        // parsed a ladder's body twice per level.
        let probed = nested.clone();
        let below = last_consumed_line_below_column(self.cur, content_col);
        self.item_paragraph_open = ParagraphOpen::deferred(move || {
            nested_ends_with_open_paragraph_masked(&probed, below, below, options)
        });
        let (nested_children, carried) =
            self.parse_continuation_chunk(nested, collected_tail_blank);
        place_item_chunk(self.deferred, last_item, nested_children, carried);
        if self.cur.peek().is_some_and(|line| {
            let indent = indent_columns(line);
            indent > self.base_indent && indent < content_col
        }) && !self.item_paragraph_open.get()
        {
            return END;
        }
        // A collected definition is an I5 block, not the comment
        // exception. If the collector stopped on a nonzero line
        // below the item's content column, no paragraph remains
        // for that line to continue (markup-carve/carve#1376).
        if (definition_ended_paragraph || degraded_comment_fence || closed_comment_span)
            && self.cur.peek().is_some_and(|line| {
                let indent = indent_columns(line);
                indent > 0 && indent < content_col
            })
        {
            return END;
        }
        NEXT
    }

    // A BLOCK THAT INTERRUPTED THE ITEM'S LEAD PARAGRAPH ENDS THE
    // ITEM WHEN IT LEAVES NOTHING OPEN, the way a marker-line block
    // that leaves nothing open does (carve-rs#2127). There the
    // block stands on the marker line; here it was collected as a
    // chunk, and the collection took the band line before anything
    // could ask (carve-rs#2128). The FLOOR is what asks: raised,
    // the line is left for the fold below, which hands it back when
    // the chunk holds an open paragraph, and the end-of-list check
    // already under the fold reads what is left.
    //
    // A COMMENT IS EXEMPT (PART 9 SS24 C3). It renders nothing at
    // any column, so it closed nothing and the lead paragraph is
    // still there for the band line to continue. Any column, too -
    // the lead may be written PAST the content column and the slice
    // takes only the column off it (corpus 446-7).
    //
    // The lead must be AT or PAST the column. A band line that
    // opens the chunk itself sits under the raised floor, so the
    // collector would return nothing and leave the cursor put.
    fn continuation_chunk_floor(&self, line: &str, indent: usize) -> usize {
        let content_col = self.content_col;
        let lead_at_column = slice_columns(line, content_col.min(indent), false);
        // Probe a shifted opener at its authored base.
        let lead_for_open =
            if indent > content_col && item_block_opener(trim_ascii_start(&lead_at_column)) {
                trim_ascii_start(&lead_at_column)
            } else {
                &lead_at_column
            };
        let lead_is_a_comment = detect_comment_fence_line_any_column(&lead_at_column).is_some()
            || is_line_comment_any_column(&lead_at_column);
        if indent >= content_col
            && !lead_is_a_comment
            && a_band_line_ends_the_chunk(self.cur, self.base_indent, content_col)
            && !chunk_run_leaves_content_open(self.cur, content_col, lead_for_open, self.options)
        {
            content_col.saturating_sub(1)
        } else {
            self.base_indent
        }
    }

    fn fold_continuation_chunk(&mut self, nested: &mut MappedSource, collected_tail_blank: bool) {
        let (base_indent, content_col, options) =
            (self.base_indent, self.content_col, self.options);
        if self.item_open_fence.is_some() {
            let mut admitted = nested.clone();
            walk_chunk(&mut admitted);
            if body_ends_with_open_paragraph(&admitted.source, options) {
                self.item_open_fence = None;
                self.item_rejected_fence_owner = true;
            }
        }
        let latest_owner = Cell::new(None);
        if self.item_rejected_fence_owner {
            let mut admitted = nested.clone();
            walk_chunk(&mut admitted);
            latest_owner.set(Some(collected_body_takes_the_lazy_line(
                &admitted.source,
                last_consumed_line_below_column(self.cur, content_col),
                options,
            )));
        }
        if collected_tail_blank
            || !(self.item_open_fence.is_none()
                || self.cur.peek().is_some_and(|line| line.starts_with(LAZY)))
        {
            return;
        }
        let item_open_fence = &mut self.item_open_fence;
        fold_lazy_run_and_resume_in_band(
            self.cur,
            nested,
            content_col,
            (content_col.saturating_sub(1), base_indent),
            (self.item_rejected_fence_owner, options),
            |src, below| {
                if let Some(open) = latest_owner.get() {
                    return open;
                }
                let mut rebased = src.clone();
                walk_chunk(&mut rebased);
                collected_body_takes_the_lazy_line(&rebased.source, below, options)
            },
            |cur| {
                let resumed = collect_item_continuation_block_mapped(
                    cur,
                    base_indent,
                    content_col,
                    item_open_fence,
                    options,
                );
                if latest_owner.get().is_some() {
                    let mut admitted = resumed.clone();
                    walk_chunk(&mut admitted);
                    let paragraph = body_ends_with_open_paragraph(&admitted.source, options);
                    if paragraph {
                        *item_open_fence = None;
                    }
                    latest_owner.set(Some(paragraph));
                }
                resumed
            },
        );
    }

    fn parse_continuation_chunk(
        &mut self,
        mut nested: MappedSource,
        collected_tail_blank: bool,
    ) -> (Vec<BlockNode>, Vec<PendingBody>) {
        let mut split_stack: Vec<(Attrs, Option<Pos>)> = Vec::new();
        while let Some(split) = split_trailing_attrs(&mut nested) {
            split_stack.push(split);
        }
        let mut carried = Vec::new();
        let (mut nested_children, nested_loose) =
            parse_item_chunk(&nested, self.options, &mut carried, self.item_shows);
        self.carry_chunk_attrs(&mut nested_children, split_stack);
        self.chunk_looseness(&nested_children, nested_loose, collected_tail_blank);
        (nested_children, carried)
    }

    fn carry_chunk_attrs(
        &mut self,
        nested_children: &mut [BlockNode],
        split_stack: Vec<(Attrs, Option<Pos>)>,
    ) {
        // Attributes held from an EARLIER chunk attach here when
        // this one opens with a block, which is the case where a
        // blank line sits between the `{…}` and its target. The
        // sub-list branch is the other consumer; between them, a set
        // of attributes reaches the first block written after it
        // whichever chunk that landed in.
        if self.pending_attrs.is_some() {
            if let Some(target) = nested_children
                .iter_mut()
                .find(|block| !matches!(block, BlockNode::Comment(_)))
            {
                apply_attrs_to_block(target, self.pending_attrs.take().unwrap());
                self.pending_attrs_pos = None;
            }
        }
        // Back into SOURCE order, so the merge resolves a repeated
        // key the way `parse_blocks` resolves it at top level.
        for (attrs, pos) in split_stack.into_iter().rev() {
            if self.pending_attrs.is_none() {
                self.pending_attrs_pos = pos;
            }
            merge_attrs_indexed(&mut self.pending_attrs, attrs, &mut self.pending_attr_slots);
        }
    }

    fn chunk_looseness(
        &mut self,
        nested_children: &[BlockNode],
        nested_loose: bool,
        collected_tail_blank: bool,
    ) {
        // A blank before an indented sub-block loosens only when it
        // is a genuine second paragraph (#74 compact list blocks).
        // Skip what renders NOTHING when looking for that
        // paragraph. An invisible line does not cancel the blank
        // line above it (PART 9 §17 L1b, markup-carve/carve#630):
        // `- a` / blank / `  %% n` / `  text` holds a second
        // paragraph with a comment in front of it, and reading only
        // the FIRST child found the comment and called the item
        // tight. Matches carve-js.
        let first_visible = nested_children
            .iter()
            .find(|block| !matches!(block, BlockNode::Comment(_)));
        let blank_can_loosen = !self.item_unopened_fence_span
            || self
                .item_unclosed_fence_base
                .is_some_and(|base| base <= self.content_col);
        let shown_before = self.item_shows;
        self.item_shows |= !nested_children.iter().all(block_renders_nothing);
        if self.pending_blank
            && blank_can_loosen
            && shown_before
            && first_visible.is_some_and(block_is_paragraph_shaped)
        {
            self.tight = false;
        }
        // A blank ABSORBED inside the collected continuation (e.g. a
        // fence/div/table followed by a blank and then trailing text)
        // loosens the item when a plain paragraph follows the blank
        // (§17 L1). The outer `pending_blank` only sees a blank BEFORE
        // this chunk, so this covers the blank-after / blank-both case.
        if blank_can_loosen && nested_loose {
            self.tight = false;
        }
        // An INVISIBLE continuation is not the item's second block
        // (§17 L2 - it renders nothing), but it does not consume the
        // blank either: the blank still sits between this item and
        // whatever follows, and a blank before a SIBLING loosens the
        // list. Clearing it here made `- a` / blank / `  %% note` /
        // `- b` tight, where carve-js and the corpus have it loose
        // (carve-rs#557, corpus 87-compact-list-blocks-6).
        let renders_nothing = nested_children.iter().all(block_renders_nothing);
        if !renders_nothing {
            self.pending_blank = collected_tail_blank
                || (self.item_open_fence.is_some() && self.previous_line_blank());
            if self.pending_blank {
                self.blank_run = self
                    .cur
                    .lines
                    .range(0..self.cur.pos)
                    .rev()
                    .take(3)
                    .take_while(|line| is_blank_line(line))
                    .count();
            }
        }
    }

    fn marker_line(&mut self, marker: ListMarker<'l>, blank_run_before: usize) -> ControlFlow<()> {
        if marker.indent < self.base_indent {
            return END;
        }
        if marker.indent > self.base_indent {
            return self.sublist_marker(marker.indent);
        }
        self.sibling_item(marker, blank_run_before)
    }

    /// A marker indented past the base nests as a sub-list. (An ordered
    /// marker BELOW the content column never reaches here -- it folds
    /// into the item paragraph in the per-item loop below, §10. Unordered
    /// and task markers always interrupt, so they nest at any indent.)
    fn sublist_marker(&mut self, sub_indent: usize) -> ControlFlow<()> {
        let content_col = self.content_col;
        if self.pending_blank && sub_indent < content_col {
            return END;
        }
        // The same fenced-body guard the non-marker branch applies. A
        // MARKER below the content column is a line below the content
        // column, and §24 stops at the ITEM for both: S1 walks the stack by
        // the indentation a line supplies, which has nothing to do with
        // what the line says. Whether the residue is a sub-list or prose is
        // decided in the surviving context, AFTER the containers close - so
        // reading the marker first nested it inside the very item the open
        // fence closes (markup-carve/carve#950).
        if (self.item_open_fence.is_some()
            || self.last_item_bare_continuation
            || self.last_item_marker_attr_only
            || !self.item_paragraph_open.get())
            && sub_indent < content_col
        {
            return END;
        }
        if self.items.is_empty() {
            return END;
        }
        let last_item = self.items.len() - 1;
        let chunk = self.collect_sublist_chunk(sub_indent);
        self.place_sublist_chunk(last_item, sub_indent, chunk)
    }

    fn collect_sublist_chunk(&mut self, sub_indent: usize) -> SublistChunk {
        let (base_indent, content_col, options) =
            (self.base_indent, self.content_col, self.options);
        let is_ordered = self.is_ordered;
        let cur = &mut *self.cur;
        // Keep the collector's fence boundary when a descendant's body
        // ends below this item's column. A fresh chunk must not reclaim it.
        let mut stopped_fence = None;
        let mut nested_columns = NestedCollectionState::default();
        // A line in this item's band is left for the fold, which takes
        // it only into an open paragraph (CARVE-P0-009).
        let band_line_ends_the_chunk = sub_indent >= content_col
            && a_band_line_ends_the_chunk(cur, base_indent, content_col)
            && !chunk_keeps_its_band_line(cur, content_col);
        let (floor, lazy_scope) = if band_line_ends_the_chunk {
            (content_col - 1, (content_col - 1, base_indent))
        } else {
            (base_indent, (0, 0))
        };
        let mut nested = collect_indented_block_mapped_with_columns(
            cur,
            floor,
            content_col,
            false,
            &mut stopped_fence,
            false,
            &mut nested_columns,
            options,
        );
        let descendant_text_after_fence = nested_columns.fence_seen
            && !is_ordered
            && cur.pos > 0
            && cur
                .peek()
                .is_some_and(|next| line_starts_paragraph(next) && !item_block_opener(next))
            && cur.lines.line(cur.pos - 1).is_some_and(|prior| {
                let indent = indent_columns(prior);
                indent >= content_col + 2
                    && line_starts_paragraph(trim_ascii_start(prior))
                    && !nested_columns.tabbed_marker
            });
        let nested_bare_ends_item = nested_bare_marker_then_plain(cur, content_col);
        // A flush lazy run keeps folding band lines below it (PART 1 S4).
        let lazy_scope = if lazy_scope == (0, 0)
            && cur
                .peek()
                .is_some_and(|line| !is_blank_line(line) && indent_columns(line) == 0)
        {
            (content_col - 1, base_indent)
        } else {
            lazy_scope
        };
        let closed_marker_comment_span = nested_columns.closed_marker_comment_span;
        if stopped_fence.is_none()
            && !descendant_text_after_fence
            && !nested_bare_ends_item
            && !closed_marker_comment_span
            && !chunk_ends_in_closed_comment_span(&nested)
        {
            fold_lazy_run_and_resume_in_band(
                cur,
                &mut nested,
                content_col,
                lazy_scope,
                (false, options),
                |src, below| collected_body_takes_the_lazy_line(&src.source, below, options),
                |cur| {
                    // PART 9 §10 I2 keeps the paragraph owner across a collection restart.
                    nested_columns.lazy_paragraph_owner = cur
                        .pos
                        .checked_sub(1)
                        .filter(|&at| {
                            let prior = cur.lines.at(at);
                            line_starts_paragraph(trim_ascii_start(prior))
                                && !item_block_opener(trim_ascii_start(prior))
                                && cur.continuation_decision(at).is_none()
                        })
                        .and_then(|_| nested_columns.last().map(|&(_, column)| column));
                    collect_indented_block_mapped_with_columns(
                        cur,
                        sub_indent - 1,
                        content_col,
                        false,
                        &mut stopped_fence,
                        false,
                        &mut nested_columns,
                        options,
                    )
                },
            );
        }
        SublistChunk {
            nested,
            closed_marker_comment_span,
            stopped_fence: stopped_fence.is_some(),
            descendant_text_after_fence,
            nested_bare_ends_item,
            band_line_ends_the_chunk,
        }
    }

    fn place_sublist_chunk(
        &mut self,
        last_item: usize,
        sub_indent: usize,
        chunk: SublistChunk,
    ) -> ControlFlow<()> {
        let (base_indent, content_col, options) =
            (self.base_indent, self.content_col, self.options);
        let SublistChunk {
            mut nested,
            closed_marker_comment_span,
            stopped_fence,
            descendant_text_after_fence,
            nested_bare_ends_item,
            band_line_ends_the_chunk,
        } = chunk;
        let nested_sublist = sub_indent >= content_col;
        let closed_comment_span =
            closed_marker_comment_span || chunk_ends_in_closed_comment_span(&nested);
        if nested_sublist && nested.outer_plus_after_blank {
            self.tight = false;
        }
        // A code fence can start a new chunk after an attribute line. Lift the
        // trailing run before deferring this sublist, or its nested reader will
        // drop the run as dangling instead of leaving it for the next block.
        let mut trailing_attrs = Vec::new();
        while let Some(split) = split_trailing_attrs(&mut nested) {
            trailing_attrs.push(split);
        }
        let held = self.pending_attrs.take();
        if held.is_some() {
            self.pending_attrs_pos = None;
        }
        for (attrs, pos) in trailing_attrs.into_iter().rev() {
            if self.pending_attrs.is_none() {
                self.pending_attrs_pos = pos;
            }
            merge_attrs_indexed(&mut self.pending_attrs, attrs, &mut self.pending_attr_slots);
        }
        if self
            .cur
            .peek()
            .is_some_and(|line| !is_blank_line(line) && indent_columns(line) < content_col)
        {
            self.item_paragraph_open =
                ParagraphOpen::Known(nested_ends_with_open_paragraph_masked(
                    &nested,
                    last_consumed_line_below_column(self.cur, content_col),
                    last_consumed_line_below_column(self.cur, content_col),
                    options,
                ));
        }
        let nested_children = item_body_with_layout_suppression(
            self.deferred,
            last_item,
            nested,
            held,
            false,
            false,
            options,
        );
        // The blank BEFORE this sub-list is consumed by it and must not
        // survive to loosen a later sibling marker (§17 L2: a blank
        // before an item's sub-block keeps the item tight). Without
        // this, `- a` / blank / `  - b` / `- c` loosened at `- c`
        // because pending_blank leaked past the sub-list, while the same
        // blank before a plain continuation block cleared it below.
        // Matches carve-js / carve-php (carve-rs#286). A blank AFTER the
        // sub-list still re-raises pending_blank in the blank branch, so
        // a genuine blank BETWEEN items keeps loosening.
        self.pending_blank = false;
        self.items[last_item].children.extend(nested_children);
        if closed_comment_span
            && self
                .cur
                .peek()
                .is_some_and(|line| !is_blank_line(line) && indent_columns(line) < content_col)
        {
            return END;
        }
        if nested_bare_ends_item {
            return END;
        }
        let below_column = self
            .cur
            .peek()
            .is_some_and(|line| !is_blank_line(line) && indent_columns(line) < content_col);
        if descendant_text_after_fence && below_column {
            return END;
        }
        if stopped_fence
            && below_column
            && self
                .cur
                .peek()
                .is_some_and(|line| detect_list_marker_full(line).is_none())
        {
            return END;
        }
        if band_line_ends_the_chunk
            && self.cur.peek().is_some_and(|line| {
                let indent = indent_columns(line);
                !is_blank_line(line) && indent > base_indent && indent < content_col
            })
        {
            return END;
        }
        NEXT
    }

    /// Past the sub-list branch, so this marker opens a SIBLING item.
    fn sibling_item(&mut self, marker: ListMarker<'l>, blank_run_before: usize) -> ControlFlow<()> {
        // Any attributes still held were written in front of something that
        // never came, and a sibling is not that something - drop them here
        // rather than let them reach a sub-list further down the list, which is
        // not where the author put them (carve-rs#1007). Reported rather than
        // silent, like every other way of running out (§15 A4,
        // markup-carve/carve#1281).
        self.drop_pending_attrs();
        if !self.sibling_joins_list(&marker, blank_run_before) {
            return END;
        }
        if self.pending_blank || (!self.items.is_empty() && self.previous_line_blank()) {
            self.tight = false;
            self.pending_blank = false;
        }
        self.content_col = self.item_content_col(&marker);
        self.item_shows = !invisible_item_lead(marker.content);
        // A new item carries none of the previous item's fence state.
        self.item_open_fence = None;
        self.item_unopened_fence_span = false;
        self.item_unclosed_fence_base = None;
        self.item_rejected_fence_owner = false;
        let source_line = self.cur.source_line(self.cur.pos);
        let at = self.cur.pos;
        self.cur.consume();
        // Every item answers this for itself; only a bare-`+` lead sets it.
        self.last_item_bare_continuation = false;
        self.last_item_marker_attr_only = false;
        let attrs = source_line_attrs(marker.attrs.clone(), source_line, self.options);
        let content = marker.content;
        let item = ItemOpen {
            marker,
            attrs,
            at,
            source_line,
        };
        // A tab after the marker's space is content. Structural readers that
        // skip leading whitespace must not see a second opener behind it.
        if content.starts_with('\t') {
            self.paragraph_lead_item(item);
            return NEXT;
        }
        // First-block form `- +` (grammar §17): a lone `+` as the marker
        // content means the item's first block is the following flush-left
        // block (no inline paragraph).
        if is_plus_marker(content) && self.cur.continuation_decision(item.at).is_some() {
            self.bare_continuation_item(item);
            return NEXT;
        }
        // When the item's content BEGINS, on the marker line, with another list
        // marker (`- - A`, `* - A`, `1. - A`, ...), the lead is itself a
        // sub-list, not a paragraph. Parse the lead together with every
        // following dedented line as ONE block stream so the marker-line
        // sub-list behaves exactly like a sub-list opened on a *following* line:
        // following same-indent markers MERGE into it as siblings, and
        // post-blank indented blocks are ABSORBED into its items. This MATCHES
        // reference djot.js (@djot/djot 0.3.2) and CommonMark, which both treat
        // a marker-line sub-list as a normal nested list. It corrects Carve's
        // prior line-scoping (which split the sub-list from following items and
        // leaked later indented blocks to the parent row) -- a bug inherited
        // from djot-php, whose marker-line handling deviates from reference
        // djot. The combined stream reuses the normal nested-list/absorption
        // logic (collect_indented_block + recursive parse) -- no separate path.
        if strip_blockquote_prefix(content).is_some() {
            return self.quote_lead_item(item);
        }
        // Braces ALONE on the marker line are a block-attribute line for the
        // item's first block, not lead text (corpus 170). `- {a=b .c}` +
        // `  # H` therefore attributes the heading, exactly as the same two
        // lines do at document level. Braces followed by TEXT stay literal
        // (`- {.c} literal text`), which is why the whole content must parse as
        // a standalone attribute line. Routed through the block stream so the
        // pending-attribute machinery, not this lead-paragraph path, sees it.
        if parse_standalone_attrs(content).is_some()
            || marker_content_is_attr_block(content, self.cur, self.content_col)
        {
            self.attr_lead_item(item);
            return NEXT;
        }
        if detect_list_marker_full(content).is_some() {
            return self.sublist_lead_item(item);
        }
        let content_col = self.content_col;
        if marker_content_starts_block(content, self.cur, content_col)
            || marker_content_is_captioned_inline(content, self.cur, content_col, self.options)
        {
            return self.block_lead_item(item);
        }
        self.paragraph_lead_item(item);
        NEXT
    }

    fn sibling_joins_list(&self, marker: &ListMarker<'_>, blank_run_before: usize) -> bool {
        if marker.ordered != self.is_ordered || marker.checked.is_some() != self.is_task {
            return false;
        }
        if !self.is_ordered && marker.marker != self.first_marker.marker {
            return false;
        }
        // §11: an ordered item whose delimiter (`.` vs `)`) or dialect family
        // (decimal / alpha / roman, case included) differs from the list's first
        // item starts a NEW sibling list. Skip the FIRST item: its own detected
        // dialect may differ from the list's resolved (tie-broken) dialect
        // (`v.` detects alpha but the list is roman), and it can never split
        // from itself.
        if self.is_ordered
            && !self.items.is_empty()
            && (marker.delim != self.first_marker.delim
                || !dialect_compatible(self.first_dialect, marker))
        {
            return false;
        }
        // PART 9 §11 N1a: a run of THREE OR MORE blank lines before a
        // compatible sibling marker is a HARD BOUNDARY. The marker opens a new
        // sibling list rather than joining this one, so leave it unconsumed
        // for the surrounding block parser.
        //
        // One blank line stays the loose separator, and so does two (§17 L1) --
        // three is the threshold precisely because the runs documents already
        // hold, changelog spacing and generator output, are runs of two.
        //
        // The run closes nothing on its own. It is not an item terminator: a
        // continuation at the item's content column still continues the item at
        // any run length, which is why this sits on the SIBLING MARKER path and
        // nowhere else.
        blank_run_before < 3
    }

    // This item's content column. For ordered/unordered it is where the
    // marker content begins (`- `=2, `1. `=3, `10. `=4). For a TASK the
    // checkbox is content, not marker, so the column stops before the box
    // (`- [x] `=2, `-   [x] `=4, CARVE-P9-083).
    //
    // An abutting attribute block is item metadata and contributes zero,
    // so changing its spelling or Unicode value cannot move the body
    // (markup-carve/carve#1701, #1698).
    fn item_content_col(&self, _marker: &ListMarker<'_>) -> usize {
        let l = self.cur.peek().unwrap();
        marker_content_col(l).expect("current item marker")
    }

    fn bare_continuation_item(&mut self, item: ItemOpen<'l>) {
        let (base_indent, content_col, options) =
            (self.base_indent, self.content_col, self.options);
        self.last_item_bare_continuation = true;
        let cur = &mut *self.cur;
        let mut children = Vec::new();
        // A first-block marker takes the opener's column or document column zero.
        let first_block_attaches = cur
            .continuation_decision(item.at)
            .is_some_and(|decision| decision.owner == Some(continuation_marker::Kind::List));
        // Only a synthetic separator can stand between an accepted `+` and its block.
        while first_block_attaches && cur.peek().is_some_and(is_blank_line) {
            cur.consume();
        }
        let attached_comment = cur.peek().is_some_and(is_line_comment_any_column);
        let pull_indent = cur
            .continuation_decision(item.at)
            .filter(|decision| decision.quote_depth > 0)
            .map_or(base_indent, |_| content_col);
        if let Some(block) = first_block_attaches
            .then(|| parse_continuation_block(cur, options, base_indent, None, pull_indent))
            .flatten()
        {
            let is_empty_paragraph = matches!(
                &block,
                BlockNode::Paragraph(p) if p.children.is_empty()
            );
            if !is_empty_paragraph {
                children.push(block);
            }
        }
        if attached_comment
            && cur
                .peek()
                .is_some_and(|line| indent_columns(line) == 0 && line_starts_paragraph(line))
        {
            if let Some(block) =
                parse_continuation_block(cur, options, base_indent, None, pull_indent)
            {
                children.push(block);
            }
        }
        // The item runs from its marker line through the block the `+`
        // attached - it was the only list item built with a hardcoded
        // `None`, so an item written this way had no span while its
        // siblings and its own contents did.
        self.push_item(item, children);
    }

    fn quote_lead_item(&mut self, item: ItemOpen<'l>) -> ControlFlow<()> {
        let (base_indent, content_col, options) =
            (self.base_indent, self.content_col, self.options);
        let content = item.marker.content;
        let mut stream = item_marker_source(self.cur, content, item.at);
        let fence_after_quote = self.fence_after_quote();
        // AN EMPTY QUOTE ON THE MARKER LINE HOLDS NO PARAGRAPH. A quote
        // has no closer, so an empty one is finished where it stands, and
        // PART 1 S4's otherwise has already ended the item - a line in the
        // band between the list's base and the item's content column has
        // nothing here to continue and belongs to the document. `. > a`
        // folds as before, because there a paragraph really is open. The
        // collection floor is what says so, as it does for a definition
        // collected off the marker line (markup-carve/carve-rs#2096).
        // A fence or a colon container written inside the quote is still
        // OPEN on one line, and its payload is exactly the band line below
        // it, so only a quote that leaves nothing at all open ends the item.
        let mut innermost = content;
        while let Some(rest) = strip_blockquote_prefix(innermost) {
            innermost = rest;
        }
        let quote_holds_a_paragraph = (!innermost.trim().is_empty()
            && body_ends_with_open_paragraph(content, options))
            || colon_fences_left_open(content) > 0
            || detect_fence_open(innermost).is_some()
            || detect_comment_fence_line(innermost).is_some();
        let next_marker_indent = self
            .cur
            .peek()
            .and_then(detect_list_marker_full)
            .map(|next| next.indent);
        let below_empty_quote_marker = !quote_holds_a_paragraph
            && next_marker_indent.is_some_and(|indent| indent < content_col.saturating_sub(1));
        let nested_marker_at_floor = !quote_holds_a_paragraph
            && next_marker_indent.is_some_and(|indent| indent == content_col.saturating_sub(1));
        // With no line left in this body to take, the `+` stays text (carve#3421).
        let first_block_quote_attaches = is_plus_marker(innermost)
            && self.cur.peek().is_some()
            && self
                .cur
                .continuation_decision(item.at)
                .is_some_and(|decision| decision.owner == Some(continuation_marker::Kind::Quote));
        let continuation_has_content = if first_block_quote_attaches {
            self.quote_first_block(&mut stream, content, innermost, item.at)
        } else if self.quote_plus_takes_paragraph(innermost) {
            self.quote_plus_paragraph(&mut stream);
            true
        } else if !below_empty_quote_marker {
            self.quote_continuation(
                &mut stream,
                quote_holds_a_paragraph || nested_marker_at_floor,
                nested_marker_at_floor,
                fence_after_quote,
            )
        } else {
            false
        };
        let after_blank = self.previous_line_blank();
        if quote_holds_a_paragraph || continuation_has_content {
            // The collector stopped short of a band line only for a quote that holds no
            // paragraph; a paragraph written after that quote still takes it lazily.
            let lazy_column = if quote_holds_a_paragraph {
                base_indent
            } else {
                content_col.saturating_sub(1)
            };
            fold_lazy_run_and_resume_in_band(
                self.cur,
                &mut stream,
                content_col,
                (lazy_column, base_indent),
                (false, options),
                |src, below| {
                    !after_blank
                        && (nested_ends_with_open_paragraph_masked(src, below, below, options)
                            || src
                                .source
                                .lines()
                                .last()
                                .is_some_and(|line| caption_content(line).is_some()))
                },
                |cur| collect_indented_block_mapped(cur, base_indent, content_col, options),
            );
        }
        // A blank line between the item's blocks loosens the list, whatever
        // the marker-line lead happens to be. This branch and the two beside
        // it build their item and `continue` past the loosening test the
        // normal path runs, so `- {a=b}` / `x` / blank / `Body.` stayed tight
        // where `- x` / blank / `Body.` went loose - on the same blank line
        // (carve-rs#476).
        rebase_quoted_fence_payload(&mut stream, innermost);
        let children = item_body(
            self.deferred,
            self.items.len(),
            stream,
            None,
            false,
            options,
        );
        // The item runs from its marker to the last line its body
        // consumed - the bullet is part of the item, unlike the
        // paragraph inside it, which starts at the text.
        self.push_item(item, children);
        if !quote_holds_a_paragraph
            && self.cur.peek().is_some_and(|line| {
                let indent = indent_columns(line);
                !is_blank_line(line)
                    && trim_ascii(line) != "+"
                    && indent > base_indent
                    && indent < content_col
                    && (detect_list_marker_full(line).is_none()
                        || indent < content_col.saturating_sub(1))
            })
        {
            return END;
        }
        NEXT
    }

    /// Decide the fence while the outer item still exposes its closer.
    /// If a below-column line ends the item first, the inner reparse
    /// cannot see that closer. The synthetic boundary preserves the
    /// block decision without moving the closer into the item.
    fn fence_after_quote(&self) -> bool {
        let content_col = self.content_col;
        self.cur.peek().is_some_and(|line| {
            indent_columns(line) >= content_col
                && detect_fence_open(&slice_columns(line, content_col, false)).is_some()
        })
    }

    /// The quote's `+` attaches the block below it; returns whether any
    /// line was taken.
    fn quote_first_block(
        &mut self,
        stream: &mut MappedSource,
        content: &str,
        innermost: &str,
        item_at: usize,
    ) -> bool {
        let (base_indent, content_col, options) =
            (self.base_indent, self.content_col, self.options);
        let cur = &mut *self.cur;
        let start = cur.pos;
        // An attached fence with no closer ends with the item (carve#4018).
        let attached_fence = detect_fence_open(trim_ascii_start(cur.lines.at(start))).is_some();
        let end = attached_indented_block_end(cur.lines, start, content_col, options)
            .unwrap_or_else(|| {
                attached_block_lines_classified(
                    cur.lines,
                    start,
                    &mut cur.comment_closer_last_index,
                    &mut cur.code_closer_last_index,
                    options,
                    &mut |line, at| {
                        at > start
                            && (detect_list_marker_full(line)
                                .is_some_and(|marker| marker.indent <= base_indent)
                                || attached_fence
                                    && !is_blank_line(line)
                                    && indent_columns(line) < content_col)
                    },
                )
            });
        // A quote follower at an eligible raw column opens a block in the
        // innermost quote, which owns the `+` (`- > > +` / `> q` nests three
        // deep, carve#3421): it takes the `+` line's place behind its prefix.
        let quote_prefix = &content[..content.len() - innermost.len()];
        let quote_column = marker_line_child_column(cur, content, item_at, content_col);
        let follower_indent = cur.lines.line(start).map_or(0, indent_columns);
        // The innermost `>` owns the `+`, so a block at its column opens in that quote.
        let innermost_marker_column = quote_prefix
            .trim_end()
            .rfind('>')
            .map(|at| quote_column + at);
        let quote_follower = !quote_prefix.is_empty()
            && end > start
            && (strip_blockquote_prefix(trim_ascii_start(cur.lines.at(start))).is_some()
                || quote_prefix.len() > 2 && Some(follower_indent) == innermost_marker_column);
        if quote_follower {
            // The marker line keeps opening the quotes; only its `+` goes.
            let stripped = cur
                .lines
                .line(item_at)
                .and_then(|line| stripped_col(cur.source_col(item_at), line, content));
            *stream = MappedSource::new_line_at(
                trim_ascii_end(quote_prefix).to_string(),
                cur.source_line(item_at),
                stripped,
            );
            stream.item_marker_at_start = true;
            stream.in_quoted_body = cur.in_quoted_body;
        }
        while quote_follower && cur.pos < end {
            let line = cur.lines.at(cur.pos);
            let (body, consumed, synthetic) = slice_columns_mapped(line, follower_indent, true);
            // The follower carries the prefixes of the quotes around the host.
            let mut carried = body.as_str();
            for _ in 1..quote_prefix.matches('>').count() {
                carried = strip_blockquote_prefix(trim_ascii_start(carried)).unwrap_or(carried);
            }
            stream.push_newline_at(
                format!("{quote_prefix}{carried}"),
                cur.source_line(cur.pos),
                cur.source_col(cur.pos).map(|col| {
                    col + consumed as isize - synthetic as isize
                        + (body.chars().count() - carried.chars().count()) as isize
                        - quote_prefix.chars().count() as isize
                }),
            );
            cur.consume();
        }
        while cur.pos < end {
            let line = cur.lines.at(cur.pos);
            // A task's quote opens past its checkbox, and owns a follower at its column.
            let column = if indent_columns(line) >= quote_column {
                quote_column
            } else {
                content_col
            };
            let (body, consumed, synthetic) = slice_columns_mapped(line, column, true);
            stream.push_newline_at(
                body,
                cur.source_line(cur.pos),
                cur.source_col(cur.pos)
                    .map(|col| col + consumed as isize - synthetic as isize),
            );
            cur.consume();
        }
        end > start
    }

    fn quote_plus_takes_paragraph(&self, innermost: &str) -> bool {
        let content_col = self.content_col;
        detect_list_marker_full(innermost)
            .is_some_and(|inner| is_plus_marker(innermost_marker_content(inner.content)))
            && self.cur.peek().is_some_and(|line| {
                let text = trim_ascii_start(line);
                indent_columns(line) < content_col
                    && strip_blockquote_prefix(text).is_none()
                    && detect_list_marker_full(text).is_none()
                    && line_starts_paragraph(text)
            })
    }

    /// A list `+` in the marker line's quote takes nothing below it: the quote
    /// has ended, and the paragraph line joins this item (carve#3421).
    fn quote_plus_paragraph(&mut self, stream: &mut MappedSource) {
        let (content_col, options) = (self.content_col, self.options);
        let cur = &mut *self.cur;
        let start = cur.pos;
        let end = attached_block_lines_classified(
            cur.lines,
            start,
            &mut cur.comment_closer_last_index,
            &mut cur.code_closer_last_index,
            options,
            &mut |next, _| is_blank_line(next) || is_plus_marker(next),
        )
        .max(start + 1);
        while cur.pos < end {
            let line = cur.lines.at(cur.pos);
            let (body, consumed, synthetic) = slice_columns_mapped(line, content_col, true);
            stream.push_newline_at(
                body,
                cur.source_line(cur.pos),
                cur.source_col(cur.pos)
                    .map(|col| col + consumed as isize - synthetic as isize),
            );
            cur.consume();
        }
    }

    fn quote_continuation(
        &mut self,
        stream: &mut MappedSource,
        floor_at_base: bool,
        nested_marker_at_floor: bool,
        fence_after_quote: bool,
    ) -> bool {
        let content_col = self.content_col;
        let continuation = collect_indented_block_mapped(
            self.cur,
            if floor_at_base {
                self.base_indent
            } else {
                content_col.saturating_sub(1)
            },
            if nested_marker_at_floor {
                content_col.saturating_sub(1)
            } else {
                content_col
            },
            self.options,
        );
        if fence_after_quote && !continuation.source.is_empty() {
            stream.push_newline_at(String::new(), None, None);
        }
        let has_content = !continuation.source.trim().is_empty();
        stream.append(continuation);
        has_content
    }

    fn attr_lead_item(&mut self, item: ItemOpen<'l>) {
        let (content_col, options) = (self.content_col, self.options);
        self.last_item_marker_attr_only = true;
        let mut stream = item_marker_source(self.cur, item.marker.content, item.at);
        let attribute_lines =
            standalone_attrs_block_len_iter(std::iter::once(item.marker.content).chain(
                (self.cur.pos..self.cur.lines.len()).map_while(|at| self.cur.lines.line(at)),
            ))
            .unwrap_or(1);
        for _ in 1..attribute_lines {
            let line = self.cur.peek().unwrap();
            let (body, consumed) = if indent_columns(line) >= content_col {
                let (body, consumed, _) = slice_columns_mapped(line, content_col, false);
                (body, consumed)
            } else {
                (trim_ascii_start(line).to_string(), leading_ws(line))
            };
            stream.push_newline_at(
                body,
                self.cur.source_line(self.cur.pos),
                self.cur
                    .source_col(self.cur.pos)
                    .map(|col| col + consumed as isize),
            );
            self.cur.consume();
        }
        // The marker-line attribute opens no paragraph. Only a following
        // line that reaches the item's content column can be its target;
        // a genuinely below-column line closes the item under PART 1 S4
        // and is reclassified by the surrounding document. Marker-attached
        // item metadata and task checkboxes do not widen that column
        // (markup-carve/carve#1705, carve-rs#1373).
        stream.append(collect_indented_block_mapped(
            self.cur,
            content_col.saturating_sub(1),
            content_col,
            options,
        ));
        self.fold_continuation_chunk(&mut stream, false);
        let probed = stream.source.clone();
        self.item_paragraph_open =
            ParagraphOpen::deferred(move || body_ends_with_open_paragraph(&probed, options));
        let children = item_body(
            self.deferred,
            self.items.len(),
            stream,
            None,
            false,
            options,
        );
        if let Some(body) = self.deferred.last_mut() {
            body.item_shown = false;
        }
        self.push_item(item, children);
    }

    fn sublist_lead_item(&mut self, item: ItemOpen<'l>) -> ControlFlow<()> {
        let (base_indent, content_col, options) =
            (self.base_indent, self.content_col, self.options);
        let content = item.marker.content;
        let item_at = item.at;
        let marker_line_child_col =
            marker_line_child_column(self.cur, content, item_at, content_col);
        // The marker line can open more than one item (`* * u`). Once a
        // flush lazy line folds into the innermost paragraph, S4 closes
        // nothing, so collection must resume against THAT item's content
        // column. Using the outer `content_col` assigned a following line
        // between the two columns to the outer item instead (#1424).
        let content_offset = self.cur.lines.at(item_at).len() - content.len();
        let cached_tail = self.cur.marker_tail.filter(|_| item_at == 0);
        #[cfg(test)]
        let cached_tail = cached_tail.filter(|_| REUSE_MARKER_TAIL.with(Cell::get));
        let carried_tail = cached_tail.and_then(|tail| {
            let relative = tail.content_offset.get().checked_sub(content_offset)?;
            let column = tail.column.checked_sub(content_col)?;
            Some((column, content.get(relative..)?))
        });
        let (tail_col, innermost_content) =
            carried_tail.unwrap_or_else(|| innermost_marker_tail(content));
        let nested_definition_ended_paragraph =
            is_collected_definition_placeholder(innermost_content);
        let mut stream = item_marker_source(self.cur, content, item_at);
        stream.marker_tail =
            NonZeroUsize::new(content.len() - innermost_content.len()).map(|content_offset| {
                MarkerTail {
                    content_offset,
                    column: tail_col,
                }
            });
        // A fence on the nested marker line keeps only lines that reach
        // its host. A flush-left line supplies neither item's content
        // column, so the nested parse must not receive it as lazy payload
        // (CARVE-P0-013).
        let inner_lead = trim_ascii_start(innermost_content);
        let lead = MarkerSublist {
            content,
            innermost_content,
            inner_lead,
            ladder_columns: marker_ladder_columns(content, marker_line_child_col),
            nested_content_col: marker_line_child_col + tail_col,
            nested_definition_ended_paragraph,
            nested_lead_is_continuation: is_plus_marker(innermost_content),
            before_block: self.cur.pos,
        };
        let collection_content_col = if detect_fence_open(inner_lead).is_some() {
            content_col
        } else {
            marker_line_child_col
        };
        let collection_floor = if nested_definition_ended_paragraph
            || detect_fence_open(inner_lead).is_some()
            || !body_ends_with_open_paragraph(content, options)
        {
            collection_content_col.saturating_sub(1)
        } else {
            base_indent
        };
        let before_collection = stream.source.len();
        if lead.nested_lead_is_continuation {
            stream.append(collect_indented_block_mapped_after_continuation(
                self.cur,
                collection_floor,
                collection_content_col,
                options,
            ));
        } else if (!nested_definition_ended_paragraph
            || self
                .cur
                .peek()
                .is_some_and(|line| indent_columns(line) >= marker_line_child_col))
            && !(is_definition_list_start(inner_lead)
                && self.cur.peek().is_some_and(|line| {
                    let indent = indent_columns(line);
                    indent < lead.nested_content_col
                        && !(innermost_marker_column(lead.content)
                            .is_some_and(|marker| indent == marker_line_child_col + marker)
                            && strip_description_prefix(trim_ascii_start(line)).is_some())
                }))
        {
            stream.append(collect_indented_block_mapped(
                self.cur,
                collection_floor,
                collection_content_col,
                options,
            ));
        }
        self.last_item_bare_continuation =
            lead.nested_lead_is_continuation && stream.source.len() == before_collection;
        // A blank line closes the sub-list's last paragraph, so the next
        // flush-left line starts a NEW top-level block instead of folding in
        // (carve-rs#490). The collected source keeps no trace of a trailing
        // blank, so `nested_ends_with_open_paragraph` below still reports the
        // paragraph as open and the lazy loop swallowed `- - a` / blank / `b`
        // into the inner item, where carve-js and carve-php end the list.
        let ended_on_blank = (self.cur.pos > lead.before_block && self.previous_line_blank())
            || sublist_source_trailing_invisible_separation(&stream.source);
        self.fold_marker_sublist(&mut stream, &lead, ended_on_blank);
        self.finish_marker_sublist(item, stream, &lead)
    }

    // A below-column lazy-continuation line following the marker-line
    // sub-list folds into its last open paragraph (`- - b` / `lazy` ->
    // `<li>b\nlazy</li>`), and a following sibling marker at the
    // sub-list's column resumes the SAME list. This is the same
    // lazy-fold / resume loop the following-line nested-list path runs
    // above; reused here so the marker-line sub-list behaves identically.
    fn fold_marker_sublist(
        &mut self,
        stream: &mut MappedSource,
        lead: &MarkerSublist<'l>,
        mut ended_on_blank: bool,
    ) {
        let content_col = self.content_col;
        // Read the prefix once, then inspect only newly appended lines so a
        // long run of lazy continuations keeps linear work.
        let mut term_lead =
            is_definition_list_start(lead.inner_lead)
                && !stream.source.lines().skip(1).any(|line| {
                    !line.starts_with(LAZY) && strip_description_prefix(line).is_some()
                });
        loop {
            if self.cur.peek().is_some_and(|line| {
                marked_quote_at_ancestor_column(line, content_col, lead.nested_content_col)
            }) {
                break;
            }
            if ended_on_blank {
                break;
            }
            if self
                .cur
                .closed_comment_location
                .is_some_and(|(at, column)| at + 1 == self.cur.pos && column >= content_col)
            {
                break;
            }
            if lead.nested_definition_ended_paragraph {
                break;
            }
            let before = stream.source.len();
            match self.marker_sublist_lazy(stream, lead, term_lead) {
                Some(blank) => ended_on_blank = blank,
                None => break,
            }
            term_lead &= !stream.source[before..]
                .lines()
                .any(|line| !line.starts_with(LAZY) && strip_description_prefix(line).is_some());
        }
    }

    /// Fold one lazy run into the innermost paragraph and resume collection.
    /// `None` when no lazy line continues it.
    fn marker_sublist_lazy(
        &mut self,
        stream: &mut MappedSource,
        lead: &MarkerSublist<'l>,
        term_lead: bool,
    ) -> Option<bool> {
        let (base_indent, content_col, options) =
            (self.base_indent, self.content_col, self.options);
        let nested_content_col = lead.nested_content_col;
        let ladder_columns = &lead.ladder_columns;
        let child_col = ladder_columns[0];
        let nested_marker_col = ladder_columns
            .get(ladder_columns.len().saturating_sub(2))
            .copied();
        let first_nested_content_col = ladder_columns.get(1).copied();
        let bare_continuation = self.last_item_bare_continuation;
        let cur = &mut *self.cur;
        let lazy_column = if term_lead {
            nested_content_col
        } else {
            content_col
        };
        let has_lazy = if let Some(line) = cur.peek() {
            let line = line.to_string();
            let indent = indent_columns(&line);
            let at_nested_marker = term_lead
                && ((indent >= child_col
                    && indent <= nested_content_col
                    && is_definition_list_start(trim_ascii_start(&line)))
                    || (nested_marker_col == Some(indent)
                        && strip_definition_marker(trim_ascii_start(&line)).is_some()));
            !is_blank_line(&line)
                && !at_nested_marker
                && (term_lead || indent < lazy_column)
                // A block opener that reaches the surviving item's column
                // has left the inner term. Test its content on that owner's
                // base before the lazy frame hides the authored column.
                && !(term_lead
                    && indent >= child_col
                    && indent <= nested_content_col
                    && !is_list_marker(trim_ascii_start(&line))
                    && interrupts_paragraph(cur, trim_ascii_start(&line)))
                && !(is_list_marker(&line)
                    && (indent == 0
                        || ladder_columns.contains(&indent)
                        || indent > nested_content_col))
                && !interrupts_paragraph(cur, &line)
        } else {
            false
        };
        if !has_lazy {
            return None;
        }
        // A refused first-block marker leaves no paragraph for a lazy fold.
        if bare_continuation {
            return None;
        }
        if reached_wrapped_attribute_tail(stream) {
            return None;
        }
        if !nested_ends_with_open_paragraph_masked(
            stream,
            last_consumed_line_below_column(cur, nested_content_col),
            last_consumed_line_below_column(cur, content_col),
            options,
        ) && !stream
            .source
            .lines()
            .last()
            .is_some_and(|line| caption_content(trim_ascii_start(line)).is_some())
        {
            return None;
        }
        // A term opened on a nested marker line keeps a below-column lazy
        // line. Frame it before the next parse so a colon-shaped line cannot
        // turn into a definition after the source columns have been rebased.
        if term_lead {
            let line = cur.peek().unwrap();
            let indent = indent_columns(line);
            let content = trim_ascii_start(line);
            let source_col = cur
                .source_col(cur.pos)
                .map(|column| column + leading_ws(line) as isize);
            stream.push_newline_at(
                if content.starts_with(LAZY) {
                    content.to_string()
                } else if indent > nested_content_col {
                    format!(
                        "{LAZY_INDENT}{}{content}",
                        " ".repeat(indent - nested_content_col)
                    )
                } else {
                    format!("{LAZY}{content}")
                },
                cur.source_line(cur.pos),
                source_col,
            );
            cur.consume();
            if cur.peek().is_some_and(|line| {
                let indent = indent_columns(line);
                let nested_list_sibling = is_list_marker(line)
                    && (ladder_columns.contains(&indent) || indent > nested_content_col);
                let nested_definition_sibling = first_nested_content_col
                    .is_some_and(|first| indent >= first && indent <= nested_content_col)
                    && is_definition_list_start(trim_ascii_start(line));
                nested_list_sibling || nested_definition_sibling
            }) {
                stream.append(collect_indented_block_mapped(
                    cur,
                    content_col.saturating_sub(1),
                    content_col,
                    options,
                ));
                return Some(false);
            }
            // A lazy line that reaches this item keeps later lines out of the
            // sub-list, except a block opener at the sub-item's content column,
            // which is that sub-item's next block (rule A, carve#3950).
            if indent >= content_col {
                if cur.peek().is_some_and(|line| {
                    indent_columns(line) == nested_content_col
                        && !is_list_marker(trim_ascii_start(line))
                        && interrupts_paragraph(cur, trim_ascii_start(line))
                }) {
                    stream.append(collect_indented_block_mapped(
                        cur,
                        nested_content_col.saturating_sub(1),
                        content_col,
                        options,
                    ));
                }
                return Some(false);
            }
            return Some(resume_marker_sublist(
                cur,
                stream,
                nested_content_col,
                content_col,
                options,
            ));
        }
        let before = cur.pos;
        collect_marker_sublist_lazy_through(
            cur,
            stream,
            nested_content_col.saturating_sub(1),
            base_indent,
            nested_content_col,
            content_col,
        );
        if cur.pos == before {
            return None;
        }
        if cur.peek().is_some_and(|line| {
            marked_quote_at_ancestor_column(line, content_col, nested_content_col)
        }) {
            return Some(false);
        }
        Some(resume_marker_sublist(
            cur,
            stream,
            nested_content_col,
            content_col,
            options,
        ))
    }

    fn finish_marker_sublist(
        &mut self,
        item: ItemOpen<'l>,
        mut stream: MappedSource,
        lead: &MarkerSublist<'l>,
    ) -> ControlFlow<()> {
        let (content_col, options) = (self.content_col, self.options);
        let item_at = item.at;
        let inner_lead = lead.inner_lead;
        // A blank line between the item's blocks loosens the list, and a
        // sub-list lead is no exception: the item holds the sub-list and
        // whatever follows the blank at THIS item's content column
        // (carve-rs#490). #476 fixed the attribute-block, quote and heading
        // leads beside this one and left the sub-list lead open, because
        // carve-php agreed with this engine at the time and carve-js did not.
        // A stacked first block can consume its following blank before the
        // deferred child tree exists. Keep that boundary for the outer item.
        let blank_after_nested_first_block =
            self.cur.lines.line(item_at + 1).is_some_and(is_blank_line)
                && self.cur.lines.line(item_at + 2).is_some_and(|line| {
                    indent_columns(line) == content_col
                        && (is_plus_marker(trim_ascii_start(line))
                            || line_starts_paragraph(trim_ascii_start(line)))
                });
        if blank_after_nested_first_block {
            self.tight = false;
        }
        if sublist_source_trailing_invisible_separation(&stream.source) {
            self.pending_blank = true;
        }
        let before_block = lead.before_block;
        if detect_fence_open(lead.innermost_content).is_some()
            && self.cur.pos > before_block
            && innermost_marker_column(lead.content).is_some_and(|marker_col| {
                indent_columns(self.cur.lines.at(before_block)) == content_col + marker_col
            })
            && trim_ascii(self.cur.lines.at(before_block)) == "+"
            && stream.source.lines().skip(1).all(is_blank_line)
        {
            if let Some(end) = stream.source.find('\n') {
                let mut kept = std::mem::take(&mut stream.source);
                kept.truncate(end);
                stream.set_source(kept);
                stream.line_map.truncate(1);
                stream.col_map.truncate(1);
                stream.reached.truncate(1);
            }
        }
        let retained_band_marker = chunk_ends_in_closed_comment_span(&stream)
            && self
                .cur
                .peek()
                .and_then(detect_list_marker_full)
                .is_some_and(|marker| {
                    marker.indent > self.base_indent && marker.indent < content_col
                });
        if !retained_band_marker {
            let probed = stream.source.clone();
            self.item_paragraph_open =
                ParagraphOpen::deferred(move || body_ends_with_open_paragraph(&probed, options));
        }
        let children = item_body_with_layout_suppression(
            self.deferred,
            self.items.len(),
            stream,
            None,
            false,
            false,
            options,
        );
        // The item runs from its marker to the last line its body
        // consumed - the bullet is part of the item, unlike the
        // paragraph inside it, which starts at the text.
        self.push_item(item, children);
        let leaves_band_line = self.cur.peek().is_some_and(|line| {
            !is_blank_line(line)
                && trim_ascii(line) != "+"
                && detect_list_marker_full(line).is_none()
                && indent_columns(line) < content_col
        });
        if (detect_fence_open(inner_lead).is_some() || lead.nested_definition_ended_paragraph)
            && leaves_band_line
        {
            return END;
        }
        NEXT
    }

    /// Whether the lines down to the first blank show nothing either: a closed
    /// `%%%` span does not, and an ordinary line does.
    fn lead_run_is_invisible(&self, item_at: usize, content: &str) -> bool {
        let mut open = comment_fence_width(trim_ascii_end(content));
        for at in item_at + 1..=self.cur.pos {
            let Some(line) = self.cur.lines.line(at).map(trim_ascii) else {
                break;
            };
            if line.is_empty() {
                break;
            }
            let run = comment_fence_width(line);
            if open != 0 {
                if run == open {
                    open = 0;
                }
                continue;
            }
            open = run;
            if run == 0 && !invisible_item_lead(line) {
                return false;
            }
        }
        true
    }

    fn block_lead_item(&mut self, item: ItemOpen<'l>) -> ControlFlow<()> {
        let (base_indent, content_col, options) =
            (self.base_indent, self.content_col, self.options);
        let content = item.marker.content;
        let item_at = item.at;
        let mut stream = item_marker_source(self.cur, content, item_at);
        let before_block = self.cur.pos;
        // A fence can open on the MARKER LINE, where its opener is the
        // marker-line content rather than a collected line - so the
        // collector's fenced-body guard is seeded from it (corpus 276).
        self.item_open_fence = detect_fence_open(content);
        // A MARKER-LINE BLOCK THAT LEAVES NOTHING OPEN ENDS THE ITEM. PART 1
        // S4's otherwise has already ended it, so a line in the band between
        // the list's base and the item's content column has nothing here to
        // continue. The collection floor is what says so, as it does for an
        // empty quote on the marker line (carve-rs#2120).
        //
        // The predicate is what the marker line leaves OPEN, not whether it
        // holds a paragraph: a code fence or a colon container is open on one
        // line and the band line below it is its payload.
        let marker_block_leaves_content_open = self.item_open_fence.is_some()
            || colon_fences_left_open(content) > 0
            || body_ends_with_open_paragraph(content, options);
        let body_floor = if detect_comment_fence_line(content).is_some()
            || is_flush_line_comment(content)
            || !marker_block_leaves_content_open
        {
            content_col.saturating_sub(1)
        } else {
            base_indent
        };
        self.cur.marker_colon_closed = false;
        stream.append(collect_indented_block_mapped_after_fence(
            self.cur,
            body_floor,
            content_col,
            &mut self.item_open_fence,
            options,
        ));
        let lead = self.block_lead_state(
            &stream,
            content,
            before_block,
            marker_block_leaves_content_open,
        );
        // NOT WHILE THE FENCE IS STILL OPEN, as the continuation branch
        // above already has it. A fenced body is verbatim and holds no
        // paragraph, so there is nothing below the content column for the
        // fold to reach - and its comment arm, which is right that a comment
        // ends no container, took the line into the verbatim body instead
        // (carve-rs#2096).
        if self.item_open_fence.is_none() {
            self.fold_block_lead(&mut stream, &lead);
        }
        // A blank absorbed inside the marker-line block's continuation that
        // is followed by a plain paragraph loosens the item (§17 L1), the
        // same rule the plain-continuation branch applies -- e.g. a
        // marker-line fence with blank-separated trailing text
        // (`- ```\n  c\n  ```\n\n  tail`).
        let refused_definition_plus = is_definition_list_start(content)
            && self
                .cur
                .lines
                .range(item_at + 1..self.cur.pos)
                .any(|line| indent_columns(line) == content_col && trim_ascii(line) == "+")
            && !self
                .cur
                .lines
                .range(item_at + 1..self.cur.pos)
                .any(is_blank_line);
        // A heading, a fence or a table on the marker line leaves the item
        // holding no paragraph - see `item_paragraph_open`.
        self.item_paragraph_open =
            if lead.literal_comment_handoff || lead.definition_comment_handoff {
                ParagraphOpen::Known(false)
            } else {
                let probed = stream.source.clone();
                ParagraphOpen::deferred(move || body_ends_with_open_paragraph(&probed, options))
            };
        let children = item_body_with_layout_suppression(
            self.deferred,
            self.items.len(),
            stream,
            None,
            false,
            refused_definition_plus,
            options,
        );
        self.item_shows =
            !(invisible_item_lead(content) && self.lead_run_is_invisible(item_at, content));
        if !self.item_shows {
            if let Some(body) = self.deferred.last_mut() {
                body.item_shown = false;
            }
        }
        // The item runs from its marker to the last line its body
        // consumed - the bullet is part of the item, unlike the
        // paragraph inside it, which starts at the text.
        self.push_item(item, children);
        // A single-line marker-line block (heading, thematic break) leaves
        // no indented continuation, so collect_indented_block_mapped above
        // swallows the trailing blank separator before the outer loop can
        // see it. Re-raise pending_blank so a following sibling item still
        // loosens the list (`- # H\n\n- b` / `- ---\n\n- b` render `<li>` as
        // loose), matching carve-js / carve-php. A multi-line block (`:::`
        // container) leaves the blank for the outer loop, so
        // swallowed_blank_separator is false there and this does not
        // double-loosen.
        if lead.swallowed_blank_separator {
            self.reraise_blank_after(item_at);
        }
        // The fold above had its chance: it takes the band line back when
        // the body does hold an open paragraph. Left behind, the line
        // reached nothing, so the LIST ends here rather than letting the
        // outer loop read it as this item's lazy continuation.
        if !marker_block_leaves_content_open
            && self.cur.peek().is_some_and(|line| {
                let indent = indent_columns(line);
                !is_blank_line(line)
                    && trim_ascii(line) != "+"
                    && detect_list_marker_full(line).is_none()
                    && indent > base_indent
                    && indent < content_col
            })
        {
            return END;
        }
        NEXT
    }

    fn block_lead_state(
        &mut self,
        stream: &MappedSource,
        content: &str,
        before_block: usize,
        leaves_content_open: bool,
    ) -> BlockLead {
        let base_indent = self.base_indent;
        let cur = &mut *self.cur;
        let retained_marker_comment = leaves_content_open
            && stream.reached.last() == Some(&false)
            && stream
                .source
                .lines()
                .collect::<Vec<_>>()
                .split_last()
                .is_some_and(|(close, body)| closes_a_held_comment_span(body, close));
        let marker_colon_closed = cur.marker_colon_closed;
        let marker_colon_open = !marker_colon_closed
            && colon_fences_left_open(content) > 0
            && detect_line_block_open(content).is_none();
        let swallowed_blank_separator =
            cur.pos > before_block && is_blank_line(cur.lines.at(cur.pos - 1));
        // A paired delimiter beyond the item's column belongs to the quote.
        // Its literal opener must not let trailing prose reclaim that line.
        let literal_comment_handoff = trim_ascii_start(content).starts_with(':')
            && detect_line_block_open(content).is_some()
            && body_open_comment_run(stream.source.lines())
                .is_some_and(|run| cur.has_comment_closer_after(cur.pos, run));
        let definition_comment_handoff = is_definition_list_start(content)
            && stream
                .source
                .lines()
                .any(|line| strip_description_prefix(trim_ascii_start(line)).is_some())
            && stream
                .source
                .lines()
                .last()
                .is_some_and(is_line_comment_any_column)
            && cur
                .peek()
                .is_some_and(|line| indent_columns(line) <= base_indent);
        BlockLead {
            leaves_content_open,
            retained_marker_comment,
            marker_colon_closed,
            marker_colon_open,
            swallowed_blank_separator,
            literal_comment_handoff,
            definition_comment_handoff,
        }
    }

    fn fold_block_lead(&mut self, stream: &mut MappedSource, lead: &BlockLead) {
        let (base_indent, content_col, options) =
            (self.base_indent, self.content_col, self.options);
        // IN THE BAND, not only at column 0. The floor above hands the
        // band line to this fold instead of collecting it blind, and the
        // fold is the thing that asks S4's question - so it has to be
        // able to see a line the raised floor left behind (#2120).
        // A marker-line colon container that has closed stops the
        // collector at a band line, so the fold has to reach it.
        let (lazy_column, list_base) =
            if lead.leaves_content_open && !lead.marker_colon_closed && !lead.marker_colon_open {
                (0, 0)
            } else {
                (content_col.saturating_sub(1), base_indent)
            };
        let refuses_fold = lead.swallowed_blank_separator
            || lead.literal_comment_handoff
            || lead.definition_comment_handoff;
        let retained_marker_comment = lead.retained_marker_comment;
        fold_lazy_run_and_resume_in_band(
            self.cur,
            stream,
            content_col,
            (lazy_column, list_base),
            (false, options),
            |src, _below| {
                if refuses_fold {
                    return false;
                }
                retained_marker_comment || body_ends_with_open_paragraph(&src.source, options)
            },
            |cur| collect_indented_block_mapped(cur, base_indent, content_col, options),
        );
    }

    fn reraise_blank_after(&mut self, item_at: usize) {
        self.pending_blank = true;
        let mut at = self.cur.pos;
        self.blank_run = 0;
        while at > item_at + 1 && self.blank_run < 3 {
            at -= 1;
            let line = self.cur.lines.at(at);
            if is_blank_line(line) {
                self.blank_run += 1;
            } else {
                break;
            }
        }
    }

    // The item's first paragraph is the marker content plus any
    // immediately-following indented prose lines (lazy continuation).
    // It stops at a blank line or a list marker: a nested sublist still
    // interrupts (the one Carve deviation, grammar §10). Block openers that
    // begin ON the marker line are handled above by
    // marker_content_starts_block (heading, fence, thematic, container,
    // table); a would-be opener on a LATER lazy-continuation line stays
    // paragraph text.
    fn paragraph_lead_item(&mut self, item: ItemOpen<'l>) {
        let content = item.marker.content;
        let paragraph_content = if item.marker.checked.is_some() {
            trim_ascii_start(content)
        } else {
            content
        };
        let mut lead = LeadParagraph {
            lines: vec![paragraph_content.to_string()],
            suppress_colon_interrupt: is_invalid_colon_fence_opener_text(content),
            attrs_scan_floor: self.cur.pos,
            anchors: self
                .options
                .positions
                .then(|| vec![inline_anchor_for_line(self.cur, item.at, paragraph_content)]),
            caption_below_content_column: false,
            image_lead: detect_block_image(paragraph_content).is_some()
                || reference_image_line(paragraph_content)
                || quiet_block_image(paragraph_content).is_some(),
            caption_claimed: false,
            inline_image_lead: detect_block_image(paragraph_content).is_some()
                || quiet_block_image(paragraph_content).is_some(),
            literal_colon_opener: detect_container_open(content)
                .map(|open| open.fence_len)
                .or_else(|| detect_line_block_open(content))
                .or_else(|| detect_hardbreaks_block_open(content))
                .or_else(|| detect_quote_block_open(content)),
        };
        while let Some(next) = self.cur.peek() {
            if !self.lead_paragraph_line(&mut lead, next) {
                break;
            }
        }
        self.finish_lead_paragraph(item, lead);
    }

    /// Fold `next` into the lead paragraph. False when it ends the paragraph.
    fn lead_paragraph_line(&mut self, lead: &mut LeadParagraph, next: &str) -> bool {
        let (base_indent, content_col) = (self.base_indent, self.content_col);
        if is_plus_marker(trim_ascii_start(next))
            && indent_columns(next) <= base_indent
            && self
                .cur
                .continuation_decision(self.cur.pos)
                .is_some_and(|decision| decision.owner == Some(continuation_marker::Kind::List))
        {
            return false;
        }
        if is_blank_line(next) {
            return false;
        }
        if let Some(fence_len) = lead.literal_colon_opener {
            if indent_columns(next) <= base_indent && exact_colon_fence_len(next) == Some(fence_len)
            {
                return false;
            }
        }
        if let Some(nm) = detect_list_marker_full(next) {
            // A marker indented past the base but BELOW this item's content
            // column is lazy continuation, not a sub-list: under symmetric
            // §10 no list marker (bullet, task, or ordered) interrupts a
            // paragraph, so fold it. A marker AT or ABOVE the content column
            // nests; one at the base column is a sibling (ends the paragraph).
            let folds = nm.indent > base_indent && nm.indent < content_col;
            if !folds {
                return false;
            }
        }
        let indent = indent_columns(next);
        // A FENCED BODY IS NOT A PARAGRAPH (§24, markup-carve/carve#950).
        // The lead paragraph may have ABSORBED a fence opener as text - §10
        // I4 keeps a fence with no closer left in the item from interrupting
        // - but the LAYOUT still reads a fence there, so a line below the
        // content column closes the item instead of folding in. The item's
        // fence state is one variable across all three of its collection
        // sites, so the outer loop sees the same open fence and closes
        // the list at this line.
        if self.item_open_fence.is_some() && indent < content_col && !next.starts_with(LAZY) {
            return false;
        }
        if indent < content_col {
            return self.lead_line_below_column(lead, next, indent);
        }
        if indent > content_col {
            return self.lead_line_over_indented(lead, next, indent);
        }
        self.lead_line_at_column(lead, next, indent)
    }

    fn lead_line_below_column(
        &mut self,
        lead: &mut LeadParagraph,
        next: &str,
        indent: usize,
    ) -> bool {
        // A run at an ancestor's column interrupts this child.
        if indent == self.base_indent && detect_fence_open(trim_ascii_start(next)).is_some() {
            return false;
        }
        let next_owned = next.to_string();
        if interrupts_lazy_continuation_as_container(self.cur, &next_owned) {
            return false;
        }
        let inline_line = trim_ascii_start(next);
        // CARVE-P0-015: a lazy fold closes no item, so a later blank separates its paragraphs.
        if self.item_unopened_fence_span && {
            let lazy = trim_ascii_start(strip_lazy(inline_line));
            detect_fence_open(lazy).is_some()
                || lazy
                    .strip_prefix('>')
                    .is_some_and(|rest| detect_fence_open(rest).is_some())
        } {
            self.item_unopened_fence_span = false;
        }
        lead.caption_below_content_column |= caption_content(inline_line).is_some();
        self.fold_lead_line(lead, inline_line);
        true
    }

    fn lead_line_over_indented(
        &mut self,
        lead: &mut LeadParagraph,
        next: &str,
        indent: usize,
    ) -> bool {
        // A recognized block opener may be authored past the canonical
        // content column: that visual column becomes its temporary
        // base, and the continuation collector below rebases the whole
        // block relative to it (#1705). Ordinary over-indented prose is
        // still lazy paragraph text.
        let over_indented = trim_ascii_start(next);
        let absorbed =
            lead.suppress_colon_interrupt && is_suppressed_colon_fence_line(over_indented);
        if !absorbed
            && item_block_opener(over_indented)
            && self
                .item_unclosed_fence_base
                .map_or(true, |base| base == indent)
        {
            return false;
        }
        if detect_fence_open(over_indented).is_some() {
            self.item_unopened_fence_span = true;
            self.item_unclosed_fence_base.get_or_insert(indent);
            self.item_rejected_fence_owner = true;
        }
        // Fully strip ordinary prose's indent and fold it into the lead
        // paragraph (inline-parsed, so residual indent never leaks).
        // A sibling/nesting marker above the content column was already
        // handled above; only a genuine lazy interrupt ends the fold.
        let next_owned = next.to_string();
        if !absorbed && interrupts_lazy_continuation_as_container(self.cur, &next_owned) {
            return false;
        }
        // CARVE-P2-020: a caption continuation keeps its indentation.
        if lead.caption_claimed && lead.inline_image_lead {
            let dedented = slice_columns(next, self.content_col, false);
            self.fold_lead_line(lead, &dedented);
        } else {
            self.fold_lead_line(lead, trim_ascii_start(next));
        }
        true
    }

    fn fold_lead_line(&mut self, lead: &mut LeadParagraph, inline_line: &str) {
        if let Some(anchors) = &mut lead.anchors {
            anchors.push(inline_anchor_for_line(self.cur, self.cur.pos, inline_line));
        }
        lead.suppress_colon_interrupt |= is_invalid_colon_fence_opener_text(inline_line);
        lead.lines.push(inline_line.to_string());
        self.cur.consume();
    }

    /// AT content_column: a block opener interrupts the lead paragraph and
    /// nests as a child block; plain text dedents to the body's column 0.
    fn lead_line_at_column(&mut self, lead: &mut LeadParagraph, next: &str, indent: usize) -> bool {
        let content_col = self.content_col;
        let dedented = slice_columns(next, content_col, false);
        let wrapped_attr_interrupts = if self.cur.pos < lead.attrs_scan_floor {
            false
        } else {
            match content_column_attr_block(
                &dedented,
                self.cur.lines.range(self.cur.pos + 1..self.cur.lines.len()),
                content_col,
            ) {
                ContentColumnAttrBlock::Block => true,
                ContentColumnAttrBlock::NoneWithin(lines) => {
                    lead.attrs_scan_floor = self.cur.pos + lines;
                    false
                }
                ContentColumnAttrBlock::No => false,
            }
        };
        let fence_open = detect_fence_open(&dedented);
        let interrupts = (wrapped_attr_interrupts
            || interrupts_paragraph_as_container(self.cur, &dedented)
            || fence_open.is_some())
            && !(lead.suppress_colon_interrupt && is_suppressed_colon_fence_line(&dedented));
        if interrupts {
            return false;
        }
        let caption = caption_content(&dedented).is_some();
        if caption && lead.caption_claimed {
            return false;
        }
        lead.caption_claimed |= caption && lead.image_lead && lead.lines.len() == 1;
        if let Some(anchors) = &mut lead.anchors {
            anchors.push(inline_anchor_for_line(self.cur, self.cur.pos, &dedented));
        }
        track_collected_fence(&mut self.item_open_fence, &dedented, true, indent);
        lead.suppress_colon_interrupt |= is_invalid_colon_fence_opener_text(&dedented);
        lead.lines.push(dedented);
        self.cur.consume();
        true
    }

    fn finish_lead_paragraph(&mut self, item: ItemOpen<'l>, lead: LeadParagraph) {
        let options = self.options;
        // Trailing ASCII whitespace is discarded before inline escape
        // resolution.  Do this per physical line: waiting until after `join`
        // leaves an escaped space on an intermediate list-item line, which the
        // inline parser then turns into NBSP instead of the intended hard break
        // (carve-rs#855, markup-carve/carve#1028).
        let normalized_para_lines: Vec<&str> = lead
            .lines
            .iter()
            .map(|line| trim_ascii_end(strip_lazy(line)))
            .collect();
        // PART 10 I2: a code include line ends at its own line, so what the
        // item folded in below it is the item's next paragraph.
        let code_include = crate::includes::code_include_line(normalized_para_lines[0]);
        let split = if code_include.is_some() {
            1
        } else {
            normalized_para_lines.len()
        };
        let (mut anchors, rest_anchors) = match lead.anchors {
            Some(mut anchors) => {
                let rest = anchors.split_off(split.min(anchors.len()));
                (Some(anchors), Some(rest))
            }
            None => (None, None),
        };
        let rest = (split < normalized_para_lines.len()).then(|| {
            let text = normalized_para_lines[split..].join("\n");
            let text = text.trim_end_matches([' ', '\t']);
            let children = match rest_anchors {
                Some(anchors) => parse_inline_lines_with_anchor(text, options, anchors),
                None => parse_inline_with_options(text, options),
            };
            BlockNode::Paragraph(Paragraph {
                attrs: None,
                children,
                at_content_column: false,
                block_image: false,
                pos: span_of(self.cur, item.at + split, self.cur.pos, options),
            })
        });
        let para_text = normalized_para_lines[..split].join("\n");
        let para_text = para_text.trim_end_matches([' ', '\t']);
        let children = if let Some(anchors) = anchors.take() {
            parse_inline_lines_with_anchor(para_text, options, anchors)
        } else {
            parse_inline_with_options(para_text, options)
        };
        let children = if code_include == Some(false) {
            crate::parse::paragraph::literal_line_children(&children, para_text)
        } else {
            children
        };
        let mut paragraph = BlockNode::Paragraph(Paragraph {
            attrs: None,
            children,
            // This paragraph BEGINS at the item's content column by construction:
            // its first line is the marker line's own content. Leaving the field
            // at its `false` default gated off the image + `^ caption` promotion
            // in `promote_block_images`, so a caption inside a list item rendered
            // as literal text (carve-rs#610). Lines folded in from BELOW the
            // content column are lazy continuations and never start this
            // paragraph, so the flag describes the first line correctly.
            at_content_column: !lead.caption_below_content_column,
            block_image: false,
            pos: item_paragraph_span(
                self.cur,
                item.at,
                if rest.is_some() {
                    item.at
                } else {
                    self.cur.pos.saturating_sub(1)
                },
                item.marker.content,
                options,
            ),
        });
        if options.source_lines {
            if let Some(line) = item.source_line {
                stamp_source_line(&mut paragraph, line);
            }
        }
        // This path's item IS a paragraph, so S4 has a paragraph for a lazy line.
        self.item_paragraph_open = ParagraphOpen::Known(true);
        self.push_item(item, std::iter::once(paragraph).chain(rest).collect());
    }
}

/// The authored column where a marker-line child begins, past a task's
/// checkbox and never before the item's content column.
fn marker_line_child_column(
    cur: &LineCursor<'_>,
    content: &str,
    item_at: usize,
    content_col: usize,
) -> usize {
    cur.lines.line(item_at).map_or(content_col, |lead| {
        let offset = (content.as_ptr() as usize).saturating_sub(lead.as_ptr() as usize);
        lead.get(..offset)
            .map_or(0, |before| {
                before.chars().fold(0, |column, ch| {
                    if ch == '\t' {
                        column + 4 - column % 4
                    } else {
                        column + 1
                    }
                })
            })
            .max(content_col)
    })
}

struct ListFinish<'a> {
    deferred_start: usize,
    span_start: usize,
    first_marker: ListMarker<'a>,
    start: Option<usize>,
    ol_type: Option<OrderedListType>,
    tight: bool,
    pending_attrs: Option<Attrs>,
    pending_attrs_pos: Option<Pos>,
}

fn finish_list(
    cur: &LineCursor,
    options: &Options<'_>,
    deferred: &mut [ItemBody],
    finish: ListFinish<'_>,
    mut items: Vec<ListItem>,
) -> Box<BlockNode> {
    let ListFinish {
        deferred_start,
        span_start,
        first_marker,
        start,
        ol_type,
        tight,
        pending_attrs,
        pending_attrs_pos,
    } = finish;
    let base_indent = first_marker.indent;
    let is_ordered = first_marker.ordered;
    let first_delim = first_marker.delim;
    // Indentation that places a list at this cursor level belongs to each item.
    // `MappedSource` anchors the stripped marker at the content column, so move
    // the start back across that placing indentation before offsets are filled.
    //
    // NOT in a footnote or definition body: there an over-indented marker
    // carries its own authored base (§24 C3), so its span begins ON the marker
    // and the run before it is not placing indentation to reclaim
    // (markup-carve/carve#1980, corpus 461-...-sibling-4).
    if base_indent > 0 && !cur.sublists_carry_authored_base {
        // Items run in line order, so one forward walk finds each item's line.
        let mut at = span_start;
        for item in &mut items {
            if let Some(pos) = &mut item.pos {
                while at < cur.pos
                    && cur
                        .source_line(at)
                        .is_some_and(|line| line < pos.start_line)
                {
                    at += 1;
                }
                let line = (cur.source_line(at) == Some(pos.start_line)).then_some(at);
                reclaim_placing_indent(pos, cur, line, base_indent);
            }
        }
    }
    // THE LIST ENDED, so a set still held here was written in front of a block
    // that never came (§15 A4). This is the list's own slot, separate from the
    // one `parse_blocks` keeps for a container BODY - attributes reach it only
    // by being lifted off a chunk, and only this loop can place them.
    if pending_attrs.is_some() {
        note_unattached_block_attrs(pending_attrs_pos);
    }
    // Over the blocks the items ALREADY hold - a lead paragraph, and anything a
    // caller that resolved its chunks in place put there. The chunks still on
    // the worklist widen their item when they land (`append_item_children`).
    widen_items_over_children(&mut items);
    let cursor_span = span_of(cur, span_start, cur.pos, options);
    // WHICH SPELLING ANSWERED decides whether the list's own end still moves.
    // The cursor's span already covers every line the items sit on, so a chunk
    // landing later cannot push past it; `span_across_items` reads the LAST
    // item's end, which a chunk landing later does move. The chunks were
    // pushed before this was known, so they are told now.
    let from_items = cursor_span.is_none();
    let mut list_pos = cursor_span.or_else(|| span_across_items(&items));
    if from_items {
        for body in &mut deferred[deferred_start..] {
            body.widen_list = true;
        }
    }
    if base_indent > 0 && !cur.sublists_carry_authored_base {
        if let Some(pos) = &mut list_pos {
            let line = (cur.source_line(span_start) == Some(pos.start_line)).then_some(span_start);
            reclaim_placing_indent(pos, cur, line, base_indent);
        }
    }
    boxed_list_node(List {
        // The cursor's own span when it can give one, else the extent of the
        // items themselves. A list inside a `+`-continued blockquote sits on
        // lines whose stripped width is unknown, so `span_of` refuses - but the
        // items were placed by other means, and a list that runs from its first
        // item to its last is not a guess.
        pos: list_pos,
        attrs: None,
        ordered: is_ordered,
        start,
        ol_type,
        bare_marker: is_ordered && first_marker.marker.is_empty(),
        // Only the non-default spelling is recorded: the schema calls `delim`
        // of `.` and `bullet_char` of `-` absent, so writing them would give
        // one document two legal trees (carve#2828). NOT "drop the field": `)`
        // and `*` ride through, and dropping those breaks the fields instead.
        delim: first_delim.map(char::from).filter(|c| *c != '.'),
        bullet_char: if is_ordered {
            None
        } else {
            first_marker.marker.chars().next().filter(|c| *c == '*')
        },
        tight,
        items,
    })
}

/// Keep a quoted fence's payload at the quote prefix after stripping the list column.
fn rebase_quoted_fence_payload(stream: &mut MappedSource, marker_leaf: &str) {
    let Some(first_fence) = detect_fence_open(marker_leaf) else {
        return;
    };
    let mut open = Some(first_fence);
    let mut payload_column = None;
    let mut lines = Vec::new();
    for (at, line) in stream.source.split('\n').enumerate() {
        if at == 0 {
            lines.push(line.to_string());
            continue;
        }
        let leading = indent_columns(line);
        let stripped = trim_ascii_start(line);
        if open.is_some()
            && payload_column.is_none()
            && strip_blockquote_prefix(stripped).is_some_and(|body| {
                !is_blank_line(body) && !open.is_some_and(|fence| is_fence_close(body, fence))
            })
        {
            payload_column = Some(leading);
        }
        let line = if open.is_some()
            && leading > 0
            && payload_column.is_some_and(|column| leading <= column)
            && strip_blockquote_prefix(stripped).is_some()
        {
            if let Some(column) = stream.col_map.get_mut(at).and_then(Option::as_mut) {
                *column += leading as isize;
            }
            stripped
        } else {
            line
        };
        if let Some(body) = strip_blockquote_prefix(line) {
            if open.is_some_and(|fence| is_fence_close(body, fence)) {
                open = None;
                payload_column = None;
            } else if open.is_none() {
                open = detect_fence_open(body);
                payload_column = None;
            }
        }
        lines.push(line.to_string());
    }
    stream.set_source(lines.join("\n"));
}

/// After a lazy run, collect the lines that reach the sub-list's content
/// column back into it. The stream is in the item's columns, not the
/// sub-list's. A run of blanks alone stays with the item, which it loosens.
/// Returns whether the collection ended on a blank line.
fn resume_marker_sublist(
    cur: &mut LineCursor,
    stream: &mut MappedSource,
    nested_content_col: usize,
    content_col: usize,
    options: &Options<'_>,
) -> bool {
    let before_block = cur.pos;
    let resumed = collect_indented_block_mapped(cur, nested_content_col - 1, content_col, options);
    if resumed.source.split('\n').all(is_blank_line) {
        cur.pos = before_block;
        return false;
    }
    stream.append(resumed);
    cur.pos > before_block && is_blank_line(cur.lines.at(cur.pos - 1))
}

/// Reads a collected body the way the document walk reads a document: an
/// opener past the body's column opens there (rule A, carve#3885).
fn walk_chunk(stream: &mut MappedSource) {
    let Some((walked, shifts)) = walk_body(&stream.source) else {
        return;
    };
    for (line, delta) in shifts {
        if let Some(Some(column)) = stream.col_map.get_mut(line) {
            *column += delta;
        }
    }
    stream.set_source(walked);
    stream.marker_tail = None;
}

fn invisible_item_lead(content: &str) -> bool {
    is_line_comment_any_column(content)
        || detect_comment_fence_line_any_column(content).is_some()
        || is_definition_placeholder(content)
        || parse_footnote_def_line(content).is_some()
        || parse_link_def_line(content).is_some_and(|(_, target)| !trim_ascii(target).is_empty())
        || parse_standalone_attrs(content).is_some()
}
