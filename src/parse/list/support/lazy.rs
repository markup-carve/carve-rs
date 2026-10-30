use super::*;

pub(crate) fn fold_lazy_run_and_resume_in_band(
    cur: &mut LineCursor,
    nested: &mut MappedSource,
    content_col: usize,
    lazy_scope: (usize, usize),
    frame: (bool, &Options<'_>),
    mut open: impl FnMut(&MappedSource, bool) -> bool,
    mut resume: impl FnMut(&mut LineCursor) -> MappedSource,
) {
    let (frame_band_lines, options) = frame;
    let (lazy_column, list_base) = lazy_scope;
    let mut pending_chunks = Vec::new();
    let mut staged = LineBuffer::default().into_source();
    let mut flush_separator_follows_other_block = false;
    loop {
        if cur
            .peek()
            .is_some_and(|line| indent_columns(line) == 0 && detect_fence_open(line).is_some())
            && matched_comment_before(cur).is_some_and(|at| marked_quote_before_comment(cur, at))
        {
            break;
        }
        // CARVE-P2-017: a flush separator leaves an item's definition list.
        if !frame_band_lines
            && !flush_separator_follows_other_block
            && cur.peek().is_some_and(|line| {
                indent_columns(line) == 0 && strip_description_prefix(line).is_some()
            })
        {
            if body_ends_in_definition_list(&nested.source, options) {
                break;
            }
            flush_separator_follows_other_block = true;
        }
        // A marker between the host's base and content columns can fold into
        // an open paragraph inside a raised colon container (carve#2474).
        let pending = if lazy_column == 0 {
            lazy_line_pending(cur)
        } else {
            cur.peek().map(str::to_string).is_some_and(|line| {
                let indent = indent_columns(&line);
                !is_blank_line(&line)
                    && indent <= lazy_column
                    && !(is_list_marker(&line) && indent <= list_base)
                    && !interrupts_lazy_continuation(cur, &line)
            })
        };
        if !pending {
            break;
        }
        // The column the run ended at, taken before the fold consumes anything.
        let below = last_consumed_line_below_column(cur, content_col);
        if !open_after_refused_marker(nested, below, &mut open) {
            break;
        }
        let before = cur.pos;
        collect_trailing_lazy_through_framed(
            cur,
            if frame_band_lines {
                &mut staged
            } else {
                &mut *nested
            },
            lazy_column,
            list_base,
            content_col,
            frame_band_lines,
            (false, None),
        );
        if cur.pos == before {
            break;
        }
        while cur.peek().is_some_and(|line| {
            indent_columns(line) > list_base
                && indent_columns(line) <= content_col
                && is_list_marker(line)
        }) && quote_fold_at_end(nested.source.rsplit('\n'), &nested.line_map)
        {
            let line = cur.peek().unwrap();
            nested.push_newline_at(
                format!("{LAZY}{}", trim_ascii_start(line)),
                cur.source_line(cur.pos),
                cur.source_col(cur.pos),
            );
            cur.consume();
        }
        let before_resume = cur.pos;
        let resumed = resume(cur);
        if !frame_band_lines && body_ends_in_definition_list(&resumed.source, options) {
            flush_separator_follows_other_block = false;
        }
        if frame_band_lines {
            pending_chunks.push(std::mem::replace(
                &mut staged,
                LineBuffer::default().into_source(),
            ));
            pending_chunks.push(resumed);
        } else {
            nested.append(resumed);
        }
        let next_is_comment = cur.pos == before_resume
            && cur
                .peek()
                .map(str::to_string)
                .is_some_and(|line| line_comment_keeps_the_container_open(cur, &line));
        if cur.pos == before_resume && !next_is_comment {
            break;
        }
    }
    if frame_band_lines {
        pending_chunks.push(staged);
        nested.append_many(pending_chunks);
    }
}

pub(crate) fn collect_marker_sublist_lazy_through(
    cur: &mut LineCursor,
    nested: &mut MappedSource,
    base_indent: usize,
    list_base: usize,
    content_col: usize,
    ancestor_column: usize,
) -> bool {
    collect_trailing_lazy_through_framed(
        cur,
        nested,
        base_indent,
        list_base,
        content_col,
        false,
        (false, Some(ancestor_column)),
    )
}

pub(crate) fn collect_trailing_lazy_through_framed(
    cur: &mut LineCursor,
    nested: &mut MappedSource,
    base_indent: usize,
    list_base: usize,
    content_col: usize,
    frame_band_lines: bool,
    lazy_scope: (bool, Option<usize>),
) -> bool {
    let (after_comment, ancestor_quote_column) = lazy_scope;
    let mut buffered = frame_band_lines.then(LineBuffer::default);
    if cur.peek().is_some_and(|line| {
        matches!(line.as_bytes().first(), Some(b'`' | b'~')) && detect_fence_open(line).is_some()
    }) {
        return false;
    }
    while let Some(line) = cur.peek() {
        let indent = indent_columns(line);
        if is_blank_line(line)
            || (indent > base_indent
                && !(after_comment
                    && indent < content_col
                    && line_starts_paragraph(trim_ascii_start(line))
                    && !item_block_opener(trim_ascii_start(line))))
            || ancestor_quote_column
                .is_some_and(|column| marked_quote_at_ancestor_column(line, column, content_col))
            || (is_list_marker(line) && indent_columns(line) <= list_base)
            || (is_plus_marker(trim_ascii_start(line))
                && cur
                    .continuation_decision(cur.pos)
                    .map_or(list_base == 0 && !cur.in_item_body, |decision| {
                        decision.refusal.is_none()
                    }))
            || {
                // AS A CONTAINER: this collector runs only for a line folding
                // into a container's open paragraph, and §7 recognizes an
                // abbreviation definition only as a direct child of the
                // document. Asked at document level it ended the fold, so `- >q`
                // / `*[A]: b` dropped the author's line and defined an
                // abbreviation where the sibling paths fold it (carve-rs#2096).
                let line_owned = line.to_string();
                interrupts_lazy_continuation_as_container(cur, &line_owned)
            }
        {
            break;
        }
        // A BLOCK-SHAPED LINE KEEPS ITS COLUMN HERE TOO. This collector took
        // every lazy line flush, so a `:::` written below every content column
        // arrived at the next re-parse spelled as a closer and published an
        // empty `<div>` where the spec folds it (carve-rs#1510). The column is
        // the whole reason the line is text, and `dedent_for_collection`
        // already spends one to say so - the two collectors have to agree, or
        // the same line parses differently depending on which one took it.
        let indent = indent_columns(line);
        // A block-shaped band line is retained text (PART 1 S4). One kept column
        // does not say so to a term, which a list marker ends at any indent and
        // which publishes the column as a leading space. The frame does.
        let framed =
            indent > list_base && (frame_band_lines || dedent_below_column(line, indent) != indent);
        let take = if framed {
            indent
        } else {
            dedent_below_column(line, indent)
        };
        let (sliced, consumed, _) = slice_columns_mapped(line, take, true);
        // A FENCE AT COLUMN 0 HAS NO COLUMN TO SPEND, so it gets the sentinel
        // instead (carve-rs#2313). One column is what keeps a block-shaped line
        // text on the re-parse, and a flush line has none: the fence shape then
        // opened a block one level in, where the fold had just made it the
        // paragraph's text. The description-body twin frames it the same way.
        let sliced =
            if framed || (indent == 0 && detect_fence_open(trim_ascii_start(line)).is_some()) {
                format!("{LAZY}{sliced}")
            } else {
                sliced
            };
        let source_line = cur.source_line(cur.pos);
        let source_col = cur.source_col(cur.pos).map(|col| col + consumed as isize);
        if let Some(buffered) = &mut buffered {
            buffered.push_at(sliced, source_line, source_col);
        } else {
            nested.push_newline_at(sliced, source_line, source_col);
        }
        cur.consume();
    }
    if let Some(buffered) = buffered {
        nested.append(buffered.into_source());
    }
    false
}

pub(crate) fn marked_quote_at_ancestor_column(
    line: &str,
    ancestor_column: usize,
    content_col: usize,
) -> bool {
    let indent = indent_columns(line);
    strip_blockquote_prefix(trim_ascii_start(line)).is_some()
        && indent >= ancestor_column
        && indent < content_col
}
/// Is the line that ends this chunk a BAND line, strictly between the list's base
/// and the item's content column?
///
/// The floor is raised only for one, so a chunk with nothing in the band collects
/// exactly as it did. Without this the raise reached a chunk that opens with an
/// attribute block and its target stopped being reachable, reported as unattached.
///
/// A COMMENT IS NOT ONE. It ends nothing at any column (PART 9 SS24 C3), so the
/// question the raised floor asks is not about it, and the chunk keeps collecting
/// through it as it always did. Left in, `- t` / `  # g` / ` %% c` / `  p` put `p`
/// outside the list, where the oracle keeps it. That the two released engines put
/// it outside is a divergence of its own and not this floor's to settle.
///
/// A blank ends the run: past one a below-column line ends the list through the
/// ordinary dedent, which needs no floor.
pub(crate) fn a_band_line_ends_the_chunk(
    cur: &LineCursor,
    _base_indent: usize,
    content_col: usize,
) -> bool {
    for candidate in cur.lines.range(cur.pos..cur.lines.len()) {
        if is_blank_line(candidate) {
            return false;
        }
        let indent = indent_columns(candidate);
        if indent >= content_col {
            continue;
        }
        if detect_comment_fence_line_any_column(candidate).is_some()
            || is_line_comment_any_column(candidate)
        {
            return false;
        }
        return true;
    }
    false
}

/// Two chunks whose band line stays with the collector: one ending at a lone
/// `+`, which is text of the innermost item (§17 L3), and one ending at the
/// closer of a code fence opened on a marker line, where the line after it is
/// still read as the outer item's.
pub(crate) fn chunk_keeps_its_band_line(cur: &LineCursor, content_col: usize) -> bool {
    let mut open: Option<FenceOpen> = None;
    let mut closed_last = false;
    for (offset, line) in cur.lines.range(cur.pos..cur.lines.len()).enumerate() {
        if is_blank_line(line) || indent_columns(line) < content_col {
            return closed_last
                || trim_ascii(line) == "+"
                    && cur
                        .continuation_decision(cur.pos + offset)
                        .is_some_and(|decision| decision.refusal.is_none());
        }
        let local = trim_ascii_start(line);
        closed_last = open.is_some_and(|fence| is_fence_close(local, fence));
        if closed_last {
            open = None;
        } else if open.is_none() {
            open = detect_list_marker_full(local)
                .and_then(|marker| detect_fence_open(innermost_marker_content(marker.content)));
        }
    }
    false
}

/// Does the chunk about to be collected leave something open for a band line to
/// continue?
///
/// Read off the cursor WITHOUT consuming, because the answer decides the
/// collection floor and the collection is what would otherwise take the band line
/// (carve-rs#2128). Only the contiguous run at or past the content column is the
/// chunk; a blank ends it, and a line below the column is the band line itself.
///
/// Three things a band line can continue, each answered where it can be seen:
///
/// - AN OPEN PARAGRAPH, asked of the LEAD alone. A quote holding one takes a
///   continuation marker written in the band (corpus 435-14), and the fold hands
///   an ordinary lazy line back anyway, so the lead is enough and stays cheap.
/// - AN OPEN CODE FENCE, whose payload the band line is. Its opener is the lead,
///   so the closer has to be looked for in the run.
/// - AN OPEN COLON CONTAINER, the same way.
///
/// The run is scanned, never parsed, so this stays a linear read of lines the
/// collector is about to walk regardless.
pub(crate) fn chunk_run_leaves_content_open(
    cur: &LineCursor,
    content_col: usize,
    lead_at_column: &str,
    options: &Options<'_>,
) -> bool {
    let mut run: Vec<String> = Vec::new();
    for candidate in cur.lines.range(cur.pos..cur.lines.len()) {
        if is_blank_line(candidate) || indent_columns(candidate) < content_col {
            break;
        }
        run.push(slice_columns(candidate, content_col, false));
    }
    if body_ends_with_open_paragraph(lead_at_column, options) {
        let mut complete = lead_at_column.to_owned();
        for line in run.iter().skip(1) {
            complete.push('\n');
            complete.push_str(line);
        }
        return body_ends_with_open_paragraph(&complete, options);
    }
    if let Some(open) = detect_fence_open(lead_at_column) {
        return !run.iter().skip(1).any(|later| is_fence_close(later, open));
    }
    colon_fences_left_open(&run.join("\n")) > 0
}

pub(crate) fn collect_item_continuation_block_mapped(
    cur: &mut LineCursor,
    parent_indent: usize,
    content_col: usize,
    open_fence: &mut Option<FenceOpen>,
    options: &Options<'_>,
) -> MappedSource {
    collect_indented_block_mapped_with(
        cur,
        parent_indent,
        content_col,
        true,
        open_fence,
        false,
        options,
    )
}

pub(crate) fn nested_bare_marker_then_plain(cur: &LineCursor<'_>, content_col: usize) -> bool {
    let Some(outer_at) = cur.pos.checked_sub(2) else {
        return false;
    };
    let Some(next) = cur.peek() else {
        return false;
    };
    if next.starts_with([' ', '\t']) || !line_starts_paragraph(next) {
        return false;
    }
    let Some(outer) = detect_list_marker_full(cur.lines.at(outer_at)) else {
        return false;
    };
    let Some(inner) = detect_list_marker_full(cur.lines.at(outer_at + 1)) else {
        return false;
    };
    outer.indent == 0
        && line_starts_paragraph(outer.content)
        && inner.indent == content_col
        && is_plus_marker(inner.content)
}

pub(crate) fn refused_first_block_below_column(cur: &LineCursor<'_>, content_col: usize) -> bool {
    let Some(line) = cur.peek() else {
        return false;
    };
    let list_marker = detect_list_marker_full(line).is_some();
    cur.pos > 0
        && indent_columns(line) < content_col
        && cur
            .continuation_decision(cur.pos - 1)
            .is_some_and(|decision| {
                // A refused first-block `+` in a quote is text, so a lazy line folds into it.
                // A list's refused `+` leaves its item empty, so not even a marker folds
                // (carve#3421).
                decision.first_block
                    && decision.refusal.is_some()
                    && if decision.owner == Some(crate::parse::continuation_marker::Kind::Quote) {
                        is_blank_line(line) && !list_marker
                    } else if decision.owner == Some(crate::parse::continuation_marker::Kind::List)
                        && !list_marker
                        && indent_columns(line) == 0
                        && line_starts_paragraph(trim_ascii_start(line))
                        && {
                            // A quote opened behind the item's own marker ends at its list
                            // `+`, and the item takes the paragraph line (`- > - +` / `b`).
                            let prior = trim_ascii_start(cur.lines.at(cur.pos - 1));
                            !prior.starts_with('>') && prior.contains('>')
                        }
                    {
                        false
                    } else {
                        !list_marker
                            || decision.owner == Some(crate::parse::continuation_marker::Kind::List)
                                && indent_columns(line) > 0
                                && {
                                    // Not a quote opened behind this item's own marker, whose
                                    // list `+` leaves the marker line's item to take the line.
                                    let raw = cur.lines.at(cur.pos - 1);
                                    let prior = trim_ascii_start(raw);
                                    prior.starts_with('>')
                                        || !prior.contains('>')
                                        || indent_columns(line) <= indent_columns(raw)
                                }
                    }
            })
}

/// How far to dedent a collected line, given the container's content column.
///
/// At or past that column the line IS item content and lands flush at column 0.
/// Below it the line never reached the column, so it is lazy paragraph text: it
/// keeps its OWN indentation instead, since dedenting it to 0 is what let the
/// recursive parse read it as a block. `- - a` / ` # H` published an `<h1>`, and
/// ` - b` a sibling item, where a below-column line at the TOP level already
/// folded as text (carve-rs#512).
pub(crate) fn dedent_for_collection(line: &str, indent: usize, strip_cols: usize) -> usize {
    if indent >= strip_cols {
        return strip_cols;
    }
    dedent_below_column(line, indent)
}

/// How much of a BELOW-COLUMN line's own indentation a collector takes.
///
/// Split out of `dedent_for_collection` because `collect_trailing_lazy_through`
/// asks the same question and used to answer it "all of it", which is how a
/// below-column `:::` reached the next re-parse spelled flush
/// (markup-carve/carve-rs#1510).
pub(crate) fn dedent_below_column(line: &str, indent: usize) -> usize {
    // Two exceptions dedent all the way. A DEFINITION (`:  `) attaches to the
    // term above it from ANY column, which is why an under-indented one is a
    // `<dd>` and not lazy text (corpus 154) - its TERM (`:: `) is not lenient
    // and folds like everything else. And a comment is invisible wherever it
    // sits: this engine finds it after trimming, so keeping the column would
    // leave its blank line in a different place without making it text.
    let trimmed = trim_ascii_start(line);
    if trimmed.starts_with(":  ") || trimmed.starts_with("%%") {
        return indent;
    }
    // Plain text dedents all the way too: its leading whitespace is not
    // significant, and keeping it would publish a `<dt>term\n wrapped</dt>`
    // where the corpus wants the wrapped line flush (corpus 156). Only a
    // block-SHAPED line needs its column back - that column is the whole
    // reason it is text rather than the block it looks like.
    if line_starts_paragraph(trimmed) {
        return indent;
    }
    // A block-shaped line keeps exactly ONE column, whatever it was indented
    // by. §24 C3: BELOW the content column a marker folds as lazy item text and
    // no other opener nests either - the depth of the indent does not enter
    // into it, and Rule B's "any indent" is scoped to where a TOP-LEVEL list
    // may open (C4), not to nesting. Keeping the ORIGINAL column let a line two
    // columns in reach the SUB-list's content column inside the re-parsed
    // stream and open a list there, which is what all three engines used to do
    // (carve#603). One column can reach no content column at all, so the fold
    // holds at every depth.
    indent.saturating_sub(1)
}

pub(crate) fn collect_indented_block_mapped(
    cur: &mut LineCursor,
    parent_indent: usize,
    strip_cols: usize,
    options: &Options<'_>,
) -> MappedSource {
    collect_indented_block_mapped_with(
        cur,
        parent_indent,
        strip_cols,
        false,
        &mut None,
        false,
        options,
    )
}

/// Collect the block named by a first-block continuation marker.
pub(crate) fn collect_indented_block_mapped_after_continuation(
    cur: &mut LineCursor,
    parent_indent: usize,
    strip_cols: usize,
    options: &Options<'_>,
) -> MappedSource {
    // A continuation assigned to an ancestor belongs to that ancestor's
    // collector. The nested source cannot claim it merely because it is flush.
    if cur.peek().is_some_and(is_plus_marker)
        && cur.continuation_decision(cur.pos).is_some_and(|decision| {
            decision.refusal.is_none()
                && decision
                    .owner_column
                    .is_some_and(|column| column < strip_cols)
        })
    {
        return LineBuffer::default().into_source();
    }
    if !cur.in_item_body
        && cur.peek().is_some_and(|line| {
            indent_columns(line) == 0 && detect_list_marker_full(line).is_none()
        })
    {
        let start = cur.pos;
        let mut end = attached_block_lines_classified(
            cur.lines,
            start,
            &mut cur.comment_closer_last_index,
            &mut cur.code_closer_last_index,
            options,
            &mut |line, at| {
                at > start && (is_plus_marker(line) || detect_list_marker_full(line).is_some())
            },
        );
        if is_line_comment_any_column(cur.lines.at(start))
            && cur
                .lines
                .line(end)
                .is_some_and(|line| indent_columns(line) == 0 && line_starts_paragraph(line))
        {
            end = attached_block_lines_classified(
                cur.lines,
                end,
                &mut cur.comment_closer_last_index,
                &mut cur.code_closer_last_index,
                options,
                &mut |line, _| is_plus_marker(line) || detect_list_marker_full(line).is_some(),
            );
        }
        let mut block = LineBuffer::default();
        while cur.pos < end {
            block.push_at(
                cur.lines.at(cur.pos).to_string(),
                cur.source_line(cur.pos),
                cur.source_col(cur.pos),
            );
            cur.consume();
        }
        return block.into_source();
    }
    collect_indented_block_mapped_with(
        cur,
        parent_indent,
        strip_cols,
        false,
        &mut None,
        true,
        options,
    )
}

/// The same collection, told that a fenced code block is ALREADY OPEN because
/// its opener was the item's MARKER LINE (`- ``` `) - a line this collector
/// never sees. Without the seed the guard below has nothing to guard, and the
/// body's first below-column line folds in (PART 9 §24, markup-carve/carve#950).
pub(crate) fn collect_indented_block_mapped_after_fence(
    cur: &mut LineCursor,
    parent_indent: usize,
    strip_cols: usize,
    open_fence: &mut Option<FenceOpen>,
    options: &Options<'_>,
) -> MappedSource {
    collect_indented_block_mapped_with(
        cur,
        parent_indent,
        strip_cols,
        false,
        open_fence,
        false,
        options,
    )
}

pub(crate) fn recent_nested_fence_lead(lines: &[String]) -> bool {
    lines
        .iter()
        .rev()
        .find(|line| !is_blank_line(line))
        .is_some_and(|line| {
            detect_list_marker_full(line).is_some_and(|marker| {
                detect_fence_open(innermost_marker_content(marker.content)).is_some()
            })
        })
}
