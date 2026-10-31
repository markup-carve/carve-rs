use super::*;

#[inline(never)]
pub(crate) fn boxed_list_node(list: List) -> Box<BlockNode> {
    Box::new(BlockNode::List(list))
}

pub(crate) fn marker_content_starts_block(
    content: &str,
    _cur: &LineCursor<'_>,
    _content_col: usize,
) -> bool {
    // Reference and footnote definitions are invisible blocks at a container's
    // content column. On a marker line they must enter the nested block parser,
    // not the lead-paragraph scanner (`r. [f]: t` is an empty list item).
    if is_collected_definition_placeholder(content)
        || parse_footnote_def_line(content).is_some()
        || parse_link_def_line(content).is_some_and(|(_, target)| !trim_ascii(target).is_empty())
    {
        return true;
    }
    // A thematic break as the marker-line content is a block (`1. ---` ->
    // <li><hr></li>), not inline text -- otherwise smart punctuation turns
    // `---` into an em-dash. Matches carve-js / carve-php.
    if detect_thematic_break(content) {
        return true;
    }
    // A heading WITH content as the marker-line first block (`- # H` ->
    // <li><h1>H</h1></li>), matching carve-js / carve-php. A heading is a single
    // line, so no multi-line close scan is needed. Bare `#`, a `# ` with no
    // content, or a tab (not the required space) stay inline text.
    if heading_content_starts(content) {
        return true;
    }
    // A fence is the item's FIRST content, so there is no open paragraph for it
    // to interrupt and the I4 closer lookahead does not apply: an unterminated
    // fence opens a code block that runs to the end, exactly as it does at the
    // top level and inside a block quote. Requiring a closer here made a list
    // item the one container that rendered it as inline verbatim, disagreeing
    // with carve-rs's own handling of the other two (carve-rs#458).
    if detect_fence_open(content).is_some() {
        return true;
    }
    // A COMMENT fence is an opener on the same terms as the code fence above
    // it: it OPENS, closer or no closer. This used to scan ahead for a closer
    // AT THE CONTENT COLUMN and fall through to the lead-paragraph path when it
    // found none, which made `- %%%` / `c` / `%%%` collect `c` as item text -
    // the item reaching past its own end for a body written at column 0, which
    // is the shape PART 1 S4 refuses (markup-carve/carve#1280). Its closer
    // travels with its opener: the item holds an empty comment and what follows
    // re-parses at document level, the same derivation `- ``` ` already gets
    // (§S4 A FENCED BODY IS NOT A PARAGRAPH) and the same output the quote
    // spelling `> %%%` already produced.
    if detect_comment_fence_line(content).is_some() {
        return true;
    }
    // A LINE comment, like the comment FENCE just above it. Left out, `- %% c`
    // routed to the lead-PARAGRAPH path, where the inline scanner consumed the
    // comment and left the item holding an EMPTY paragraph: the comment was
    // absent from the AST where carve-js publishes a `comment` node, the
    // canonical writer saw an item with no content and substituted the
    // CONTINUATION MARKER (`- +` - a different construct, one that takes a
    // body), and the empty paragraph rendered a blank line inside the item
    // (carve-rs#511 item 7, carve-rs#532).
    if trim_ascii_start(content).starts_with("%%") {
        return true;
    }
    let colon_fence_len = detect_container_open(content)
        .map(|open| open.fence_len)
        .or_else(|| detect_line_block_open(content))
        .or_else(|| detect_hardbreaks_block_open(content))
        .or_else(|| detect_quote_block_open(content));
    if colon_fence_len.is_some() {
        return true;
    }
    if is_table_start(content) {
        return true;
    }
    // A definition TERM. PART 9 §24 C3 names it in the same uniform block-opener
    // set as the branches above - "block quote, heading, thematic break, fenced
    // code, colon fence / admonition, TABLE, and DEFINITION LIST (a `:: term`
    // opener)" - and it was the one member missing here, so `* :: t` kept the
    // term as literal item text where carve-js and carve-php open a list.
    //
    // No lookahead: a term stands alone as a `<dl>` holding only a `<dt>`, and
    // `is_definition_list_start` already requires the `:: ` separator and
    // non-blank content, so `:::` (a div) and a bare `::` are both excluded.
    if is_definition_list_start(content) {
        return true;
    }
    false
}

/// The lead-paragraph scanner has no caption slot. A standalone image or
/// display equation followed by a caption goes through the block parser.
pub(crate) fn marker_content_is_captioned_inline(
    content: &str,
    cur: &LineCursor<'_>,
    content_col: usize,
    options: &Options<'_>,
) -> bool {
    if !content.starts_with("$$`") && !content.starts_with("![") {
        return false;
    }
    let blank_before_caption = cur.lines.line(cur.pos).is_some_and(is_blank_line);
    let caption_at = cur.pos + usize::from(blank_before_caption);
    let captioned = cur.lines.line(caption_at).is_some_and(|line| {
        indent_columns(line) == content_col
            && caption_content(&slice_columns(line, content_col, false)).is_some()
    });
    if !captioned {
        return content.starts_with("$$`")
            && multiline_math_takes_caption(content, cur, content_col, options);
    }
    if blank_before_caption && quiet_block_image(content).is_some() {
        return true;
    }
    let inline = parse_inline_with_options(content, options);
    inline.len() == 1
        && match &inline[0] {
            InlineNode::Image(_) => blank_before_caption,
            InlineNode::Math(math) => math.display,
            _ => false,
        }
}

/// The item's own lines up to the first caption are one display-math span.
fn multiline_math_takes_caption(
    content: &str,
    cur: &LineCursor<'_>,
    content_col: usize,
    options: &Options<'_>,
) -> bool {
    let mut joined = content.to_string();
    let mut at = cur.pos;
    loop {
        let Some(line) = cur.lines.line(at) else {
            return false;
        };
        if is_blank_line(line) {
            at += 1;
            let captioned = cur.lines.line(at).is_some_and(|next| {
                indent_columns(next) == content_col
                    && caption_content(&slice_columns(next, content_col, false)).is_some()
            });
            if !captioned {
                return false;
            }
            break;
        }
        if indent_columns(line) < content_col {
            return false;
        }
        let body = slice_columns(line, content_col, false);
        if caption_content(&body).is_some() {
            break;
        }
        joined.push('\n');
        joined.push_str(trim_ascii(&body));
        at += 1;
    }
    let inline = parse_inline_with_options(&joined, options);
    inline.len() == 1 && matches!(&inline[0], InlineNode::Math(math) if math.display)
}

/// Whether a valid standalone attribute block starts in a list marker's
/// content. Its continuation must reach the item's content column, exactly like
/// every other nested block. Validate the whole run before routing it through
/// the block path: an unclosed or malformed brace remains ordinary paragraph
/// text and therefore keeps the item open.
pub(crate) fn marker_content_is_attr_block(
    content: &str,
    cur: &LineCursor<'_>,
    content_col: usize,
) -> bool {
    matches!(
        content_column_attr_block(
            content,
            cur.lines.range(cur.pos..cur.lines.len()),
            content_col
        ),
        ContentColumnAttrBlock::Block
    )
}

pub(crate) enum ContentColumnAttrBlock {
    Block,
    NoneWithin(usize),
    No,
}

pub(crate) fn content_column_attr_block<'a>(
    content: &str,
    rest: impl Iterator<Item = &'a str>,
    content_col: usize,
) -> ContentColumnAttrBlock {
    if !trim_ascii(content).starts_with('{') {
        return ContentColumnAttrBlock::No;
    }
    if parse_standalone_attrs(content).is_some() {
        return ContentColumnAttrBlock::Block;
    }
    if trim_ascii_end(content).ends_with('}') {
        return ContentColumnAttrBlock::No;
    }

    let mut normalized = vec![content.to_string()];
    let mut met_closer = false;
    for line in rest {
        if is_blank_line(line) {
            break;
        }
        #[cfg(test)]
        ATTRS_LOOKAHEAD_BYTES.with(|work| work.set(work.get() + line.len()));
        // The opener's later lines belong to it at any column (carve#3421).
        let fragment = if indent_columns(line) < content_col {
            trim_ascii_start(line).to_string()
        } else {
            slice_columns(line, content_col, false)
        };
        if !valid_wrapped_attr_continuation(&fragment) {
            return ContentColumnAttrBlock::NoneWithin(normalized.len());
        }
        normalized.push(fragment);
        if trim_ascii_end(line).ends_with('}') {
            met_closer = true;
            break;
        }
    }
    let lines: Vec<&str> = normalized.iter().map(String::as_str).collect();
    match standalone_attrs_block_len(&lines) {
        Some(_) => ContentColumnAttrBlock::Block,
        // A closer was available but did not validate this opener. A later
        // opener may still form a valid block with that closer, so only the
        // current line has been answered.
        None if met_closer => ContentColumnAttrBlock::No,
        // No closer exists in the run. None of the lines scanned before its
        // boundary can open a valid wrapped attribute block either.
        None => ContentColumnAttrBlock::NoneWithin(normalized.len()),
    }
}

#[derive(Clone)]
pub(crate) struct ListMarker<'a> {
    pub(crate) indent: usize,
    pub(crate) ordered: bool,
    pub(crate) checked: Option<bool>,
    /// The authored task state, when it is not the default for `checked`.
    pub(crate) task_state: Option<char>,
    pub(crate) start: Option<usize>,
    pub(crate) ol_type: Option<OrderedListType>,
    pub(crate) content: &'a str,
    pub(crate) attrs: Option<Attrs>,
    /// Ordered-marker delimiter (`.` or `)`); `None` for bullets/tasks. A change
    /// in delimiter starts a new sibling list (§11).
    pub(crate) delim: Option<u8>,
    /// The raw ordered marker text (`i`, `iv`, `3`, `b`); used to re-classify an
    /// ambiguous single roman-letter via its sibling (§11 tie-break).
    pub(crate) marker: &'a str,
}

/// Coarse ordered-list dialect family for the §11 same-list test: decimal,
/// alphabetic, or roman (case included). A change splits the list.
#[derive(PartialEq, Eq, Clone, Copy)]
pub(crate) enum OlDialect {
    Decimal,
    Alpha(bool),
    Roman(bool),
}

pub(crate) fn ol_dialect(ol_type: Option<OrderedListType>) -> OlDialect {
    match ol_type {
        None => OlDialect::Decimal,
        Some(OrderedListType::LowerAlpha) => OlDialect::Alpha(false),
        Some(OrderedListType::UpperAlpha) => OlDialect::Alpha(true),
        Some(OrderedListType::LowerRoman) => OlDialect::Roman(false),
        Some(OrderedListType::UpperRoman) => OlDialect::Roman(true),
    }
}

/// Does an ordered `marker` keep the list's dialect (no §11 dialect split)? A
/// non-ambiguous marker must match the family exactly; an ambiguous single
/// roman-letter is compatible with EITHER a roman or an alpha list of the same
/// case (it continues as that dialect), but never a decimal list.
pub(crate) fn dialect_compatible(first: OlDialect, marker: &ListMarker<'_>) -> bool {
    if marker.marker.is_empty() {
        return first == OlDialect::Decimal;
    }
    if is_ambiguous_roman_letter(marker.marker) {
        let upper = marker.marker.chars().all(|c| c.is_ascii_uppercase());
        match first {
            OlDialect::Roman(u) | OlDialect::Alpha(u) => u == upper,
            OlDialect::Decimal => false,
        }
    } else {
        ol_dialect(marker.ol_type) == first
    }
}

/// The dialects an ordered marker can read as, each with its value.
fn ordered_dialects(m: &ListMarker<'_>) -> Vec<(OlDialect, usize)> {
    if is_ambiguous_roman_letter(m.marker) {
        let upper = m.marker.bytes().all(|b| b.is_ascii_uppercase());
        let letter = m.marker.as_bytes()[0].to_ascii_lowercase();
        let roman = match letter {
            b'i' => 1,
            b'v' => 5,
            b'x' => 10,
            b'l' => 50,
            b'c' => 100,
            b'd' => 500,
            _ => 1000,
        };
        return vec![
            (OlDialect::Roman(upper), roman),
            (OlDialect::Alpha(upper), usize::from(letter - b'a') + 1),
        ];
    }
    vec![(ol_dialect(m.ol_type), m.start.unwrap_or(1))]
}

/// PART 9 §11 N1: does `head` continue the list `first` opened, on every axis
/// (bullet character, ordered dialect and delimiter, plain or task)? An
/// ambiguous first marker resolves by the §11 tie-break against `head`.
pub(crate) fn same_list_axes(first: &ListMarker<'_>, head: &ListMarker<'_>) -> bool {
    if first.ordered != head.ordered {
        return false;
    }
    if !first.ordered {
        return first.marker == head.marker && first.checked.is_some() == head.checked.is_some();
    }
    if first.delim != head.delim {
        return false;
    }
    let heads = ordered_dialects(head);
    let mut list = ordered_dialects(first);
    if list.len() > 1 {
        let next = |d: (OlDialect, usize)| heads.iter().any(|h| h.0 == d.0 && h.1 == d.1 + 1);
        let (roman, alpha) = (list[0], list[1]);
        list = vec![if next(roman) {
            roman
        } else if next(alpha) || roman.1 != 1 {
            alpha
        } else {
            roman
        }];
    }
    list.iter().any(|d| heads.iter().any(|h| h.0 == d.0))
}

/// Is `m` a single roman-letter marker (i/v/x/l/c/d/m, either case)? Such a
/// marker is dialect-AMBIGUOUS: roman or alpha depending on its sibling (§11).
pub(crate) fn is_ambiguous_roman_letter(m: &str) -> bool {
    m.len() == 1
        && matches!(
            m.to_ascii_lowercase().as_str(),
            "i" | "v" | "x" | "l" | "c" | "d" | "m"
        )
}

/// The column width of the `{...}` attribute block abutting `marker` on
/// `line`, or 0 when it carries none.
///
/// This is measured only so callers can remove metadata from the physical
/// prefix width: item attributes do not move the content column
/// (markup-carve/carve#1701). `marker` excludes an ordered delimiter, so skip
/// that delimiter before looking for the abutting block.
pub(crate) fn marker_attrs_width(line: &str, marker: &ListMarker<'_>) -> usize {
    let bytes = line.as_bytes();
    let marker_end = (marker.marker.as_ptr() as usize).saturating_sub(line.as_ptr() as usize)
        + marker.marker.len()
        + usize::from(marker.ordered);
    read_list_item_attrs(bytes, marker_end).map_or(0, |(_, end)| end - marker_end)
}

/// Visual column (tab-aware) at which a list ITEM's continuation content begins,
/// mirroring `parse_list` exactly: for ordered/unordered it is where the marker
/// content begins (`- ` -> 2, `1. ` -> 3, `10. ` -> 4); for a TASK the checkbox
/// counts as content, not marker, so the column is the bullet width (`- ` -> 2)
/// An abutting attribute block is item metadata and contributes zero
/// (`-{#k} [x] ` -> 2, markup-carve/carve#1701).
/// Returns `None` when `line` is not a list marker.
pub(crate) fn marker_content_col(line: &str) -> Option<usize> {
    marker_content_col_at(line, indent_columns(line))
}

/// `marker_content_col` for a caller holding the line's indentation already.
pub(crate) fn marker_content_col_at(line: &str, indent: usize) -> Option<usize> {
    let m = detect_list_marker_full_at(line, indent)?;
    if m.checked.is_some() {
        return Some(m.indent + 2);
    }
    let content_off = (m.content.as_ptr() as usize).saturating_sub(line.as_ptr() as usize);
    let column = indent
        + content_off
            .saturating_sub(leading_ws(line))
            .saturating_sub(marker_attrs_width(line, &m));
    Some(past_lead_tab_run(m.content, column, m.indent))
}

/// CARVE-P2-010: a tab that begins an item's content is leading whitespace, so
/// the content column is where the content starts. Tab stops count from
/// `origin`, the column the marker's own frame starts at (PART 9 §24 C5).
pub(crate) fn past_lead_tab_run(content: &str, column: usize, origin: usize) -> usize {
    if !content.starts_with('\t') || content[1..].starts_with("%%%") {
        return column;
    }
    let mut end = column;
    for b in content.bytes() {
        match b {
            b' ' => end += 1,
            b'\t' => end = origin + ((end - origin) / 4 + 1) * 4,
            _ => break,
        }
    }
    end
}

/// The content column a definition body marker opens, or `None` when the line
/// carries no `:` body marker.
///
/// The list twin is [`marker_content_col`]. A definition description is a
/// container in exactly the same sense - PART 9 §24 C3 asks the base question
/// of the INNERMOST container open where a line is written - and this is the
/// column that decides which one that is. The separator is a RUN of spaces and
/// its width sets the column (markup-carve/carve#1757), so the answer is read
/// from the marker rather than assumed.
pub(crate) fn definition_body_content_col(line: &str) -> Option<usize> {
    let indent = indent_columns(line);
    let local = strip_leading_columns(line, indent);
    let (_, width) = strip_definition_marker(&local)?;
    Some(indent + width)
}

/// The content column of the deepest marker in a marker-line list run,
/// relative to the first marker's content.
pub(crate) fn innermost_marker_content_col(line: &str) -> usize {
    innermost_marker_tail(line).0
}

pub(crate) fn paragraph_owner_column(line: &str, base: usize) -> Option<usize> {
    if let Some(marker) = detect_list_marker_full(line) {
        let column = base + marker_content_col(line)?;
        return paragraph_owner_column(marker.content, column);
    }
    if let Some(column) = definition_body_content_col(line) {
        let body = strip_definition_marker(trim_ascii_start(line))?.0;
        return paragraph_owner_column(body, base + column);
    }
    (detect_fence_open(trim_ascii_start(line)).is_none()
        && !is_definition_list_start(trim_ascii_start(line))
        && ParaOpen::from_line(line, false).get())
    .then_some(base)
}

pub(crate) fn is_reference_definition(line: &str) -> bool {
    parse_link_def_line(line).is_some()
        || parse_footnote_def_line(line).is_some()
        || is_definition_placeholder(line)
}

pub(crate) fn innermost_marker_tail(mut line: &str) -> (usize, &str) {
    let mut column = 0;
    while let Some(marker) = detect_list_marker_full(line) {
        #[cfg(test)]
        INNERMOST_MARKER_STEPS.with(|count| count.set(count.get() + 1));
        let Some(next) = marker_content_col(line) else {
            break;
        };
        column += next;
        line = marker.content;
    }
    (column, line)
}

pub(crate) fn innermost_marker_column(mut line: &str) -> Option<usize> {
    let mut content_column = 0;
    let mut marker_column = None;
    while let Some(marker) = detect_list_marker_full(line) {
        marker_column = Some(content_column + marker.indent);
        content_column += marker_content_col(line)?;
        line = marker.content;
    }
    marker_column
}

pub(crate) fn innermost_marker_content(mut line: &str) -> &str {
    #[cfg(test)]
    INNERMOST_CONTENT_SCANS.with(|count| count.set(count.get() + 1));
    while let Some(marker) = detect_list_marker_full(line) {
        line = marker.content;
    }
    line
}

/// Whether `line` (in the dedented sub-list coordinate space) begins a plain
/// PARAGRAPH rather than a block opener. Any indented line is paragraph text
/// under the strict column-0 rule; a flush-left line is a paragraph only when it
/// matches none of the block openers.
pub(crate) fn line_starts_paragraph(line: &str) -> bool {
    if is_blank_line(line) {
        return false;
    }
    if line.starts_with([' ', '\t']) {
        // An indented line is outer-item paragraph content ONLY if it does not
        // open a block. An indented sibling marker belongs to the nested list,
        // not to the outer item, so treating it as paragraph content propagated
        // the nested list's looseness outwards - which PART 9 section 17 says
        // it must not (corpus 142). The 2-space form dedents to column 0 and
        // reached the marker check below; the 4-space form kept its indent and
        // short-circuited here.
        // ...but only a LIST MARKER disqualifies it. Unordered and task markers
        // nest at any indent, so an indented sibling marker belongs to the
        // nested list. Every other opener needs its own column, so an indented
        // `> q` or `# h` is literal paragraph text and DOES loosen (corpus 160).
        return detect_list_marker_full(line).is_none();
    }
    detect_heading(line).is_none()
        && !detect_thematic_break(line)
        && !line.starts_with('>')
        && detect_fence_open(line).is_none()
        && detect_container_open(line).is_none()
        && detect_line_block_open(line).is_none()
        && detect_hardbreaks_block_open(line).is_none()
        && detect_quote_block_open(line).is_none()
        && detect_comment_fence_line(line).is_none()
        && !is_table_start(line)
        && !is_definition_list_start(line)
        && detect_list_marker_full(line).is_none()
        // A flush-left block-attribute line floats forward to the block below
        // it (§15), which is exactly why `interrupts_paragraph` lets it end an
        // open paragraph. Missing it here is the same omission read from the
        // other side: the writer asks this function whether a child's first
        // line would FOLD into the paragraph above, and answered `{.x}` with
        // yes. It then wrote the child behind a `+` at the item's MARKER
        // column, where a following `- b` is a SIBLING item rather than the
        // nested list the attributes were reaching for, so
        // `- a` / `  {.x}` / `  - b` came back as two items and a literal
        // `{.x}` (corpus 322/323). The indented branch above is deliberately
        // untouched: under the strict column-0 rule an INDENTED attr line is
        // lazy paragraph text, not a floater.
        && parse_standalone_attrs(line).is_none()
        && !trim_ascii_start(line).starts_with("%%")
}

pub(crate) fn sublist_source_trailing_invisible_separation(source: &str) -> bool {
    let Some(inner_content_col) = source.lines().find_map(marker_content_col) else {
        return false;
    };
    let lines: Vec<_> = source.lines().collect();
    // Pair exact-width comment delimiters once. An unmatched opener is one
    // line comment; its following lines are classified normally.
    let mut next_fence = std::collections::HashMap::new();
    let mut partner = vec![None; lines.len()];
    for at in (0..lines.len()).rev() {
        if let Some(fence) = detect_comment_fence_line_any_column(lines[at]) {
            partner[at] = next_fence.insert(fence.fence_len, at);
        }
    }
    let mut blank_since_visible = false;
    let mut invisible_since_blank = false;
    let mut at = 0;
    while at < lines.len() {
        let line = lines[at];
        if is_blank_line(line) {
            blank_since_visible = true;
            at += 1;
            continue;
        }
        let comment_end = partner[at];
        let invisible = (is_line_comment_any_column(line)
            || detect_comment_fence_line_any_column(line).is_some()
            || is_definition_placeholder(line))
            && indent_columns(line) < inner_content_col;
        if invisible {
            invisible_since_blank = blank_since_visible;
            at = comment_end.map_or(at + 1, |end| end + 1);
        } else {
            invisible_since_blank = false;
            blank_since_visible = false;
            at += 1;
        }
    }
    invisible_since_blank
}

pub(crate) fn detect_list_marker_full(line: &str) -> Option<ListMarker<'_>> {
    detect_list_marker_full_at(line, indent_columns(line))
}

/// `detect_list_marker_full` for a caller that has already measured the line's
/// indentation. Every scan below starts past that run, so re-measuring it is
/// pure repetition - and a nested list's line is scanned once per enclosing
/// item, which is what made that repetition cost the line's whole indent again
/// at every level (carve-rs#2301).
pub(crate) fn detect_list_marker_full_at(line: &str, indent: usize) -> Option<ListMarker<'_>> {
    let trimmed = trim_ascii_start(line);
    let bytes = trimmed.as_bytes();
    // Plain ASCII bullets need no task, ordered-marker, or attribute checks.
    // Keep decorated markers on the general path.
    if matches!(bytes.first(), Some(b'-' | b'*'))
        && bytes.get(1) == Some(&b' ')
        && bytes.get(2).is_some_and(u8::is_ascii_alphanumeric)
    {
        let marker_start = line.len() - trimmed.len();
        return Some(ListMarker {
            indent,
            ordered: false,
            checked: None,
            task_state: None,
            start: None,
            ol_type: None,
            content: trim_ascii_end(&trimmed[2..]),
            attrs: None,
            delim: None,
            marker: &line[marker_start..marker_start + 1],
        });
    }
    if let Some(task) = detect_task(line) {
        return Some(ListMarker {
            indent,
            ordered: false,
            checked: Some(task.checked),
            task_state: task.task_state,
            start: None,
            ol_type: None,
            content: task.content,
            attrs: task.attrs,
            delim: None,
            marker: task.marker,
        });
    }
    if let Some((content, start, ol_type, attrs, delim, marker)) = detect_ordered_full(line) {
        return Some(ListMarker {
            indent,
            ordered: true,
            checked: None,
            task_state: None,
            start,
            ol_type,
            content,
            attrs,
            delim: Some(delim),
            marker,
        });
    }
    if let Some((content, attrs, marker)) = detect_unordered(line) {
        return Some(ListMarker {
            indent,
            ordered: false,
            checked: None,
            task_state: None,
            start: None,
            ol_type: None,
            content,
            attrs,
            delim: None,
            marker,
        });
    }
    None
}

/// S4's lazy question for a body collected at a list item's CONTENT COLUMN:
/// does a following flush-left line fold into it?
///
/// DEPTH IS NOT A PARAMETER (carve-rs#1025). When the body's last line is a
/// SUB-ITEM's marker line, the innermost container's last block is that
/// marker's CONTENT, so S4's question is asked of it directly - the same
/// question `- # H` / `tail` answers one level up (markup-carve/carve#1280).
/// Without this the two levels disagree: the sub-item refuses the line on
/// re-parse while this collector has already claimed it, and the line lands in
/// the OUTER item, which is a third answer no engine produces.
///
/// A heading at the sub-item's CONTENT COLUMN leaves no paragraph open, and so
/// the line ends the inner item AND the list: a below-column line continues a
/// paragraph only where one is open at the deepest frame, and no surviving
/// frame adopts it (CARVE-P0-009, markup-carve/carve#2884). An arm that folded
/// such a line into the OUTER item used to live here.
/// Does the body collected so far end INSIDE a quote's lazy continuation?
///
/// A QUOTE IS REACHED BY ITS MARKER, AND A COLUMN NEVER REACHES INTO ONE
/// (`01-layout.ebnf:330`). A marker line carrying no `>` is in no quote at
/// whatever column it lands on, so CARVE-P0-020 does not govern it and the only
/// route by which it touches the quote is PART 0's lazy fold - which is what the
/// PARAGRAPH twin has always done. This makes the marker twin agree with it
/// (markup-carve/carve#1905, markup-carve/carve-rs#1537).
///
/// Answered on the LINES rather than on a parse: the plain collector carries no
/// `Options`, and a probe parse there would have to invent one. Walking back
/// over the trailing non-blank run is also what the rule actually says - a blank
/// is the only exit, and prose in between stays inside the fold.
pub(crate) fn collected_body_ends_in_a_quote_fold(lines: &[String]) -> bool {
    quote_fold_at_end(lines.iter().map(String::as_str).rev())
}

pub(crate) fn quote_fold_at_end<'a>(lines: impl Iterator<Item = &'a str>) -> bool {
    for line in lines {
        if is_blank_line(line) {
            return false;
        }
        let rest = trim_ascii_start(strip_lazy(line));
        if rest.starts_with('>') {
            return true;
        }
        // A visible opener ends the fold, so what is below it is not the
        // quote's any more.
        if item_block_opener(rest) {
            return false;
        }
    }
    false
}

pub(crate) fn collected_body_takes_the_lazy_line(
    src: &str,
    trailing_below_column: bool,
    options: &Options<'_>,
) -> bool {
    if let Some(content) = trailing_marker_line_content(src) {
        return body_ends_with_open_paragraph(&content, options);
    }
    nested_ends_with_open_paragraph(src, trailing_below_column, trailing_below_column, options)
}

/// A wrapped attribute that reached this collector's item column applies one
/// attribute-line transition across its closer, even when it appeared below a
/// deeper child's column. The re-parsed string has lost that reached-column
/// fact, so the lazy probe must use the collector's parallel reach map.
pub(crate) fn reached_wrapped_attribute_tail(src: &MappedSource) -> bool {
    let lines: Vec<_> = src.source.lines().collect();
    let Some(end) = lines.iter().rposition(|line| !is_blank_line(line)) else {
        return false;
    };
    let trimmed: Vec<_> = lines.iter().map(|line| trim_ascii_start(line)).collect();
    (0..=end).rev().any(|start| {
        src.reached.get(start) == Some(&true)
            && trimmed[start].starts_with('{')
            && standalone_attrs_block_len(&trimmed[start..=end]) == Some(end - start + 1)
    })
}

pub(crate) fn body_ends_in_definition_list(body: &str, options: &Options<'_>) -> bool {
    #[cfg(test)]
    BODY_ENDS_DEFINITION_PROBE_BYTES.with(|work| work.set(work.get() + body.len() as u64));
    let last = probe_blocks(body, options).pop();
    // A flush separator also leaves a definition list that ends a nested item
    // (CARVE-P2-017, carve#3473). Only a term or separator line ends that list:
    // a lazy line below it may belong to an outer block instead.
    let nested_tail = body
        .lines()
        .rev()
        .find(|line| !is_blank_line(line))
        .map(trim_ascii_start)
        .is_some_and(|line| {
            is_definition_list_start(line) || strip_description_prefix(line).is_some()
        });
    ends_in_definition_list(last.as_ref(), nested_tail)
}

fn ends_in_definition_list(block: Option<&BlockNode>, nested: bool) -> bool {
    match block {
        Some(BlockNode::DefinitionList(_)) => true,
        Some(BlockNode::Div(div)) if nested => ends_in_definition_list(div.children.last(), nested),
        Some(BlockNode::Admonition(boxed)) if nested => {
            ends_in_definition_list(boxed.children.last(), nested)
        }
        Some(BlockNode::Directive(directive)) if nested => {
            ends_in_definition_list(directive.children.last(), nested)
        }
        Some(BlockNode::List(list)) if nested => list
            .items
            .last()
            .is_some_and(|item| ends_in_definition_list(item.children.last(), nested)),
        _ => false,
    }
}

#[cfg(test)]
mod description_separator_probe_work {
    use super::BODY_ENDS_DEFINITION_PROBE_BYTES;

    fn work_for(count: usize, repeated_items: bool) -> (usize, u64) {
        let mut source = String::new();
        if repeated_items {
            for index in 0..count {
                source.push_str(&format!("- item {index}\n  :: term\n: tail\n"));
            }
        } else {
            source.push_str("- item\n");
            for index in 0..count {
                source.push_str(&format!("  :: term {index}\n   tail\n"));
            }
            source.push_str(": tail\n");
        }
        BODY_ENDS_DEFINITION_PROBE_BYTES.with(|work| work.set(0));
        let _ = crate::to_html(&source);
        let bytes = BODY_ENDS_DEFINITION_PROBE_BYTES.with(|work| work.get());
        (source.len(), bytes)
    }

    fn repeated_separator_work(count: usize) -> (usize, u64) {
        let mut source = String::from("- :: term\n");
        for index in 0..count {
            source.push_str(&format!(": tail\n  :: term {index}\n"));
        }
        BODY_ENDS_DEFINITION_PROBE_BYTES.with(|work| work.set(0));
        let _ = crate::to_html(&source);
        let bytes = BODY_ENDS_DEFINITION_PROBE_BYTES.with(|work| work.get());
        (source.len(), bytes)
    }

    #[test]
    fn definition_tail_probes_have_bounded_work() {
        for repeated_items in [false, true] {
            let mut measurements = Vec::new();
            for count in [32, 64, 128, 256] {
                let (input, probed) = work_for(count, repeated_items);
                assert!(
                    probed <= (input * 4) as u64,
                    "items={repeated_items} count={count}: {probed} probe bytes for {input} input bytes"
                );
                measurements.push(probed);
            }
            assert!(
                measurements[3] <= measurements[0] * 12,
                "items={repeated_items}: 8x input used more than 12x probe bytes: {measurements:?}"
            );
        }
        let measurements =
            [32, 64, 128, 256]
                .map(repeated_separator_work)
                .map(|(input, probed)| {
                    assert!(probed > 0, "repeated separators did not reach the probe");
                    assert!(probed <= (input * 4) as u64);
                    probed
                });
        assert!(
            measurements[3] <= measurements[0] * 12,
            "repeated separators: 8x input used more than 12x probe bytes: {measurements:?}"
        );
    }
}

/// Whether the last consumed content belongs below this container's column.
/// A comment closer inherits its opener's column (CARVE-P9-053). The cursor
/// keeps that column even when position tracking is disabled.
pub(crate) fn last_consumed_line_below_column(cur: &LineCursor, content_col: usize) -> bool {
    if content_col == 0 || cur.pos == 0 {
        return false;
    }
    let column = cur
        .closed_comment_location
        .filter(|(closer, _)| *closer == cur.pos - 1)
        .map_or_else(
            || indent_columns(cur.lines.at(cur.pos - 1)),
            |(_, column)| column,
        );
    column < content_col
}

/// The marker-line CONTENT of a collected body whose last line opens a list
/// item, if that is what its last line is.
///
/// `- a` / `  - # N` collects `a` / `- # N`, and the block a following
/// flush-left line would have to fold into is the sub-item's - which is the
/// marker's content, `# N`. A body whose last line is anything else (a line at
/// the sub-item's content column, a paragraph, a fence) is not this shape.
pub(crate) fn trailing_marker_line_content(body: &str) -> Option<String> {
    let last = body.lines().rev().find(|line| !is_blank_line(line))?;
    let marker = detect_list_marker_full(last)?;
    Some(marker.content.to_string())
}

#[cfg(test)]
thread_local! {
    pub(crate) static PARAGRAPH_BODY_PROBES: Cell<usize> = const { Cell::new(0) };
    /// Calls into `parse_blocks`, the nested parse every container body takes.
    pub(crate) static BLOCK_SEQUENCE_PARSES: Cell<usize> = const { Cell::new(0) };
    pub(crate) static BODY_ENDS_DEFINITION_PROBE_BYTES: Cell<u64> = const { Cell::new(0) };
}

/// PART 1 S4's one question, asked of a container's WHOLE body: does the last
/// block leave a paragraph open?
///
/// [`nested_ends_with_open_paragraph`] answers the same question for a body
/// collected BESIDE a marker line that is still holding a paragraph of its own,
/// which is why it looks PAST a trailing run of comments: there the open
/// paragraph is the marker line's, and an invisible block did not close it.
/// Here the body is all there is - `- %% c` writes the comment as the item's
/// first and only block, so there is no earlier paragraph for it to leave open,
/// and looking past it would find one that was never written
/// (markup-carve/carve#1280).
pub(crate) fn body_ends_with_open_paragraph(body: &str, options: &Options<'_>) -> bool {
    if body_ends_with_an_attribute_line(body)
        || quoted_body_ends_with_an_invisible_line(body)
        || body
            .lines()
            .rev()
            .find(|line| !is_blank_line(line))
            .is_some_and(|line| is_collected_definition_placeholder(trim_ascii_start(line)))
    {
        return false;
    }
    // Resolve one-line quote/list chains from their terminal content. A full
    // probe at every prefix otherwise repeats the same nested parses.
    if options.extensions.is_empty() && !body.contains(['\n', '\r']) {
        let mut content = body;
        let mut containers = 0;
        loop {
            if let Some(rest) = strip_blockquote_prefix(content) {
                content = rest;
            } else if let Some(marker) = detect_list_marker_full(content) {
                content = marker.content;
            } else {
                break;
            }
            containers += 1;
        }
        if NESTING_DEPTH.with(|depth| depth.get() + containers + 1 < MAX_NESTING_DEPTH)
            && (containers > 0 && is_blank_line(content)
                || detect_heading(content).is_some()
                || detect_thematic_break(content)
                || detect_fence_open(content).is_some()
                || is_table_start(content)
                || is_flush_line_comment(content)
                || detect_comment_fence_line(content).is_some()
                || parse_standalone_attrs(content).is_some()
                || detect_container_open(content).is_some())
        {
            return false;
        }
        if line_starts_paragraph(content)
            && NESTING_DEPTH.with(|depth| depth.get() + containers + 1 < MAX_NESTING_DEPTH)
        {
            return true;
        }
    }
    #[cfg(test)]
    PARAGRAPH_BODY_PROBES.with(|count| count.set(count.get() + 1));
    let blocks = probe_blocks(body, options);
    block_ends_with_open_paragraph(blocks.last(), colon_fences_left_open(body), false)
}

/// Whether a body's last line is a definition or attribute line written
/// inside a quote. It leaves no paragraph open in the quote (CARVE-P0-011,
/// CARVE-P0-012), so the plain check, which sees the `>` first, misses it. A
/// definition at the quote's own column leaves the document-column
/// placeholder, which here is still inside the quote.
pub(crate) fn quoted_body_ends_with_an_invisible_line(body: &str) -> bool {
    let Some(last) = body.lines().rev().find(|line| !is_blank_line(line)) else {
        return false;
    };
    if strip_blockquote_prefix(trim_ascii_start(last)).is_none() {
        return false;
    }
    // The innermost quote's residue, with its depth.
    let residue = |line: &str| -> (usize, String) {
        let mut local = trim_ascii_start(line);
        // An item marker line reaches the quote it opens.
        while strip_blockquote_prefix(local).is_none() {
            match detect_list_marker_full(local) {
                Some(marker) => local = trim_ascii_start(marker.content),
                None => break,
            }
        }
        if strip_blockquote_prefix(local).is_none() {
            local = trim_ascii_start(line);
        }
        let mut depth = 0;
        while let Some(rest) = strip_blockquote_prefix(local) {
            depth += 1;
            local = rest;
            if !trim_ascii_start(rest).starts_with('>') {
                break;
            }
            local = trim_ascii_start(rest);
        }
        (depth, local.to_string())
    };
    let (depth, last_residue) = residue(last);
    let past = indent_columns(&last_residue);
    // A quote does not rebase (CARVE-P0-004): past its column the line is
    // paragraph text unless a container inside the quote owns that column.
    if past > 0 {
        let owned = body
            .lines()
            .rev()
            .skip_while(|line| *line != last)
            .skip(1)
            .any(|line| {
                let (at, local) = residue(line);
                at == depth
                    && indent_columns(&local) < past
                    && (marker_content_col(&local).is_some()
                        || definition_body_content_col(&local).is_some())
            });
        if !owned {
            return false;
        }
    }
    let dequote = |line: &str| trim_ascii_start(&residue(line).1).to_string();
    if is_definition_placeholder(&dequote(last)) {
        return true;
    }
    let dequoted: Vec<String> = body.lines().map(dequote).collect();
    body_ends_with_an_attribute_line(&dequoted.join("\n"))
}

/// Whether a list item still holds an open paragraph, probed only when a later
/// line asks.
///
/// The probe re-parses the item's collected body, and a sub-list in that body
/// probes its own body inside the probe, so answering it for every item parsed
/// a nested ladder's body twice per level: 2^depth parses. Most items are
/// followed by nothing that asks - a sibling marker at the list's column, a
/// blank line, the end of the source - and those never pay for it.
pub(crate) enum ParagraphOpen<'a> {
    Known(bool),
    Deferred(Option<Box<dyn FnOnce() -> bool + 'a>>),
}

impl<'a> ParagraphOpen<'a> {
    pub(crate) fn deferred(probe: impl FnOnce() -> bool + 'a) -> Self {
        Self::Deferred(Some(Box::new(probe)))
    }

    pub(crate) fn get(&mut self) -> bool {
        if let Self::Deferred(probe) = self {
            let value = probe.take().is_some_and(|probe| probe());
            *self = Self::Known(value);
        }
        matches!(self, Self::Known(true))
    }
}

/// Whether the collected nested block ends in an OPEN paragraph -- i.e. its
/// last block is a paragraph, or a container (block quote / div / admonition)
/// whose last child recursively ends in a paragraph. CommonMark lazy
/// continuation folds a following dedented non-blank line into the deepest open
/// paragraph: when a list item's last block is a block quote whose trailing
/// block is a paragraph, the dedented line is the quote's own lazy continuation
/// and must stay INSIDE the item rather than ending it. A code block or table
/// has no open paragraph, so it does NOT fold (the dedented line ends the item).
/// [`nested_ends_with_open_paragraph`] on a collected body with its refused
/// marker tail masked.
pub(crate) fn nested_ends_with_open_paragraph_masked(
    nested: &MappedSource,
    trailing_below_column: bool,
    below_outer_column: bool,
    options: &Options<'_>,
) -> bool {
    nested_ends_with_open_paragraph(
        &nested.source,
        trailing_below_column,
        below_outer_column,
        options,
    )
}

pub(crate) fn nested_ends_with_open_paragraph(
    nested: &str,
    trailing_below_column: bool,
    below_outer_column: bool,
    options: &Options<'_>,
) -> bool {
    // The fragment probe loses its marker host. A term at the end of an open
    // colon body is a block there, so a following flush-left line cannot fold
    // into a paragraph inside that body.
    if !trailing_below_column
        && colon_fences_left_open(nested) > 0
        && nested
            .lines()
            .rev()
            .find(|line| !is_blank_line(line))
            .is_some_and(|line| {
                !line.starts_with(LAZY) && is_definition_list_start(trim_ascii_start(line))
            })
    {
        return false;
    }
    // A wrapped attribute is one construct wherever its closer sits (carve#3421), so a
    // closer below the column still ends the body on an attribute block.
    let wrapped_attribute_tail = || {
        let lines: Vec<&str> = nested.lines().collect();
        let Some(end) = lines.iter().rposition(|line| !is_blank_line(line)) else {
            return false;
        };
        if parse_standalone_attrs(trim_ascii_start(lines[end])).is_some() {
            return false;
        }
        let trimmed: Vec<&str> = lines.iter().map(|line| trim_ascii_start(line)).collect();
        // The opener must sit at a container's column: the body's own, or a nested
        // item's content column. Past it, `{.c` is paragraph text.
        let Some(start) = (0..end)
            .rev()
            .take_while(|&at| !is_blank_line(lines[at]))
            .find(|&at| standalone_attrs_block_len(&trimmed[at..=end]) == Some(end - at + 1))
        else {
            return false;
        };
        let indent = indent_columns(lines[start]);
        indent == 0
            || lines[..start].iter().rev().any(|line| {
                detect_list_marker_full(trim_ascii_start(line)).is_some_and(|marker| {
                    indent_columns(line)
                        + (marker.content.as_ptr() as usize
                            - trim_ascii_start(line).as_ptr() as usize)
                        == indent
                })
            })
    };
    if below_outer_column && wrapped_attribute_tail() {
        return false;
    }
    if !below_outer_column
        && (body_ends_with_an_attribute_line(nested)
            || quoted_body_ends_with_an_invisible_line(nested)
            || nested
                .lines()
                .rev()
                .find(|line| !is_blank_line(line))
                .is_some_and(|line| is_collected_definition_placeholder(trim_ascii_start(line))))
    {
        return false;
    }
    let blocks = probe_blocks(nested, options);
    // Rule A (carve#3885): a comment interrupts the paragraph above it.
    let Some(last) = blocks.last() else {
        return false;
    };

    block_ends_with_open_paragraph(
        Some(last),
        colon_fences_left_open(nested),
        below_outer_column,
    )
}

/// How many colon fences does this collected body end INSIDE, unclosed?
///
/// A colon fence closes on an EXACT length match, so the widths are tracked as
/// a stack rather than a count. Only what an opener actually opens is pushed:
/// a `:::`-shaped line that fails the opener test is absorbed paragraph text
/// and opens nothing (PART 7's separator narrowing, carve#900/#905), which is
/// the reading that made `:::note` stop being a container.
///
/// The DEPTH is what the caller needs, not just "any". An unterminated fence can
/// only be the last block at its level, so the open ones are exactly the last N
/// containers on the last-child chain, and a CLOSED container nested inside an
/// open one must still read as closed.
///
/// A VERBATIM BODY OPENS NOTHING: a colon-shaped line inside a code fence is
/// code text and inside a LINE BLOCK is verse text, so this scan carries both
/// bodies' state. A line block's body is where such a line is MOST likely to be
/// content, which is what makes the omission wrong rather than optimistic.
pub(crate) fn colon_fences_left_open(nested: &str) -> usize {
    let mut open: Vec<(usize, usize, usize)> = Vec::new();
    let mut code: Option<FenceOpen> = None;
    let lines: Vec<_> = nested
        .lines()
        .map(|line| {
            let mut local = line;
            while let Some(rest) = strip_blockquote_prefix(trim_ascii_start(local)) {
                local = rest;
            }
            local
        })
        .collect();
    // Longest bare backtick and tilde run on any later line.
    let mut closer_after = (vec![0usize; lines.len()], vec![0usize; lines.len()]);
    let mut longest = (0usize, 0usize);
    for (index, line) in lines.iter().enumerate().rev() {
        closer_after.0[index] = longest.0;
        closer_after.1[index] = longest.1;
        let run = trim_ascii(line);
        let width = run.len();
        if width >= 3 && run.bytes().all(|b| b == b'`') {
            longest.0 = longest.0.max(width);
        } else if width >= 3 && run.bytes().all(|b| b == b'~') {
            longest.1 = longest.1.max(width);
        }
    }
    let mut host_floor = 0;
    let mut verse_end = 0;
    let mut closer_index = None;
    let mut comment_closers = None;
    for (index, line) in lines.iter().copied().enumerate() {
        let trimmed = trim_ascii_start(line);
        let (offset, content) = if index == 0 || (code.is_none() && index >= verse_end) {
            innermost_marker_tail(trimmed)
        } else {
            (0, trimmed)
        };
        let column = indent_columns(line) + offset;
        if offset > 0 {
            host_floor = column;
        }
        let trimmed = content;
        if index < verse_end {
            continue;
        }
        if let Some(fence) = code {
            if is_fence_close(trimmed, fence) {
                code = None;
            }
            continue;
        }
        if let Some(fence) = detect_fence_open(trimmed) {
            // With no closer below, the run is paragraph text and hides no
            // colon closer (CARVE-P0-014).
            let closer = if fence.fence_char == b'`' {
                &closer_after.0
            } else {
                &closer_after.1
            };
            if closer[index] >= fence.fence_len {
                code = Some(fence);
            }
            continue;
        }
        if let Some(fence_len) = detect_line_block_open(trimmed) {
            verse_end = find_colon_fence_end(
                CursorLines::Slice(&lines),
                index,
                fence_len,
                true,
                &mut closer_index,
                &mut comment_closers,
            );
            if verse_end == lines.len()
                && exact_colon_fence_len(trim_ascii_start(lines[verse_end - 1])) != Some(fence_len)
            {
                return open.len() + 1;
            }
            continue;
        }
        if let Some(len) = exact_colon_fence_len(trimmed) {
            if let Some(&(width, floor, opener_column)) = open.last() {
                if width == len && (column == floor || column == opener_column) {
                    open.pop();
                    continue;
                }
                if column != floor && column != opener_column {
                    continue;
                }
            }
        }
        if let Some(container) = detect_container_open(trimmed) {
            open.push((container.fence_len, host_floor, column));
        }
    }
    open.len()
}

/// The last child that could hold the open paragraph.
///
/// §24 C3's comment exception has to reach every level, not only the outermost:
/// a comment written below the content column of a NESTED item is not that
/// item's block either, and reading it as the last one answered "the paragraph
/// is closed" for a container that never saw a block at all
/// (markup-carve/carve-rs#1516).
pub(crate) fn last_open_child(children: &[BlockNode], skip_comments: bool) -> Option<&BlockNode> {
    if skip_comments {
        children
            .iter()
            .rev()
            .find(|block| !matches!(block, BlockNode::Comment(_)))
    } else {
        children.last()
    }
}

pub(crate) fn block_ends_with_open_paragraph(
    block: Option<&BlockNode>,
    colon_open: usize,
    skip_comments: bool,
) -> bool {
    match block {
        Some(BlockNode::Paragraph(_)) => true,
        // A blockquote has no explicit closer: lazy continuation keeps its
        // trailing paragraph open, so a dedented line folds into it.
        Some(BlockNode::BlockQuote(q)) => block_ends_with_open_paragraph(
            last_open_child(&q.children, skip_comments),
            colon_open,
            skip_comments,
        ),
        // A list's last item can hold an open paragraph (the deepest open
        // paragraph a dedented line continues, e.g. a sub-list item's text).
        Some(BlockNode::List(l)) => block_ends_with_open_paragraph(
            l.items
                .last()
                .and_then(|it| last_open_child(&it.children, skip_comments)),
            colon_open,
            skip_comments,
        ),
        // A definition list has no explicit closer either: its last item stays
        // open -- a term still awaiting its `:  ` definition, or a definition
        // whose body ends in a paragraph. A following flush-left `:  ` line (at
        // any column at or below the term) attaches as a `<dd>`, and lazy body
        // text folds into the open definition. This is the lenient def-attach
        // rule shared with carve-php / carve-js: a definition marker is not
        // subject to the column-0-exits rule that ends a list item.
        Some(BlockNode::DefinitionList(dl)) => match dl.items.last() {
            None => false,
            // Bare term, no definition yet: open (awaiting `:  def`).
            Some(item) if item.definitions.is_empty() => true,
            // Otherwise the last definition's body must end in an open paragraph.
            Some(item) => block_ends_with_open_paragraph(
                item.definitions
                    .last()
                    .and_then(|d| last_open_child(&d.children, skip_comments)),
                colon_open,
                skip_comments,
            ),
        },
        // AN UNTERMINATED DIV IS A CONTAINER LIKE ANY OTHER (carve#939, carve#909).
        // PART 1 S4 folds a flush-left line into the innermost OPEN PARAGRAPH,
        // and a `::: ` div that no `:::` line closed still holds one.
        //
        // A div CLOSED by its fence is the case the catch-all below is right
        // about: the fence closes the paragraph inside it, and a dedented line
        // after it ends the item (like code/table). One line of body decides it
        // two ways, which is why `colon_open` is read from the SOURCE.
        Some(BlockNode::LineBlock(line)) if colon_open > 0 => line.children.last().is_some_and(|child| matches!(child, BlockNode::Paragraph(paragraph) if paragraph.children.iter().rev().find(|inline| !matches!(inline, InlineNode::SoftBreak(_) | InlineNode::HardBreak(_))).is_some_and(|inline| !matches!(inline, InlineNode::Comment(_))))),
        Some(BlockNode::Directive(d)) if colon_open > 0 => block_ends_with_open_paragraph(
            last_open_child(&d.children, skip_comments),
            colon_open - 1,
            skip_comments,
        ),
        Some(BlockNode::Div(d)) if colon_open > 0 => block_ends_with_open_paragraph(
            last_open_child(&d.children, skip_comments),
            colon_open - 1,
            skip_comments,
        ),
        Some(BlockNode::Admonition(a)) if colon_open > 0 => block_ends_with_open_paragraph(
            last_open_child(&a.children, skip_comments),
            colon_open - 1,
            skip_comments,
        ),
        // A bare `::: figure` fence is a container like the two above; left
        // unterminated it still holds its open paragraph (§4c defers to §12's
        // container rules for body and closer discipline).
        Some(BlockNode::FigureGroup(g)) if colon_open > 0 => block_ends_with_open_paragraph(
            last_open_child(&g.children, skip_comments),
            colon_open - 1,
            skip_comments,
        ),
        _ => false,
    }
}

/// Whether a list item's second block came from a PARAGRAPH-SHAPED line, and so
/// loosens the item when a blank line sits above it (§17 L1).
pub(crate) fn block_is_paragraph_shaped(block: &BlockNode) -> bool {
    match block {
        BlockNode::Paragraph(_) => true,
        BlockNode::BlockImage(_) => true,
        BlockNode::Figure(f) => matches!(
            *f.target,
            FigureTarget::Image(_) | FigureTarget::Paragraph(_)
        ),
        _ => false,
    }
}

/// A flush-left single-line comment (`%% ...`), the one construct §24 C3
/// recognizes at any column that leaves its container open.
///
/// SHAPE ONLY. A comment FENCE is not one BY SHAPE, but §28 can degrade it into
/// one - see `flush_comment_keeps_the_item_open`, which is the ownership
/// question. Callers that want the shape (seeding a body floor, the marker-line
/// sub-list arm) keep asking this.
pub(crate) fn is_flush_line_comment(line: &str) -> bool {
    line.starts_with("%%") && detect_comment_fence_line(line).is_none()
}

/// The text of a trailing `%%` comment: one separating space or tab off the
/// front, trailing spaces and tabs off the back.
pub(crate) fn trailing_comment_content(raw: &str) -> &str {
    raw.strip_prefix(' ')
        .unwrap_or(raw)
        .trim_end_matches([' ', '\t'])
}

/// Where the trailing `%%` comment whose text starts at `at` ends: the index of
/// the line break, or of the closer of the innermost bracket run holding it,
/// whichever comes first. `None` is a comment that reaches the end of the text
/// with neither bound, which no closer can follow.
///
/// `CARVE-P9-041` gives the marker "all remaining characters up to the line
/// break", and the line is bounded by the INLINE RUN hosting the marker: a
/// bracket run ends at its own closer, so a comment inside one cannot take that
/// `]` (markup-carve/carve#2576, markup-carve/carve-rs#2160). Were it able to,
/// `::: note [a %% x]` would never complete its opener and the container would
/// lose every child it holds.
///
/// Two scans read this rule - the inline loop and the emphasis closing scan -
/// and one spelling is how a rule with two keeps half of itself.
pub(crate) fn trailing_comment_end(
    bytes: &[u8],
    at: usize,
    run_close: Option<usize>,
) -> Option<usize> {
    let newline = bytes[at..].iter().position(|&b| b == b'\n').map(|p| at + p);
    match (newline, run_close) {
        (Some(newline), Some(close)) => Some(newline.min(close)),
        (bound, None) | (None, bound) => bound,
    }
}

/// A container label without the trailing comment `CARVE-P9-041` consumes.
///
/// The clause grants a trailing `%%` marker three properties in EVERY inline
/// host: a tab separates as a space does, a marker that starts the inline run
/// needs no separator before it, and the marker, the rest of the line and the
/// whole separating run before it produce no output. A `[label]` is a leaf
/// host - its run begins mid-line, so nothing at the block layer covers a `%%`
/// there (markup-carve/carve#2552, merged as carve#2562).
///
/// A label is carried as authored text rather than as a parsed inline run, so
/// it never reaches the loop that reads this rule for every other host. What it
/// must NOT do is read the rule a second time from scratch. Three rounds of
/// review found three more constructs whose body may spell a `%%` that is not a
/// marker - a backtick run, a delimited comment `{% ... %}`, an editorial
/// comment `{# ... #}` - and each one missed DELETES visible label text. The
/// list is not the kind of thing to get right by enumeration.
///
/// So the run itself arbitrates. The label is parsed as inlines, which is the
/// one reader that knows what is opaque to a marker; if that yields no trailing
/// comment the label is returned whole. Only the CUT has to be recovered, and a
/// candidate `%%` is confirmed by whether the text after it normalizes to the
/// content the parser recorded. A non-delimited comment reaches the end of its
/// line and a label is one line, so there is at most one to find.
///
/// If a label ever becomes an inline run this disappears, which is open and
/// unruled as markup-carve/carve#2572: carve-js publishes the authored text
/// here exactly as this engine does, and only the oracle renders the run.
pub(crate) fn label_without_trailing_comment<'a>(label: &'a str, options: &Options<'_>) -> &'a str {
    let Some(content) = parse_inline_with_options(label, options)
        .into_iter()
        .find_map(|node| match node {
            InlineNode::Comment(comment) if !comment.delimited => Some(comment.content),
            _ => None,
        })
    else {
        return label;
    };
    let bytes = label.as_bytes();
    for i in 0..bytes.len() {
        if bytes[i] != b'%' || bytes.get(i + 1) != Some(&b'%') {
            continue;
        }
        if !(i == 0 || bytes[i - 1] == b' ') {
            continue;
        }
        if trailing_comment_content(&label[i + 2..]) == content {
            let kept = label[..i].trim_end_matches(' ');
            // An escaped space is content: the separating run starts after it.
            let backslashes = kept.bytes().rev().take_while(|&b| b == b'\\').count();
            return if backslashes % 2 == 1 && kept.len() < i {
                &label[..=kept.len()]
            } else {
                kept
            };
        }
    }
    label
}

pub(crate) fn marked_quote_before_comment(cur: &LineCursor<'_>, comment_at: usize) -> bool {
    // CARVE-P0-013 keeps an unmatched fence after the span in the item.
    if let Some(open) = detect_comment_fence_line_any_column(cur.lines.at(comment_at)) {
        if let Some(close) = (comment_at + 1..cur.lines.len())
            .find(|&at| is_comment_fence_close_any_column(cur.lines.at(at), open.fence_len))
        {
            let next = close + 1;
            if next < cur.lines.len()
                && indent_columns(cur.lines.at(next)) == 0
                && detect_fence_open(cur.lines.at(next)).is_some_and(|code| {
                    !cur.lines
                        .range(next + 1..cur.lines.len())
                        .any(|line| is_fence_close(trim_ascii_start(line), code))
                })
            {
                return false;
            }
        }
    }
    let Some((marker_at, marker)) = (0..comment_at)
        .rev()
        .find_map(|at| detect_list_marker_full(cur.lines.at(at)).map(|marker| (at, marker)))
    else {
        return false;
    };
    let Some(content_col) = marker_content_col_at(cur.lines.at(marker_at), marker.indent) else {
        return false;
    };
    (marker_at + 1..comment_at).any(|at| {
        let line = cur.lines.at(at);
        indent_columns(line) >= content_col
            && strip_blockquote_prefix(trim_ascii_start(line)).is_some()
    })
}

pub(crate) fn matched_comment_before(cur: &LineCursor<'_>) -> Option<usize> {
    let close_at = cur.pos.checked_sub(1)?;
    let close = detect_comment_fence_line_any_column(cur.lines.at(close_at))?;
    let marker_at = (0..close_at)
        .rev()
        .find(|&at| detect_list_marker_full(cur.lines.at(at)).is_some())?;
    if cur
        .lines
        .range(marker_at + 1..close_at + 1)
        .filter(|line| is_comment_fence_close_any_column(line, close.fence_len))
        .count()
        % 2
        != 0
    {
        return None;
    }
    let opener = (marker_at + 1..close_at).rev().find(|&at| {
        detect_comment_fence_line_any_column(cur.lines.at(at))
            .is_some_and(|open| open.fence_len == close.fence_len)
    })?;
    Some(opener)
}

/// The `%%` line form at ANY column.
///
/// A comment is column-exempt (§24 C3): below a container's content column it is
/// still invisible and still closes the paragraph, and because it adds no block
/// there it cannot end the container it never reached
/// (markup-carve/carve-rs#1516).
pub(crate) fn is_line_comment_any_column(line: &str) -> bool {
    let trimmed = trim_ascii_start(line);
    trimmed.starts_with("%%")
        && !is_document_definition_placeholder(trimmed)
        && detect_comment_fence_line(trimmed).is_none()
}

pub(crate) fn ends_with_empty_comment_fence(lines: &[String]) -> bool {
    lines.len() >= 2
        && detect_comment_fence_line_any_column(&lines[lines.len() - 2]).is_some_and(|open| {
            is_comment_fence_close_any_column(&lines[lines.len() - 1], open.fence_len)
        })
}

pub(crate) fn ends_with_closed_comment_fence(lines: &[String]) -> bool {
    let Some((closer, body)) = lines.split_last() else {
        return false;
    };
    body_open_comment_run(body).is_some_and(|run| is_comment_fence_close_any_column(closer, run))
}

/// `is_line_comment_any_column`, over BOTH spellings §28 gives a comment.
///
/// An opener with no exact-width closer AHEAD opens nothing and IS one
/// `comment_line`, and that classification is TOTAL - ownership included
/// (markup-carve/carve#1903). So a degraded fence below a container's content
/// column is column-exempt exactly as the `%%` line is, and the line under it
/// still lands in the innermost open item. A TERMINATED fence is a comment
/// BLOCK and keeps its own answer.
pub(crate) fn line_comment_keeps_the_container_open(cur: &mut LineCursor<'_>, line: &str) -> bool {
    if is_line_comment_any_column(line) {
        return true;
    }
    detect_comment_fence_line_any_column(line)
        .is_some_and(|open| !cur.has_comment_closer_after(cur.pos + 1, open.fence_len))
}

pub(crate) fn effective_comment_line_at(cur: &mut LineCursor<'_>, at: usize) -> bool {
    let line = cur.lines.at(at);
    if is_line_comment_any_column(line) {
        return true;
    }
    detect_comment_fence_line_any_column(line).is_some_and(|open| {
        let run = open.fence_len;
        !cur.has_comment_closer_after(at + 1, run)
            && (cur
                .closed_comment_location
                .map(|(closed, _)| closed != at)
                .unwrap_or(true)
                || at > 0 && is_comment_fence_close_any_column(cur.lines.at(at - 1), run))
    })
}

/// Is the next line one `collect_trailing_lazy` could actually fold?
///
/// A cheap peek, kept separate because the open-paragraph test that follows it
/// reparses the whole collected subtree: it must run only when there is a lazy
/// line pending, else deeply nested lists blow up.
///
/// AS A CONTAINER, because that is the only thing this runs for: the paragraph
/// the line would fold into belongs to a container, and §7 recognizes an
/// abbreviation definition only as a direct child of the DOCUMENT. Asked at
/// document level, `*[A]: b` interrupted a marker-line block's paragraph and
/// was collected, where `- a` on the plain-paragraph path next to it folded the
/// same line in - two paths, one construct, two answers (carve-rs#2096).
pub(crate) fn lazy_line_pending(cur: &mut LineCursor) -> bool {
    let Some(line) = cur.peek() else { return false };
    let line = line.to_string();
    !is_blank_line(&line)
        && indent_columns(&line) == 0
        && !is_list_marker(&line)
        && !interrupts_paragraph_as_container(cur, &line)
}
