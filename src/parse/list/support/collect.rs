use super::*;

/// The same collection for a MARKER-LADDER item (`- - - x`), told the content
/// columns its descendants opened.
///
/// The ladder re-parses a dedented copy of the chunk at every level, and each
/// level collects against ITS OWN content column - so a degraded comment fence
/// written at a DESCENDANT's column looked like ordinary over-indented content
/// to every frame above the innermost one, and the item that ended was the
/// wrong one (markup-carve/carve-rs#1518).
pub(crate) fn collect_indented_block_mapped_with(
    cur: &mut LineCursor,
    parent_indent: usize,
    strip_cols: usize,
    stop_at_content_column_marker: bool,
    fence: &mut Option<FenceOpen>,
    lead_is_continuation: bool,
    options: &Options<'_>,
) -> MappedSource {
    collect_indented_block_mapped_with_columns(
        cur,
        parent_indent,
        strip_cols,
        stop_at_content_column_marker,
        fence,
        lead_is_continuation,
        &mut NestedCollectionState::default(),
        options,
    )
}

/// Do the collected lines end in a `+`, or in a `+` and the attribute blocks it
/// carries to its block (carve#1238)? Either way the next line is the block it attaches.
fn follows_plus_marker(lines: &[String]) -> bool {
    let run_start = lines
        .iter()
        .rposition(|line| is_blank_line(line))
        .map_or(0, |at| at + 1);
    let Some(plus) = lines[run_start..]
        .iter()
        .rposition(|line| is_plus_marker(trim_ascii_start(line)))
        .map(|at| run_start + at)
    else {
        return false;
    };
    let column = indent_columns(&lines[plus]);
    let trimmed: Vec<&str> = lines[plus + 1..]
        .iter()
        .map(|line| trim_ascii_start(line))
        .collect();
    let mut at = plus + 1;
    while at < lines.len() {
        if indent_columns(&lines[at]) != column {
            return false;
        }
        match standalone_attrs_block_len(&trimmed[at - plus - 1..]) {
            Some(len) if at + len <= lines.len() => at += len,
            _ => return false,
        }
    }
    true
}

#[derive(Default)]
pub(crate) struct NestedCollectionState {
    columns: Vec<(usize, usize)>,
    pub(crate) fence_seen: bool,
    pub(crate) lazy_paragraph_owner: Option<usize>,
    pub(crate) tabbed_marker: bool,
}

impl std::ops::Deref for NestedCollectionState {
    type Target = Vec<(usize, usize)>;

    fn deref(&self) -> &Self::Target {
        &self.columns
    }
}

impl std::ops::DerefMut for NestedCollectionState {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.columns
    }
}

// Descendant columns survive a collection restart after a below-column comment.
#[allow(clippy::too_many_arguments)]
pub(crate) fn collect_indented_block_mapped_with_columns(
    cur: &mut LineCursor,
    parent_indent: usize,
    strip_cols: usize,
    stop_at_content_column_marker: bool,
    fence: &mut Option<FenceOpen>,
    lead_is_continuation: bool,
    nested_item_columns: &mut NestedCollectionState,
    options: &Options<'_>,
) -> MappedSource {
    let authored_base_at_start = cur.peek().is_some_and(is_blank_line);
    let container = IndentContainer {
        marker_column: parent_indent,
        content_column: strip_cols,
        owner: continuation_marker::Kind::List,
    };
    // ONE COLLECTOR FOR BOTH PATHS. Without a `line_map` there is nothing to
    // map, so both maps stay empty and the rest of the walk is the walk the
    // separate plain collector used to run line for line. Keeping it as a second
    // copy is what let a fix land in one and not the other, which split
    // `to_html` from the position path on eighteen documents
    // (markup-carve/carve-rs#908, #1490).
    //
    // ONE FLAG FOR BOTH MAPS, and it is the LINE map's. The two are gated
    // differently upstream - the line map by `source_lines || positions`, the
    // column map by `positions` alone - so with only `source_lines` asked for,
    // `col_map` is pushed and every entry is `None`, because `source_col` has
    // nothing to answer from. That is what the delegated plain path produced too,
    // and reading the column map's own gate here would produce a SHORTER vector
    // instead, which the parallel-to-`lines` invariant below forbids.
    let building_maps = cur.line_map.is_some();
    let mut lines: Vec<String> = Vec::new();
    let mut line_map = Vec::new();
    let mut col_map: Vec<Option<isize>> = Vec::new();
    // See `MappedSource::reached`.
    let mut reached: Vec<bool> = Vec::new();
    let mut opaque_blank_lines: Vec<usize> = Vec::new();
    let mut discarded_outer_blank = false;
    let mut outer_plus_after_blank = false;
    // Strip authored code once before a nested strip changes its source columns.
    let mut authored_code_base: Option<usize> = None;
    let mut colon_open: Vec<usize> = Vec::new();
    let marker_lead = cur
        .pos
        .checked_sub(1)
        .and_then(|at| detect_list_marker_full(cur.lines.at(at)));
    let marker_base = marker_lead
        .as_ref()
        .map_or(parent_indent, |marker| marker.indent);
    let marker_ordered = marker_lead.as_ref().is_some_and(|marker| marker.ordered);
    let marker_content = marker_lead.map(|marker| marker.content);
    let mut table_seen = marker_content.is_some_and(is_table_start);
    let mut table_caption_open = false;
    nested_item_columns.fence_seen |= marker_content.is_some_and(|content| {
        content.contains(['`', '~'])
            && detect_fence_open(innermost_marker_content(content)).is_some()
    });
    let marker_is_definition_lead = marker_content.is_some_and(|content| {
        let content = trim_ascii_start(content);
        is_definition_list_start(content)
            || is_collected_definition_placeholder(content)
            || (content.starts_with("[^") && parse_footnote_def_line(content).is_some())
    });
    let marker_is_definition_term =
        marker_content.is_some_and(|content| is_definition_list_start(trim_ascii_start(content)));
    // A term on the item's marker line opens at the item's content column.
    // Later terms in the body open at their own authored columns.
    let mut definition_list_column = marker_is_definition_term.then_some(strip_cols);
    let mut open_definition_body_column = marker_content
        .and_then(definition_body_content_col)
        .map(|column| strip_cols + column);
    // A nested description may end while an outer description still owns the
    // next marker. Keep that surviving body's column alongside the innermost.
    let mut surviving_definition_body_column = open_definition_body_column;
    let mut after_definition_blank = false;
    let mut marker_definition_has_description = !marker_is_definition_term;
    let mut marker_description_code_admitted = false;
    let mut marker_description_has_inline_code = false;
    let mut definition_band_inline_run = None;
    let mut marker_description_has_lazy_tail = false;
    let mut marker_description_is_paragraph = false;
    let mut nested_paragraph_column = None;
    let mut carried_lazy_owner = nested_item_columns.lazy_paragraph_owner.take();
    // A quote between the item's content column and the marker note's body
    // column belongs to the item and can take a flush-left lazy line.
    let mut marker_note_intermediate_quote_open = false;
    let mut marker_attribute_end = None;
    let mut term_code_lazy_tail = false;
    let mut term_code_tail_can_resume = false;
    let mut term_code_closer_checked = false;
    let mut term_code_start = 0;
    let mut marker_term_paragraph_open = marker_is_definition_term;
    // Track term ownership separately from paragraph retention across deeper comments.
    let mut marker_term_open = marker_is_definition_term;
    let marker_comment = marker_content
        .and_then(detect_comment_fence_line)
        .filter(|open| cur.has_comment_closer_after(cur.pos, open.fence_len));
    let literal_marker_body = marker_content.is_some_and(|content| {
        trim_ascii_start(content).starts_with(':') && detect_line_block_open(content).is_some()
    });
    let mut literal_colon = marker_content
        .and_then(detect_line_block_open)
        .map(|width| (width, strip_cols, strip_cols));
    let mut comment_fence = marker_comment.map(|open| (open.fence_len, strip_cols));
    let mut comment_fence_strip = comment_fence.map(|_| strip_cols);
    // A colon opener on this item's own marker line is a line the walk never
    // sees, so its closer would read as an anonymous opener below.
    let mut marker_colon = cur
        .pos
        .checked_sub(1)
        .and_then(|at| detect_list_marker_full(cur.lines.at(at)))
        .filter(|marker| marker.indent <= parent_indent)
        .and_then(|marker| {
            let mut seed = Vec::new();
            track_collected_colon_fence(&mut seed, marker.content, true);
            seed.pop()
        });
    let mut marker_colon_closed = false;
    let mut marker_colon_retained_para = false;
    let mut definition_ended_paragraph = false;
    let mut closed_code_leaf = false;
    // A COMMENT FENCE WITH NO CLOSER OPENS NO SPAN AND LEAVES NO PARAGRAPH.
    // PART 9 SS28 degrades it to a line comment, so a frame one level in ends at
    // a line an ancestor claimed (markup-carve/carve-rs#1512) while the OUTERMOST
    // frame keeps it, the follower being the item's own lazy line
    // (markup-carve/carve#1920). See the break below for which of the two a line
    // gets. Sticky for the rest of the item, exactly as the executable spec keeps
    // it: an at-column line after the fence does not reopen the band.
    let mut degraded_comment_fence = false;
    // The same degradation, for a fence written at a DESCENDANT's content
    // column rather than at this container's own. It closed the DESCENDANT's
    // paragraph, so this container ends only for a line that reached nothing at
    // all - which is what the carried `reached` flag answers and the local
    // column arithmetic cannot (markup-carve/carve-rs#1518).
    let mut degraded_descendant_fence = false;
    // A span that CLOSED renders nothing either, so it leaves no paragraph open
    // and the degraded reasoning applies unchanged: below the content column
    // there is nothing left for a line to continue, so this frame ends for a
    // line that reached nothing (markup-carve/carve-rs#1531). Recorded when the
    // span OPENS - the closer line does not carry the opener's column.
    let mut span_reached_this_frame = comment_fence.is_some();
    let mut closed_comment_span_above = false;
    let mut folded_code_span = false;
    let mut folded_code_column = strip_cols;
    let mut folded_code_paragraph_open = false;
    // A descendant fence may open past this collector's strip column.
    // Preserve that decision before the deferred child loses its lookahead.
    // A marker-line ladder opens descendants before collection begins.
    let mut marker_line_columns = Vec::new();
    if let Some(previous) = cur.pos.checked_sub(1) {
        let mut lead = cur.lines.at(previous);
        let mut offset = 0;
        while let Some(marker) = detect_list_marker_full_at(lead, indent_columns(lead)) {
            // A task checkbox is content; its fence starts after the bullet.
            let width = if marker.checked.is_some() {
                marker.indent + 2
            } else {
                (marker.content.as_ptr() as usize).saturating_sub(lead.as_ptr() as usize)
            };
            let column = offset + width;
            if column > strip_cols {
                marker_line_columns.push(column);
            }
            offset = column;
            lead = marker.content;
        }
    }
    let mut nested_fence: Option<FenceOpen> = None;
    let mut active_nested_fence = marker_content
        .filter(|content| content.contains(['`', '~']))
        .and_then(detect_list_marker_full)
        .and_then(|marker| detect_fence_open(innermost_marker_content(marker.content)))
        .map(|mut open| {
            open.content_col = marker_line_columns.last().copied().unwrap_or(strip_cols);
            open
        });
    let mut nested_folded_fence = false;
    let mut nested_blank_run: Option<(usize, usize, bool)> = None;
    run_indented_body!(cur.lines, cur.pos, container, |line, admission| {
        if admission.blank {
            table_caption_open = false;
        }
        let caption_follower = table_caption_open
            && !admission.blank
            && caption_content(trim_ascii_start(line)).is_none()
            && !is_line_comment_any_column(line)
            && !is_list_marker(line)
            && !interrupts_lazy_continuation_as_container(cur, trim_ascii_start(line));
        after_definition_blank |= is_blank_line(line);
        let follows_first_block = (!lead_is_continuation
            || item_block_opener(trim_ascii_start(line)))
            && cur.pos.checked_sub(1).is_some_and(|previous| {
                cur.continuation_decision(previous)
                    .is_some_and(|decision| decision.first_block && decision.refusal.is_none())
            });
        if follows_first_block {
            let start = cur.pos;
            let end = attached_indented_block_end(cur.lines, start, strip_cols, options)
                .unwrap_or_else(|| {
                    attached_block_lines_classified(
                        cur.lines,
                        start,
                        &mut cur.comment_closer_last_index,
                        &mut cur.code_closer_last_index,
                        options,
                        &mut |next, _| is_blank_line(next) || is_plus_marker(next),
                    )
                })
                .max(start + 1);
            // A fence attached below an indented `+` line closes only at its own column; a
            // line below it has left the item and is payload, never its closer (carve#3421).
            let fence_column = detect_fence_open(trim_ascii_start(cur.lines.at(start)))
                .map(|_| indent_columns(cur.lines.at(start)))
                .filter(|&column| {
                    column >= strip_cols && indent_columns(cur.lines.at(start - 1)) > 0
                });
            while cur.pos < end {
                let raw = cur.lines.at(cur.pos);
                let (body, consumed, synthetic) = slice_columns_mapped(raw, strip_cols, true);
                let body = if cur.pos > start
                    && fence_column
                        .is_some_and(|column| !is_blank_line(raw) && indent_columns(raw) < column)
                {
                    format!("{LAZY}{}", trim_ascii_start(raw))
                } else {
                    body
                };
                lines.push(body);
                reached.push(false);
                if building_maps {
                    line_map.push(cur.source_line(cur.pos));
                    col_map.push(
                        cur.source_col(cur.pos)
                            .map(|col| col + consumed as isize - synthetic as isize),
                    );
                }
                cur.consume();
            }
            continue;
        }
        if is_plus_marker(trim_ascii_start(line))
            && (indent_columns(line) == parent_indent
                || definition_ended_paragraph && indent_columns(line) == strip_cols)
            && container.owns_continuation(line, cur.continuation_decision(cur.pos))
        {
            outer_plus_after_blank |= lines.last().is_some_and(|prior| is_blank_line(prior));
            break;
        }
        if nested_bare_marker_then_plain(cur, strip_cols)
            || refused_first_block_below_column(cur, strip_cols)
        {
            break;
        }
        if active_nested_fence.is_some_and(|open| {
            indent_columns(line) >= open.content_col && is_fence_close(trim_ascii_start(line), open)
        }) {
            active_nested_fence = None;
        }
        if nested_blank_run.is_some_and(|(end, _, _)| cur.pos >= end) {
            nested_blank_run = None;
        }
        outer_plus_after_blank |= trim_ascii(line) == "+"
            && indent_columns(line) == strip_cols
            && lines.last().is_some_and(|prior| is_blank_line(prior))
            && cur.continuation_decision(cur.pos).is_some_and(|decision| {
                decision.owner == Some(continuation_marker::Kind::List)
                    && decision.refusal.is_none()
            });
        if trim_ascii(line) == "+"
            && cur
                .continuation_decision(cur.pos)
                .is_some_and(|decision| decision.refusal.is_none())
            && indent_columns(line) < strip_cols
            && !lines
                .last()
                .is_some_and(|prior| line_starts_paragraph(prior))
            && (recent_nested_fence_lead(&lines)
                || nested_fence.is_some()
                || active_nested_fence.is_some())
            && (indent_columns(line) > 0
                || cur
                    .lines
                    .line(cur.pos + 1)
                    .is_some_and(|next| !is_blank_line(next)))
        {
            if indent_columns(line) > 0 {
                *fence = active_nested_fence.or(nested_fence);
            }
            // An empty nested fence keeps no manufactured payload line.
            if indent_columns(line) == 0
                && !cur.in_item_body
                && !lines.last().is_some_and(|prior| is_blank_line(prior))
                && !(lines.is_empty() && active_nested_fence.is_some())
            {
                lines.push(String::new());
                reached.push(false);
                if building_maps {
                    line_map.push(None);
                    col_map.push(None);
                }
                if cur
                    .lines
                    .line(cur.pos + 1)
                    .is_some_and(|next| detect_list_marker_full(next).is_some())
                {
                    lines.push(String::new());
                    reached.push(false);
                    if building_maps {
                        line_map.push(None);
                        col_map.push(None);
                    }
                }
            }
            break;
        }
        if strip_cols > 0
            && matches!(line.as_bytes().first(), Some(b'`' | b'~'))
            && cur.pos > 0
            && is_line_comment_any_column(cur.lines.at(cur.pos - 1))
            && detect_fence_open(line)
                .is_some_and(|open| cur.has_list_fence_closer_after(cur.pos, open))
        {
            break;
        }
        if comment_fence.is_some_and(|(width, opener_column)| {
            opener_column >= strip_cols
                && !is_blank_line(line)
                && !line.starts_with(LAZY)
                && indent_columns(line) < strip_cols
                && indent_columns(line) < opener_column
                && !is_comment_fence_close_any_column(line, width)
        }) {
            break;
        }
        if is_definition_placeholder(line) && indent_columns(line) < strip_cols {
            break;
        }
        if admission.blank {
            // A nested marker-line fence keeps one separator before a plus at
            // its marker column. A direct item fence keeps every payload blank.
            if active_nested_fence.is_some()
                && nested_blank_run.is_none()
                && (recent_nested_fence_lead(&lines)
                    || marker_content.is_some_and(|content| {
                        detect_fence_open(innermost_marker_content(content)).is_some()
                    }))
            {
                let end = container.blank_run_end(cur.lines, cur.pos + 1);
                let outer_follower = cur.lines.line(end).is_some_and(|next| {
                    !is_blank_line(next)
                        && indent_columns(next) >= strip_cols
                        && !container.owns_continuation(next, cur.continuation_decision(end))
                        && !active_nested_fence.is_some_and(|open| {
                            indent_columns(next) == open.content_col
                                && is_fence_close(trim_ascii_start(next), open)
                        })
                });
                let direct_fence_payload = lines.is_empty()
                    || lines
                        .iter()
                        .rev()
                        .find(|line| !is_blank_line(line))
                        .is_some_and(|line| {
                            detect_fence_open(innermost_marker_content(line)).is_some()
                        });
                let applies = outer_follower
                    && (!direct_fence_payload
                        || cur
                            .lines
                            .line(end)
                            .is_some_and(|next| is_plus_marker(trim_ascii_start(next))));
                if outer_follower && direct_fence_payload && !applies {
                    discarded_outer_blank = true;
                }
                nested_blank_run = Some((end, 0, applies));
            }
            if let Some((end, kept, applies)) = nested_blank_run.as_mut() {
                if *applies && cur.pos < *end {
                    if *kept >= 1 {
                        discarded_outer_blank = true;
                        cur.consume();
                        continue;
                    }
                    *kept += 1;
                }
            }
            nested_folded_fence = false;
            marker_note_intermediate_quote_open = false;
            nested_paragraph_column = None;
            definition_band_inline_run = None;
            marker_colon_retained_para = false;
            if comment_fence.is_none() && fence.is_none() {
                marker_term_open = false;
            }
            marker_term_paragraph_open = false;
            if term_code_lazy_tail && fence.is_some() {
                term_code_tail_can_resume = true;
            }
            term_code_lazy_tail = false;
            // INSIDE AN OPEN FENCE A BLANK IS CONTENT. Mirrors the plain
            // collector: the lookahead below asks whether the ITEM continues,
            // which is the wrong question while a fence is running. The plain
            // path was fixed in #911 and this one was not, so `--html` (which
            // takes the plain path) and `--json`/`fmt` (which take this one)
            // parsed the same document differently (markup-carve/carve-rs#908).
            // What lies past the opener column is the code line's content
            // (PART 11 §7), so a line of spaces keeps that residue.
            if fence.is_some() && !folded_code_span {
                let (residue, consumed, synthetic) =
                    slice_columns_mapped(line, authored_code_base.unwrap_or(strip_cols), true);
                opaque_blank_lines.push(lines.len());
                lines.push(residue);
                if cur.line_map.is_some() {
                    line_map.push(cur.source_line(cur.pos));
                }
                reached.push(false);
                if building_maps {
                    col_map.push(
                        cur.source_col(cur.pos)
                            .map(|col| col + consumed as isize - synthetic as isize),
                    );
                }
                cur.consume();
                continue;
            }
            // Lazy continuation does not cross a blank line: after a blank, only
            // keep collecting if the next non-blank line is still indented to the
            // block's own level. A shallower line (e.g. a dedent landing below a
            // sublist) ends the block and is left for the caller, so it can close
            // the list rather than fold in (grammar §10, corpus 81-list-lazy-5).
            {
                let mut k = cur.pos + 1;
                while k < cur.lines.len() && is_blank_line(cur.lines.at(k)) {
                    k += 1;
                }
                // The item's CONTENT COLUMN outright. It used to be
                // `min(block_indent, content_col)`, for the sibling-marker case
                // (carve-rs#301): the collected block is indented DEEPER than
                // the column and a marker below it still belongs to the item.
                // But there both spellings give the column, so the min only ever
                // differed when the block was SHALLOWER - and a block below the
                // content column is text, not a block (§24 C3). Taking the
                // column directly is the same rule with nothing left to get
                // wrong, and it is what made the comment special-casing this
                // guard once needed redundant.
                let threshold = strip_cols;
                let continues = k < cur.lines.len()
                    && (indent_columns(cur.lines.at(k)) >= threshold
                        || (comment_fence.is_some() && cur.lines.at(k).starts_with(LAZY)));
                if !continues {
                    if folded_code_span {
                        break;
                    }
                    // Descendants may own these blanks as fence payload. Copy
                    // them without advancing the outer separator cursor.
                    for at in cur.pos..k {
                        let (residue, consumed, synthetic) =
                            slice_columns_mapped(cur.lines.at(at), strip_cols, true);
                        opaque_blank_lines.push(lines.len());
                        lines.push(residue);
                        reached.push(false);
                        if building_maps {
                            line_map.push(cur.source_line(at));
                            col_map.push(
                                cur.source_col(at)
                                    .map(|col| col + consumed as isize - synthetic as isize),
                            );
                        }
                    }
                    break;
                }
            }
            // The residue stays for a descendant's open fence, which this
            // frame does not track; a blank line reads as blank either way.
            let (residue, consumed, synthetic) = slice_columns_mapped(line, strip_cols, true);
            if active_nested_fence.is_some() || nested_fence.is_some() || comment_fence.is_some() {
                opaque_blank_lines.push(lines.len());
            }
            lines.push(residue);
            if building_maps {
                line_map.push(cur.source_line(cur.pos));
            }
            reached.push(false);
            if building_maps {
                col_map.push(
                    cur.source_col(cur.pos)
                        .map(|col| col + consumed as isize - synthetic as isize),
                );
            }
            cur.consume();
            continue;
        }
        let indent = indent_columns(line);
        let plain_nested_lazy = fence.is_none()
            && comment_fence.is_none()
            && colon_open.is_empty()
            && indent >= strip_cols
            && line_starts_paragraph(trim_ascii_start(line))
            && !(is_plus_marker(trim_ascii_start(line))
                && cur
                    .continuation_decision(cur.pos)
                    .is_some_and(|decision| decision.refusal.is_none())
                && cur.lines.line(cur.pos + 1).is_some_and(|next| {
                    line_starts_paragraph(trim_ascii_start(next))
                        && !item_block_opener(trim_ascii_start(next))
                }))
            && detect_list_marker_full_at(line, indent).is_none()
            && !item_block_opener_with_invisible_arms(trim_ascii_start(line), true)
            && !is_reference_definition(trim_ascii_start(line));
        let term = indent >= strip_cols
            && fence.is_none()
            && comment_fence.is_none()
            && literal_colon.is_none()
            && is_definition_list_start(trim_ascii_start(line));
        let below_definition_term = indent > parent_indent
            && indent < strip_cols
            && definition_list_column.is_some_and(|column| indent < column)
            && fence.is_none()
            && comment_fence.is_none()
            && literal_colon.is_none()
            && is_definition_list_start(trim_ascii_start(line));
        let description = strip_description_prefix(trim_ascii_start(line)).is_some()
            && fence.is_none()
            && comment_fence.is_none()
            && literal_colon.is_none();
        if after_definition_blank
            && !term
            && !(description && definition_list_column == Some(indent))
            && definition_list_column.is_some_and(|column| indent <= column)
        {
            definition_list_column = None;
            surviving_definition_body_column = None;
        }
        if term && !below_definition_term {
            if definition_list_column.is_some_and(|column| indent <= column) {
                surviving_definition_body_column = None;
            }
            open_definition_body_column = None;
            definition_list_column = Some(match definition_list_column {
                Some(column) if !after_definition_blank => column.min(indent),
                _ => indent,
            });
        } else if description {
            open_definition_body_column = definition_body_content_col(line);
            if let Some(column) = open_definition_body_column {
                surviving_definition_body_column = Some(
                    surviving_definition_body_column.map_or(column, |outer| outer.min(column)),
                );
            }
        } else if !description
            && definition_list_column.is_some()
            && item_block_opener(trim_ascii_start(line))
        {
            definition_list_column = None;
            surviving_definition_body_column = None;
        }
        after_definition_blank = false;
        // CARVE-P2-017: a separator below the list column cannot open a dd.
        let below_definition_separator = below_definition_term
            || (definition_list_column.is_some_and(|column| indent < column) && description);
        if below_definition_separator && indent == 0 {
            break;
        }
        // At a nested item's marker column the separator ends that item instead of
        // folding into its term (CARVE-P2-017, carve#3473).
        let separator_at_nested_marker = below_definition_separator
            && indent == strip_cols
            && definition_list_column.is_some_and(|list| {
                marker_line_columns.last() == Some(&list)
                    || nested_item_columns.iter().any(|&(base, _)| base == indent)
                        && nested_item_columns
                            .iter()
                            .any(|&(_, content)| content == list)
            });
        // At or past the column of the item that owns the list it is item paragraph text (carve#3474).
        let nested_owner_content = definition_list_column.and_then(|list| {
            marker_line_columns
                .iter()
                .copied()
                .chain(nested_item_columns.iter().map(|&(_, content)| content))
                .filter(|&content| content <= list)
                .max()
        });
        let separator_in_own_item = below_definition_separator
            && !separator_at_nested_marker
            && indent >= strip_cols
            && definition_list_column.is_some_and(|column| column <= strip_cols)
            && nested_owner_content.is_none();
        // Past a nested owner's content column the nested collector decides.
        let separator_for_nested_item = below_definition_separator
            && !separator_at_nested_marker
            && nested_owner_content.is_some_and(|content| indent >= content);
        if separator_in_own_item {
            definition_list_column = None;
            surviving_definition_body_column = None;
            open_definition_body_column = None;
            marker_term_open = false;
            marker_term_paragraph_open = false;
        }
        if indent == strip_cols
            && (item_block_opener(trim_ascii_start(line))
                || detect_comment_fence_line_any_column(line).is_some())
        {
            nested_paragraph_column = None;
        }
        if nested_paragraph_column.is_some_and(|owner| indent < owner)
            && indent >= strip_cols
            && is_reference_definition(trim_ascii_start(line))
        {
            nested_paragraph_column = None;
        }
        if nested_paragraph_column.is_some_and(|owner| indent >= owner)
            && item_block_opener_with_invisible_arms(trim_ascii_start(line), true)
        {
            nested_paragraph_column = None;
        }
        if nested_paragraph_column.is_some_and(|owner| indent < owner)
            && indent >= strip_cols
            && detect_fence_open(trim_ascii_start(line)).is_some_and(|open| {
                list_code_fence_closer_ahead(cur, open, strip_cols, parent_indent)
            })
        {
            nested_paragraph_column = None;
        }
        if marker_note_intermediate_quote_open
            && indent >= strip_cols
            && item_block_opener(trim_ascii_start(line))
            && !strip_blockquote_prefix(trim_ascii_start(line))
                .is_some_and(|rest| line_starts_paragraph(trim_ascii_start(rest)))
        {
            marker_note_intermediate_quote_open = false;
        }
        if marker_is_definition_lead
            && lines.is_empty()
            && indent > strip_cols
            && (!marker_term_open || is_definition_list_start(trim_ascii_start(line)))
            && (strip_blockquote_prefix(trim_ascii_start(line)).is_some()
                || is_definition_list_start(trim_ascii_start(line)))
        {
            authored_code_base = Some(indent);
            marker_note_intermediate_quote_open = marker_content
                .is_some_and(is_collected_definition_placeholder)
                && indent == strip_cols + 1
                && strip_blockquote_prefix(trim_ascii_start(line)).is_some();
        }

        if marker_is_definition_term
            && marker_definition_has_description
            && fence.is_none()
            && comment_fence.is_none()
            && indent < strip_cols
            && is_line_comment_any_column(line)
            && detect_comment_fence_line_any_column(line).is_none()
        {
            lines.push(trim_ascii_start(line).to_owned());
            if building_maps {
                line_map.push(cur.source_line(cur.pos));
                col_map.push(
                    cur.source_col(cur.pos)
                        .map(|col| col + leading_ws(line) as isize),
                );
            }
            reached.push(false);
            cur.consume();
            if marker_description_is_paragraph
                || cur.peek().is_some_and(|next| {
                    is_blank_line(next)
                        || indent_columns(next) > marker_base
                        || detect_fence_open(trim_ascii_start(next)).is_some()
                })
            {
                continue;
            }
            break;
        }

        if indent < strip_cols
            && fence.is_none()
            && comment_fence.is_none()
            && cur.pos.checked_sub(1).is_some_and(|at| {
                indent_columns(cur.lines.at(at)) >= strip_cols
                    && is_line_comment_any_column(cur.lines.at(at))
            })
        {
            break;
        }
        if indent < strip_cols
            && cur.carried_reach(cur.pos) == Some(true)
            && item_block_opener(trim_ascii_start(line))
            && (cur
                .closed_comment_location
                .is_some_and(|(at, column)| at + 1 == cur.pos && column < strip_cols)
                || cur.pos.checked_sub(1).is_some_and(|at| {
                    indent_columns(cur.lines.at(at)) < strip_cols
                        && is_line_comment_any_column(cur.lines.at(at))
                }))
        {
            break;
        }
        if marker_definition_has_description
            && indent <= strip_cols
            && fence.is_none()
            && comment_fence.is_none()
            && colon_open.is_empty()
            && (is_line_comment_any_column(line)
                || detect_comment_fence_line_any_column(line).is_some())
        {
            marker_description_is_paragraph = false;
        }
        if literal_colon.is_some_and(|(_, _, floor)| indent < floor)
            && !is_line_comment_any_column(line)
            && detect_comment_fence_line_any_column(line).is_none()
        {
            literal_colon = None;
        }
        if term_code_tail_can_resume {
            term_code_lazy_tail = indent >= strip_cols && fence.is_some();
            term_code_tail_can_resume = false;
        }
        // A LAZY-framed line has NO column (PART 0), so under an OPEN fence it is
        // the fence's own verbatim body wherever it sits - a fence's content is
        // not re-scanned (carve#1958). This is how a flush-left body folded into
        // a nested item-lead fence reaches it (carve-rs#1547/#1559).
        if indent < strip_cols && term_code_lazy_tail && !term_code_closer_checked {
            if let Some(open) = *fence {
                term_code_lazy_tail = !list_code_fence_closer_ahead_from(
                    cur,
                    open,
                    strip_cols,
                    parent_indent,
                    term_code_start,
                );
                term_code_closer_checked = true;
            }
        }
        let fence_owns_flush_left = (line.starts_with(LAZY)
            && (marker_colon.is_some() || !colon_open.is_empty()))
            || ((fence.is_some() || (comment_fence.is_some() && !literal_marker_body))
                && line.starts_with(LAZY))
            || (indent < strip_cols
                && term_code_lazy_tail
                && fence.is_some()
                && !is_definition_placeholder(trim_ascii_start(line))
                && !is_list_marker(line)
                && (is_line_comment_any_column(line)
                    || !interrupts_lazy_continuation_as_container(cur, line)))
            || (folded_code_span
                && folded_code_paragraph_open
                && (!is_list_marker(line) || indent > parent_indent)
                && !interrupts_lazy_continuation_as_container(cur, line))
            || (!colon_open.is_empty()
                && indent < strip_cols
                && !is_list_marker(line)
                && lines.last().is_some_and(|prior| {
                    line_starts_paragraph(prior)
                        && opens_colon_body_paragraph(prior, None, true).is_some()
                        && exact_colon_fence_len(trim_ascii_start(prior)).is_none()
                        && detect_comment_fence_line_any_column(prior).is_none()
                        && detect_fence_open(trim_ascii_start(prior)).is_none()
                })
                && !interrupts_lazy_continuation_as_container(cur, line));
        // The first block's attachment is resolved before ordinary item collection.
        if lead_is_continuation && lines.is_empty() {
            if indent == 0 && detect_list_marker_full(line).is_some() {
                break;
            }
            if indent == 0 {
                // A fence takes its payload and closer along (carve#3421): the first block
                // is the whole fence, not its opener line.
                let end = if detect_fence_open(line).is_some() {
                    attached_block_lines_classified(
                        cur.lines,
                        cur.pos,
                        &mut cur.comment_closer_last_index,
                        &mut cur.code_closer_last_index,
                        options,
                        &mut |next, _| is_blank_line(next) || is_plus_marker(next),
                    )
                    .max(cur.pos + 1)
                } else {
                    cur.pos + 1
                };
                while cur.pos < end {
                    let line = cur.lines.at(cur.pos);
                    // THE MAPS MOVE WITH THE LINE. This collector keeps `line_map`
                    // and `col_map` parallel to `lines`, and a push that skips them
                    // desynchronises all three - which does not fail here but panics
                    // later, where a slice of the map is taken by the line range.
                    lines.push(slice_columns(line, 0, true).to_string());
                    if cur.line_map.is_some() {
                        line_map.push(cur.source_line(cur.pos));
                    }
                    reached.push(false);
                    if building_maps {
                        col_map.push(cur.source_col(cur.pos));
                    }
                    cur.consume();
                }
                continue;
            }
            if indent < strip_cols
                || (indent == strip_cols && detect_list_marker_full(line).is_none())
            {
                break;
            }
        }
        if nested_fence.is_some() && indent < strip_cols {
            *fence = nested_fence;
            break;
        }
        let prior_comment = cur
            .pos
            .checked_sub(1)
            .is_some_and(|at| effective_comment_line_at(cur, at));
        let definition_comment_band = marker_is_definition_term
            && marker_definition_has_description
            && fence.is_none()
            && comment_fence.is_none()
            && (indent > marker_base || detect_fence_open(trim_ascii_start(line)).is_some())
            && indent < strip_cols
            && prior_comment;
        let definition_band_inline_closer = definition_band_inline_run.is_some_and(|run| {
            if last_backtick_run_starts(line.as_bytes()).contains_key(&run) {
                definition_band_inline_run = None;
                trim_ascii_start(line)
                    .bytes()
                    .take_while(|&byte| byte == b'`')
                    .count()
                    == run
            } else {
                false
            }
        });
        if definition_comment_band {
            let content = trim_ascii_start(line);
            let run = content.bytes().take_while(|&byte| byte == b'`').count();
            if run >= 3 && !last_backtick_run_starts(&content.as_bytes()[run..]).contains_key(&run)
            {
                definition_band_inline_run = Some(run);
            }
        }
        // A CLOSER IS PART OF THE SPAN, NOT A DEDENT. See
        // `body_open_comment_run`: this dedent ends the container, and a comment
        // delimiter closing a span these lines already hold is not one.
        let comment_boundary = indent < strip_cols
            && (marker_colon.is_some()
                || (marker_is_definition_term && marker_definition_has_description))
            && (marker_ordered
                || cur.pos.checked_sub(1).is_some_and(|at| {
                    indent_columns(cur.lines.at(at)) > marker_base && prior_comment
                }))
            && (cur.pos.checked_sub(1).is_some_and(|at| {
                let previous = cur.lines.at(at);
                indent_columns(previous) < strip_cols && prior_comment
            }) || cur
                .closed_comment_location
                .is_some_and(|(at, _)| at + 1 == cur.pos));
        let trailing_comment_lines = if prior_comment && ends_with_empty_comment_fence(&lines) {
            2
        } else if prior_comment
            && lines
                .last()
                .is_some_and(|line| detect_comment_fence_line_any_column(line).is_some())
        {
            1
        } else {
            0
        };
        let after_retaining_comment = indent < strip_cols
            && (prior_comment
                || cur
                    .closed_comment_location
                    .is_some_and(|(at, _)| at + 1 == cur.pos));
        let paragraph_before_comment = (comment_boundary || after_retaining_comment)
            && (lines[..lines.len() - trailing_comment_lines]
                .iter()
                .rposition(|line| !is_blank_line(line) && !is_line_comment_any_column(line))
                .is_some_and(|end| {
                    let mut body = marker_content.unwrap_or_default().to_owned();
                    body.push('\n');
                    body.push_str(&lines[..=end].join("\n"));
                    body_ends_with_open_paragraph(&body, &Options::default())
                })
                || marker_content.is_some_and(|content| {
                    lines.len() == trailing_comment_lines
                        && (body_ends_with_open_paragraph(content, &Options::default())
                            || strip_blockquote_prefix(content).is_some_and(line_starts_paragraph))
                }));
        let comment_retains_owner = comment_boundary
            && paragraph_before_comment
            && line_starts_paragraph(line)
            && !item_block_opener(line)
            && !interrupts_lazy_continuation_as_container(cur, line);
        let retained_item_fence = indent < strip_cols
            && paragraph_before_comment
            && !marker_is_definition_term
            && after_retaining_comment
            && detect_fence_open(trim_ascii_start(line)).is_some_and(|open| {
                !cur.has_code_closer_after(cur.pos + 1, open.fence_char, open.fence_len)
                    && !item_body_fence_has_closer(
                        cur.lines.range(cur.pos + 1..cur.lines.len()),
                        open,
                        strip_cols,
                        |next, _| {
                            detect_list_marker_full(next)
                                .is_some_and(|marker| marker.indent <= parent_indent)
                        },
                    )
            });
        if indent <= parent_indent
            && !(plain_nested_lazy && carried_lazy_owner.is_some_and(|owner| indent < owner))
            && !comment_retains_owner
            && !retained_item_fence
            && !(marker_note_intermediate_quote_open
                && line_starts_paragraph(line)
                && !item_block_opener(line))
            && !definition_comment_band
            && !definition_band_inline_closer
            && !fence_owns_flush_left
            && !comment_fence.is_some_and(|(run, _)| is_comment_fence_close_any_column(line, run))
            && !closes_a_held_comment_span(&lines, line)
            && !caption_follower
        {
            break;
        }
        if indent < strip_cols
            && is_definition_list_start(trim_ascii_start(line))
            && !below_definition_term
            && fence.is_none()
            && comment_fence.is_none()
            && colon_open.is_empty()
        {
            break;
        }
        if (definition_ended_paragraph || closed_code_leaf)
            && indent < strip_cols
            && !definition_comment_band
            && !definition_band_inline_closer
            && !retained_item_fence
        {
            break;
        }
        // ONLY FOR A LINE AN ANCESTOR ALREADY CLAIMED. PART 9 SS28's degradation
        // is total, so at the outermost frame the follower is the item's own lazy
        // line and stays, exactly as it does under `%%` (markup-carve/carve#1920,
        // corpus 446). One frame in, the carried flag says the line reached an
        // ancestor and not this container, and the frame still ends there.
        if degraded_comment_fence && indent < strip_cols {
            break;
        }
        if degraded_descendant_fence && !cur.line_reached(cur.pos, indent, strip_cols) {
            break;
        }
        if closed_comment_span_above && indent < strip_cols {
            break;
        }
        if indent >= strip_cols && comment_fence.is_none() && !is_line_comment_any_column(line) {
            closed_comment_span_above = false;
        }
        if fence.is_some() && indent < strip_cols && !fence_owns_flush_left {
            break;
        }
        if !colon_open.is_empty()
            && indent < strip_cols
            && !fence_owns_flush_left
            && !retained_item_fence
        {
            break;
        }
        if marker_colon_retained_para
            && indent >= strip_cols
            && item_block_opener(trim_ascii_start(line))
            && strip_blockquote_prefix(trim_ascii_start(line)).is_none()
            && !is_line_comment_any_column(line)
            && comment_fence.is_none()
            && fence.is_none()
        {
            marker_colon_retained_para = false;
        }
        if marker_colon.is_some()
            && !literal_marker_body
            && indent < strip_cols
            && indent >= parent_indent
            && !marker_colon_retained_para
        {
            let after_comment = cur.pos.checked_sub(1).is_some_and(|at| {
                let previous = cur.lines.at(at);
                indent_columns(previous) < strip_cols && prior_comment
            });
            let after_span = cur
                .closed_comment_location
                .is_some_and(|(at, _)| at + 1 == cur.pos);
            if (after_comment || after_span)
                && paragraph_before_comment
                && (!interrupts_lazy_continuation_as_container(cur, line) || retained_item_fence)
            {
                marker_colon_retained_para = true;
            }
        }
        if marker_colon.is_some()
            && !literal_marker_body
            && !marker_colon_retained_para
            && !fence_owns_flush_left
            && !retained_item_fence
            && fence.is_none()
            && comment_fence.is_none()
            && indent < strip_cols
            && !(indent > parent_indent
                && line_starts_paragraph(line)
                && !item_block_opener(trim_ascii_start(line))
                && lines
                    .last()
                    .is_some_and(|prior| line_starts_paragraph(prior)))
            && !is_line_comment_any_column(line)
            && detect_comment_fence_line_any_column(line).is_none()
        {
            break;
        }
        if marker_colon_closed && indent < strip_cols && !fence_owns_flush_left {
            break;
        }
        // A below-column comment stays only to close an open paragraph; with
        // none in the marker-line container it ends the item, as its closed
        // twin does (corpus 556-13).
        if marker_colon.is_some()
            && !literal_marker_body
            && fence.is_none()
            && comment_fence.is_none()
            && !fence_owns_flush_left
            && indent < strip_cols
            && (is_line_comment_any_column(line)
                || detect_comment_fence_line_any_column(line).is_some())
            && !lines
                .iter()
                .rev()
                .find(|line| !is_blank_line(line) && !is_line_comment_any_column(line))
                .is_some_and(|line| line_starts_paragraph(line))
        {
            break;
        }
        if let Some(open) = nested_fence {
            if indent < open.content_col {
                nested_fence = None;
                nested_item_columns.clear();
            } else if indent == open.content_col && is_fence_close(trim_ascii_start(line), open) {
                nested_fence = None;
            }
        } else if fence.is_none() && comment_fence.is_none() && colon_open.is_empty() {
            if let Some(marker) = detect_list_marker_full_at(line, indent) {
                nested_item_columns.tabbed_marker |= line[..leading_ws(line)].contains('\t');
                if active_nested_fence.is_none() && marker.content.contains(['`', '~']) {
                    active_nested_fence = detect_fence_open(innermost_marker_content(
                        marker.content,
                    ))
                    .map(|mut open| {
                        open.content_col =
                            marker_content_col_at(line, indent).unwrap_or(strip_cols);
                        open
                    });
                }
                nested_item_columns.fence_seen |= marker.content.contains(['`', '~'])
                    && detect_fence_open(innermost_marker_content(marker.content)).is_some();
                nested_folded_fence = false;
                while nested_item_columns
                    .last()
                    .is_some_and(|&(base, _)| base >= marker.indent)
                {
                    nested_item_columns.pop();
                }
                if let Some(column) = marker_content_col_at(line, indent) {
                    nested_item_columns.push((marker.indent, column));
                }
            } else {
                if lines.last().is_some_and(|line| is_blank_line(line)) {
                    while nested_item_columns
                        .last()
                        .is_some_and(|&(_, column)| indent < column)
                    {
                        nested_item_columns.pop();
                    }
                }
                if let Some(mut open) = detect_fence_open(trim_ascii_start(line)) {
                    if let Some(&(base, column)) = nested_item_columns.last() {
                        if column > strip_cols
                            && indent >= column
                            && lines.last().is_some_and(|line| {
                                line_starts_paragraph(innermost_marker_content(trim_ascii_start(
                                    line,
                                )))
                            })
                        {
                            // CARVE-P0-014: only an in-child closer lets this run interrupt its paragraph.
                            nested_folded_fence = !item_body_fence_has_closer(
                                cur.lines.range(cur.pos + 1..cur.lines.len()),
                                open,
                                column,
                                |line, _| {
                                    detect_list_marker_full(line)
                                        .is_some_and(|marker| marker.indent <= base)
                                },
                            );
                        }
                    }
                    if let Some(&(base, column)) = nested_item_columns
                        .iter()
                        .rev()
                        .find(|&&(_, column)| indent == column && column > strip_cols)
                    {
                        let after_blank = lines.last().is_some_and(|line| is_blank_line(line));
                        let interrupts = !after_blank
                            && cur.has_code_closer_after(
                                cur.pos + 1,
                                open.fence_char,
                                open.fence_len,
                            )
                            && item_body_fence_has_closer(
                                cur.lines.range(cur.pos + 1..cur.lines.len()),
                                open,
                                column,
                                |line, _| {
                                    detect_list_marker_full(line)
                                        .is_some_and(|marker| marker.indent <= base)
                                },
                            );
                        if after_blank || interrupts {
                            if !after_blank {
                                // Preserve the accepted interruption when collection
                                // ends before the closer reaches the child parser.
                                lines.push(String::new());
                                reached.push(true);
                                if building_maps {
                                    line_map.push(None);
                                    col_map.push(None);
                                }
                            }
                            open.content_col = column;
                            nested_fence = Some(open);
                            nested_item_columns.fence_seen = true;
                        }
                    }
                }
            }
        }
        // A closed fence at this item's own column starts after the nested
        // child. Leave it for the parent item to collect as its next block.
        if indent == strip_cols
            && nested_item_columns
                .last()
                .is_some_and(|&(_, child)| child > indent)
            && !follows_plus_marker(&lines)
            && fence.is_none()
            && comment_fence.is_none()
            && colon_open.is_empty()
            && detect_fence_open(trim_ascii_start(line)).is_some_and(|open| {
                list_code_fence_closer_ahead(cur, open, strip_cols, parent_indent)
            })
        {
            break;
        }
        let is_marker = detect_list_marker_full_at(line, indent).is_some();
        if stop_at_content_column_marker
            && is_marker
            && indent >= strip_cols
            && fence.is_none()
            && comment_fence.is_none()
            && colon_open.is_empty()
            && !surviving_definition_body_column.is_some_and(|column| indent >= column)
            && !collected_body_ends_in_a_quote_fold(&lines, &line_map)
        {
            break;
        }
        let was_in_comment_span = comment_fence_strip.is_some();
        if let Some((fence_len, opener_column)) = comment_fence {
            if is_comment_fence_close_any_column(line, fence_len) {
                cur.closed_comment_location = Some((cur.pos, opener_column));
                comment_fence = None;
                closed_comment_span_above |= span_reached_this_frame;
                span_reached_this_frame = false;
            }
        } else if let Some(open) =
            detect_comment_fence_line_any_column(line).filter(|_| fence.is_none())
        {
            let closes = if let Some((_, _, floor)) = literal_colon {
                if cur.literal_comments.is_none() {
                    let source: Vec<_> = cur.lines.range(0..cur.lines.len()).collect();
                    cur.literal_comments = Some(ContainerCommentClosers::build(&source));
                }
                cur.literal_comments.as_mut().unwrap().closes_in_cursor(
                    cur.lines,
                    cur.pos,
                    floor,
                    open.fence_len,
                )
            } else {
                cur.has_comment_closer_after(cur.pos + 1, open.fence_len)
            };
            if closes {
                comment_fence = Some((open.fence_len, indent));
                comment_fence_strip = Some(if indent < strip_cols { 0 } else { strip_cols });
                span_reached_this_frame = indent >= strip_cols;
            } else if indent == strip_cols {
                // Degraded, and written AT this container's own content column.
                // Below it the fence never reached the container and ends
                // nothing there.
                degraded_comment_fence = true;
            } else if indent > strip_cols {
                // Degraded PAST this frame's own content column, so it was
                // written inside some item below this one. Every frame of a
                // marker ladder re-parses a dedented copy of the same chunk
                // against its OWN column, so only the innermost one ever saw
                // the fence - and the frames above it kept collecting, which put
                // the line under it one item too deep (markup-carve/carve-rs#1518).
                //
                // THE COLUMN, NOT A MEMBERSHIP TEST. This asked
                // `descendant_columns.contains(&indent)`, so a fence at a column
                // that is not exactly some listed descendant's - column 5 under
                // `- - - x`, where the listed ones are 2, 4 and 6 - armed nothing
                // and the item ended in the wrong place. The CLOSED spelling of
                // the same fence has used `indent >= strip_cols` since #1540,
                // which is why the two spellings answered one document two ways
                // (markup-carve/carve-rs#1545). `>` rather than `>=` because the
                // equal case is the arm above, which ends nothing.
                degraded_descendant_fence = true;
            }
        }
        let in_comment_span = was_in_comment_span || comment_fence.is_some();
        // A block reached by an ancestor ends the inner fold. A marker
        // between item columns remains lazy text of the inner item.
        let at_ancestor_column = indent == strip_cols
            || nested_item_columns
                .iter()
                .any(|&(_, column)| column == indent)
            || marker_line_columns.contains(&indent);
        if folded_code_span
            && indent < folded_code_column
            && indent >= strip_cols
            && ((is_list_marker(line) && at_ancestor_column)
                || (!is_list_marker(line)
                    && interrupts_lazy_continuation_as_container(cur, trim_ascii_start(line))))
        {
            folded_code_span = false;
            *fence = None;
        }
        let lazy_code_line = folded_code_span && indent < folded_code_column;
        // A retained marker below this body's column cannot open a child list.
        let lazy_list_marker =
            !in_comment_span && fence.is_none() && indent < strip_cols && is_list_marker(line);
        let comment_payload = was_in_comment_span && comment_fence.is_some();
        let payload_line = if comment_payload {
            strip_lazy(line)
        } else {
            line
        };
        let framed_comment_payload = comment_payload
            && !(marker_is_definition_term && marker_definition_has_description)
            && (line.starts_with(LAZY)
                || comment_fence_strip.is_some_and(|strip| strip < strip_cols));
        if marker_is_definition_term
            && marker_term_paragraph_open
            && fence.is_none()
            && colon_open.is_empty()
            && indent >= strip_cols
            && (in_comment_span || is_line_comment_any_column(line))
        {
            marker_term_paragraph_open = false;
        }
        if marker_is_definition_term
            && marker_term_open
            && !was_in_comment_span
            && fence.is_none()
            && colon_open.is_empty()
            && indent <= strip_cols
            && (in_comment_span
                || is_line_comment_any_column(line)
                || detect_comment_fence_line_any_column(line).is_some())
        {
            marker_term_open = false;
        }
        if marker_is_definition_term
            && fence.is_none()
            && !in_comment_span
            && colon_open.is_empty()
            && indent <= strip_cols
        {
            if is_definition_list_start(trim_ascii_start(line)) && !below_definition_term {
                nested_paragraph_column = None;
                marker_definition_has_description = false;
                marker_description_code_admitted = false;
                marker_description_has_inline_code = false;
                marker_description_has_lazy_tail = false;
                marker_description_is_paragraph = false;
                marker_term_paragraph_open = true;
                marker_term_open = true;
            } else if let Some(body) = (!below_definition_separator)
                .then(|| strip_description_prefix(trim_ascii_start(line)))
                .flatten()
            {
                marker_definition_has_description = true;
                marker_description_code_admitted = false;
                marker_description_has_inline_code = trim_ascii_start(line).contains('`');
                marker_description_has_lazy_tail = false;
                marker_description_is_paragraph = ParaOpen::from_line(body, false).get();
                nested_paragraph_column = paragraph_owner_column(trim_ascii_start(line), indent)
                    .filter(|owner| *owner > strip_cols);
                marker_term_paragraph_open = false;
                marker_term_open = false;
            } else if marker_definition_has_description
                && line_starts_paragraph(trim_ascii_start(line))
                && detect_fence_open(trim_ascii_start(line)).is_none()
            {
                marker_description_has_lazy_tail = true;
            } else if marker_term_open
                && detect_fence_open(trim_ascii_start(line)).is_none()
                && ((matches!(
                    trim_ascii_start(line).as_bytes().first(),
                    Some(
                        b'-' | b'*' | b'.' | b'0'
                            ..=b'9'
                                | b'i'
                                | b'I'
                                | b'v'
                                | b'V'
                                | b'x'
                                | b'X'
                                | b'l'
                                | b'L'
                                | b'c'
                                | b'C'
                                | b'd'
                                | b'D'
                                | b'm'
                                | b'M'
                    )
                ) || matches!(trim_ascii_start(line).as_bytes().get(1), Some(b'.' | b')')))
                    && is_list_marker(line)
                    || (matches!(
                        trim_ascii_start(line).as_bytes().first(),
                        Some(b'#' | b'>' | b':' | b'|' | b'%' | b'{' | b'*' | b'-' | b'_' | b'^')
                    ) && interrupts_lazy_continuation_as_container(
                        cur,
                        trim_ascii_start(line),
                    ))
                    || (cur.invisible_arms
                        && indent == strip_cols
                        && trim_ascii_start(line).starts_with('{')
                        && standalone_attrs_block_len_iter(
                            std::iter::once(trim_ascii_start(line)).chain(
                                (cur.pos + 1..cur.lines.len()).map_while(|i| cur.lines.line(i)),
                            ),
                        )
                        .is_some_and(|count| {
                            marker_attribute_end = Some(cur.pos + count - 1);
                            cur.lines.line(cur.pos + count).map_or(true, |next| {
                                is_blank_line(next)
                                    || detect_fence_open(trim_ascii_start(next)).is_some()
                                    || interrupts_lazy_continuation_as_container(
                                        cur,
                                        trim_ascii_start(next),
                                    )
                            })
                        })))
            {
                marker_term_paragraph_open = false;
                marker_term_open = false;
            }
        }
        if marker_is_definition_term
            && marker_definition_has_description
            && !marker_description_has_inline_code
            && detect_fence_open(trim_ascii_start(line)).is_some()
            && lines.last().is_some_and(|line| is_blank_line(line))
        {
            marker_description_code_admitted = lines
                .iter()
                .rev()
                .nth(1)
                .is_some_and(|line| !is_blank_line(line));
        }
        if authored_code_base.is_none()
            && (marker_definition_has_description || !marker_term_open)
            && !marker_description_code_admitted
            && !(marker_is_definition_term
                && marker_description_is_paragraph
                && !marker_description_has_inline_code
                && !marker_description_has_lazy_tail
                && lines
                    .iter()
                    .rev()
                    .take(2)
                    .filter(|line| is_blank_line(line))
                    .count()
                    < 2
                && marker_definition_has_description
                && detect_fence_open(trim_ascii_start(line)).is_some_and(|open| {
                    list_code_fence_closer_ahead(cur, open, indent, parent_indent)
                }))
            && fence.is_none()
            && !in_comment_span
            && colon_open.is_empty()
            && (stop_at_content_column_marker || marker_is_definition_lead)
            && nested_item_columns.is_empty()
            && indent > strip_cols
            && !folded_code_span
            && (marker_is_definition_lead
                || line[..line.len() - trim_ascii_start(line).len()].contains('\t'))
            && detect_fence_open(trim_ascii_start(line)).is_some_and(|open| {
                (marker_definition_has_description
                    || !collected_body_ends_in_a_quote_fold(&lines, &line_map))
                    && (lines.is_empty()
                        || lines.last().is_some_and(|line| is_blank_line(line))
                        || list_code_fence_closer_ahead(cur, open, strip_cols, parent_indent))
            })
        {
            authored_code_base = Some(indent);
        }
        let stripped = match (in_comment_span, comment_fence_strip) {
            _ if authored_code_base.is_some_and(|base| indent >= base) => {
                authored_code_base.unwrap()
            }
            // A line below the opener's base loses its available prefix.
            _ if literal_colon.is_none()
                && fence.is_some_and(|open| {
                    indent < open.content_col
                        && is_fence_close(trim_ascii_start(payload_line), open)
                }) =>
            {
                indent
            }
            _ if lazy_code_line
                || lazy_list_marker
                || definition_comment_band
                || definition_band_inline_closer =>
            {
                indent
            }
            (true, Some(_))
                if comment_payload
                    && marker_is_definition_term
                    && marker_definition_has_description =>
            {
                comment_fence.map_or(0, |(_, opener)| {
                    if opener >= strip_cols && indent >= opener {
                        strip_cols
                    } else {
                        0
                    }
                })
            }
            (true, Some(_)) if comment_payload => strip_cols.min(indent_columns(payload_line)),
            (true, Some(span_strip)) => span_strip.min(indent),
            _ => dedent_for_collection(line, indent, strip_cols),
        };
        if comment_fence.is_none() {
            comment_fence_strip = None;
        }
        let (sliced, consumed, synthetic) = slice_columns_mapped(payload_line, stripped, true);
        // A tab left in a closer's indent after the slice would expand from the
        // body's column 0, not its source column, and land the closer below
        // its opener. Its columns are measured in the source (carve#3141).
        let (sliced, consumed, synthetic) = match *fence {
            Some(open)
                if literal_colon.is_none()
                    && stripped < indent
                    && indent <= open.content_col
                    && sliced[..sliced.len() - trim_ascii_start(&sliced).len()].contains('\t')
                    && is_fence_close(trim_ascii_start(payload_line), open) =>
            {
                let lead = payload_line.len() - trim_ascii_start(payload_line).len();
                let mut columns = " ".repeat(indent - stripped);
                columns.push_str(&payload_line[lead..]);
                (
                    columns,
                    payload_line[..lead].chars().count(),
                    indent - stripped,
                )
            }
            _ => (sliced, consumed, synthetic),
        };
        let mut sliced = sliced;
        let mut trimmed_after_slice = 0usize;
        // PART 9 §10 I2 carries the deepest open paragraph across plain lazy lines.
        let marker_paragraph_owner = (nested_paragraph_column.is_none() && plain_nested_lazy)
            .then(|| {
                lines.last().map_or(carried_lazy_owner, |prior| {
                    if detect_list_marker_full(prior).is_some()
                        && paragraph_owner_column(prior, 0).is_some()
                    {
                        nested_item_columns.last().map(|&(_, column)| column)
                    } else if prior.starts_with(LAZY)
                        || (carried_lazy_owner.is_some()
                            && line_starts_paragraph(trim_ascii_start(prior)))
                    {
                        carried_lazy_owner
                    } else {
                        None
                    }
                })
            })
            .flatten()
            .filter(|&owner| indent < owner);
        if let Some(owner) = marker_paragraph_owner {
            nested_paragraph_column = Some(owner);
        }
        // CARVE-P0-015: a below-child run is a lazy fold even when the outer item reaches its column.
        if nested_folded_fence
            && fence.is_none()
            && nested_item_columns
                .last()
                .is_some_and(|&(_, column)| indent < column && indent >= strip_cols)
            && detect_fence_open(trim_ascii_start(line)).is_some()
        {
            let trimmed = trim_ascii_start(&sliced);
            trimmed_after_slice += sliced.len() - trimmed.len();
            sliced = format!("{LAZY}{trimmed}");
            nested_folded_fence = false;
        }
        // The tracker also records folded marker shapes. A `+` reaches an item
        // only when that item's base reaches its parent's content column.
        if nested_paragraph_column.is_some_and(|owner| indent < owner)
            && !sliced.starts_with(LAZY)
            && !(is_plus_marker(trim_ascii_start(line))
                && nested_item_columns
                    .iter()
                    .enumerate()
                    .any(|(at, &(base, _))| {
                        base == indent && (at == 0 || nested_item_columns[at - 1].1 <= base)
                    })
                && container.owns_continuation(line, cur.continuation_decision(cur.pos)))
            && indent != strip_cols
            && !(is_list_marker(trim_ascii_start(line))
                && open_definition_body_column
                    .is_some_and(|column| indent >= strip_cols && indent < column))
            && !(indent > strip_cols
                && item_block_opener_with_invisible_arms(trim_ascii_start(line), true)
                && detect_fence_open(trim_ascii_start(line)).is_none())
            && !is_blank_line(line)
            && !is_line_comment_any_column(line)
            && detect_comment_fence_line_any_column(line).is_none()
            && exact_colon_fence_len(trim_ascii_start(line)).is_none()
            && !(detect_fence_open(trim_ascii_start(line)).is_some()
                && lines
                    .last()
                    .is_some_and(|line| is_line_comment_any_column(line)))
            && strip_definition_marker(trim_ascii_start(line)).is_none()
            && fence.is_none()
            && comment_fence.is_none()
        {
            let trimmed = trim_ascii_start(&sliced);
            trimmed_after_slice += sliced.len() - trimmed.len();
            sliced = format!("{LAZY}{trimmed}");
        }
        if marker_colon_retained_para && indent < strip_cols && !retained_item_fence {
            let trimmed = trim_ascii_start(&sliced);
            trimmed_after_slice += sliced.len() - trimmed.len();
            sliced = trimmed.to_string();
        }
        if (lazy_code_line
            || framed_comment_payload
            || lazy_list_marker
            || below_definition_separator
                && !separator_at_nested_marker
                && !separator_in_own_item
                && !separator_for_nested_item
            || definition_band_inline_closer
            || (definition_comment_band
                && detect_fence_open(trim_ascii_start(line)).is_none()
                && !is_definition_list_start(trim_ascii_start(line))
                && strip_description_prefix(trim_ascii_start(line)).is_none())
            || (marker_colon_retained_para && indent < strip_cols && !retained_item_fence))
            && !sliced.starts_with(LAZY)
        {
            sliced.insert_str(0, LAZY);
        }
        if separator_in_own_item {
            if let Some(rest) = sliced.strip_prefix(LAZY) {
                sliced = rest.to_string();
            }
            sliced.insert_str(0, ITEM_PARAGRAPH);
        }
        definition_ended_paragraph = is_collected_definition_placeholder(&sliced)
            || is_quoted_definition_placeholder(&sliced);
        // A COLON LINE INSIDE A CODE OR COMMENT SPAN OPENS AND CLOSES NOTHING:
        // §28 makes both bodies verbatim, which is the same reading that keeps a
        // marker in one from being a marker. Read before the code tracker
        // advances, so the CLOSER line counts as inside its own fence.
        //
        // Without it a `:::` written inside a code fence pushed onto the stack
        // and never came off, so the item's next REAL `:::` opener matched that
        // ghost as a closer, the stack emptied one level early, and the marker
        // gate severed the very div this tracker exists to keep whole.
        let opaque_here = fence.is_some() || in_comment_span;
        // A below-column comment retains the paragraph used by I4's
        // interruption test. An unclosed code fence still folds into it.
        let after_below_comment = cur.closed_comment_location.is_some_and(|(at, column)| {
            at + 1 == cur.pos && column < strip_cols && indent != strip_cols
        }) || cur.pos.checked_sub(1).is_some_and(|at| {
            is_line_comment_any_column(cur.lines.at(at))
                && indent_columns(cur.lines.at(at)) < strip_cols
        });
        let at_descendant_column = after_below_comment
            && nested_item_columns
                .iter()
                .map(|&(_, column)| column)
                .chain(marker_line_columns.iter().copied())
                .any(|column| column == indent && column > strip_cols);
        // Match the authored fence column before rebasing the collected body.
        // Markers inside that body must reach the rebase pass as payload.
        let fence_line = if let Some(open) = fence.as_ref() {
            if indent == open.content_col {
                trim_ascii_start(line)
            } else {
                &sliced
            }
        } else if at_descendant_column {
            // Keep a descendant's folded fence visible to this collector so
            // its below-column payload reaches the deferred inner item.
            trim_ascii_start(line)
        } else if stop_at_content_column_marker
            && indent > strip_cols
            && (lines.is_empty() || lines.last().is_some_and(|line| is_blank_line(line)))
        {
            trim_ascii_start(line)
        } else {
            &sliced
        };
        let mut frame_lazy_fence = false;
        if !in_comment_span {
            if fence.is_none()
                && (after_below_comment
                    || (marker_is_definition_term
                        && !marker_term_open
                        && indent == strip_cols
                        && ((collected_body_ends_in_a_quote_fold(&lines, &line_map)
                            && lines.last().is_some_and(|line| {
                                line_starts_paragraph(innermost_marker_content(
                                    without_blockquote_prefixes(trim_ascii_start(line)),
                                ))
                            }))
                            || (!closed_code_leaf
                                && lines.last().is_some_and(|line| {
                                    line_starts_paragraph(innermost_marker_content(
                                        trim_ascii_start(line),
                                    )) || strip_description_prefix(trim_ascii_start(line)).is_some()
                                })))))
            {
                folded_code_column = if at_descendant_column {
                    indent
                } else {
                    strip_cols
                };
                folded_code_span = !retained_item_fence
                    && !(at_descendant_column
                        && marker_line_columns.contains(&indent)
                        && cur.closed_comment_location.is_none())
                    && detect_fence_open(fence_line).is_some_and(|open| {
                        !cur.has_code_closer_after(cur.pos + 1, open.fence_char, open.fence_len)
                            || !item_body_fence_has_closer(
                                cur.lines.range(cur.pos + 1..cur.lines.len()),
                                open,
                                folded_code_column,
                                |line, _| {
                                    detect_list_marker_full(line)
                                        .is_some_and(|marker| marker.indent <= parent_indent)
                                },
                            )
                    });
            }
            if folded_code_span && indent >= folded_code_column {
                let mut paragraph_line = innermost_marker_content(fence_line);
                while let Some(rest) = strip_blockquote_prefix(paragraph_line) {
                    paragraph_line = rest;
                }
                folded_code_paragraph_open = line_starts_paragraph(paragraph_line)
                    || detect_fence_open(paragraph_line).is_some();
            }
            let was_in_code = fence.is_some();
            let may_track_code = literal_colon.map_or(true, |(_, column, floor)| {
                indent >= floor
                    && (fence.is_some()
                        || detect_fence_open(trim_ascii_start(line)).is_some_and(|open| {
                            literal_code_closes_cursor(cur, floor, column, open)
                        }))
            });
            // An unclosed run below a descendant's open paragraph is lazy text
            // there, not a fence of this body (CARVE-P0-014).
            let descendant_shape = fence.is_none()
                && indent == strip_cols
                && nested_item_columns
                    .last()
                    .is_some_and(|&(_, column)| indent < column)
                && cur.pos > 0
                && line_starts_paragraph(innermost_marker_content(trim_ascii_start(
                    cur.lines.at(cur.pos - 1),
                )));
            // The same below a quote a marker line opens (`- - > x` / `   ```), whose paragraph
            // takes the unclosed run as lazy text (carve#3421).
            let marker_quote_shape = fence.is_none()
                && !descendant_shape
                && indent > strip_cols
                && cur.pos > 0
                && nested_item_columns
                    .last()
                    .map(|&(_, column)| column)
                    .or(marker_line_columns.last().copied())
                    .is_some_and(|column| indent < column)
                && {
                    let mut prior =
                        innermost_marker_content(trim_ascii_start(cur.lines.at(cur.pos - 1)));
                    let mut quoted = false;
                    while let Some(rest) = strip_blockquote_prefix(prior) {
                        prior = innermost_marker_content(trim_ascii_start(rest));
                        quoted = true;
                    }
                    quoted && line_starts_paragraph(prior)
                };
            // A closer at the opener's own column inside this item keeps the fence a fence.
            let marker_quote_shape = marker_quote_shape
                && detect_fence_open(trim_ascii_start(line)).is_some_and(|open| {
                    !cur.lines
                        .range(cur.pos + 1..cur.lines.len())
                        .take_while(|next| {
                            is_blank_line(next) || indent_columns(next) >= strip_cols
                        })
                        .any(|next| {
                            indent_columns(next) == indent
                                && is_fence_close(trim_ascii_start(next), open)
                        })
                });
            let lazy_in_descendant = (descendant_shape || marker_quote_shape)
                && detect_fence_open(trim_ascii_start(line)).is_some_and(|open| {
                    !cur.has_code_closer_after(cur.pos + 1, open.fence_char, open.fence_len)
                        || !item_body_fence_has_closer(
                            cur.lines.range(cur.pos + 1..cur.lines.len()),
                            open,
                            strip_cols,
                            |line, _| {
                                detect_list_marker_full(line)
                                    .is_some_and(|marker| marker.indent <= parent_indent)
                            },
                        )
                });
            frame_lazy_fence = lazy_in_descendant && marker_quote_shape
                // A quote marker past the column, below a quote whose list `+` refused it,
                // is no quote of its own: the indented `>` is paragraph text (carve#3421).
                || indent > strip_cols
                    && cur.pos > 0
                    && strip_blockquote_prefix(trim_ascii_start(line)).is_some()
                    && trim_ascii_start(cur.lines.at(cur.pos - 1)).starts_with('>')
                    // Only a `>` out of line with the quote's own marker.
                    && indent_columns(cur.lines.at(cur.pos - 1)) != indent
                    && cur.continuation_decision(cur.pos - 1).is_some_and(|decision| {
                        decision.first_block
                            && decision.refusal.is_some()
                            && decision.owner == Some(continuation_marker::Kind::List)
                    });
            if retained_item_fence {
                let mut open = detect_fence_open(trim_ascii_start(line)).unwrap();
                open.content_col = strip_cols;
                *fence = Some(open);
                marker_colon_retained_para = false;
            } else if may_track_code && !lazy_in_descendant {
                let tracked = if literal_colon.is_some() {
                    trim_ascii_start(fence_line)
                } else {
                    fence_line
                };
                track_collected_fence(fence, tracked, indent >= strip_cols, indent);
            }
            if marker_is_definition_term && !was_in_code && fence.is_some() {
                term_code_lazy_tail = marker_term_paragraph_open
                    && !marker_definition_has_description
                    && indent == strip_cols;
                if term_code_lazy_tail {
                    term_code_start = cur.pos;
                    term_code_closer_checked = false;
                }
                marker_term_paragraph_open = false;
            }
            if was_in_code && fence.is_none() {
                if marker_is_definition_term {
                    term_code_lazy_tail = false;
                }
                closed_code_leaf = true;
            } else if !was_in_code
                && !is_line_comment_any_column(line)
                && !is_collected_definition_placeholder(&sliced)
            {
                closed_code_leaf = indent >= strip_cols
                    && (parse_standalone_attrs(trim_ascii_start(line)).is_some()
                        || parse_footnote_def_line(trim_ascii_start(line)).is_some()
                        || parse_link_def_line(trim_ascii_start(line)).is_some()
                        || marker_attribute_end == Some(cur.pos));
            }
            if fence.is_none() {
                folded_code_span = false;
                authored_code_base = None;
            }
        }
        if !opaque_here {
            if let Some((width, column, floor)) = literal_colon {
                if (indent == floor || indent == column)
                    && exact_colon_fence_len(trim_ascii_start(line)) == Some(width)
                {
                    literal_colon = None;
                }
            } else if let Some(width) = detect_line_block_open(trim_ascii_start(line)) {
                if indent >= strip_cols {
                    let floor = nested_item_columns
                        .last()
                        .map_or(strip_cols, |&(_, col)| col);
                    literal_colon = Some((width, indent, floor));
                }
            }
        }
        if indent == strip_cols
            && !opaque_here
            && colon_open.is_empty()
            && marker_colon.is_some()
            && exact_colon_fence_len(&sliced) == marker_colon
        {
            marker_colon = None;
            marker_colon_closed = true;
            cur.marker_colon_closed = true;
        } else {
            track_collected_colon_fence(
                &mut colon_open,
                &sliced,
                indent >= strip_cols && !opaque_here,
            );
        }
        // Lazy text of a descendant's paragraph, which the reparse must not read as a fence.
        let term_comment_below_column = marker_is_definition_term
            && !marker_definition_has_description
            && indent > 0
            && indent < strip_cols
            && is_line_comment_any_column(line)
            && !is_definition_placeholder(line);
        lines.push(if frame_lazy_fence {
            format!("{LAZY}{}", trim_ascii_start(&sliced))
        } else if term_comment_below_column {
            format!(" {sliced}")
        } else {
            sliced
        });
        table_seen |= is_table_start(trim_ascii_start(line));
        table_caption_open = if caption_content(trim_ascii_start(line)).is_some() {
            table_seen
        } else {
            caption_follower
        };
        carried_lazy_owner = marker_paragraph_owner
            .or_else(|| carried_lazy_owner.filter(|&owner| plain_nested_lazy && indent >= owner));
        if marker_paragraph_owner.is_some() {
            nested_paragraph_column = None;
        }
        if building_maps {
            line_map.push(cur.source_line(cur.pos));
        }
        reached.push(indent >= strip_cols);
        if building_maps {
            col_map.push(cur.source_col(cur.pos).map(|outer| {
                outer + consumed as isize + trimmed_after_slice as isize
                    - synthetic as isize
                    - usize::from(term_comment_below_column) as isize
            }));
        }
        cur.consume();
        // A wrapped attribute opened at the content column is one construct: its
        // later lines belong to it at any column (carve#3421).
        let wrapped_attribute = ((indent == strip_cols
            || nested_item_columns
                .iter()
                .any(|&(_, column)| column == indent))
            && !opaque_here
            && fence.is_none()
            && trim_ascii_start(line).starts_with('{')
            && parse_standalone_attrs(trim_ascii_start(line)).is_none())
        .then(|| {
            standalone_attrs_block_len_iter(
                std::iter::once(trim_ascii_start(line))
                    .chain((cur.pos..cur.lines.len()).map_while(|i| cur.lines.line(i))),
            )
        })
        .flatten();
        if wrapped_attribute.is_some_and(|count| count > 1) {
            closed_code_leaf = true;
        }
        for _ in 1..wrapped_attribute.unwrap_or(1) {
            let next = cur.lines.at(cur.pos);
            // Kept at the opener's column, so a reparse reads them inside its container.
            lines.push(format!(
                "{}{}",
                " ".repeat(indent - strip_cols),
                trim_ascii_start(next)
            ));
            reached.push(true);
            if building_maps {
                line_map.push(cur.source_line(cur.pos));
                col_map.push(
                    cur.source_col(cur.pos).map(|col| {
                        col + leading_ws(next) as isize - (indent - strip_cols) as isize
                    }),
                );
            }
            cur.consume();
        }
    });
    // Preserve the last physical blank through `str::lines()`, including
    // payload owned by an unfinished descendant fence.
    let mut source = lines.join("\n");
    if lines.last().is_some_and(|line| line.is_empty()) {
        source.push('\n');
    }
    #[cfg(test)]
    BODY_LINES_COLLECTED.with(|c| c.set(c.get() + lines.len() as u64));
    debug_assert_eq!(reached.len(), lines.len());
    MappedSource {
        col_map,
        source,
        line_map,
        authored_base_at_start,
        item_marker_at_start: false,
        marker_colon_rebased: false,
        marker_tail: None,
        reached,
        opaque_blank_lines,
        discarded_outer_blank,
        outer_plus_after_blank,
        in_quoted_body: false,
        sublists_carry_authored_base: false,
    }
}

/// Follow the fenced code block a collected line opens or closes, so the
/// below-column guard above knows whether one is open.
///
/// Only a line AT OR PAST the content column can do either: below it the line
/// never reached the container, so a fence-shaped one is paragraph text and
/// opens nothing (§24 C3). The test runs on the DEDENTED line for the same
/// reason `detect_fence_open` refuses an indented one - a residual column is
/// still a column.
/// Advance a collector's OPEN COLON CONTAINER stack over one collected line.
///
/// The code counterpart is `track_collected_fence`, and this is deliberately
/// written beside it: one construct answered in two places is what let a `:::`
/// body sever on a marker where a code body did not.
///
/// EXACT WIDTHS, so the widths in flight are a stack (markup-carve/carve#455):
/// a bare run of the innermost width closes, any other opener nests. Only a
/// line that reaches the content column can open or close one - below it the
/// line is text (§24 C3), which is the same condition the code tracker takes.
pub(crate) fn track_collected_colon_fence(
    open: &mut Vec<usize>,
    line: &str,
    at_content_column: bool,
) {
    if !at_content_column {
        return;
    }
    if let Some(len) = exact_colon_fence_len(line) {
        if open.last() == Some(&len) {
            open.pop();
            return;
        }
    }
    if let Some(len) = detect_line_block_open(line) {
        open.push(len);
        return;
    }
    if let Some(len) = detect_hardbreaks_block_open(line) {
        open.push(len);
        return;
    }
    if let Some(len) = detect_quote_block_open(line) {
        open.push(len);
        return;
    }
    if let Some(container) = detect_container_open(line) {
        open.push(container.fence_len);
    }
}

/// §10 I4 for a fence that follows an item's open paragraph: only a closer
/// written in the item's own body counts. The search stops where the item
/// ends: at a sibling or outer marker, and at a blank followed by a line below
/// the content column (carve#1379). A below-column line folds as lazy text, so
/// the search runs past it, and a closer is read at the content column exactly.
pub(crate) fn item_body_fence_has_closer<'a>(
    lines: impl Iterator<Item = &'a str>,
    open: FenceOpen,
    content_col: usize,
    ends_item: impl Fn(&str, usize) -> bool,
) -> bool {
    let mut pending_blank = false;
    for line in lines {
        if is_blank_line(line) {
            pending_blank = true;
            continue;
        }
        let indent = indent_columns(line);
        if ends_item(line, indent) {
            return false;
        }
        if pending_blank && indent < content_col {
            return false;
        }
        pending_blank = false;
        if indent >= content_col && is_fence_close(&slice_columns(line, content_col, false), open) {
            return true;
        }
    }
    false
}

pub(crate) fn track_collected_fence(
    fence: &mut Option<FenceOpen>,
    dedented: &str,
    at_content_column: bool,
    opener_column: usize,
) {
    if !at_content_column {
        return;
    }
    match *fence {
        Some(open) => {
            if is_fence_close(dedented, open) {
                *fence = None;
            }
        }
        None => {
            if let Some(mut open) = detect_fence_open(dedented) {
                open.content_col = opener_column;
                *fence = Some(open);
            }
        }
    }
}

/// Does this collected chunk leave a comment fence that never closes?
///
/// PART 9 SS28 degrades such a fence to an ordinary line comment, and a line
/// comment leaves no paragraph open - so a line written BELOW the container's
/// content column continues nothing and the container ends there
/// (markup-carve/carve-rs#1512).
///
/// The collector's own `break` cannot say this: it ends the CHUNK, and the
/// outer loop collects the next one and takes the line back. `reached` is what
/// keeps the column question answerable after the dedent - a fence below the
/// content column never reached the container and ends nothing there - and the
/// closer index is asked from the cursor's position, so "unterminated" means
/// the same thing here as it does inside the collector.
pub(crate) fn chunk_ends_in_degraded_comment_fence(
    cur: &mut LineCursor<'_>,
    chunk: &MappedSource,
) -> bool {
    let mut open: Option<usize> = None;
    // NO DESCENDANT-COLUMN WALK HERE. There used to be one, on the reading that
    // a fence at a SUB-item's column ends that item and not this one - the rule
    // markup-carve/carve-rs#1518 is about. It could not reach a document from
    // here: this answer only matters through a break that needs the collector to
    // have STOPPED on a below-column line, and the degraded stop that leaves one
    // fires at `indent == strip_cols`, where the fence is at chunk column 0 and
    // no descendant column can be at or below it. The rule now lives in
    // `collect_indented_block_*`, where the collector can act on it.
    for (index, line) in chunk.source.lines().enumerate() {
        if let Some(fence_len) = open {
            if is_comment_fence_close_any_column(line, fence_len) {
                open = None;
            }
            continue;
        }
        if !chunk.reached.get(index).copied().unwrap_or(false) {
            continue;
        }
        if let Some(fence) = detect_comment_fence_line_any_column(line) {
            open = Some(fence.fence_len);
        }
    }
    open.is_some_and(|fence_len| !cur.has_comment_closer_after(cur.pos, fence_len))
}

/// Does the chunk END with a comment span that OPENED at a line reaching this
/// container and CLOSED inside it?
///
/// A closed span renders nothing, exactly as a degraded fence does, so it
/// leaves no paragraph for a below-column line to continue and the frames under
/// it end the same way (markup-carve/carve-rs#1531). The degraded test above
/// cannot answer this: it reports the span still OPEN at the chunk's end.
pub(crate) fn chunk_ends_in_closed_comment_span(chunk: &MappedSource) -> bool {
    let mut open: Option<usize> = None;
    let mut closed_at_end = false;
    for (index, line) in chunk.source.lines().enumerate() {
        if is_blank_line(line) {
            continue;
        }
        if let Some(fence_len) = open {
            if is_comment_fence_close_any_column(line, fence_len) {
                open = None;
                closed_at_end = true;
            }
            continue;
        }
        closed_at_end = false;
        if !chunk.reached.get(index).copied().unwrap_or(false) {
            continue;
        }
        if let Some(fence) = detect_comment_fence_line_any_column(line) {
            open = Some(fence.fence_len);
        }
    }
    // No `&& open.is_none()` here: a second opener resets `closed_at_end` on
    // its own line, so the two can never disagree - and a span still open at
    // the chunk's end is what the degraded test above already reports.
    closed_at_end
}
