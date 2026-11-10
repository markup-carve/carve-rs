use super::definition_regions::VerseLine;
use super::*;

/// The `defs` key of a duplicate definition's body, kept only until the nested
/// walk has collected the definitions inside it. No label can start with NUL.
pub(in crate::parse) const LOSING_NOTE_PREFIX: &str = "\u{0}losing:";

fn authored_indent_column(line_number: usize, line: &str) -> usize {
    let walked_column = line
        .chars()
        .take_while(|ch| *ch == ' ' || *ch == '\t')
        .count()
        + 1;
    // The authored map restores stripped prefixes. A tab that remains in the
    // walked line still occupies its full visual width (carve#5541).
    own_lines::authored_visual_column(line_number, walked_column).max(indent_columns(line))
}

fn attached_quote_window_end(
    lines: &[&str],
    lead: usize,
    window_end: usize,
    first_source_line: usize,
    authored_marker: usize,
) -> usize {
    if !lines
        .get(lead)
        .is_some_and(|line| strip_blockquote_prefix(trim_ascii_start(line)).is_some())
    {
        return window_end;
    }
    (lead + 1..window_end)
        .find(|&at| {
            let raw = lines[at];
            let text = trim_ascii_start(raw);
            strip_blockquote_prefix(text).is_none()
                && (is_list_marker(text) || item_block_opener(text))
                && authored_indent_column(first_source_line + at, raw) <= authored_marker
        })
        .unwrap_or(window_end)
}

/// Past a sibling definition the attached block is measured from its own lead (carve#5542).
fn lead_block_end(attached: &[&str], lead: usize, end: usize, options: &Options<'_>) -> usize {
    if lead == 0 || lead >= end {
        return end;
    }
    let lead_end =
        lead + attached_one_block_lines(CursorLines::Slice(&attached[lead..end]), options);
    end.min(lead_end.max(lead + 1))
}

/// A line's indentation inside its note's host, past the host's prefixes.
fn host_indent(host: &NoteHost, raw: &str) -> usize {
    indent_columns(host.content(raw).unwrap_or(raw))
}

/// Whether document lines end in a refused brace run, its columns read inside
/// any quote prefixes (PART 9 §15 A5).
fn lines_end_in_refused_brace_run(lines: &[&str]) -> bool {
    let Some(last) = lines.last().filter(|line| line.contains('}')) else {
        return false;
    };
    fn inside(line: &str) -> &str {
        if trim_ascii_start(line).starts_with('>') {
            without_blockquote_prefixes(trim_ascii_start(line))
        } else {
            line
        }
    }
    // A marker-line brace opens at its written column. Preserve that column
    // while presenting the run to the shared refusal predicate: the marker is
    // a host prefix, not part of the brace paragraph.
    fn brace_line(line: &str) -> String {
        let text = trim_ascii_start(line);
        let body = strip_definition_marker(text)
            .map(|(body, _)| body)
            .or_else(|| detect_list_marker_full(text).map(|marker| marker.content));
        if let Some(body) = body.filter(|body| body.starts_with('{')) {
            if let Some(offset) = line.find(body) {
                let col = line[..offset].chars().fold(0usize, |col, ch| {
                    if ch == '\t' {
                        col + 4 - col % 4
                    } else {
                        col + 1
                    }
                });
                return format!("{}{body}", " ".repeat(col));
            }
        }
        line.to_owned()
    }
    let mut run = vec![brace_line(inside(last))];
    for &line in lines[..lines.len() - 1].iter().rev() {
        let line = brace_line(inside(line));
        if is_blank_line(&line) {
            return false;
        }
        let opens = trim_ascii_start(&line).starts_with('{');
        let closes = line.contains('}') && !line.contains(['"', '\'']);
        run.push(line);
        if opens {
            break;
        }
        // A run closes at its first brace, so an earlier closed line ends the search.
        if closes {
            return false;
        }
    }
    run.reverse();
    ends_in_refused_brace_run(&run)
}

/// The fence a refused brace run's follower on line `i` writes. It is that
/// paragraph's text, so it opens no fence block (CARVE-P2-035).
fn refused_run_follower(lines: &[&str], i: usize) -> Option<FenceOpen> {
    let inside = trim_ascii_start(without_blockquote_prefixes(trim_ascii_start(lines[i])));
    let open = detect_fence_open(inside)?;
    // The run and its follower share one quote host; a lazy or quote-leaving
    // line ends that paragraph's run.
    let (depth, follower) = quote_depth_and_body(lines[i]);
    // A follower below the run's column has left the run's host.
    if i == 0 || indent_columns(follower) < indent_columns(quote_depth_and_body(lines[i - 1]).1) {
        return None;
    }
    for line in lines[..i].iter().rev() {
        let (at_depth, body) = quote_depth_and_body(line);
        if is_blank_line(body) || at_depth != depth {
            return None;
        }
        if trim_ascii_start(body).starts_with('{') {
            break;
        }
    }
    lines_end_in_refused_brace_run(&lines[..i]).then_some(open)
}

/// The quote depth of a line and the text inside its quote prefixes.
fn quote_depth_and_body(line: &str) -> (usize, &str) {
    let mut depth = 0;
    let mut body = line;
    while let Some(inner) = strip_blockquote_prefix(trim_ascii_start(body)) {
        depth += 1;
        body = inner;
    }
    (depth, body)
}

/// Whether the follower on line `i` finds its closer in its own host: a line at
/// its quote depth and indent, before a line that leaves the host.
fn follower_closes_in_host(lines: &[&str], i: usize, open: FenceOpen) -> bool {
    let (depth, body) = quote_depth_and_body(lines[i]);
    let indent = indent_columns(body);
    for line in &lines[i + 1..] {
        let (at_depth, rest) = quote_depth_and_body(line);
        if is_blank_line(rest) {
            continue;
        }
        if at_depth < depth || (at_depth == depth && indent_columns(rest) < indent) {
            return false;
        }
        if at_depth == depth && is_fence_close(trim_ascii_start(rest), open) {
            return true;
        }
    }
    false
}

/// Whether attached line `at` follows a refused brace run, whose paragraph
/// takes it as text (CARVE-P2-035).
fn follows_refused_run(attached: &[String], lead: usize, at: usize) -> bool {
    at > lead && ends_in_refused_brace_run(&attached[lead..at])
}

/// A follower that reaches the note's marker column stays in a refused run's
/// paragraph; a lazy line has no column and misses it (PART 0 S3).
fn follower_keeps_run(
    attached: &[String],
    lead: usize,
    at: usize,
    raw: &str,
    marker_col: usize,
) -> bool {
    !raw.starts_with(LAZY)
        && indent_columns(raw) >= marker_col
        && follows_refused_run(attached, lead, at)
}

/// A list marker ends an attached paragraph (PART 0 S3, carve#4860).
fn marker_ends_attached_paragraph(
    attached: &[String],
    lead: usize,
    at: usize,
    raw: &str,
    marker_col: usize,
) -> bool {
    at > lead
        && is_list_marker(trim_ascii_start(raw))
        && !follower_keeps_run(attached, lead, at, raw, marker_col)
}

/// A `+` takes its block at the block's own column (carve#5542).
fn at_own_column(line: &str, lead_col: usize, marker_col: usize) -> Option<String> {
    (lead_col > marker_col && indent_columns(line) >= lead_col)
        .then(|| strip_leading_columns(line, lead_col))
}

fn marker_column_ends_attached_block(
    attached: &[String],
    raw: &str,
    at: usize,
    first_visible_at: usize,
    marker_col: usize,
    lead_is_quote: bool,
    lead_is_list: bool,
) -> bool {
    at > first_visible_at
        && (marker_col > 0 || lead_is_quote || lead_is_list)
        && indent_columns(raw) <= marker_col
        && (item_block_opener(trim_ascii_start(raw)) || is_list_marker(raw))
        && !(indent_columns(raw) == marker_col
            && follower_keeps_run(attached, first_visible_at, at, raw, marker_col))
}

fn compatible_list_sibling_after_marker(lines: &[&str], start: usize, marker_at: usize) -> bool {
    let Some(first) = lines
        .get(start)
        .and_then(|line| detect_list_marker_full(trim_ascii_start(line)))
    else {
        return false;
    };
    lines
        .get(marker_at + 1)
        .and_then(|line| detect_list_marker_full(line))
        .is_some_and(|next| {
            next.indent == first.indent
                && next.ordered == first.ordered
                && next.checked.is_some() == first.checked.is_some()
                && if first.ordered {
                    next.delim == first.delim
                        && dialect_compatible(ol_dialect(first.ol_type), &next)
                } else {
                    next.marker == first.marker
                }
        })
}

fn continued_list_window(
    lines: &[&str],
    start: usize,
    marker_at: usize,
    options: &Options<'_>,
) -> Option<usize> {
    let marker = lines.get(marker_at)?;
    if !(is_plus_marker(marker)
        || continuation_marker::is_consumed_marker(trim_ascii_start(marker)))
        || !compatible_list_sibling_after_marker(lines, start, marker_at)
    {
        return None;
    }
    Some(
        marker_at
            + 1
            + attached_one_block_lines(CursorLines::Slice(&lines[marker_at + 1..]), options),
    )
}

fn trim_attached_list_window(lines: &[&str], start: usize, mut end: usize) -> usize {
    while end > start && is_blank_line(lines[end - 1]) {
        end -= 1;
    }
    end
}

fn later_marker_end(lines: &[&str], start: usize, end: usize, marker_col: usize) -> usize {
    if !lines
        .get(start)
        .is_some_and(|line| detect_list_marker_full(trim_ascii_start(line)).is_some())
    {
        return end;
    }
    (start + 1..end)
        .find(|&at| {
            (indent_columns(lines[at]) <= marker_col
                && is_plus_marker(lines[at])
                && !compatible_list_sibling_after_marker(lines, start, at))
                || (continuation_marker::is_consumed_marker(trim_ascii_start(lines[at]))
                    && !compatible_list_sibling_after_marker(lines, start, at))
        })
        .unwrap_or(end)
}

/// Whether a blank in an attached list child's fence is followed by a
/// compatible sibling of that list. The one-block list probe owns that blank
/// and sibling; a generic attachment boundary sees only the blank.
pub(super) fn attached_list_fence_sibling(lines: CursorLines<'_>, start: usize) -> bool {
    let Some(first) = lines.line(start).and_then(detect_list_marker_full) else {
        return false;
    };
    let mut fence: Option<(FenceOpen, usize)> = None;
    let mut item_markers = vec![start];
    for at in start + 1..lines.len() {
        let line = lines.at(at);
        if is_blank_line(line) {
            if fence.is_some() {
                let next = (at + 1..lines.len()).find(|&k| !is_blank_line(lines.at(k)));
                if let Some(next) = next.and_then(|k| detect_list_marker_full(lines.at(k))) {
                    return item_markers.iter().rev().any(|&k| {
                        let prior = detect_list_marker_full(lines.at(k)).expect("list marker");
                        next.indent == prior.indent
                            && next.ordered == prior.ordered
                            && next.checked.is_some() == prior.checked.is_some()
                            && if prior.ordered {
                                next.delim == prior.delim
                                    && dialect_compatible(ol_dialect(prior.ol_type), &next)
                            } else {
                                next.marker == prior.marker
                            }
                    });
                }
            }
            return false;
        }
        let indent = indent_columns(line);
        if let Some((open, owner)) = fence {
            if indent >= owner {
                if is_fence_close(trim_ascii_start(line), open) {
                    fence = None;
                }
                continue;
            }
            fence = None;
        }
        if let Some(marker) = detect_list_marker_full(line) {
            if marker.indent < first.indent {
                return false;
            }
            while item_markers.last().is_some_and(|&k| {
                detect_list_marker_full(lines.at(k))
                    .is_some_and(|prior| prior.indent >= marker.indent)
            }) {
                item_markers.pop();
            }
            item_markers.push(at);
        } else if let Some(open) = detect_fence_open(trim_ascii_start(line)) {
            if let Some(owner) = item_markers
                .iter()
                .rev()
                .filter_map(|&k| marker_content_col(lines.at(k)))
                .find(|&column| indent >= column)
            {
                fence = Some((open, owner));
            }
        }
    }
    false
}

fn unfenced_marker_content_col(line: &str) -> Option<usize> {
    let column = marker_content_col(line)?;
    let mut content = line;
    while let Some(marker) = detect_list_marker_full(content) {
        content = marker.content;
    }
    if detect_fence_open(content).is_some() {
        return None;
    }
    Some(column)
}

fn definition_source(note: NoteBody) -> MappedSource {
    // An authored trailing blank (an open fence's payload, corpus 291) survives
    // `str::lines()`; the separator `push_blank` leaves after a `+` block does not.
    let mut source = note.lines.join("\n");
    if note.lines.len() > 1
        && note.lines.last().is_some_and(|line| line.is_empty())
        && note.line_map.last().is_some_and(Option::is_some)
    {
        source.push('\n');
    }
    let mut mapped = MappedSource {
        newlines_counted: (0, 0),
        reached: vec![true; note.lines.len()],
        col_map: note.col_map,
        source,
        line_map: note.line_map,
        item_marker_at_start: false,
        marker_tail: None,
        marker_brace_column: note.marker_brace_column,
        opaque_blank_lines: Vec::new(),
        discarded_outer_blank: false,
        outer_plus_after_blank: false,
        // A FOOTNOTE BODY carries authored base for its sublists, so
        // a list marker past the body's content column anchors its
        // span at the marker (markup-carve/carve#1980).
        in_quoted_body: false,
        sublists_carry_authored_base: true,
    };
    // A marker folded below a marker-line list child's content column leaves
    // a following code fence below that column at the note level. Extraction
    // has stripped the note floor, so walk this local geometry before parsing.
    if note
        .lines
        .first()
        .and_then(|line| unfenced_marker_content_col(line))
        .is_some_and(|content_col| {
            note.lines.windows(2).skip(1).any(|pair| {
                indent_columns(&pair[0]) < content_col
                    && is_list_marker(trim_ascii_start(&pair[0]))
                    && indent_columns(&pair[1]) < content_col
                    && detect_fence_open(trim_ascii_start(&pair[1])).is_some()
            })
        })
    {
        if let Some((walked, shifts)) = walk_body(&mapped.source) {
            for (line, delta) in shifts {
                if let Some(Some(column)) = mapped.col_map.get_mut(line) {
                    *column += delta;
                }
            }
            mapped.set_source(walked);
            mapped.marker_tail = None;
        }
    }
    mapped
}

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
    extract_footnote_defs_seeded(
        source,
        first_source_line,
        positions,
        nested,
        source_line_map,
        options,
        None,
    )
}

/// The labels a note body's scan already knows: the note's own, and each label
/// whose first definition starts before the note (`line`).
pub(in crate::parse) struct DefinedBefore<'d> {
    pub(in crate::parse) own: &'d str,
    pub(in crate::parse) first_lines: &'d HashMap<String, usize>,
    pub(in crate::parse) line: usize,
}

/// The same scan, inside a note body: a definition of a label `outer` already
/// knows loses (PART 9R R2).
pub(in crate::parse) fn extract_footnote_defs_seeded(
    source: &str,
    first_source_line: usize,
    positions: bool,
    nested: bool,
    source_line_map: Option<&[Option<usize>]>,
    options: &Options<'_>,
    outer: Option<DefinedBefore<'_>>,
) -> FootnoteExtraction {
    let lines: Vec<&str> = source.lines().collect();
    let mut scan = FootnoteScan::new(
        source,
        &lines,
        first_source_line,
        positions,
        nested,
        source_line_map,
        options,
    );
    if let Some(outer) = outer {
        scan.definition_keys
            .insert(label_key(outer.own), outer.own.to_string());
        scan.defined_before = Some((outer.first_lines, outer.line));
    }
    let mut i = 0;
    while i < lines.len() {
        i = scan.step(i);
    }
    scan.finish()
}

/// Whether a note body still holds an open paragraph for the lazy line
/// `rest`. A first-block `+` pulls in no list (PART 9 SS17 L4).
fn note_paragraph_open(note: &NoteBody, rest: &str, options: &Options<'_>) -> bool {
    let lines = &note.lines;
    let last = lines.iter().rev().find(|line| !is_blank_line(line));
    if is_list_marker(rest) && last.is_some_and(|line| is_cont_marker_line(line)) {
        return false;
    }
    // Prose after an open paragraph continues it, so only the lines since the
    // last full read are checked; a reparse per lazy line was quadratic.
    if let Some(known) = note.open_paragraph_at.get() {
        if known <= lines.len() && lines[known..].iter().all(|line| continues_paragraph(line)) {
            note.open_paragraph_at.set(Some(lines.len()));
            return true;
        }
    }
    // A nested note that still holds the last line is the deepest leaf; the
    // probe below lifts it out and would answer for the outer note instead.
    let body = open_nested_note_body(lines).unwrap_or_else(|| lines.join("\n"));
    let open = marker_body_ends_with_open_paragraph(&body, options);
    note.open_paragraph_at.set(open.then_some(lines.len()));
    open
}

/// Whether the nested note holding the last line has an open paragraph. Shares
/// `note_paragraph_open`'s cache, so a long body is not reparsed per line.
fn nested_note_paragraph_open(note: &NoteBody, options: &Options<'_>) -> bool {
    let lines = &note.lines;
    if lines.last().map_or(true, |last| is_blank_line(last)) {
        return false;
    }
    if let Some(known) = note.open_paragraph_at.get() {
        if known <= lines.len() && lines[known..].iter().all(|line| keeps_paragraph_open(line)) {
            note.open_paragraph_at.set(Some(lines.len()));
            return true;
        }
    }
    let open = open_nested_note_body(lines)
        .is_some_and(|body| marker_body_ends_with_open_paragraph(&body, options));
    note.open_paragraph_at.set(open.then_some(lines.len()));
    open
}

/// The body of the last nested note, when every line after its marker reaches
/// that note's body column.
fn open_nested_note_body(lines: &[String]) -> Option<String> {
    let mut body = direct_nested_note_body(lines)?;
    while let Some(inner) =
        direct_nested_note_body(&body.lines().map(str::to_owned).collect::<Vec<_>>())
    {
        body = inner;
    }
    Some(body)
}

/// Peel one reachable nested note. The caller keeps peeling until the body
/// belongs to the deepest note, whose last block decides lazy ownership.
fn direct_nested_note_body(lines: &[String]) -> Option<String> {
    let at = lines
        .iter()
        .rposition(|line| parse_footnote_def_line(trim_ascii_start(line)).is_some())?;
    let floor = indent_columns(&lines[at]) + footnote_body_floor(trim_ascii_start(&lines[at]));
    let tail = &lines[at + 1..];
    if !tail
        .iter()
        .all(|line| is_blank_line(line) || line.starts_with(LAZY) || indent_columns(line) >= floor)
    {
        return None;
    }
    let (_, first) = parse_footnote_def_line(trim_ascii_start(&lines[at]))?;
    let mut body = first.to_owned();
    for line in tail {
        body.push('\n');
        body.push_str(&strip_leading_columns(line, floor));
    }
    Some(body)
}

/// A line that can only be paragraph text: it starts with a letter or digit
/// and is no list marker.
fn continues_paragraph(line: &str) -> bool {
    let text = trim_ascii_start(line.strip_prefix(LAZY).unwrap_or(line));
    text.chars().next().is_some_and(char::is_alphanumeric) && !is_list_marker(text)
}

/// A line below an open paragraph that interrupts nothing, whatever it starts with.
fn keeps_paragraph_open(line: &str) -> bool {
    let text = trim_ascii_start(line.strip_prefix(LAZY).unwrap_or(line));
    !is_blank_line(text)
        && !is_list_marker(text)
        && !interrupts_paragraph_with_rest(text, &[])
        && !is_colon_fence_opener_shape(text)
        && !opens_code_fence(text)
        && !text.starts_with(['{', '[', '%', '+', '|', ':', '^'])
}

fn is_cont_marker_line(line: &str) -> bool {
    trim_ascii(without_blockquote_prefixes(trim_ascii_start(line))) == "+"
}

/// The lines of the attribute run that opens on line `at`, if one does.
fn attr_run_at(lines: &[&str], at: usize) -> Option<usize> {
    let line = lines.get(at)?;
    let text = trim_ascii_start(line);
    text.starts_with('{')
        .then(|| attrs_block_len_at(indent_columns(line), text, lines[at + 1..].iter().copied()))
        .flatten()
}

fn first_visible_note_line(lines: &[&str]) -> Option<usize> {
    let mut at = 0;
    while let Some(&line) = lines.get(at) {
        let text = trim_ascii_start(line);
        if let Some((_, first)) = parse_footnote_def_line(text) {
            // A note definition's body belongs to the note (CARVE-P9-067, carve#5034).
            // A comment body, such as a consumed `+`, holds no paragraph to fold into.
            let folds = !is_line_comment_any_column(first);
            at += 1;
            while folds
                && lines
                    .get(at)
                    .is_some_and(|line| own_lines::foldable_plain_line(trim_ascii_start(line)))
                && attr_run_at(lines, at).is_none()
            {
                at += 1;
            }
        } else if is_blank_line(line)
            || is_line_comment_any_column(line)
            || parse_link_def_line(text).is_some()
        {
            at += 1;
        } else if let Some(len) = attr_run_at(lines, at) {
            // An attribute line is invisible: the block below it is the first.
            at += len;
        } else if let Some(open) = detect_comment_fence_line(text) {
            at = lines[at + 1..]
                .iter()
                .position(|next| is_comment_fence_close_any_column(next, open.fence_len))
                .map_or(at + 1, |offset| at + offset + 2);
        } else {
            return Some(at);
        }
    }
    None
}

/// A code fence opener interrupts a note's paragraph whether or not a closer
/// follows (carve#4018).
fn opens_code_fence(rest: &str) -> bool {
    detect_fence_open(rest).is_some()
}

/// A note may fold text below its body column, but a definition that misses
/// every enclosing frame is a document block (PART 0 S4, CARVE-P12-026).
/// Keep the authored quote prefix in the test: `> *[A]: b` is quote text.
/// Colon containers have no content-column entry, so check their open span
/// separately before treating a zero reached column as the document.
fn document_abbreviation_below_note(
    line: &str,
    columns: &ContentColumns,
    document_colon_open: bool,
) -> bool {
    detect_abbreviation_def(trim_ascii_start(line)).is_some()
        && columns.reached_by(indent_columns(line)) == 0
        && !document_colon_open
}

/// Colon containers at document column 0 keep their body at that same column,
/// so a zero reached content column alone does not make an abbreviation a
/// document block. Record their span once for the footnote walk rather than
/// rescanning the preceding source for each note.
fn document_colon_spans(lines: &[&str], fence_closers: &PrepassFenceClosers) -> Vec<bool> {
    let mut spans = Vec::with_capacity(lines.len());
    let mut containers = Vec::new();
    let mut code = None;
    let mut literal = None;
    let mut comment = None;
    for (at, &line) in lines.iter().enumerate() {
        spans.push(!containers.is_empty() || literal.is_some());
        if let Some(open) = code {
            if indent_columns(line) == 0 && is_fence_close(line, open) {
                code = None;
            }
            continue;
        }
        if let Some(run) = literal {
            if indent_columns(line) == 0 && exact_colon_fence_len(line) == Some(run) {
                literal = None;
            }
            continue;
        }
        if let Some(run) = comment {
            if is_comment_fence_close_any_column(line, run) {
                comment = None;
            }
            continue;
        }
        if let Some(open) = detect_comment_fence_line_any_column(line) {
            comment = Some(open.fence_len);
            continue;
        }
        if indent_columns(line) != 0 {
            continue;
        }
        if let Some(open) = detect_fence_open(line) {
            if fence_closers.closes_after_at(at, open, 0) {
                code = Some(open);
            }
            continue;
        }
        if let Some(run) = detect_line_block_open(line)
            .or_else(|| detect_hardbreaks_block_open(line))
            .or_else(|| detect_quote_block_open(line))
        {
            literal = Some(run);
            continue;
        }
        if let Some(run) = exact_colon_fence_len(line) {
            if containers.last() == Some(&run) {
                containers.pop();
                continue;
            }
        }
        if let Some(open) = detect_container_open(line) {
            containers.push(open.fence_len);
        }
    }
    spans
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Flow {
    Next,
    Stop,
}

/// Open-fence state for the document walk.
#[derive(Default)]
struct PrepassFenceState {
    open: Option<FenceOpen>,
    term_quotes: usize,
    term_columns: Vec<usize>,
    term_owned: bool,
    description: bool,
    description_blank: bool,
    inline_marker: bool,
    inline_paragraph: Option<PrepassParagraph>,
    term_floor: usize,
    // Whether that fence has a closer, which decides whether a quoted line ends it.
    /// A refused brace run's follower: its paragraph, and so the fence, ends at a blank.
    paragraph_text: bool,
}

impl PrepassFenceState {
    fn ends_before(&self, line: &str, open: FenceOpen) -> bool {
        (self.paragraph_text && is_blank_line(without_blockquote_prefixes(trim_ascii_start(line))))
            || inline_fence_paragraph_ends(line, open, self.inline_paragraph.as_ref())
            || (self.description
                && (self.description_blank || !self.inline_marker)
                && description_fence_leaves_after_blank(line, open, &self.term_columns))
            || !inline_fence_lazy_line(line, self.inline_paragraph.as_ref())
                && ((self.inline_marker && is_blank_line(without_blockquote_prefixes(line)))
                    || (self.term_owned
                        && term_fence_ends_above(line, self.term_floor, &self.term_columns))
                    || prepass_fence_ends_above(line, open))
    }
}

/// One definition's body as it is gathered: its lines, the document line each
/// came from, and (with positions on) the column each was stripped at.
struct NoteBody {
    lines: Vec<String>,
    comment_state: BodyCommentState,
    stanza: crate::parse::definition::LineBlockTail,
    open_colons: OpenBodyColons,
    line_map: Vec<Option<usize>>,
    col_map: Vec<Option<isize>>,
    marker_brace_column: Option<usize>,
    positions: bool,
    /// `lines.len()` when the body last read as ending in an open paragraph.
    open_paragraph_at: Cell<Option<usize>>,
    /// The code fence the trailing quoted run leaves open, with its quote depth.
    quoted_fence: Option<(FenceOpen, usize)>,
    /// A brace run opened on a lazy line keeps source columns when a later
    /// line reaches the note's content column.
    lazy_brace_open: bool,
    /// The synthetic blank that ends a hosted `+` first block. It separates
    /// blocks; it does not mark the start of an attached one.
    block_separator: Option<usize>,
}

impl NoteBody {
    fn frame_lazy(&self, column: usize, text: &str) -> String {
        // A line admitted by an enclosing note already carries its ownership
        // frame. Nesting that frame would leak its inner marker into prose.
        if text.starts_with(LAZY) {
            return text.to_string();
        }
        if self.lazy_brace_open || trim_ascii_start(text).starts_with("{%") {
            frame_lazy_at_column(LAZY, column, text)
        } else {
            format!("{LAZY}{text}")
        }
    }

    fn push_with(
        &mut self,
        line: String,
        source_line: Option<usize>,
        col: impl FnOnce(&str) -> Option<isize>,
    ) {
        let body = trim_ascii_start(strip_lazy(&line));
        if is_blank_line(&line) || self.lazy_brace_open && line.contains("%}") {
            self.lazy_brace_open = false;
        } else if line.starts_with(LAZY) && body.starts_with("{%") {
            self.lazy_brace_open = !line.contains("%}");
        }
        let opaque = self.comment_state.code.is_some() || self.comment_state.comment.is_some();
        self.comment_state.observe(&line);
        self.stanza.step(&line);
        if !opaque && self.comment_state.code.is_none() && self.comment_state.comment.is_none() {
            self.open_colons.observe(&line);
        }
        if self.positions {
            self.col_map.push(col(&line));
        }
        self.quoted_fence = next_quoted_fence(self.quoted_fence, &line);
        self.lines.push(line);
        self.line_map.push(source_line);
    }

    fn push_blank(&mut self) {
        self.push_with(String::new(), None, |_| None);
    }

    /// Does the body end inside a code fence a blank run still reaches? A fence
    /// a `+` attached (after its synthetic separator) ends at the blank; one
    /// nested in an attached block does not (PART 0 S1, carve#5539).
    fn holds_open_fence(&self) -> bool {
        let Some(opener) = open_code_fence_opener(&self.lines) else {
            return false;
        };
        let attached = self.line_map[..opener]
            .iter()
            .enumerate()
            .rposition(|(at, line)| line.is_none() && self.block_separator != Some(at))
            .is_some_and(|separator| {
                self.lines[separator + 1..opener].iter().all(|l| {
                    !is_blank_line(l)
                        && (is_line_comment_any_column(l)
                            || is_document_definition_placeholder(l)
                            || parse_standalone_attrs(trim_ascii_start(l)).is_some())
                })
            });
        // An own-line plus grants only the first block. Once an authored blank
        // has ended that grant and body text has continued the still-open
        // fence, later blanks are ordinary fence payload (carve#5560).
        if !attached {
            return true;
        }
        let mut past_authored_blank = false;
        for at in opener + 1..self.lines.len() {
            if is_blank_line(&self.lines[at]) {
                past_authored_blank |= self.line_map[at].is_some();
            } else if past_authored_blank {
                return true;
            }
        }
        false
    }
}

/// What one document line looks like once its container prefix is stripped.
struct LineScan<'a> {
    stripped: StrippedContainerLine<'a>,
    after_term: bool,
    fence_line: &'a str,
    /// The column of a fence written at a list item's content column.
    fence_col: Option<usize>,
    /// A fence line right after a refused brace run, and whether a closer
    /// follows it. It is that paragraph's text (CARVE-P2-035).
    refused_follower: Option<bool>,
}

/// A line that parsed as a footnote definition and passed the fold guards.
struct DefinitionLine<'a> {
    label: &'a str,
    first: &'a str,
    def_line: &'a str,
    authored_quote_def: bool,
}

/// State of an unindented-host note body walked by `run_indented_body!`.
struct IndentedNote<'a> {
    label: &'a str,
    first: &'a str,
    container: IndentContainer,
    body_indent: usize,
    /// A quote's `+` granted the definition's current paragraph only.
    quote_granted: bool,
    /// Authored marker column when the marker-line `+` took a quote.
    attached_quote_marker: Option<usize>,
    def_column: usize,
    span_note_end: Option<(usize, bool)>,
    note_fence: Option<FenceOpen>,
    // A fence on a list item's marker line in the body: its payload is never
    // a definition, though the body's own fence state does not track it.
    marker_fence: Option<(FenceOpen, usize)>,
    // Is this note's body still open for an INDENTED definition below
    // it? A flush line opens it and a block opener closes it. Only
    // whether, not where: a footnote definition registers wherever it
    // lands and is hoisted either way, so no column bound is enforced.
    body_open_below: bool,
    open_term: bool,
    retained_below_comment: bool,
    below_comment_span: Option<usize>,
    // Content columns of the items holding the note, and the one holding the
    // span: a line left of that item ends the span (PART 0 S1).
    host_items: Vec<usize>,
    span_floor: Option<usize>,
    // A matched comment is one invisible extent. Its closer leaves no open
    // paragraph for a later line below this note's body column to continue.
    closed_comment_paragraph: bool,
    lazy_open: bool,
    marker_line_note: bool,
}

/// The quote / list / description host a container-hosted note sits in.
struct NoteHost {
    description_host: bool,
    quote_indents: Vec<usize>,
    quote_depth: usize,
    note_floor: usize,
    /// Content column of the item hosting the note at the document level,
    /// outside the note's quotes.
    outer_list_floor: Option<usize>,
    /// A task box keeps the note at its literal column (carve#4352).
    behind_task_box: bool,
}

impl NoteHost {
    // Measure indentation inside the note's existing quote host.
    // A further quote marker is note content and stays in the body.
    fn content<'l>(&self, raw: &'l str) -> Option<&'l str> {
        strip_indented_quote_levels(raw, &self.quote_indents)
    }

    fn blank(&self, raw: &str) -> bool {
        self.content(raw).is_some_and(is_blank_line)
    }

    // A comment below the note's floor closes it (rule A, carve#3885).
    fn reaches_floor(&self, raw: &str, admitted_fence: bool, comment_open: bool) -> bool {
        self.content(raw).is_some_and(|line| {
            admitted_fence || comment_open || indent_columns(line) >= self.note_floor
        })
    }
}

fn behind_task_box(raw_def_line: &str) -> bool {
    let mut line = without_blockquote_prefixes(trim_ascii_start(raw_def_line));
    while let Some(marker) = detect_list_marker_full(line) {
        if marker.checked.is_some() {
            return true;
        }
        line = marker.content;
    }
    false
}

fn item_columns(frame: &ColumnFrame) -> Vec<usize> {
    frame
        .cols
        .iter()
        .copied()
        .filter(|col| !frame.body_columns.contains(col))
        .collect()
}

fn item_floor(frame: &ColumnFrame) -> Option<usize> {
    frame
        .cols
        .iter()
        .copied()
        .filter(|col| !frame.body_columns.contains(col))
        .max()
}

/// Comment and fence opacity of a container-hosted note body.
struct HostedWalk {
    /// The body column of a nested note a line may still be lazy in.
    nested_note: Option<usize>,
    track: bool,
    marker_fence: Option<(FenceOpen, usize)>,
    opacity: BodyCommentState,
    admitted_fence: Option<(FenceOpen, usize)>,
    lazy_open: bool,
}

impl HostedWalk {
    fn reaches_floor(&self, host: &NoteHost, raw: &str) -> bool {
        host.reaches_floor(
            raw,
            self.admitted_fence.is_some(),
            self.opacity.comment.is_some(),
        )
    }
}

struct FootnoteScan<'a, 'o> {
    source: &'a str,
    lines: &'a [&'a str],
    first_source_line: usize,
    positions: bool,
    nested: bool,
    source_line_map: Option<&'a [Option<usize>]>,
    options: &'a Options<'o>,
    recent_list_marker: Vec<bool>,
    // See `probe_budget_for`: spent by `line_folds_into_an_open_paragraph`, and
    // running out only ever collects a definition this guard would have left as
    // text.
    probe_budget: usize,
    body: Vec<String>,
    body_line_map: Vec<Option<usize>>,
    defs: BTreeMap<String, MappedSource>,
    definition_keys: BTreeMap<String, String>,
    /// A registered note passed by a preceding note's first-block grant.
    passed_note: Option<String>,
    /// In a note body: each label's first definition line, and the note's line.
    defined_before: Option<(&'a HashMap<String, usize>, usize)>,
    // Where each definition was written. Lines and columns are final here; the
    // OFFSETS are filled by the caller, which is the only place that holds the
    // original text the offsets are measured in - the same split `fill_offsets`
    // already uses for every other block.
    def_positions: BTreeMap<String, Pos>,
    note_link_defs: BTreeMap<String, LinkDef>,
    body_opacity_candidates: Option<bool>,
    document_colon_open: Vec<bool>,
    fence: PrepassFenceState,
    // A LINE BLOCK's body is inline content, so a definition-shaped line inside
    // one is text. Without this the line was extracted here and never reached
    // the block parser, so it vanished from the output (#491). carve-js keeps it
    // and does NOT register the footnote; this matches that.
    verse_lines: Vec<VerseLine>,
    // A `%%%` comment fence is opaque, so a literal `::: |` inside one is not
    // an opener. Entering the state there left it open past the comment's own
    // closer - which is not a colon fence - and every later definition in the
    // document was skipped. Comment bodies remain opaque here.
    in_comment_fence: Option<OpenCommentFence>,
    closer_maxima: PrepassFenceClosers,
    folded_into_term: TermLineOwnership,
    // Built on the first CONTAINER-scoped opener and never for a document that
    // has none, which is every document that only ever writes `%%%` at column 0.
    container_closers: Option<ContainerCommentClosers>,
    comment_closers: Option<HashMap<usize, usize>>,
    closer_index: Option<HashMap<u8, Vec<usize>>>,
    line_block_closers: Option<HashMap<usize, Vec<usize>>>,
    // See ContentColumns: a definition on an item's CONTINUATION line carries no
    // marker, so without this the line kept its indentation, stopped looking
    // like a definition, and was neither collected nor rendered - the author's
    // line disappeared and a reference to it stayed literal (carve-rs#568).
    columns: ContentColumns,
    // The line the `after_term` gate below asks about. See its note: a blank
    // line between entries is transparent to a definition list, so the gate
    // reads past one.
    previous_line: PreviousLine,
    last_folded_marker: bool,
}

impl<'a, 'o> FootnoteScan<'a, 'o> {
    fn new(
        source: &'a str,
        lines: &'a [&'a str],
        first_source_line: usize,
        positions: bool,
        nested: bool,
        source_line_map: Option<&'a [Option<usize>]>,
        options: &'a Options<'o>,
    ) -> Self {
        let recent_list_marker = list_marker_since_blank(lines);
        let probe_budget = probe_budget_for(source.len());
        let verse_lines = verse_owned_lines(source, options);
        let closer_maxima = fence_closer_suffix_maxima(lines);
        let folded_into_term = term_folded_lines(lines, &closer_maxima, options);
        let document_colon_open = if source.contains("*[") && source.contains(":::") {
            document_colon_spans(lines, &closer_maxima)
        } else {
            Vec::new()
        };
        Self {
            source,
            lines,
            first_source_line,
            positions,
            nested,
            source_line_map,
            options,
            recent_list_marker,
            probe_budget,
            body: Vec::new(),
            body_line_map: Vec::new(),
            defs: BTreeMap::new(),
            definition_keys: BTreeMap::new(),
            passed_note: None,
            defined_before: None,
            def_positions: BTreeMap::new(),
            note_link_defs: BTreeMap::new(),
            body_opacity_candidates: None,
            document_colon_open,
            fence: PrepassFenceState::default(),
            verse_lines,
            in_comment_fence: None,
            closer_maxima,
            folded_into_term,
            container_closers: None,
            comment_closers: None,
            closer_index: None,
            line_block_closers: None,
            columns: ContentColumns::new(),
            previous_line: PreviousLine::default(),
            last_folded_marker: false,
        }
    }

    fn decision_at(&self, line: usize) -> Option<continuation_marker::Decision> {
        let physical = match self.source_line_map {
            Some(map) => map
                .get(line.checked_sub(self.first_source_line)?)
                .copied()
                .flatten()?,
            None => line,
        };
        continuation_marker::decision(physical)
    }

    fn keep_line(&mut self, i: usize) {
        self.body.push(self.lines[i].to_string());
        self.body_line_map.push(Some(self.first_source_line + i));
    }

    /// A definition left of its line block's fence is lazy, and a footnote
    /// definition interrupts the stanza rather than joining it.
    fn lazy_definition_in_verse(&self, i: usize) -> bool {
        let line = self.lines[i];
        if !self.verse_lines[i].is_body()
            || trim_ascii_start(line).starts_with('>')
            || parse_footnote_def_line(trim_ascii_start(line)).is_none()
        {
            return false;
        }
        let Some(opener) = (0..i).rev().find(|&at| !self.verse_lines[at].is_body()) else {
            return false;
        };
        let opener = self.lines[opener];
        let Some(fence) = opener.find(":::") else {
            return false;
        };
        let column = opener[..fence].chars().fold(0, |col, ch| {
            if ch == '\t' {
                col + 4 - col % 4
            } else {
                col + 1
            }
        });
        indent_columns(line) < column
    }

    /// Handles line `i` and returns the index of the next unhandled line.
    fn step(&mut self, i: usize) -> usize {
        let scan = self.observe_line(i);
        if self.verse_lines[i].is_owned() && !self.lazy_definition_in_verse(i) {
            self.keep_line(i);
            return i + 1;
        }
        if self.continue_fence(i, scan.fence_line)
            || self.step_comment_fence(i)
            || self.step_fence_opener(i, &scan)
        {
            return i + 1;
        }
        match self.definition_at(i, &scan) {
            Some(def) => self.collect_definition(i, &scan, def),
            None => {
                self.keep_line(i);
                i + 1
            }
        }
    }

    fn observe_line(&mut self, i: usize) -> LineScan<'a> {
        let lines = self.lines;
        let line = lines[i];
        let follower = refused_run_follower(lines, i);
        let refused_follower = follower.map(|open| {
            self.closer_maxima.has_run_after(i, open) && follower_closes_in_host(lines, i, open)
        });
        let folded_fence = refused_follower == Some(false)
            || detect_fence_open(trim_ascii_start(line)).is_some_and(|open| {
                !self.closer_maxima.has_run_after(i, open)
                    && !i.checked_sub(1).is_some_and(|above| {
                        strip_blockquote_prefix(trim_ascii_start(lines[above])).is_some()
                            && strip_blockquote_prefix(trim_ascii_start(line)).is_none()
                    })
                    && (prepass_paragraph_open_above(lines, i, &[], &self.folded_into_term)
                        || lines_end_in_refused_brace_run(&lines[..i]))
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
        let folded_marker = marker_line_may_be_lazy(line)
            && (self.last_folded_marker || !marker_follows_sibling(&self.body, line))
            && line_folds_into_an_open_paragraph(
                &self.body,
                line,
                self.options,
                &mut self.probe_budget,
                &mut marker_probe_unaffordable,
            );
        self.last_folded_marker = folded_marker;
        let opaque = self.fence.open.is_some()
            || folded_fence
            || folded_marker
            || self.verse_lines[i].is_body()
            || self.in_comment_fence.is_some()
            || prepass_quote_line_is_folded(&self.folded_into_term, i, line);
        let content_col = self.columns.observe(line, opaque);
        // A quote nested in a list item sits AT the item's content column, so
        // the prefix scan needs that column to see it (carve-rs#588).
        //
        // THE PREVIOUS NON-BLANK LINE, not the one directly above: a blank
        // between description entries does not end the list, it only makes it
        // loose. Asking the line above left the `:  ` marker unstripped after
        // one, so a definition written as that description's body was consumed
        // by the parser and collected by nobody (#1500).
        let after_term = self.previous_line.after_term();
        let stripped = strip_container_prefixes_at(line, &self.columns, after_term);
        remember_prepass_paragraph_line(&self.folded_into_term, i, line, &stripped, &self.columns);
        self.previous_line.note(line);
        // A footnote definition is NEVER collected from inside a fenced code
        // block: a `[^x]: ...` line there is literal content. The prepass has
        // only a prefix-stripped line, not the block parser's container-column
        // context, so it recognizes fences only with no residual indentation.
        // This can collect a def from a container-nested fence body, but avoids
        // opening a fence the block parser never opens and swallowing every
        // later definition in the document.
        let mut fence_col = None;
        let fence_line = match self.folded_into_term.fence_openers.get(&i) {
            Some(host) => &line[host.opener_offset..],
            None if !folded_fence && !stripped.description => {
                // As `extract_link_defs_with_guard_full`: a fence at an item's
                // content column is payload for both collectors.
                let kept = strip_container_prefixes_keep_indent(line);
                let kept_indent = leading_ws(&kept);
                let authored = trim_ascii_start(&kept);
                if kept_indent >= content_col
                    && (content_col > 0 || kept_indent <= 3)
                    && line.ends_with(authored)
                    && detect_fence_open(authored).is_some()
                {
                    fence_col = Some(kept_indent);
                    &line[line.len() - authored.len()..]
                } else {
                    stripped.bare
                }
            }
            None => stripped.bare,
        };
        LineScan {
            stripped,
            after_term,
            fence_line,
            fence_col,
            refused_follower,
        }
    }

    /// Inside an open fence: keeps line `i` as payload, or ends the fence and
    /// returns false so the line is read afresh.
    fn continue_fence(&mut self, i: usize, fence_line: &str) -> bool {
        let Some(open) = self.fence.open else {
            return false;
        };
        let line = self.lines[i];
        if self.fence.ends_before(line, open) {
            self.fence.open = None;
            return false;
        }
        advance_inline_paragraph(line, self.fence.inline_paragraph.as_mut());
        end_prepass_paragraph_at(&self.folded_into_term, i);
        self.keep_line(i);
        let state = &mut self.fence;
        let close_kept = if state.term_quotes > 0 {
            strip_prepass_quote_levels(line, state.term_quotes).unwrap_or(line)
        } else {
            line
        };
        let close_line = if !open.quoted && open.content_col == 0 {
            fence_line
        } else if leading_ws(close_kept) >= open.content_col {
            &close_kept[open.content_col..]
        } else {
            close_kept
        };
        state.description_blank = state.description && is_blank_line(close_kept);
        if (!state.description || leading_ws(close_kept) >= open.content_col)
            && is_fence_close(close_line, open)
        {
            if state.inline_marker {
                self.folded_into_term
                    .paragraph_quotes
                    .borrow_mut()
                    .insert(i, state.inline_paragraph.clone());
            }
            state.open = None;
        }
        true
    }

    fn step_comment_fence(&mut self, i: usize) -> bool {
        let line = self.lines[i];
        if self.in_comment_fence.is_some_and(|open| {
            !closes_open_comment_fence(line, open) && misses_comment_owner(line, open)
        }) {
            self.in_comment_fence = None;
        }
        if let Some(open) = self.in_comment_fence {
            end_prepass_paragraph_at(&self.folded_into_term, i);
            // ANY column, matching `comment_fence_close_index` and the block
            // parser. A comment fence closes at whatever indent its closer sits
            // at (PART 9 §24 C3, markup-carve/carve#629), so testing the strict
            // form here made this pass disagree with the one that decides: the
            // block parser closed the comment, this scan did not, and every
            // definition after it went unregistered and came back as visible
            // text (#574 regression). S1 also accepts a closer with fewer
            // quote prefixes than its opener; see `closes_open_comment_fence`.
            if closes_open_comment_fence(line, open) {
                self.in_comment_fence = None;
            }
            // A comment's body is OPAQUE: a `[^a]: note` inside one is comment
            // text and defines nothing. The state was tracked here already, but
            // only to gate the line-block opener below, so the scan still walked
            // in and registered the footnote - producing an endnote nobody wrote
            // and a live reference for a later `see [^a]` (#504).
            self.keep_line(i);
            return true;
        }
        let Some((open, scope)) = detect_comment_fence_opener_scoped(line, &self.columns) else {
            return false;
        };
        end_prepass_paragraph_at(&self.folded_into_term, i);
        self.in_comment_fence = Some(OpenCommentFence {
            fence_len: open.fence_len,
            quote_depth: scope.quote_depth(),
            floor: scope.floor(),
        });
        self.keep_line(i);
        true
    }

    fn step_fence_opener(&mut self, i: usize, scan: &LineScan<'a>) -> bool {
        let Some(mut open) = detect_fence_open(scan.fence_line)
            .filter(|_| !self.folded_into_term.folded.contains(&i))
        else {
            return false;
        };
        let line = self.lines[i];
        let follower = scan
            .refused_follower
            .filter(|_| !self.folded_into_term.fence_openers.contains_key(&i));
        if follower == Some(false) {
            self.keep_line(i);
            return true;
        }
        let term_fence = self.folded_into_term.fence_openers.get(&i);
        if scan.stripped.description {
            open.content_col =
                description_fence_content_column(line, &scan.stripped, &self.columns);
        } else if let Some(col) = scan.fence_col {
            open.content_col = col;
        }
        if let Some(host) = term_fence {
            open.content_col = host.content_col;
            open.quoted = host.quote_depth > 0;
        }
        // §10 I4, as `extract_link_defs_with_guard`: an unterminated fence
        // interrupts no open paragraph, so going opaque here hid every
        // definition below one from collection (carve-rs#2096).
        let fence = prepass_fence_opens(
            self.lines,
            i,
            &self.folded_into_term,
            &self.columns,
            scan.after_term,
        );
        if fence.opens || (scan.stripped.description && term_fence.is_none()) {
            if !open.quoted && fence.quote_columns.is_empty() && fence.host_content_col > 0 {
                open.content_col = fence.host_content_col;
            }
            end_prepass_paragraph_at(&self.folded_into_term, i);
            // A fence on a quoted item's marker line is measured from inside the
            // quote, so a quoted line's raw indent does not end it.
            open.quoted |= !fence.quote_columns.is_empty()
                && scan
                    .stripped
                    .structural
                    .chars()
                    .any(|character| !matches!(character, '>' | ' ' | '\t'));
            let inline_paragraph = if fence.inline_marker {
                previous_prepass_paragraph(&self.folded_into_term, i)
            } else {
                None
            };
            let term_quotes = term_fence.map_or(fence.quote_columns.len(), |host| host.quote_depth);
            let term_columns = term_fence
                .map(|host| host.quote_columns.clone())
                .unwrap_or(fence.quote_columns);
            let description = scan.stripped.description && term_fence.is_none();
            let term_floor = if description {
                0
            } else {
                term_fence.map_or(fence.host_content_col, |host| host.content_col)
            };
            self.fence = PrepassFenceState {
                open: Some(open),
                term_quotes,
                term_owned: term_fence.is_some() || !term_columns.is_empty(),
                term_columns,
                description,
                description_blank: false,
                inline_marker: fence.inline_marker,
                inline_paragraph,
                term_floor,
                paragraph_text: follower.is_some(),
            };
        }
        self.keep_line(i);
        true
    }

    fn definition_at(&mut self, i: usize, scan: &LineScan<'a>) -> Option<DefinitionLine<'a>> {
        let line = self.lines[i];
        let stripped = &scan.stripped;
        let def_indent = indent_columns(stripped.bare);
        let def_line =
            if self.nested && parse_footnote_def_line(trim_ascii_start(stripped.bare)).is_some() {
                // A nested note-body definition at any indent (see `nested`).
                trim_ascii_start(stripped.bare)
            } else {
                at_content_column(
                    stripped.bare,
                    stripped.structural,
                    self.columns.reached_by(def_indent),
                )
            };
        let authored_quote_def = !self.nested
            && self.recent_list_marker[i]
            && strip_blockquote_prefix(trim_ascii_start(def_line))
                .is_some_and(|content| parse_footnote_def_line(content).is_some());
        let def_line = if authored_quote_def {
            without_blockquote_prefixes(trim_ascii_start(def_line))
        } else {
            def_line
        };
        // THE PROBE RUNS ONLY ON A LINE THAT PARSED as a definition. Hoisting it
        // out spent the budget on every ordinary `- item` too, and the budget is
        // shared: four hundred of them ahead of a real definition exhausted it
        // and the definition was declined.
        // A marker has already established the body column. Further content
        // space before the opener does not turn its block into inline text.
        let def_line = if stripped.needs_empty_list_content {
            def_line.trim_start_matches(' ')
        } else {
            def_line
        };
        let (label, first) = parse_footnote_def_line(def_line)?;
        // PART 9R R1a: a line that folds into an open paragraph defines nothing,
        // and a probe that cannot afford to say so collects nothing either. The
        // two are one condition here - the fallback is conservative, so it may
        // decline a definition an affordable probe would have taken.
        let mut unaffordable = false;
        let folds = marker_line_may_be_lazy(line)
            && line_folds_into_an_open_paragraph(
                &self.body,
                line,
                self.options,
                &mut self.probe_budget,
                &mut unaffordable,
            );
        if folds || unaffordable || self.folded_into_term.folded.contains(&i) {
            return None;
        }
        // The line after a refused brace run is its paragraph's text (CARVE-P2-035).
        if lines_end_in_refused_brace_run(&self.lines[..i]) {
            return None;
        }
        Some(DefinitionLine {
            label,
            first,
            def_line,
            authored_quote_def,
        })
    }

    /// Collects the definition opened on line `i` and returns the line after it.
    fn collect_definition(
        &mut self,
        i: usize,
        scan: &LineScan<'a>,
        mut def: DefinitionLine<'a>,
    ) -> usize {
        // A link definition on the marker line is hoisted like one on a body line.
        if self.hoist_note_link_def(def.first, i) {
            static PLACEHOLDER: std::sync::OnceLock<String> = std::sync::OnceLock::new();
            def.first = PLACEHOLDER.get_or_init(super::definition_placeholder);
        }
        let stripped = &scan.stripped;
        let in_container = !stripped.structural.is_empty();
        let normalized_label = label_key(def.label);
        let first_for_label = !self.is_defined(&normalized_label);
        let def_start_line = self.first_source_line + i;
        let raw_def_line = self.lines[i];
        if self.positions && first_for_label {
            self.record_definition_start(def.label, raw_def_line, def.def_line, def_start_line);
        }
        // A losing duplicate's `+` takes nothing: the lines below it are read
        // where they stand, as if the marker line were not there (PART 9R R2,
        // carve#5502).
        if !first_for_label && continuation_marker::is_consumed_marker(def.first) {
            self.push_definition_replacement(raw_def_line, &def, stripped, def_start_line, false);
            return i + 1;
        }
        let mut i = i + 1;
        let mut comment_state = BodyCommentState::new();
        comment_state.observe(def.first);
        let mut stanza = crate::parse::definition::LineBlockTail::default();
        stanza.step(def.first);
        let body_indent = footnote_body_floor(raw_def_line);
        // A run opened by the marker-line body compares continuations with
        // the body's column. The note label contributes no width to that
        // column (CARVE-P2-035, carve#5748).
        let marker_brace_column = def.first.starts_with('{').then_some(body_indent);
        let mut note = NoteBody {
            lines: vec![def.first.to_string()],
            comment_state,
            stanza,
            open_colons: OpenBodyColons::from_source(def.first),
            line_map: vec![Some(def_start_line)],
            col_map: if self.positions {
                vec![stripped_col(
                    Some(stripped.structural.chars().count() as isize),
                    stripped.bare,
                    def.first,
                )]
            } else {
                Vec::new()
            },
            marker_brace_column,
            positions: self.positions,
            open_paragraph_at: Cell::new(None),
            block_separator: None,
            quoted_fence: next_quoted_fence(None, def.first),
            lazy_brace_open: false,
        };
        let quote_granted = def_start_line > self.first_source_line
            && self
                .decision_at(def_start_line - 1)
                .is_some_and(|decision| decision.owner == Some(continuation_marker::Kind::Quote));
        let container = IndentContainer {
            marker_column: indent_columns(def.first),
            content_column: body_indent,
            owner: continuation_marker::Kind::Footnote,
        };
        if first_for_label && !in_container {
            i = self.skip_losing_plus_lines(i, def.label, def.first, raw_def_line);
            i = self.lone_plus_first_block(
                &mut note,
                i,
                def.label,
                def.first,
                container,
                def_start_line,
            );
        }
        // Once the marker-line grant has taken its list, a later marker at
        // the note's column cannot grant that note a second block.
        let detached_after_plus = trim_ascii(def.first) == "+"
            && note.line_map.first() == Some(&None)
            && self.lines.get(i).is_some_and(|line| {
                (indent_columns(line) <= indent_columns(raw_def_line) && is_plus_marker(line))
                    || continuation_marker::is_consumed_marker(trim_ascii_start(line))
            })
            && self
                .lines
                .get(i + 1)
                .is_some_and(|line| detect_list_marker_full(trim_ascii_start(line)).is_some());
        if detached_after_plus {
            i += 1;
        }
        // A refused marker after the marker-line grant's block renders nothing;
        // the note keeps the next line at its body column (carve#5601).
        if trim_ascii(def.first) == "+"
            && !detached_after_plus
            && note.line_map.first() == Some(&None)
            && indent_columns(raw_def_line) == 0
            && self.lines.get(i).is_some_and(|line| {
                indent_columns(line) == 0
                    && continuation_marker::is_consumed_marker(trim_ascii_start(line))
            })
            && self
                .lines
                .get(i + 1)
                .is_some_and(|line| !is_blank_line(line) && indent_columns(line) >= body_indent)
        {
            i += 1;
        }
        let attached_quote_marker = (trim_ascii(def.first) == "+"
            && note.line_map.first() == Some(&None)
            && note
                .lines
                .iter()
                .find(|line| !is_blank_line(line))
                .is_some_and(|line| strip_blockquote_prefix(trim_ascii_start(line)).is_some()))
        .then(|| {
            own_lines::authored_visual_column(def_start_line, indent_columns(raw_def_line) + 1)
        });
        // An authored quote may be deeper than the item's content column.
        // Its prefix is still a container for note-body lines.
        if self
            .passed_note
            .as_deref()
            .is_some_and(|key| key != normalized_label)
        {
            // A later winning `+` definition owns the grant's first block.
            // Leave its definition line for the outer walk to collect.
        } else if detached_after_plus {
            // The next list is read at the document level.
        } else if !in_container && !def.authored_quote_def {
            let state = IndentedNote {
                label: def.label,
                first: def.first,
                container,
                body_indent,
                quote_granted,
                attached_quote_marker,
                def_column: indent_columns(raw_def_line),
                span_note_end: None,
                // A marker starts the body, so a fence on it opens (PART 0 O1).
                note_fence: detect_fence_open(def.first).map(|mut open| {
                    open.content_col = body_indent;
                    open
                }),
                marker_fence: marker_line_fence(def.first),
                body_open_below: false,
                open_term: term_start_column(def.first).is_some(),
                retained_below_comment: false,
                below_comment_span: None,
                host_items: item_columns(&self.columns.frames[0]),
                span_floor: None,
                closed_comment_paragraph: false,
                lazy_open: false,
                // A note on its outer note's marker line takes the lazy lines that note gave up.
                marker_line_note: i == 1,
            };
            i = self.collect_indented_body(&mut note, i, state);
        } else {
            let structural_len = if def.authored_quote_def {
                raw_def_line.len() - def.def_line.len()
            } else {
                stripped.structural.len()
            };
            i = self.collect_hosted_body(
                &mut note,
                i,
                raw_def_line,
                &def,
                structural_len,
                stripped.description,
            );
        }
        // ONLY FOR THE DEFINITION THIS POSITION DESCRIBES. The first
        // definition for a label wins, both here and in `defs` below, so a
        // later duplicate must not move the accepted one's end: this wrote
        // the DUPLICATE's following line onto the FIRST definition's span,
        // and the container that hosted the first one was then reported as
        // running to it (carve-rs#1106). `defs` holds the label exactly
        // when an earlier definition was accepted, which is the test.
        if self.positions
            && !in_container
            && first_for_label
            && i < self.lines.len()
            && is_blank_line(self.lines[i])
        {
            if let Some(pos) = self.def_positions.get_mut(def.label) {
                pos.end_line = self.first_source_line + i;
                pos.end_column = 1;
            }
        }
        // First definition for a label wins (later duplicates are ignored).
        let note_lines: Vec<&str> = note.lines.iter().map(String::as_str).collect();
        let attached_nonparagraph = trim_ascii(def.first) == "+"
            && note.line_map.first() == Some(&None)
            && first_visible_note_line(&note_lines)
                .is_some_and(|at| !line_starts_paragraph(note_lines[at]));
        // Keep the marker-line child's authored column relative to the note
        // floor when a folded marker precedes a below-column code fence.
        if !in_container
            && !def.authored_quote_def
            && unfenced_marker_content_col(def.first).is_some_and(|width| {
                let first_col = stripped_col(
                    Some(stripped.structural.chars().count() as isize),
                    stripped.bare,
                    def.first,
                )
                .unwrap_or(0)
                .max(0) as usize;
                note.line_map.windows(2).skip(1).any(|pair| {
                    let raw = |source_line: Option<usize>| {
                        source_line
                            .and_then(|line| line.checked_sub(self.first_source_line))
                            .and_then(|at| self.lines.get(at))
                    };
                    raw(pair[0])
                        .zip(raw(pair[1]))
                        .is_some_and(|(folded, fence)| {
                            indent_columns(folded) < first_col + width
                                && is_list_marker(trim_ascii_start(folded))
                                && indent_columns(fence) < first_col + width
                                && detect_fence_open(trim_ascii_start(fence)).is_some()
                        })
                })
            })
        {
            let first_col = stripped_col(
                Some(stripped.structural.chars().count() as isize),
                stripped.bare,
                def.first,
            )
            .unwrap_or(0)
            .max(0) as usize;
            let shift = first_col.saturating_sub(body_indent);
            if shift > 0 {
                note.lines[0] = format!("{}{}", " ".repeat(shift), note.lines[0]);
                if let Some(Some(col)) = note.col_map.get_mut(0) {
                    *col -= shift as isize;
                }
            }
        }
        if first_for_label {
            self.register_definition(normalized_label, &def, note);
        } else if note.lines.iter().any(|line| line.contains("[^")) {
            // The label loses, not the definitions nested in its body.
            let key = format!("{LOSING_NOTE_PREFIX}{def_start_line}");
            self.defs.insert(key, definition_source(note));
        }
        self.push_definition_replacement(
            raw_def_line,
            &def,
            stripped,
            def_start_line,
            attached_nonparagraph,
        );
        i
    }

    // The definition's OWN extent, before the body is collected.
    //
    // PART 12 §4: a span begins at the markup that opens the construct,
    // which for a definition is the `[` of `[^label]:` and not the
    // container prefix that carried the line. `def_line` is that text,
    // so what precedes it on the raw line is exactly the prefix to skip -
    // measured by taking `def_line` off the end rather than by re-deriving
    // a column, so a tab or a quote marker cannot be counted differently
    // here than it was when the line was stripped.
    fn record_definition_start(
        &mut self,
        label: &str,
        raw_def_line: &str,
        def_line: &str,
        def_start_line: usize,
    ) {
        let Some(prefix) = raw_def_line.strip_suffix(def_line) else {
            return;
        };
        // First definition for a label wins, matching `defs` below.
        self.def_positions
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

    fn plus_definition_key(line: &str) -> Option<String> {
        parse_footnote_def_line(trim_ascii_start(line))
            .filter(|(_, body)| {
                trim_ascii(body) == "+" || continuation_marker::is_consumed_marker(body)
            })
            .map(|(label, _)| label_key(label))
    }

    fn pass_winning_note(
        &mut self,
        note: &mut NoteBody,
        i: usize,
        label: &str,
        first: &str,
    ) -> bool {
        let lines = self.lines;
        let Some(key) = lines
            .get(i)
            .and_then(|line| Self::plus_definition_key(line))
        else {
            return false;
        };
        if key == label_key(label) || self.is_defined(&key) || trim_ascii(first) != "+" {
            return false;
        }
        // A later winning note takes its own indented body after a blank.
        // Losing duplicate `+` lines, of any label, do not change that owner.
        let own = label_key(label);
        let mut recipient = key;
        let mut recipient_at = i;
        let mut after = i + 1;
        while let Some(other) = lines
            .get(after)
            .and_then(|line| Self::plus_definition_key(line))
        {
            if other != own && other != recipient && !self.is_defined(&other) {
                recipient = other;
                recipient_at = after;
            }
            after += 1;
        }
        let blank = after;
        while lines.get(after).is_some_and(|line| is_blank_line(line)) {
            after += 1;
        }
        if !lines.get(after).is_some_and(|line| {
            let structural =
                item_block_opener(trim_ascii_start(line)) || is_list_marker(trim_ascii_start(line));
            indent_columns(line) >= footnote_body_floor(lines[recipient_at])
                || after > blank && structural
        }) {
            return false;
        }
        note.lines[0].clear();
        note.line_map[0] = None;
        if self.positions {
            note.col_map[0] = None;
        }
        self.passed_note = Some(recipient);
        true
    }

    /// Losing duplicate `+` lines right below a `+` note are read as if they
    /// were not there, so the note's `+` still takes the block below them
    /// (PART 9R R2, carve#5538).
    fn skip_losing_plus_lines(
        &mut self,
        mut i: usize,
        label: &str,
        first: &str,
        raw_def_line: &str,
    ) -> usize {
        if !is_cont_marker_line(first) && !continuation_marker::is_consumed_marker(first) {
            return i;
        }
        let column = indent_columns(raw_def_line);
        let losing = |raw: &str| {
            indent_columns(raw) == column
                && parse_footnote_def_line(trim_ascii_start(raw)).is_some_and(|(other, body)| {
                    (trim_ascii(body) == "+" || continuation_marker::is_consumed_marker(body))
                        && (label_key(other) == label_key(label)
                            || self.is_defined(&label_key(other)))
                })
        };
        let end = i + self.lines[i..].iter().take_while(|raw| losing(raw)).count();
        // After a blank the passed-note walk already reads the block below.
        if end == i || !self.lines.get(end).is_some_and(|raw| !is_blank_line(raw)) {
            return i;
        }
        while i < end {
            let raw = self.lines[i];
            let scan = self.observe_line(i);
            let Some(duplicate) = self.definition_at(i, &scan) else {
                break;
            };
            self.push_definition_replacement(
                raw,
                &duplicate,
                &scan.stripped,
                self.first_source_line + i,
                false,
            );
            i += 1;
        }
        i
    }

    fn resume_passed_note(
        &mut self,
        note: &mut NoteBody,
        mut i: usize,
        label: &str,
        container: IndentContainer,
    ) -> usize {
        self.passed_note = None;
        let lines = self.lines;
        let key = label_key(label);
        while lines
            .get(i)
            .and_then(|line| Self::plus_definition_key(line))
            .is_some_and(|other| other == key || self.is_defined(&other))
        {
            let scan = self.observe_line(i);
            let Some(duplicate) = self.definition_at(i, &scan) else {
                break;
            };
            self.push_definition_replacement(
                lines[i],
                &duplicate,
                &scan.stripped,
                self.first_source_line + i,
                false,
            );
            i += 1;
        }
        let blank = i;
        while lines.get(i).is_some_and(|line| is_blank_line(line)) {
            i += 1;
        }
        if i > blank
            && lines.get(i).is_some_and(|line| {
                indent_columns(line) < container.content_column
                    && (is_list_marker(trim_ascii_start(line))
                        || item_block_opener(trim_ascii_start(line)))
            })
        {
            for at in blank..i {
                note.push_with(String::new(), Some(self.first_source_line + at), |_| {
                    Some(0)
                });
            }
            let blank_end = (i..lines.len())
                .find(|&at| is_blank_line(lines[at]))
                .unwrap_or(lines.len());
            let end = detect_fence_open(trim_ascii_start(lines[i]))
                .and_then(|open| {
                    (i + 1..lines.len())
                        .find(|&at| is_fence_close(trim_ascii_start(lines[at]), open))
                })
                .map_or(blank_end, |at| at + 1);
            let first = trim_ascii_start(lines[i]);
            let count =
                if strip_blockquote_prefix(first).is_some() || detect_fence_open(first).is_some() {
                    end - i
                } else {
                    attached_one_block_lines(CursorLines::Slice(&lines[i..end]), self.options)
                };
            let opener_column = {
                let raw = lines[i];
                let walked = raw.chars().count() - trim_ascii_start(raw).chars().count() + 1;
                own_lines::authored_visual_column(self.first_source_line + i, walked)
            };
            for offset in 0..count {
                let raw = lines[i + offset];
                let kept = if is_blank_line(raw) {
                    String::new()
                } else {
                    let text = trim_ascii_start(raw);
                    let walked = raw.chars().count() - text.chars().count() + 1;
                    let authored = own_lines::authored_visual_column(
                        self.first_source_line + i + offset,
                        walked,
                    );
                    format!(
                        "{}{}",
                        " ".repeat(authored.saturating_sub(opener_column)),
                        text
                    )
                };
                note.push_with(kept, Some(self.first_source_line + i + offset), |_| {
                    Some(if offset == 0 {
                        indent_columns(raw) as isize
                    } else {
                        0
                    })
                });
            }
            return i + count;
        }
        blank
    }

    /// A later winning definition takes the pending first-block grant.
    fn yields_first_block(
        &mut self,
        note: &mut NoteBody,
        i: usize,
        label: &str,
        first: &str,
    ) -> bool {
        self.passed_note
            .as_deref()
            .is_some_and(|key| key != label_key(label))
            || self.pass_winning_note(note, i, label, first)
    }

    fn resume_own_passed_note(
        &mut self,
        note: &mut NoteBody,
        i: &mut usize,
        label: &str,
        container: IndentContainer,
    ) -> bool {
        if self.passed_note.as_deref() == Some(&label_key(label)) {
            *i = self.resume_passed_note(note, *i, label, container);
            true
        } else {
            false
        }
    }

    // A lone marker-line `+` gives the note its first flush-left block.
    // The same block bound is used by a later standalone `+` in a note.
    #[allow(clippy::too_many_arguments)]
    fn lone_plus_first_block(
        &mut self,
        note: &mut NoteBody,
        mut i: usize,
        label: &str,
        first: &str,
        container: IndentContainer,
        def_start_line: usize,
    ) -> usize {
        let lines = self.lines;
        if self.yields_first_block(note, i, label, first) {
            return i;
        }
        let received_grant = self.resume_own_passed_note(note, &mut i, label, container);
        if !received_grant
            && (trim_ascii(first) != "+"
                || !container.owns_continuation(first, self.decision_at(def_start_line)))
            || !lines.get(i).is_some_and(|line| !is_blank_line(line))
        {
            return i;
        }
        let start = i;
        let marker_col = container.content_column.saturating_sub(2);
        let first_text = trim_ascii_start(lines[i]);
        let needs_raw_probe = parse_link_def_line(first_text).is_some()
            || parse_footnote_def_line(first_text).is_some()
            || detect_list_marker_full(first_text).is_some()
            || parse_standalone_attrs(first_text).is_some()
            || is_plus_marker(first_text);
        let mut list_fence_sibling = attached_list_fence_sibling(CursorLines::Slice(lines), i);
        let first_boundary_after = |from: usize| {
            let mut nested_fence = NestedAttachedFence::default();
            from + lines[from..]
                .iter()
                .enumerate()
                .position(|(offset, line)| {
                    nested_fence.ends_at_blank(CursorLines::Slice(lines), from, from + offset, line)
                        || is_plus_marker(line)
                })
                .unwrap_or(lines.len() - from)
        };
        let mut fenced_block_end = |from: usize| {
            let text = trim_ascii_start(lines.get(from)?);
            if let Some(open) = detect_fence_open(text) {
                return Some(
                    (from + 1..lines.len())
                        .find(|&at| {
                            indent_columns(lines[at]) == marker_col
                                && is_fence_close(
                                    strip_leading_columns(lines[at], marker_col).as_str(),
                                    open,
                                )
                        })
                        // An unclosed fence's payload keeps a lone `+` (PART 9 §17 L3,
                        // carve#5494): only its blank ends it.
                        .map_or_else(
                            || {
                                (from + 1..lines.len())
                                    .find(|&at| is_blank_line(lines[at]))
                                    .unwrap_or(lines.len())
                            },
                            |at| at + 1,
                        ),
                );
            }
            detect_line_block_open(text)
                .or_else(|| detect_hardbreaks_block_open(text))
                .or_else(|| detect_quote_block_open(text))
                .or_else(|| detect_container_open(text).map(|open| open.fence_len))?;
            let first_blank = (from + 1..lines.len())
                .find(|&at| is_blank_line(lines[at]))
                .unwrap_or(lines.len());
            Some(attached_block_past_invisible(
                CursorLines::Slice(lines),
                from,
                &mut self.comment_closers,
                &mut self.closer_index,
                self.options,
                &mut |line, at| {
                    is_blank_line(line)
                        || is_plus_marker(line)
                        || (at > first_blank
                            && indent_columns(line) < container.content_column
                            && exact_colon_fence_len(trim_ascii_start(line)).is_none()
                            && item_block_opener(trim_ascii_start(line)))
                },
            ))
        };
        // Erased comment spans are invisible: the block they lead is the attached one (CARVE-P9-067).
        let lead = i + lines[i..]
            .iter()
            .take_while(|line| is_line_comment_any_column(line))
            .count();
        let pulled_closer = pulled_fence_closer(lines, lead, container.content_column, marker_col);
        let window_end = if let Some(closer) = pulled_closer {
            closer + 1
        } else if let Some(end) = fenced_block_end(lead) {
            end
        } else if let Some(open) = detect_comment_fence_line(first_text) {
            let after_comment = (i + 1..lines.len())
                .find(|&at| is_comment_fence_close_any_column(lines[at], open.fence_len))
                .map_or(i + 1, |at| at + 1);
            first_boundary_after(after_comment)
        } else if list_fence_sibling {
            i + attached_one_block_lines(CursorLines::Slice(&lines[i..]), self.options)
        } else if needs_raw_probe {
            let mut nested_fence = NestedAttachedFence::default();
            attached_block_past_invisible(
                CursorLines::Slice(lines),
                i,
                &mut self.comment_closers,
                &mut self.closer_index,
                self.options,
                &mut |line, at| {
                    nested_fence.ends_at_blank(CursorLines::Slice(lines), i, at, line)
                        || is_plus_marker(line)
                },
            )
        } else {
            first_boundary_after(i)
        };
        // The list probe consumes a trailing separator as part of the list
        // cursor. That separator belongs to the host after the one block.
        let mut window_end = window_end;
        // A refused marker between compatible list items renders nothing.
        // The first-block grant already owns the list, including its next item.
        let continued = continued_list_window(lines, i, window_end, self.options);
        list_fence_sibling |= continued.is_some();
        window_end = continued.unwrap_or(window_end);
        if list_fence_sibling {
            window_end = trim_attached_list_window(lines, i, window_end);
        }
        // A later marker at the attached block's column belongs to its host,
        // even when a list probe could interpret it as an item continuation.
        let window_end = later_marker_end(lines, i, window_end, marker_col);
        let window_end = attached_quote_window_end(
            lines,
            lead,
            window_end,
            self.first_source_line,
            own_lines::authored_visual_column(def_start_line, 1),
        );
        let lead_col = indent_columns(lines[lead]);
        let attached: Vec<String> = lines[i..window_end]
            .iter()
            .enumerate()
            .map(|(offset, line)| {
                if pulled_closer == Some(i + offset) {
                    trim_ascii_start(line).to_string()
                } else if let Some(own) = at_own_column(line, lead_col, marker_col) {
                    own
                } else if i + offset > lead
                    && indent_columns(line) == container.content_column
                    && strip_blockquote_prefix(trim_ascii_start(line)).is_some()
                    && strip_blockquote_prefix(trim_ascii_start(lines[lead])).is_some()
                {
                    strip_leading_columns(line, container.content_column)
                } else {
                    strip_leading_columns(line, marker_col)
                }
            })
            .collect();
        let attached_refs: Vec<&str> = attached.iter().map(String::as_str).collect();
        let first_visible_at = first_visible_note_line(&attached_refs).unwrap_or(0);
        // A list marker ends an attached paragraph; the body then opens its
        // list there (PART 0 S3, carve#4860).
        let lead_is_paragraph = attached_refs
            .get(first_visible_at)
            .is_some_and(|line| line_starts_paragraph(trim_ascii_start(line)));
        let lead_is_quote = attached_refs
            .get(first_visible_at)
            .is_some_and(|line| strip_blockquote_prefix(trim_ascii_start(line)).is_some());
        let lead_is_list = attached_refs
            .get(first_visible_at)
            .is_some_and(|line| is_list_marker(trim_ascii_start(line)))
            && lines
                .get(start + first_visible_at)
                .is_some_and(|line| indent_columns(line) > marker_col);
        let after_refused_run = |at: usize| follows_refused_run(&attached, first_visible_at, at);
        let ends_at_marker = |at: usize, raw: &str| {
            lead_is_paragraph
                && marker_ends_attached_paragraph(&attached, first_visible_at, at, raw, marker_col)
        };
        let losing = |other: &str, first: &str| self.losing_plus_line(label, other, first);
        let mut comment_closers = None;
        let mut closer_index = None;
        let mut nested_fence = NestedAttachedFence::default();
        let mut end = if list_fence_sibling {
            attached_one_block_lines(CursorLines::Slice(&attached_refs), self.options)
        } else {
            attached_block_past_invisible(
                CursorLines::Slice(&attached_refs),
                0,
                &mut comment_closers,
                &mut closer_index,
                self.options,
                &mut |line, at| {
                    let raw = lines[start + at];
                    nested_fence.ends_at_blank(
                        CursorLines::Slice(&attached_refs),
                        first_visible_at,
                        at,
                        line,
                    )
                    || is_plus_marker(line)
                    || ends_at_marker(at, raw)
                    || (at > first_visible_at
                        && lead_is_quote
                        && indent_columns(raw) >= container.content_column
                        && (if caption_content(trim_ascii_start(raw)).is_some() {
                            indent_columns(raw) == container.content_column
                        } else {
                            item_block_opener(trim_ascii_start(raw))
                                && !(lead_is_quote
                                    && strip_blockquote_prefix(trim_ascii_start(raw)).is_some())
                        }))
                    || marker_column_ends_attached_block(
                        &attached,
                        raw,
                        at,
                        first_visible_at,
                        marker_col,
                        lead_is_quote,
                        lead_is_list,
                    )
                    // A definition at the marker column opens a sibling note (carve-rs#4090).
                    // A losing duplicate's consumed `+` opens nothing (carve#5502).
                    || (at > first_visible_at
                        && indent_columns(raw) <= marker_col
                        && parse_footnote_def_line(trim_ascii_start(raw))
                            .is_some_and(|(other, first)| !losing(other, first))
                        && !after_refused_run(at))
                },
            )
        };
        while end <= first_visible_at && end < attached_refs.len() {
            let next = attached_block_past_invisible(
                CursorLines::Slice(&attached_refs),
                end,
                &mut comment_closers,
                &mut closer_index,
                self.options,
                &mut |line, at| {
                    is_blank_line(line)
                        || is_plus_marker(line)
                        || ends_at_marker(at, lines[start + at])
                        || marker_column_ends_attached_block(
                            &attached,
                            lines[start + at],
                            at,
                            first_visible_at,
                            marker_col,
                            false,
                            lead_is_list,
                        )
                },
            );
            if next <= end {
                break;
            }
            end = next;
        }
        let end = lead_block_end(&attached_refs, first_visible_at, end, self.options);
        let mut end = i + end;
        if attached
            .first()
            .is_some_and(|line| line_starts_paragraph(line))
        {
            for (at, &raw) in lines.iter().enumerate().take(end).skip(i + 1) {
                let text = trim_ascii_start(raw);
                if indent_columns(raw) >= container.content_column
                    && item_block_opener(text)
                    && !follows_refused_run(&attached, first_visible_at, at - start)
                {
                    end = at;
                    break;
                }
            }
        }
        if end > i {
            note.lines[0].clear();
            note.comment_state = BodyCommentState::new();
            note.open_colons = OpenBodyColons::default();
            note.line_map[0] = None;
            if self.positions {
                note.col_map[0] = None;
            }
            let mut visible = false;
            // A skipped note definition keeps its folded body (CARVE-P9-067, carve#5034).
            let mut in_skipped_note = false;
            while i < end {
                let at = i - start;
                let line = attached[at].clone();
                // Invisible definitions ahead of the block are the document's (carve#4449).
                if !visible && self.hoist_note_link_def(&line, i) {
                    note.push_with(
                        definition_placeholder(),
                        Some(self.first_source_line + i),
                        |_| Some(marker_col as isize),
                    );
                    in_skipped_note = false;
                    i += 1;
                    continue;
                }
                in_skipped_note = if at <= first_visible_at
                    && !line.starts_with([' ', '\t'])
                    && parse_footnote_def_line(&line).is_some()
                {
                    false
                } else {
                    at < first_visible_at
                        && (in_skipped_note
                            || at > 0 && parse_footnote_def_line(&attached[at - 1]).is_some())
                        && own_lines::foldable_plain_line(trim_ascii_start(&line))
                        && attr_run_at(&attached_refs, at).is_none()
                };
                visible |= !is_line_comment_any_column(&line);
                let kept =
                    continued_list_window(&attached_refs, first_visible_at, at, self.options)
                        .filter(|_| is_plus_marker(&line))
                        .map(|_| String::new())
                        .unwrap_or_else(|| {
                            if in_skipped_note {
                                note.frame_lazy(indent_columns(&line), trim_ascii_start(&line))
                            } else {
                                line
                            }
                        });
                note.push_with(kept, Some(self.first_source_line + i), |_| {
                    Some(marker_col as isize)
                });
                i += 1;
            }
            let later_line_block_closer = attached_refs
                .get(first_visible_at)
                .and_then(|line| detect_line_block_open(trim_ascii_start(line)))
                .filter(|run| {
                    lines.get(i).is_some_and(|line| is_blank_line(line))
                        && !attached_refs
                            .get(first_visible_at + 1..end - start)
                            .unwrap_or(&[])
                            .iter()
                            .any(|line| exact_colon_fence_len(trim_ascii_start(line)) == Some(*run))
                })
                .and_then(|run| {
                    let closers = self.line_block_closers.get_or_insert_with(|| {
                        let mut index: HashMap<usize, Vec<usize>> = HashMap::new();
                        for (at, line) in lines.iter().enumerate() {
                            if let Some(width) = exact_colon_fence_len(trim_ascii_start(line)) {
                                index.entry(width).or_default().push(at);
                            }
                        }
                        index
                    });
                    let positions = closers.get(&run)?;
                    positions
                        .get(positions.partition_point(|&at| at < i))
                        .copied()
                });
            if later_line_block_closer.is_none()
                && lines.get(i).is_some_and(|line| is_blank_line(line))
            {
                let remaining = &lines[i + 1..];
                for closer in attached_first_blank_closers(&attached_refs[..end - start], remaining)
                {
                    note.push_with(closer, None, |_| None);
                }
            }
            // A closed attached line block owns the blank and its exact
            // closer, even when that closer misses the note's content column.
            if let Some(close) = later_line_block_closer {
                let closer_reaches_note = indent_columns(lines[close]) >= container.content_column;
                while i <= close {
                    let kept = if i == close {
                        trim_ascii_start(lines[i]).to_owned()
                    } else if is_line_comment_any_column(lines[i]) {
                        String::new()
                    } else if closer_reaches_note {
                        strip_leading_columns(lines[i], container.content_column)
                    } else {
                        strip_leading_columns(lines[i], marker_col)
                    };
                    let column = stripped_col(Some(0), lines[i], &kept);
                    note.push_with(kept, Some(self.first_source_line + i), |_| column);
                    i += 1;
                }
            }
            let caption_after_one_blank = lead_is_quote
                && lines.get(i).is_some_and(|line| is_blank_line(line))
                && lines.get(i + 1).is_some_and(|line| {
                    indent_columns(line) >= container.content_column
                        && caption_content(trim_ascii_start(line)).is_some()
                });
            // A fence nested in the attached block keeps the authored blanks
            // as payload; a synthetic separator would add one (carve#5539).
            if !caption_after_one_blank && !note.holds_open_fence() {
                note.push_blank();
            }
        }
        i
    }

    fn collect_indented_body(
        &mut self,
        note: &mut NoteBody,
        mut i: usize,
        mut state: IndentedNote<'a>,
    ) -> usize {
        let _container = state.container;
        run_indented_body!(
            CursorLines::Slice(self.lines),
            i,
            _container,
            |line, admission| {
                if self.indented_line(&mut state, note, line, admission.blank, &mut i) == Flow::Stop
                {
                    break;
                }
            }
        );
        i
    }

    fn indented_line(
        &mut self,
        state: &mut IndentedNote<'a>,
        note: &mut NoteBody,
        line: &str,
        blank: bool,
        i: &mut usize,
    ) -> Flow {
        let column = indent_columns(line);
        if let Some(marker) = state.attached_quote_marker {
            let text = trim_ascii_start(line);
            if !blank
                && strip_blockquote_prefix(text).is_none()
                && (is_list_marker(text) || item_block_opener(text))
                && authored_indent_column(self.first_source_line + *i, line) <= marker
            {
                return Flow::Stop;
            }
        }
        if state.quote_granted
            && (blank
                || item_block_opener(trim_ascii_start(line))
                || is_list_marker(trim_ascii_start(line)))
        {
            return Flow::Stop;
        }
        let lazy_open = std::mem::take(&mut state.lazy_open);
        if let Some(flow) = self.retained_fence_line(state, note, line, column, i) {
            return flow;
        }
        if let Some(flow) = self.below_comment_line(state, note, line, column, i) {
            return flow;
        }
        if let Some(flow) = self.retained_flush_line(state, note, line, column, i) {
            return flow;
        }
        if is_blank_line(line) || column >= state.body_indent {
            state.retained_below_comment = false;
            if column >= state.body_indent && !is_blank_line(line) {
                state.closed_comment_paragraph = false;
            }
        }
        if parse_footnote_def_line(line).is_some() {
            return Flow::Stop;
        }
        if blank {
            return self.blank_note_line(state, note, line, i);
        }
        if let Some(flow) = self.attached_plus_block(state, note, line, column, i) {
            return flow;
        }
        if trim_ascii(state.first) == "+"
            && column >= state.def_column
            && column < state.body_indent
            && strip_blockquote_prefix(trim_ascii_start(line)).is_some()
            && *i > 0
            && !is_blank_line(self.lines[*i - 1])
            && note
                .lines
                .iter()
                .rev()
                .find(|previous| !is_blank_line(previous))
                .is_some_and(|previous| {
                    strip_blockquote_prefix(trim_ascii_start(previous)).is_some()
                })
        {
            if note.lines.last().is_some_and(|last| is_blank_line(last))
                && note.line_map.last() == Some(&None)
            {
                note.lines.pop();
                note.line_map.pop();
                if note.positions {
                    note.col_map.pop();
                }
            }
            return self.indented_content_line(state, note, line, i);
        }
        // COLUMNS, not a byte count. §16 asks for >= 2 columns and
        // §24 C1 gives a tab a column value, so a bare tab reaches
        // the body's column exactly as two spaces do. Counting
        // bytes made this half disagree with the dedent below, which
        // has always used `strip_leading_columns`: a bare tab
        // counted as one and was refused (carve#796).
        if column >= state.body_indent {
            return self.indented_content_line(state, note, line, i);
        }
        // A COMMENT DELIMITER CLOSING A SPAN THIS BODY ALREADY HOLDS
        // IS NOT A COMMENT THAT ENDS THE BODY - see
        // `body_open_comment_run`. This host hides the payload
        // already at a delimiter REACHING the body's floor; the band
        // below it is what this covers.
        if detect_comment_fence_line_any_column(line).is_some()
            && closes_a_held_span(&note.lines, line)
        {
            note.push_with(
                kept_comment_delimiter(line),
                Some(self.first_source_line + *i),
                |kept| stripped_col(Some(0), line, kept),
            );
            *i += 1;
            state.closed_comment_paragraph = true;
            return Flow::Next;
        }
        if state.closed_comment_paragraph {
            return Flow::Stop;
        }
        // Lazy continuation, the same rule as every container (ruling on
        // carve#3885): a line that interrupts no paragraph joins an open one.
        if note.comment_state.comment.is_some() {
            return Flow::Stop;
        }
        let rest = trim_ascii_start(line);
        // A nested note can also take below-column prose that the outer note
        // owned at its content column. A refused `+` is an internal letter,
        // not prose, and closes this claim before its following paragraph.
        if (!self.nested
            || state.marker_line_note
            || line.starts_with(LAZY)
            || (!continuation_marker::is_refused_letter(rest) && column > 0))
            && state.note_fence.is_none()
            && !interrupts_paragraph_with_rest(line, &self.lines[*i + 1..])
            && !interrupts_paragraph_with_rest(rest, &self.lines[*i + 1..])
            && !is_colon_fence_opener_shape(line)
            && !exact_colon_fence_len(rest).is_some_and(|run| run >= 3)
            && parse_link_def_line(rest).is_none()
            && parse_footnote_def_line(rest).is_none()
            && !document_abbreviation_below_note(
                line,
                &self.columns,
                self.document_colon_open.get(*i).copied().unwrap_or(false),
            )
            // A list marker that misses a marker body closes it (PART 0 S3, carve#4860).
            && !is_list_marker(rest)
            && !opens_code_fence(rest)
            // A descendant's closing fence ends its paragraph before a line
            // below this note's floor can ask for lazy continuation.
            && (note.stanza.in_block() == Some(true)
                || !note.lines.last().is_some_and(|last| {
                    let text = trim_ascii_start(last);
                    detect_fence_open(text).is_some() || detect_comment_fence_line(text).is_some()
                })
                || ends_in_refused_follower(&note.lines))
            && (lazy_open || note_paragraph_open(note, rest, self.options))
        {
            let text = strip_lazy(rest);
            let kept = note.frame_lazy(
                lazy_source_column(line).unwrap_or_else(|| indent_columns(line)),
                text,
            );
            note.push_with(kept, Some(self.first_source_line + *i), |_| {
                stripped_col(Some(0), line, text)
            });
            state.lazy_open = true;
            *i += 1;
            return Flow::Next;
        }
        Flow::Stop
    }

    fn retained_fence_line(
        &mut self,
        state: &mut IndentedNote<'a>,
        note: &mut NoteBody,
        line: &str,
        column: usize,
        i: &mut usize,
    ) -> Option<Flow> {
        if !state.retained_below_comment {
            return None;
        }
        let open = state.note_fence?;
        if column < state.body_indent
            && !is_blank_line(line)
            && !is_fence_close(trim_ascii_start(line), open)
            && !self.lines[*i..]
                .iter()
                .any(|next| is_fence_close(trim_ascii_start(next), open))
        {
            return Some(Flow::Stop);
        }
        note.push_with(line.to_owned(), Some(self.first_source_line + *i), |_| {
            Some(0)
        });
        if is_fence_close(trim_ascii_start(line), open) {
            state.note_fence = None;
            state.retained_below_comment = false;
        }
        *i += 1;
        Some(Flow::Next)
    }

    fn below_comment_line(
        &mut self,
        state: &mut IndentedNote<'a>,
        note: &mut NoteBody,
        line: &str,
        column: usize,
        i: &mut usize,
    ) -> Option<Flow> {
        let lines = self.lines;
        let below_comment = column < state.body_indent
            && trim_ascii_start(line).starts_with("%%")
            && !is_definition_placeholder(line);
        // Rule A (carve#3885): a comment below the body's column closes the note.
        let held_run = below_comment
            .then(|| body_held_span_run(&note.lines))
            .flatten();
        let held_payload =
            held_run.is_some_and(|run| !is_comment_fence_close_any_column(line, run));
        if held_payload {
            note.push_with(line.to_owned(), Some(self.first_source_line + *i), |kept| {
                stripped_col(Some(0), line, kept)
            });
            *i += 1;
            return Some(Flow::Next);
        }
        if below_comment
            && state.below_comment_span.is_none()
            && state.note_fence.is_none()
            // §28: an opener with no closer ahead is a `%%` line for this question too.
            && detect_comment_fence_line_any_column(line).is_none()
        {
            return Some(Flow::Stop);
        }
        // S1 tests an open span's closer before ownership (carve#4721).
        if below_comment
            && column == 0
            && state.below_comment_span.is_none()
            && detect_comment_fence_line_any_column(line).is_some()
            && !held_run.is_some_and(|run| is_comment_fence_close_any_column(line, run))
        {
            return Some(Flow::Stop);
        }
        let empty_fence = detect_comment_fence_line_any_column(line).is_some_and(|open| {
            lines
                .get(*i + 1)
                .is_some_and(|next| is_comment_fence_close_any_column(next, open.fence_len))
        });
        let after_comment = *i + 1 + usize::from(empty_fence);
        if below_comment
            && column == 0
            && !state.open_term
            && (!self.nested || note.lines.len() == 1)
            && (is_line_comment_any_column(line) || empty_fence)
            && self.body.iter().all(|line| is_blank_line(line))
            && lines.get(after_comment).is_some_and(|next| {
                let follower_column = indent_columns(next);
                (follower_column == 0 || follower_column >= state.body_indent)
                    && detect_fence_open(trim_ascii_start(next)).is_none()
            })
            && self
                .source
                .match_indices(&format!("[^{}]", state.label))
                .count()
                <= 1
        {
            return Some(Flow::Stop);
        }
        if state.below_comment_span.is_some() || (below_comment && state.note_fence.is_none()) {
            return Some(self.retain_below_comment(state, note, line, column, i));
        }
        None
    }

    fn retain_below_comment(
        &mut self,
        state: &mut IndentedNote<'a>,
        note: &mut NoteBody,
        line: &str,
        column: usize,
        i: &mut usize,
    ) -> Flow {
        let closes_held_span = closes_a_held_span(&note.lines, line);
        if let Some(width) = state.below_comment_span {
            if !is_blank_line(line)
                && state.span_floor.is_some_and(|floor| column < floor)
                && !is_comment_fence_close_any_column(line, width)
            {
                return Flow::Stop;
            }
        }
        if state.below_comment_span.is_none()
            && !closes_held_span
            && !state.retained_below_comment
            && ((parse_footnote_def_line(state.first).is_some()
                && column == 0
                && note.lines.len() == 1)
                || (!state.open_term
                    && !body_ends_with_open_paragraph(&note.lines.join("\n"), self.options)))
        {
            return Flow::Stop;
        }
        let matched_span_line = state.below_comment_span.is_some();
        if let Some(width) = state.below_comment_span {
            if is_comment_fence_close_any_column(line, width) {
                state.below_comment_span = None;
                state.closed_comment_paragraph = true;
            }
        } else if let Some(open) =
            detect_comment_fence_line_any_column(line).filter(|_| !closes_held_span)
        {
            state.below_comment_span = Some(open.fence_len);
            state.span_floor = state
                .host_items
                .iter()
                .copied()
                .filter(|&item| item <= column)
                .max();
        }
        let matched_span_line = matched_span_line || state.below_comment_span.is_some();
        let kept =
            if trim_ascii_start(line).starts_with("%%") && !state.open_term && !matched_span_line {
                trim_ascii_start(line).to_owned()
            } else {
                line.to_owned()
            };
        note.push_with(kept, Some(self.first_source_line + *i), |kept| {
            stripped_col(Some(0), line, kept)
        });
        state.closed_comment_paragraph |= closes_held_span;
        state.retained_below_comment = !closes_held_span;
        *i += 1;
        Flow::Next
    }

    fn retained_flush_line(
        &mut self,
        state: &mut IndentedNote<'a>,
        note: &mut NoteBody,
        line: &str,
        column: usize,
        i: &mut usize,
    ) -> Option<Flow> {
        let lines = self.lines;
        let retained = state.retained_below_comment
            && column < state.body_indent
            && !is_blank_line(line)
            // A matched fence interrupts at this column. Keeping it
            // with an unreferenced note removes its visible block.
            && !detect_fence_open(trim_ascii_start(line)).is_some_and(|open| {
                #[cfg(test)]
                FOOTNOTE_FENCE_CLOSER_LOOKUPS.with(|count| count.set(count.get() + 1));
                self.closer_maxima.closes_after(*i, open)
            })
            && (column > 0
                || !item_block_opener(trim_ascii_start(line))
                || (detect_fence_open(trim_ascii_start(line)).is_some()
                    && held_comment_left_open(&note.lines, lines, *i)));
        if !retained {
            return None;
        }
        let kept = trim_ascii_start(line).to_owned();
        if held_comment_left_open(&note.lines, lines, *i) {
            if let Some(mut open) = detect_fence_open(&kept) {
                open.content_col = 0;
                state.note_fence = Some(open);
            }
        }
        note.push_with(kept, Some(self.first_source_line + *i), |kept| {
            stripped_col(Some(0), line, kept)
        });
        if state.note_fence.is_none() {
            state.retained_below_comment = false;
        }
        *i += 1;
        Some(Flow::Next)
    }

    fn blank_note_line(
        &mut self,
        state: &mut IndentedNote<'a>,
        note: &mut NoteBody,
        line: &str,
        i: &mut usize,
    ) -> Flow {
        let lines = self.lines;
        if let Some(open) = state.note_fence {
            let (residue, consumed, synthetic) = slice_columns_mapped(line, open.content_col, true);
            note.push_with(residue, Some(self.first_source_line + *i), |_| {
                Some(consumed as isize - synthetic as isize)
            });
            *i += 1;
            return Flow::Next;
        }
        let container = state.container;
        let after_run = container.blank_run_end(CursorLines::Slice(lines), *i + 1);
        let resumes = after_run < lines.len()
            && match container.classify(lines[after_run], || {
                self.decision_at(self.first_source_line + after_run)
            }) {
                ContainerLine::Content => true,
                ContainerLine::OwnedContinuation => is_plus_marker(lines[after_run]),
                _ => false,
            };
        // A blank inside a live span is payload; a comment line below the
        // column still closes it (PART 9 §28, carve#4996).
        let span_resumes = !resumes
            && after_run < lines.len()
            && note.comment_state.comment.is_some()
            && trim_ascii_start(strip_lazy(lines[after_run])).starts_with("%%");
        // The run still separates the host's blocks when nothing visible of
        // the note follows it (§17 L1c, carve-rs#3798).
        if span_resumes && {
            // Every blank run up to the same end shares the answer, so the scan stays linear.
            let (end, separates) = match state.span_note_end {
                Some((end, separates)) if end >= after_run => (end, separates),
                _ => span_note_end(lines, after_run, state.def_column, state.body_indent),
            };
            state.span_note_end = Some((end, separates));
            separates
        } {
            for at in *i..after_run {
                self.keep_line(at);
            }
        }
        let resumes = resumes || span_resumes;
        if !resumes {
            // The run still reaches an open fence that ends with the note.
            if note.holds_open_fence() {
                for (at, blank) in lines.iter().enumerate().take(after_run).skip(*i) {
                    let (residue, consumed, synthetic) =
                        slice_columns_mapped(blank, state.body_indent, true);
                    note.push_with(residue, Some(self.first_source_line + at), |_| {
                        Some(consumed as isize - synthetic as isize)
                    });
                }
            }
            return Flow::Stop;
        }
        while *i < after_run {
            let (residue, consumed, synthetic) =
                slice_columns_mapped(lines[*i], state.body_indent, true);
            note.push_with(residue, Some(self.first_source_line + *i), |_| {
                Some(consumed as isize - synthetic as isize)
            });
            *i += 1;
        }
        Flow::Next
    }

    // Form B: a lone `+` attaches the following flush-left block
    // to the note with no indentation (the same continuation
    // marker lists, block quotes and definition bodies use); the
    // attached block ends at a blank line, another `+`, or the
    // next footnote definition.
    fn attached_plus_block(
        &mut self,
        state: &IndentedNote<'a>,
        note: &mut NoteBody,
        line: &str,
        column: usize,
        i: &mut usize,
    ) -> Option<Flow> {
        if !(is_plus_marker(trim_ascii_start(line))
            && column < state.body_indent
            && state
                .container
                .owns_continuation(line, self.decision_at(self.first_source_line + *i)))
        {
            return None;
        }
        let lines = self.lines;
        *i += 1;
        if let Some(open) = note.comment_state.code {
            note.push_with(
                char::from(open.fence_char)
                    .to_string()
                    .repeat(open.fence_len),
                None,
                |_| None,
            );
        }
        let closing: Vec<_> = note.open_colons.closing_widths().collect();
        for width in closing {
            note.push_with(":".repeat(width), None, |_| None);
        }
        let mut attached: Vec<String> = Vec::new();
        let attached_start = *i;
        let pulled_closer = pulled_fence_closer(lines, *i, state.body_indent, state.body_indent);
        let end = pulled_closer.map_or_else(
            || {
                attached_block_past_invisible(
                    CursorLines::Slice(lines),
                    *i,
                    &mut self.comment_closers,
                    &mut self.closer_index,
                    self.options,
                    &mut |a, _| is_blank_line(a) || is_plus_marker(a),
                )
            },
            |closer| closer + 1,
        );
        while *i < end {
            attached.push(if pulled_closer == Some(*i) {
                trim_ascii_start(lines[*i]).to_string()
            } else {
                slice_columns(lines[*i], column, true)
            });
            *i += 1;
        }
        if !attached.is_empty() {
            note.push_blank();
            for (offset, attached_line) in attached.into_iter().enumerate() {
                note.push_with(
                    attached_line,
                    Some(self.first_source_line + attached_start + offset),
                    |_| Some(column as isize),
                );
            }
        }
        Some(Flow::Next)
    }

    fn indented_content_line(
        &mut self,
        state: &mut IndentedNote<'a>,
        note: &mut NoteBody,
        line: &str,
        i: &mut usize,
    ) -> Flow {
        // Strip EXACTLY the body's own indent, not all leading
        // whitespace. A full trim flattened everything the
        // relative indentation says: a nested list marker
        // landed at its parent's column and the sublist became
        // siblings, and a code block's interior indentation -
        // which is content, not layout - was eaten with it
        // (#611). Every other container strips its own prefix
        // and leaves the rest, which is why the same two lines
        // nest in a quote, a div and a list item.
        let dedented = if note.lazy_brace_open {
            line.to_string()
        } else {
            strip_leading_columns(line, state.body_indent)
        };
        let trimmed: &str = &dedented;
        // A CODE FENCE inside the body is opaque, so a
        // definition-shaped line in it is content. Every engine
        // already agrees about that at the top level - none
        // registers `[r]: /u` written inside ``` - and the
        // top-level pass in `extract_link_defs` tracks fences
        // for exactly this reason. Without the state here the
        // line was consumed as a definition and vanished from
        // the code block.
        let in_marker_fence = advance_marker_fence(&mut state.marker_fence, trimmed, &note.lines);
        // A line block's payload is inline content whatever its shape (PART 2 `line_block`).
        let in_line_block = state.note_fence.is_none() && note.comment_state.line_block.is_some();
        if let Some(open) = state.note_fence {
            if is_fence_close(trimmed, open) {
                state.note_fence = None;
            }
        } else if let Some(open) = detect_fence_open(trimmed)
            .filter(|_| !in_line_block)
            .map(|mut open| {
                open.content_col = indent_columns(line);
                open
            })
            .filter(|open| {
                // §10 I4: an unterminated fence does not interrupt an open paragraph.
                !note.lines.last().is_some_and(|above| {
                    let above = trim_ascii_start(above);
                    !is_blank_line(above)
                        && !item_block_opener(above)
                        && detect_fence_open(above).is_none()
                }) || self.closer_maxima.closes_after(*i, *open)
            })
        {
            state.note_fence = Some(open);
        } else if !in_marker_fence
            && !in_line_block
            && note.comment_state.comment.is_none()
            && (!state.open_term || indent_columns(trimmed) == 0)
            && self.hoist_note_link_def(note_link_def_candidate(trimmed, note), *i)
        {
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
            let candidate = note_link_def_candidate(trimmed, note);
            note.push_with(
                format!(
                    "{}{}",
                    &trimmed[..trimmed.len() - candidate.len()],
                    definition_placeholder()
                ),
                Some(self.first_source_line + *i),
                |_| stripped_col(Some(0), line, trimmed),
            );
            *i += 1;
            return Flow::Next;
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
            state.open_term = term_start_column(flush).is_some();
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
            state.body_open_below = false;
        } else if residual == 0 {
            state.body_open_below = true;
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
        let host_colon_opener = residual > 0
            && note.lines.last().is_some_and(|prior| is_blank_line(prior))
            && state.note_fence.is_none()
            && note.comment_state.line_block.is_none()
            && !is_list_marker(trim_ascii_start(state.first))
            && !exact_colon_fence_len(flush)
                .is_some_and(|run| note.open_colons.closing_widths().any(|width| width == run))
            && !exact_colon_fence_len(flush).is_some_and(|run| {
                is_list_marker(trim_ascii_start(state.first))
                    && exact_colon_fence_len(innermost_marker_content(state.first)) == Some(run)
            })
            && is_colon_fence_opener_shape(flush);
        let caption_after_quote_blank = caption_content(flush).is_some()
            && note.lines.last().is_some_and(|line| is_blank_line(line))
            && note
                .lines
                .iter()
                .rev()
                .nth(1)
                .is_some_and(|line| strip_blockquote_prefix(line).is_some());
        // A line at the ancestor body's column can still be lazy prose in
        // the last nested note. Keep that ownership in the source handed to
        // the recursive note pass, just as the hosted-note walk does.
        let nested_lazy = state.body_open_below
            && !is_note_def
            && residual < 2
            && (keeps_paragraph_open(trimmed)
                || (flush.starts_with('[') && own_lines::foldable_plain_line(flush)))
            && nested_note_paragraph_open(note, self.options);
        if nested_lazy {
            let kept = note.frame_lazy(residual, flush);
            note.push_with(kept, Some(self.first_source_line + *i), |_| {
                stripped_col(Some(0), line, flush)
            });
            *i += 1;
            return Flow::Next;
        }
        let trimmed: &str = if caption_after_quote_blank
            || host_colon_opener
            || state.body_open_below && is_note_def && state.note_fence.is_none() && residual < 2
        {
            flush
        } else {
            trimmed
        };
        note.push_with(
            trimmed.to_string(),
            Some(self.first_source_line + *i),
            |_| stripped_col(Some(0), line, trimmed),
        );
        *i += 1;
        Flow::Next
    }

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
    fn hoist_note_link_def(&mut self, text: &str, i: usize) -> bool {
        let Some((label_part, target_part, mut def)) = parse_link_def_line_with_value(text) else {
            return false;
        };
        if label_part.starts_with(['@', '^']) || target_part.trim().is_empty() {
            return false;
        }
        // The DOCUMENT line, so the hoisted node gets a
        // `pos` like every other definition (§4, §10).
        // `first_source_line` is 1-BASED - it is built as
        // a newline count plus one - while `LinkDef.line`
        // is the 0-based index `extract_link_defs`
        // records, so the conversion is explicit here
        // rather than off by one (carve-rs#636).
        def.line = Some(self.first_source_line + i - 1);
        def.raw_label = Some(label_part.to_string());
        self.note_link_defs.insert(label_key(label_part), def);
        true
    }

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
    fn collect_hosted_body(
        &mut self,
        note: &mut NoteBody,
        i: usize,
        raw_def_line: &'a str,
        def: &DefinitionLine<'a>,
        structural_len: usize,
        description_host: bool,
    ) -> usize {
        if self.pass_winning_note(note, i, def.label, def.first) {
            return i;
        }
        let (quote_indents, inside) = note_quote_host(raw_def_line, structural_len);
        let prefix_cols = inside.chars().count() - def.def_line.chars().count();
        let host = NoteHost {
            quote_depth: quote_indents.len(),
            quote_indents,
            note_floor: prefix_cols + footnote_body_floor(def.def_line),
            outer_list_floor: item_floor(&self.columns.frames[0]),
            description_host,
            behind_task_box: behind_task_box(raw_def_line),
        };
        let i = self.hosted_first_block(&host, note, i, def.label, def.first);
        let mut i = self.attach_quoted_plus_block(&host, note, i, raw_def_line);
        // A losing duplicate's `+` line under a `+` note is read as if it were
        // not there (carve#5502).
        let plus_note =
            is_cont_marker_line(def.first) || continuation_marker::is_consumed_marker(def.first);
        while plus_note
            && self
                .lines
                .get(i)
                .and_then(|raw| host.content(raw))
                .is_some_and(|line| {
                    indent_columns(line) == host.note_floor.saturating_sub(2)
                        && parse_footnote_def_line(trim_ascii_start(line)).is_some_and(
                            |(other, first)| {
                                label_key(other) == label_key(def.label)
                                    && continuation_marker::is_consumed_marker(first)
                            },
                        )
                })
        {
            i += 1;
        }
        let source = self.source;
        let track = *self.body_opacity_candidates.get_or_insert_with(|| {
            source.contains("%%%") || source.contains("~~~") || source.contains("```")
        });
        let mut walk = HostedWalk {
            track,
            nested_note: parse_footnote_def_line(trim_ascii_start(def.first))
                .map(|_| footnote_body_floor(trim_ascii_start(def.first))),
            marker_fence: marker_line_fence(def.first),
            opacity: BodyCommentState::new(),
            admitted_fence: None,
            lazy_open: false,
        };
        // A fence opened by an item on the note's marker line has the item's
        // extent. The marker fence tracker handles it; it must not leave the
        // note's own code opacity open after that item ends.
        if walk.track && walk.marker_fence.is_none() {
            walk.opacity.observe(def.first);
        }
        if walk
            .opacity
            .comment
            .is_some_and(|width| !self.hosted_comment_closes(&host, i - 1, width))
        {
            walk.opacity.comment = None;
        }
        let resume_at = self.next_hosted_non_blank(&host, i);
        if plus_note && host.quote_depth > 0 && resume_at > i {
            if let Some(raw) = self.lines.get(resume_at) {
                if let Some(line) = host.content(raw) {
                    let text = trim_ascii_start(line);
                    let walked_col = raw.chars().count() - text.chars().count() + 1;
                    let authored_col = own_lines::authored_visual_column(
                        self.first_source_line + resume_at,
                        walked_col,
                    );
                    // The ownership walk can strip the indentation of a list
                    // pulled through a quote. Recover the authored column so
                    // a list at the note floor stays in the note.
                    if (is_list_marker(text) || item_block_opener(text))
                        && authored_col >= host.note_floor + host.quote_depth * 2
                    {
                        for at in i..resume_at {
                            self.push_hosted_blank(&host, note, at);
                        }
                        let mut candidates = Vec::new();
                        for (offset, raw) in self.lines[resume_at..].iter().enumerate() {
                            let Some(line) = host.content(raw) else {
                                break;
                            };
                            let text = trim_ascii_start(line);
                            if !is_blank_line(line) {
                                let column = raw.chars().count() - text.chars().count() + 1;
                                let authored = own_lines::authored_visual_column(
                                    self.first_source_line + resume_at + offset,
                                    column,
                                );
                                if authored < host.note_floor + host.quote_depth * 2 {
                                    break;
                                }
                            }
                            candidates.push(line);
                        }
                        let count = candidates.len();
                        for (offset, line) in candidates.into_iter().enumerate() {
                            let kept = if is_blank_line(line) {
                                String::new()
                            } else {
                                let text = trim_ascii_start(line);
                                let raw = self.lines[resume_at + offset];
                                let walked = raw.chars().count() - text.chars().count() + 1;
                                let authored = own_lines::authored_visual_column(
                                    self.first_source_line + resume_at + offset,
                                    walked,
                                );
                                format!(
                                    "{}{}",
                                    " ".repeat(authored.saturating_sub(authored_col)),
                                    text
                                )
                            };
                            note.push_with(
                                kept,
                                Some(self.first_source_line + resume_at + offset),
                                |_| {
                                    Some(
                                        (self.lines[resume_at + offset].len() - line.len())
                                            as isize,
                                    )
                                },
                            );
                        }
                        return resume_at + count;
                    }
                }
            }
        }
        if resume_at >= self.lines.len() {
            self.keep_blanks_for_open_fence(&host, note, i, resume_at);
            return i;
        }
        if i == resume_at && self.hosted_marker_column_plus(&host, &mut walk, note, &mut i) {
            // The attached block leaves the walk at its next line.
        } else if !walk.reaches_floor(&host, self.lines[resume_at]) {
            while self.hosted_lazy_line(&host, &mut walk, note, &mut i) {}
            if !walk.lazy_open {
                self.keep_blanks_for_open_fence(&host, note, i, resume_at);
                return i;
            }
        }
        while i < self.lines.len() {
            if self.hosted_line(&host, &mut walk, note, &mut i) == Flow::Stop {
                break;
            }
        }
        i
    }

    /// A later definition of `own` or of an already defined label whose `+` the
    /// walk consumed: it takes nothing and opens no sibling note (PART 9R R2).
    fn losing_plus_line(&self, own: &str, other: &str, first: &str) -> bool {
        let key = label_key(other);
        is_line_comment_any_column(first) && (key == label_key(own) || self.is_defined(&key))
    }

    fn is_defined(&self, key: &str) -> bool {
        self.definition_keys.contains_key(key)
            || self.defined_before.is_some_and(|(first_lines, line)| {
                first_lines.get(key).is_some_and(|&at| at < line)
            })
    }

    /// A hosted note's marker-line `+` takes its one block through the host's prefix.
    fn hosted_first_block(
        &mut self,
        host: &NoteHost,
        note: &mut NoteBody,
        i: usize,
        label: &str,
        first: &str,
    ) -> usize {
        let lines = self.lines;
        if trim_ascii(first) != "+"
            || !self
                .decision_at(self.first_source_line + i - 1)
                .is_some_and(|decision| decision.owner == Some(continuation_marker::Kind::Footnote))
        {
            return i;
        }
        // A second lone `+` passes the grant on to the block below it.
        let i = i + usize::from(
            !host.behind_task_box
                && lines
                    .get(i)
                    .and_then(|raw| host.content(raw))
                    .is_some_and(|line| {
                        trim_ascii(line) == "+"
                            && indent_columns(line) == host.note_floor.saturating_sub(2)
                    })
                && lines
                    .get(i + 1)
                    .and_then(|raw| host.content(raw))
                    .is_some_and(|line| !is_blank_line(line)),
        );
        let mut attached: Vec<String> = lines[i..]
            .iter()
            .map_while(|raw| host.content(raw).filter(|line| !is_blank_line(line)))
            .map(|line| strip_leading_columns(line, host.note_floor.saturating_sub(2)))
            .collect();
        // A lazy line without the quote prefix may finish a refused brace run
        // in the first attached paragraph. Keep it in that block before the
        // separator is inserted, so the paragraph reader sees the whole run.
        if let Some(opener) = attached
            .last()
            .filter(|line| trim_ascii_start(line).starts_with('{') && !line.contains('}'))
        {
            let continuation: Vec<&str> = lines[i + attached.len()..]
                .iter()
                .take_while(|raw| !is_blank_line(raw) && host.content(raw).is_none())
                .copied()
                .collect();
            // The lazy line keeps its written column, so it can also finish
            // a valid attribute run (CARVE-P2-035, carve#5619).
            let count = match refused_brace_run(
                std::iter::once(opener.as_str()).chain(continuation.iter().copied()),
                true,
            ) {
                RefusedBraceRun::Lines(count) => Some(count),
                RefusedBraceRun::No(_) if quote_depth(lines[i + attached.len() - 1]) == 1 => {
                    // The paragraph the run leads folds lazily into the note too.
                    attrs_block_len_at(0, opener.as_str(), continuation.iter().copied())
                        .filter(|&count| count > 1)
                        .map(|count| {
                            count
                                + continuation[count - 1..]
                                    .iter()
                                    .take_while(|line| {
                                        line_starts_paragraph(trim_ascii_start(line))
                                    })
                                    .count()
                        })
                }
                RefusedBraceRun::No(_) => None,
            };
            if let Some(count) = count {
                attached.extend(
                    continuation[..count.saturating_sub(1)]
                        .iter()
                        .map(|line| (*line).to_owned()),
                );
            }
        }
        // A lazy line continues the attached paragraph (PART 0 S4).
        if quote_depth(lines[i.saturating_sub(1)]) == 1
            && !ends_in_refused_brace_run(&attached)
            && body_ends_with_open_paragraph(&attached.join("\n"), self.options)
        {
            while let Some(raw) = lines.get(i + attached.len()).filter(|raw| {
                !is_blank_line(raw)
                    && host.content(raw).is_none()
                    && line_starts_paragraph(trim_ascii_start(raw))
                    && !trim_ascii_start(raw).starts_with('{')
            }) {
                attached.push((*raw).to_owned());
            }
        }
        // The line after a refused run is still in its paragraph, even when
        // it has no host prefix and would otherwise start a sibling block.
        if ends_in_refused_brace_run(&attached) {
            if let Some(raw) = lines
                .get(i + attached.len())
                .filter(|raw| !is_blank_line(raw))
            {
                let follower = host.content(raw).map_or_else(
                    || (*raw).to_owned(),
                    |line| strip_leading_columns(line, host.note_floor.saturating_sub(2)),
                );
                attached.push(follower);
            }
        }
        if attached.is_empty() {
            return i;
        }
        // A list marker ends an attached paragraph; the note then opens its
        // list there (PART 0 S3, carve#4860).
        let attached_refs: Vec<&str> = attached.iter().map(String::as_str).collect();
        let lead = first_visible_note_line(&attached_refs).unwrap_or(0);
        // A definition opens its own span, even when the preceding note has
        // a first-block grant. In particular, a losing duplicate must collect
        // and discard its own lazy continuation instead of lending that text
        // to the winning note.
        // A definition whose `+` the walk consumed leaves a block at this note's
        // marker column to this note (carve#5502, carve#5542).
        let lead_at_marker = attached
            .get(lead)
            .is_some_and(|line| !line.starts_with([' ', '\t']));
        let winning_plain_sibling = attached.iter().take(lead).any(|line| {
            parse_footnote_def_line(trim_ascii_start(line)).is_some_and(|(other, first)| {
                label_key(other) != label_key(label)
                    && !continuation_marker::is_consumed_marker(first)
            })
        });
        if !winning_plain_sibling
            && attached.iter().take(lead + 1).any(|line| {
                parse_footnote_def_line(trim_ascii_start(line)).is_some_and(|(_, first)| {
                    !lead_at_marker || !continuation_marker::is_consumed_marker(first)
                })
            })
        {
            note.lines[0].clear();
            note.line_map[0] = None;
            if self.positions {
                note.col_map[0] = None;
            }
            return i;
        }
        if line_starts_paragraph(trim_ascii_start(&attached[lead])) {
            if host.description_host {
                if let Some(at) = (lead + 1..attached.len()).find(|&at| {
                    let line = host.content(lines[i + at]).unwrap_or(lines[i + at]);
                    indent_columns(line) < host.note_floor
                        && (trim_ascii_start(line).starts_with(":: ")
                            || strip_definition_marker(trim_ascii_start(line)).is_some())
                }) {
                    attached.truncate(at);
                }
            }
            if let Some(at) = (lead + 1..attached.len()).find(|&at| {
                is_list_marker(trim_ascii_start(&attached[at]))
                    && !(host_indent(host, lines[i + at]) >= host_indent(host, lines[i + lead])
                        && ends_in_refused_brace_run(&attached[lead..at]))
            }) {
                attached.truncate(at);
            }
        }
        // A block opener at the note's floor ends an attached paragraph or
        // quote, as it does for a root note (PART 9 §17 L3).
        let lead_is_quote = strip_blockquote_prefix(trim_ascii_start(&attached[lead])).is_some();
        if lead_is_quote || line_starts_paragraph(trim_ascii_start(&attached[lead])) {
            if let Some(at) = (lead + 1..attached.len()).find(|&at| {
                let line = host.content(lines[i + at]).unwrap_or(lines[i + at]);
                let text = trim_ascii_start(line);
                indent_columns(line) >= host.note_floor
                    && item_block_opener(text)
                    && !(lead_is_quote && strip_blockquote_prefix(text).is_some())
                    && !ends_in_refused_brace_run(&attached[lead..at])
            }) {
                attached.truncate(at);
            }
        }
        // A definition at the marker column opens a sibling note (carve-rs#4090),
        // unless it is the payload of a fenced or delimited first block.
        let lead_text = attached[lead..]
            .iter()
            .map(|line| trim_ascii_start(line))
            .find(|line| parse_standalone_attrs(line).is_none())
            .unwrap_or("");
        let opaque_lead = detect_fence_open(lead_text).is_some()
            || detect_comment_fence_line(lead_text).is_some()
            || detect_container_open(lead_text).is_some()
            || detect_line_block_open(lead_text).is_some()
            || detect_hardbreaks_block_open(lead_text).is_some()
            || detect_quote_block_open(lead_text).is_some();
        if let Some(at) = attached
            .iter()
            .enumerate()
            .skip(lead + 1)
            .position(|(at, line)| {
                !opaque_lead
                    && !line.starts_with([' ', '\t'])
                    && parse_footnote_def_line(line)
                        .is_some_and(|(other, first)| !self.losing_plus_line(label, other, first))
                    && !ends_in_refused_brace_run(&attached[lead..at])
            })
        {
            attached.truncate(lead + 1 + at);
        }
        let attached_refs: Vec<&str> = attached.iter().map(String::as_str).collect();
        let mut count = attached_one_block_lines(CursorLines::Slice(&attached_refs), self.options);
        // The invisible lines ahead of the lead come with it (carve#4449).
        if lead > 0 && lead < attached_refs.len() {
            count = count.max(
                lead + attached_one_block_lines(
                    CursorLines::Slice(&attached_refs[lead..]),
                    self.options,
                ),
            );
        }
        let closers = if lines
            .get(i + count)
            .and_then(|raw| host.content(raw))
            .is_some_and(is_blank_line)
        {
            let remaining: Vec<&str> = lines[i + count + 1..]
                .iter()
                .map_while(|raw| host.content(raw))
                .collect();
            attached_first_blank_closers(&attached_refs[..count], &remaining)
        } else {
            Vec::new()
        };
        note.lines[0].clear();
        note.comment_state = BodyCommentState::new();
        note.stanza = crate::parse::definition::LineBlockTail::default();
        note.open_colons = OpenBodyColons::default();
        note.line_map[0] = None;
        if self.positions {
            note.col_map[0] = None;
        }
        let mut in_skipped_note = false;
        for (offset, line) in attached.iter().take(count).cloned().enumerate() {
            let col = (lines[i + offset].chars().count() - line.chars().count()) as isize;
            in_skipped_note = offset < lead
                && if parse_footnote_def_line(trim_ascii_start(&line)).is_some() {
                    false
                } else {
                    in_skipped_note
                        || offset > 0
                            && parse_footnote_def_line(trim_ascii_start(attached_refs[offset - 1]))
                                .is_some()
                }
                && own_lines::foldable_plain_line(trim_ascii_start(&line));
            let kept = if in_skipped_note {
                note.frame_lazy(indent_columns(&line), trim_ascii_start(&line))
            } else {
                line
            };
            note.push_with(kept, Some(self.first_source_line + i + offset), |_| {
                Some(col)
            });
        }
        for closer in closers {
            note.push_with(closer, None, |_| None);
        }
        note.block_separator = Some(note.lines.len());
        note.push_blank();
        i + count
    }

    fn attach_quoted_plus_block(
        &mut self,
        host: &NoteHost,
        note: &mut NoteBody,
        i: usize,
        raw_def_line: &str,
    ) -> usize {
        let lines = self.lines;
        let applies = host.quote_depth > 0
            && raw_def_line.starts_with('>')
            && host.quote_indents.iter().all(|&indent| indent == 0)
            && lines.get(i).is_some_and(|raw| raw.starts_with('>'))
            && lines.get(i).and_then(|raw| host.content(raw)) == Some("+")
            && self
                .decision_at(self.first_source_line + i)
                .is_some_and(|decision| {
                    decision.owner == Some(continuation_marker::Kind::Footnote)
                });
        if !applies {
            return i;
        }
        let start = i + 1;
        let attached: Vec<&str> = lines[start..]
            .iter()
            .map_while(|raw| host.content(raw).filter(|line| !is_blank_line(line)))
            .take_while(|line| *line != "+")
            .collect();
        if attached.is_empty() {
            return i;
        }
        let count = attached_one_block_lines(CursorLines::Slice(&attached), self.options);
        note.push_blank();
        for (offset, line) in attached.into_iter().take(count).enumerate() {
            note.push_with(
                line.to_string(),
                Some(self.first_source_line + start + offset),
                |_| Some((lines[start + offset].chars().count() - line.chars().count()) as isize),
            );
        }
        start + count
    }

    fn hosted_comment_closes(&mut self, host: &NoteHost, at: usize, width: usize) -> bool {
        let lines = self.lines;
        let index = self
            .container_closers
            .get_or_insert_with(|| ContainerCommentClosers::build(lines));
        let floor = host
            .content(lines[at])
            .map(|line| indent_columns(line).min(host.note_floor))
            .unwrap_or(host.note_floor);
        index.closes_in_note(lines, at, host.quote_depth, floor, width)
    }

    // A matched fence at or past a marker-line note's body column
    // opens in the note. The closer may return to the body column
    // or use the opener's authored base (CARVE-P2-017, carve#3023).
    fn closed_note_fence(&self, host: &NoteHost, at: usize) -> bool {
        let Some(line) = host.content(self.lines[at]) else {
            return false;
        };
        let column = indent_columns(line);
        let Some(mut open) = detect_fence_open(trim_ascii_start(line)) else {
            return false;
        };
        open.quoted = host.quote_depth > 0;
        // A closer beyond a line that has already returned to the host cannot
        // admit the fence through that host line. The fence ends with its note.
        for raw in &self.lines[at + 1..] {
            let Some(body) = host.content(raw) else {
                break;
            };
            let body_col = indent_columns(body);
            if !is_blank_line(body) && body_col < host.note_floor {
                break;
            }
            if (body_col == host.note_floor || body_col == column)
                && is_fence_close(trim_ascii_start(body), open)
            {
                return true;
            }
        }
        false
    }

    fn hosted_lazy_flow(
        &mut self,
        host: &NoteHost,
        walk: &mut HostedWalk,
        note: &mut NoteBody,
        i: &mut usize,
    ) -> Flow {
        if self.hosted_lazy_line(host, walk, note, i) {
            Flow::Next
        } else {
            Flow::Stop
        }
    }

    /// A footnote body takes a lazy line like every container (carve#3885):
    /// a line short of the note's floor that interrupts no paragraph joins an
    /// open one.
    fn hosted_lazy_line(
        &mut self,
        host: &NoteHost,
        walk: &mut HostedWalk,
        note: &mut NoteBody,
        i: &mut usize,
    ) -> bool {
        let Some(&raw) = self.lines.get(*i) else {
            return false;
        };
        if self.is_marker_column_plus(host, walk, *i) {
            return false;
        }
        // An accepted `+` the layout gave to another host continues that host.
        if is_plus_marker(trim_ascii_start(without_blockquote_prefixes(
            trim_ascii_start(raw),
        ))) && self
            .decision_at(self.first_source_line + *i)
            .is_some_and(|decision| {
                decision
                    .owner
                    .is_some_and(|owner| owner != continuation_marker::Kind::Footnote)
            })
        {
            return false;
        }
        let inside = host.content(raw);
        let rest = trim_ascii_start(
            inside.unwrap_or_else(|| without_blockquote_prefixes(trim_ascii_start(raw))),
        );
        // A refused marker-line run keeps a following note-shaped line as
        // paragraph text rather than registering a sibling definition.
        // List and heading markers at this column still leave the host first.
        if parse_footnote_def_line(rest).is_some() && ends_in_refused_brace_run(&note.lines) {
            let local = inside.unwrap_or_else(|| without_blockquote_prefixes(raw));
            let kept = note.frame_lazy(indent_columns(local), rest);
            note.push_with(kept, Some(self.first_source_line + *i), |_| {
                stripped_col(Some(0), raw, rest)
            });
            walk.lazy_open = true;
            *i += 1;
            return true;
        }
        let verse_text = note.stanza.in_block() == Some(true) && !is_list_marker(rest);
        let lazy = !is_blank_line(rest)
            && walk.admitted_fence.is_none()
            && walk.opacity.comment.is_none()
            && walk.opacity.code.is_none()
            && !document_abbreviation_below_note(
                raw,
                &self.columns,
                self.document_colon_open.get(*i).copied().unwrap_or(false),
            )
            && !interrupts_paragraph_with_rest(rest, &self.lines[*i + 1..])
            && !is_colon_fence_opener_shape(rest)
            && !exact_colon_fence_len(rest).is_some_and(|run| run >= 3)
            && parse_link_def_line(rest).is_none()
            && parse_footnote_def_line(rest).is_none()
            // A list marker that misses a marker body closes it (PART 0 S3, carve#4860).
            && !is_list_marker(rest)
            // Outside the note's quotes, a marker or a quote left of an outer
            // item's content column leaves the item.
            && !(inside.is_none()
                && host.quote_depth > 0
                && host
                    .outer_list_floor
                    .is_some_and(|floor| indent_columns(raw) < floor)
                && (is_list_marker(trim_ascii_start(raw))
                    || strip_blockquote_prefix(trim_ascii_start(raw)).is_some()))
            // The next entry of the hosting list stays out of the note.
            && !(host.description_host
                && !verse_text
                && (rest.starts_with(":: ") || strip_definition_marker(rest).is_some()))
            && !opens_code_fence(rest)
            && (verse_text || walk.lazy_open || note_paragraph_open(note, rest, self.options));
        if !lazy {
            return false;
        }
        let local = inside.unwrap_or_else(|| without_blockquote_prefixes(raw));
        let kept = note.frame_lazy(indent_columns(local), rest);
        note.push_with(kept, Some(self.first_source_line + *i), |_| {
            stripped_col(Some(0), raw, rest)
        });
        walk.lazy_open = true;
        *i += 1;
        true
    }

    fn next_hosted_non_blank(&self, host: &NoteHost, mut at: usize) -> usize {
        while at < self.lines.len() && host.blank(self.lines[at]) {
            at += 1;
        }
        at
    }

    fn is_marker_column_plus(&self, host: &NoteHost, walk: &HostedWalk, at: usize) -> bool {
        let Some(&raw) = self.lines.get(at) else {
            return false;
        };
        let marker_col = host.note_floor.saturating_sub(2);
        !host.behind_task_box
            && !walk.reaches_floor(host, raw)
            && host
                .content(raw)
                .is_some_and(|line| indent_columns(line) == marker_col && trim_ascii(line) == "+")
            && self
                .decision_at(self.first_source_line + at)
                .is_some_and(|decision| decision.owner == Some(continuation_marker::Kind::Footnote))
    }

    /// A lone `+` at the column of a note's marker gives the note its next
    /// block, measured from that column (PART 9 §17 L3).
    fn hosted_marker_column_plus(
        &mut self,
        host: &NoteHost,
        walk: &mut HostedWalk,
        note: &mut NoteBody,
        i: &mut usize,
    ) -> bool {
        if !self.is_marker_column_plus(host, walk, *i) {
            return false;
        }
        let marker_col = host.note_floor.saturating_sub(2);
        let lines = self.lines;
        let start = *i + 1;
        let attached: Vec<String> = lines[start..]
            .iter()
            .map_while(|raw| host.content(raw).filter(|line| !is_blank_line(line)))
            .map(|line| strip_leading_columns(line, marker_col))
            .collect();
        let attached_refs: Vec<&str> = attached.iter().map(String::as_str).collect();
        let (mut comment_closers, mut closer_index) = (None, None);
        let count = attached_block_past_invisible(
            CursorLines::Slice(&attached_refs),
            0,
            &mut comment_closers,
            &mut closer_index,
            self.options,
            &mut |line, _| is_blank_line(line) || is_plus_marker(line),
        );
        if let Some(open) = note.comment_state.code {
            note.push_with(
                char::from(open.fence_char)
                    .to_string()
                    .repeat(open.fence_len),
                None,
                |_| None,
            );
        }
        let closing: Vec<_> = note.open_colons.closing_widths().collect();
        for width in closing {
            note.push_with(":".repeat(width), None, |_| None);
        }
        note.push_blank();
        let mut visible = false;
        for (offset, line) in attached.into_iter().take(count).enumerate() {
            let source_line = self.first_source_line + start + offset;
            let col = (lines[start + offset].chars().count() - line.chars().count()) as isize;
            if !visible && self.hoist_note_link_def(&line, start + offset) {
                note.push_with(definition_placeholder(), Some(source_line), |_| Some(col));
                continue;
            }
            visible |= !is_line_comment_any_column(&line);
            note.push_with(line, Some(source_line), |_| Some(col));
        }
        walk.lazy_open = false;
        walk.marker_fence = None;
        walk.opacity = BodyCommentState::new();
        *i = start + count;
        true
    }

    fn hosted_line(
        &mut self,
        host: &NoteHost,
        walk: &mut HostedWalk,
        note: &mut NoteBody,
        i: &mut usize,
    ) -> Flow {
        let raw = self.lines[*i];
        // S1 pairs a held span's exact closer before checking whether this
        // line still carries the quote prefix of the note's host.
        if note
            .comment_state
            .held_span()
            .is_some_and(|run| is_comment_fence_close_any_column(raw, run))
        {
            note.push_with(
                kept_comment_delimiter(raw),
                Some(self.first_source_line + *i),
                |_| Some(0),
            );
            *i += 1;
            walk.opacity.comment = None;
            return Flow::Next;
        }
        let Some(line) = host.content(raw) else {
            return self.hosted_lazy_flow(host, walk, note, i);
        };
        if is_blank_line(line) {
            walk.lazy_open = false;
            return self.hosted_blank_run(host, walk, note, i);
        }
        if self.hosted_marker_column_plus(host, walk, note, i) {
            return Flow::Next;
        }
        // Ownership can shift a quoted follower onto the note's body column.
        // Use its authored column after a refused marker-line plus: the band
        // between the marker and body columns still belongs to the quote.
        let follows_refused_plus = host.quote_depth > 0
            && self.lines.get(i.saturating_sub(1)).is_some_and(|above| {
                host.content(above).is_some_and(|text| {
                    parse_footnote_def_line(trim_ascii_start(text))
                        .is_some_and(|(_, first)| continuation_marker::is_consumed_marker(first))
                })
            });
        if follows_refused_plus {
            let text = trim_ascii_start(line);
            let walked_col = raw.chars().count() - text.chars().count() + 1;
            let authored_col =
                own_lines::authored_visual_column(self.first_source_line + *i, walked_col);
            let marker_col = host.note_floor + host.quote_depth * 2 - 2;
            if authored_col > marker_col && authored_col < marker_col + 2 {
                return Flow::Stop;
            }
        }
        if !walk.reaches_floor(host, raw) {
            return self.hosted_lazy_flow(host, walk, note, i);
        }
        walk.lazy_open = false;
        let dedented = if note.lazy_brace_open {
            line.to_string()
        } else {
            strip_leading_columns(line, host.note_floor)
        };
        if trim_ascii(line) == "+"
            && self
                .decision_at(self.first_source_line + *i)
                .is_some_and(|decision| decision.owner == Some(continuation_marker::Kind::Footnote))
        {
            let start = *i + 1;
            let lines = self.lines;
            let attached: Vec<&str> = lines[start..]
                .iter()
                .map_while(|raw| host.content(raw).filter(|line| !is_blank_line(line)))
                .take_while(|line| *line != "+")
                .collect();
            let count = attached_one_block_lines(CursorLines::Slice(&attached), self.options);
            note.push_blank();
            for (offset, line) in attached.into_iter().take(count).enumerate() {
                note.push_with(
                    line.to_string(),
                    Some(self.first_source_line + start + offset),
                    |_| {
                        Some(
                            (lines[start + offset].chars().count() - line.chars().count()) as isize,
                        )
                    },
                );
            }
            *i = start + count;
            return Flow::Next;
        }
        // A link definition at the note's own column is metadata,
        // even when an open marker-line paragraph precedes it.
        let definition_text = note_link_def_candidate(&dedented, note);
        let in_marker_fence = advance_marker_fence(&mut walk.marker_fence, &dedented, &note.lines);
        if indent_columns(line) >= host.note_floor
            && !in_marker_fence
            && walk.admitted_fence.is_none()
            && walk.opacity.comment.is_none()
            && walk.opacity.code.is_none()
            && note.comment_state.line_block.is_none()
            && self.hoist_note_link_def(definition_text, *i)
        {
            note.push_with(
                format!(
                    "{}{}",
                    &dedented[..dedented.len() - definition_text.len()],
                    definition_placeholder()
                ),
                Some(self.first_source_line + *i),
                |_| {
                    let prefix = raw.chars().count() - line.chars().count();
                    stripped_col(Some(prefix as isize), line, &dedented)
                },
            );
            *i += 1;
            return Flow::Next;
        }
        let col = if self.positions {
            let prefix = raw.chars().count() - line.chars().count();
            stripped_col(Some(prefix as isize), line, &dedented)
        } else {
            None
        };
        if walk.opacity.comment.is_none() && note.comment_state.line_block.is_none() {
            self.track_admitted_fence(host, walk, line, *i);
        }
        let was_comment = walk.opacity.comment.is_some();
        if walk.track {
            walk.opacity.observe(&dedented);
        }
        if !was_comment
            && walk
                .opacity
                .comment
                .is_some_and(|width| !self.hosted_comment_closes(host, *i, width))
        {
            walk.opacity.comment = None;
        }
        // At the outer note's body column a plain line is lazy in the nested
        // note's open paragraph (PART 0 S4); the body pass reads LAZY as that.
        let column = indent_columns(&dedented);
        let text = trim_ascii_start(&dedented);
        let below_nested = walk.nested_note.is_some_and(|floor| column < floor);
        let lazy_in_nested = below_nested
            && walk.opacity.code.is_none()
            && walk.opacity.comment.is_none()
            && !interrupts_paragraph_with_rest(text, &self.lines[*i + 1..])
            && !is_colon_fence_opener_shape(text)
            && !exact_colon_fence_len(text).is_some_and(|run| run >= 3)
            && parse_link_def_line(text).is_none()
            && parse_footnote_def_line(text).is_none()
            && !opens_code_fence(text)
            && nested_note_paragraph_open(note, self.options);
        if walk.opacity.code.is_none() && parse_footnote_def_line(text).is_some() {
            walk.nested_note = Some(column + footnote_body_floor(text));
        } else if below_nested && !lazy_in_nested {
            // Any other line short of the nested body closes that note.
            walk.nested_note = None;
        }
        let kept = if lazy_in_nested {
            note.frame_lazy(column, text)
        } else {
            dedented
        };
        note.push_with(kept, Some(self.first_source_line + *i), |_| col);
        *i += 1;
        Flow::Next
    }

    fn track_admitted_fence(&self, host: &NoteHost, walk: &mut HostedWalk, line: &str, i: usize) {
        let column = indent_columns(line);
        if walk.admitted_fence.is_some_and(|(open, base)| {
            (column == host.note_floor || column == base)
                && is_fence_close(trim_ascii_start(line), open)
        }) {
            walk.admitted_fence = None;
        } else if walk.admitted_fence.is_none()
            && column >= host.note_floor
            && self.closed_note_fence(host, i)
        {
            if let Some(open) = detect_fence_open(trim_ascii_start(line)) {
                walk.admitted_fence = Some((open, column));
            }
        }
    }

    fn hosted_blank_run(
        &mut self,
        host: &NoteHost,
        walk: &mut HostedWalk,
        note: &mut NoteBody,
        i: &mut usize,
    ) -> Flow {
        let lines = self.lines;
        let after = self.next_hosted_non_blank(host, *i + 1);
        if after >= lines.len() || !walk.reaches_floor(host, lines[after]) {
            self.keep_blanks_for_open_fence(host, note, *i, after);
            return Flow::Stop;
        }
        while *i < after {
            let residue = self.push_hosted_blank(host, note, *i);
            if walk.track {
                walk.opacity.observe(&residue);
            }
            *i += 1;
        }
        Flow::Next
    }

    /// A blank run that ends the note still reaches the note's open fence
    /// (PART 0 S2), and stays in the host as its separator.
    fn keep_blanks_for_open_fence(
        &self,
        host: &NoteHost,
        note: &mut NoteBody,
        from: usize,
        to: usize,
    ) {
        if from < to && note.holds_open_fence() {
            for at in from..to {
                self.push_hosted_blank(host, note, at);
            }
        }
    }

    fn push_hosted_blank(&self, host: &NoteHost, note: &mut NoteBody, at: usize) -> String {
        let raw = self.lines[at];
        let line = host.content(raw).expect("checked quote prefix");
        let prefix = raw.chars().count() - line.chars().count();
        let (residue, consumed, synthetic) = slice_columns_mapped(line, host.note_floor, true);
        note.push_with(residue.clone(), Some(self.first_source_line + at), |_| {
            Some((prefix + consumed) as isize - synthetic as isize)
        });
        residue
    }

    fn register_definition(
        &mut self,
        normalized_label: String,
        def: &DefinitionLine<'a>,
        note: NoteBody,
    ) {
        self.definition_keys
            .insert(normalized_label, def.label.to_string());
        self.defs
            .insert(def.label.to_string(), definition_source(note));
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
    fn push_definition_replacement(
        &mut self,
        raw_def_line: &str,
        def: &DefinitionLine<'a>,
        stripped: &StrippedContainerLine<'a>,
        def_start_line: usize,
        attached_nonparagraph: bool,
    ) {
        let mut replacement = if def.authored_quote_def {
            raw_def_line
                .strip_suffix(def.def_line)
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
            let document_column = definition_at_quote_document_column(&replacement, stripped.bare);
            if !def.authored_quote_def {
                replacement.push_str(&stripped.bare[..leading_ws(stripped.bare)]);
            }
            replacement.push_str(&if document_column {
                document_definition_placeholder()
            } else if attached_nonparagraph {
                attached_nonparagraph_definition_placeholder()
            } else {
                definition_placeholder()
            });
            replacement.extend(footnote_placeholder_tag());
        }
        replacement.push_str(
            &" ".repeat(
                raw_def_line
                    .chars()
                    .count()
                    .saturating_sub(replacement.chars().count()),
            ),
        );
        self.body.push(replacement);
        self.body_line_map.push(Some(def_start_line));
    }

    fn finish(self) -> FootnoteExtraction {
        (
            MappedSource {
                newlines_counted: (0, 0),
                // The document body's lines are top-level: the footnote-definition
                // extraction removes whole lines, never a prefix, so nothing has
                // been stripped from the front of the ones that remain.
                col_map: if self.positions {
                    vec![Some(0); self.body.len()]
                } else {
                    Vec::new()
                },
                source: joined_source(&self.body),
                item_marker_at_start: false,
                marker_tail: None,
                marker_brace_column: None,
                reached: Vec::new(),
                opaque_blank_lines: Vec::new(),
                discarded_outer_blank: false,
                outer_plus_after_blank: false,
                in_quoted_body: false,
                sublists_carry_authored_base: false,
                line_map: self.body_line_map,
            },
            self.defs,
            self.def_positions,
            self.note_link_defs,
        )
    }
}

/// Track a fence opened on a list item's marker line through one body line and
/// say whether the line is that fence's payload or closer. The fence ends with
/// its item, at a line below the item's content column, and a marker folded into
/// a paragraph opens none.
/// Find the closer at the column the host reads: the marker column for a
/// marker-line `+`, or the note body column for a later standalone `+`.
fn pulled_fence_closer(
    lines: &[&str],
    opener: usize,
    body_column: usize,
    closer_column: usize,
) -> Option<usize> {
    let text = lines.get(opener)?;
    if indent_columns(text) >= body_column {
        return None;
    }
    let open = detect_fence_open(trim_ascii_start(text))?;
    let mut past_blank = false;
    for (at, line) in lines.iter().enumerate().skip(opener + 1) {
        if is_blank_line(line) {
            past_blank = true;
        } else if !past_blank {
            if is_fence_close(
                strip_leading_columns(line, indent_columns(text)).as_str(),
                open,
            ) {
                return None;
            }
        } else if closer_column == body_column && indent_columns(line) < body_column {
            return None;
        } else if indent_columns(line) == closer_column
            && is_fence_close(trim_ascii_start(line), open)
        {
            return Some(at);
        }
    }
    None
}

fn advance_marker_fence(
    fence: &mut Option<(FenceOpen, usize)>,
    line: &str,
    body: &[String],
) -> bool {
    let ended =
        fence.is_some_and(|(_, column)| !is_blank_line(line) && indent_columns(line) < column);
    if ended {
        *fence = None;
    }
    if let Some((open, _)) = *fence {
        if is_fence_close(trim_ascii_start(line), open) {
            *fence = None;
        }
        return true;
    }
    // A line the ended fence held, or its closer, leaves no paragraph open.
    if ended
        || !body.last().is_some_and(|above| {
            let above = trim_ascii_start(above);
            !is_blank_line(above)
                && detect_list_marker_full(above).is_none()
                && detect_fence_open(above).is_none()
        })
    {
        *fence = marker_line_fence(line);
    }
    false
}

/// The fence a line opens on a list item's marker line, with the item's
/// content column, if any.
fn marker_line_fence(line: &str) -> Option<(FenceOpen, usize)> {
    let indent = indent_columns(line);
    let line = trim_ascii_start(line);
    let bottom = innermost_marker_content(line);
    if bottom.len() == line.len() {
        return None;
    }
    let column = indent + innermost_marker_content_col(line);
    detect_fence_open(bottom).map(|open| (open, column))
}

fn note_link_def_candidate<'a>(line: &'a str, note: &NoteBody) -> &'a str {
    let text = trim_ascii_start(line);
    let unquoted = without_blockquote_prefixes(text);
    if unquoted.len() == text.len()
        || parse_link_def_line(unquoted).is_none()
        || note
            .quoted_fence
            .is_some_and(|(_, fence_depth)| quote_depth(text) >= fence_depth)
    {
        return text;
    }
    // A quoted definition defines beside an open quote paragraph too (PART 9R R1a).
    unquoted
}

fn quote_depth(mut text: &str) -> usize {
    let mut depth = 0;
    while let Some(rest) = strip_blockquote_prefix(trim_ascii_start(text)) {
        text = rest;
        depth += 1;
    }
    depth
}

fn strip_quote_levels(mut text: &str, levels: usize) -> Option<&str> {
    for _ in 0..levels {
        text = strip_blockquote_prefix(trim_ascii_start(text))?;
    }
    Some(text)
}

/// The quoted code fence still open after `line`, with its quote depth.
fn next_quoted_fence(open: Option<(FenceOpen, usize)>, line: &str) -> Option<(FenceOpen, usize)> {
    if !trim_ascii_start(line).starts_with('>') {
        return None;
    }
    let line_depth = quote_depth(line);
    if let Some((fence, fence_depth)) = open {
        // A deeper quoted line is the fence's payload, never its closer.
        if line_depth > fence_depth {
            return open;
        }
        if line_depth == fence_depth {
            let closes = strip_quote_levels(line, fence_depth)
                .is_some_and(|body| is_fence_close(body, fence));
            return (!closes).then_some((fence, fence_depth));
        }
        // A shallower line ended the fence's quote and may open a fence of its own.
    }
    detect_fence_open(without_blockquote_prefixes(trim_ascii_start(line)))
        .map(|fence| (fence, line_depth))
}

/// Does the note body end on a comment that is still open at line `at`?
fn held_comment_left_open(def_lines: &[String], lines: &[&str], at: usize) -> bool {
    def_lines.last().is_some_and(|last| {
        is_line_comment_any_column(last)
            || detect_comment_fence_line_any_column(last).is_some_and(|open| {
                !lines[at..]
                    .iter()
                    .any(|next| is_comment_fence_close_any_column(next, open.fence_len))
            })
    }) || ends_with_empty_comment_fence(def_lines)
}

/// The quote levels in front of a container-hosted definition, and the text
/// after them.
fn note_quote_host(raw_def_line: &str, structural_len: usize) -> (Vec<usize>, &str) {
    let mut quote_indents = Vec::new();
    let mut inside = raw_def_line;
    // A marker in front of the quote (`- > [^f]: n`) stands for the
    // same number of spaces on the lines below it.
    loop {
        let mut indent = leading_ws(inside);
        let mut item = detect_list_marker_full(inside).map(|marker| marker.content);
        while let Some(nested) = item.and_then(detect_list_marker_full) {
            item = Some(nested.content);
        }
        if let Some(content) = item
            .or_else(|| strip_definition_marker(&inside[indent..]).map(|(body, _)| body))
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
    (quote_indents, inside)
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

/// The first line from `from` that ends a note holding a live `%%%` span
/// (nonblank, not `%%`-led, left of `body_indent`), and whether it stands
/// left of the note's marker column `def_column`, so nothing of the host
/// follows a blank before it. The end of the lines also separates. A line at
/// the body column outside a `%%%` span opened here is visible: the blank
/// before it separates nothing (§17 L1c).
fn span_note_end(
    lines: &[&str],
    from: usize,
    def_column: usize,
    body_indent: usize,
) -> (usize, bool) {
    let mut span = None;
    for (at, line) in lines.iter().enumerate().skip(from) {
        let line = strip_lazy(line);
        if is_blank_line(line) {
            continue;
        }
        let column = indent_columns(line);
        let rest = trim_ascii_start(line);
        if !rest.starts_with("%%") && column < body_indent {
            return (at, column < def_column);
        }
        let run = rest.bytes().take_while(|&b| b == b'%').count();
        if run >= 3 {
            span = if span == Some(run) {
                None
            } else {
                span.or(Some(run))
            };
        } else if span.is_none() && !rest.starts_with("%%") {
            return (at, false);
        }
    }
    (lines.len(), true)
}
