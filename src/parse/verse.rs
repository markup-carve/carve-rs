use super::*;

/// A `::: |` line-block (verse) opener: a colon fence (3+) then a bare pipe and
/// nothing else (grammar PART 9 §23). Returns the fence length.
pub(in crate::parse) fn detect_line_block_open(line: &str) -> Option<usize> {
    let trimmed = trim_ascii(line);
    let fence_len = trimmed.bytes().take_while(|b| *b == b':').count();
    if fence_len < 3 {
        return None;
    }
    // grammar: `line_block_open = colon_fence, space, "|"` -- a SPACE between
    // the fence and the pipe is REQUIRED, so `:::|` is not a line block and
    // neither is a tabbed opener. The production always said `space`; this read
    // it as "space or tab", which PART 7's MARKER SEPARATORS AND PADDING SLOTS
    // now rules out explicitly - the pipe is the token that selects this block,
    // so the slot before it is a marker separator (#712, spec carve#886).
    let after = &trimmed[fence_len..];
    let trimmed_after = after.trim_start_matches(' ');
    if trimmed_after.len() == after.len() {
        return None; // no space before the pipe
    }
    // `trimmed` already had its trailing SPACE and TAB run removed by
    // `trim_ascii`, so a `trim_end` here can only ever strip what that helper
    // deliberately left: a VERTICAL TAB, a FORM FEED or a NO-BREAK SPACE, every
    // one of them CONTENT (PART 7, carve#977). The comparison is direct.
    if trimmed_after == "|" {
        Some(fence_len)
    } else {
        None
    }
}

/// Count a line's leading whitespace in visual columns (tab = next 4-stop).
pub(in crate::parse) fn leading_ws_columns(line: &str) -> usize {
    let mut columns = 0usize;
    for ch in line.chars() {
        match ch {
            ' ' => columns += 1,
            '\t' => columns += 4 - (columns % 4),
            _ => break,
        }
    }
    columns
}

/// Remove up to `cols` columns of leading whitespace (tab-aware). When a tab
/// straddles the boundary its unconsumed columns are re-inserted as spaces, so
/// a verse line's relative indentation is preserved exactly (the residual-aware
/// dedent Carve uses on indentation it must keep).
pub(in crate::parse) fn strip_leading_columns(line: &str, cols: usize) -> String {
    let mut columns = 0usize;
    for (i, ch) in line.char_indices() {
        if columns >= cols {
            return line[i..].to_string();
        }
        match ch {
            ' ' => columns += 1,
            '\t' => {
                let next = columns + (4 - columns % 4);
                if next > cols {
                    // Tab crosses the reference column: keep the leftover columns.
                    return " ".repeat(next - cols) + &line[i + 1..];
                }
                columns = next;
            }
            _ => return line[i..].to_string(),
        }
    }
    String::new()
}

/// Parse a `::: |` line block into a `<div class="line-block">`: each stanza
/// (blank-line-separated run) is a paragraph whose soft breaks become hard
/// breaks and whose per-line leading whitespace is preserved (grammar §23).
/// One line-block stanza: its lines, its own span, and the SOURCE columns each
/// line starts and ends at.
///
/// The two column vectors are recorded whatever `placeable_indent` decided,
/// because a break's span is line geometry and survives a tab that makes the
/// line's TEXT unplaceable (carve-rs#480).
pub(in crate::parse) struct Stanza {
    lines: LineBuffer,
    source_columns: Vec<Vec<Option<usize>>>,
    at: Option<Pos>,
    end_cols: Vec<Option<isize>>,
    start_cols: Vec<Option<isize>>,
    /// The comment-only body lines this stanza had, as `(line index, node)`.
    /// Their text is already gone from `lines` - see `verse_comment_line`.
    comments: Vec<(usize, Comment)>,
}

/// A verse body line that is nothing but a comment (grammar PART 9 §23, IT IS
/// REMOVED AT THE BLOCK LAYER). Returns the comment's content.
///
/// ONLY a line whose FIRST character is `%` qualifies: `comment_line`'s optional
/// `[whitespace]` prefix has nothing to consume in verse, where a leading run is
/// CONTENT, so an indented `%%` line is ordinary text. The line is measured
/// AFTER the fence's own structural indent is stripped, which is the reference
/// column the rest of the block measures against.
pub(in crate::parse) fn verse_comment_line(stripped: &str) -> Option<String> {
    let rest = stripped.strip_prefix("%%")?;
    Some(
        rest.strip_prefix(' ')
            .unwrap_or(rest)
            .trim_end_matches([' ', '\t'])
            .to_string(),
    )
}

/// Put the comment-only lines back, at the content position of the line each
/// one came from.
///
/// The block layer emptied those lines before the stanza reached the inline
/// parser, so no inline run can reach a comment and an unclosed verbatim run
/// opened on an earlier line cannot claim it (carve#1333). The node is still
/// published: PART 12 has a `comment` node and the author wrote it.
///
/// Line k begins just after the k-th boundary, counted AT EVERY DEPTH and
/// however it is spelled - an inline container holds the boundaries it spans as
/// its own children, and a closed verbatim run carries one in its value rather
/// than as a node. A stanza with no k-th boundary drops the comment, which
/// happens only inside an unclosed run, where there is no place for a node.
///
/// One pass building new vectors: a stanza of nothing but comment lines is
/// writable, and repeated `Vec::insert` would be quadratic in its length.
/// PART 9 §23: every SOFT line break inside a stanza becomes a HARD break, at
/// EVERY DEPTH (markup-carve/carve#1351, corpus `348`).
///
/// DRIVEN BY NODE KIND, not by depth. §23's neighbouring clause exempts a
/// backslash break and a verbatim run because neither leaves a boundary node in
/// the tree - the backslash consumes its own newline, the run carries it as
/// content. An emphasis run consumes nothing, so its boundary is a node beside
/// the text and the exemption never reaches it.
///
/// Both exemptions therefore need no code here: a verbatim run has no children
/// to walk (corpus `348-2`), and a `hard_break` is already hard (corpus
/// `348-4`).
///
/// The slots are `splice_verse_comments_into`'s, for the same reason: a
/// boundary is a child of whatever construct spans it.
pub(in crate::parse) fn harden_verse_breaks(inlines: Vec<InlineNode>) -> Vec<InlineNode> {
    inlines
        .into_iter()
        .map(|mut node| {
            let attrs = match &mut node {
                InlineNode::NonBreakingSpace(n) => n.attrs.as_mut(),
                InlineNode::Emphasis(n) => n.attrs.as_mut(),
                InlineNode::Link(n) => n.attrs.as_mut(),
                InlineNode::Image(n) => n.attrs.as_mut(),
                InlineNode::Span(n) => n.attrs.as_mut(),
                InlineNode::Ruby(n) => n.attrs.as_mut(),
                InlineNode::Extension(n) => n.attrs.as_mut(),
                InlineNode::Code(n) => n.attrs.as_mut(),
                InlineNode::Math(n) => n.attrs.as_mut(),
                InlineNode::LiteralInline(n) => n.attrs.as_mut(),
                InlineNode::Symbol(n) => n.attrs.as_mut(),
                InlineNode::AutoLink(n) => n.attrs.as_mut(),
                InlineNode::Mention(n) => n.attrs.as_mut(),
                InlineNode::Tag(n) => n.attrs.as_mut(),
                InlineNode::Footnote(n) => n.attrs.as_mut(),
                InlineNode::CriticInsert(n) => n.attrs.as_mut(),
                InlineNode::CriticDelete(n) => n.attrs.as_mut(),
                _ => None,
            };
            if let Some(attrs) = attrs {
                for value in attrs
                    .id
                    .iter_mut()
                    .chain(attrs.classes.iter_mut())
                    .chain(attrs.key_values.values_mut())
                {
                    *value = value.replace(crate::NBSP_PLACEHOLDER, "\u{00a0}");
                }
            }
            match node {
                // The hard break here IS the source's line ending, so it keeps the
                // soft break's span rather than being rebuilt without one.
                InlineNode::SoftBreak(b) => InlineNode::HardBreak(b),
                InlineNode::Emphasis(mut n) => {
                    n.children = harden_verse_breaks(n.children);
                    InlineNode::Emphasis(n)
                }
                InlineNode::Link(mut n) => {
                    n.href = n.href.replace(crate::NBSP_PLACEHOLDER, "\u{00a0}");
                    for value in n
                        .title
                        .iter_mut()
                        .chain(n.ref_label.iter_mut())
                        .chain(n.raw_ref.iter_mut())
                    {
                        *value = value.replace(crate::NBSP_PLACEHOLDER, "\u{00a0}");
                    }
                    n.children = harden_verse_breaks(n.children);
                    InlineNode::Link(n)
                }
                InlineNode::Span(mut n) => {
                    n.children = harden_verse_breaks(n.children);
                    InlineNode::Span(n)
                }
                InlineNode::Extension(mut n) => {
                    n.children = harden_verse_breaks(n.children);
                    InlineNode::Extension(n)
                }
                InlineNode::CriticInsert(mut n) => {
                    n.children = harden_verse_breaks(n.children);
                    InlineNode::CriticInsert(n)
                }
                InlineNode::CriticDelete(mut n) => {
                    n.children = harden_verse_breaks(n.children);
                    InlineNode::CriticDelete(n)
                }
                InlineNode::CriticSubstitute(mut n) => {
                    n.old = harden_verse_breaks(n.old);
                    n.new = harden_verse_breaks(n.new);
                    InlineNode::CriticSubstitute(n)
                }
                InlineNode::Footnote(mut n) => {
                    for value in n.id.iter_mut().chain(n.ref_id.iter_mut()) {
                        *value = value.replace(crate::NBSP_PLACEHOLDER, "\u{00a0}");
                    }
                    n.inline = n.inline.map(harden_verse_breaks);
                    InlineNode::Footnote(n)
                }
                InlineNode::CitationGroup(mut group) => {
                    group.raw = group.raw.replace(crate::NBSP_PLACEHOLDER, "\u{00a0}");
                    for item in &mut group.items {
                        for field in [&mut item.prefix, &mut item.locator, &mut item.suffix]
                            .into_iter()
                            .flatten()
                        {
                            *field = harden_verse_breaks(std::mem::take(field));
                        }
                    }
                    InlineNode::CitationGroup(group)
                }
                InlineNode::Code(mut n) => {
                    n.value = n.value.replace(crate::NBSP_PLACEHOLDER, "\u{00a0}");
                    InlineNode::Code(n)
                }
                InlineNode::Math(mut n) => {
                    n.content = n.content.replace(crate::NBSP_PLACEHOLDER, "\u{00a0}");
                    InlineNode::Math(n)
                }
                InlineNode::RawInline(mut n) => {
                    n.content = n.content.replace(crate::NBSP_PLACEHOLDER, "\u{00a0}");
                    InlineNode::RawInline(n)
                }
                InlineNode::LiteralInline(mut n) => {
                    n.content = n.content.replace(crate::NBSP_PLACEHOLDER, "\u{00a0}");
                    InlineNode::LiteralInline(n)
                }
                InlineNode::Image(mut n) => {
                    for value in [&mut n.src, &mut n.alt]
                        .into_iter()
                        .chain(n.title.iter_mut())
                        .chain(n.ref_label.iter_mut())
                        .chain(n.raw_ref.iter_mut())
                    {
                        *value = value.replace(crate::NBSP_PLACEHOLDER, "\u{00a0}");
                    }
                    InlineNode::Image(n)
                }
                InlineNode::Ruby(mut n) => {
                    for pair in &mut n.pairs {
                        pair.base = harden_verse_breaks(std::mem::take(&mut pair.base));
                        pair.annotation = harden_verse_breaks(std::mem::take(&mut pair.annotation));
                    }
                    InlineNode::Ruby(n)
                }
                other => other,
            }
        })
        .collect()
}

pub(in crate::parse) fn splice_verse_comments(
    inlines: Vec<InlineNode>,
    comments: Vec<(usize, Comment)>,
) -> Vec<InlineNode> {
    if comments.is_empty() {
        return inlines;
    }
    let mut pending = comments.into_iter().peekable();
    let mut line = 0usize;
    // Was the boundary that OPENED the line we are on eaten by a verbatim run,
    // or is it still a node in the tree? See the trailing drain below.
    let mut opened_inside_a_run = false;
    let mut out =
        splice_verse_comments_into(inlines, &mut pending, &mut line, &mut opened_inside_a_run);
    if !opened_inside_a_run {
        while let Some((_, comment)) = pending.next_if(|(at, _)| *at == line) {
            out.push(InlineNode::Comment(comment));
        }
    }
    out
}

/// Move the line counter over the boundaries a verbatim run SWALLOWED, and
/// record that the line it leaves us on was opened by one of them.
///
/// A run with no newline in it crosses no boundary, so it neither moves the
/// counter nor changes whose boundary opened the current line.
pub(in crate::parse) fn advance_over_run(
    value: &str,
    line: &mut usize,
    opened_inside_a_run: &mut bool,
) {
    let eaten = value.matches('\n').count();
    if eaten == 0 {
        return;
    }
    *line += eaten;
    *opened_inside_a_run = true;
}

/// One level of `splice_verse_comments`, sharing the caller's line counter so a
/// boundary counts once wherever in the tree it turns up.
pub(in crate::parse) fn splice_verse_comments_into(
    inlines: Vec<InlineNode>,
    pending: &mut std::iter::Peekable<std::vec::IntoIter<(usize, Comment)>>,
    line: &mut usize,
    opened_inside_a_run: &mut bool,
) -> Vec<InlineNode> {
    let mut out = Vec::with_capacity(inlines.len());
    for mut node in inlines {
        // The comment sits BEFORE the boundary that ends its line: the line is
        // empty now, so there is nothing else on it.
        while let Some((_, comment)) = pending.next_if(|(at, _)| *at == *line) {
            out.push(InlineNode::Comment(comment));
        }
        // Nothing left to place, so nothing left to count for: the rest of this
        // level is carried through without descending into it.
        if pending.peek().is_none() {
            out.push(node);
            continue;
        }
        match &mut node {
            InlineNode::SoftBreak(_) | InlineNode::HardBreak(_) => {
                *line += 1;
                // A break node IS the boundary, so the line it opens has one.
                *opened_inside_a_run = false;
            }
            // A verbatim run's newlines are boundaries it ATE. §23 says the run
            // "carries the break, and what it carries is a NEWLINE", so each one
            // ends a body line exactly as a break node does - and the line the
            // LAST of them opens is inside the run, with no boundary node of its
            // own. Recorded rather than inferred from the count, because a run
            // that crosses no boundary must not claim the line it merely sits on.
            InlineNode::Code(n) => advance_over_run(&n.value, line, opened_inside_a_run),
            InlineNode::Math(n) => advance_over_run(&n.content, line, opened_inside_a_run),
            InlineNode::RawInline(n) => advance_over_run(&n.content, line, opened_inside_a_run),
            InlineNode::LiteralInline(n) => advance_over_run(&n.content, line, opened_inside_a_run),
            // EVERY SLOT AN INLINE NODE HOLDS INLINES IN, not just `children`.
            // An inline footnote carries its body in `inline` and a citation
            // item carries three arrays of its own, so a walk that knew only
            // `children` missed those containers and the emptied line stayed
            // pending - `^[a` / `%% secret` / `c]` wrote the line back as an
            // empty one (markup-carve/carve-js#1184 found the same two slots).
            InlineNode::Emphasis(n) => {
                n.children = splice_verse_comments_into(
                    std::mem::take(&mut n.children),
                    pending,
                    line,
                    opened_inside_a_run,
                )
            }
            InlineNode::Link(n) => {
                n.children = splice_verse_comments_into(
                    std::mem::take(&mut n.children),
                    pending,
                    line,
                    opened_inside_a_run,
                )
            }
            InlineNode::Span(n) => {
                n.children = splice_verse_comments_into(
                    std::mem::take(&mut n.children),
                    pending,
                    line,
                    opened_inside_a_run,
                )
            }
            InlineNode::Extension(n) => {
                n.children = splice_verse_comments_into(
                    std::mem::take(&mut n.children),
                    pending,
                    line,
                    opened_inside_a_run,
                )
            }
            InlineNode::Footnote(n) => {
                if let Some(inline) = &mut n.inline {
                    *inline = splice_verse_comments_into(
                        std::mem::take(inline),
                        pending,
                        line,
                        opened_inside_a_run,
                    );
                }
            }
            InlineNode::CitationGroup(group) => {
                for item in &mut group.items {
                    for field in [&mut item.prefix, &mut item.locator, &mut item.suffix]
                        .into_iter()
                        .flatten()
                    {
                        *field = splice_verse_comments_into(
                            std::mem::take(field),
                            pending,
                            line,
                            opened_inside_a_run,
                        );
                    }
                }
            }
            InlineNode::CriticInsert(n) => {
                n.children = splice_verse_comments_into(
                    std::mem::take(&mut n.children),
                    pending,
                    line,
                    opened_inside_a_run,
                )
            }
            InlineNode::CriticDelete(n) => {
                n.children = splice_verse_comments_into(
                    std::mem::take(&mut n.children),
                    pending,
                    line,
                    opened_inside_a_run,
                )
            }
            _ => {}
        }
        out.push(node);
    }
    out
}

/// Give every line-block hard break a span, even where the stanza's TEXT has
/// none.
pub(in crate::parse) fn place_line_block_breaks(
    inlines: Vec<InlineNode>,
    lines: &LineBuffer,
    end_cols: &[Option<isize>],
    start_cols: &[Option<isize>],
    emptied: &[usize],
    options: &Options<'_>,
) -> Vec<InlineNode> {
    if !options.positions {
        return inlines;
    }
    let mut line = 0usize;
    place_line_block_breaks_into(inlines, lines, end_cols, start_cols, emptied, &mut line)
}

/// One level of [`place_line_block_breaks`], sharing the caller's line counter
/// so a boundary counts once wherever in the tree it turns up.
pub(in crate::parse) fn place_line_block_breaks_into(
    inlines: Vec<InlineNode>,
    lines: &LineBuffer,
    end_cols: &[Option<isize>],
    start_cols: &[Option<isize>],
    emptied: &[usize],
    line: &mut usize,
) -> Vec<InlineNode> {
    let mut out = Vec::with_capacity(inlines.len());
    for mut node in inlines {
        match &mut node {
            // A `SoftBreak` reaching here is one `harden_verse_breaks` did not
            // convert, so it gets no span - but it IS a boundary and has to move
            // the counter, exactly as it does in the comment walk.
            InlineNode::SoftBreak(_) => *line += 1,
            InlineNode::HardBreak(brk) => {
                let k = *line;
                *line += 1;
                // A break ending a line the block layer EMPTIED is recomputed
                // even when the inline parser gave it one: what that parser
                // measured is the emptied line, which ends at column 1, and the
                // newline the author wrote is at the end of the comment they
                // wrote (grammar PART 9 §23).
                if brk.pos.is_none() || emptied.binary_search(&k).is_ok() {
                    // Start: just past the last character of line k. End: the
                    // first column of line k+1, which is where the next line's
                    // content begins. Both are 1-based, matching every other
                    // span.
                    let pos = match (
                        lines.lines.get(k).and_then(|line| line.source_line()),
                        end_cols.get(k).copied().flatten(),
                        lines.lines.get(k + 1).and_then(|line| line.source_line()),
                        start_cols.get(k + 1).copied().flatten(),
                    ) {
                        (Some(start_line), Some(end_col), Some(end_line), Some(next_col)) => {
                            Some(Pos {
                                start_line,
                                start_column: document_column(end_col, 0),
                                end_line,
                                end_column: document_column(next_col, 0),
                                ..Default::default()
                            })
                        }
                        _ => None,
                    };
                    if pos.is_some() {
                        brk.pos = pos;
                    }
                }
            }
            // A verbatim run's newlines are boundaries it ATE, carried in the
            // value rather than as nodes, so they move the counter here for the
            // same reason they do in `splice_verse_comments_into`.
            InlineNode::Code(n) => *line += n.value.matches('\n').count(),
            InlineNode::Math(n) => *line += n.content.matches('\n').count(),
            InlineNode::RawInline(n) => *line += n.content.matches('\n').count(),
            InlineNode::LiteralInline(n) => *line += n.content.matches('\n').count(),
            // EVERY SLOT AN INLINE NODE HOLDS INLINES IN, the same list
            // `splice_verse_comments_into` and `harden_verse_breaks` walk.
            InlineNode::Emphasis(n) => {
                n.children = place_line_block_breaks_into(
                    std::mem::take(&mut n.children),
                    lines,
                    end_cols,
                    start_cols,
                    emptied,
                    line,
                )
            }
            InlineNode::Link(n) => {
                n.children = place_line_block_breaks_into(
                    std::mem::take(&mut n.children),
                    lines,
                    end_cols,
                    start_cols,
                    emptied,
                    line,
                )
            }
            InlineNode::Span(n) => {
                n.children = place_line_block_breaks_into(
                    std::mem::take(&mut n.children),
                    lines,
                    end_cols,
                    start_cols,
                    emptied,
                    line,
                )
            }
            InlineNode::Extension(n) => {
                n.children = place_line_block_breaks_into(
                    std::mem::take(&mut n.children),
                    lines,
                    end_cols,
                    start_cols,
                    emptied,
                    line,
                )
            }
            InlineNode::CriticInsert(n) => {
                n.children = place_line_block_breaks_into(
                    std::mem::take(&mut n.children),
                    lines,
                    end_cols,
                    start_cols,
                    emptied,
                    line,
                )
            }
            InlineNode::CriticDelete(n) => {
                n.children = place_line_block_breaks_into(
                    std::mem::take(&mut n.children),
                    lines,
                    end_cols,
                    start_cols,
                    emptied,
                    line,
                )
            }
            InlineNode::Footnote(n) => {
                if let Some(inline) = &mut n.inline {
                    *inline = place_line_block_breaks_into(
                        std::mem::take(inline),
                        lines,
                        end_cols,
                        start_cols,
                        emptied,
                        line,
                    );
                }
            }
            InlineNode::CitationGroup(group) => {
                for item in &mut group.items {
                    for field in [&mut item.prefix, &mut item.locator, &mut item.suffix]
                        .into_iter()
                        .flatten()
                    {
                        *field = place_line_block_breaks_into(
                            std::mem::take(field),
                            lines,
                            end_cols,
                            start_cols,
                            emptied,
                            line,
                        );
                    }
                }
            }
            _ => {}
        }
        out.push(node);
    }
    out
}

pub(in crate::parse) fn parse_line_block(cur: &mut LineCursor, options: &Options<'_>) -> BlockNode {
    let span_start = cur.pos;
    let opener = cur.peek().unwrap();
    let fence_len = detect_line_block_open(opener).unwrap();
    // Verse indentation is measured RELATIVE TO THE FENCE (grammar §23
    // REFERENCE COLUMN): strip the opener's own structural indent from each
    // body line before preserving the author's intra-verse whitespace.
    let base_indent = leading_ws_columns(opener);
    let end = find_colon_fence_end(
        cur.lines,
        cur.pos,
        fence_len,
        true,
        &mut cur.code_closer_last_index,
        &mut cur.comment_closer_last_index,
    );
    if definition_regions::recording() {
        definition_regions::record(
            cur.source_line(span_start),
            definition_regions::VerseLine::Opener,
        );
        for line in span_start + 1..end {
            definition_regions::record(cur.source_line(line), definition_regions::VerseLine::Body);
        }
        cur.pos = end;
        return BlockNode::LineBlock(LineBlock {
            pos: span_of(cur, span_start, cur.pos, options),
            attrs: None,
            children: Vec::new(),
            lines: None,
        });
    }
    cur.consume();
    let mut stanzas: Vec<Stanza> = Vec::new();
    let mut stanza = LineBuffer::default();
    let mut source_columns = Vec::new();
    let mut stanza_end_cols: Vec<Option<isize>> = Vec::new();
    let mut stanza_start_cols: Vec<Option<isize>> = Vec::new();
    let mut stanza_comments: Vec<(usize, Comment)> = Vec::new();
    // Where the open stanza began, and where it ended - the CURSOR's own line
    // indices, which still point at the source. The rewritten verse text cannot
    // give a column back, but the lines a stanza occupies are not in doubt.
    let mut stanza_start: Option<usize> = None;
    let mut stanza_end = cur.pos;
    while let Some(line) = cur.peek() {
        if cur.pos + 1 == end && exact_colon_fence_len(line) == Some(fence_len) {
            cur.consume();
            break;
        }
        let source_line = cur.source_line(cur.pos);
        let line_at = cur.pos;
        cur.consume();
        if is_blank_line(line) {
            if !stanza.lines.is_empty() {
                let at = stanza_start.take();
                let end_cols = std::mem::take(&mut stanza_end_cols);
                let start_cols = std::mem::take(&mut stanza_start_cols);
                stanzas.push(Stanza {
                    source_columns: std::mem::take(&mut source_columns),
                    lines: std::mem::take(&mut stanza),
                    at: at.and_then(|start| span_of(cur, start, stanza_end, options)),
                    end_cols,
                    start_cols,
                    comments: std::mem::take(&mut stanza_comments),
                });
            }
            continue;
        }
        stanza_start.get_or_insert(line_at);
        stanza_end = cur.pos;
        let stripped = strip_leading_columns(line, base_indent);
        // A COMMENT-ONLY BODY LINE IS DECIDED HERE, with the other block-layer
        // decisions and BEFORE any inline content exists (grammar PART 9 §23, IT
        // IS REMOVED AT THE BLOCK LAYER). Leaving it to the inline parser let
        // §21's verbatim exclusion claim the line, so a stray backtick anywhere
        // above PUBLISHED the comment's own text inside the code span - the one
        // outcome a comment may never have (carve#1333).
        //
        // The line stays: what is removed is its TEXT, so the stanza keeps its
        // shape and the boundary below it still hardens into the `<br>` the
        // author wrote. The comment itself is put back after the inline run, by
        // `splice_verse_comments`.
        let comment = verse_comment_line(&stripped);
        let mut columns = Vec::new();
        let expanded = expand_line_block_ws(&stripped, options.positions.then_some(&mut columns));
        if options.positions {
            source_columns.push(columns);
        }
        let stripped_columns = stripped_col(cur.source_col(line_at), line, &stripped);
        // Where this line ENDS in the source, recorded whatever the indent
        // check decided. A hard break is the newline ENDING a line, not content
        // on it, and tab expansion does not move a line ending -- so the break's
        // span stays derivable on exactly the lines whose text is not
        // (carve-rs#480). Measured on `line`, which is the source text before
        // `expand_line_block_ws` rewrites gaps into placeholders.
        if options.positions {
            stanza_end_cols.push(
                cur.source_col(line_at)
                    .map(|stripped_cols| stripped_cols + line.chars().count() as isize),
            );
            stanza_start_cols.push(cur.source_col(line_at));
        }
        if let Some(content) = comment {
            let lead = (line.chars().count() - stripped.chars().count()) as isize;
            let start = cur.source_col(line_at).map(|col| col + lead);
            let pos = match (options.positions.then_some(()).and(source_line), start) {
                (Some(at), Some(start)) => Some(Pos {
                    start_line: at,
                    start_column: document_column(start, 0),
                    end_line: at,
                    end_column: document_column(start, stripped.chars().count()),
                    ..Default::default()
                }),
                _ => None,
            };
            stanza_comments.push((
                stanza.lines.len(),
                Comment {
                    block: false,
                    delimited: false,
                    content,
                    pos,
                },
            ));
            stanza.push_at(String::new(), source_line, stripped_columns);
        } else {
            stanza.push_at(expanded, source_line, stripped_columns);
        }
    }
    if !stanza.lines.is_empty() {
        let at = stanza_start.take();
        stanzas.push(Stanza {
            source_columns,
            lines: stanza,
            at: at.and_then(|start| span_of(cur, start, stanza_end, options)),
            end_cols: stanza_end_cols,
            start_cols: stanza_start_cols,
            comments: stanza_comments,
        });
    }

    let children = stanzas
        .into_iter()
        .map(
            |Stanza {
                 lines,
                 source_columns,
                 at,
                 end_cols,
                 start_cols,
                 comments,
             }| {
                let source_line = lines.lines.first().and_then(|line| line.source_line());
                let anchors: Vec<Option<(usize, isize)>> = if options.positions {
                    lines
                        .lines
                        .iter()
                        .map(|line| Some((line.source_line()?, line.stripped?)))
                        .collect()
                } else {
                    Vec::new()
                };
                // AT EVERY DEPTH (PART 9 section 23, markup-carve/carve#1351).
                // See `harden_verse_breaks` for why the test is node kind and
                // not depth, and why both exemptions need no code.
                let _verse = InLineBlock::enter();
                let inlines = harden_verse_breaks(parse_inline_with_anchor(
                    &lines
                        .lines
                        .iter()
                        .map(|l| strip_lazy(&l.text))
                        .collect::<Vec<_>>()
                        .join("\n"),
                    options,
                    InlineAnchor {
                        lines: &anchors,
                        breaks: &[],
                        columns: Some(&source_columns),
                    },
                ));
                let emptied: Vec<usize> = if options.positions {
                    comments.iter().map(|(line, _)| *line).collect()
                } else {
                    Vec::new()
                };
                let inlines = place_line_block_breaks(
                    inlines,
                    &lines,
                    &end_cols,
                    &start_cols,
                    &emptied,
                    options,
                );
                let inlines = splice_verse_comments(inlines, comments);
                let mut at = at;
                if let (Some(pos), Some(Some((_, column)))) = (&mut at, anchors.first()) {
                    pos.start_column = document_column(*column, 0);
                }
                let mut node = BlockNode::Paragraph(Paragraph {
                    attrs: None,
                    children: inlines,
                    pos: at,
                    ..Default::default()
                });
                if options.source_lines {
                    if let Some(line) = source_line {
                        stamp_source_line(&mut node, line);
                    }
                }
                node
            },
        )
        .collect();

    // No inline opener attributes (strict djot); a preceding block-attribute
    // line merges onto this node in parse_blocks.
    BlockNode::LineBlock(LineBlock {
        pos: span_of(cur, span_start, cur.pos, options),
        attrs: None,
        children,
        lines: None,
    })
}
