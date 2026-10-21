use super::*;

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
    let is_task = first_marker.checked.is_some();
    let is_ordered = first_marker.ordered;
    // §11 ambiguous-letter tie-break: a single roman-letter first marker is
    // roman or alpha depending on its consecutive sibling. Resolve it against
    // the next sibling marker before fixing the list's type.
    let (start, ol_type) = resolve_ordered_first(&first_marker, cur, base_indent);
    let first_delim = first_marker.delim;
    let first_dialect = ol_dialect(ol_type);
    let mut items: Vec<ListItem> = Vec::new();
    let mut tight = true;
    let mut pending_blank = false;
    // How many blank lines this loop has consumed since the last line with
    // content. PART 9 §11 N1a's hard boundary is a run of THREE OR MORE before
    // a compatible sibling marker, so the count matters and `pending_blank`
    // alone -- which is a flag -- cannot carry it.
    let mut blank_run = 0usize;
    // A block-attribute block that ended a continuation chunk, waiting for the
    // SUB-LIST it was written in front of. The chunk boundary is what separated
    // them (see `split_trailing_attrs`), so this carries them across it.
    //
    // It survives blank lines, because a blank does not break attachment
    // anywhere else either - `{.x}` / blank / `- b` at document level attaches.
    // Everything that opens an item or attaches a block of its own clears it,
    // so a set of attributes can only ever reach the sub-list that directly
    // follows it, and one that reaches nothing is dropped exactly as before.
    let mut pending_attrs: Option<Attrs> = None;
    let mut pending_attr_slots = None;
    // Where that set was written, for §15 A4's diagnostic. This slot holds
    // attributes LIFTED off a chunk (`split_trailing_attrs`), so the span comes
    // from the chunk's own maps rather than from the cursor.
    let mut pending_attrs_pos: Option<Pos> = None;
    // The current item's content column (where its content begins after the
    // marker). Nested content and sub-blocks of the last item dedent by this, so
    // it persists across iterations and is updated as each item is opened.
    //
    // A PLACEHOLDER, NOT A RULE. `parse_list` is only entered on a marker, and
    // the marker branch below assigns before anything reads this, so the value
    // here is dead - measured, by setting it to 999 and finding the corpus and
    // every list test green. It is deliberately not spelled as the real
    // computation: a third copy of the column rule that nothing can exercise is
    // exactly what let the constant here drift from the two live ones.
    let mut content_col = base_indent + 2;
    // The current item's own fenced code block, still OPEN. A FENCED BODY IS
    // NOT A PARAGRAPH, so nothing below the content column folds into the item
    // while one is open: PART 9 §24's S1 stops at the ITEM, S2 never fires, and
    // S4's lazy branch wants a paragraph a verbatim body is not - the item and
    // the list close, and the residue re-parses in the surviving context
    // (markup-carve/carve#950). It lives here, beside `content_col`, because the
    // guard is on the OPEN FENCE rather than on where the fence was opened: the
    // opener may be the MARKER line, which the collectors never see, or a later
    // CONTINUATION line, after the item's paragraph state has reopened.
    let mut item_open_fence: Option<FenceOpen> = None;
    // Does the current item still hold an OPEN PARAGRAPH for a lazy line?
    //
    // The comment exception below needs it. Section 24 C3 says a comment does
    // not close the item, and that is right - but it does not reach the item
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
    let mut item_paragraph_open = ParagraphOpen::Known(true);
    // A refused fence stays paragraph text but still affects lazy-line
    // ownership. Its blank separators count when the second paragraph reaches
    // the item's content column.
    let mut item_unopened_fence_span = false;
    let mut item_unclosed_fence_base = None;
    let mut item_rejected_fence_owner = false;
    // An empty first-block item has no paragraph for a below-column line to continue.
    let mut last_item_bare_continuation = false;
    // The same S4 state for an item whose marker-line content is only a
    // floating attribute block: it has no paragraph for a below-column line to
    // continue, and the pending attribute cannot pull that line into its scope.
    let mut last_item_marker_attr_only = false;
    while let Some(line) = cur.peek() {
        if is_blank_line(line) {
            // A blank alone does not loosen the list; it loosens only when the
            // next line is a sibling item (handled at the marker branch via
            // pending_blank) or an indented second paragraph (below). A blank
            // before any other indented block keeps it compact (#74).
            pending_blank = true;
            blank_run += 1;
            cur.consume();
            continue;
        }
        // Every nonblank line ends the preceding blank run.
        let blank_run_before = std::mem::take(&mut blank_run);
        // Physical ownership survives every enclosing collector's dedent.
        if is_plus_marker(trim_ascii_start(line))
            && matches!(indent_columns(line), column if column == base_indent || column == content_col)
            && cur.continuation_decision(cur.pos).is_some_and(|decision| {
                decision.owner == Some(continuation_marker::Kind::List)
                    && decision.refusal.is_none()
            })
        {
            let marker_column = indent_columns(line);
            cur.consume();
            pending_blank = false;
            // A marker is not the block these were written for either (§15 A4).
            note_unattached_block_attrs(
                pending_attrs_pos.take().filter(|_| pending_attrs.is_some()),
            );
            pending_attrs = None;
            let attached_comment = cur.peek().is_some_and(is_line_comment_any_column);
            let attached = parse_continuation_block(
                cur,
                options,
                base_indent,
                Some(content_col),
                marker_column,
            );
            if attached.is_none()
                && cur.peek().is_some_and(is_plus_marker)
                && cur.continuation_decision(cur.pos).is_some_and(|decision| {
                    decision.owner == Some(continuation_marker::Kind::List)
                        && decision.refusal == Some(continuation_marker::Refusal::Blank)
                })
            {
                let mut bodies = Vec::new();
                if let Some(block) = parse_block(cur, options, &mut bodies) {
                    if let Some(last_item) = items.len().checked_sub(1) {
                        place_item_chunk(deferred, last_item, vec![*block], Vec::new());
                    }
                }
            }
            if let Some(block) = attached {
                // Hoisted definitions leave no paragraph node behind (carve#801).
                let is_empty_paragraph = matches!(
                    &block,
                    BlockNode::Paragraph(p) if p.children.is_empty()
                );
                if !items.is_empty() && !is_empty_paragraph {
                    // Keep the attachment after any deferred item body.
                    let last_item = items.len() - 1;
                    place_item_chunk(deferred, last_item, vec![block], Vec::new());
                }
            }
            if attached_comment
                && cur.peek().is_some_and(|next| {
                    indent_columns(next) == base_indent
                        && line_starts_paragraph(next)
                        && !item_block_opener(next)
                })
            {
                if let Some(block) = parse_continuation_block(
                    cur,
                    options,
                    base_indent,
                    Some(content_col),
                    base_indent,
                ) {
                    if let Some(last_item) = items.len().checked_sub(1) {
                        place_item_chunk(deferred, last_item, vec![block], Vec::new());
                    }
                }
            }
            if options.extensions.is_empty()
                && cur
                    .peek()
                    .is_some_and(|line| below_item_definition(line, base_indent, content_col))
            {
                break;
            }
            continue;
        }
        let Some(marker) = detect_list_marker_full(line) else {
            let indent = indent_columns(line);
            if indent < content_col
                && is_line_comment_any_column(line)
                && !line.starts_with(LAZY)
                && !item_paragraph_open.get()
            {
                break;
            }
            if indent > base_indent {
                let foreign_fence_base = indent > content_col
                    && item_unclosed_fence_base.is_some_and(|base| base != indent);
                let authored_overindented_opener = indent > content_col
                    && !foreign_fence_base
                    && item_block_opener(trim_ascii_start(line));
                // After a blank line, lazy continuation no longer applies: a line
                // must reach the item's content column to keep belonging to it
                // (PART 9 §24 C3). A line BELOW content_column ends the list and
                // parses at document level (corpus 81-list-lazy-5). content_col
                // is the item's true column (`- `=2, `1. `=3, `10. `=4), NOT a
                // fixed base+2 -- an ordered item's body sits deeper.
                if pending_blank && indent < content_col {
                    break;
                }
                // The item holds an OPEN fenced body and this line is below its
                // content column, so §24 closes the item and the list here
                // rather than folding the line in (#950). The collector below
                // stops on the same state and would otherwise return nothing,
                // leaving the cursor where it is.
                if item_open_fence.is_some() && indent < content_col && !line.starts_with(LAZY) {
                    break;
                }
                // An unattached first-block marker cannot admit a lazy continuation.
                if (last_item_bare_continuation || last_item_marker_attr_only)
                    && indent < content_col
                {
                    break;
                }
                if items
                    .last()
                    .is_some_and(|item| item.children.iter().all(crate::ast::publishes_nothing))
                    && indent < content_col
                {
                    break;
                }
                if !items.is_empty() {
                    let last_item = items.len() - 1;
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
                    let lead_at_column = slice_columns(line, content_col.min(indent), false);
                    let lead_is_a_comment = detect_comment_fence_line_any_column(&lead_at_column)
                        .is_some()
                        || is_line_comment_any_column(&lead_at_column);
                    let chunk_floor = if indent >= content_col
                        && !lead_is_a_comment
                        && a_band_line_ends_the_chunk(cur, base_indent, content_col)
                        && !chunk_run_leaves_content_open(
                            cur,
                            content_col,
                            &lead_at_column,
                            options,
                        ) {
                        content_col.saturating_sub(1)
                    } else {
                        base_indent
                    };
                    let mut nested = collect_item_continuation_block_mapped(
                        cur,
                        chunk_floor,
                        content_col,
                        &mut item_open_fence,
                        options,
                    );
                    nested.authored_base_at_start |= authored_overindented_opener;
                    if foreign_fence_base {
                        nested.authored_base_at_start = false;
                    }
                    let collected_tail_blank = item_open_fence.is_some()
                        && cur.pos > 0
                        && is_blank_line(cur.lines.at(cur.pos - 1));
                    if item_open_fence.is_some() {
                        let mut admitted = nested.clone();
                        rebase_overindented_blocks(&mut admitted, false, options);
                        if body_ends_with_open_paragraph(&admitted.source, options) {
                            item_open_fence = None;
                            item_rejected_fence_owner = true;
                        }
                    }
                    let latest_owner = Cell::new(None);
                    if item_rejected_fence_owner {
                        let mut admitted = nested.clone();
                        rebase_overindented_blocks(&mut admitted, false, options);
                        latest_owner.set(Some(collected_body_takes_the_lazy_line(
                            &admitted.source,
                            last_consumed_line_below_column(cur, content_col),
                            options,
                        )));
                    }
                    if !collected_tail_blank
                        && (item_open_fence.is_none()
                            || cur.peek().is_some_and(|line| line.starts_with(LAZY)))
                    {
                        fold_lazy_run_and_resume_in_band(
                            cur,
                            &mut nested,
                            content_col,
                            (content_col.saturating_sub(1), base_indent),
                            (item_rejected_fence_owner, options),
                            |src, below| {
                                if let Some(open) = latest_owner.get() {
                                    return open;
                                }
                                let mut rebased = src.clone();
                                rebase_overindented_blocks(&mut rebased, false, options);
                                collected_body_takes_the_lazy_line(&rebased.source, below, options)
                            },
                            |cur| {
                                let resumed = collect_item_continuation_block_mapped(
                                    cur,
                                    base_indent,
                                    content_col,
                                    &mut item_open_fence,
                                    options,
                                );
                                if latest_owner.get().is_some() {
                                    let mut admitted = resumed.clone();
                                    rebase_overindented_blocks(&mut admitted, false, options);
                                    let paragraph =
                                        body_ends_with_open_paragraph(&admitted.source, options);
                                    if paragraph {
                                        item_open_fence = None;
                                    }
                                    latest_owner.set(Some(paragraph));
                                }
                                resumed
                            },
                        );
                    }
                    // The outer list loop consumes the separating blank before
                    // collecting this chunk. Preserve that boundary so an
                    // over-indented opener at the chunk's first visible line
                    // can establish its authored block base (#1705). Without
                    // this, it is indistinguishable from a first-line residual
                    // indent produced by a tab straddling the content column.
                    nested.authored_base_at_start |= pending_blank && !foreign_fence_base;
                    let definition_ended_paragraph = nested.source.lines().any(|line| {
                        is_collected_definition_placeholder(line)
                            || is_quoted_definition_placeholder(line)
                    });
                    let degraded_comment_fence = chunk_ends_in_degraded_comment_fence(cur, &nested);
                    let closed_comment_span = chunk_ends_in_closed_comment_span(&nested);
                    // Normalize the authored base before peeling trailing
                    // attribute lines: the peel is intentionally syntactic and
                    // only recognizes a flush attribute block. The target may
                    // live in the following chunk (notably a sub-list), so
                    // waiting for `parse_item_chunk` would drop the attributes
                    // before they can be carried across that boundary.
                    rebase_overindented_blocks(&mut nested, false, options);
                    // Deferred: the probe re-parses the whole collected body, and
                    // the body's own sub-lists probe theirs, so an eager probe here
                    // parsed a ladder's body twice per level.
                    let probed = nested.clone();
                    let below = last_consumed_line_below_column(cur, content_col);
                    item_paragraph_open = ParagraphOpen::deferred(move || {
                        nested_ends_with_open_paragraph_rebased(&probed, below, below, options)
                    });
                    let mut split_stack: Vec<(Attrs, Option<Pos>)> = Vec::new();
                    while let Some(split) = split_trailing_attrs(&mut nested) {
                        split_stack.push(split);
                    }
                    let mut carried = Vec::new();
                    let (mut nested_children, nested_loose) =
                        parse_item_chunk(&nested, options, &mut carried);
                    // Attributes held from an EARLIER chunk attach here when
                    // this one opens with a block, which is the case where a
                    // blank line sits between the `{…}` and its target. The
                    // sub-list branch is the other consumer; between them, a set
                    // of attributes reaches the first block written after it
                    // whichever chunk that landed in.
                    if pending_attrs.is_some() {
                        if let Some(target) = nested_children
                            .iter_mut()
                            .find(|block| !matches!(block, BlockNode::Comment(_)))
                        {
                            apply_attrs_to_block(target, pending_attrs.take().unwrap());
                            pending_attrs_pos = None;
                        }
                    }
                    // Back into SOURCE order, so the merge resolves a repeated
                    // key the way `parse_blocks` resolves it at top level.
                    for (attrs, pos) in split_stack.into_iter().rev() {
                        if pending_attrs.is_none() {
                            pending_attrs_pos = pos;
                        }
                        merge_attrs_indexed(&mut pending_attrs, attrs, &mut pending_attr_slots);
                    }
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
                    let blank_can_loosen = !item_unopened_fence_span
                        || item_unclosed_fence_base.is_some_and(|base| base <= content_col);
                    if pending_blank
                        && blank_can_loosen
                        && first_visible.is_some_and(block_is_paragraph_shaped)
                    {
                        tight = false;
                    }
                    // A blank ABSORBED inside the collected continuation (e.g. a
                    // fence/div/table followed by a blank and then trailing text)
                    // loosens the item when a plain paragraph follows the blank
                    // (§17 L1). The outer `pending_blank` only sees a blank BEFORE
                    // this chunk, so this covers the blank-after / blank-both case.
                    if blank_can_loosen && nested_loose {
                        tight = false;
                    }
                    // An INVISIBLE continuation is not the item's second block
                    // (§17 L2 - it renders nothing), but it does not consume the
                    // blank either: the blank still sits between this item and
                    // whatever follows, and a blank before a SIBLING loosens the
                    // list. Clearing it here made `- a` / blank / `  %% note` /
                    // `- b` tight, where carve-js and the corpus have it loose
                    // (carve-rs#557, corpus 87-compact-list-blocks-6).
                    let renders_nothing = nested_children.iter().all(|block| {
                        matches!(
                            block,
                            BlockNode::Comment(_)
                                | BlockNode::AbbreviationDef(_)
                                | BlockNode::LinkReferenceDefinition(_)
                        )
                    });
                    if !renders_nothing {
                        pending_blank = collected_tail_blank
                            || (item_open_fence.is_some()
                                && cur.pos > 0
                                && is_blank_line(cur.lines.at(cur.pos - 1)));
                        if pending_blank {
                            blank_run = cur
                                .lines
                                .range(0..cur.pos)
                                .rev()
                                .take(3)
                                .take_while(|line| is_blank_line(line))
                                .count();
                        }
                    }
                    place_item_chunk(deferred, last_item, nested_children, carried);
                    if cur.peek().is_some_and(|line| {
                        let indent = indent_columns(line);
                        indent > base_indent && indent < content_col
                    }) && !item_paragraph_open.get()
                    {
                        break;
                    }
                    // A collected definition is an I5 block, not the comment
                    // exception. If the collector stopped on a nonzero line
                    // below the item's content column, no paragraph remains
                    // for that line to continue (markup-carve/carve#1376).
                    if (definition_ended_paragraph || degraded_comment_fence || closed_comment_span)
                        && cur.peek().is_some_and(|line| {
                            let indent = indent_columns(line);
                            indent > 0 && indent < content_col
                        })
                    {
                        break;
                    }
                    continue;
                }
            }
            // AN OPEN FENCE MAKES THE BODY VERBATIM, so a `%%` line in it is not
            // a comment and §24 C3's column exemption has nothing to exempt.
            // Below the content column the line reached nothing at all, so S4's
            // otherwise ends the item and its fence rather than taking the line
            // as payload - which is the answer the two `item_open_fence` guards
            // above already give every other shape (carve-rs#2096). A separate
            // condition from `item_paragraph_open`, which asks whether a
            // paragraph is left at all: a verbatim body holds none, and a line
            // below the column reaches neither.
            let fence_body_is_verbatim_here = item_open_fence.is_some() && indent < content_col;
            let below_fence = matched_fence_below_item(cur, line, content_col);
            if !pending_blank
                && !fence_body_is_verbatim_here
                && item_paragraph_open.get()
                && (flush_comment_keeps_the_item_open(cur, line) || below_fence.is_some())
            {
                // A collected definition at this container's column zero is
                // not the comment exception below. It is a column-scoped I5
                // interrupter: below the open item's content column it closes
                // the item, then the surrounding block parser consumes the
                // placeholder and parses what follows as a sibling. At the
                // item's content column the indented continuation branch above
                // already collected it inside the item (corpus 228).
                if is_document_definition_placeholder(line) {
                    break;
                }
                if !items.is_empty() {
                    let last_item = items.len() - 1;
                    let mut nested = MappedSource::new_line_at(
                        line.to_string(),
                        cur.source_line(cur.pos),
                        cur.source_col(cur.pos),
                    );
                    cur.consume();
                    if let Some(run) = below_fence {
                        append_comment_span_through_closer(cur, &mut nested, run, false);
                    }
                    // The lazy text after the comment belongs to the item too,
                    // and parsing it TOGETHER with the comment is what makes it
                    // a second paragraph rather than a continuation of the
                    // first: a comment ends the paragraph above it (§10) while
                    // leaving its container open.
                    let retained_fence = collect_trailing_lazy_through(
                        cur,
                        &mut nested,
                        base_indent,
                        base_indent,
                        content_col,
                    );
                    if retained_fence && cur.pos > 0 && is_blank_line(cur.lines.at(cur.pos - 1)) {
                        pending_blank = true;
                    }
                    let children = item_body(deferred, last_item, nested, None, false, options);
                    items[last_item].children.extend(children);
                    if retained_fence {
                        item_paragraph_open = ParagraphOpen::Known(false);
                        if cur.peek().is_some_and(|line| {
                            indent_columns(line) < content_col
                                && !detect_list_marker_full(line)
                                    .is_some_and(|marker| marker.indent == base_indent)
                        }) {
                            break;
                        }
                    }
                    continue;
                }
            }
            break;
        };
        if marker.indent < base_indent {
            break;
        }
        if marker.indent > base_indent {
            // A marker indented past the base nests as a sub-list. (An ordered
            // marker BELOW the content column never reaches here -- it folds
            // into the item paragraph in the per-item loop below, §10. Unordered
            // and task markers always interrupt, so they nest at any indent.)
            if pending_blank && marker.indent < content_col {
                break;
            }
            // The same fenced-body guard the non-marker branch above applies. A
            // MARKER below the content column is a line below the content
            // column, and §24 stops at the ITEM for both: S1 walks the stack by
            // the indentation a line supplies, which has nothing to do with
            // what the line says. Whether the residue is a sub-list or prose is
            // decided in the surviving context, AFTER the containers close - so
            // reading the marker first nested it inside the very item the open
            // fence closes (markup-carve/carve#950).
            if (item_open_fence.is_some()
                || last_item_bare_continuation
                || last_item_marker_attr_only
                || !item_paragraph_open.get())
                && marker.indent < content_col
            {
                break;
            }
            if !items.is_empty() {
                let last_item = items.len() - 1;
                let sub_indent = marker.indent;
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
                    && cur.peek().is_some_and(|next| {
                        line_starts_paragraph(next) && !item_block_opener(next)
                    })
                    && cur.lines.line(cur.pos - 1).is_some_and(|prior| {
                        let indent = indent_columns(prior);
                        indent >= content_col + 2
                            && line_starts_paragraph(trim_ascii_start(prior))
                            && !nested_columns.tabbed_marker
                    });
                let nested_bare_ends_item = nested_bare_marker_then_plain(cur, content_col)
                    || refused_first_block_below_column(cur, content_col);
                if stopped_fence.is_none() && !descendant_text_after_fence && !nested_bare_ends_item
                {
                    fold_lazy_run_and_resume_in_band(
                        cur,
                        &mut nested,
                        content_col,
                        lazy_scope,
                        (false, options),
                        |src, below| {
                            if !src.source.contains("```") && !src.source.contains("~~~") {
                                return collected_body_takes_the_lazy_line(
                                    &src.source,
                                    below,
                                    options,
                                );
                            }
                            // The authored fence column must be rebased before
                            // deciding whether a lazy line can follow it.
                            let mut rebased = src.clone();
                            rebase_overindented_blocks(&mut rebased, false, options);
                            collected_body_takes_the_lazy_line(&rebased.source, below, options)
                        },
                        |cur| {
                            // PART 9 §10 I2 keeps the paragraph owner across a collection restart.
                            nested_columns.lazy_paragraph_owner = cur
                                .pos
                                .checked_sub(1)
                                .filter(|&at| {
                                    let prior = cur.lines.at(at);
                                    line_starts_paragraph(trim_ascii_start(prior))
                                        && !item_block_opener(trim_ascii_start(prior))
                                        && !cur
                                            .continuation_decision(at)
                                            .is_some_and(|decision| decision.refusal.is_none())
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
                let nested_sublist = marker.indent >= content_col;
                if nested_sublist && sublist_source_loosens_outer_item(&nested.source) {
                    tight = false;
                }
                let held = pending_attrs.take();
                if held.is_some() {
                    pending_attrs_pos = None;
                }
                if cur
                    .peek()
                    .is_some_and(|line| !is_blank_line(line) && indent_columns(line) < content_col)
                {
                    item_paragraph_open =
                        ParagraphOpen::Known(nested_ends_with_open_paragraph_rebased(
                            &nested,
                            last_consumed_line_below_column(cur, content_col),
                            last_consumed_line_below_column(cur, content_col),
                            options,
                        ));
                }
                let nested_children = item_body_with_layout_suppression(
                    deferred,
                    last_item,
                    nested,
                    held,
                    false,
                    nested_sublist,
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
                pending_blank = false;
                items[last_item].children.extend(nested_children);
                if nested_bare_ends_item {
                    break;
                }
                if descendant_text_after_fence
                    && cur.peek().is_some_and(|line| {
                        !is_blank_line(line) && indent_columns(line) < content_col
                    })
                {
                    break;
                }
                if stopped_fence.is_some()
                    && cur.peek().is_some_and(|line| {
                        !is_blank_line(line)
                            && detect_list_marker_full(line).is_none()
                            && indent_columns(line) < content_col
                    })
                {
                    break;
                }
                if band_line_ends_the_chunk
                    && cur.peek().is_some_and(|line| {
                        let indent = indent_columns(line);
                        !is_blank_line(line) && indent > base_indent && indent < content_col
                    })
                {
                    break;
                }
                continue;
            }
            break;
        }
        // Past the sub-list branch, so this marker opens a SIBLING item. Any
        // attributes still held were written in front of something that never
        // came, and a sibling is not that something - drop them here rather
        // than let them reach a sub-list further down the list, which is not
        // where the author put them (carve-rs#1007). Reported rather than
        // silent, like every other way of running out (§15 A4,
        // markup-carve/carve#1281).
        note_unattached_block_attrs(pending_attrs_pos.take().filter(|_| pending_attrs.is_some()));
        pending_attrs = None;
        if marker.ordered != is_ordered || marker.checked.is_some() != is_task {
            break;
        }
        if !is_ordered && marker.marker != first_marker.marker {
            break;
        }
        // §11: an ordered item whose delimiter (`.` vs `)`) or dialect family
        // (decimal / alpha / roman, case included) differs from the list's first
        // item starts a NEW sibling list. Skip the FIRST item: its own detected
        // dialect may differ from the list's resolved (tie-broken) dialect
        // (`v.` detects alpha but the list is roman), and it can never split
        // from itself.
        if is_ordered
            && !items.is_empty()
            && (marker.delim != first_delim || !dialect_compatible(first_dialect, &marker))
        {
            break;
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
        if blank_run_before >= 3 {
            break;
        }
        if pending_blank
            || (!items.is_empty() && cur.pos > 0 && is_blank_line(cur.lines.at(cur.pos - 1)))
        {
            tight = false;
            pending_blank = false;
        }
        // This item's content column. For ordered/unordered it is where the
        // marker content begins (`- `=2, `1. `=3, `10. `=4). For a TASK the
        // checkbox is content, not marker, so the column is the bullet width
        // (`- `/`* ` = 2) -- a child indented to 2 nests, matching the spec's
        // task attribute/continuation convention (`- [x] x` / `  {.c}`).
        //
        // An abutting attribute block is item metadata and contributes zero,
        // so changing its spelling or Unicode value cannot move the body
        // (markup-carve/carve#1701, #1698).
        content_col = if marker.checked.is_some() {
            base_indent + 2
        } else {
            let l = cur.peek().unwrap();
            let byte_off = (marker.content.as_ptr() as usize).saturating_sub(l.as_ptr() as usize);
            indent_columns(l)
                + byte_off
                    .saturating_sub(leading_ws(l))
                    .saturating_sub(marker_attrs_width(l, &marker))
        };
        // A new item carries none of the previous item's fence state.
        item_open_fence = None;
        item_unopened_fence_span = false;
        item_unclosed_fence_base = None;
        item_rejected_fence_owner = false;
        let item_source_line = cur.source_line(cur.pos);
        let item_at = cur.pos;
        cur.consume();
        // Every item answers this for itself; only a bare-`+` lead sets it.
        last_item_bare_continuation = false;
        last_item_marker_attr_only = false;
        let item_attrs = source_line_attrs(marker.attrs.clone(), item_source_line, options);
        // First-block form `- +` (grammar §17): a lone `+` as the marker
        // content means the item's first block is the following flush-left
        // block (no inline paragraph).
        if is_plus_marker(marker.content) {
            last_item_bare_continuation = true;
            let mut item = ListItem {
                attrs: item_attrs,
                checked: marker.checked,
                task_state: marker.task_state,
                children: Vec::new(),

                pos: None,
            };
            // A first-block marker takes the opener's column or document column zero.
            let first_block_attaches = cur.continuation_decision(item_at).is_some_and(|decision| {
                decision.owner == Some(continuation_marker::Kind::List)
                    && decision.refusal.is_none()
            });
            let attached_comment = cur.peek().is_some_and(is_line_comment_any_column);
            let pull_indent = cur
                .continuation_decision(item_at)
                .filter(|decision| decision.quote_depth > 0 && decision.refusal.is_none())
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
                    item.children.push(block);
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
                    item.children.push(block);
                }
            }
            // The item runs from its marker line through the block the `+`
            // attached - it was the only list item built with a hardcoded
            // `None`, so an item written this way had no span while its
            // siblings and its own contents did.
            item.pos = span_of(cur, item_at, cur.pos, options);
            items.push(item);
            continue;
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
        if marker.content.starts_with('>') {
            let mut stream = item_marker_source(cur, marker.content, item_at);
            // Decide the fence while the outer item still exposes its closer.
            // If a below-column line ends the item first, the inner reparse
            // cannot see that closer. The synthetic boundary preserves the
            // block decision without moving the closer into the item.
            let fence_after_quote = cur.peek().is_some_and(|line| {
                let indent = indent_columns(line);
                if indent < content_col {
                    return false;
                }
                detect_fence_open(&slice_columns(line, content_col, false)).is_some_and(|open| {
                    item_body_fence_has_closer(
                        cur.lines.range(cur.pos + 1..cur.lines.len()),
                        open,
                        content_col,
                        |line, _| {
                            detect_list_marker_full(line)
                                .is_some_and(|marker| marker.indent <= base_indent)
                        },
                    )
                })
            });
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
            let mut innermost = marker.content;
            while let Some(rest) = strip_blockquote_prefix(innermost) {
                innermost = rest;
            }
            let quote_holds_a_paragraph = (!innermost.trim().is_empty()
                && body_ends_with_open_paragraph(marker.content, options))
                || colon_fences_left_open(marker.content) > 0
                || detect_fence_open(innermost).is_some()
                || detect_comment_fence_line(innermost).is_some();
            let below_empty_quote_marker = !quote_holds_a_paragraph
                && cur.peek().is_some_and(|line| {
                    detect_list_marker_full(line)
                        .is_some_and(|next| next.indent < content_col.saturating_sub(1))
                });
            let nested_marker_at_floor = !quote_holds_a_paragraph
                && cur.peek().is_some_and(|line| {
                    detect_list_marker_full(line)
                        .is_some_and(|next| next.indent == content_col.saturating_sub(1))
                });
            let first_block_quote_attaches = is_plus_marker(innermost)
                && cur.continuation_decision(item_at).is_some_and(|decision| {
                    decision.owner == Some(continuation_marker::Kind::Quote)
                        && decision.refusal.is_none()
                });
            let mut continuation_has_content = false;
            if first_block_quote_attaches {
                let start = cur.pos;
                let end = attached_block_lines_classified(
                    cur.lines,
                    start,
                    &mut cur.comment_closer_last_index,
                    &mut cur.code_closer_last_index,
                    options,
                    &mut |line, at| {
                        at > start
                            && detect_list_marker_full(line)
                                .is_some_and(|marker| marker.indent <= base_indent)
                    },
                );
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
                continuation_has_content = end > start;
            } else if !below_empty_quote_marker {
                let continuation = collect_indented_block_mapped(
                    cur,
                    if quote_holds_a_paragraph || nested_marker_at_floor {
                        base_indent
                    } else {
                        content_col.saturating_sub(1)
                    },
                    if nested_marker_at_floor {
                        content_col.saturating_sub(1)
                    } else {
                        content_col
                    },
                    options,
                );
                if fence_after_quote && !continuation.source.is_empty() {
                    stream.push_newline_at(String::new(), None, None);
                }
                continuation_has_content = !continuation.source.trim().is_empty();
                stream.append(continuation);
            }
            let after_blank = cur.pos > 0 && is_blank_line(cur.lines.at(cur.pos - 1));
            if quote_holds_a_paragraph || continuation_has_content {
                fold_lazy_run_and_resume(
                    cur,
                    &mut stream,
                    content_col,
                    options,
                    |src, below| {
                        !after_blank
                            && (nested_ends_with_open_paragraph_rebased(src, below, below, options)
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
            stream.authored_base_at_start |= after_blank;
            let children = item_body(deferred, items.len(), stream, None, false, options);
            items.push(ListItem {
                attrs: item_attrs,
                checked: marker.checked,
                task_state: marker.task_state,
                children,

                // The item runs from its marker to the last line its body
                // consumed - the bullet is part of the item, unlike the
                // paragraph inside it, which starts at the text.
                pos: span_of(cur, item_at, cur.pos, options),
            });
            if !quote_holds_a_paragraph
                && cur.peek().is_some_and(|line| {
                    let indent = indent_columns(line);
                    !is_blank_line(line)
                        && trim_ascii(line) != "+"
                        && indent > base_indent
                        && indent < content_col
                        && (detect_list_marker_full(line).is_none()
                            || indent < content_col.saturating_sub(1))
                })
            {
                break;
            }
            continue;
        }
        // Braces ALONE on the marker line are a block-attribute line for the
        // item's first block, not lead text (corpus 170). `- {a=b .c}` +
        // `  # H` therefore attributes the heading, exactly as the same two
        // lines do at document level. Braces followed by TEXT stay literal
        // (`- {.c} literal text`), which is why the whole content must parse as
        // a standalone attribute line. Routed through the block stream so the
        // pending-attribute machinery, not this lead-paragraph path, sees it.
        if parse_standalone_attrs(marker.content).is_some() {
            last_item_marker_attr_only = true;
            let mut stream = item_marker_source(cur, marker.content, item_at);
            // The marker-line attribute opens no paragraph. Only a following
            // line that reaches the item's content column can be its target;
            // a genuinely below-column line closes the item under PART 1 S4
            // and is reclassified by the surrounding document. Marker-attached
            // item metadata and task checkboxes do not widen that column
            // (markup-carve/carve#1705, carve-rs#1373).
            stream.append(collect_indented_block_mapped(
                cur,
                content_col.saturating_sub(1),
                content_col,
                options,
            ));
            let children = item_body(deferred, items.len(), stream, None, false, options);
            items.push(ListItem {
                attrs: item_attrs,
                checked: marker.checked,
                task_state: marker.task_state,
                children,
                pos: span_of(cur, item_at, cur.pos, options),
            });
            continue;
        }
        if detect_list_marker_full(marker.content).is_some() {
            // The marker line can open more than one item (`* * u`). Once a
            // flush lazy line folds into the innermost paragraph, S4 closes
            // nothing, so collection must resume against THAT item's content
            // column. Using the outer `content_col` assigned a following line
            // between the two columns to the outer item instead (#1424).
            let content_offset = cur.lines.at(item_at).len() - marker.content.len();
            let cached_tail = cur.marker_tail.filter(|_| item_at == 0);
            #[cfg(test)]
            let cached_tail = cached_tail.filter(|_| REUSE_MARKER_TAIL.with(Cell::get));
            let carried_tail = cached_tail.and_then(|tail| {
                let relative = tail.content_offset.get().checked_sub(content_offset)?;
                let column = tail.column.checked_sub(content_col)?;
                Some((column, marker.content.get(relative..)?))
            });
            let (tail_col, innermost_content) =
                carried_tail.unwrap_or_else(|| innermost_marker_tail(marker.content));
            let nested_content_col = content_col + tail_col;
            let nested_definition_ended_paragraph =
                is_collected_definition_placeholder(innermost_content);
            let mut stream = item_marker_source(cur, marker.content, item_at);
            stream.marker_tail = NonZeroUsize::new(marker.content.len() - innermost_content.len())
                .map(|content_offset| MarkerTail {
                    content_offset,
                    column: tail_col,
                });
            // A fence on the nested marker line keeps only lines that reach
            // its host. A flush-left line supplies neither item's content
            // column, so the nested parse must not receive it as lazy payload
            // (CARVE-P0-013).
            let inner_lead = trim_ascii_start(innermost_content);
            let before_block = cur.pos;
            let collection_floor =
                if nested_definition_ended_paragraph || detect_fence_open(inner_lead).is_some() {
                    content_col.saturating_sub(1)
                } else {
                    base_indent
                };
            let nested_lead_is_continuation = is_plus_marker(innermost_content);
            let before_collection = stream.source.len();
            if nested_lead_is_continuation {
                stream.append(collect_indented_block_mapped_after_continuation(
                    cur,
                    collection_floor,
                    content_col,
                    options,
                ));
            } else if !nested_definition_ended_paragraph
                || cur
                    .peek()
                    .is_some_and(|line| indent_columns(line) >= content_col)
            {
                stream.append(collect_indented_block_mapped(
                    cur,
                    collection_floor,
                    content_col,
                    options,
                ));
            }
            last_item_bare_continuation =
                nested_lead_is_continuation && stream.source.len() == before_collection;
            // A blank line closes the sub-list's last paragraph, so the next
            // flush-left line starts a NEW top-level block instead of folding in
            // (carve-rs#490). The collected source keeps no trace of a trailing
            // blank, so `nested_ends_with_open_paragraph` below still reports the
            // paragraph as open and the lazy loop swallowed `- - a` / blank / `b`
            // into the inner item, where carve-js and carve-php end the list.
            let mut ended_on_blank =
                cur.pos > before_block && is_blank_line(cur.lines.at(cur.pos - 1));
            // A below-column lazy-continuation line following the marker-line
            // sub-list folds into its last open paragraph (`- - b` / `lazy` ->
            // `<li>b\nlazy</li>`), and a following sibling marker at the
            // sub-list's column resumes the SAME list. This is the same
            // lazy-fold / resume loop the following-line nested-list path runs
            // above; reused here so the marker-line sub-list behaves identically.
            loop {
                if ended_on_blank {
                    break;
                }
                if cur
                    .closed_comment_location
                    .is_some_and(|(at, column)| at + 1 == cur.pos && column >= content_col)
                {
                    break;
                }
                if nested_definition_ended_paragraph {
                    break;
                }
                // A flush line comment ends the inner paragraph but not the
                // innermost item (§24 C3). Consume it into the nested stream,
                // then resume relative to that item's column just as the plain
                // lazy path below does. Leaving it for the outer list's comment
                // exception put the line after it in the outer item (#1424).
                let below_fence = cur
                    .peek()
                    .map(str::to_string)
                    .and_then(|line| matched_fence_below_item(cur, &line, nested_content_col));
                if cur
                    .peek()
                    .map(str::to_string)
                    .is_some_and(|line| flush_comment_keeps_the_item_open(cur, &line))
                    || below_fence.is_some()
                {
                    let line = cur.peek().unwrap();
                    stream.push_newline_at(
                        line.to_string(),
                        cur.source_line(cur.pos),
                        cur.source_col(cur.pos),
                    );
                    cur.consume();
                    if let Some(run) = below_fence {
                        append_comment_span_through_closer(cur, &mut stream, run, false);
                    }
                    collect_trailing_lazy_through(
                        cur,
                        &mut stream,
                        content_col.saturating_sub(1),
                        base_indent,
                        nested_content_col,
                    );
                    let before_block = cur.pos;
                    stream.append(collect_indented_block_mapped(
                        cur,
                        content_col - 1,
                        content_col,
                        options,
                    ));
                    ended_on_blank =
                        cur.pos > before_block && is_blank_line(cur.lines.at(cur.pos - 1));
                    continue;
                }
                let has_lazy = if let Some(line) = cur.peek() {
                    let line = line.to_string();
                    !is_blank_line(&line)
                        && indent_columns(&line) < content_col
                        && !is_list_marker(&line)
                        && !interrupts_paragraph(cur, &line)
                } else {
                    false
                };
                if !has_lazy {
                    break;
                }
                // A refused first-block marker leaves no paragraph for a lazy fold.
                if last_item_bare_continuation {
                    break;
                }
                if detect_fence_open(inner_lead).is_some()
                    && cur.pos > 0
                    && innermost_marker_column(marker.content).is_some_and(|column| {
                        let prior = cur.lines.at(cur.pos - 1);
                        column > 0
                            && indent_columns(prior) >= content_col + column
                            && line_starts_paragraph(trim_ascii_start(prior))
                            && (trim_ascii(prior) != "+"
                                || cur
                                    .continuation_decision(cur.pos - 1)
                                    .is_some_and(|decision| decision.refusal.is_some()))
                    })
                {
                    break;
                }
                if !nested_ends_with_open_paragraph_rebased(
                    &stream,
                    last_consumed_line_below_column(cur, nested_content_col),
                    last_consumed_line_below_column(cur, content_col),
                    options,
                ) {
                    break;
                }
                let before = cur.pos;
                collect_trailing_lazy_through(
                    cur,
                    &mut stream,
                    nested_content_col.saturating_sub(1),
                    base_indent,
                    nested_content_col,
                );
                if cur.pos == before {
                    break;
                }
                let before_block = cur.pos;
                stream.append(collect_indented_block_mapped(
                    cur,
                    nested_content_col - 1,
                    nested_content_col,
                    options,
                ));
                ended_on_blank = cur.pos > before_block && is_blank_line(cur.lines.at(cur.pos - 1));
            }
            // A blank line between the item's blocks loosens the list, and a
            // sub-list lead is no exception: the item holds the sub-list and
            // whatever follows the blank at THIS item's content column
            // (carve-rs#490). #476 fixed the attribute-block, quote and heading
            // leads beside this one and left the sub-list lead open, because
            // carve-php agreed with this engine at the time and carve-js did not.
            // Content at or past the SUB-LIST's content column is the sub-list's
            // own business - `sublist_source_loosens_outer_item` is the same
            // non-propagating test the following-line sub-list path uses, not the
            // blanket `continuation_source_loosens` the other leads can afford.
            let blank_after_bare_nested_marker = nested_lead_is_continuation
                && cur.lines.line(item_at + 1).is_some_and(is_blank_line)
                && cur.lines.line(item_at + 2).is_some_and(|line| {
                    indent_columns(line) == content_col
                        && (is_plus_marker(trim_ascii_start(line))
                            || line_starts_paragraph(trim_ascii_start(line)))
                });
            if sublist_source_loosens_outer_item(&stream.source) || blank_after_bare_nested_marker {
                tight = false;
            }
            if sublist_source_trailing_invisible_separation(&stream.source) {
                pending_blank = true;
            }
            if detect_fence_open(innermost_content).is_some()
                && cur.pos > before_block
                && innermost_marker_column(marker.content).is_some_and(|marker_col| {
                    indent_columns(cur.lines.at(before_block)) == content_col + marker_col
                })
                && trim_ascii(cur.lines.at(before_block)) == "+"
                && stream.source.lines().skip(1).all(is_blank_line)
            {
                if let Some(end) = stream.source.find('\n') {
                    stream.source.truncate(end);
                    stream.line_map.truncate(1);
                    stream.col_map.truncate(1);
                    stream.reached.truncate(1);
                }
            }
            if detect_container_open(inner_lead).is_some()
                || detect_fence_open(inner_lead).is_some()
            {
                item_paragraph_open =
                    ParagraphOpen::Known(body_ends_with_open_paragraph(&stream.source, options));
            }
            let children = item_body_with_layout_suppression(
                deferred,
                items.len(),
                stream,
                None,
                false,
                true,
                options,
            );
            items.push(ListItem {
                attrs: item_attrs,
                checked: marker.checked,
                task_state: marker.task_state,
                children,

                // The item runs from its marker to the last line its body
                // consumed - the bullet is part of the item, unlike the
                // paragraph inside it, which starts at the text.
                pos: span_of(cur, item_at, cur.pos, options),
            });
            if detect_fence_open(inner_lead).is_some()
                && cur.peek().is_some_and(|line| {
                    !is_blank_line(line)
                        && indent_columns(line) < content_col
                        && detect_list_marker_full(line).is_none()
                        && trim_ascii(line) != "+"
                })
            {
                break;
            }
            if nested_definition_ended_paragraph
                && cur.peek().is_some_and(|line| {
                    !is_blank_line(line)
                        && trim_ascii(line) != "+"
                        && detect_list_marker_full(line).is_none()
                        && indent_columns(line) < content_col
                })
            {
                break;
            }
            continue;
        }
        if marker_content_starts_block(marker.content, cur, content_col)
            || marker_content_is_attr_block(marker.content, cur, content_col)
            || marker_content_is_captioned_inline(marker.content, cur, content_col, options)
        {
            let mut stream = item_marker_source(cur, marker.content, item_at);
            let before_block = cur.pos;
            // A fence can open on the MARKER LINE, where its opener is the
            // marker-line content rather than a collected line - so the
            // collector's fenced-body guard is seeded from it (corpus 276).
            item_open_fence = detect_fence_open(marker.content);
            // A MARKER-LINE BLOCK THAT LEAVES NOTHING OPEN ENDS THE ITEM. PART 1
            // S4's otherwise has already ended it, so a line in the band between
            // the list's base and the item's content column has nothing here to
            // continue. The collection floor is what says so, as it does for an
            // empty quote on the marker line (carve-rs#2120).
            //
            // The predicate is what the marker line leaves OPEN, not whether it
            // holds a paragraph: a code fence or a colon container is open on one
            // line and the band line below it is its payload.
            let marker_block_leaves_content_open = item_open_fence.is_some()
                || colon_fences_left_open(marker.content) > 0
                || body_ends_with_open_paragraph(marker.content, options);
            let body_floor = if detect_comment_fence_line(marker.content).is_some()
                || is_flush_line_comment(marker.content)
                || !marker_block_leaves_content_open
            {
                content_col.saturating_sub(1)
            } else {
                base_indent
            };
            cur.marker_colon_closed = false;
            stream.append(collect_indented_block_mapped_after_fence(
                cur,
                body_floor,
                content_col,
                &mut item_open_fence,
                options,
            ));
            let retained_marker_comment = marker_block_leaves_content_open
                && stream.reached.last() == Some(&false)
                && stream
                    .source
                    .lines()
                    .collect::<Vec<_>>()
                    .split_last()
                    .is_some_and(|(close, body)| closes_a_held_comment_span(body, close));
            let marker_colon_closed = cur.marker_colon_closed;
            let marker_colon_open = !marker_colon_closed
                && colon_fences_left_open(marker.content) > 0
                && detect_line_block_open(marker.content).is_none();
            let swallowed_blank_separator =
                cur.pos > before_block && is_blank_line(cur.lines.at(cur.pos - 1));
            // A paired delimiter beyond the item's column belongs to the quote.
            // Its literal opener must not let trailing prose reclaim that line.
            let literal_comment_handoff = trim_ascii_start(marker.content).starts_with(':')
                && detect_line_block_open(marker.content).is_some()
                && body_open_comment_run(stream.source.lines())
                    .is_some_and(|run| cur.has_comment_closer_after(cur.pos, run));
            let definition_comment_handoff = is_definition_list_start(marker.content)
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
            // NOT WHILE THE FENCE IS STILL OPEN, as the continuation branch
            // above already has it. A fenced body is verbatim and holds no
            // paragraph, so there is nothing below the content column for the
            // fold to reach - and its comment arm, which is right that a comment
            // ends no container, took the line into the verbatim body instead
            // (carve-rs#2096).
            if item_open_fence.is_none() {
                // IN THE BAND, not only at column 0. The floor above hands the
                // band line to this fold instead of collecting it blind, and the
                // fold is the thing that asks S4's question - so it has to be
                // able to see a line the raised floor left behind (#2120).
                // A marker-line colon container that has closed stops the
                // collector at a band line, so the fold has to reach it.
                let (lazy_column, list_base) = if marker_block_leaves_content_open
                    && !marker_colon_closed
                    && !marker_colon_open
                {
                    (0, 0)
                } else {
                    (content_col.saturating_sub(1), base_indent)
                };
                fold_lazy_run_and_resume_in_band(
                    cur,
                    &mut stream,
                    content_col,
                    (lazy_column, list_base),
                    (false, options),
                    |src, _below| {
                        if swallowed_blank_separator
                            || literal_comment_handoff
                            || definition_comment_handoff
                        {
                            return false;
                        }
                        let mut rebased = src.clone();
                        rebase_overindented_blocks(&mut rebased, false, options);
                        retained_marker_comment
                            || body_ends_with_open_paragraph(&rebased.source, options)
                    },
                    |cur| collect_indented_block_mapped(cur, base_indent, content_col, options),
                );
            }
            // A blank absorbed inside the marker-line block's continuation that
            // is followed by a plain paragraph loosens the item (§17 L1), the
            // same rule the plain-continuation branch applies -- e.g. a
            // marker-line fence with blank-separated trailing text
            // (`- ```\n  c\n  ```\n\n  tail`).
            let refused_definition_plus = is_definition_list_start(marker.content)
                && cur
                    .lines
                    .range(item_at + 1..cur.pos)
                    .any(|line| indent_columns(line) == content_col && trim_ascii(line) == "+")
                && !cur.lines.range(item_at + 1..cur.pos).any(is_blank_line);
            // A heading, a fence or a table on the marker line leaves the item
            // holding no paragraph - see `item_paragraph_open`.
            item_paragraph_open = if literal_comment_handoff || definition_comment_handoff {
                ParagraphOpen::Known(false)
            } else {
                let mut rebased = stream.clone();
                rebase_overindented_blocks(&mut rebased, false, options);
                ParagraphOpen::deferred(move || {
                    body_ends_with_open_paragraph(&rebased.source, options)
                })
            };
            let children = item_body_with_layout_suppression(
                deferred,
                items.len(),
                stream,
                None,
                false,
                refused_definition_plus,
                options,
            );
            items.push(ListItem {
                attrs: item_attrs,
                checked: marker.checked,
                task_state: marker.task_state,
                children,

                // The item runs from its marker to the last line its body
                // consumed - the bullet is part of the item, unlike the
                // paragraph inside it, which starts at the text.
                pos: span_of(cur, item_at, cur.pos, options),
            });
            // A single-line marker-line block (heading, thematic break) leaves
            // no indented continuation, so collect_indented_block_mapped above
            // swallows the trailing blank separator before the outer loop can
            // see it. Re-raise pending_blank so a following sibling item still
            // loosens the list (`- # H\n\n- b` / `- ---\n\n- b` render `<li>` as
            // loose), matching carve-js / carve-php. A multi-line block (`:::`
            // container) leaves the blank for the outer loop, so
            // swallowed_blank_separator is false there and this does not
            // double-loosen.
            if swallowed_blank_separator {
                pending_blank = true;
                let mut at = cur.pos;
                blank_run = 0;
                while at > item_at + 1 && blank_run < 3 {
                    at -= 1;
                    let line = cur.lines.at(at);
                    if is_blank_line(line) {
                        blank_run += 1;
                    } else {
                        break;
                    }
                }
            }
            // The fold above had its chance: it takes the band line back when
            // the body does hold an open paragraph. Left behind, the line
            // reached nothing, so the LIST ends here rather than letting the
            // outer loop read it as this item's lazy continuation.
            if !marker_block_leaves_content_open
                && cur.peek().is_some_and(|line| {
                    let indent = indent_columns(line);
                    !is_blank_line(line)
                        && trim_ascii(line) != "+"
                        && detect_list_marker_full(line).is_none()
                        && indent > base_indent
                        && indent < content_col
                })
            {
                break;
            }
            continue;
        }
        // The item's first paragraph is the marker content plus any
        // immediately-following indented prose lines (lazy continuation).
        // It stops at a blank line or a list marker: a nested sublist still
        // interrupts (the one Carve deviation, grammar §10). Block openers that
        // begin ON the marker line are handled above by
        // marker_content_starts_block (heading, fence, thematic, container,
        // table); a would-be opener on a LATER lazy-continuation line stays
        // paragraph text.
        let mut para_lines = vec![marker.content.to_string()];
        let mut suppress_colon_interrupt = is_invalid_colon_fence_opener_text(marker.content);
        // The first raw line a failed wrapped-attribute lookahead has not
        // already proved cannot start one. Without this floor, a run of
        // unclosed `{.a` lines scans the whole remaining item once per line.
        let mut attrs_scan_floor = cur.pos;
        let mut anchors = options
            .positions
            .then(|| vec![inline_anchor_for_line(cur, item_at, marker.content)]);
        let mut caption_below_content_column = false;
        let literal_colon_opener = detect_container_open(marker.content)
            .map(|open| open.fence_len)
            .or_else(|| detect_line_block_open(marker.content))
            .or_else(|| detect_hardbreaks_block_open(marker.content))
            .or_else(|| detect_quote_block_open(marker.content));
        while let Some(next) = cur.peek() {
            if is_plus_marker(trim_ascii_start(next))
                && indent_columns(next) <= base_indent
                && cur.continuation_decision(cur.pos).is_some_and(|decision| {
                    decision.owner == Some(continuation_marker::Kind::List)
                        && decision.refusal.is_none()
                })
            {
                break;
            }
            if is_blank_line(next) {
                break;
            }
            if let Some(fence_len) = literal_colon_opener {
                if indent_columns(next) <= base_indent
                    && exact_colon_fence_len(next) == Some(fence_len)
                {
                    break;
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
                    break;
                }
            }
            let indent = indent_columns(next);
            // A FENCED BODY IS NOT A PARAGRAPH (§24, markup-carve/carve#950).
            // The lead paragraph may have ABSORBED a fence opener as text - §10
            // I4 keeps a fence with no closer left in the item from interrupting
            // - but the LAYOUT still reads a fence there, so a line below the
            // content column closes the item instead of folding in. The item's
            // fence state is one variable across all three of its collection
            // sites, so the outer loop below sees the same open fence and closes
            // the list at this line.
            if item_open_fence.is_some() && indent < content_col && !next.starts_with(LAZY) {
                break;
            }
            if indent < content_col {
                // A closed run at an ancestor's column interrupts this child.
                // Its host is the ancestor, so only a closer at a column that
                // host reaches counts; the child's content column does not
                // (CARVE-P0-014).
                if indent == base_indent
                    && detect_fence_open(trim_ascii_start(next)).is_some_and(|open| {
                        list_code_fence_closer_ahead(cur, open, base_indent, base_indent)
                    })
                {
                    break;
                }
                let next_owned = next.to_string();
                if interrupts_lazy_continuation_as_container(cur, &next_owned) {
                    break;
                }
                let inline_line = trim_ascii_start(next);
                // CARVE-P0-015: a lazy fold closes no item, so a later blank separates its paragraphs.
                if item_unopened_fence_span && {
                    let lazy = trim_ascii_start(strip_lazy(inline_line));
                    detect_fence_open(lazy).is_some()
                        || lazy
                            .strip_prefix('>')
                            .is_some_and(|rest| detect_fence_open(rest).is_some())
                } {
                    item_unopened_fence_span = false;
                }
                caption_below_content_column |= caption_content(inline_line).is_some();
                if let Some(anchors) = &mut anchors {
                    anchors.push(inline_anchor_for_line(cur, cur.pos, inline_line));
                }
                suppress_colon_interrupt |= is_invalid_colon_fence_opener_text(inline_line);
                para_lines.push(inline_line.to_string());
                cur.consume();
                continue;
            }
            if indent > content_col {
                // A recognized block opener may be authored past the canonical
                // content column: that visual column becomes its temporary
                // base, and the continuation collector below rebases the whole
                // block relative to it (#1705). Ordinary over-indented prose is
                // still lazy paragraph text.
                let over_indented = trim_ascii_start(next);
                if item_block_opener(over_indented)
                    && item_unclosed_fence_base.map_or(true, |base| base == indent)
                    && !detect_fence_open(over_indented).is_some_and(|open| {
                        !list_code_fence_closer_ahead(cur, open, content_col, base_indent)
                    })
                {
                    break;
                }
                if detect_fence_open(over_indented).is_some() {
                    item_unopened_fence_span = true;
                    item_unclosed_fence_base.get_or_insert(indent);
                    item_rejected_fence_owner = true;
                }
                // Fully strip ordinary prose's indent and fold it into the lead
                // paragraph (inline-parsed, so residual indent never leaks).
                // A sibling/nesting marker above the content column was already
                // handled above; only a genuine lazy interrupt ends the fold.
                let next_owned = next.to_string();
                if interrupts_lazy_continuation_as_container(cur, &next_owned) {
                    break;
                }
                let inline_line = trim_ascii_start(next);
                if let Some(anchors) = &mut anchors {
                    anchors.push(inline_anchor_for_line(cur, cur.pos, inline_line));
                }
                suppress_colon_interrupt |= is_invalid_colon_fence_opener_text(inline_line);
                para_lines.push(inline_line.to_string());
                cur.consume();
                continue;
            }
            // AT content_column: a block opener interrupts the lead paragraph and
            // nests as a child block; plain text dedents to the body's column 0.
            let dedented = slice_columns(next, content_col, false);
            let wrapped_attr_interrupts = if cur.pos < attrs_scan_floor {
                false
            } else {
                match content_column_attr_block(
                    &dedented,
                    cur.lines.range(cur.pos + 1..cur.lines.len()),
                    content_col,
                ) {
                    ContentColumnAttrBlock::Block => true,
                    ContentColumnAttrBlock::NoneWithin(lines) => {
                        attrs_scan_floor = cur.pos + lines;
                        false
                    }
                    ContentColumnAttrBlock::No => false,
                }
            };
            let fence_open = detect_fence_open(&dedented);
            let fence_interrupts = fence_open.is_some_and(|open| {
                item_body_fence_has_closer(
                    cur.lines.range(cur.pos + 1..cur.lines.len()),
                    open,
                    content_col,
                    |line, _| {
                        detect_list_marker_full(line)
                            .is_some_and(|marker| marker.indent <= base_indent)
                    },
                )
            });
            let interrupts = (wrapped_attr_interrupts
                || interrupts_paragraph_as_container(cur, &dedented)
                || fence_interrupts)
                && !(suppress_colon_interrupt && is_suppressed_colon_fence_line(&dedented));
            if interrupts {
                break;
            }
            if let Some(anchors) = &mut anchors {
                anchors.push(inline_anchor_for_line(cur, cur.pos, &dedented));
            }
            // A code-fence-shaped line with no closer is inline paragraph text
            // (§10 I4), not an open fenced body. Do not seed the layout fence
            // tracker unless a closer is written in this item's own body:
            // doing so closes the item before the next below-column lazy line
            // (corpus 367).
            let rejected_fence_has_closer = fence_open.is_some_and(|open| {
                cur.has_code_closer_after(cur.pos + 1, open.fence_char, open.fence_len)
                    && fence_interrupts
            });
            if item_open_fence.is_some() || fence_open.is_none() || rejected_fence_has_closer {
                track_collected_fence(&mut item_open_fence, &dedented, true, indent);
            } else {
                item_unopened_fence_span = true;
                item_unclosed_fence_base.get_or_insert(indent);
                item_rejected_fence_owner = true;
            }
            suppress_colon_interrupt |= is_invalid_colon_fence_opener_text(&dedented);
            para_lines.push(dedented);
            cur.consume();
        }
        // Trailing ASCII whitespace is discarded before inline escape
        // resolution.  Do this per physical line: waiting until after `join`
        // leaves an escaped space on an intermediate list-item line, which the
        // inline parser then turns into NBSP instead of the intended hard break
        // (carve-rs#855, markup-carve/carve#1028).
        let normalized_para_lines: Vec<&str> = para_lines
            .iter()
            .map(|line| trim_ascii_end(strip_lazy(line)))
            .collect();
        let para_text = normalized_para_lines.join("\n");
        let para_text = para_text.trim_end_matches([' ', '\t']);
        let children = if let Some(anchors) = anchors {
            parse_inline_lines_with_anchor(para_text, options, anchors)
        } else {
            parse_inline_with_options(para_text, options)
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
            at_content_column: !caption_below_content_column,
            block_image: false,
            pos: item_paragraph_span(
                cur,
                item_at,
                cur.pos.saturating_sub(1),
                marker.content,
                options,
            ),
        });
        if options.source_lines {
            if let Some(line) = item_source_line {
                stamp_source_line(&mut paragraph, line);
            }
        }
        // This path's item IS a paragraph, so S4 has a paragraph for a lazy line.
        item_paragraph_open = ParagraphOpen::Known(true);
        items.push(ListItem {
            attrs: item_attrs,
            checked: marker.checked,
            task_state: marker.task_state,
            children: vec![paragraph],

            pos: span_of(cur, item_at, cur.pos, options),
        });
    }
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
