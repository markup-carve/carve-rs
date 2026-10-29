use super::*;

/// Codepoints a container took from the front of a line, when that is knowable.
pub(in crate::parse) fn stripped_col(
    outer: Option<isize>,
    original: &str,
    stripped: &str,
) -> Option<isize> {
    let outer = outer?;
    if original.ends_with(stripped) {
        return Some(outer + original.chars().count() as isize - stripped.chars().count() as isize);
    }
    // THE SAME SUFFIX TEST, against the line with its trailing whitespace
    // dropped. It stays anchored at the end - of the trimmed body - so it names
    // exactly one offset, and the prefix it implies is measured rather than
    // guessed. That is what keeps it as safe as the rule above it: a trailing
    // trim is invisible to a constant that describes the front of the line.
    let body = trim_ascii_end(original);
    if body.ends_with(stripped) {
        return Some(outer + body.chars().count() as isize - stripped.chars().count() as isize);
    }
    let synthetic = stripped.chars().take_while(|c| *c == ' ').count();
    let tail = &stripped[synthetic..];
    if synthetic == 0 || !original.ends_with(tail) {
        return None;
    }
    let consumed = original.chars().count().checked_sub(tail.chars().count())?;
    if !original
        .chars()
        .take(consumed)
        .all(|c| c == ' ' || c == '\t')
    {
        return None;
    }
    Some(outer + consumed as isize - synthetic as isize)
}

/// WHERE THE SIGNED QUANTITY STOPS: a 1-based document column.
///
/// `stripped` is the constant the column map carries for a line and `within` is
/// the codepoint offset inside the line the parser saw, so their sum is the
/// 0-based document column. Every `Pos` column in this file is built here, so
/// there is exactly one place the two units meet and exactly one place the
/// clamp lives.
///
/// The clamp is UNREACHABLE for a well-formed map, and is a guard rather than a
/// fix: a negative constant only ever arises from a synthetic space prefix, and
/// nothing is placed at a column inside that prefix - it is indentation, and
/// `within` has already skipped it. Saturating here means a future producer
/// that does get it wrong publishes column 1 rather than wrapping to
/// `usize::MAX` and reporting a span past the end of the document.
pub(in crate::parse) fn document_column(stripped: isize, within: usize) -> usize {
    (stripped + within as isize).max(0) as usize + 1
}

/// Seed a list item's body with its marker line, carrying the column the marker
/// itself occupied so a block opened on that line lands where the author wrote
/// it rather than at column 1.
pub(in crate::parse) fn item_marker_source(
    cur: &LineCursor<'_>,
    content: &str,
    at: usize,
) -> MappedSource {
    let stripped = cur
        .lines
        .line(at)
        .and_then(|line| stripped_col(cur.source_col(at), line, content));
    let mut source = MappedSource::new_line_at(content.to_string(), cur.source_line(at), stripped);
    source.item_marker_at_start = true;
    source.in_quoted_body = cur.in_quoted_body;
    source
}

/// Span of a list item's lead paragraph. It starts where the marker's CONTENT
/// starts - the paragraph is the text, not the bullet - and ends at the last
/// line folded into it by lazy continuation.
pub(in crate::parse) fn item_paragraph_span(
    cur: &LineCursor<'_>,
    start_at: usize,
    end_at: usize,
    content: &str,
    options: &Options<'_>,
) -> Option<Pos> {
    if !options.positions {
        return None;
    }
    let start_line = cur.source_line(start_at)?;
    let line = cur.lines.line(start_at)?;
    let start_stripped = stripped_col(cur.source_col(start_at), line, content)?;
    let end_line = cur.source_line(end_at).unwrap_or(start_line);
    let end_width = cur
        .lines
        .line(end_at)
        .map(|l| strip_lazy(l).chars().count())
        .unwrap_or(0);
    Some(Pos {
        start_line,
        end_line,
        start_column: document_column(start_stripped, 0),
        end_column: document_column(cur.source_col(end_at).unwrap_or(0), end_width),
        start_offset: 0,
        end_offset: 0,
        file: None,
    })
}

/// Anchor for the line at `pos`: its source line, and the codepoints in front
/// of `inline_line` in the document.
///
/// THE LAZY FRAME IS NOT SOURCE TEXT, so neither side is measured with it on.
/// A framed line is `LAZY` plus the line's content, and the inline text a
/// caller builds holds the content alone, so an anchor taken against the framed
/// line read the frame's three codepoints as the line's own indentation: a code
/// span opened there began at the INDENTATION rather than at its backtick run
/// (markup-carve/carve-rs#1832). Two framing sites had compensated for that by
/// discounting the frame from the column they recorded, which left the same
/// line reading one way here and another in `span_of`; the frame is discounted
/// in the readers now, so every site records the column it actually took.
pub(in crate::parse) fn inline_anchor_for_line(
    cur: &LineCursor<'_>,
    pos: usize,
    inline_line: &str,
) -> Option<(usize, isize)> {
    Some((
        cur.source_line(pos)?,
        stripped_col(
            cur.source_col(pos),
            strip_lazy(cur.lines.line(pos)?),
            strip_lazy(inline_line),
        )?,
    ))
}

pub(in crate::parse) fn parse_inline_lines_with_anchor(
    text: &str,
    options: &Options<'_>,
    lines: Vec<Option<(usize, isize)>>,
) -> Vec<InlineNode> {
    parse_inline_with_anchor(text, options, InlineAnchor::lines(&lines))
}

/// The same, for a text whose segments are NOT separated by newlines.
///
/// `breaks` are the byte offsets at which the next entry of `lines` takes over.
/// A table cell rebuilt across a `+` continuation is the only text shaped this
/// way: its fragments come from different source lines and are joined by a
/// manufactured space, so nothing in the text itself marks where one ends.
pub(in crate::parse) fn parse_inline_segments_with_anchor(
    text: &str,
    options: &Options<'_>,
    lines: Vec<Option<(usize, isize)>>,
    breaks: Vec<usize>,
) -> Vec<InlineNode> {
    parse_inline_with_anchor(
        text,
        options,
        InlineAnchor {
            lines: &lines,
            breaks: &breaks,
            columns: None,
        },
    )
}

/// Build a span for the lines `[start, end)` of `cur`, in the ORIGINAL source.
///
/// Returns `None` when the source line or the stripped column width is unknown
/// for the first line - a position that cannot be trusted is worse than no
/// position, because a consumer cannot tell the difference.
///
/// Offsets are left at zero here and filled by `fill_offsets` once the whole
/// document is parsed: an offset needs the original text, which the parser sees
/// only as already-stripped lines.
pub(in crate::parse) fn span_of(
    cur: &LineCursor<'_>,
    start: usize,
    end: usize,
    options: &Options<'_>,
) -> Option<Pos> {
    if !options.positions {
        return None;
    }
    let start_line = cur.source_line(start)?;
    let stripped = cur.source_col(start)?;
    let last = end.saturating_sub(1).max(start);
    let end_line = cur.source_line(last).unwrap_or(start_line);
    // The parser sees the line with its container prefix removed, so the
    // document column is what the container took plus the indent that remains.
    //
    // INDENTATION IS SPACE AND TAB, and nothing else is. `trim_start` here was
    // the Unicode whitespace property, so a line opening with a no-break space
    // or an en quad had that character counted as INDENT - the block's span
    // began one column past its own first child, which PART 12 §4's containment
    // rule forbids (carve#913, and the finding that pass reports). Those
    // characters are CONTENT under PART 1's `indent` terminal (carve#890), so
    // they belong inside the span, not in front of it.
    let indent = cur
        .lines
        .line(start)
        .map(strip_lazy)
        .map(|l| l.chars().count() - trim_ascii_start(l).chars().count())
        .unwrap_or(0);
    let width = cur
        .lines
        .line(last)
        .map(|l| strip_lazy(l).chars().count())
        .unwrap_or(0);
    // The LAST line may have had a different amount taken off it than the
    // first: a lazily continued paragraph starts inside a blockquote or list
    // item and ends flush left, so reusing the opening line's count runs the
    // end column past the end of the document.
    let end_stripped = if last == start {
        stripped
    } else {
        cur.source_col(last)?
    };
    Some(Pos {
        start_line,
        end_line,
        start_column: document_column(stripped, indent),
        end_column: document_column(end_stripped, width),
        start_offset: 0,
        end_offset: 0,
        file: None,
    })
}

/// Fill node and sidecar offsets in codepoints (PART 12 §4).
pub(in crate::parse) fn fill_offsets(blocks: &mut [BlockNode], line_starts: &CodepointLineStarts) {
    for block in blocks {
        if let Some(pos) = block_pos_mut(block) {
            apply_offsets(pos, line_starts);
        }
        // Recurse into the containers that hold blocks and inline content.
        match block {
            BlockNode::Heading(h) => apply_inline_offsets(&mut h.children, line_starts),
            BlockNode::Paragraph(p) => {
                apply_inline_offsets(&mut p.children, line_starts);
                let first = p
                    .children
                    .iter()
                    .find(|node| {
                        !matches!(node, InlineNode::SoftBreak(_) | InlineNode::HardBreak(_))
                    })
                    .and_then(owned_inline_pos);
                let last = p.children.last().and_then(owned_inline_pos);
                // THE TWO ENDS ARE TAKEN SEPARATELY, because they are two
                // rules. Requiring BOTH before touching either meant an
                // unplaced last child also gave up the start the first child
                // supplies, and in a footnote body - where the block layer's
                // extent and the inline offsets do not agree, a question still
                // open across the engines - that put the stanza's first `text`
                // OUTSIDE its own paragraph. Which is the containment defect
                // this pair of rules exists to prevent, arrived at from the
                // third side.
                if let Some(pos) = &mut p.pos {
                    if let Some(first) = first {
                        pos.start_line = first.start_line;
                        pos.start_column = first.start_column;
                        pos.start_offset = first.start_offset;
                    }
                    if let Some(last) = last {
                        pos.end_line = last.end_line;
                        pos.end_column = last.end_column;
                        pos.end_offset = last.end_offset;
                    }
                }
            }
            BlockNode::BlockQuote(b) => {
                fill_offsets(&mut b.children, line_starts);
            }
            BlockNode::Directive(d) => {
                if let Some(title) = &mut d.title {
                    apply_inline_offsets(title, line_starts);
                }
                fill_offsets(&mut d.children, line_starts);
            }
            BlockNode::Div(d) => fill_offsets(&mut d.children, line_starts),
            BlockNode::Admonition(a) => {
                if let Some(title) = &mut a.title {
                    apply_inline_offsets(title, line_starts);
                }
                fill_offsets(&mut a.children, line_starts);
            }
            BlockNode::List(l) => {
                for item in &mut l.items {
                    // The ITEM's own span too, not only the blocks inside it:
                    // a span whose offsets are never filled stays 0..0, which
                    // reads as present and selects nothing.
                    if let Some(pos) = item.pos.as_mut() {
                        apply_offsets(pos, line_starts);
                    }
                    fill_offsets(&mut item.children, line_starts);
                    if let (Some(pos), Some(last)) = (
                        item.pos.as_mut(),
                        item.children
                            .iter()
                            .rev()
                            .find_map(crate::ast_json::block_pos)
                            .cloned(),
                    ) {
                        pos.end_line = last.end_line;
                        pos.end_column = last.end_column;
                        pos.end_offset = last.end_offset;
                    }
                }
            }
            BlockNode::Table(t) => apply_table_offsets(t, line_starts),
            BlockNode::LineBlock(l) => fill_offsets(&mut l.children, line_starts),
            BlockNode::DefinitionList(d) => {
                for item in &mut d.items {
                    for term in &mut item.terms {
                        // The TERM's own span, not only its inline content. This
                        // walk reached the children and skipped the node, so a
                        // `<dt>` carried line and column with offsets of 0..0 -
                        // present, and selecting nothing. Same for `<dd>` below.
                        if let Some(pos) = term.pos.as_mut() {
                            apply_offsets(pos, line_starts);
                        }
                        apply_inline_offsets(&mut term.children, line_starts);
                    }
                    for def in &mut item.definitions {
                        if let Some(pos) = def.pos.as_mut() {
                            apply_offsets(pos, line_starts);
                        }
                        fill_offsets(&mut def.children, line_starts);
                    }
                }
            }
            BlockNode::Figure(f) => {
                // The CAPTION first. It was the one part of a figure this walk
                // never reached, so a caption's inline spans kept line and
                // column but offsets of 0..0 - which reads as present and
                // selects nothing, the exact shape section 4 forbids.
                apply_inline_offsets(&mut f.caption, line_starts);
                match &mut *f.target {
                    FigureTarget::BlockQuote(q) => {
                        // The quote's OWN span, which this arm skipped while
                        // filling everything inside it: it kept line and column
                        // and offsets of 0..0, so the quote selected nothing and
                        // every block within it fell outside its own parent
                        // (carve#565). Same defect the code-block arm below
                        // records, one target along.
                        if let Some(pos) = q.pos.as_mut() {
                            apply_offsets(pos, line_starts);
                        }
                        fill_offsets(&mut q.children, line_starts);
                    }
                    FigureTarget::Paragraph(p) => {
                        if let Some(pos) = p.pos.as_mut() {
                            apply_offsets(pos, line_starts);
                        }
                        apply_inline_offsets(&mut p.children, line_starts);
                    }
                    FigureTarget::Table(t) => apply_table_offsets(t, line_starts),
                    FigureTarget::Image(i) => {
                        if let Some(pos) = i.pos.as_mut() {
                            apply_offsets(pos, line_starts);
                        }
                    }
                    // A code block was reached by the catch-all below and so
                    // kept offsets of 0..0 - present, and selecting nothing.
                    // The arms are exhaustive now: a target added later fails to
                    // compile rather than silently reporting an empty span.
                    FigureTarget::CodeBlock(c) => {
                        if let Some(pos) = c.pos.as_mut() {
                            apply_offsets(pos, line_starts);
                        }
                    }
                }
            }
            BlockNode::FigureGroup(g) => {
                if let Some(caption) = &mut g.caption {
                    apply_inline_offsets(caption, line_starts);
                }
                fill_offsets(&mut g.children, line_starts);
            }
            _ => {}
        }
    }
}

/// Widen a container over the DEFINITION it hosted.
pub(in crate::parse) fn start_list_at_markup_after_quote(pos: &mut Pos, lines: &[&str]) {
    let Some(line) = lines.get(pos.start_line.saturating_sub(1)) else {
        return;
    };
    let prefix: String = line
        .chars()
        .take(pos.start_column.saturating_sub(1))
        .collect();
    if !prefix.contains('>') {
        return;
    }
    let tail: String = line
        .chars()
        .skip(pos.start_column.saturating_sub(1))
        .collect();
    if detect_list_marker_full(&tail).is_none() {
        return;
    }
    let placing = tail.chars().count() - trim_ascii_start(&tail).chars().count();
    pos.start_column += placing;
    pos.start_offset += placing;
}

pub(in crate::parse) fn narrow_to_last_placed_child(blocks: &mut [BlockNode], lines: &[&str]) {
    for block in blocks.iter_mut() {
        match block {
            BlockNode::BlockQuote(n) => narrow_to_last_placed_child(&mut n.children, lines),
            BlockNode::Directive(n) => narrow_to_last_placed_child(&mut n.children, lines),
            BlockNode::Div(n) => narrow_to_last_placed_child(&mut n.children, lines),
            BlockNode::Admonition(n) => narrow_to_last_placed_child(&mut n.children, lines),
            BlockNode::FigureGroup(n) => narrow_to_last_placed_child(&mut n.children, lines),
            BlockNode::LineBlock(n) => narrow_to_last_placed_child(&mut n.children, lines),
            BlockNode::ExtensionCarrier(n) => narrow_to_last_placed_child(&mut n.children, lines),
            BlockNode::Figure(n) => {
                if let FigureTarget::BlockQuote(q) = &mut *n.target {
                    narrow_to_last_placed_child(&mut q.children, lines);
                    // A FENCED quote owns its `:::` closer, so it ends after it
                    // rather than at its last child (CARVE-P12-014). Narrowing
                    // here made the same construct answer one way standalone
                    // and another as a figure target (carve-rs#2165).
                    if !q.fenced {
                        let last = q
                            .children
                            .iter()
                            .rev()
                            .find_map(crate::ast_json::block_pos)
                            .cloned();
                        if let Some(pos) = q.pos.as_mut() {
                            narrow_container_end(pos, last, lines, true);
                        }
                    }
                }
            }
            BlockNode::DefinitionList(n) => {
                for item in &mut n.items {
                    // A TERM ENDS AT ITS LAST PLACED CHILD like every other
                    // container without a closer. Its span covers the `:: `
                    // marker and every line the term folded, so it was derived
                    // from those LINES - and a folded line ending in a space
                    // put that space inside the term and inside no child of it,
                    // because PART 2 drops a trailing run before the inline
                    // content is parsed. Its sibling `definition_description`
                    // already narrowed to its last block at parse time; the
                    // term was the odd one out in its own function.
                    for term in &mut item.terms {
                        let last = term.children.iter().rev().find_map(owned_inline_pos);
                        if let Some(pos) = term.pos.as_mut() {
                            end_at_last_placed_child(pos, last);
                        }
                    }
                    for def in &mut item.definitions {
                        narrow_to_last_placed_child(&mut def.children, lines);
                        // END AT THE LAST PLACED CHILD, like every other
                        // container. A footnote definition authored in the body
                        // and hoisted out by PART 9 SS7 is a SIBLING, not a
                        // child (markup-carve/carve#1522), so the description's
                        // span must not reach over it (carve-rs#1555). The list
                        // that hosts it derives its own end from this, so both
                        // narrow together.
                        let last = def
                            .children
                            .iter()
                            .rev()
                            .find_map(crate::ast_json::block_pos)
                            .cloned();
                        if let Some(pos) = def.pos.as_mut() {
                            // A description whose only content hoisted to the
                            // root has no placed child; its span is then its own
                            // MARKER LINE and stops there, not the continuation
                            // lines that carried the hoisted definitions
                            // (carve#1522, carve#1963). Its `:` marker is not one
                            // of the list openers `narrow_container_end` knows, so
                            // the marker-line fallback is spelled here.
                            if !end_at_last_placed_child(pos, last) {
                                end_at_marker_line(pos, lines);
                            }
                        }
                    }
                }
            }
            BlockNode::List(n) => {
                if let Some(pos) = n.pos.as_mut() {
                    start_list_at_markup_after_quote(pos, lines);
                }
                for item in &mut n.items {
                    narrow_to_last_placed_child(&mut item.children, lines);
                    let last = item
                        .children
                        .iter()
                        .rev()
                        .find_map(crate::ast_json::block_pos)
                        .cloned();
                    if let Some(pos) = item.pos.as_mut() {
                        start_list_at_markup_after_quote(pos, lines);
                        narrow_container_end(pos, last, lines, false);
                    }
                }
            }
            _ => {}
        }
        match block {
            // A QUOTE HAS TWO SPELLINGS AND ONLY ONE OF THEM ENDS AT ITS LAST
            // CHILD. `docs/ast-json-contract.md` reads §4's end rule as a
            // statement about MARKUP - a container ends at the markup that
            // closes it, and "ends at its last placed child" is the case for a
            // container whose closer is IMPLICIT. The `>` prefix form has none,
            // so its content's end is its extent; the `::: >` form has one, and
            // every other colon fence spans it. Narrowed by kind alone, the
            // fenced quote was shrunk off its own closer, so it read 1 to 2
            // where a div, an admonition and a line block over the same three
            // lines all read 1 to 3 - and `carve lint`, which reads the span to
            // find the closer, then reported a closed container as unclosed
            // (carve-rs#2099, and carve-php#2636 for the same reading). An
            // emptied fenced quote is unaffected: it has no placed child, so the
            // narrowing never reached it.
            BlockNode::BlockQuote(n) if !n.fenced => {
                let last = n
                    .children
                    .iter()
                    .rev()
                    .find_map(crate::ast_json::block_pos)
                    .cloned();
                if let Some(pos) = n.pos.as_mut() {
                    narrow_container_end(pos, last, lines, true);
                }
            }
            BlockNode::List(n) => {
                let last = n.items.iter().rev().find_map(|item| item.pos.clone());
                if let Some(pos) = n.pos.as_mut() {
                    narrow_container_end(pos, last, lines, false);
                }
            }
            // A HEADING ENDS AT ITS LAST PLACED CHILD. Its span came from the
            // line it consumed, at that line's full width, while its children
            // were parsed from the same line passed through `trim_ascii_end` -
            // so a heading written with a trailing space claimed a codepoint
            // that PART 2 had already dropped and no child could hold.
            //
            // With NO placed child the heading keeps the line it was written
            // on: `#` alone still names a construct the author typed, and §4's
            // "omit rather than invent" is about a node that cannot be placed
            // at all, not one whose content is empty.
            BlockNode::Heading(n) => {
                let last = n.children.iter().rev().find_map(owned_inline_pos);
                if let Some(pos) = n.pos.as_mut() {
                    end_at_last_placed_child(pos, last);
                }
            }
            BlockNode::DefinitionList(n) => {
                let last = n.items.iter().rev().find_map(|item| {
                    item.definitions
                        .iter()
                        .rev()
                        .find_map(|def| def.pos.clone())
                        .or_else(|| item.terms.iter().rev().find_map(|term| term.pos.clone()))
                });
                if let Some(pos) = n.pos.as_mut() {
                    narrow_container_end(pos, last, lines, false);
                }
            }
            _ => {}
        }
    }
}

/// One container's end: its last placed child's, or its own markup where it has
/// no placed child at all.
pub(crate) fn end_at_last_placed_child(pos: &mut Pos, last: Option<Pos>) -> bool {
    let Some(last) = last else {
        return false;
    };
    pos.end_line = last.end_line;
    pos.end_column = last.end_column;
    pos.end_offset = last.end_offset;
    true
}

/// End a span at its own start line's full width - the reading a
/// `definition_description` takes when its only content hoisted to the root, so
/// it has no placed child but its authored marker line is still its own extent
/// (carve#1963). Matches the width the reference derives from that line: the
/// span runs from its start column to the end of the line, trailing run and all,
/// the same bytes `lineRange` covers in carve-js.
pub(in crate::parse) fn end_at_marker_line(pos: &mut Pos, lines: &[&str]) {
    let Some(line) = lines.get(pos.start_line.saturating_sub(1)) else {
        return;
    };
    let len = line.chars().count();
    let start = pos.start_column.saturating_sub(1);
    if len < start {
        return;
    }
    let width = len - start;
    pos.end_line = pos.start_line;
    pos.end_column = pos.start_column + width;
    pos.end_offset = pos.start_offset + width;
}

pub(in crate::parse) fn narrow_container_end(
    pos: &mut Pos,
    last: Option<Pos>,
    lines: &[&str],
    quote: bool,
) {
    if end_at_last_placed_child(pos, last) {
        return;
    }
    let Some(line) = lines.get(pos.start_line.saturating_sub(1)) else {
        return;
    };
    let width = emptied_container_markup(line, pos.start_column, quote);
    if width == 0 {
        return;
    }
    pos.end_line = pos.start_line;
    pos.end_column = pos.start_column + width;
    pos.end_offset = pos.start_offset + width;
}

/// The width in CODEPOINTS of the markup that opened an emptied container, read
/// from its own start column, or 0 where the line does not spell one - in which
/// case the span is left as it was rather than guessed at.
pub(in crate::parse) fn emptied_container_markup(
    line: &str,
    start_column: usize,
    quote: bool,
) -> usize {
    let tail: Vec<char> = line.chars().skip(start_column.saturating_sub(1)).collect();
    let mut i = 0;
    while i < tail.len() && (tail[i] == ' ' || tail[i] == '\t') {
        i += 1;
    }
    if quote {
        if i < tail.len() && tail[i] == '>' {
            i += 1;
        } else {
            return 0;
        }
    } else if i < tail.len() && matches!(tail[i], '-' | '+' | '*' | '.') {
        i += 1;
    } else {
        let mut j = i;
        while j < tail.len() && tail[j].is_ascii_alphanumeric() {
            j += 1;
        }
        if j > i && j < tail.len() && (tail[j] == '.' || tail[j] == ')') {
            i = j + 1;
        } else {
            return 0;
        }
    }
    while i < tail.len() && (tail[i] == ' ' || tail[i] == '\t') {
        i += 1;
    }
    i
}

/// Turn the line/column pair already on a span into codepoint offsets.
pub(in crate::parse) fn apply_offsets(pos: &mut Pos, line_starts: &CodepointLineStarts) {
    restore_authored_columns(pos);
    if let Some(start) = line_starts.get(pos.start_line.saturating_sub(1)) {
        pos.start_offset = start + pos.start_column.saturating_sub(1);
    }
    if let Some(end) = line_starts.get(pos.end_line.saturating_sub(1)) {
        pos.end_offset = end + pos.end_column.saturating_sub(1);
    }
    if line_starts.has_leading_bom() {
        if pos.start_line == 1 {
            pos.start_column += 1;
        }
        if pos.end_line == 1 {
            pos.end_column += 1;
        }
    }
}

/// Fill offsets for a table: the rows' and cells' OWN spans, and the inline
/// content of the caption and of every cell.
///
/// Both halves in one place, because they were added a week apart and the
/// second nearly dropped the first: a span whose offsets are never filled stays
/// 0..0, which reads as present and selects nothing.
pub(in crate::parse) fn apply_table_offsets(table: &mut Table, line_starts: &CodepointLineStarts) {
    if let Some(caption) = &mut table.caption {
        apply_inline_offsets(caption, line_starts);
    }
    for row in &mut table.rows {
        if let Some(pos) = row.pos.as_mut() {
            apply_offsets(pos, line_starts);
        }
        for cell in &mut row.cells {
            if let Some(pos) = cell.pos.as_mut() {
                apply_offsets(pos, line_starts);
            }
            apply_inline_offsets(&mut cell.children, line_starts);
        }
    }
}

pub(in crate::parse) fn apply_inline_offsets(
    nodes: &mut [InlineNode],
    line_starts: &CodepointLineStarts,
) {
    for node in nodes {
        if let Some(pos) = inline_pos_mut(node) {
            apply_offsets(pos, line_starts);
        }
        match node {
            InlineNode::Emphasis(e) => apply_inline_offsets(&mut e.children, line_starts),
            InlineNode::Link(l) => apply_inline_offsets(&mut l.children, line_starts),
            InlineNode::Span(s) => apply_inline_offsets(&mut s.children, line_starts),
            InlineNode::Extension(e) => apply_inline_offsets(&mut e.children, line_starts),
            InlineNode::CitationGroup(g) => {
                for item in &mut g.items {
                    if let Some(pos) = &mut item.pos {
                        apply_offsets(pos, line_starts);
                    }
                    if let Some(prefix) = &mut item.prefix {
                        apply_inline_offsets(prefix, line_starts);
                    }
                    if let Some(locator) = &mut item.locator {
                        apply_inline_offsets(locator, line_starts);
                    }
                    if let Some(suffix) = &mut item.suffix {
                        apply_inline_offsets(suffix, line_starts);
                    }
                }
            }
            InlineNode::Footnote(f) => {
                if let Some(inline) = &mut f.inline {
                    apply_inline_offsets(inline, line_starts);
                }
            }
            InlineNode::CriticInsert(c) => apply_inline_offsets(&mut c.children, line_starts),
            InlineNode::CriticDelete(c) => apply_inline_offsets(&mut c.children, line_starts),
            InlineNode::CriticSubstitute(c) => {
                apply_inline_offsets(&mut c.old, line_starts);
                apply_inline_offsets(&mut c.new, line_starts);
            }
            _ => {}
        }
    }
}

pub(in crate::parse) fn owned_inline_pos(node: &InlineNode) -> Option<Pos> {
    node.pos().cloned()
}
