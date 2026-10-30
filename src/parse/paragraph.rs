use super::*;

pub(super) fn detect_block_image(line: &str) -> Option<Image> {
    if !line.starts_with("![") {
        return None;
    }
    let bytes = line.as_bytes();
    let (bracket_matches, bracket_run_at) = compute_bracket_matches(bytes);
    // Block-image detection runs once on a single line (not in a per-position
    // loop), so full-slice last-occurrence scans are fine here.
    let bracket_openers = compute_bracket_openers(&bracket_matches);
    let bounds = InlineBounds {
        matches: &bracket_matches,
        openers: &bracket_openers,
        run_at: &bracket_run_at,
        last_close_paren: bytes.iter().rposition(|&b| b == b')'),
        last_close_brace: bytes.iter().rposition(|&b| b == b'}'),
        last_close_bracket: bytes.iter().rposition(|&b| b == b']'),
        last_gt: bytes.iter().rposition(|&b| b == b'>'),
        crossref_invalid_range: Cell::new(None),
        delim_brace: [None; DELIM_BRACE_SLOTS],
        braced_closers: RefCell::new(None),
        native_pairs: None,
        footnote_refs: true,
    };
    let (img, consumed) = parse_image_at(bytes, 0, &bounds)?;
    let after = &line[consumed..];
    if !trim_ascii(after).is_empty() {
        return None;
    }
    Some(img)
}

fn reference_image_line(line: &str) -> bool {
    if !line.starts_with("![") {
        return false;
    }
    let bytes = line.as_bytes();
    let (bracket_matches, bracket_run_at) = compute_bracket_matches(bytes);
    let bracket_openers = compute_bracket_openers(&bracket_matches);
    let bounds = InlineBounds {
        matches: &bracket_matches,
        openers: &bracket_openers,
        run_at: &bracket_run_at,
        last_close_paren: bytes.iter().rposition(|&b| b == b')'),
        last_close_brace: bytes.iter().rposition(|&b| b == b'}'),
        last_close_bracket: bytes.iter().rposition(|&b| b == b']'),
        last_gt: bytes.iter().rposition(|&b| b == b'>'),
        crossref_invalid_range: Cell::new(None),
        delim_brace: [None; DELIM_BRACE_SLOTS],
        braced_closers: RefCell::new(None),
        native_pairs: None,
        footnote_refs: true,
    };
    parse_reference_image(bytes, 0, &bounds)
        .is_some_and(|(_, consumed)| trim_ascii(&line[consumed..]).is_empty())
}

pub(super) fn parse_paragraph(cur: &mut LineCursor, options: &Options<'_>) -> BlockNode {
    let span_start = cur.pos;
    let refused_plus = cur
        .peek()
        .is_some_and(|line| is_plus_marker(trim_ascii_start(line)))
        && cur
            .continuation_decision(span_start)
            .is_some_and(|decision| {
                decision.refusal.is_some()
                    && decision.owner != Some(continuation_marker::Kind::Footnote)
                    && !decision.after_quote_blank
            });
    // A caption at the content column can attach to an indented image-only
    // paragraph. An indented caption remains literal paragraph text.
    let image_lead = cur.peek().is_some_and(|line| {
        let content = trim_ascii_start(line);
        detect_block_image(content).is_some() || reference_image_line(content)
    });
    let image_opener = cur
        .peek()
        .is_some_and(|line| trim_ascii_start(line).starts_with("!["));
    let mut at_content_column = cur.peek().is_some_and(|l| !l.starts_with([' ', '\t']));
    let reference_caption_candidate = cur.peek().is_some_and(reference_image_line);
    let mut reference_caption_claimed = false;
    let mut lines: Vec<&str> = Vec::new();
    let mut line_indices = Vec::new();
    let mut suppress_colon_interrupt = false;
    // PART 10 I2: a code include line at a block start ends at its own line.
    let code_include = cur
        .peek()
        .and_then(|line| crate::includes::code_include_line(strip_lazy(trim_ascii_start(line))));
    while let Some(line) = cur.peek() {
        if code_include.is_some() && !lines.is_empty() {
            break;
        }
        if is_blank_line(line) {
            if image_lead
                && lines.len() == 1
                && cur
                    .lines
                    .line(cur.pos + 1)
                    .is_some_and(|next| caption_content(next).is_some())
            {
                cur.consume();
                continue;
            }
            break;
        }
        // First line is always part of the paragraph; from the second on, a
        // visible block opener interrupts (§10).
        let line_owned = line.to_string();
        let same_authored_column = cur
            .source_line(span_start)
            .zip(cur.source_line(cur.pos))
            .map(|(first, next)| {
                continuation_marker::document_column(first)
                    == continuation_marker::document_column(next)
            })
            .unwrap_or(true);
        if !lines.is_empty()
            && ((reference_caption_claimed && caption_content(line).is_some())
                || (!refused_plus || !is_list_marker(&line_owned) || same_authored_column)
                    && interrupts_paragraph(cur, &line_owned)
                || (refused_plus
                    && lines.len() == 1
                    && cur.lines.at(span_start).starts_with('+')
                    && cur
                        .continuation_decision(span_start)
                        .is_some_and(|decision| {
                            (decision.owner.is_some()
                                || cur.source_line(span_start).is_some_and(|source_line| {
                                    continuation_marker::preceding_nested_list_marker(
                                        source_line,
                                        decision.quote_depth == 0,
                                    )
                                }))
                                && decision.sibling_interrupt
                        })
                    && is_list_marker(line)
                    && indent_columns(line) == 0
                    && same_authored_column)
                || (cur.colon_view.is_some_and(|view| view.marker_hosted)
                    && strip_blockquote_prefix(trim_ascii_start(line)).is_some()))
            && !(suppress_colon_interrupt && is_suppressed_colon_fence_line(&line_owned))
        {
            break;
        }
        report_unclaimed_document_plus(cur, line);
        if image_opener
            && !lines.is_empty()
            && caption_content(line).is_some()
            && !line.starts_with([' ', '\t'])
        {
            at_content_column = true;
        }
        cur.consume();
        let trimmed = trim_ascii_end(trim_ascii_start(line));
        suppress_colon_interrupt |= is_invalid_colon_fence_opener_text(trimmed);
        reference_caption_claimed |=
            reference_caption_candidate && lines.len() == 1 && caption_content(line).is_some();
        line_indices.push(cur.pos - 1);
        lines.push(trimmed);
    }
    // A paragraph never carries its OWN trailing attribute block: a standalone
    // `{...}` line floats forward (handled via interrupts_paragraph + the
    // pending-attrs loop), and a trailing same-line `{...}` with no abutting
    // host stays literal inline content (§14). Paragraph attributes come only
    // from a preceding block-attribute line (§15), applied by the caller.
    // Every line arrives already stripped at both ends (see the loop above), so
    // the join needs no further trimming. A HARD BREAK is unaffected: it is a
    // trailing BACKSLASH, which is content and not whitespace, so the line does
    // not end in whitespace at all and nothing is dropped in front of it. Carve
    // has no two-space hard break to lose - `a<SP><SP>` + newline + `b` is one
    // paragraph with a soft break in the executable spec.
    let joined = lines
        .iter()
        .map(|l| strip_lazy(l))
        .collect::<Vec<_>>()
        .join("\n");
    let at_content_column = at_content_column
        && !lines
            .iter()
            .skip(1)
            .any(|line| line.starts_with(LAZY) && caption_content(strip_lazy(line)).is_some());
    let joined = joined.as_str();
    let children = if options.positions {
        let anchors = lines
            .iter()
            .zip(&line_indices)
            .map(|(line, &index)| inline_anchor_for_line(cur, index, line))
            .collect();
        parse_inline_lines_with_anchor(joined, options, anchors)
    } else {
        parse_inline_with_options(joined, options)
    };
    // Unexpanded, it is literal text at document level; nested, its source is inline content.
    let children = if code_include == Some(false) && cur.at_document_level {
        let pos = children
            .first()
            .and_then(InlineNode::pos)
            .zip(children.last().and_then(InlineNode::pos))
            .map(|(first, last)| Pos {
                end_line: last.end_line,
                end_column: last.end_column,
                end_offset: last.end_offset,
                ..first.clone()
            });
        vec![InlineNode::Text(Text {
            value: joined.to_string(),
            pos,
        })]
    } else {
        children
    };
    let pos = span_of(cur, span_start, cur.pos, options);
    BlockNode::Paragraph(Paragraph {
        attrs: None,
        children,
        at_content_column,
        // The promotion phase recomputes this after resolution; a paragraph
        // cannot know here whether its reference image resolves.
        block_image: false,
        pos,
    })
}

/// `q` / `+` / `- s`: no open container claims a document-level `+`, which lint
/// reports when a block follows it and not before paragraph text (carve#3421).
fn report_unclaimed_document_plus(cur: &mut LineCursor<'_>, line: &str) {
    if !cur.at_document_level
        || cur.colon_view.is_some()
        || !line.starts_with('+')
        || !line[1..].bytes().all(|b| matches!(b, b' ' | b'\t'))
    {
        return;
    }
    let Some(follower) = cur.lines.line(cur.pos + 1) else {
        return;
    };
    let Some(source_line) = cur.source_line(cur.pos) else {
        return;
    };
    let block = !follower.starts_with([' ', '\t'])
        && (detect_list_marker_full(follower).is_some()
            || !follower.starts_with('%') && {
                cur.pos += 1;
                let interrupts = interrupts_paragraph_in_band(cur, follower, false);
                cur.pos -= 1;
                interrupts
            });
    if block {
        note_lint_event(LintEvent::Marker {
            line: source_line,
            column: 1,
            host: "document",
            reason: "unclaimed",
            first_block: false,
        });
    }
}

/// Whether `line`, seen while accumulating a paragraph, ends it and starts a
/// new block (grammar §10, post-Markdown default).
///
/// A VISIBLE block interrupts an open paragraph with no blank line, at the top
/// level and nested: heading, thematic break, block quote, bullet/task list, a
/// valid table row, a fenced code block (closer or not, carve#4018), and
/// a valid flush-left colon-fence block. INVISIBLE constructs
/// (comments, abbreviation definitions) interrupt too. ORDERED lists do NOT
/// interrupt, `+` is the continuation marker not a bullet, and a bare image
/// stays inline.
pub(super) fn interrupts_paragraph(cur: &mut LineCursor<'_>, line: &str) -> bool {
    // §10 (post-Markdown default): a VISIBLE block interrupts an open paragraph
    // with no blank line. Invisible constructs (comments, abbreviation defs)
    // interrupt too. Ordered lists do NOT interrupt, `+` is the continuation
    // marker not a bullet, and a bare image stays inline.
    if trim_ascii_start(line).starts_with("%%")
        || (cur.at_document_level && detect_abbreviation_def(line).is_some())
    {
        return true;
    }
    if cur.invisible_arms && !line.starts_with([' ', '\t']) {
        if parse_standalone_attrs(line).is_some() {
            return true;
        }
        if cur.lines.line(cur.pos) == Some(line)
            && standalone_attrs_block_len_iter(
                (cur.pos..cur.lines.len()).map_while(|i| cur.lines.line(i)),
            )
            .is_some()
        {
            return true;
        }
    }
    // A MARKER AT A LIST ITEM'S CONTENT COLUMN OPENS A SUBLIST, first in the
    // item or not (§24 C3, markup-carve/carve#1517). These lines are the item's
    // body, so their column 0 IS the content column: "AT content_column ... a
    // list marker opens a sublist", "whether or not a blank line precedes the
    // child". §10 I2 defers to it by name rather than competing with it.
    //
    // COLUMN 0 EXACTLY, and the leading-whitespace guard is what says so: §24
    // C3's other band is explicit that BELOW the content column "a list marker
    // folds as lazy item text", and the collector forwards such a line with its
    // residual indent intact. `is_list_marker` trims, so without the guard the
    // two bands would answer the same.
    if cur.in_item_body
        && cur.carried_reach(cur.pos) != Some(false)
        && !line.starts_with([' ', '\t'])
        && is_list_marker(line)
    {
        return true;
    }
    // Symmetric §10: a list marker (bullet OR task OR ordered) does NOT
    // interrupt a paragraph -- a list needs a blank line before it. Only the
    // other visible blocks interrupt.
    if detect_heading(line).is_some()
        || detect_thematic_break(line)
        || strip_blockquote_prefix(line).is_some()
        // A definition-list term `:: ` is a first-class block opener (§24 C3):
        // it interrupts at column 0 and nests at the content column, uniform
        // with quote/heading/fence/table. `is_definition_list_start` requires a
        // flush-left `:: `, so an indented term folds as lazy text like the rest.
        || is_definition_list_start(line)
        // A table row interrupts only when FLUSH-LEFT, like the quote/heading
        // checks above -- `is_table_start` trims leading whitespace, so without
        // this guard an INDENTED row (`  |a|`) would interrupt where an indented
        // `> q` / `# h` does not. An indented row below/above a list item's
        // content column is lazy paragraph text (§24 C3), not a nested table.
        || (!line.starts_with([' ', '\t']) && is_table_start(line))
    {
        return true;
    }
    // A fence opener interrupts closer or not (carve#4018).
    if detect_fence_open(line).is_some() {
        return true;
    }
    if is_colon_fence_opener_shape(line) {
        return true;
    }
    false
}

pub(super) fn interrupts_lazy_continuation(cur: &mut LineCursor<'_>, line: &str) -> bool {
    let saved = cur.in_item_body;
    cur.in_item_body = false;
    let interrupts = interrupts_paragraph(cur, line);
    cur.in_item_body = saved;
    interrupts || is_colon_fence_opener_shape(line)
}

pub(super) fn interrupts_paragraph_as_container(cur: &mut LineCursor<'_>, line: &str) -> bool {
    let saved = cur.at_document_level;
    cur.at_document_level = false;
    let interrupts = interrupts_paragraph(cur, line);
    cur.at_document_level = saved;
    interrupts
}

/// `interrupts_paragraph` for a container's BELOW-COLUMN band.
///
/// Always as a container, so the abbreviation arm is waived (§7 recognizes that
/// definition only as a direct child of the document). `at_document_column`
/// additionally selects the band: at a NONZERO column below the content column
/// the block-attribute line is lazy paragraph text of that container too (§10 I5,
/// markup-carve/carve#1809), while at column 0 it is a floating attribute that
/// attaches forward.
///
/// The COMMENT is deliberately not waived: it is column-exempt (PART 9 §24) and
/// renders nothing at any column, which corpus 430-5 pins - waiving it would put
/// its characters on the page.
pub(super) fn interrupts_paragraph_in_band(
    cur: &mut LineCursor<'_>,
    line: &str,
    at_document_column: bool,
) -> bool {
    let saved_level = cur.at_document_level;
    let saved_arms = cur.invisible_arms;
    cur.at_document_level = false;
    cur.invisible_arms = at_document_column;
    let interrupts = interrupts_paragraph(cur, line);
    cur.at_document_level = saved_level;
    cur.invisible_arms = saved_arms;
    interrupts
}

pub(super) fn interrupts_lazy_continuation_as_container(
    cur: &mut LineCursor<'_>,
    line: &str,
) -> bool {
    let saved = cur.at_document_level;
    cur.at_document_level = false;
    let interrupts = interrupts_lazy_continuation(cur, line);
    cur.at_document_level = saved;
    interrupts
}

pub(super) fn is_colon_fence_opener_shape(line: &str) -> bool {
    // Only a FLUSH-LEFT colon fence ends lazy continuation (grammar PART 9
    // §10). An INDENTED colon-shaped line (the detectors trim leading
    // whitespace) is still within the container, so it folds as lazy text
    // instead of escaping the container.
    if line.starts_with(' ') || line.starts_with('\t') {
        return false;
    }
    detect_container_open(line).is_some()
        || detect_line_block_open(line).is_some()
        || detect_hardbreaks_block_open(line).is_some()
        || detect_quote_block_open(line).is_some()
}

/// Whether a glued colon fence earlier in the paragraph (`:::note`, `:::]`)
/// keeps THIS line from interrupting it.
///
/// Only a BARE fence (`:::`, nothing after the colons) is held back: it is
/// closer-shaped, and the paragraph it would close is literal text, so it stays
/// literal text too. A real opener - a type word, `::: |`, `::: \` - opens its
/// block as usual, glued predecessor or not (carve-rs#496). Matches carve-js
/// (`RE_ADMONITION_CLOSE` + `isLiteralColonFenceLine`) and carve-php.
pub(super) fn is_suppressed_colon_fence_line(line: &str) -> bool {
    is_colon_fence_opener_shape(line) && exact_colon_fence_len(line).is_some()
}

pub(super) fn interrupts_paragraph_with_rest(line: &str, rest: &[&str]) -> bool {
    if trim_ascii_start(line).starts_with("%%") {
        return true;
    }
    // Flush-left only (see interrupts_paragraph): an indented `{...}` line is
    // lazy paragraph text under the strict column-0 rule, not a floating attr.
    //
    // The WRAPPED spelling interrupts too, which is what `rest` is for. Only the
    // single-line form was tested here, so `{.k` opened nothing and folded into
    // the paragraph as literal text - the author's braces reached the page and
    // the attributes reached nothing (markup-carve/carve-rs#1039, and the half
    // of markup-carve/carve#1281's `329-...-6` that the container's end does not
    // answer on its own).
    //
    // The brace test comes FIRST so the common line costs nothing: this
    // predicate is asked of every blank-separated block, and copying `rest`
    // for a heading or a paragraph line would make that quadratic.
    if !line.starts_with([' ', '\t'])
        && trim_ascii(line).starts_with('{')
        && standalone_attrs_block_len_iter(std::iter::once(line).chain(rest.iter().copied()))
            .is_some()
    {
        return true;
    }
    if detect_heading(line).is_some()
        || detect_thematic_break(line)
        || strip_blockquote_prefix(line).is_some()
        // A definition-list term `:: ` is a first-class block opener (§24 C3):
        // it interrupts at column 0 and nests at the content column, uniform
        // with quote/heading/fence/table. `is_definition_list_start` requires a
        // flush-left `:: `, so an indented term folds as lazy text like the rest.
        || is_definition_list_start(line)
        // A table row interrupts only when FLUSH-LEFT, like the quote/heading
        // checks above -- `is_table_start` trims leading whitespace, so without
        // this guard an INDENTED row (`  |a|`) would interrupt where an indented
        // `> q` / `# h` does not. An indented row below/above a list item's
        // content column is lazy paragraph text (§24 C3), not a nested table.
        || (!line.starts_with([' ', '\t']) && is_table_start(line))
    {
        return true;
    }
    if let Some(open) = detect_fence_open(line) {
        if rest.iter().any(|l| is_fence_close(l, open)) {
            return true;
        }
    }
    // A FLUSH-LEFT colon-fence family opener interrupts blockquote lazy
    // continuation like any block opener. An INDENTED colon fence (above the
    // quote's content column) is literal paragraph text under the strict
    // column-0 rule, so lazy continuation stays inside the quote.
    if !line.starts_with([' ', '\t'])
        && (detect_container_open(line).is_some()
            || detect_line_block_open(line).is_some()
            || detect_hardbreaks_block_open(line).is_some()
            || detect_quote_block_open(line).is_some())
    {
        return true;
    }
    false
}

/// A `- ` / `* ` bullet, including the attributed form `-{.c} ` (NOT `+`, the
/// continuation marker; not ordered).
///
/// Delegates to `detect_unordered` so an attributed bullet interrupts a
/// paragraph just like a plain one (and an attributed task already does via
/// `detect_task`). Leading tabs are skipped as well as spaces: a bullet opens
/// a list at any indentation (Rule B), so a tab-indented bullet interrupts a
/// paragraph too.
/// A bare image line is a block image (or figure) ONLY when it stands alone --
/// the next line is blank / EOF, a `^ ` caption, or a paragraph interrupter.
/// When the next line FOLDS (plain text, list marker, another bare image), the
/// image stays inline in a paragraph with that content, per grammar §1722 I3
/// ("an image is not a block of its own; it stays inline in the paragraph").
pub(super) fn image_is_block(cur: &mut LineCursor) -> bool {
    let Some(next) = cur.lines.line(cur.pos + 1) else {
        return true;
    };
    if is_blank_line(next) || caption_content(next).is_some() {
        return true;
    }
    // Peek-1 interruption: test the next line as if it were current, then rewind.
    let next_owned = next.to_string();
    let saved = cur.pos;
    cur.pos += 1;
    let interrupts = interrupts_paragraph(cur, &next_owned);
    cur.pos = saved;
    interrupts
}

pub(super) fn consume_caption(
    cur: &mut LineCursor,
    options: &Options<'_>,
) -> Option<Vec<InlineNode>> {
    consume_caption_slot(cur, options, true)
}

fn consume_caption_slot(
    cur: &mut LineCursor,
    options: &Options<'_>,
    caption_context: bool,
) -> Option<Vec<InlineNode>> {
    let saved = cur.pos;
    // PART 9 §4 (NORMATIVE): `caption_slot = [blank_line], caption` carries at
    // most ONE optional blank line. A caption line adjacent to its host, or
    // separated from it by exactly one blank line, attaches; TWO blank lines
    // DETACH and leave the `^ ` line an ordinary paragraph. Scanning blank
    // lines in a loop attaches across any number of them (carve-rs#830), which
    // no corpus document could distinguish because every captioned document
    // had zero or one blank line.
    if matches!(cur.peek(), Some(line) if is_blank_line(line)) {
        cur.consume();
    }
    let Some(line) = cur.peek() else {
        cur.pos = saved;
        return None;
    };
    let Some(text) = caption_content(line) else {
        cur.pos = saved;
        return None;
    };
    let mut joined = text.to_string();
    // One anchor per folded line, the same shape a paragraph builds. The first
    // entry accounts for the `^ ` marker, which `inline_anchor_for_line`
    // derives by comparing the full line against the inline text.
    let mut anchors = options
        .positions
        .then(|| vec![inline_anchor_for_line(cur, cur.pos, text)]);
    cur.consume();
    // A caption is multi-line inline content, so it folds following lines like a
    // PARAGRAPH (§10), NOT like a heading: a list marker FOLDS in (djot -- a
    // list needs a blank line to interrupt), while a heading / blockquote /
    // table / fenced code / `:::` div / thematic break / `%%%` comment
    // interrupts and ends the caption. A blank line or a further `^ ` caption
    // line also ends it. Continuation lines join with `\n`.
    while let Some(next) = cur.peek() {
        if is_blank_line(next) || caption_content(next).is_some() {
            break;
        }
        let next_owned = next.to_string();
        if interrupts_paragraph(cur, &next_owned) {
            break;
        }
        joined.push('\n');
        joined.push_str(next);
        if let Some(anchors) = &mut anchors {
            anchors.push(inline_anchor_for_line(cur, cur.pos, next));
        }
        cur.consume();
    }
    // §756 (NORMATIVE): strip trailing whitespace - on EVERY line the caption
    // spans, not only its last (PART 2 NO TRAILING WHITESPACE, carve#926; a
    // table caption is one of the contexts that ruling names, and one the
    // executable spec itself was missing until it was measured). Stripping only
    // ever shortens a line's END, so it cannot shift any anchor.
    let joined = joined
        .split('\n')
        .map(|l| trim_ascii_end(strip_lazy(l)))
        .collect::<Vec<_>>()
        .join("\n");
    let text = trim_ascii_end(&joined);
    Some(match anchors {
        Some(anchors) => parse_caption_inline_with_anchor(text, options, anchors, caption_context),
        None => parse_caption_inline_with_options(text, options, caption_context),
    })
}
