use super::*;

/// Last line in a colon group rebased with its opener.
pub(in crate::parse) fn rebased_colon_group_end(
    lines: &[Cow<'_, str>],
    start: usize,
    base: usize,
    width: usize,
) -> usize {
    let mut end = start;
    // CARVE-P0-004: the closer is measured from the
    // authored base, so only the container's content column or that base
    // closes. A run in the band between them is payload, and the extent
    // must keep it rather than break and let the outer scan re-read it as
    // an opener of its own. The comment fence is the deliberate exception
    // and is left alone - both its delimiters carry a whitespace slot.
    let mut stack = vec![width];
    for (j, candidate) in lines.iter().enumerate().skip(start + 1) {
        end = j;
        let candidate = strip_lazy(candidate);
        let column = indent_columns(candidate);
        if !is_blank_line(candidate) && (column == 0 || column == base) {
            let local = strip_leading_columns(candidate, column);
            let trimmed = trim_ascii(&local);
            if trimmed.len() >= 3 && trimmed.bytes().all(|byte| byte == b':') {
                if stack.last() == Some(&trimmed.len()) {
                    stack.pop();
                    if stack.is_empty() {
                        break;
                    }
                } else {
                    stack.push(trimmed.len());
                }
            } else if let Some(open) = detect_container_open(trimmed) {
                if stack.len() < MAX_NESTING_DEPTH {
                    stack.push(open.fence_len);
                }
            }
        }
    }
    end
}

/// Last line in a list run before a dedent or a below-content-column block.
pub(in crate::parse) fn rebased_list_end(
    lines: &[Cow<'_, str>],
    start: usize,
    base: usize,
) -> usize {
    let mut end = start;
    let content = marker_content_col(&lines[start]).unwrap_or(base);
    let mut blank_seen = false;
    for (j, candidate) in lines.iter().enumerate().skip(start + 1) {
        if is_blank_line(candidate) {
            blank_seen = true;
            end = j;
            continue;
        }
        let indent = indent_columns(candidate);
        if indent < base {
            break;
        }
        // THE SECOND BOUND. Carrying a block short of the item's
        // content column into the run puts it in the LIST's coordinate
        // system one or two columns shy of that content column, where
        // the ordinary list rule reads it as lazy text - which is
        // exactly what the enclosing body's own block is not.
        //
        // `indent > base` rather than `>= base` is equivalent for known shapes:
        // widening it moves nothing, because the outer scan
        // picks a marker at the base up again as its own opener. It is
        // written this way because a line AT the marker's column is
        // where a sibling marker goes, which belongs in the run.
        if blank_seen && indent > base && indent < content {
            break;
        }
        end = j;
    }
    end
}

/// For each line, whether a fence opened above it is still open there, so the
/// line is that fence's body or closer. A nested line block's payload opens
/// nothing, so a fence-shaped line in it is skipped.
pub(in crate::parse) fn fences_open_before(lines: &[Cow<'_, str>]) -> Vec<bool> {
    let mut open: Option<(FenceOpen, usize)> = None;
    let mut line_block: Option<(usize, usize)> = None;
    let mut out = Vec::with_capacity(lines.len());
    for line in lines {
        let column = indent_columns(line);
        let local = trim_ascii_start(line);
        if let Some((width, at)) = line_block {
            if is_blank_line(line) || column >= at {
                out.push(false);
                if column == at && exact_colon_fence_len(local) == Some(width) {
                    line_block = None;
                }
                continue;
            }
            line_block = None;
        }
        match open {
            Some((fence, base)) => {
                out.push(true);
                if column == base && is_fence_close(local, fence) {
                    open = None;
                }
            }
            None => {
                out.push(false);
                line_block = detect_line_block_open(local)
                    .map(|width| (width, column))
                    .or_else(|| {
                        let marker = detect_list_marker_full_at(line, column)?;
                        let width = detect_line_block_open(marker.content)?;
                        Some((width, marker_content_col_at(line, column)?))
                    });
                if line_block.is_none() {
                    open = detect_fence_open(local).map(|fence| (fence, column));
                }
            }
        }
    }
    out
}

/// Last line owned by a rebased code fence.
pub(in crate::parse) fn rebased_code_fence_end(
    lines: &[Cow<'_, str>],
    start: usize,
    base: usize,
    open: FenceOpen,
) -> usize {
    let mut end = start;
    // CARVE-P0-004: only the container's content column and the opener's
    // own base close it, so an unterminated fence runs to the container's
    // end and a run in between stays payload at its authored column.
    for (j, candidate) in lines.iter().enumerate().skip(start + 1) {
        end = j;
        let column = indent_columns(candidate);
        if !is_blank_line(candidate)
            && (column == 0 || column == base)
            && is_fence_close(&strip_leading_columns(candidate, column), open)
        {
            break;
        }
    }
    end
}

/// Last line owned by a rebased comment fence.
pub(in crate::parse) fn rebased_comment_end(
    lines: &[Cow<'_, str>],
    start: usize,
    base: usize,
    width: usize,
) -> usize {
    let mut end = start;
    for (j, candidate) in lines.iter().enumerate().skip(start + 1) {
        if !is_blank_line(candidate) && indent_columns(candidate) < base {
            break;
        }
        end = j;
        if !is_blank_line(candidate)
            && is_comment_fence_close(&strip_leading_columns(candidate, base), width)
        {
            break;
        }
    }
    end
}

/// Last line owned by a rebased line block.
pub(in crate::parse) fn rebased_line_block_end(
    lines: &[Cow<'_, str>],
    start: usize,
    base: usize,
    width: usize,
    closers: &mut Option<HashMap<u8, Vec<usize>>>,
) -> usize {
    let mut end = start;
    let mut j = start + 1;
    while j < lines.len() {
        let candidate = &lines[j];
        end = j;
        if let Some(open) = detect_fence_open(trim_ascii_start(candidate)) {
            let index = closers.get_or_insert_with(|| {
                let views: Vec<_> = lines.iter().map(|line| line.as_ref()).collect();
                build_code_closer_last_index(&views)
            });
            if code_closer_exists_after(index, j, open.fence_char, open.fence_len) {
                if let Some(close) = (j + 1..lines.len())
                    .find(|&at| is_fence_close(trim_ascii_start(&lines[at]), open))
                {
                    end = close;
                    j = close + 1;
                    continue;
                }
            }
        }
        let column = indent_columns(candidate);
        if (column == 0 || column == base)
            && exact_colon_fence_len(&strip_leading_columns(candidate, column)) == Some(width)
        {
            break;
        }
        j += 1;
    }
    end
}

/// Last line owned by a rebased definition list.
pub(in crate::parse) fn rebased_definition_list_end(
    lines: &[Cow<'_, str>],
    start: usize,
    base: usize,
) -> usize {
    let mut end = start;
    let mut content = definition_body_content_col(&lines[start]);
    for (j, candidate) in lines.iter().enumerate().skip(start + 1) {
        if is_blank_line(candidate) {
            end = j;
            continue;
        }
        let indent = indent_columns(candidate);
        if indent < base {
            break;
        }
        if indent == base {
            // A sibling term or description marker re-opens the run's
            // innermost body, so the column to measure against moves
            // with it.
            if let Some(column) = definition_body_content_col(candidate) {
                content = Some(column);
            }
            end = j;
            continue;
        }
        if content.is_some_and(|column| indent < column)
            && item_block_opener(&strip_leading_columns(candidate, indent))
        {
            break;
        }
        end = j;
    }
    end
}

/// Last line in the footnote metadata group, retaining intervening blanks.
#[inline(always)]
pub(in crate::parse) fn rebased_footnote_end(
    lines: &[Cow<'_, str>],
    start: usize,
    base: usize,
) -> usize {
    let mut end = start;
    // The authored base belongs to the remainder of this metadata
    // group, not only to the definition opener. A later paragraph at
    // the same authored column is a sibling in the enclosing footnote
    // body; leaving its residual indent made it a continuation of the
    // nested definition after the opener itself had been rebased.
    for (j, candidate) in lines.iter().enumerate().skip(start + 1) {
        if !is_blank_line(candidate) && indent_columns(candidate) < base {
            break;
        }
        end = j;
    }
    end
}

/// Last quote prefix or lazy continuation before a blank or dedent.
#[inline(always)]
pub(in crate::parse) fn rebased_quote_end(
    lines: &[Cow<'_, str>],
    start: usize,
    base: usize,
) -> usize {
    let mut end = start;
    // Quote prefixes and their lazy paragraph continuations share the
    // opener's authored base until a blank or a dedent ends the run.
    for (j, candidate) in lines.iter().enumerate().skip(start + 1) {
        if is_blank_line(candidate) || indent_columns(candidate) < base {
            break;
        }
        end = j;
    }
    end
}

/// Last table row or continuation at the opener's authored base.
#[inline(always)]
pub(in crate::parse) fn rebased_table_end(
    lines: &[Cow<'_, str>],
    start: usize,
    base: usize,
) -> usize {
    let mut end = start;
    for (j, candidate) in lines.iter().enumerate().skip(start + 1) {
        if is_blank_line(candidate) || indent_columns(candidate) != base {
            break;
        }
        let local = strip_leading_columns(candidate, base);
        if !is_table_start(&local) && !is_table_continuation(&local) {
            break;
        }
        end = j;
    }
    end
}

#[cfg(test)]
thread_local! {
    pub(in crate::parse) static NAMED_COLON_REBASE_LINES: Cell<usize> = const { Cell::new(0) };
}

pub(in crate::parse) fn rebase_named_colon_extent(
    lines: &mut [Cow<'_, str>],
    columns: &mut [Option<isize>],
    start: usize,
    end: usize,
    base: usize,
    width: usize,
) {
    // Each payload line is rewritten once, even when named children nest.
    let mut frames = vec![(width, base, 0, false, false)];
    let mut items: Vec<usize> = Vec::new();
    let mut comment_retains_frame = false;
    let mut code: Option<(FenceOpen, usize, bool)> = None;
    let mut comment: Option<usize> = None;
    for (at, line) in lines.iter_mut().enumerate().take(end + 1).skip(start) {
        let column = indent_columns(line);
        let local = trim_ascii_start(line);
        let Some(&(frame_width, frame_base, parent_base, literal, paragraph)) = frames.last()
        else {
            break;
        };
        let mut strip = frame_base;
        let mut opaque = false;
        if let Some((open, opener_column, payload)) = code {
            if !is_blank_line(line) && column < opener_column && !payload {
                code = None;
            } else {
                if (payload || column == opener_column) && is_fence_close(local, open) {
                    code = None;
                }
                if !payload {
                    strip = opener_column;
                }
                opaque = true;
            }
        }
        if let Some(width) = comment {
            if is_comment_fence_close_any_column(line, width) {
                comment = None;
            }
            opaque = true;
        }
        if at > start && !opaque {
            if is_blank_line(line) {
                comment_retains_frame = false;
                if let Some(frame) = frames.last_mut() {
                    frame.4 = false;
                }
            } else {
                let comment_is_body_owned = items.is_empty();
                let comment_line = is_line_comment_any_column(line);
                if !comment_line && column <= frame_base {
                    comment_retains_frame = false;
                }
                if comment_line {
                    comment_retains_frame = items.last().is_some_and(|content| column < *content);
                }
                if comment_retains_frame
                    && !comment_line
                    && column >= frame_base
                    && item_block_opener(local)
                {
                    comment_retains_frame = false;
                }
                while !comment_retains_frame
                    && items.last().is_some_and(|content| column < *content)
                {
                    items.pop();
                }
                let in_item = items.last().is_some_and(|content| column >= *content);
                let held_by_item = in_item || comment_retains_frame;
                if let Some(open) = detect_comment_fence_line_any_column(line) {
                    if comment_is_body_owned {
                        comment_retains_frame = false;
                    }
                    comment = Some(open.fence_len);
                } else if let Some(open) =
                    detect_fence_open(local).filter(|_| literal || column >= frame_base || in_item)
                {
                    let inline_code = !literal && !in_item && column > frame_base;
                    code = Some((open, column, literal || inline_code));
                    if let Some(frame) = frames.last_mut() {
                        frame.4 = inline_code;
                    }
                } else if !held_by_item && !literal {
                    if exact_colon_fence_len(local) == Some(frame_width)
                        && (column == frame_base || column == parent_base)
                    {
                        strip = column;
                        frames.pop();
                    } else if column >= frame_base {
                        if let Some(next_width) = detect_container_open(local)
                            .map(|open| open.fence_len)
                            .or_else(|| detect_line_block_open(local))
                            .or_else(|| detect_hardbreaks_block_open(local))
                            .or_else(|| detect_quote_block_open(local))
                        {
                            if frames.len() < MAX_NESTING_DEPTH {
                                let next_literal = detect_line_block_open(local).is_some()
                                    || detect_quote_block_open(local).is_some();
                                frames.push((next_width, column, frame_base, next_literal, false));
                                strip = column;
                            }
                        } else if let Some(frame) = frames.last_mut() {
                            frame.4 =
                                detect_fence_open(local).is_some() || !item_block_opener(local);
                        }
                    }
                } else if literal
                    && exact_colon_fence_len(local) == Some(frame_width)
                    && column == frame_base
                {
                    frames.pop();
                }
                if !literal && (!paragraph || in_item) {
                    if let Some(content) = marker_content_col_at(line, column) {
                        items.push(content);
                    }
                }
            }
        }
        if !is_blank_line(line) && column >= strip {
            #[cfg(test)]
            NAMED_COLON_REBASE_LINES.with(|count| count.set(count.get() + 1));
            let (residue, consumed, synthetic) = slice_columns_mapped(line, strip, true);
            *line = Cow::Owned(residue);
            if let Some(Some(column)) = columns.get_mut(at) {
                *column += consumed as isize - synthetic as isize;
            }
        }
    }
}

/// Apply an authored block base after a container's minimum content column has
/// been stripped. List-item calls leave sublists alone because their residual
/// indentation expresses another list level; definition and footnote bodies
/// include them under carve#1729's shared rule.
pub(in crate::parse) fn rebase_overindented_blocks(
    source: &mut MappedSource,
    include_sublists: bool,
    options: &Options<'_>,
) {
    rebase_body_blocks(source, include_sublists, options, false);
}

pub(in crate::parse) fn rebase_body_blocks(
    source: &mut MappedSource,
    include_sublists: bool,
    options: &Options<'_>,
    absorbed_note_marker: bool,
) {
    let trailing_newline = source.source.ends_with('\n');
    let mut lines: Vec<Cow<'_, str>> = source.source.lines().map(Cow::Borrowed).collect();
    if trailing_newline {
        lines.push(Cow::Borrowed(""));
    }
    // No authored base can move when every line already starts at column zero.
    // This also keeps a quote replay's one-line probes from replaying again.
    if !lines.iter().any(|line| line.starts_with([' ', '\t'])) {
        return;
    }
    // Dedenting keeps every line's `%` run, so this index outlives the edits.
    let entry_closers = std::cell::OnceCell::new();
    // Each edit ends before the next iteration, so future columns stay indexed.
    let code_columns = std::cell::OnceCell::new();
    let quote_lines = std::cell::OnceCell::new();
    // Read on the authored lines: ownership is settled before any rebase here.
    let authored: Vec<Cow<'_, str>> = lines.clone();
    let fences_before = std::cell::OnceCell::new();
    let mut i = 0usize;
    let mut nested_columns: Vec<usize> = Vec::new();
    let mut nested_list_marker_bases: Vec<Option<usize>> = Vec::new();
    let mut descendant_code: Option<(FenceOpen, usize)> = None;
    let mut orphaned_code: Option<(FenceOpen, usize)> = None;
    let mut after_blank = false;
    let mut paragraph_open = false;
    // The open paragraph is the container's own, not a descendant's.
    let mut own_paragraph = false;
    let mut item_separator_paragraph = false;
    let mut opaque_quote_tail = false;
    // A code-shaped run outside the preceding verse's bases is inline text.
    // Keep its authored column through repeated rebase passes.
    let mut last_literal_base = None;
    let mut term_boundary = false;
    let mut description_tail = false;
    let mut payload_closers = None;
    // Has a line AT the container's minimum column been read since the scan
    // last stood inside a descendant? It is ownership evidence exactly as an
    // open paragraph is: the scan is back in the parent's coordinate system,
    // so the next over-indented opener was authored there rather than left
    // behind by a child whose own column it fell short of. See the guard
    // below for what only counting the paragraph cost (carve-rs#1415).
    let mut block_at_minimum = false;
    let mut paragraph_after_invisible_base = None;
    let mut invisible_end = None;
    while i < lines.len() {
        if is_blank_line(&lines[i]) {
            paragraph_after_invisible_base = None;
            last_literal_base = None;
            opaque_quote_tail = false;
            after_blank = true;
            paragraph_open = false;
            item_separator_paragraph = false;
            i += 1;
            continue;
        }
        if !paragraph_open {
            item_separator_paragraph = false;
        }
        let base = indent_columns(&lines[i]);
        let local_at_base = strip_leading_columns(&lines[i], base);
        if include_sublists && paragraph_open {
            if let Some(open) = detect_fence_open(&local_at_base) {
                let columns = code_columns.get_or_init(|| BodyCodeColumns::new(&lines));
                if !columns.closes_after(i, 0, open) && !columns.closes_after(i, base, open) {
                    // An unmatched run is inline text and establishes no block base.
                    after_blank = false;
                    i += 1;
                    continue;
                }
            }
        }
        let mut closed_descendant_code = false;
        if let Some((open, column)) = descendant_code {
            if base < column {
                orphaned_code = descendant_code.take();
            } else if base == column && is_fence_close(&local_at_base, open) {
                descendant_code = None;
                closed_descendant_code = true;
            }
        }
        if base == 0 && heading_content_starts(&local_at_base) {
            orphaned_code = None;
        }
        if orphaned_code
            .is_some_and(|(open, column)| base == column && is_fence_close(&local_at_base, open))
        {
            orphaned_code = None;
            paragraph_open = true;
            own_paragraph = false;
            after_blank = false;
            i += 1;
            continue;
        }
        if !closed_descendant_code
            && descendant_code.is_none()
            && nested_columns.last().is_some_and(|column| base >= *column)
        {
            if let Some(open) = detect_fence_open(&local_at_base) {
                descendant_code = Some((open, base));
            }
        }
        if base > 0
            && last_literal_base.is_some_and(|column| base != column)
            && detect_fence_open(&local_at_base).is_some()
        {
            let open = detect_fence_open(&local_at_base).unwrap();
            i = rebased_code_fence_end(&lines, i, base, open) + 1;
            paragraph_open = true;
            own_paragraph = false;
            continue;
        }
        if base == 0 {
            last_literal_base = None;
        }
        if i == 0 && base > 0 && !source.authored_base_at_start {
            // A residual first-line marker still opens a descendant. Register
            // its content column before preserving the residual line so a
            // post-blank block at or past that column remains the child's.
            if let Some(column) = marker_content_col(&lines[i]) {
                nested_columns.push(column);
                nested_list_marker_bases.push(
                    detect_list_marker_full(&lines[i])
                        .filter(|_| base > 0)
                        .map(|_| base),
                );
            }
            paragraph_open = !is_line_comment_any_column(&lines[i]);
            own_paragraph = false;
            block_at_minimum = !paragraph_open && source.reached.get(i).copied().unwrap_or(false);
            i += 1;
            continue;
        }
        if !include_sublists && opaque_quote_tail && base > 0 {
            i += 1;
            continue;
        }
        // A closed fence short of the child column belongs to the outer item.
        // Only an unclosed run remains lazy text of the child's paragraph.
        let closed_fence_below_descendant = nested_columns.last().is_some_and(|col| base < *col)
            && nested_list_marker_bases.last() == Some(&Some(base))
            && detect_fence_open(&local_at_base).is_some_and(|open| {
                code_columns
                    .get_or_init(|| BodyCodeColumns::new(&lines))
                    .closes_after(i, base, open)
            });
        if !include_sublists
            && paragraph_open
            && base > 0
            && item_block_opener(&local_at_base)
            && !(paragraph_after_invisible_base.is_some_and(|column| base >= column)
                && is_table_start(&local_at_base))
            && !(source.reached.get(i).copied().unwrap_or(false)
                && strip_blockquote_prefix(&local_at_base).is_some())
            && !closed_fence_below_descendant
            && !(item_separator_paragraph && source.reached.get(i).copied().unwrap_or(false))
        {
            i += 1;
            continue;
        }
        // Did the line REACH the container's own content column? A line one
        // column past it and a line below it both arrive with a single
        // residual column and only `reached` separates them - see
        // `MappedSource::reached`. Unknown reads as "below", which is what
        // this scan assumed before markup-carve/carve#1896.
        let reaches_container = source.reached.get(i).copied().unwrap_or(false);
        // Reaching the outer container does not move a line out of a deeper
        // item's still-open paragraph. It remains below that item's content
        // column, and the next line at the column still belongs to that item.
        if paragraph_open
            && !after_blank
            && nested_columns.last().is_some_and(|column| base < *column)
            && !item_block_opener(&local_at_base)
        {
            i += 1;
            continue;
        }
        // §10 I5's invisible kinds are lazy paragraph text of THIS container
        // only BELOW its content column (markup-carve/carve#1809, corpus
        // 430-2/430-3). At or past the column the same line is the container's
        // own block content and rebases like any other opener
        // (markup-carve/carve#1911, corpus 444-7) - the residual the slice
        // leaves in front of it is what made the two look alike.
        if paragraph_open
            && base > 0
            && !reaches_container
            && item_block_opener(&local_at_base)
            && !item_block_opener_with_invisible_arms(&local_at_base, false)
        {
            i += 1;
            continue;
        }
        // A residual indent below EVERY live column is nobody's authored base:
        // it reaches no descendant, and not the container either, so it stays
        // the lazy line of whatever the line above left open.
        if !reaches_container
            && !after_blank
            && nested_columns.last().is_some_and(|column| base < *column)
        {
            let local = strip_leading_columns(&lines[i], base);
            if base > 0 || !item_block_opener(&local) {
                block_at_minimum = false;
                i += 1;
                continue;
            }
        }
        // It reached the container and fell short of every descendant, so no
        // descendant owns it (markup-carve/carve#1896): the deeper one opened
        // on a line above and never on this one. The scan is back in the
        // container's coordinate system exactly as a line at its minimum column
        // puts it there, and the opener written here is the container's own.
        if reaches_container && nested_columns.last().is_some_and(|column| base < *column) {
            block_at_minimum = true;
        }
        // A line comment below a descendant's column leaves that item available.
        if is_line_comment_any_column(&lines[i])
            && nested_columns.last().is_some_and(|column| base < *column)
        {
            paragraph_open = false;
            i += 1;
            continue;
        }
        if nested_columns.last().is_some_and(|column| base < *column) {
            if let Some(open) = detect_comment_fence_line(&local_at_base) {
                let close = entry_closers
                    .get_or_init(|| comment_closers_by_run(&lines))
                    .get(&open.fence_len)
                    .and_then(|at| at.get(at.partition_point(|&at| at <= i)).copied());
                if let Some(close) = close {
                    let between_items = nested_list_marker_bases
                        .last()
                        .and_then(|marker_base| *marker_base)
                        .is_some_and(|outer_col| base >= outer_col);
                    if include_sublists && between_items {
                        nested_columns.pop();
                        nested_list_marker_bases.pop();
                        paragraph_open = false;
                        i = close + 1;
                        continue;
                    }
                    if include_sublists || indent_columns(&lines[i]) < base || close == i + 1 {
                        paragraph_open = false;
                        i = close + 1;
                        continue;
                    }
                }
            }
        }
        let refused_lazy_plus = is_plus_marker(trim_ascii_start(&lines[i]))
            && source
                .line_map
                .get(i)
                .copied()
                .flatten()
                .and_then(continuation_marker::decision)
                .is_some_and(|decision| decision.refusal.is_some())
            && nested_columns.last().is_some_and(|column| base < *column)
            && (paragraph_open
                || i > 0
                    && detect_list_marker_full(&lines[i - 1])
                        .is_some_and(|marker| line_starts_paragraph(marker.content)));
        if refused_lazy_plus {
            paragraph_open = true;
            own_paragraph = false;
            after_blank = false;
            i += 1;
            continue;
        }
        while nested_columns.last().is_some_and(|column| base < *column) {
            nested_columns.pop();
            nested_list_marker_bases.pop();
        }
        // §10 I2: a marker under the container's own paragraph folds into it
        // and opens no item, so it sets no column for the lines below.
        if include_sublists
            && own_paragraph
            && paragraph_open
            && !after_blank
            && is_list_marker(&local_at_base)
        {
            i += 1;
            continue;
        }
        if let Some(column) = marker_content_col_at(&lines[i], base)
            .filter(|_| !(absorbed_note_marker && paragraph_open && !after_blank))
        {
            if !include_sublists || base == 0 || !nested_columns.is_empty() {
                let marker = detect_list_marker_full_at(&lines[i], base);
                let colon_folds = marker.as_ref().is_some_and(|marker| {
                    detect_container_open(marker.content).is_some()
                        || detect_line_block_open(marker.content).is_some()
                        || detect_hardbreaks_block_open(marker.content).is_some()
                        || detect_quote_block_open(marker.content).is_some()
                }) && lines
                    .get(i + 1)
                    .is_some_and(|next| !is_blank_line(next) && indent_columns(next) < column);
                own_paragraph = false;
                paragraph_open = colon_folds
                    || (include_sublists
                        && marker
                            .as_ref()
                            .is_some_and(|marker| line_starts_paragraph(marker.content)));
                nested_columns.push(column);
                nested_list_marker_bases.push(marker.as_ref().filter(|_| base > 0).map(|_| base));
                let mut inner_base = column;
                let mut inner_content = marker.as_ref().map(|marker| marker.content);
                while let Some(content) =
                    inner_content.filter(|content| indent_columns(content) == 0)
                {
                    let Some(inner_marker) = detect_list_marker_full_at(content, 0) else {
                        break;
                    };
                    let Some(width) = marker_content_col_at(content, 0) else {
                        break;
                    };
                    let inner_column = inner_base + width;
                    nested_columns.push(inner_column);
                    nested_list_marker_bases.push(Some(inner_base));
                    inner_base = inner_column;
                    inner_content = Some(inner_marker.content);
                }
                after_blank = false;
                block_at_minimum = false;
                i += 1;
                continue;
            }
        }
        let bare_description = strip_definition_marker(trim_ascii_start(&lines[i])).is_some();
        if !(absorbed_note_marker && bare_description && !term_boundary) {
            if let Some(column) = definition_body_content_col(&lines[i]) {
                nested_columns.push(column);
                nested_list_marker_bases.push(None);
            }
        }
        // AN OVER-INDENTED NESTED NOTE DEFINITION KEEPS ITS AUTHORED COLUMN.
        // A note's body column is measured from its OWN marker (§16, marker plus
        // two - carve-js#1664), so the recursive note pass needs the residual
        // indent that says how deep this marker sits. Flattening it to the
        // container's own column zero collapsed a two-level nesting into one, so
        // a trailing line below the inner note's own content column was claimed
        // by it instead of falling to the reachable ancestor note
        // (markup-carve/carve#1946, carve-php#1895). The floor is NOT registered
        // as a descendant column here: a note's opaque payload sits BELOW that
        // floor, and a live column above it would make the rebasing branch read
        // an indented fence there as a lazy line and strand its payload. A note
        // at the container's own column zero is left to the base-zero handling
        // below.
        if include_sublists && base > 0 && parse_footnote_def_line(&local_at_base).is_some() {
            after_blank = false;
            block_at_minimum = false;
            i += 1;
            continue;
        }
        if nested_columns.last().is_some_and(|column| base >= *column) {
            // A descendant's line can leave a paragraph or a term open for a
            // band marker below it, which S4 retains as its text.
            if include_sublists
                && (line_starts_paragraph(&local_at_base)
                    || is_definition_list_start(&local_at_base))
            {
                paragraph_open = true;
                own_paragraph = false;
            }
            block_at_minimum = false;
            i += 1;
            continue;
        }
        if base == 0 {
            opaque_quote_tail = false;
            if strip_blockquote_prefix(&lines[i]).is_some() {
                // Unmarked continuations belong to the quote before their
                // residual indentation can become another block's base.
                let marker_fence_quote = i == 0
                    && source.item_marker_at_start
                    && !source.authored_base_at_start
                    && detect_fence_open(without_blockquote_prefixes(&lines[i])).is_some();
                let original =
                    quote_lines.get_or_init(|| source.source.lines().collect::<Vec<_>>());
                let mut cursor = LineCursor::new_with_cols(original, None, None);
                cursor.pos = i;
                cursor.in_item_body = !include_sublists;
                cursor.reached = Some(&source.reached);
                {
                    let _probing = ProbeGuard::enter();
                    let (_, _, opaque_tail) = collect_blockquote_body(&mut cursor, options);
                    opaque_quote_tail = marker_fence_quote && opaque_tail;
                }
                i = cursor.pos;
                after_blank = false;
                paragraph_open = false;
                block_at_minimum = true;
                continue;
            }
            // An opaque group already at the container's minimum column owns
            // its payload. Do not reconsider a fence-shaped payload line as a
            // separate authored-base opener.
            if let Some(open) = detect_fence_open(&lines[i]) {
                i += 1;
                while i < lines.len() {
                    let closes = is_fence_close(&lines[i], open);
                    i += 1;
                    if closes {
                        break;
                    }
                }
                after_blank = false;
                paragraph_open = false;
                block_at_minimum = term_boundary;
                continue;
            }
            if let Some(open) = detect_comment_fence_line(&lines[i]) {
                let comment_at = i;
                let close = entry_closers
                    .get_or_init(|| comment_closers_by_run(&lines))
                    .get(&open.fence_len)
                    .and_then(|at| at.get(at.partition_point(|&at| at <= i)).copied());
                i = close.map_or(i + 1, |at| at + 1);
                invisible_end = i.checked_sub(1);
                after_blank = false;
                paragraph_open = false;
                block_at_minimum = term_boundary
                    || (close == Some(comment_at + 1) && block_at_minimum)
                    || (close.is_none()
                        && !include_sublists
                        && (block_at_minimum || comment_at == 0));
                continue;
            }
            // Verse lines are inline content, including indented block markers.
            // Only the line block's own closer ends this opaque body.
            if let Some(width) = detect_line_block_open(&lines[i]) {
                last_literal_base = Some(0);
                i += 1;
                while i < lines.len() {
                    let closes = exact_colon_fence_len(&lines[i]) == Some(width);
                    i += 1;
                    if closes {
                        break;
                    }
                }
                after_blank = false;
                paragraph_open = false;
                // The completed block lets the next authored opener rebase.
                block_at_minimum = true;
                continue;
            }
            // A COLON CONTAINER AT THE MINIMUM COLUMN OWNS ITS EXTENT TOO. It
            // is not opaque, but its payload is still its own: CARVE-P0-004
            // keeps a run in the band between the container's column and a
            // fence base inside it at the authored column, and reading that run
            // again as an authored base dedents it onto the container's column,
            // where the container takes it as its closer. This pass runs twice
            // over one item body - `item_body` and then `parse_item_chunk` - so
            // the second run is where that happened (carve-rs#2095).
            //
            // The walk tracks colon widths only, as the base-past-zero arm
            // below does.
            if let Some(width) = detect_container_open(&lines[i])
                .map(|open| open.fence_len)
                .or_else(|| detect_quote_block_open(&lines[i]))
            {
                let marker_body = i == 0 && !source.marker_colon_rebased;
                let mut body_paragraph = false;
                let mut body_code: Option<(FenceOpen, usize)> = None;
                let mut body_comment: Option<usize> = None;
                let mut body_items: Vec<usize> = Vec::new();
                let mut comment_retains_frame = false;
                let mut rebased_until = 0;
                let mut stack = vec![width];
                i += 1;
                while i < lines.len() {
                    let base = indent_columns(&lines[i]);
                    if let Some((open, column)) = body_code {
                        if !is_blank_line(&lines[i]) && base < column {
                            body_code = None;
                        } else {
                            if base == column && is_fence_close(trim_ascii_start(&lines[i]), open) {
                                body_code = None;
                            }
                            i += 1;
                            continue;
                        }
                    }
                    if let Some(width) = body_comment {
                        if is_comment_fence_close_any_column(&lines[i], width) {
                            body_comment = None;
                        }
                        i += 1;
                        continue;
                    }
                    let comment_is_body_owned = body_items.is_empty();
                    let comment_line = is_line_comment_any_column(&lines[i]);
                    if is_blank_line(&lines[i]) || (!comment_line && base == 0) {
                        comment_retains_frame = false;
                    }
                    if comment_line {
                        comment_retains_frame =
                            body_items.last().is_some_and(|column| base < *column);
                    }
                    if comment_retains_frame
                        && !comment_line
                        && base > 0
                        && source.reached.get(i).copied().unwrap_or(false)
                        && item_block_opener(trim_ascii_start(&lines[i]))
                    {
                        comment_retains_frame = false;
                        body_paragraph = false;
                    }
                    while !comment_retains_frame
                        && body_items.last().is_some_and(|column| base < *column)
                        && !is_blank_line(&lines[i])
                    {
                        body_items.pop();
                    }
                    let owned_by_item = body_items.last().is_some_and(|column| base >= *column);
                    if (marker_body || !include_sublists)
                        && base > 0
                        && !owned_by_item
                        && !comment_retains_frame
                        && source.reached.get(i).copied().unwrap_or(false)
                    {
                        let local = trim_ascii_start(&lines[i]).to_owned();
                        let code = detect_fence_open(&local);
                        let colon = detect_container_open(&local)
                            .map(|open| open.fence_len)
                            .or_else(|| detect_line_block_open(&local))
                            .or_else(|| detect_hardbreaks_block_open(&local))
                            .or_else(|| detect_quote_block_open(&local));
                        let end = if let Some(open) = code {
                            let end = rebased_code_fence_end(&lines, i, base, open);
                            (!body_paragraph || is_fence_close(trim_ascii_start(&lines[end]), open))
                                .then_some(end)
                        } else if let Some(width) =
                            colon.filter(|_| exact_colon_fence_len(&local).is_none())
                        {
                            Some(rebased_colon_group_end(&lines, i, base, width))
                        } else if heading_content_starts(&local)
                            || detect_thematic_break(&local)
                            || is_table_start(&local)
                            || parse_standalone_attrs(&local).is_some()
                        {
                            Some(i)
                        } else if !body_paragraph && strip_blockquote_prefix(&local).is_some() {
                            // CARVE-P0-011: past the box's column it opens on
                            // its authored base, as a nested list does.
                            Some(rebased_quote_end(&lines, i, base))
                        } else if !body_paragraph
                            && parse_link_def_line_with_value(&local).is_some()
                        {
                            Some(i)
                        } else {
                            None
                        };
                        if let Some(end) = end {
                            if let Some(width) = colon {
                                rebase_named_colon_extent(
                                    &mut lines,
                                    &mut source.col_map,
                                    i,
                                    end,
                                    base,
                                    width,
                                );
                            } else {
                                // An unclosed fence owns its body to the box's end
                                // (CARVE-P0-014), so a colon run in it must not
                                // land on the box's column as its closer.
                                let unclosed = code.is_some_and(|open| {
                                    end == i || !is_fence_close(trim_ascii_start(&lines[end]), open)
                                }) && !fences_before
                                    .get_or_init(|| fences_open_before(&authored))
                                    .get(i)
                                    .copied()
                                    .unwrap_or(false);
                                for (at, line) in lines.iter_mut().enumerate().take(end + 1).skip(i)
                                {
                                    // A run on the box's own column closes the box.
                                    let on_base = indent_columns(line) > 0;
                                    let (residue, consumed, synthetic) =
                                        slice_columns_mapped(line, base, true);
                                    let framed = unclosed
                                        && at > i
                                        && on_base
                                        && exact_colon_fence_len(&residue).is_some();
                                    *line = Cow::Owned(if framed {
                                        format!("{LAZY}{residue}")
                                    } else {
                                        residue
                                    });
                                    if let Some(Some(col)) = source.col_map.get_mut(at) {
                                        *col += consumed as isize - synthetic as isize;
                                    }
                                }
                            }
                            source.marker_colon_rebased = true;
                            body_paragraph = false;
                            i = end + 1;
                            continue;
                        }
                    }
                    // A nested line block is opaque: its verse lines open
                    // nothing, and its closer leaves no paragraph open.
                    let nested_line_block = detect_line_block_open(trim_ascii_start(&lines[i]))
                        .filter(|_| base > 0 && owned_by_item)
                        .map(|width| (width, base, false))
                        .or_else(|| {
                            let marker = detect_list_marker_full_at(&lines[i], base)?;
                            let width = detect_line_block_open(marker.content)?;
                            Some((width, marker_content_col_at(&lines[i], base)?, true))
                        });
                    if let Some((width, column, item)) = nested_line_block {
                        if item {
                            body_items.push(column);
                        }
                        i += 1;
                        while i < lines.len() {
                            let line = &lines[i];
                            if !is_blank_line(line) && indent_columns(line) < column {
                                break;
                            }
                            i += 1;
                            if indent_columns(line) == column
                                && exact_colon_fence_len(trim_ascii_start(line)) == Some(width)
                            {
                                break;
                            }
                        }
                        body_paragraph = false;
                        continue;
                    }
                    if let Some(open) = detect_fence_open(trim_ascii_start(&lines[i]))
                        .filter(|_| base == 0 || owned_by_item)
                    {
                        body_code = Some((open, base));
                        i += 1;
                        continue;
                    }
                    if let Some(open) = detect_comment_fence_line(trim_ascii_start(&lines[i]))
                        .filter(|_| {
                            entry_closers
                                .get_or_init(|| comment_closers_by_run(&lines))
                                .get(
                                    &detect_comment_fence_line(trim_ascii_start(&lines[i]))
                                        .unwrap()
                                        .fence_len,
                                )
                                .is_some_and(|indices| indices.last().is_some_and(|last| *last > i))
                        })
                    {
                        if comment_is_body_owned {
                            comment_retains_frame = false;
                        }
                        body_comment = Some(open.fence_len);
                        body_paragraph = false;
                        i += 1;
                        continue;
                    }
                    if (!body_paragraph || !body_items.is_empty())
                        && detect_list_marker_full_at(&lines[i], base).is_some()
                    {
                        if let Some(column) = marker_content_col_at(&lines[i], base) {
                            body_items.push(column);
                        }
                    }
                    if marker_body
                        && !body_paragraph
                        && !owned_by_item
                        && !comment_retains_frame
                        && i >= rebased_until
                        && base > 0
                    {
                        if let Some(open) = detect_container_open(trim_ascii_start(&lines[i])) {
                            if exact_colon_fence_len(trim_ascii_start(&lines[i])).is_none() {
                                let end = rebased_colon_group_end(&lines, i, base, open.fence_len);
                                rebased_until = end + 1;
                                source.marker_colon_rebased = true;
                                rebase_named_colon_extent(
                                    &mut lines,
                                    &mut source.col_map,
                                    i,
                                    end,
                                    base,
                                    open.fence_len,
                                );
                            }
                        }
                    }
                    let line = &lines[i];
                    // A bare colon run closes a nested container, which ends
                    // its paragraph too (CARVE-P0-014: an unclosed fence below
                    // then has nothing to interrupt).
                    if is_blank_line(line)
                        || is_line_comment_any_column(line)
                        || (base > 0 && exact_colon_fence_len(trim_ascii_start(line)).is_some())
                    {
                        body_paragraph = false;
                    } else if detect_fence_open(trim_ascii_start(line)).is_some()
                        || !item_block_opener(trim_ascii_start(line))
                    {
                        body_paragraph = true;
                    }
                    i += 1;
                    if exact_colon_fence_len(line) == Some(*stack.last().unwrap()) {
                        body_paragraph = false;
                        stack.pop();
                        if stack.is_empty() {
                            break;
                        }
                        continue;
                    }
                    if line.starts_with([' ', '\t']) || stack.len() >= MAX_NESTING_DEPTH {
                        continue;
                    }
                    if let Some(len) = detect_container_open(line)
                        .map(|open| open.fence_len)
                        .or_else(|| detect_line_block_open(line))
                        .or_else(|| detect_hardbreaks_block_open(line))
                        .or_else(|| detect_quote_block_open(line))
                    {
                        stack.push(len);
                    }
                }
                after_blank = false;
                paragraph_open = false;
                block_at_minimum = true;
                continue;
            }
            // A definition entry at the container minimum remains the
            // innermost owner for its exact extent. Skip that extent so a
            // structural payload at the description column is not reconsidered
            // as an authored base of the outer body; the first line outside the
            // extent is deliberately classified by that outer body.
            if is_definition_list_start(&lines[i]) {
                let (end, term) = definition_entry_extent(&lines, i, 0, &entry_closers);
                term_boundary = term;
                description_tail = !term;
                i = end + 1;
                after_blank = false;
                paragraph_open = false;
                block_at_minimum = true;
                continue;
            }
            paragraph_open = line_starts_paragraph(strip_lazy(&lines[i]));
            item_separator_paragraph = lines[i].starts_with(ITEM_PARAGRAPH);
            own_paragraph = paragraph_open;
            if paragraph_open {
                term_boundary = false;
            }
            after_blank = false;
            // A comment reaching this frame closes its paragraph. A lower
            // comment preserves the owner established before it; the next
            // opener must still reach this frame to use that evidence.
            block_at_minimum = block_at_minimum
                || reaches_container
                || (i == 0 && source.item_marker_at_start)
                || (i == 0 && is_collected_definition_placeholder(&lines[i]))
                || !is_line_comment_any_column(&lines[i]);
            i += 1;
            continue;
        }
        let opener = strip_leading_columns(&lines[i], base);
        if (is_list_marker(&opener) && !include_sublists)
            || (!is_list_marker(&opener) && !item_block_opener(&opener))
        {
            let paragraph = line_starts_paragraph(&opener);
            if !description_tail && !include_sublists {
                let follows_invisible = i > 0
                    && (is_collected_definition_placeholder(&lines[i - 1])
                        || is_line_comment_any_column(&lines[i - 1])
                        || invisible_end == Some(i - 1));
                paragraph_after_invisible_base = if paragraph && follows_invisible {
                    Some(base)
                } else if paragraph && paragraph_open {
                    paragraph_after_invisible_base
                } else {
                    None
                };
                paragraph_open = paragraph;
            }
            if paragraph {
                term_boundary = false;
            }
            after_blank = false;
            i += 1;
            continue;
        }
        if i > 0
            && !after_blank
            && !paragraph_open
            && !(block_at_minimum
                && (reaches_container || !is_line_comment_any_column(&lines[i - 1])))
            && !source.authored_base_at_start
        {
            i += 1;
            continue;
        }

        paragraph_open = false;
        paragraph_after_invisible_base = None;

        let mut end = i;
        let code = detect_fence_open(&opener);
        let comment = code
            .is_none()
            .then(|| detect_comment_fence_line(&opener))
            .flatten();
        let line_block = detect_line_block_open(&opener);
        let colon = if code.is_none() && comment.is_none() {
            detect_container_open(&opener)
                .map(|open| open.fence_len)
                .or_else(|| detect_quote_block_open(&opener))
        } else {
            None
        };
        let footnote = code.is_none()
            && comment.is_none()
            && colon.is_none()
            && parse_footnote_def_line(&opener).is_some();
        if let Some(open) = code {
            end = rebased_code_fence_end(&lines, i, base, open);
        } else if let Some(ref open) = comment {
            end = rebased_comment_end(&lines, i, base, open.fence_len);
        } else if let Some(width) = line_block {
            last_literal_base = Some(base);
            end = rebased_line_block_end(&lines, i, base, width, &mut payload_closers);
        } else if let Some(width) = colon {
            end = rebased_colon_group_end(&lines, i, base, width);
        } else if footnote {
            end = rebased_footnote_end(&lines, i, base);
        } else if is_list_marker(&opener) {
            end = rebased_list_end(&lines, i, base);
        } else if strip_blockquote_prefix(&opener).is_some() {
            end = rebased_quote_end(&lines, i, base);
        } else if is_table_start(&opener) {
            end = rebased_table_end(&lines, i, base);
        } else if is_definition_list_start(&opener) {
            end = rebased_definition_list_end(&lines, i, base);
        } else if parse_standalone_attrs(&opener).is_some() {
            // A floating attribute line and the list marker it targets share
            // the authored base. Sublists are otherwise intentionally left to
            // their own column logic, so carry this one marker with its
            // attribute rather than leaving a residual indent between them.
            if let Some(candidate) = lines.get(i + 1) {
                let target = strip_leading_columns(candidate, base);
                if !is_blank_line(candidate)
                    && indent_columns(candidate) >= base
                    && (is_list_marker(&target) || (include_sublists && item_block_opener(&target)))
                {
                    end = i + 1;
                }
            }
        }
        // A caption is a structural continuation of the captionable block
        // immediately above it, and therefore shares that opener's authored
        // base. Leaving it at the authored column detaches it as literal text.
        if let Some(candidate) = lines.get(end + 1) {
            if !is_blank_line(candidate) && indent_columns(candidate) >= base {
                let local = strip_leading_columns(candidate, base);
                if local.starts_with("^ ") && local[2..].chars().any(|ch| !ch.is_whitespace()) {
                    end += 1;
                }
            }
        }
        let comment_close = comment.and_then(|open| {
            let at = entry_closers
                .get_or_init(|| comment_closers_by_run(&lines))
                .get(&open.fence_len);
            at.and_then(|at| at.get(at.partition_point(|&k| k <= i)).copied())
        });
        for (j, line) in lines.iter_mut().enumerate().take(end + 1).skip(i) {
            if comment_close.is_some_and(|close| j > i && j < close) {
                continue;
            }
            if is_blank_line(line) {
                if code.is_some() {
                    let (residue, consumed, synthetic) = slice_columns_mapped(line, base, true);
                    *line = Cow::Owned(residue);
                    if let Some(Some(col)) = source.col_map.get_mut(j) {
                        *col += consumed as isize - synthetic as isize;
                    }
                }
                continue;
            }
            if j == end && code.is_some_and(|open| is_fence_close(strip_lazy(line), open)) {
                *line = Cow::Owned(strip_lazy(line).to_string());
                continue;
            }
            // Code payload below the fence's base loses its whole prefix
            // (CARVE-P0-004); a run there still closes nothing. Colon payload
            // keeps its residual indentation, so an in-band run cannot become
            // a closer.
            if indent_columns(line) < base {
                if let Some(open) = code {
                    let consumed = indent_columns(line);
                    let residue = trim_ascii_start(line);
                    *line = Cow::Owned(if is_fence_close(residue, open) {
                        format!("{LAZY}{residue}")
                    } else {
                        residue.to_string()
                    });
                    if let Some(Some(col)) = source.col_map.get_mut(j) {
                        *col += consumed as isize;
                    }
                    continue;
                }
                if colon.is_some() || line_block.is_some() {
                    continue;
                }
            }
            if line_block.is_some() {
                let (residue, consumed, synthetic) = slice_columns_mapped(line, base, true);
                *line = Cow::Owned(residue);
                if let Some(Some(col)) = source.col_map.get_mut(j) {
                    // Positions count authored codepoints, excluding the spaces
                    // synthesized when a tab crosses the reference column.
                    *col += consumed as isize - synthetic as isize;
                }
                continue;
            }
            let (residue, consumed, synthetic) = slice_columns_mapped(line, base, true);
            *line = Cow::Owned(residue);
            if let Some(Some(col)) = source.col_map.get_mut(j) {
                // Codepoints, like the line-block arm above: a tab is one
                // codepoint however many columns it fills.
                *col += consumed as isize - synthetic as isize;
            }
        }
        i = end + 1;
        after_blank = false;
        // The block just moved to the minimum column, so it is evidence for
        // the next one exactly as an authored block written there is. Without
        // this a RUN of authored openers rebased only its first member.
        block_at_minimum = true;
    }
    if trailing_newline && lines.last().is_some_and(|line| line.is_empty()) {
        lines.pop();
    }
    let mut rebased = lines.join("\n");
    if trailing_newline {
        rebased.push('\n');
    }
    source.source = rebased;
    source.marker_tail = None;
}
