use super::*;

/// What `extract_footnote_defs` hands back, in order: the document body with
/// the definition lines removed, each definition's own source, where each
/// definition LINE sits, and the link definitions lifted out of the note bodies.
pub(in crate::parse) type FootnoteExtraction = (
    MappedSource,
    BTreeMap<String, MappedSource>,
    BTreeMap<String, Pos>,
    BTreeMap<String, LinkDef>,
);

/// Footnote definitions, and the LINK DEFINITIONS written inside their bodies.
///
/// A note body is lifted out of the document here, before `extract_link_defs`
/// runs - so a `[r]: /u` on a body continuation line was never offered to that
/// pass at all. It stayed in the body and rendered as text, and the reference
/// below it never resolved (carve-rs#599). carve-js, carve-php and the
/// executable spec all collect it.
///
/// Collected here rather than by re-running the link pass over the extracted
/// body: the body's `line_map` is built line by line in this loop, and a pass
/// that removes a line from the middle of a finished body would have to rebuild
/// that mapping from nothing.
pub(in crate::parse) fn extract_footnote_defs(
    source: &str,
    first_source_line: usize,
    positions: bool,
    // Inside a NOTE BODY (the recursion), a `[^x]: ` line is a nested definition
    // at ANY indent - a note's body recognizes them flatly rather than by the
    // per-level content column the top level uses, so a definition one column
    // deeper than its parent is still hoisted (carve-rs#1560).
    nested: bool,
    source_line_map: Option<&[Option<usize>]>,
    options: &Options<'_>,
) -> FootnoteExtraction {
    let lines: Vec<&str> = source.lines().collect();
    let recent_list_marker = list_marker_since_blank(&lines);
    let decision_at = |line: usize| {
        let physical = match source_line_map {
            Some(map) => map
                .get(line.checked_sub(first_source_line)?)
                .copied()
                .flatten()?,
            None => line,
        };
        continuation_marker::decision(physical)
    };
    // See `probe_budget_for`: spent by `line_folds_into_an_open_paragraph`, and
    // running out only ever collects a definition this guard would have left as
    // text.
    let mut probe_budget = probe_budget_for(source.len());
    let mut body = Vec::new();
    let mut body_line_map = Vec::new();
    let mut defs = BTreeMap::new();
    let mut definition_keys: BTreeMap<String, String> = BTreeMap::new();
    // Where each definition was written. Lines and columns are final here; the
    // OFFSETS are filled by the caller, which is the only place that holds the
    // original text the offsets are measured in - the same split `fill_offsets`
    // already uses for every other block.
    let mut def_positions: BTreeMap<String, Pos> = BTreeMap::new();
    let mut note_link_defs: BTreeMap<String, LinkDef> = BTreeMap::new();
    let mut body_opacity_candidates: Option<bool> = None;
    // Fence state for the note-body walk below, declared per definition since a
    // fence cannot span two notes.
    let mut in_fence: Option<FenceOpen> = None;
    let mut term_fence_quotes = 0;
    let mut term_fence_columns = Vec::new();
    let mut term_fence_owned = false;
    let mut description_fence = false;
    let mut description_blank = false;
    let mut inline_marker_fence = false;
    let mut inline_marker_paragraph = None;
    let mut term_fence_floor = 0;
    // Whether that fence has a closer, which decides whether a dedent ends it.
    let mut fence_closer_ahead = false;
    // A LINE BLOCK's body is inline content, so a definition-shaped line inside
    // one is text. Without this the line was extracted here and never reached
    // the block parser, so it vanished from the output (#491). carve-js keeps it
    // and does NOT register the footnote; this matches that.
    let verse_lines = verse_owned_lines(source, options);
    // A `%%%` comment fence is opaque, so a literal `::: |` inside one is not
    // an opener. Entering the state there left it open past the comment's own
    // closer - which is not a colon fence - and every later definition in the
    // document was skipped. Comment bodies remain opaque here.
    let mut in_comment_fence: Option<OpenCommentFence> = None;
    let comment_fence_closers = comment_fence_close_index(&lines);
    let closer_maxima = fence_closer_suffix_maxima(&lines);
    let folded_into_term = term_folded_lines(&lines, &closer_maxima, options);
    // Built on the first CONTAINER-scoped opener and never for a document that
    // has none, which is every document that only ever writes `%%%` at column 0.
    let mut container_closers: Option<ContainerCommentClosers> = None;
    let mut comment_closers: Option<HashMap<usize, usize>> = None;
    let mut closer_index = None;
    // See ContentColumns: a definition on an item's CONTINUATION line carries no
    // marker, so without this the line kept its indentation, stopped looking
    // like a definition, and was neither collected nor rendered - the author's
    // line disappeared and a reference to it stayed literal (carve-rs#568).
    let mut columns = ContentColumns::new();
    // The line the `after_term` gate below asks about. See its note: a blank
    // line between entries is transparent to a definition list, so the gate
    // reads past one.
    let mut previous_non_blank = "";
    let mut last_folded_marker = false;
    let mut i = 0;
    while i < lines.len() {
        let folded_fence = detect_fence_open(trim_ascii_start(lines[i])).is_some_and(|open| {
            !closer_maxima.has_run_after(i, open)
                && prepass_paragraph_open_above(&lines, i, &[], &folded_into_term)
        });
        // A footnote definition is collected at the top level AND from inside a
        // blockquote / bullet-list container: `> [^a]: body` and `- [^a]: body`
        // both stash the def and leave the container empty, matching carve-js
        // (which recognizes the def inside the container's sub-lexer). Strip the
        // container prefix first, then test the bare content (corpus 115).
        // The footnote prepass asks per LINE which column a definition reaches
        // (see `reached_by`), so the innermost column alone is not enough here.
        // Called for its effect on the column stack, not for its return: every
        // question below asks `reached_by` about a specific column instead of
        // taking the innermost one (carve-rs#1054).
        let mut marker_probe_unaffordable = false;
        let folded_marker = marker_line_may_be_lazy(lines[i])
            && (last_folded_marker || !marker_follows_sibling(&body, lines[i]))
            && line_folds_into_an_open_paragraph(
                &body,
                lines[i],
                options,
                &mut probe_budget,
                &mut marker_probe_unaffordable,
            );
        last_folded_marker = folded_marker;
        let retained_unclosed_fence = i > 0
            && is_line_comment_any_column(lines[i - 1])
            && indent_columns(lines[i]) == 0
            && columns.current().cols.last().is_some()
            && detect_fence_open(lines[i])
                .is_some_and(|open| !closer_maxima.has_run_after(i, open));
        columns.observe(
            lines[i],
            in_fence.is_some()
                || folded_fence
                || folded_marker
                || verse_lines[i].is_body()
                || in_comment_fence.is_some()
                || prepass_quote_line_is_folded(&folded_into_term, i, lines[i])
                || retained_unclosed_fence,
        );
        // A quote nested in a list item sits AT the item's content column, so
        // the prefix scan needs that column to see it (carve-rs#588).
        //
        // THE PREVIOUS NON-BLANK LINE, not the one directly above: a blank
        // between description entries does not end the list, it only makes it
        // loose. Asking the line above left the `:  ` marker unstripped after
        // one, so a definition written as that description's body was consumed
        // by the parser and collected by nobody (#1500).
        let after_term = opens_definition_entry(previous_non_blank);
        let stripped = strip_container_prefixes_at(lines[i], &columns, after_term);
        remember_prepass_paragraph_line(&folded_into_term, i, lines[i], &stripped, &columns);
        if !is_blank_line(lines[i]) {
            previous_non_blank = lines[i];
        }
        let in_container = !stripped.structural.is_empty();
        // A footnote definition is NEVER collected from inside a fenced code
        // block: a `[^x]: ...` line there is literal content. The prepass has
        // only a prefix-stripped line, not the block parser's container-column
        // context, so it recognizes fences only with no residual indentation.
        // This can collect a def from a container-nested fence body, but avoids
        // opening a fence the block parser never opens and swallowing every
        // later definition in the document.
        let term_fence = folded_into_term.fence_openers.get(&i);
        let fence_line = if let Some(host) = term_fence {
            &lines[i][host.opener_offset..]
        } else {
            stripped.bare
        };
        if verse_lines[i].is_owned() {
            body.push(lines[i].to_string());
            body_line_map.push(Some(first_source_line + i));
            i += 1;
            continue;
        }
        if let Some(open) = in_fence {
            if inline_fence_paragraph_ends(lines[i], open, inline_marker_paragraph.as_ref())
                || (description_fence
                    && (description_blank || !inline_marker_fence)
                    && description_fence_leaves_after_blank(lines[i], open, &term_fence_columns))
                || !inline_fence_lazy_line(lines[i], inline_marker_paragraph.as_ref())
                    && ((inline_marker_fence
                        && is_blank_line(without_blockquote_prefixes(lines[i])))
                        || (term_fence_owned
                            && term_fence_ends_above(
                                lines[i],
                                term_fence_floor,
                                &term_fence_columns,
                            ))
                        || prepass_fence_ends_above(lines[i], open, fence_closer_ahead))
            {
                in_fence = None;
            } else {
                advance_inline_paragraph(lines[i], inline_marker_paragraph.as_mut());
                end_prepass_paragraph_at(&folded_into_term, i);
                body.push(lines[i].to_string());
                body_line_map.push(Some(first_source_line + i));
                let close_kept = if term_fence_quotes > 0 {
                    strip_prepass_quote_levels(lines[i], term_fence_quotes).unwrap_or(lines[i])
                } else {
                    lines[i]
                };
                let close_line = if !open.quoted && open.content_col == 0 {
                    fence_line
                } else if leading_ws(close_kept) >= open.content_col {
                    &close_kept[open.content_col..]
                } else {
                    close_kept
                };
                description_blank = description_fence && is_blank_line(close_kept);
                if (!description_fence || leading_ws(close_kept) >= open.content_col)
                    && is_fence_close(close_line, open)
                {
                    if inline_marker_fence {
                        folded_into_term
                            .paragraph_quotes
                            .borrow_mut()
                            .insert(i, inline_marker_paragraph.clone());
                    }
                    in_fence = None;
                }
                i += 1;
                continue;
            }
        }
        if let Some(open) = in_comment_fence {
            end_prepass_paragraph_at(&folded_into_term, i);
            // ANY column, matching `comment_fence_close_index` and the block
            // parser. A comment fence closes at whatever indent its closer sits
            // at (PART 9 §24 C3, markup-carve/carve#629), so testing the strict
            // form here made this pass disagree with the one that decides: the
            // block parser closed the comment, this scan did not, and every
            // definition after it went unregistered and came back as visible
            // text (#574 regression). Any column AT THE OPENER'S QUOTE DEPTH -
            // see `closes_open_comment_fence`.
            if closes_open_comment_fence(lines[i], open) {
                in_comment_fence = None;
            }
            // A comment's body is OPAQUE: a `[^a]: note` inside one is comment
            // text and defines nothing. The state was tracked here already, but
            // only to gate the line-block opener below, so the scan still walked
            // in and registered the footnote - producing an endnote nobody wrote
            // and a live reference for a later `see [^a]` (#504).
            body.push(lines[i].to_string());
            body_line_map.push(Some(first_source_line + i));
            i += 1;
            continue;
        } else if let Some((open, scope)) = detect_comment_fence_opener_scoped(lines[i], &columns) {
            // THE CONTAINER BOUNDS THE SPAN, NOT THE DELIMITER. For a column
            // scope the bound ends the container at the first line that dedents
            // below the column the fence REACHES, so measuring from where the
            // `%%%` happens to be written read a legal body line as the end:
            // `- item` / `    %%%` / `  [r]: /u` / `    %%%` put the fence at 4
            // and its body at 2, still inside the item, and the fence was
            // declined (carve-rs#1054). For a quote scope the blank line is the
            // bound - see `CommentFenceScope`.
            if comment_fence_scope_closes(
                scope,
                &lines,
                i,
                open.fence_len,
                &columns,
                &comment_fence_closers,
                &mut container_closers,
            ) {
                end_prepass_paragraph_at(&folded_into_term, i);
                in_comment_fence = Some(OpenCommentFence {
                    fence_len: open.fence_len,
                    quote_depth: scope.quote_depth(),
                });
                body.push(lines[i].to_string());
                body_line_map.push(Some(first_source_line + i));
                i += 1;
                continue;
            }
        }

        if let Some(mut open) =
            detect_fence_open(fence_line).filter(|_| !folded_into_term.folded.contains(&i))
        {
            if stripped.description {
                open.content_col = description_fence_content_column(lines[i], &stripped, &columns);
            }
            if let Some(host) = term_fence {
                open.content_col = host.content_col;
                open.quoted = host.quote_depth > 0;
            }
            // §10 I4, as `extract_link_defs_with_guard`: an unterminated fence
            // interrupts no open paragraph, so going opaque here hid every
            // definition below one from collection (carve-rs#2096).
            let fence = prepass_fence_opens(
                &lines,
                i,
                open,
                &closer_maxima,
                &folded_into_term,
                &columns,
                after_term,
            );
            if fence.opens {
                if retained_unclosed_fence {
                    open.content_col = columns.current().cols.last().copied().unwrap_or(0);
                }
                if !open.quoted && fence.quote_columns.is_empty() && fence.host_content_col > 0 {
                    open.content_col = fence.host_content_col;
                }
                end_prepass_paragraph_at(&folded_into_term, i);
                in_fence = Some(open);
                inline_marker_fence = fence.inline_marker;
                inline_marker_paragraph = if fence.inline_marker {
                    previous_prepass_paragraph(&folded_into_term, i)
                } else {
                    None
                };
                term_fence_quotes =
                    term_fence.map_or(fence.quote_columns.len(), |host| host.quote_depth);
                term_fence_columns = term_fence
                    .map(|host| host.quote_columns.clone())
                    .unwrap_or(fence.quote_columns);
                description_fence = stripped.description && term_fence.is_none();
                description_blank = false;
                term_fence_floor = if description_fence {
                    0
                } else {
                    term_fence.map_or(fence.host_content_col, |host| host.content_col)
                };
                term_fence_owned = term_fence.is_some() || !term_fence_columns.is_empty();
                fence_closer_ahead = fence.closer_ahead;
            }
            body.push(lines[i].to_string());
            body_line_map.push(Some(first_source_line + i));
            i += 1;
            continue;
        }
        let def_indent = indent_columns(stripped.bare);
        let def_line =
            if nested && parse_footnote_def_line(trim_ascii_start(stripped.bare)).is_some() {
                // A nested note-body definition at any indent (see `nested`).
                trim_ascii_start(stripped.bare)
            } else {
                at_content_column(
                    stripped.bare,
                    stripped.structural,
                    columns.reached_by(def_indent),
                )
            };
        let authored_quote_def = !nested
            && recent_list_marker[i]
            && strip_blockquote_prefix(trim_ascii_start(def_line))
                .is_some_and(|content| parse_footnote_def_line(content).is_some());
        let def_line = if authored_quote_def {
            without_blockquote_prefixes(trim_ascii_start(def_line))
        } else {
            def_line
        };
        // PART 9R R1a: a line that folds into an open paragraph defines nothing,
        // and a probe that cannot afford to say so collects nothing either. The
        // two are one condition here - the fallback is conservative, so it may
        // decline a definition an affordable probe would have taken.
        let mut unaffordable = false;
        // THE PROBE STAYS INSIDE THE FILTER, which only runs on a line that
        // PARSED as a definition. Hoisting it out spent the budget on every
        // ordinary `- item` too, and the budget is shared: four hundred of them
        // ahead of a real definition exhausted it and the definition was
        // declined.
        if let Some((label, first)) = parse_footnote_def_line(def_line).filter(|_| {
            let quoted_note_paragraph = nested
                && stripped.structural.contains('>')
                && body.last().is_some_and(|previous: &String| {
                    strip_blockquote_prefix(trim_ascii_start(previous)).is_some_and(|content| {
                        !is_blank_line(content) && !item_block_opener(trim_ascii_start(content))
                    })
                });
            let folds = marker_line_may_be_lazy(lines[i])
                && line_folds_into_an_open_paragraph(
                    &body,
                    lines[i],
                    options,
                    &mut probe_budget,
                    &mut unaffordable,
                );
            !(folds
                || quoted_note_paragraph
                || unaffordable
                || folded_into_term.folded.contains(&i))
        }) {
            let normalized_label = label_key(label);
            let first_for_label = !definition_keys.contains_key(&normalized_label);
            let def_start_line = first_source_line + i;
            // The definition's OWN extent, before the body is collected.
            //
            // PART 12 §4: a span begins at the markup that opens the construct,
            // which for a definition is the `[` of `[^label]:` and not the
            // container prefix that carried the line. `def_line` is that text,
            // so what precedes it on the raw line is exactly the prefix to skip -
            // measured by taking `def_line` off the end rather than by re-deriving
            // a column, so a tab or a quote marker cannot be counted differently
            // here than it was when the line was stripped.
            let raw_def_line = lines[i];
            if positions && first_for_label {
                if let Some(prefix) = raw_def_line.strip_suffix(def_line) {
                    // First definition for a label wins, matching `defs` below.
                    def_positions
                        .entry(label.to_string())
                        .or_insert_with(|| Pos {
                            // `def_start_line` is already the 1-based source
                            // line - `body_start_line` starts the count at 1 -
                            // and it is the same number `def_line_map` records
                            // for the body's first line.
                            start_line: def_start_line,
                            end_line: def_start_line,
                            start_column: prefix.chars().count() + 1,
                            end_column: raw_def_line.chars().count() + 1,
                            start_offset: 0,
                            end_offset: 0,
                            file: None,
                        });
                }
            }
            i += 1;
            let mut def_lines = vec![first.to_string()];
            let mut def_line_map = vec![Some(def_start_line)];
            let mut def_col_map = if positions {
                vec![stripped_col(
                    Some(stripped.structural.chars().count() as isize),
                    stripped.bare,
                    first,
                )]
            } else {
                Vec::new()
            };
            let body_indent = footnote_body_floor(lines[def_start_line - first_source_line]);
            let container = IndentContainer {
                content_column: body_indent,
                owner: continuation_marker::Kind::Footnote,
            };
            let mut note_fence: Option<FenceOpen> = None;
            // The body column of the note a NESTED definition opened inside this
            // one, live only while that note is the block a line lands in. See
            // its use below.
            // Is this note's body still open for an INDENTED definition below
            // it? A flush line opens it and a block opener closes it. Only
            // whether, not where: a footnote definition registers wherever it
            // lands and is hoisted either way, so no column bound is enforced.
            let mut body_open_below = false;
            let mut open_term = term_start_column(first).is_some();
            let mut retained_below_comment = false;
            let mut below_comment_span = None;
            // A lone marker-line `+` gives the note its first flush-left block.
            // The same block bound is used by a later standalone `+` in a note.
            if trim_ascii(first) == "+"
                && !in_container
                && container.owns_continuation(first, decision_at(def_start_line))
                && lines.get(i).is_some_and(|line| !is_blank_line(line))
            {
                let end = attached_block_lines_classified(
                    CursorLines::Slice(&lines),
                    i,
                    &mut comment_closers,
                    &mut closer_index,
                    options,
                    &mut |line, _| is_blank_line(line) || is_plus_marker(line),
                );
                if end > i {
                    def_lines[0].clear();
                    def_line_map[0] = None;
                    if positions {
                        def_col_map[0] = None;
                    }
                    while i < end {
                        def_lines.push(lines[i].to_string());
                        def_line_map.push(Some(first_source_line + i));
                        if positions {
                            def_col_map.push(Some(0));
                        }
                        i += 1;
                    }
                    def_lines.push(String::new());
                    def_line_map.push(None);
                    if positions {
                        def_col_map.push(None);
                    }
                }
            }
            // An authored quote may be deeper than the item's content column.
            // Its prefix is still a container for note-body lines.
            if !in_container && !authored_quote_def {
                while i < lines.len() {
                    let line = lines[i];
                    let column = indent_columns(line);
                    if retained_below_comment {
                        if let Some(open) = note_fence {
                            if column < body_indent
                                && !is_blank_line(line)
                                && !is_fence_close(trim_ascii_start(line), open)
                                && !lines[i..]
                                    .iter()
                                    .any(|next| is_fence_close(trim_ascii_start(next), open))
                            {
                                break;
                            }
                            def_lines.push(line.to_owned());
                            def_line_map.push(Some(first_source_line + i));
                            if positions {
                                def_col_map.push(Some(0));
                            }
                            if is_fence_close(trim_ascii_start(line), open) {
                                note_fence = None;
                                retained_below_comment = false;
                            }
                            i += 1;
                            continue;
                        }
                    }
                    let below_comment = column < body_indent
                        && trim_ascii_start(line).starts_with("%%")
                        && !is_definition_placeholder(line);
                    if below_comment
                        && column == 0
                        && below_comment_span.is_none()
                        && detect_comment_fence_line_any_column(line).is_some_and(|open| {
                            comment_fence_closers
                                .get(&open.fence_len)
                                .is_some_and(|&last| last > i)
                        })
                    {
                        break;
                    }
                    let empty_fence =
                        detect_comment_fence_line_any_column(line).is_some_and(|open| {
                            lines.get(i + 1).is_some_and(|next| {
                                is_comment_fence_close_any_column(next, open.fence_len)
                            })
                        });
                    let after_comment = i + 1 + usize::from(empty_fence);
                    if below_comment
                        && column == 0
                        && !open_term
                        && (!nested || def_lines.len() == 1)
                        && (is_line_comment_any_column(line) || empty_fence)
                        && body.iter().all(|line| is_blank_line(line))
                        && lines.get(after_comment).is_some_and(|next| {
                            let follower_column = indent_columns(next);
                            (follower_column == 0 || follower_column >= body_indent)
                                && detect_fence_open(trim_ascii_start(next)).is_none()
                        })
                        && source.match_indices(&format!("[^{label}]")).count() <= 1
                    {
                        break;
                    }
                    if below_comment_span.is_some() || (below_comment && note_fence.is_none()) {
                        if below_comment_span.is_none()
                            && !retained_below_comment
                            && ((parse_footnote_def_line(first).is_some()
                                && column == 0
                                && def_lines.len() == 1)
                                || (!open_term
                                    && !body_ends_with_open_paragraph(
                                        &def_lines.join("\n"),
                                        options,
                                    )))
                        {
                            break;
                        }
                        let matched_span_line = below_comment_span.is_some();
                        if let Some(width) = below_comment_span {
                            if is_comment_fence_close_any_column(line, width) {
                                below_comment_span = None;
                            }
                        } else if let Some(open) = detect_comment_fence_line_any_column(line) {
                            if comment_fence_closers
                                .get(&open.fence_len)
                                .is_some_and(|&last| last > i)
                            {
                                below_comment_span = Some(open.fence_len);
                            }
                        }
                        let matched_span_line = matched_span_line || below_comment_span.is_some();
                        let kept = if trim_ascii_start(line).starts_with("%%")
                            && !open_term
                            && !matched_span_line
                        {
                            trim_ascii_start(line).to_owned()
                        } else {
                            line.to_owned()
                        };
                        def_line_map.push(Some(first_source_line + i));
                        if positions {
                            def_col_map.push(stripped_col(Some(0), line, &kept));
                        }
                        def_lines.push(kept);
                        retained_below_comment = true;
                        i += 1;
                        continue;
                    }
                    if retained_below_comment
                        && column < body_indent
                        && !is_blank_line(line)
                        // A matched fence interrupts at this column. Keeping it
                        // with an unreferenced note removes its visible block.
                        && !detect_fence_open(trim_ascii_start(line)).is_some_and(|open| {
                            #[cfg(test)]
                            FOOTNOTE_FENCE_CLOSER_LOOKUPS
                                .with(|count| count.set(count.get() + 1));
                            closer_maxima.closes_after(i, open)
                        })
                        && (column > 0
                            || !item_block_opener(trim_ascii_start(line))
                            || (detect_fence_open(trim_ascii_start(line)).is_some()
                                && (def_lines.last().is_some_and(|last| {
                                    is_line_comment_any_column(last)
                                        || detect_comment_fence_line_any_column(last).is_some_and(
                                            |open| {
                                                !lines[i..].iter().any(|next| {
                                                    is_comment_fence_close_any_column(
                                                        next,
                                                        open.fence_len,
                                                    )
                                                })
                                            },
                                        )
                                }) || ends_with_empty_comment_fence(&def_lines))))
                    {
                        let kept = trim_ascii_start(line).to_owned();
                        if def_lines.last().is_some_and(|last| {
                            is_line_comment_any_column(last)
                                || detect_comment_fence_line_any_column(last).is_some_and(|open| {
                                    !lines[i..].iter().any(|next| {
                                        is_comment_fence_close_any_column(next, open.fence_len)
                                    })
                                })
                        }) || ends_with_empty_comment_fence(&def_lines)
                        {
                            if let Some(mut open) = detect_fence_open(&kept) {
                                open.content_col = 0;
                                note_fence = Some(open);
                            }
                        }
                        def_line_map.push(Some(first_source_line + i));
                        if positions {
                            def_col_map.push(stripped_col(Some(0), line, &kept));
                        }
                        def_lines.push(kept);
                        if note_fence.is_none() {
                            retained_below_comment = false;
                        }
                        i += 1;
                        continue;
                    }
                    if is_blank_line(line) || column >= body_indent {
                        retained_below_comment = false;
                    }
                    if parse_footnote_def_line(line).is_some() {
                        break;
                    }
                    if is_blank_line(line) {
                        if let Some(open) = note_fence {
                            let (residue, consumed, synthetic) =
                                slice_columns_mapped(line, open.content_col, true);
                            def_lines.push(residue);
                            def_line_map.push(Some(first_source_line + i));
                            if positions {
                                def_col_map.push(Some(consumed as isize - synthetic as isize));
                            }
                            i += 1;
                            continue;
                        }
                        let after_run = container.blank_run_end(CursorLines::Slice(&lines), i + 1);
                        if after_run < lines.len()
                            && match container.classify(
                                lines[after_run],
                                decision_at(first_source_line + after_run),
                            ) {
                                ContainerLine::Content => true,
                                ContainerLine::OwnedContinuation => {
                                    is_plus_marker(lines[after_run])
                                }
                                _ => false,
                            }
                        {
                            while i < after_run {
                                let (residue, consumed, synthetic) =
                                    slice_columns_mapped(lines[i], body_indent, true);
                                def_lines.push(residue);
                                def_line_map.push(Some(first_source_line + i));
                                if positions {
                                    def_col_map.push(Some(consumed as isize - synthetic as isize));
                                }
                                i += 1;
                            }
                            continue;
                        }
                        break;
                    }
                    // Form B: a lone `+` attaches the following flush-left block
                    // to the note with no indentation (the same continuation
                    // marker lists, block quotes and definition bodies use); the
                    // attached block ends at a blank line, another `+`, or the
                    // next footnote definition.
                    if is_plus_marker(trim_ascii_start(line))
                        && column < body_indent
                        && container.owns_continuation(line, decision_at(first_source_line + i))
                    {
                        i += 1;
                        let mut attached: Vec<String> = Vec::new();
                        let attached_start = i;
                        let end = attached_block_lines_classified(
                            CursorLines::Slice(&lines),
                            i,
                            &mut comment_closers,
                            &mut closer_index,
                            options,
                            &mut |a, _| {
                                is_blank_line(a)
                                    || is_plus_marker(a)
                                    || parse_footnote_def_line(a).is_some()
                            },
                        );
                        while i < end {
                            let a = lines[i];
                            attached.push(slice_columns(a, column, true));
                            i += 1;
                        }
                        if !attached.is_empty() {
                            def_lines.push(String::new());
                            def_line_map.push(None);
                            if positions {
                                def_col_map.push(None);
                            }
                            let attached_len = attached.len();
                            def_lines.extend(attached);
                            def_line_map.extend(
                                (attached_start..attached_start + attached_len)
                                    .map(|line_idx| Some(first_source_line + line_idx)),
                            );
                            if positions {
                                def_col_map
                                    .extend((0..attached_len).map(|_| Some(column as isize)));
                            }
                        }
                        continue;
                    }
                    // COLUMNS, not a byte count. §16 asks for >= 2 columns and
                    // §24 C1 gives a tab a column value, so a bare tab reaches
                    // the body's column exactly as two spaces do. Counting
                    // bytes made this half disagree with the dedent below, which
                    // has always used `strip_leading_columns`: a bare tab
                    // counted as one and was refused (carve#796).
                    if indent_columns(line) >= body_indent {
                        // Strip EXACTLY the body's own indent, not all leading
                        // whitespace. A full trim flattened everything the
                        // relative indentation says: a nested list marker
                        // landed at its parent's column and the sublist became
                        // siblings, and a code block's interior indentation -
                        // which is content, not layout - was eaten with it
                        // (#611). Every other container strips its own prefix
                        // and leaves the rest, which is why the same two lines
                        // nest in a quote, a div and a list item.
                        let dedented = strip_leading_columns(line, body_indent);
                        let trimmed: &str = &dedented;
                        // A CODE FENCE inside the body is opaque, so a
                        // definition-shaped line in it is content. Every engine
                        // already agrees about that at the top level - none
                        // registers `[r]: /u` written inside ``` - and the
                        // top-level pass in `extract_link_defs` tracks fences
                        // for exactly this reason. Without the state here the
                        // line was consumed as a definition and vanished from
                        // the code block.
                        if let Some(open) = note_fence {
                            if is_fence_close(trimmed, open) {
                                note_fence = None;
                            }
                        } else if let Some(mut open) = detect_fence_open(trimmed) {
                            open.content_col = indent_columns(line);
                            note_fence = Some(open);
                        } else if let Some((label_part, target_part, mut def)) = (!open_term
                            || indent_columns(trimmed) == 0)
                            .then(|| parse_link_def_line_with_value(trim_ascii_start(trimmed)))
                            .flatten()
                        {
                            // A LINK DEFINITION inside the body is
                            // document-level metadata like any other, so it is
                            // collected and the line renders nothing - the same
                            // answer §16 already gives a definition inside a
                            // list item or a quote.
                            //
                            // LAST one wins among note bodies, which is what
                            // `extract_link_defs` does for the document; a
                            // top-level definition still beats both, and that
                            // precedence is applied by the caller.
                            if !label_part.starts_with(['@', '^']) && !target_part.trim().is_empty()
                            {
                                // The DOCUMENT line, so the hoisted node gets a
                                // `pos` like every other definition (§4, §10).
                                // `first_source_line` is 1-BASED - it is built as
                                // a newline count plus one - while `LinkDef.line`
                                // is the 0-based index `extract_link_defs`
                                // records, so the conversion is explicit here
                                // rather than off by one (carve-rs#636).
                                def.line = Some(first_source_line + i - 1);
                                def.raw_label = Some(label_part.to_string());
                                note_link_defs.insert(label_key(label_part), def);
                                // A BLOCK'S EXTENT ENDS AT A DEFINITION IT CAN
                                // ONLY TAKE AS TEXT (carve#1918). Dropping the
                                // line erased the column it stood at, so a
                                // nested note opened above kept its floor live
                                // and a trailing line further down reached a
                                // body that had already ended
                                // (markup-carve/carve#1971). The placeholder
                                // renders nothing and stands at the definition's
                                // own column, which is the signal the recursive
                                // pass reads.
                                def_lines.push(format!(
                                    "{}{}",
                                    " ".repeat(indent_columns(trimmed)),
                                    definition_placeholder()
                                ));
                                def_line_map.push(Some(first_source_line + i));
                                if positions {
                                    def_col_map.push(stripped_col(Some(0), line, trimmed));
                                }
                                i += 1;
                                continue;
                            }
                        }
                        // A NESTED NOTE DEFINITION IS AT THE BODY'S OWN COLUMN.
                        // carve#1918 puts a line STRICTLY BETWEEN two open content
                        // columns at the outer of them, and inside a note body the
                        // outer one is the body itself - which the dedent above has
                        // already moved to column 0. The line above absorbs that
                        // residual for a link definition; a footnote definition kept
                        // it, so the recursive pass saw an indented line, left it as
                        // text, and the note never registered (band 447 row 14).
                        //
                        // BETWEEN, not merely indented: a line AT OR PAST the nested
                        // note's own body column is that note's, and one under no
                        // nested note reaches nothing to be between. Absorbing there
                        // too hoisted a line every other engine keeps in the body.
                        let residual = indent_columns(trimmed);
                        let flush = trim_ascii_start(trimmed);
                        if residual == 0 || is_blank_line(trimmed) {
                            open_term = term_start_column(flush).is_some();
                        }
                        let is_note_def = parse_footnote_def_line(flush).is_some();
                        // A DEFINITION OPENS THE NESTED BODY; A BLOCK CLOSES IT.
                        // Plain prose is that body's own lazy continuation and
                        // leaves it open, so the flag survives it - closing on
                        // every non-definition line lost three documents the
                        // oracle registers.
                        //
                        // AN OPAQUE PAYLOAD IS NOT ABSORBED, and the closer has
                        // to be read at ANY column to say so: `detect_fence_open`
                        // above only sees a flush fence, so an INDENTED one left
                        // the flag standing and the dedent ate a column of the
                        // code block.
                        //
                        // NO UPPER BOUND on the residual. A first version
                        // absorbed only strictly BELOW the nested note's own
                        // floor, reading an at-or-past line as that note's. A
                        // footnote definition registers wherever it lands and is
                        // hoisted either way, so the oracle takes it at every
                        // column - the bound only withheld 50 it accepts.
                        // A FENCE PAYLOAD IS OPAQUE, state and all. The flush
                        // arm below would otherwise re-open the body on a
                        // payload LINE inside the fence, and the absorb would
                        // then eat a column from a definition-shaped line of
                        // code. `note_fence` is updated above for this line, so
                        // it already covers the opener and the closer.
                        if !is_note_def && item_block_opener(flush) {
                            body_open_below = false;
                        } else if residual == 0 {
                            body_open_below = true;
                        }
                        // No `residual > 0` on the absorb: at column 0 `flush`
                        // IS `trimmed`, so the arm could never return anything
                        // different there.
                        //
                        // ONLY UP TO THE OPEN SIBLING'S BODY FLOOR. `body_open_below`
                        // is set exactly when a preceding nested note sits at this
                        // body's own column 0 (residual 0), so its body floor is two.
                        // A later note that reaches that floor (`residual >= 2`) is
                        // INSIDE the sibling, not a same-level sibling itself: leaving
                        // it flush hoisted it to this body's column 0, collapsing the
                        // two-level dedent into one so a trailing line below the inner
                        // note's own content column was still claimed by it instead of
                        // falling to the reachable ancestor (markup-carve/carve#1946,
                        // carve-php#1895). Keeping the residual nests it, matching the
                        // per-marker body column carve-js#1664 measures.
                        let trimmed: &str = if body_open_below
                            && is_note_def
                            && note_fence.is_none()
                            && residual < 2
                        {
                            flush
                        } else {
                            trimmed
                        };
                        def_lines.push(trimmed.to_string());
                        def_line_map.push(Some(first_source_line + i));
                        if positions {
                            def_col_map.push(stripped_col(Some(0), line, trimmed));
                        }
                        i += 1;
                        continue;
                    }
                    // A COMMENT DELIMITER CLOSING A SPAN THIS BODY ALREADY HOLDS
                    // IS NOT A COMMENT THAT ENDS THE BODY - see
                    // `body_open_comment_run`. This host hides the payload
                    // already at a delimiter REACHING the body's floor; the band
                    // below it is what this covers.
                    if detect_comment_fence_line_any_column(line).is_some()
                        && closes_a_held_comment_span(&def_lines, line)
                    {
                        let kept = kept_comment_delimiter(line);
                        if positions {
                            def_col_map.push(stripped_col(Some(0), line, &kept));
                        }
                        def_lines.push(kept);
                        def_line_map.push(Some(first_source_line + i));
                        i += 1;
                        continue;
                    }
                    break;
                }
            } else if !nested {
                // A BLOCK OPENER AT A DD-HOSTED NOTE'S OWN FLOOR REACHES THE NOTE
                // BODY (markup-carve/carve#1974). The per-marker note floor that
                // carve-rs#1575 applies to a note inside another note extends to
                // one inside a `dd`: the floor is the note MARKER'S column plus
                // two (§16), which inside a container is the stripped prefix's
                // width plus `footnote_body_floor` of the bare definition. An
                // line at or past that floor is the note's, so it is gathered here
                // and removed from the container's body - an unreferenced note
                // then drops with it.
                //
                // A BLOCK OPENER AND PLAIN CONTINUATION ARE THE SAME LINE HERE.
                // This arm first gathered only an opener, on the reading that
                // continuation at the same column was a separate settled
                // question. It is not one: corpus 359 pins the per-marker floor
                // for a container-hosted note with plain continuation text under
                // a LIST ITEM, and carve-js and carve-php both give the `dd` host
                // the same answer. Excluding text left this engine reading one
                // floor for openers and another for prose, which is the
                // two-answers-at-once state the fix above set out to end.
                // A quote at an item's or a description's content column
                // (`- > - para` / `  >   [^f]: n`) is a host level too; its
                // continuation lines repeat the same indentation before `>`.
                // The column scanner did not strip an overindented quote, so
                // use its authored prefix through the definition marker.
                let structural_len = if authored_quote_def {
                    raw_def_line.len() - def_line.len()
                } else {
                    stripped.structural.len()
                };
                let mut quote_indents = Vec::new();
                let mut inside = raw_def_line;
                // A marker in front of the quote (`- > [^f]: n`) stands for the
                // same number of spaces on the lines below it.
                loop {
                    let mut indent = leading_ws(inside);
                    if let Some(content) = detect_list_marker_full(inside)
                        .map(|marker| marker.content)
                        .or_else(|| {
                            strip_definition_marker(&inside[indent..]).map(|(body, _)| body)
                        })
                        .filter(|content| content.starts_with('>'))
                    {
                        indent = content.as_ptr() as usize - inside.as_ptr() as usize;
                    }
                    let consumed = raw_def_line.len() - inside.len();
                    if consumed + indent >= structural_len
                        || !inside.is_char_boundary(indent)
                        || inside[..indent].contains('\t')
                    {
                        break;
                    }
                    let Some(rest) = strip_blockquote_prefix(&inside[indent..]) else {
                        break;
                    };
                    quote_indents.push(indent);
                    inside = rest;
                }
                let quote_depth = quote_indents.len();
                let prefix_cols = inside.chars().count() - def_line.chars().count();
                let note_floor = prefix_cols + footnote_body_floor(def_line);
                // Measure indentation inside the note's existing quote host.
                // A further quote marker is note content and stays in the body.
                let content = |raw| strip_indented_quote_levels(raw, &quote_indents);
                if quote_depth > 0
                    && raw_def_line.starts_with('>')
                    && quote_indents.iter().all(|&indent| indent == 0)
                    && lines.get(i).is_some_and(|raw| raw.starts_with('>'))
                    && lines.get(i).and_then(|raw| content(raw)) == Some("+")
                    && decision_at(first_source_line + i).is_some_and(|decision| {
                        decision.owner == Some(continuation_marker::Kind::Footnote)
                            && decision.refusal.is_none()
                    })
                {
                    let start = i + 1;
                    let attached: Vec<&str> = lines[start..]
                        .iter()
                        .map_while(|raw| content(raw).filter(|line| !is_blank_line(line)))
                        .take_while(|line| *line != "+")
                        .collect();
                    if !attached.is_empty() {
                        let count =
                            attached_one_block_lines(CursorLines::Slice(&attached), options);
                        def_lines.push(String::new());
                        def_line_map.push(None);
                        if positions {
                            def_col_map.push(None);
                        }
                        for (offset, line) in attached.into_iter().take(count).enumerate() {
                            def_lines.push(line.to_string());
                            def_line_map.push(Some(first_source_line + start + offset));
                            if positions {
                                def_col_map.push(Some(
                                    (lines[start + offset].chars().count() - line.chars().count())
                                        as isize,
                                ));
                            }
                        }
                        i = start + count;
                    }
                }
                let marker_note = detect_list_marker_full(inside)
                    .is_some_and(|marker| parse_footnote_def_line(marker.content).is_some());
                let track_note_opacity = *body_opacity_candidates
                    .get_or_insert_with(|| source.contains("%%%") || source.contains("~~~"));
                let mut note_opacity = BodyCommentState::new();
                let mut note_admitted_fence: Option<(FenceOpen, usize)> = None;
                let comment_closes = |at, width, cache: &mut Option<ContainerCommentClosers>| {
                    let index = cache.get_or_insert_with(|| ContainerCommentClosers::build(&lines));
                    let floor = content(lines[at])
                        .map(|line| indent_columns(line).min(note_floor))
                        .unwrap_or(note_floor);
                    index.closes_in_note(&lines, at, quote_depth, floor, width)
                };
                // A matched fence at or past a marker-line note's body column
                // opens in the note. The closer may return to the body column
                // or use the opener's authored base (CARVE-P2-017, carve#3023).
                let closed_note_fence = |at: usize| {
                    let Some(line) = content(lines[at]) else {
                        return false;
                    };
                    let column = indent_columns(line);
                    let Some(mut open) = detect_fence_open(trim_ascii_start(line)) else {
                        return false;
                    };
                    open.quoted = quote_depth > 0;
                    [note_floor, column]
                        .into_iter()
                        .any(|closer_column| closer_maxima.closes_after_at(at, open, closer_column))
                };
                if track_note_opacity {
                    note_opacity.observe(first);
                }
                if note_opacity
                    .comment
                    .is_some_and(|width| !comment_closes(i - 1, width, &mut container_closers))
                {
                    note_opacity.comment = None;
                }
                // A marker note admits comments at its host column.
                let reaches_floor =
                    |at, admitted_fence: Option<(FenceOpen, usize)>, comment_open| {
                        content(lines[at]).is_some_and(|line| {
                            admitted_fence.is_some()
                                || comment_open
                                || indent_columns(line) >= note_floor
                                || (quote_depth > 0
                                    && indent_columns(line) > 0
                                    && trim_ascii_start(line).starts_with("%%"))
                                || (marker_note
                                    && indent_columns(line) >= prefix_cols
                                    && is_line_comment_any_column(line))
                        })
                    };
                let blank = |raw| content(raw).is_some_and(is_blank_line);
                let body_resumes = {
                    let mut at = i;
                    while at < lines.len() && blank(lines[at]) {
                        at += 1;
                    }
                    at < lines.len()
                        && reaches_floor(at, note_admitted_fence, note_opacity.comment.is_some())
                };
                if body_resumes {
                    while i < lines.len() {
                        let raw = lines[i];
                        let Some(line) = content(raw) else { break };
                        if is_blank_line(line) {
                            let mut after = i + 1;
                            while after < lines.len() && blank(lines[after]) {
                                after += 1;
                            }
                            if after < lines.len()
                                && reaches_floor(
                                    after,
                                    note_admitted_fence,
                                    note_opacity.comment.is_some(),
                                )
                            {
                                while i < after {
                                    let raw = lines[i];
                                    let line = content(raw).expect("checked quote prefix");
                                    let prefix = raw.chars().count() - line.chars().count();
                                    let (residue, consumed, synthetic) =
                                        slice_columns_mapped(line, note_floor, true);
                                    if track_note_opacity {
                                        note_opacity.observe(&residue);
                                    }
                                    def_lines.push(residue);
                                    def_line_map.push(Some(first_source_line + i));
                                    if positions {
                                        def_col_map.push(Some(
                                            (prefix + consumed) as isize - synthetic as isize,
                                        ));
                                    }
                                    i += 1;
                                }
                                continue;
                            }
                            break;
                        }
                        let line_reaches_floor =
                            reaches_floor(i, note_admitted_fence, note_opacity.comment.is_some());
                        if !line_reaches_floor {
                            break;
                        }
                        if marker_note && parse_footnote_def_line(trim_ascii_start(line)).is_some()
                        {
                            break;
                        }
                        let dedented = strip_leading_columns(line, note_floor);
                        // A link definition at the note's own column is metadata,
                        // even when an open marker-line paragraph precedes it.
                        if indent_columns(line) >= note_floor
                            && note_admitted_fence.is_none()
                            && note_opacity.comment.is_none()
                            && note_opacity.code.is_none()
                        {
                            if let Some((label_part, target_part, mut def)) =
                                parse_link_def_line_with_value(trim_ascii_start(&dedented))
                            {
                                if !label_part.starts_with(['@', '^'])
                                    && !target_part.trim().is_empty()
                                {
                                    def.line = Some(first_source_line + i - 1);
                                    def.raw_label = Some(label_part.to_string());
                                    note_link_defs.insert(label_key(label_part), def);
                                    def_lines.push(definition_placeholder());
                                    def_line_map.push(Some(first_source_line + i));
                                    if positions {
                                        let prefix = raw.chars().count() - line.chars().count();
                                        def_col_map.push(stripped_col(
                                            Some(prefix as isize),
                                            line,
                                            &dedented,
                                        ));
                                    }
                                    i += 1;
                                    continue;
                                }
                            }
                        }
                        if positions {
                            let prefix = raw.chars().count() - line.chars().count();
                            def_col_map.push(stripped_col(Some(prefix as isize), line, &dedented));
                        }
                        if note_opacity.comment.is_none() {
                            let column = indent_columns(line);
                            if note_admitted_fence.is_some_and(|(open, base)| {
                                (column == note_floor || column == base)
                                    && is_fence_close(trim_ascii_start(line), open)
                            }) {
                                note_admitted_fence = None;
                            } else if note_admitted_fence.is_none()
                                && column >= note_floor
                                && closed_note_fence(i)
                            {
                                if let Some(open) = detect_fence_open(trim_ascii_start(line)) {
                                    note_admitted_fence = Some((open, column));
                                }
                            }
                        }
                        let was_comment = note_opacity.comment.is_some();
                        if track_note_opacity {
                            note_opacity.observe(&dedented);
                        }
                        if !was_comment
                            && note_opacity.comment.is_some_and(|width| {
                                !comment_closes(i, width, &mut container_closers)
                            })
                        {
                            note_opacity.comment = None;
                        }
                        def_lines.push(dedented);
                        def_line_map.push(Some(first_source_line + i));
                        i += 1;
                    }
                }
            }
            // ONLY FOR THE DEFINITION THIS POSITION DESCRIBES. The first
            // definition for a label wins, both here and in `defs` below, so a
            // later duplicate must not move the accepted one's end: this wrote
            // the DUPLICATE's following line onto the FIRST definition's span,
            // and the container that hosted the first one was then reported as
            // running to it (carve-rs#1106). `defs` holds the label exactly
            // when an earlier definition was accepted, which is the test.
            if positions
                && !in_container
                && first_for_label
                && i < lines.len()
                && is_blank_line(lines[i])
            {
                if let Some(pos) = def_positions.get_mut(label) {
                    pos.end_line = first_source_line + i;
                    pos.end_column = 1;
                }
            }
            // First definition for a label wins (later duplicates are ignored).
            if first_for_label {
                definition_keys.insert(normalized_label, label.to_string());
                let mut definition_source = MappedSource {
                    col_map: def_col_map,
                    source: def_lines.join("\n"),
                    line_map: def_line_map,
                    authored_base_at_start: false,
                    item_marker_at_start: false,
                    marker_colon_rebased: false,
                    marker_tail: None,
                    reached: vec![true; def_lines.len()],
                    // A FOOTNOTE BODY carries authored base for its sublists, so
                    // a list marker past the body's content column anchors its
                    // span at the marker (markup-carve/carve#1980).
                    in_quoted_body: false,
                    sublists_carry_authored_base: true,
                };
                let absorbed_note_marker =
                    detect_list_marker_full(without_blockquote_prefixes(raw_def_line))
                        .is_some_and(|marker| parse_footnote_def_line(marker.content).is_some());
                if absorbed_note_marker
                    || parse_footnote_def_line(trim_ascii_start(first)).is_none()
                {
                    rebase_body_blocks(&mut definition_source, true, options, absorbed_note_marker);
                }
                defs.insert(label.to_string(), definition_source);
            }
            // Leave the container's structural prefix (or a blank line at top
            // level) where the invisible definition was, so the container still
            // renders and the line still acts as a block boundary -- a following
            // paragraph or a lazy blockquote continuation does not absorb across
            // it.
            // A definition matched through the CONTENT-COLUMN strip leaves the
            // container prefix behind, and inside a quoted list item that
            // prefix alone is a BLANK line - which loosens the list (§17 L1).
            // The definition rendered nothing, so it is not the item's second
            // block and must not loosen it (§17 L2), exactly as the
            // marker-consuming case above keeps the item non-empty. `%%` is
            // invisible at any column and closes nothing (§24 C3).
            let mut replacement = if authored_quote_def {
                raw_def_line
                    .strip_suffix(def_line)
                    .expect("quoted definition suffix")
                    .to_string()
            } else {
                stripped.replacement()
            };
            if !replacement.ends_with("%%") && !replacement.ends_with(&definition_placeholder()) {
                // AND IT HAS TO STAND AT THE DEFINITION'S OWN COLUMN. Inside a
                // container the structural prefix already carries it; at top
                // level that prefix is empty, so an unindented placeholder
                // lands at column 0 and CLOSES the item it exists to keep
                // open - the line after it leaves the list entirely.
                let document_column =
                    definition_at_quote_document_column(&replacement, stripped.bare);
                if !authored_quote_def {
                    replacement.push_str(&stripped.bare[..leading_ws(stripped.bare)]);
                }
                replacement.push_str(&if document_column {
                    document_definition_placeholder()
                } else {
                    definition_placeholder()
                });
            }
            replacement.push_str(
                &" ".repeat(
                    raw_def_line
                        .chars()
                        .count()
                        .saturating_sub(replacement.chars().count()),
                ),
            );
            body.push(replacement);
            body_line_map.push(Some(def_start_line));
        } else {
            body.push(lines[i].to_string());
            body_line_map.push(Some(first_source_line + i));
            i += 1;
        }
    }
    (
        MappedSource {
            // The document body's lines are top-level: the footnote-definition
            // extraction removes whole lines, never a prefix, so nothing has
            // been stripped from the front of the ones that remain.
            col_map: if positions {
                vec![Some(0); body.len()]
            } else {
                Vec::new()
            },
            source: joined_source(&body),
            authored_base_at_start: false,
            item_marker_at_start: false,
            marker_colon_rebased: false,
            marker_tail: None,
            reached: Vec::new(),
            in_quoted_body: false,
            sublists_carry_authored_base: false,
            line_map: body_line_map,
        },
        defs,
        def_positions,
        note_link_defs,
    )
}

pub(in crate::parse) fn parse_footnote_def_line(line: &str) -> Option<(&str, &str)> {
    let rest = line.strip_prefix("[^")?;
    let (label, body) = rest.split_once("]: ")?;
    if label.is_empty() || label.contains(']') {
        return None;
    }
    // No content after the separator is not a definition either. `[^a]: ` was
    // consumed with an empty body and vanished; carve-js and carve-php both
    // keep it as `<p>[^a]:</p>`.
    let body = trim_ascii_start(body);
    if body.is_empty() {
        return None;
    }
    Some((label, body))
}
