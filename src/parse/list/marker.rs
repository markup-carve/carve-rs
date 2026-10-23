use super::*;

pub(crate) fn is_list_marker(line: &str) -> bool {
    detect_task(line).is_some()
        || detect_unordered(line).is_some()
        || detect_ordered(line).is_some()
}

/// Read an attribute block abutting a list marker (`-{.c}` / `3.{#x}`):
/// the parsed attributes (`None` for an empty `{}` block) and the byte index
/// just past the closing `}`. Returns `None` when there is no closing brace
/// or the content is not a valid attribute list -- in which case the marker
/// is not a list item (the line is ordinary text, grammar `item_attributes`).
pub(crate) fn read_list_item_attrs(bytes: &[u8], start: usize) -> Option<(Option<Attrs>, usize)> {
    if bytes.get(start) != Some(&b'{') {
        return None;
    }
    let mut i = start + 1;
    let mut quote: Option<u8> = None;
    let mut escaped = false;
    while i < bytes.len() {
        let b = bytes[i];
        if escaped {
            escaped = false;
        } else if b == b'\\' {
            escaped = true;
        } else if let Some(q) = quote {
            if b == q {
                quote = None;
            }
        } else if b == b'"' || b == b'\'' {
            quote = Some(b);
        } else if b == b'}' {
            let inner = std::str::from_utf8(&bytes[start + 1..i]).ok()?;
            let end = i + 1;
            return if inner.trim().is_empty() {
                Some((None, end))
            } else {
                Some((Some(parse_attrs(inner)?), end))
            };
        }
        i += 1;
    }
    None
}

/// The text after a list marker: an optional abutting attribute block, then
/// the marker's required single space, then the item content. Returns the
/// content (trailing whitespace trimmed) and the item attributes. `None` when
/// the required space is missing or an abutting `{...}` is not a valid
/// attribute block (so the line is not a list item). A SPACE before `{` is
/// ordinary content, not an item-attribute, so it is handled by the plain
/// space branch and the `{...}` stays in the content.
pub(crate) fn marker_tail(line: &str, marker_end: usize) -> Option<(&str, Option<Attrs>)> {
    let bytes = line.as_bytes();
    let (content, attrs) = match bytes.get(marker_end) {
        Some(&b' ') => {
            let mut content_start = marker_end;
            while matches!(bytes.get(content_start), Some(b' ' | b'\t')) {
                content_start += 1;
            }
            (trim_ascii_end(&line[content_start..]), None)
        }
        Some(&b'{') => {
            let (attrs, end) = read_list_item_attrs(bytes, marker_end)?;
            if bytes.get(end) != Some(&b' ') {
                return None;
            }
            let mut content_start = end;
            while matches!(bytes.get(content_start), Some(b' ' | b'\t')) {
                content_start += 1;
            }
            (trim_ascii_end(&line[content_start..]), attrs)
        }
        _ => return None,
    };
    // A marker with no same-line content is not a list item -- a bare `- `
    // (or `-{.c} `) is ordinary text (matches carve-js / carve-php; a list
    // item carries its content on the marker line).
    if content.is_empty() {
        return None;
    }
    Some((content, attrs))
}

pub(crate) fn detect_unordered(line: &str) -> Option<(&str, Option<Attrs>, &str)> {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
        i += 1;
    }
    if i >= bytes.len() {
        return None;
    }
    let c = bytes[i];
    // `+` is the list continuation marker, not a bullet (#80).
    if c != b'-' && c != b'*' {
        return None;
    }
    let (content, attrs) = marker_tail(line, i + 1)?;
    Some((content, attrs, &line[i..i + 1]))
}

pub(crate) fn detect_ordered(line: &str) -> Option<&str> {
    detect_ordered_full(line).map(|(content, _, _, _, _, _)| content)
}

#[allow(clippy::type_complexity)]
pub(crate) fn detect_ordered_full(
    line: &str,
) -> Option<(
    &str,
    Option<usize>,
    Option<OrderedListType>,
    Option<Attrs>,
    u8,
    &str,
)> {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
        i += 1;
    }
    let marker_start = i;
    while i < bytes.len() && bytes[i].is_ascii_alphanumeric() {
        i += 1;
    }
    if i == marker_start {
        if bytes.get(i) != Some(&b'.') {
            return None;
        }
        let (content, attrs) = marker_tail(line, i + 1)?;
        return Some((content, None, None, attrs, b'.', ""));
    }
    if bytes.get(i) != Some(&b'.') && bytes.get(i) != Some(&b')') {
        return None;
    }
    let delim = bytes[i];
    // The required space may be preceded by an abutting attribute block
    // (`3.{#x} item`); `marker_tail` enforces the space and rejects an
    // invalid block.
    let (content, attrs) = marker_tail(line, i + 1)?;
    let marker = &line[marker_start..i];
    if marker.bytes().all(|b| b.is_ascii_digit()) {
        return Some((
            content,
            marker.parse::<usize>().ok().filter(|n| *n != 1),
            None,
            attrs,
            delim,
            marker,
        ));
    }
    // A single letter is ALPHA by default, EXCEPT a lone `i`/`I`, which defaults
    // to roman (§11 ambiguous-letter rule; the list parser may re-classify
    // either way when a consecutive sibling disambiguates).
    if marker.len() == 1 && !marker.eq_ignore_ascii_case("i") {
        let b = marker.as_bytes()[0];
        if b.is_ascii_lowercase() {
            return Some((
                content,
                Some((b - b'a' + 1) as usize).filter(|n| *n != 1),
                Some(OrderedListType::LowerAlpha),
                attrs,
                delim,
                marker,
            ));
        }
        if b.is_ascii_uppercase() {
            return Some((
                content,
                Some((b - b'A' + 1) as usize).filter(|n| *n != 1),
                Some(OrderedListType::UpperAlpha),
                attrs,
                delim,
                marker,
            ));
        }
    }
    let roman = roman_to_int(marker)?;
    Some((
        content,
        Some(roman).filter(|n| *n != 1),
        Some(if marker.chars().all(|c| c.is_ascii_uppercase()) {
            OrderedListType::UpperRoman
        } else {
            OrderedListType::LowerRoman
        }),
        attrs,
        delim,
        marker,
    ))
}

pub(crate) fn roman_to_int(s: &str) -> Option<usize> {
    let mut total = 0isize;
    let mut prev = 0isize;
    for ch in s.chars().rev() {
        let val = match ch.to_ascii_lowercase() {
            'i' => 1,
            'v' => 5,
            'x' => 10,
            'l' => 50,
            'c' => 100,
            'd' => 500,
            'm' => 1000,
            _ => return None,
        };
        if val < prev {
            total -= val;
        } else {
            total += val;
            prev = val;
        }
    }
    (total > 0).then_some(total as usize)
}

/// A task marker the reader recognized.
pub(crate) struct TaskMarker<'a> {
    pub(crate) checked: bool,
    /// The authored state, when it is not the default for the box.
    pub(crate) task_state: Option<char>,
    pub(crate) content: &'a str,
    pub(crate) attrs: Option<Attrs>,
    pub(crate) marker: &'a str,
}

pub(crate) fn detect_task(line: &str) -> Option<TaskMarker<'_>> {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
        i += 1;
    }
    if i >= bytes.len() {
        return None;
    }
    let c = bytes[i];
    // `+` is the list continuation marker, not a bullet (#80).
    if c != b'-' && c != b'*' {
        return None;
    }
    // An attribute block abuts the bullet, BEFORE the task marker:
    // `-{.c} [ ] text`. `marker_tail` consumes the optional block and the
    // bullet's required space; the task box `[x] ` then opens the content.
    let (after, attrs) = marker_tail(line, i + 1)?;
    let ab = after.as_bytes();
    if ab.len() < 4 || ab[0] != b'[' || ab[2] != b']' || ab[3] != b' ' {
        return None;
    }
    // PART 9 enumerates the states exhaustively:
    //
    //   task_state = ' ' | 'x' | 'X' | '-' | '_' | '>' | '?' ;
    //
    // The bracket shape was checked and the state byte was not, so any single
    // character opened a task item - and `- [!] urgent` did not merely
    // reinterpret the marker, it DELETED the `[!]` and rendered a checkbox
    // nobody wrote (carve-rs#471). Two characters were already rejected by the
    // `ab[2] != b']'` test; only the one-character case was open.
    if !matches!(ab[1], b' ' | b'x' | b'X' | b'-' | b'_' | b'>' | b'?') {
        return None;
    }
    let checked = matches!(ab[1], b'x' | b'X');
    // PART 11 §6g. A checked box records nothing: `[X]` and `[x]` are one state.
    let task_state = (!checked && ab[1] != b' ').then(|| ab[1] as char);
    Some(TaskMarker {
        checked,
        task_state,
        content: trim_ascii_end(&after[4..]),
        attrs,
        marker: &line[i..i + 1],
    })
}

/// Lower-alpha index of a single letter (`a`=1 … `z`=26), case-insensitive.
pub(crate) fn alpha_index(m: &str) -> Option<usize> {
    if m.len() != 1 {
        return None;
    }
    let b = m.as_bytes()[0].to_ascii_lowercase();
    (b.is_ascii_lowercase()).then(|| (b - b'a' + 1) as usize)
}

/// Resolve the §11 ambiguous-letter tie-break for an ordered list's FIRST
/// marker, returning its `(start, ol_type)`. A single roman-letter marker
/// (i/v/x/l/c/d/m) is reclassified to ROMAN when the next sibling is the
/// consecutive roman numeral, to ALPHA when the next is the consecutive letter;
/// otherwise the detector's default stands (lone `i`/`I` roman, others alpha).
pub(crate) fn resolve_ordered_first(
    first: &ListMarker<'_>,
    cur: &LineCursor,
    base_indent: usize,
) -> (Option<usize>, Option<OrderedListType>) {
    if !first.ordered || !is_ambiguous_roman_letter(first.marker) {
        return (first.start, first.ol_type);
    }
    // Find the next sibling ordered marker at the same indent, skipping the
    // first item's own body (blank lines and lines indented past the base).
    let mut sibling = None;
    let mut blank_run = 0;
    for l in cur.lines.range(cur.pos + 1..cur.lines.len()) {
        if is_blank_line(l) {
            blank_run += 1;
            continue;
        }
        if indent_columns(l) > base_indent {
            blank_run = 0;
            continue; // part of the first item's body
        }
        if blank_run >= 3 {
            break;
        }
        sibling = detect_list_marker_full(l).filter(|m| m.ordered && m.indent == base_indent);
        break;
    }
    let upper = first.marker.chars().all(|c| c.is_ascii_uppercase());
    let roman_type = if upper {
        OrderedListType::UpperRoman
    } else {
        OrderedListType::LowerRoman
    };
    let alpha_type = if upper {
        OrderedListType::UpperAlpha
    } else {
        OrderedListType::LowerAlpha
    };
    if let Some(sib) = sibling {
        if sib.marker.chars().all(|c| c.is_ascii_uppercase()) != upper {
            return (first.start, first.ol_type);
        }
        let first_roman = roman_to_int(first.marker);
        let sib_roman = roman_to_int(sib.marker).filter(|_| !sib.marker.is_empty());
        if let (Some(fr), Some(sr)) = (first_roman, sib_roman) {
            // sibling is itself a roman-shaped marker and the consecutive value
            if sr == fr + 1 {
                return (Some(fr).filter(|n| *n != 1), Some(roman_type));
            }
        }
        if let (Some(fa), Some(sa)) = (alpha_index(first.marker), alpha_index(sib.marker)) {
            if sa == fa + 1 {
                return (Some(fa).filter(|n| *n != 1), Some(alpha_type));
            }
        }
    }
    (first.start, first.ol_type)
}

pub(crate) fn has_indexed_comment_closer_after(
    lines: CursorLines<'_>,
    comment_closers: &mut Option<HashMap<usize, usize>>,
    start: usize,
    fence_len: usize,
) -> bool {
    if let Some(answer) = lines.comment_after(start, fence_len) {
        return answer;
    }
    if comment_closers.is_none() {
        *comment_closers = Some(build_comment_closer_last_index(&lines));
    }
    comment_closers
        .as_ref()
        .and_then(|index| index.get(&fence_len).copied())
        .is_some_and(|last| last >= start)
}

/// Length of the block attached by `+` within the caller's bounded slice.
/// Leading attributes belong to that block. Fences and colon containers
/// use their line extent directly; other blocks are parsed to find the end.
/// Nested `+` markers stay whole: probing them repeats work per level, while
/// an inner split cannot change the enclosing block's line extent.
/// A nonempty slice consumes at least one line so the caller advances.
pub(crate) fn attached_one_block_lines(slice: CursorLines<'_>, options: &Options<'_>) -> usize {
    if slice.is_empty() {
        return 0;
    }
    // A cache OF ITS OWN, never the caller's. The closer index is keyed by line
    // number, and `slice` is renumbered from the container's own lines - handing
    // it an index built over those would read a closer at the wrong place.
    let mut comment_closers: Option<HashMap<usize, usize>> = None;
    let _measuring = MeasuringGuard::enter();
    let mut sub = LineCursor::from_lines(slice, None, None);
    while let Some(line) = sub.peek() {
        if line.starts_with([' ', '\t']) || !line.starts_with('{') {
            break;
        }
        if parse_standalone_attrs_block(&mut sub).is_none() {
            break;
        }
    }
    if let Some(end) = self_delimiting_block_end(slice, sub.pos, &mut comment_closers) {
        return end.clamp(1, slice.len());
    }
    // The chunks a list left pending are resolved and thrown away with the
    // rest of the probe's tree: the probe reads the CURSOR, and a chunk that
    // never parsed is a parse whose side effects never ran.
    let mut item_bodies = Vec::new();
    if let Some(block) = parse_block(&mut sub, options, &mut item_bodies) {
        drop(drained_list_bodies(block, item_bodies, options));
    }
    sub.pos.clamp(1, slice.len())
}

/// One past the last line of the self-delimiting block opening at `slice[start]`,
/// or `None` when that line opens none.
///
/// The five openers and the four helpers are the ones [`attached_block_end`]
/// skips regions with, reused rather than restated: a fence or colon container
/// runs to its closer, or to end of input when it has none, and nothing inside
/// it can shorten that.
pub(crate) fn self_delimiting_block_end(
    slice: CursorLines<'_>,
    start: usize,
    comment_closers: &mut Option<HashMap<usize, usize>>,
) -> Option<usize> {
    let line = slice.line(start)?;
    if let Some(open) = detect_fence_open(line) {
        let mut i = start + 1;
        while i < slice.len() {
            if is_fence_close(slice.at(i), open) {
                return Some(i + 1);
            }
            i += 1;
        }
        return Some(slice.len());
    }
    if let Some(open) = detect_comment_fence_line(line) {
        if has_indexed_comment_closer_after(slice, comment_closers, start + 1, open.fence_len) {
            if let Some(close) = (start + 1..slice.len())
                .find(|&j| is_comment_fence_close(slice.at(j), open.fence_len))
            {
                return Some(close + 1);
            }
        }
        // §28: with no closer ahead it is not a fence, it is a `%%` line
        // comment - one line, and the lines after it are just lines.
        return Some(start + 1);
    }
    if let Some(fence_len) = detect_line_block_open(line) {
        return Some(find_line_block_end(slice, start, fence_len));
    }
    if let Some(fence_len) = detect_hardbreaks_block_open(line) {
        return Some(find_colon_container_end(slice, start, fence_len));
    }
    if let Some(fence_len) = detect_quote_block_open(line) {
        return Some(find_colon_container_end(slice, start, fence_len));
    }
    if let Some(open) = detect_container_open(line) {
        return Some(find_colon_container_end(slice, start, open.fence_len));
    }
    None
}

thread_local! {
    // Set while [`attached_one_block_lines`] is measuring, so a `+` marker
    // reached inside the probe splices its extent rather than running a probe of
    // its own. See that function for why the count is unaffected and for the
    // cost this avoids.
    //
    // Plain initializer for the same MSRV reason as `NESTING_DEPTH`.
    #[allow(clippy::missing_const_for_thread_local)]
    static MEASURING_ATTACHED_BLOCK: Cell<bool> = const { Cell::new(false) };
}

/// RAII flag for the measurement probe, restoring the previous value on drop
/// (panic unwind included), the discipline [`DepthGuard`] keeps for the depth
/// counter.
pub(crate) struct MeasuringGuard {
    previous: bool,
}

impl MeasuringGuard {
    fn enter() -> MeasuringGuard {
        MeasuringGuard {
            previous: MEASURING_ATTACHED_BLOCK.with(|m| m.replace(true)),
        }
    }
}

impl Drop for MeasuringGuard {
    fn drop(&mut self) {
        MEASURING_ATTACHED_BLOCK.with(|m| m.set(self.previous));
    }
}

pub(crate) fn measuring_attached_block() -> bool {
    MEASURING_ATTACHED_BLOCK.with(|m| m.get())
}

/// Find the container boundary after `start`. Lines inside an open fence remain
/// content. The caller shares its comment-closer index across probes.
pub(crate) fn attached_block_end(
    lines: CursorLines<'_>,
    start: usize,
    comment_closers: &mut Option<HashMap<usize, usize>>,
    closer_index: &mut Option<HashMap<u8, Vec<usize>>>,
    is_boundary: &mut dyn FnMut(&str, usize) -> bool,
) -> usize {
    let mut end = start;
    let mut in_fence: Option<FenceOpen> = None;
    let mut unclosed_fence = false;
    while end < lines.len() {
        let line = lines.at(end);
        if unclosed_fence && is_blank_line(line) {
            break;
        }
        if let Some(open) = in_fence {
            if is_fence_close(line, open) {
                in_fence = None;
            }
            end += 1;
            continue;
        }
        if let Some(open) = detect_fence_open(line) {
            // An unclosed attached fence takes the rest of its container only when no
            // blank line follows; otherwise it ends with the attached block (carve#3421).
            // The index rules out most suffixes, so unclosed openers stay linear.
            let possible = lines
                .code_after(end, open.fence_char, open.fence_len)
                .unwrap_or_else(|| {
                    let index =
                        closer_index.get_or_insert_with(|| build_code_closer_last_index(&lines));
                    code_closer_exists_after(index, end, open.fence_char, open.fence_len)
                });
            let closed =
                possible && (end + 1..lines.len()).any(|at| is_fence_close(lines.at(at), open));
            // Read from the end: the tail past the last blank line is short in practice,
            // and a scan forward from every unclosed opener would be quadratic.
            if !closed
                && (end + 1..lines.len())
                    .rev()
                    .find(|&at| is_blank_line(lines.at(at)))
                    .is_some()
            {
                unclosed_fence = true;
                end += 1;
                continue;
            }
            in_fence = Some(open);
            end += 1;
            continue;
        }
        if let Some(open) = detect_comment_fence_line(line) {
            if has_indexed_comment_closer_after(lines, comment_closers, end + 1, open.fence_len) {
                if let Some(close) = (end + 1..lines.len())
                    .find(|&j| is_comment_fence_close(lines.at(j), open.fence_len))
                {
                    end = close + 1;
                    continue;
                }
            }
        }
        // A colon fence is a self-delimiting block; skip the whole region so a
        // `+` inside it is content, not the parent's boundary. Unterminated
        // colon containers close at end of input.
        if let Some(fence_len) = detect_line_block_open(line) {
            end = find_colon_fence_end(lines, end, fence_len, true, closer_index, comment_closers);
            continue;
        }
        if let Some(fence_len) = detect_hardbreaks_block_open(line) {
            end = find_colon_fence_end(lines, end, fence_len, false, closer_index, comment_closers);
            continue;
        }
        if let Some(fence_len) = detect_quote_block_open(line) {
            end = find_colon_fence_end(lines, end, fence_len, false, closer_index, comment_closers);
            continue;
        }
        if let Some(open) = detect_container_open(line) {
            end = find_colon_fence_end(
                lines,
                end,
                open.fence_len,
                false,
                closer_index,
                comment_closers,
            );
            continue;
        }
        if is_boundary(line, end) {
            break;
        }
        end += 1;
    }
    end
}

pub(crate) fn attached_block_lines_classified(
    lines: CursorLines<'_>,
    start: usize,
    comment_closers: &mut Option<HashMap<usize, usize>>,
    closer_index: &mut Option<HashMap<u8, Vec<usize>>>,
    options: &Options<'_>,
    is_boundary: &mut dyn FnMut(&str, usize) -> bool,
) -> usize {
    let end = attached_block_end(lines, start, comment_closers, closer_index, is_boundary);
    if measuring_attached_block() {
        return end;
    }
    let materialized = std::cell::OnceCell::new();
    start + attached_one_block_lines(lines.window(start..end, &materialized), options)
}

/// One past a colon block attached by a first-block `+` whose opener sits at or
/// past `floor`, measured on the lines it owns with its indentation stripped:
/// an indented colon fence's closer is only read as one there. `None` for any
/// other line.
pub(crate) fn attached_indented_block_end(
    lines: CursorLines<'_>,
    start: usize,
    floor: usize,
    options: &Options<'_>,
) -> Option<usize> {
    let first = lines.line(start)?;
    let column = indent_columns(first);
    if column == 0 || column < floor || !trim_ascii_start(first).starts_with(":::") {
        return None;
    }
    let mut stop = start;
    while stop < lines.len() {
        let line = lines.at(stop);
        if !is_blank_line(line) && indent_columns(line) < column {
            break;
        }
        stop += 1;
    }
    let owned: Vec<String> = lines
        .range(start..stop)
        .map(|line| slice_columns_mapped(line, column, true).0)
        .collect();
    let owned: Vec<&str> = owned.iter().map(String::as_str).collect();
    Some(
        start
            + attached_block_lines_classified(
                CursorLines::Slice(&owned),
                0,
                &mut None,
                &mut None,
                options,
                &mut |_, _| false,
            ),
    )
}

/// Parse one attached block, stopping at the host's next marker or sibling item.
pub(crate) fn parse_continuation_block(
    cur: &mut LineCursor,
    options: &Options<'_>,
    base_indent: usize,
    content_col: Option<usize>,
    pull_indent: usize,
) -> Option<BlockNode> {
    // Definitions belong to the attached block. Nested lists manage their own markers.
    while cur.peek().is_some_and(is_document_definition_placeholder) {
        cur.consume();
    }
    if let Some(line) = cur.peek() {
        if let Some(nm) = detect_list_marker_full(line) {
            // A marker indented past the base nests as a child list of THIS
            // item, so parse it unbounded (it manages its own `+`). But a
            // marker AT or BELOW the outer base column is a SIBLING of the
            // outer list, not content of this `+`-attached block: bound the
            // block to empty so the outer list takes the marker as a sibling
            // item rather than nesting it (matches carve-php for `+`-then-
            // marker, e.g. `- a / + / text / + / - b`).
            if nm.indent > base_indent {
                let mut item_bodies = Vec::new();
                return parse_block(cur, options, &mut item_bodies)
                    .map(|block| *drained_list_bodies(block, item_bodies, options));
            }
            return None;
        }
    }
    let lines = cur.lines;
    let start = cur.pos;
    let end = attached_block_end(
        lines,
        start,
        &mut cur.comment_closer_last_index,
        &mut cur.code_closer_last_index,
        &mut |line, end| {
            if trim_ascii(line) == "+" && indent_columns(line) == base_indent {
                return true;
            }
            // A sibling item ends the attachment before lazy continuation can claim it.
            end > start
                && indent_columns(line) <= base_indent
                && detect_list_marker_full(line).is_some()
        },
    );
    let stripped = (pull_indent > 0).then(|| {
        cur.lines
            .range(cur.pos..end)
            .map(|line| slice_columns_mapped(line, pull_indent, false).0)
            .collect::<Vec<_>>()
    });
    let slice: Vec<&str> = stripped.as_ref().map_or_else(
        || cur.lines.range(cur.pos..end).collect(),
        |lines| lines.iter().map(String::as_str).collect(),
    );
    let line_map: Vec<Option<usize>> = if cur.line_map.is_some() {
        (cur.pos..end).map(|pos| cur.source_line(pos)).collect()
    } else {
        Vec::new()
    };
    // Keep the authored column when rebasing a block attached at an indented
    // marker column.
    let col_map: Vec<Option<isize>> = if cur.col_map.is_some() {
        (cur.pos..end)
            .map(|pos| {
                let consumed = if pull_indent > 0 {
                    slice_columns_mapped(cur.lines.at(pos), pull_indent, false).1
                } else {
                    0
                };
                cur.source_col(pos).map(|col| col + consumed as isize)
            })
            .collect()
    } else {
        Vec::new()
    };
    let mut sub = LineCursor::new_with_cols(
        &slice,
        cur.line_map.is_some().then_some(line_map.as_slice()),
        cur.col_map.is_some().then_some(col_map.as_slice()),
    );
    let mut floating_attrs: Option<Attrs> = None;
    let mut floating_attr_slots = None;
    let mut floating_attrs_pos: Option<Pos> = None;
    while let Some(line) = sub.peek() {
        if indent_columns(line) != 0 || !trim_ascii_start(line).starts_with('{') {
            break;
        }
        let attrs_start = sub.pos;
        let Some(attrs) = parse_standalone_attrs_block(&mut sub) else {
            break;
        };
        merge_attrs_indexed(&mut floating_attrs, attrs, &mut floating_attr_slots);
        if floating_attrs_pos.is_none() {
            floating_attrs_pos = span_of(&sub, attrs_start, sub.pos, options);
        }
    }
    // The block starts where the attribute run ended, which is what the source
    // stamp below has to name: `parse_blocks` likewise takes `start_line` after
    // its own attribute lines are consumed. Stamping the slice's first line
    // would point an editor's scroll-sync at the `{…}` line instead.
    let block_start = sub.pos;
    // A + attaches one block; below-column definitions remain outside its paragraph.
    if let Some(content_col) = content_col
        .filter(|_| options.extensions.is_empty() && sub.peek().is_some_and(line_starts_paragraph))
    {
        let paragraph_end = attached_block_end(
            sub.lines,
            block_start,
            &mut sub.comment_closer_last_index,
            &mut sub.code_closer_last_index,
            &mut |line, at| {
                at > block_start && below_item_definition(line, base_indent, content_col)
            },
        );
        sub.lines = CursorLines::Slice(&slice[..paragraph_end]);
    }
    let mut item_bodies = Vec::new();
    let mut block = parse_block(&mut sub, options, &mut item_bodies)
        .map(|block| drained_list_bodies(block, item_bodies, options));
    if let Some(attrs) = floating_attrs {
        // AN EMPTY PARAGRAPH IS NOT A BLOCK THE AUTHOR WROTE THESE FOR. It is
        // what a `parse_block` call returns when the extent held no content -
        // `- a` / `+` / `{.x}` / blank / `> q`, where §17 L3 bounds the
        // attachment at the blank - and `parse_list` filters it out again, so
        // attributes applied to it were discarded one step later without
        // anything noticing. The set reaches nothing; §15 A4 drops it and says
        // so (markup-carve/carve#1281).
        //
        // This does NOT change a byte of HTML or of the AST: the node it used
        // to attach to never reached the tree. What it changes is that the loss
        // is now reported, which is the whole of the rule.
        let reaches_nothing = match block.as_deref() {
            None => true,
            Some(BlockNode::Paragraph(p)) => p.children.is_empty(),
            // AND A BLOCK THAT TAKES NO ATTRIBUTES IS NOT ONE EITHER.
            // `apply_attrs_to_block` ends in `_ => {}`, so handing it a comment
            // or an abbreviation definition discards the set exactly as having
            // no block at all would - and §15 A2a's "float past what renders
            // nothing" cannot save it here, because a `+` attaches ONE block and
            // there is no next one to float to. `- a` / `+` / `{.x}` / `%% c`
            // dropped the attributes with nothing reporting it, while the
            // document-level twin `{.x}` / blank / `%% c` reported them.
            Some(BlockNode::Comment(_)) | Some(BlockNode::AbbreviationDef(_)) => true,
            Some(_) => false,
        };
        if reaches_nothing {
            note_unattached_block_attrs(floating_attrs_pos);
        } else if let Some(node) = &mut block {
            // The invisible-block guard is now ABOVE, in `reaches_nothing`.
            // It was removed from here once, correctly: it could not change a
            // byte of HTML or of the AST, and a check that cannot fail is the
            // defect class markup-carve/carve#755 catalogs. §15 A4's diagnostic
            // is what gives it something to change - the set is still dropped
            // either way, and now the drop is reported.
            apply_attrs_to_block(node, attrs);
        }
    }
    if options.source_lines {
        if let Some(block) = &mut block {
            if let Some(line) = line_map.get(block_start).copied().flatten() {
                stamp_source_line(block, line);
            }
        }
    }
    cur.pos += sub.pos;
    block.map(|block| *block)
}

pub(crate) fn below_item_definition(line: &str, base_indent: usize, content_col: usize) -> bool {
    let indent = indent_columns(line);
    let trimmed = trim_ascii_start(line);
    indent > base_indent
        && indent < content_col
        && (parse_link_def_line(trimmed).is_some() || parse_footnote_def_line(trimmed).is_some())
}

/// A list's extent, taken from the items it holds.
///
/// Used only when the cursor cannot supply one. Both ends have to exist, or the
/// range would start or stop somewhere arbitrary - so a list whose first or last
/// item is unplaced stays unplaced itself rather than reporting a partial span.
pub(crate) fn span_across_items(items: &[ListItem]) -> Option<Pos> {
    let first = items.first()?.pos.clone()?;
    let last = items.last()?.pos.clone()?;
    Some(Pos {
        end_line: last.end_line,
        end_column: last.end_column,
        end_offset: last.end_offset,
        ..first
    })
}

/// Widen each item's span to cover the blocks inside it.
///
/// An item's span is fixed when the item is BUILT, and the loop then attaches
/// later blocks - an indented paragraph, a nested list, a quote, a `+`
/// continuation - to `items.last_mut()`. Every one of those landed after the
/// span was taken, so the item claimed its marker line and the rest of the item
/// sat outside it: 55 nodes across the spec corpus, all of them invisible,
/// because a span is compared against source text for `text` nodes alone
/// (carve#565).
///
/// Widening after the fact rather than re-taking the span at each of the six
/// push sites: the sites disagree about what they have in scope, and a rule
/// that runs once cannot be half-applied by the next branch someone adds.
pub(crate) fn widen_items_over_children(items: &mut [ListItem]) {
    for item in items.iter_mut() {
        widen_item_over_children(item);
    }
}

/// [`widen_items_over_children`] for ONE item, which is the granularity the
/// worklist fills at: a chunk arrives for a single item, and re-running the
/// rule over that item is what keeps the span honest without waiting for the
/// rest of the list.
pub(crate) fn widen_item_over_children(item: &mut ListItem) {
    let Some(ref mut pos) = item.pos else { return };
    let mut last_owned: Option<Pos> = None;
    for child in &item.children {
        let Some(child_pos) = crate::ast_json::block_pos(child) else {
            continue;
        };
        // LINE and COLUMN, not offsets: a span carries the pair at parse
        // time and `fill_offsets` turns it into offsets afterwards, so
        // comparing offsets here compares two zeroes and widens nothing.
        let is_later = match last_owned {
            None => true,
            Some(ref last) => {
                (child_pos.end_line, child_pos.end_column) > (last.end_line, last.end_column)
            }
        };
        if is_later {
            last_owned = Some(child_pos.clone());
        }
    }
    if let Some(last) = last_owned {
        pos.end_line = last.end_line;
        pos.end_column = last.end_column;
        pos.end_offset = last.end_offset;
    }
    item.pos = Some(pos.clone());
}

/// Move a list span's start back across the indentation that placed the list.
/// `base_indent` is in tab-expanded columns, so a tab shared with the parent's
/// content column is narrower in the source; the start never passes the first
/// source column of its own line, cursor line `at` (carve-rs#2967).
pub(crate) fn reclaim_placing_indent(
    pos: &mut Pos,
    cur: &LineCursor<'_>,
    at: Option<usize>,
    base_indent: usize,
) {
    let floor = at
        .and_then(|i| cur.source_col(i))
        .map_or(1, |stripped| document_column(stripped, 0));
    pos.start_column = pos.start_column.saturating_sub(base_indent).max(floor);
}
