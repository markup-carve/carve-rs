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
            while matches!(bytes.get(content_start), Some(b' ')) {
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
            while matches!(bytes.get(content_start), Some(b' ')) {
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
    // The thematic break is tested before a list marker, so `- - -` is no bullet (carve#4814).
    if thematic_break_marker(&line[i..]).is_some() {
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
    // A letter or roman marker with `.` needs two spaces, so initials stay text
    // (carve#4811).
    if delim == b'.'
        && marker.bytes().all(|b| b.is_ascii_alphabetic())
        && !content.is_empty()
        && !line[..content.as_ptr() as usize - line.as_ptr() as usize].ends_with("  ")
    {
        return None;
    }
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
    //   task_state = ' ' | 'x' | 'X' | '-' | '_' | '/' | '!' | '>' | '?' ;
    //
    // The bracket shape was checked and the state byte was not, so any single
    // character opened a task item - and `- [%] urgent` did not merely
    // reinterpret the marker, it DELETED the `[%]` and rendered a checkbox
    // nobody wrote (carve-rs#471). Two characters were already rejected by the
    // `ab[2] != b']'` test; only the one-character case was open.
    if !matches!(
        ab[1],
        b' ' | b'x' | b'X' | b'-' | b'_' | b'/' | b'!' | b'>' | b'?'
    ) {
        return None;
    }
    let checked = matches!(ab[1], b'x' | b'X');
    // PART 11 §6g. A checked box records nothing: `[X]` and `[x]` are one state.
    let task_state = (!checked && ab[1] != b' ').then(|| ab[1] as char);
    Some(TaskMarker {
        checked,
        task_state,
        // The gap after the box is the item's authored gap (CARVE-P9-083), so
        // content starts past all of it, as the paragraph path already read it.
        content: trim_ascii_end(after[4..].trim_start_matches(' ')),
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
    _comment_closers: &mut Option<HashMap<usize, usize>>,
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
        // An unclosed span runs to the end of its container (carve#4907).
        return Some(
            (start + 1..slice.len())
                .find(|&j| is_comment_fence_close(slice.at(j), open.fence_len))
                .map_or(slice.len(), |close| close + 1),
        );
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
    let mut unclosed_fence = None;
    // A refused brace run's closed follower fence: paragraph text through its
    // closer, where only a sibling marker still ends the block (CARVE-P2-035).
    let mut follower: Option<FenceOpen> = None;
    while end < lines.len() {
        let line = lines.at(end);
        if let Some(open) = follower {
            if is_fence_close(line, open) {
                follower = None;
            } else if is_blank_line(line)
                || ((is_plus_marker(line)
                    || line.strip_prefix(":: ").is_some()
                    || strip_definition_marker(line).is_some())
                    && is_boundary(line, end))
            {
                break;
            }
            end += 1;
            continue;
        }
        if let Some(owner_col) = unclosed_fence {
            // An open payload ignores marker shapes. Only its first blank or a
            // nonblank line left of the opener's owner can end it (PART 0 S1).
            if is_blank_line(line) || indent_columns(line) < owner_col {
                break;
            }
            end += 1;
            continue;
        }
        if let Some(open) = in_fence {
            if is_fence_close(line, open) {
                in_fence = None;
            }
            end += 1;
            continue;
        }
        // A new block at the host's boundary is not part of the block the
        // marker attached, even when that new block is itself a fence.
        if end > start && is_boundary(line, end) {
            break;
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
            // A refused brace run's follower is paragraph text; only a closer
            // carries it further (CARVE-P2-035).
            if end > start && cursor_ends_in_refused_brace_run(lines, start, end) {
                follower = closed.then_some(open);
                end += 1;
                continue;
            }
            // Read from the end: the tail past the last blank line is short in practice,
            // and a scan forward from every unclosed opener would be quadratic.
            if !closed
                && (end + 1..lines.len())
                    .rev()
                    .find(|&at| is_blank_line(lines.at(at)))
                    .is_some()
            {
                unclosed_fence = Some(indent_columns(line));
                end += 1;
                continue;
            }
            in_fence = Some(open);
            end += 1;
            continue;
        }
        if let Some(open) = detect_comment_fence_line(line) {
            end = (end + 1..lines.len())
                .find(|&j| is_comment_fence_close(lines.at(j), open.fence_len))
                .map_or(lines.len(), |close| close + 1);
            continue;
        }
        // A colon fence is a self-delimiting block; skip the whole region so a
        // `+` inside it is content, not the parent's boundary. An unterminated
        // one ends with the attached block, at its first blank line (§17 L3).
        let colon = detect_line_block_open(line)
            .map(|fence_len| (fence_len, true))
            .or_else(|| detect_hardbreaks_block_open(line).map(|fence_len| (fence_len, false)))
            .or_else(|| detect_quote_block_open(line).map(|fence_len| (fence_len, false)))
            .or_else(|| detect_container_open(line).map(|open| (open.fence_len, false)));
        if let Some((fence_len, literal)) = colon {
            let opener = end;
            // Only a later bare run of this width can close it. The index keeps
            // repeated unclosed openers linear.
            let may_close = lines.colon_closes(opener).unwrap_or_else(|| {
                closer_index
                    .get_or_insert_with(|| build_code_closer_last_index(&lines))
                    .entry(b':')
                    .or_insert_with(|| {
                        let mut last = Vec::new();
                        for at in 0..lines.len() {
                            if let Some(width) = exact_colon_fence_len(lines.at(at)) {
                                if last.len() <= width {
                                    last.resize(width + 1, 0);
                                }
                                last[width] = at;
                            }
                        }
                        last
                    })
                    .get(fence_len)
                    .is_some_and(|&last| last > opener)
            });
            if may_close {
                let mut past_blank = false;
                let candidate = find_colon_fence_end_bounded(
                    lines,
                    opener,
                    fence_len,
                    literal,
                    closer_index,
                    comment_closers,
                    // A sibling of the host ends the frame, whatever closer follows
                    // (CARVE-P9-084, carve#4798).
                    &mut |line, at| {
                        if is_blank_line(line) {
                            past_blank = true;
                            return false;
                        }
                        (detect_list_marker_full(line).is_some()
                            || strip_lazy(line).strip_prefix(":: ").is_some()
                            || strip_definition_marker(strip_lazy(line)).is_some()
                            || (past_blank
                                && exact_colon_fence_len(trim_ascii_start(line)).is_none()
                                && item_block_opener(trim_ascii_start(line))))
                            && is_boundary(line, at)
                    },
                );
                if candidate > opener
                    && exact_colon_fence_len(lines.at(candidate - 1)) == Some(fence_len)
                {
                    end = candidate;
                    continue;
                }
                // The bounded scan still knows which marker-shaped lines were
                // inside a code fence. Keep its boundary when no blank ended
                // the unclosed attachment before that boundary.
                if (opener + 1..candidate).all(|at| !is_blank_line(lines.at(at))) {
                    end = candidate;
                    continue;
                }
            }
            // An unclosed colon block has no later closer to carry its
            // attachment past the first blank or host boundary.
            end = (opener + 1..lines.len())
                .find(|&at| is_blank_line(lines.at(at)) || is_boundary(lines.at(at), at))
                .unwrap_or(lines.len());
            break;
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

/// Narrows `start..end` to the one block a `+` attached, read at that block's
/// own column: with its indent kept, a quote or heading reads as a paragraph
/// that takes the next unmarked line lazily (carve#5330).
pub(crate) fn attached_block_at_own_column(
    lines: CursorLines<'_>,
    start: usize,
    end: usize,
    options: &Options<'_>,
) -> usize {
    let column = lines.line(start).map_or(0, indent_columns);
    if column == 0 || end <= start + 1 || measuring_attached_block() {
        return end;
    }
    let owned: Vec<String> = lines
        .range(start..end)
        .map(|line| slice_columns_mapped(line, column, true).0)
        .collect();
    let owned: Vec<&str> = owned.iter().map(String::as_str).collect();
    start + attached_one_block_lines(CursorLines::Slice(&owned), options)
}

/// Whether `from..to` is a column-0 note definition and its folded body lines,
/// which belong to the note (CARVE-P9-067, carve#5034).
fn note_definition_with_body(lines: CursorLines<'_>, from: usize, to: usize) -> bool {
    to > from + 1
        && lines.line(from).is_some_and(|first| {
            !first.starts_with([' ', '\t']) && parse_footnote_def_line(first).is_some()
        })
        && (from + 1..to)
            .all(|at| crate::parse::own_lines::foldable_plain_line(trim_ascii_start(lines.at(at))))
}

/// One past the block a `+` attached in a host with no marker column: a comment or
/// a definition is invisible, so the block it leads is attached with it (CARVE-P0-004).
/// Whether a fence nested in an attached block is open before a line. Such a
/// fence is not ended by a blank run; only a fence the `+` attached itself is
/// (PART 0 S1, carve#5539). Advances incrementally, so a boundary closure that
/// asks once per line stays linear.
#[derive(Default)]
pub(crate) struct NestedAttachedFence {
    scanned: Option<usize>,
    open: Option<(FenceOpen, usize)>,
}

impl NestedAttachedFence {
    /// Whether `line` at `at` is a blank that ends the attached block.
    pub(crate) fn ends_at_blank(
        &mut self,
        lines: CursorLines<'_>,
        start: usize,
        at: usize,
        line: &str,
    ) -> bool {
        is_blank_line(line) && !self.open_before(lines, start, at)
    }

    pub(crate) fn open_before(&mut self, lines: CursorLines<'_>, start: usize, at: usize) -> bool {
        let Some(first) = lines.line(start) else {
            return false;
        };
        if detect_fence_open(trim_ascii_start(first)).is_some() {
            return false;
        }
        let floor = indent_columns(first);
        let mut next = self.scanned.unwrap_or(start + 1).max(start + 1);
        while next < at && next < lines.len() {
            let line = lines.at(next);
            next += 1;
            if is_blank_line(line) {
                continue;
            }
            let (indent, rest) = (indent_columns(line), trim_ascii_start(line));
            if let Some((open, column)) = self.open {
                if indent >= column {
                    if indent == column && is_fence_close(rest, open) {
                        self.open = None;
                    }
                    continue;
                }
                self.open = None;
            }
            if indent > floor {
                self.open = detect_fence_open(rest).map(|open| (open, indent));
            }
        }
        self.scanned = Some(next);
        self.open.is_some()
    }
}

pub(crate) fn attached_block_past_invisible(
    lines: CursorLines<'_>,
    start: usize,
    comment_closers: &mut Option<HashMap<usize, usize>>,
    closer_index: &mut Option<HashMap<u8, Vec<usize>>>,
    options: &Options<'_>,
    is_boundary: &mut dyn FnMut(&str, usize) -> bool,
) -> usize {
    let mut end = attached_block_lines_classified(
        lines,
        start,
        comment_closers,
        closer_index,
        options,
        is_boundary,
    );
    let mut block = start;
    while end > start
        && (lines.line(end - 1).is_some_and(|last| {
            is_line_comment_any_column(last)
                || (!last.starts_with([' ', '\t'])
                    && (parse_link_def_line(last).is_some()
                        || parse_footnote_def_line(last).is_some()
                        || detect_abbreviation_def(last).is_some()))
        }) || note_definition_with_body(lines, block, end))
        && lines.line(end).is_some_and(|next| !is_boundary(next, end))
    {
        let prior = end;
        block = prior;
        end = attached_block_lines_classified(
            lines,
            prior,
            comment_closers,
            closer_index,
            options,
            is_boundary,
        );
        if end == prior {
            break;
        }
    }
    end
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
/// One past the closer of a `+`-attached colon body that [`attached_block_end`]
/// ended at its first blank, when the host's lines at or past `content_col`
/// hold that closer: such a body is not unclosed (PART 0 S1, §17 L3).
fn host_closed_colon_end(
    lines: CursorLines<'_>,
    start: usize,
    end: usize,
    content_col: usize,
    index: &mut Option<HostColonClosers>,
) -> Option<usize> {
    if !lines.line(end).is_some_and(is_blank_line) {
        return None;
    }
    let opener = trim_ascii_start(lines.line(start)?);
    let (fence_len, literal) = detect_line_block_open(opener)
        .map(|fence_len| (fence_len, true))
        .or_else(|| detect_hardbreaks_block_open(opener).map(|fence_len| (fence_len, false)))
        .or_else(|| detect_quote_block_open(opener).map(|fence_len| (fence_len, false)))
        .or_else(|| detect_container_open(opener).map(|open| (open.fence_len, false)))?;
    // The index keeps repeated unclosed openers in one item linear.
    if index
        .as_ref()
        .map_or(true, |index| index.content_col != content_col)
    {
        let mut next_below = vec![lines.len(); lines.len() + 1];
        let mut closers: HashMap<usize, Vec<usize>> = HashMap::new();
        for at in (0..lines.len()).rev() {
            let line = lines.at(at);
            next_below[at] = if !is_blank_line(line)
                && indent_columns(line) < content_col
                && exact_colon_fence_len(trim_ascii_start(line)).is_none()
            {
                at
            } else {
                next_below[at + 1]
            };
        }
        for at in 0..lines.len() {
            if let Some(width) = exact_colon_fence_len(trim_ascii_start(lines.at(at))) {
                closers.entry(width).or_default().push(at);
            }
        }
        *index = Some(HostColonClosers {
            content_col,
            next_below,
            closers,
        });
    }
    let index = index.as_ref()?;
    let last = index.next_below[end];
    let candidates = index.closers.get(&fence_len)?;
    let first = candidates.partition_point(|&at| at <= end);
    if candidates.get(first).map_or(true, |&at| at >= last) {
        return None;
    }
    // The scan reports an unclosed body at its end too, so a text line and a
    // boundary past the host tell it apart from a closer on the host's last line.
    let host_len = last - start;
    let host: Vec<&str> = (start..last)
        .map(|at| {
            let line = lines.at(at);
            if is_blank_line(line) {
                ""
            } else {
                trim_ascii_start(line)
            }
        })
        .chain(["\u{0}", "\u{0}"])
        .collect();
    let close = find_colon_fence_end_bounded(
        CursorLines::Slice(&host),
        0,
        fence_len,
        literal,
        &mut None,
        &mut None,
        &mut |_, at| at > host_len,
    );
    (close > end - start && close <= host_len).then_some(start + close)
}

/// An own-line `+` keeps an unclosed fence's first blank and the payload after
/// it; only a line that misses the host ends it (PART 0 S1 step 3, carve#5635).
fn own_line_fence_end(
    lines: CursorLines<'_>,
    start: usize,
    end: usize,
    content_col: Option<usize>,
) -> usize {
    let (Some(content_col), Some(open)) = (
        content_col,
        lines.line(start).and_then(|line| detect_fence_open(trim_ascii_start(line))),
    ) else {
        return end;
    };
    if !lines.line(end).is_some_and(is_blank_line)
        || (start + 1..end).any(|at| is_fence_close(trim_ascii_start(lines.at(at)), open))
    {
        return end;
    }
    let opener_col = indent_columns(lines.at(start));
    let mut at = end;
    while let Some(line) = lines.line(at) {
        if is_blank_line(line) {
            at += 1;
            continue;
        }
        let closes = is_fence_close(trim_ascii_start(line), open);
        if indent_columns(line) < content_col && !(closes && indent_columns(line) == opener_col) {
            break;
        }
        at += 1;
        if closes {
            break;
        }
    }
    at
}

pub(crate) fn parse_continuation_block(
    cur: &mut LineCursor,
    options: &Options<'_>,
    base_indent: usize,
    content_col: Option<usize>,
    pull_indent: usize,
    own_line: bool,
    leading: &mut Vec<BlockNode>,
) -> Option<BlockNode> {
    // Definitions belong to the attached block. Nested lists manage their own markers.
    let mut next_visible = cur.pos;
    while let Some(line) = cur.lines.line(next_visible) {
        if let Some(len) =
            standalone_attrs_block_len_iter(cur.lines.range(next_visible..cur.lines.len()))
        {
            next_visible += len;
        } else if is_document_definition_placeholder(line)
            || is_document_abbreviation(line)
            || is_line_comment_any_column(line)
        {
            next_visible += 1;
        } else if let Some(open) = detect_comment_fence_line_any_column(line) {
            // A comment span the walk skipped before the block (carve#4449).
            let close = (next_visible + 1..cur.lines.len()).find(|&k| {
                is_comment_fence_close(trim_ascii_start(cur.lines.at(k)), open.fence_len)
            });
            next_visible = close.map_or(cur.lines.len(), |close| close + 1);
        } else {
            break;
        }
    }
    let comments_precede_block = cur.lines.line(next_visible).is_some_and(|line| {
        !is_blank_line(line) && !(trim_ascii(line) == "+" && indent_columns(line) == base_indent)
    });
    let mut leading_attrs: Option<Attrs> = None;
    let mut leading_attr_slots = None;
    let mut leading_attrs_pos: Option<Pos> = None;
    while cur.pos < next_visible {
        let line = cur.lines.at(cur.pos);
        if standalone_attrs_block_len_iter(cur.lines.range(cur.pos..cur.lines.len())).is_some() {
            let start = cur.pos;
            if let Some(attrs) = parse_standalone_attrs_block(cur) {
                merge_attrs_indexed(&mut leading_attrs, attrs, &mut leading_attr_slots);
                if leading_attrs_pos.is_none() {
                    leading_attrs_pos = span_of(cur, start, cur.pos, options);
                }
            }
        } else if is_document_definition_placeholder(line) {
            cur.consume();
        } else if is_document_abbreviation(line) {
            hoist_document_abbreviation(cur, options);
        } else if !comments_precede_block {
            break;
        } else {
            // The comments stay in the host, ahead of the block (carve#4449).
            let start = cur.pos;
            if let Some(block) = parse_block(cur, options, &mut Vec::new()) {
                if matches!(*block, BlockNode::Comment(_)) {
                    leading.push(*block);
                }
            }
            if cur.pos == start {
                cur.consume();
            }
        }
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
    let following_plus_attaches = cur.lines.line(start + 1).is_some_and(|line| {
        !is_blank_line(line)
            && !detect_list_marker_full(line).is_some_and(|marker| marker.indent <= base_indent)
    });
    let first_blank = (start + 1..lines.len())
        .find(|&at| is_blank_line(lines.at(at)))
        .unwrap_or(lines.len());
    let attached_end = attached_block_end(
        lines,
        start,
        &mut cur.comment_closer_last_index,
        &mut cur.code_closer_last_index,
        &mut |line, end| {
            if (end > start || following_plus_attaches)
                && trim_ascii(line) == "+"
                && indent_columns(line) == base_indent
                && end + 1 < lines.len()
            {
                return true;
            }
            // A sibling item ends the attachment before lazy continuation can claim it.
            end > start
                && ((indent_columns(line) <= base_indent
                    && detect_list_marker_full(line).is_some())
                    || (detect_container_open(lines.at(start)).is_some()
                        && end > first_blank
                        && content_col.is_some_and(|col| {
                            indent_columns(line) < col
                                && exact_colon_fence_len(trim_ascii_start(line)).is_none()
                                && item_block_opener(trim_ascii_start(line))
                        })))
        },
    );
    let attached_end = if own_line {
        own_line_fence_end(lines, start, attached_end, content_col)
    } else {
        attached_end
    };
    // A closer the host holds keeps the colon body open past its first blank;
    // the lines from that blank on are item content (carve#5405).
    let host_closed = content_col.and_then(|content_col| {
        host_closed_colon_end(
            lines,
            start,
            attached_end,
            content_col,
            &mut cur.host_colon_closers,
        )
        .map(|end| (end, attached_end, content_col))
    });
    let mut end = host_closed.map_or(attached_end, |(end, _, _)| end);
    let attached_quote = lines
        .line(start)
        .is_some_and(|line| strip_blockquote_prefix(trim_ascii_start(line)).is_some());
    let caption_at_host = content_col.is_some_and(|column| {
        let first_blank = (start + 1..=end)
            .find(|&pos| lines.line(pos).is_some_and(is_blank_line))
            .unwrap_or(end + 1);
        (start + 1..end.min(first_blank)).any(|pos| {
            let line = lines.at(pos);
            indent_columns(line) == column && caption_content(trim_ascii_start(line)).is_some()
        }) || end < first_blank
            && lines.line(end).is_some_and(|line| {
                indent_columns(line) == column && caption_content(trim_ascii_start(line)).is_some()
            })
    });
    if attached_quote
        && caption_at_host
        && lines.line(end).is_some_and(|line| {
            content_col.is_some_and(|column| indent_columns(line) == column)
                && caption_content(trim_ascii_start(line)).is_some()
        })
    {
        end += 1;
    }
    let caption_column = (attached_quote && caption_at_host)
        .then_some(content_col)
        .flatten();
    // The closer the host test found closes at whatever column it stands.
    let column_at = |pos: usize| match host_closed {
        Some((end, _, _)) if pos + 1 == end => indent_columns(lines.at(pos)),
        Some((_, blank, content_col)) if pos >= blank => content_col,
        _ if caption_column.is_some_and(|column| {
            pos > start
                && indent_columns(lines.at(pos)) == column
                && caption_content(trim_ascii_start(lines.at(pos))).is_some()
        }) =>
        {
            caption_column.expect("checked above")
        }
        _ => pull_indent,
    };
    let stripped = (pull_indent > 0 || host_closed.is_some() || attached_quote && caption_at_host)
        .then(|| {
            (cur.pos..end)
                .map(|pos| slice_columns_mapped(cur.lines.at(pos), column_at(pos), false).0)
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
                let column = column_at(pos);
                let consumed = if column > 0 {
                    slice_columns_mapped(cur.lines.at(pos), column, false).1
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
    // The attached block is the item's content, a marker body (PART 0 S3: an
    // unmarked list marker below the content column ends a quote there).
    sub.in_note_or_description = true;
    let mut floating_attrs = leading_attrs;
    let mut floating_attr_slots = leading_attr_slots;
    let mut floating_attrs_pos = leading_attrs_pos;
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
        let forced = lines_after_refused_brace_runs(&slice, block_start, !cur.in_quoted_body);
        let paragraph_end = attached_block_end(
            sub.lines,
            block_start,
            &mut sub.comment_closer_last_index,
            &mut sub.code_closer_last_index,
            &mut |line, at| {
                at > block_start
                    && (below_item_definition(line, base_indent, content_col)
                        // A block at the item's column interrupts: the paragraph
                        // only stands left of it (§17 L3).
                        || (indent_columns(line) >= content_col
                            && !is_blank_line(line)
                            && !line_starts_paragraph(trim_ascii_start(line))
                            && !forced.get(at).copied().unwrap_or(false)))
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
    if let Some(node) = &mut block {
        promote_delimited_code(node, &sub, block_start, options);
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

/// The lines a refused brace run forces into its paragraph: the line after the
/// run, and through its closer when that line opens a fence (CARVE-P2-035).
fn lines_after_refused_brace_runs(slice: &[&str], from: usize, lazy_refuses: bool) -> Vec<bool> {
    let mut forced = vec![false; slice.len()];
    let mut at = from;
    // An unclosed comment run scans ahead; the `{%` lines it passed open none either.
    let mut comment_scanned_through = 0;
    while at < slice.len() {
        let brace = trim_ascii_start(slice[at]);
        if !brace.starts_with('{') || (brace.starts_with("{%") && at < comment_scanned_through) {
            at += 1;
            continue;
        }
        let count = match refused_brace_run(slice[at..].iter().copied(), lazy_refuses) {
            RefusedBraceRun::Lines(count) => count,
            RefusedBraceRun::No(scanned) => {
                if brace.starts_with("{%") {
                    comment_scanned_through = at.saturating_add(scanned);
                }
                at += 1;
                continue;
            }
        };
        let next = at + count.max(1);
        let Some(line) = slice.get(next) else { break };
        let through = detect_fence_open(trim_ascii_start(line))
            .and_then(|open| {
                (next + 1..slice.len()).find(|&k| is_fence_close(trim_ascii_start(slice[k]), open))
            })
            .unwrap_or(next);
        forced[next..=through].fill(true);
        at = next;
    }
    forced
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
