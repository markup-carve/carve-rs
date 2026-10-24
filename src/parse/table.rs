use super::*;

pub(in crate::parse) fn is_table_start(line: &str) -> bool {
    // A standard table row opens AND closes with `|` (grammar standard_row; a
    // `|=` cell is a header cell). A stray leading `|` with no closing `|`
    // (`| a`) is ordinary paragraph text, not a table. (`+` multi-line-cell
    // continuations are consumed inside parse_table; a `+` line never starts a
    // table, #80.)
    //
    // A row may also carry a `{...}` attribute block glued to its closing pipe
    // (`| a |{.x}` -> <tr class="x">); split_row_attrs validates it, so a line
    // ending in a valid row-attribute block also opens a table.
    let trimmed = trim_ascii(line);
    if trimmed.len() < 2 || !trimmed.starts_with('|') {
        return false;
    }
    // CARVE-P9-082: a cell slot is counted, not weighed. `||` and `| |` are one
    // empty cell; only a lone `|` has no slot.
    let (_, body) = split_row_attrs(trimmed);
    body.len() >= 2 && body.ends_with('|')
}

/// A `{...}` attribute block GLUED to the row's closing `|` sets the row's
/// `<tr>` attributes -- the row-level twin of a cell's opening-pipe block. The
/// whole payload must be a valid attribute block running to end of line;
/// otherwise the `{` is ordinary content. Returns the parsed attributes and the
/// line body up to and including the closing pipe (with the block removed).
pub(in crate::parse) fn split_row_attrs(content: &str) -> (Option<Attrs>, &str) {
    if let Some(idx) = content.rfind('|') {
        let bytes = content.as_bytes();
        if bytes.get(idx + 1) == Some(&b'{') {
            let last_close_brace = bytes.iter().rposition(|&b| b == b'}');
            if let Some((attrs, next)) = read_attrs_at(bytes, idx + 1, last_close_brace) {
                if next == content.len() {
                    return (Some(attrs), &content[..=idx]);
                }
            }
        }
    }
    (None, content)
}

/// A cell padding slot is a run of U+0020 (spec PART 7). A tab there is not
/// padding, so it stays as cell content.
pub(in crate::parse) fn trim_cell_padding(s: &str) -> &str {
    s.trim_matches(' ')
}

/// A cell padding slot is a run of U+0020 (spec PART 7). A tab there is not
/// padding, so it stays as cell content.
pub(in crate::parse) fn trim_cell_padding_start(s: &str) -> &str {
    s.trim_start_matches(' ')
}

pub(in crate::parse) fn parse_table(cur: &mut LineCursor, options: &Options<'_>) -> BlockNode {
    let span_start = cur.pos;
    let mut rows: Vec<TableRow> = Vec::new();
    let mut row_inputs: Vec<Vec<CellInput>> = Vec::new();
    let mut first_is_delim = false;
    let mut saw_separator = false;
    let mut separator_widths: Option<Vec<u64>> = None;
    while let Some(line) = cur.peek() {
        // ONLY a `|` row opens an iteration. A `+` continuation is taken by the
        // row it continues, below, so reaching one here means the row above did
        // not want it - after a delimiter row, where a continuation has only a
        // HEADER to attach to and the table ends instead.
        if line.starts_with([' ', '\t']) || !is_table_start(line) {
            break;
        }
        let row_at = cur.pos;
        cur.consume();
        if rows.is_empty() {
            first_is_delim = is_delim_row(line);
        } else if rows.len() == 1 && !saw_separator && !first_is_delim && is_delim_row(line) {
            // The separator row: make the first row the header, drop the row.
            saw_separator = true;
            separator_widths = Some(parse_delim_widths(line));
            let column_aligns = parse_delim_aligns(line);
            for cell in &mut rows[0].cells {
                cell.header = true;
            }
            apply_column_aligns(&mut rows[0], &column_aligns);
            continue;
        }
        // THE CONTINUATIONS ARE TAKEN WITH THE ROW, not applied to a finished
        // one. A `+` row extends a CELL, and the cell is the block an unclosed
        // inline run reaches the end of (carve#1293), so the cell's content
        // must be assembled before it is parsed - which means the row parser
        // has to see every line the row runs to at once.
        let mut conts: Vec<(&str, Option<(usize, isize)>)> = Vec::new();
        let mut last_cont_at = None;
        while let Some(next) = cur.peek() {
            if next.starts_with([' ', '\t']) || !is_table_continuation(next) {
                break;
            }
            let cont_at = cur.pos;
            cur.consume();
            conts.push((
                next,
                cur.source_line(cont_at)
                    .zip(cur.source_col(cont_at))
                    .filter(|_| options.positions),
            ));
            last_cont_at = Some(cont_at);
        }
        let (mut row, inputs) = parse_table_row(
            line,
            &conts,
            options,
            options
                .positions
                .then(|| (cur.source_line(row_at), cur.source_col(row_at)))
                .and_then(|(l, c)| Some((l?, c?))),
        );
        // The cursor is the only place that knows where these lines sit, which
        // is why `parse_table_row` cannot place the row itself.
        row.pos = span_of(cur, row_at, row_at + 1, options);
        // The row RUNS to its last continuation. It stays one contiguous range
        // that no sibling row overlaps, so it keeps a position - unlike the
        // cell a continuation extends, whose content sits in two column ranges
        // with another column's content between them.
        if let (Some(pos), Some(end)) = (
            row.pos.as_mut(),
            last_cont_at.and_then(|at| span_of(cur, at, at + 1, options)),
        ) {
            pos.end_line = end.end_line;
            pos.end_column = end.end_column;
            pos.end_offset = end.end_offset;
        }
        rows.push(row);
        row_inputs.push(inputs);
    }
    join_marker_continuations(&mut rows, &mut row_inputs, options);
    // The caption FIRST, then the span: a caption is one of the table's
    // children and sits after the last row, so a span taken before it is
    // consumed stops short and leaves the caption's inlines outside their own
    // parent. Struct-field order made that the default, and nothing could see
    // it - a span is compared against source text for `text` nodes alone
    // (carve#565).
    let caption = consume_caption(cur, options);
    // The `if caption.is_some()` that stood here returned `BlockNode::Table`
    // and fell through to `BlockNode::Table`: two arms, one behavior, no way to
    // fail. Removed rather than corrected, since the two are now genuinely the
    // same.
    let mut table = Table {
        source_format: None,
        pos: span_of(cur, span_start, cur.pos, options),
        attrs: None,
        caption,
        short_caption: None,
        columns: Vec::new(),
        rows,
        row_groups: None,
        separator_widths,
    };
    // PART 12 §26: the resolved extent is published beside the markers, so it is
    // filled in here rather than recomputed by each consumer of the tree.
    crate::table_spans::resolve_table_spans(&mut table);
    BlockNode::Table(table)
}

/// The table a line-based scan is currently inside, mirrored from the row loop
/// in [`parse_table`].
///
/// PART 5 T6 gives a continuation row `table_cell`s and joins them onto the row
/// above, so it is as much a part of the table as the row it appends to, and
/// PART 1 S4 asks what a container's last BLOCK is rather than how its last line
/// is written. A scan that reads one line at a time cannot answer that without
/// carrying the run: `+ b |` is a ROW under a table and ORDINARY PROSE under
/// anything else (markup-carve/carve#1345), and the rule has to give both
/// answers rather than one of them everywhere.
///
/// The row loop is mirrored rather than approximated because the two disagree
/// in a place that is easy to miss: a continuation directly after the GFM
/// DELIMITER row is not taken, since the separator `continue`s past the
/// continuation loop and the table ends there. Answering "row" for it is
/// markup-carve/carve#1354 in this engine's own code.
#[derive(Default)]
pub(in crate::parse) struct TableRun {
    /// Rows consumed so far, the delimiter row excluded exactly as the row loop
    /// excludes it. `None` when no table is open.
    rows: Option<usize>,
    first_is_delim: bool,
    saw_separator: bool,
    /// Whether the last line consumed was a row that goes on to take
    /// continuations. False right after the separator row.
    takes_continuations: bool,
    /// How many quote markers the run's own lines sit behind, inside the
    /// container being read. A run does not survive a change of depth.
    depth: usize,
}

impl TableRun {
    /// Feed the next line of the container's own content, already stripped of
    /// the container prefix. Answers whether the line is a CONTINUATION ROW of
    /// the table above it - the one shape whose meaning the line alone does not
    /// carry.
    ///
    /// Indented lines are not rows at all: `is_table_start` trims, so without
    /// the flush-left guard an indented `  |a|` would open a table where an
    /// indented `> q` or `# h` opens nothing (PART 9 section 24 C3). The same
    /// guard is on the continuation, so the two halves of one table cannot be
    /// read at two different columns.
    pub(in crate::parse) fn observe(&mut self, line: &str) -> bool {
        if !line.as_bytes().contains(&b'|') {
            *self = TableRun::default();
            return false;
        }
        let mut innermost = line;
        let mut depth = 0usize;
        while let Some(rest) = strip_blockquote_prefix(innermost) {
            depth += 1;
            innermost = rest;
        }
        if self.rows.is_some() && depth != self.depth {
            *self = TableRun::default();
        }
        self.depth = depth;
        let line = innermost;
        let flush = !line.starts_with([' ', '\t']);
        if flush && is_table_start(line) {
            let rows = self.rows.unwrap_or(0);
            if rows == 0 {
                self.first_is_delim = is_delim_row(line);
                self.rows = Some(1);
                self.takes_continuations = true;
            } else if rows == 1 && !self.saw_separator && !self.first_is_delim && is_delim_row(line)
            {
                self.saw_separator = true;
                self.takes_continuations = false;
            } else {
                self.rows = Some(rows + 1);
                self.takes_continuations = true;
            }
            return false;
        }
        if self.takes_continuations && flush && is_table_continuation(line) {
            return true;
        }
        let depth = self.depth;
        *self = TableRun::default();
        self.depth = depth;
        false
    }
}

pub(in crate::parse) fn is_table_continuation(line: &str) -> bool {
    // `continuation_row` ends in `'|'` just like `standard_row`, so the closing
    // pipe is required here too: `+ c | d` is prose and ends the table. Unlike a
    // standard row it has no `row_attributes` slot, so a trailing `|{.x}` does
    // NOT stand in for the closing pipe.
    let trimmed = trim_ascii(line);
    if trimmed.len() < 2 || !trimmed.starts_with('+') || !trimmed.ends_with('|') {
        return false;
    }
    trimmed != "+|"
}

/// A GFM delimiter cell: an optional leading colon, one or more dashes, an
/// optional trailing colon, and nothing else.
pub(in crate::parse) fn is_delim_cell(s: &str) -> bool {
    let b = s.as_bytes();
    let mut i = 0;
    if i < b.len() && b[i] == b':' {
        i += 1;
    }
    let dash_start = i;
    while i < b.len() && b[i] == b'-' {
        i += 1;
    }
    if i == dash_start {
        return false; // need at least one dash
    }
    if i < b.len() && b[i] == b':' {
        i += 1;
    }
    i == b.len()
}

/// Strip a row's CLOSING pipe, unless that pipe is escaped.
///
/// An escaped closing pipe is still an escape (markup-carve/carve#1293). The
/// row closes there either way, because the line ends in a pipe; what the
/// escape decides is what the CELL holds, which is a literal pipe and not an
/// orphaned backslash. Leaving the `\|` pair in the content is what says that:
/// the splitter already reads the escape at every other position and keeps the
/// pair, so the cell ends up holding one pipe and the row is not split there.
///
/// Cutting the pipe off blindly left a trailing `\` behind, which the inline
/// parser then read as a hard break - `| a b \|` published a `<br>` where the
/// author wrote a pipe.
///
/// A pipe preceded by an EVEN number of backslashes is not escaped: the
/// backslashes escape each other, so the pipe is the plain closer.
pub(in crate::parse) fn strip_row_closing_pipe(content: &str) -> &str {
    let Some(stripped) = content.strip_suffix('|') else {
        return content;
    };
    let backslashes = stripped.len() - stripped.trim_end_matches('\\').len();
    if backslashes % 2 == 1 {
        return content;
    }
    stripped
}

/// A delimiter row: every cell is a delimiter cell (and there is at least one).
pub(in crate::parse) fn is_delim_row(line: &str) -> bool {
    let mut content = trim_ascii(line);
    content = content.strip_prefix('|').unwrap_or(content);
    content = content.strip_suffix('|').unwrap_or(content);
    let cells = split_table_cells(content);
    // PART 7: cell padding is U+0020 only, so a tab makes the cell content and
    // the cell stops being a delimiter cell -- which unmakes the whole row.
    !cells.is_empty() && cells.iter().all(|c| is_delim_cell(trim_cell_padding(c)))
}

/// Per-cell dash counts from a delimiter row, alignment colons excluded
/// (PART 12 §3). The row is already known to be a delimiter row, so every cell
/// has at least one dash and every count is at least 1.
pub(in crate::parse) fn parse_delim_widths(line: &str) -> Vec<u64> {
    let mut content = trim_ascii(line);
    content = content.strip_prefix('|').unwrap_or(content);
    content = content.strip_suffix('|').unwrap_or(content);
    split_table_cells(content)
        .iter()
        .map(|c| trim_cell_padding(c).bytes().filter(|b| *b == b'-').count() as u64)
        .collect()
}

/// Per-column alignment from a delimiter row's colons.
pub(in crate::parse) fn parse_delim_aligns(line: &str) -> Vec<Option<TableAlign>> {
    let mut content = trim_ascii(line);
    content = content.strip_prefix('|').unwrap_or(content);
    content = content.strip_suffix('|').unwrap_or(content);
    split_table_cells(content)
        .iter()
        .map(|c| {
            // PART 7: cell padding is U+0020 only. UNREACHABLE-BY-CONSTRUCTION
            // today -- this runs only behind `is_delim_row`, whose own
            // space-only trim already rejected any row with a tab in a cell's
            // padding, so no tab can arrive here. It is spelled the same way
            // anyway rather than left as `.trim()`: one rule spelled two ways in
            // two functions is how this family drifts (markup-carve/carve#755),
            // and if the gate above is ever restructured this site is already
            // right. Reverting it alone is a GREEN mutation, recorded as such
            // rather than pinned by a test that could not fail.
            let t = trim_cell_padding(c);
            match (t.starts_with(':'), t.ends_with(':')) {
                (true, true) => Some(TableAlign::Center),
                (false, true) => Some(TableAlign::Right),
                (true, false) => Some(TableAlign::Left),
                (false, false) => None,
            }
        })
        .collect()
}

/// Apply a column default alignment to each cell that has no alignment of its
/// own (a native `|<` marker wins over the column default).
pub(in crate::parse) fn apply_column_aligns(row: &mut TableRow, aligns: &[Option<TableAlign>]) {
    for (i, cell) in row.cells.iter_mut().enumerate() {
        if cell.align.is_none() {
            if let Some(a) = aligns.get(i).copied().flatten() {
                cell.align = Some(a);
            }
        }
    }
}

/// A cell's content contributed by ONE `+` continuation row, with the anchor
/// its own source line gives it.
pub(in crate::parse) struct CellFragment {
    text: String,
    anchor: Option<(usize, isize)>,
    marker_pos: Option<Pos>,
    separator_pos: Option<Pos>,
    /// The continuation line of its row it came from, counted from 1.
    line: usize,
    /// Rows down from the receiving cell to that row (CARVE-P12-069).
    row: usize,
    col: usize,
}

thread_local! {
    /// The joiner offsets of the table cell being parsed, each with the
    /// `cell_wrap` it becomes (CARVE-P12-069).
    pub(in crate::parse) static CELL_WRAPS: RefCell<Vec<(usize, CellWrap)>> = const { RefCell::new(Vec::new()) };
}

/// The `+` joins inside the verbatim payload of the span whose backtick run
/// starts at `tick`, as codepoint offsets into `value` (CARVE-P12-069). A join
/// the padding strip consumed has no space left to name and is dropped.
pub(in crate::parse) fn verbatim_cell_wraps(
    bytes: &[u8],
    base: usize,
    tick: usize,
    value: &str,
) -> Option<Vec<VerbatimCellWrap>> {
    let run = bytes[tick..].iter().take_while(|&&b| b == b'`').count();
    let region = &bytes[tick + run..];
    let start = if region.starts_with(value.as_bytes()) && !region.starts_with(b" ") {
        tick + run
    } else if region
        .get(1..)
        .is_some_and(|rest| rest.starts_with(value.as_bytes()))
    {
        tick + run + 1
    } else if region.starts_with(value.as_bytes()) {
        tick + run
    } else {
        return None;
    };
    let mut wraps = Vec::new();
    let padded = start > tick + run;
    // A join the padding strip consumed sits at the boundary (carve#3138).
    if padded {
        if let Some(wrap) = cell_wrap_at(base + tick + run) {
            wraps.push(VerbatimCellWrap {
                offset: 0,
                line: wrap.line,
                row: wrap.row,
                col: wrap.col,
            });
        }
    }
    for (offset, (at, ch)) in value.char_indices().enumerate() {
        if ch != ' ' {
            continue;
        }
        if let Some(wrap) = cell_wrap_at(base + start + at) {
            wraps.push(VerbatimCellWrap {
                offset,
                line: wrap.line,
                row: wrap.row,
                col: wrap.col,
            });
        }
    }
    if padded && bytes.get(start + value.len()) == Some(&b' ') {
        if let Some(wrap) = cell_wrap_at(base + start + value.len()) {
            wraps.push(VerbatimCellWrap {
                offset: value.chars().count(),
                line: wrap.line,
                row: wrap.row,
                col: wrap.col,
            });
        }
    }
    (!wraps.is_empty()).then_some(wraps)
}

pub(in crate::parse) fn cell_wrap_at(offset: usize) -> Option<CellWrap> {
    CELL_WRAPS.with(|wraps| {
        let wraps = wraps.borrow();
        if wraps.is_empty() {
            return None;
        }
        // Ascending by construction: each join is pushed at the text's growing length.
        wraps
            .binary_search_by_key(&offset, |(at, _)| *at)
            .ok()
            .map(|index| wraps[index].1.clone())
    })
}

/// What a cell was parsed from, kept so a later continuation can rejoin it.
pub(in crate::parse) struct CellInput {
    text: String,
    anchor: Option<(usize, isize)>,
    extra: Vec<CellFragment>,
    alone_pos: Option<Pos>,
}

pub(in crate::parse) fn reparse_table_cell(
    cell: &mut TableCell,
    input: &CellInput,
    options: &Options<'_>,
) {
    let fresh = parse_table_cell(&input.text, options, input.anchor, &input.extra);
    cell.children = fresh.children;
    cell.blocks = fresh.blocks;
    cell.pos = if input.extra.is_empty() {
        input.alone_pos.clone()
    } else {
        None
    };
}

/// T6 under a span: a continuation fragment on a `^` or `<` marker joins the
/// cell the T5 walk resolves that marker into, in row-major order. A blocked
/// marker is a cell of its own and keeps its fragments (carve#3151).
pub(in crate::parse) fn join_marker_continuations(
    rows: &mut [TableRow],
    row_inputs: &mut [Vec<CellInput>],
    options: &Options<'_>,
) {
    let mut dirty: Vec<(usize, usize)> = Vec::new();
    for (r, row) in rows.iter().enumerate() {
        for (c, cell) in row.cells.iter().enumerate() {
            if cell.span.is_some() && !row_inputs[r][c].extra.is_empty() {
                dirty.push((r, c));
            }
        }
    }
    if dirty.is_empty() {
        return;
    }
    let widths: Vec<usize> = rows.iter().map(|row| row.cells.len()).collect();
    let mut absorbed: Vec<((usize, usize), (usize, usize))> = Vec::new();
    crate::table_spans::resolve_span_grid_absorbing(
        &widths,
        |r, c| rows[r].cells[c].span,
        |origin, marker| absorbed.push((origin, marker)),
    );
    for (origin, marker) in absorbed {
        let mut moved = std::mem::take(&mut row_inputs[marker.0][marker.1].extra);
        if moved.is_empty() {
            continue;
        }
        for fragment in &mut moved {
            fragment.row += marker.0 - origin.0;
            fragment.col += marker.1 - origin.1;
            // A row below the cell's own lies outside the cell's placed span.
            if marker.0 != origin.0 {
                fragment.anchor = None;
                fragment.marker_pos = None;
                fragment.separator_pos = None;
            }
        }
        row_inputs[origin.0][origin.1].extra.extend(moved);
        if !dirty.contains(&origin) {
            dirty.push(origin);
        }
    }
    for (r, c) in dirty {
        reparse_table_cell(&mut rows[r].cells[c], &row_inputs[r][c], options);
    }
}

/// `base` is the row line's (source line, columns already stripped by an
/// enclosing container). Without it a cell cannot be placed: this function is
/// handed an already-split line and cannot know where it sits.
///
/// `conts` are the `+` CONTINUATION lines that belong to this row, each with
/// the same pair for its own line. They are taken here rather than applied
/// afterwards because a continuation extends a CELL, and the cell is the block
/// an unclosed inline run reaches the end of (carve#1293) - so the cell's
/// content has to be assembled BEFORE it is parsed, not parsed twice and
/// concatenated. Parsing each fragment on its own closed the run at the row's
/// pipe and opened a fresh one for the continuation, which published an empty
/// `<code></code>` that no clause in the language produces.
pub(in crate::parse) fn parse_table_row(
    line: &str,
    conts: &[(&str, Option<(usize, isize)>)],
    options: &Options<'_>,
    base: Option<(usize, isize)>,
) -> (TableRow, Vec<CellInput>) {
    let mut content = trim_ascii(line);
    let (attrs, body) = split_row_attrs(content);
    content = body;
    if let Some(stripped) = content.strip_prefix('|') {
        content = stripped;
    }
    content = strip_row_closing_pipe(content);
    // Where `content` starts inside `line`, in CHARS: it is a slice of `line`
    // after trimming, the row-attribute split and the outer pipes, so the byte
    // distance between them is exact.
    let content_off = base.map(|_| char_offset_of(line, content));
    let split = split_table_cells_ranged(content);
    let mut fragments: Vec<Vec<CellFragment>> =
        (0..split.cells.len()).map(|_| Vec::new()).collect();
    collect_continuation_fragments(conts, &split, &mut fragments);
    let mut inputs = Vec::with_capacity(split.cells.len());
    let cells = split
        .cells
        .into_iter()
        .zip(fragments)
        .map(|(slice, extra)| {
            // The cell's own start column, which is also the anchor its inline
            // content is parsed against: the cell text is a verbatim slice of
            // the row now that the escaped pipe is preserved, so an offset
            // inside it maps straight back to the document.
            let cell_anchor = match (base, content_off) {
                (Some((line_no, stripped)), Some(off)) => {
                    Some((line_no, stripped + (off + slice.start) as isize))
                }
                _ => None,
            };
            let mut cell = parse_table_cell(&slice.text, options, cell_anchor, &extra);
            let alone_pos = match (base, content_off) {
                (Some((line_no, stripped)), Some(off)) => Some(Pos {
                    start_line: line_no,
                    end_line: line_no,
                    start_column: document_column(stripped, off + slice.start),
                    end_column: document_column(stripped, off + slice.end),
                    // Filled from the line table once the document is parsed.
                    start_offset: 0,
                    end_offset: 0,
                    file: None,
                }),
                _ => None,
            };
            // A cell reassembled from discontiguous fragments has no single
            // exact source extent.
            cell.pos = if extra.is_empty() {
                alone_pos.clone()
            } else {
                None
            };
            inputs.push(CellInput {
                text: slice.text.to_string(),
                anchor: cell_anchor,
                extra,
                alone_pos,
            });
            cell
        })
        .collect();
    (
        TableRow {
            cells,
            attrs,
            continuation_lines: (!conts.is_empty()).then_some(conts.len()),
            pos: None,
        },
        inputs,
    )
}

/// Where `part` starts inside `whole`, in CHARS.
///
/// `part` must be a SUB-SLICE of `whole` - it always is at the call sites, which
/// only ever narrow - so the byte distance between the two pointers is exact and
/// converts without re-scanning.
pub(in crate::parse) fn char_offset_of(whole: &str, part: &str) -> usize {
    let bytes = (part.as_ptr() as usize).saturating_sub(whole.as_ptr() as usize);
    whole[..bytes].chars().count()
}

/// Cut each continuation line into cells and file every non-empty one under the
/// column it extends.
///
/// The verbatim run a line leaves OPEN is carried into the next one: a `+` row
/// continues the cell, so a run the row's closing pipe did not close is still
/// open where the continuation picks that cell up (carve#1293). An EMPTY cell
/// contributes nothing - not even the joining space - which is what lets a
/// continuation address one column of a wide row.
pub(in crate::parse) fn collect_continuation_fragments(
    conts: &[(&str, Option<(usize, isize)>)],
    row: &RowSplit,
    fragments: &mut [Vec<CellFragment>],
) {
    // The run's WIDTH travels with it, not just the fact that one is open: a
    // run closes on a run of exactly its own length, on the continuation row as
    // much as on the row that opened it (carve-rs#1051).
    let mut open_len = row.open_len;
    let mut open_run_at = row.open_run_at;
    for (line_index, (line, base)) in conts.iter().enumerate() {
        let marker_pos = base.and_then(|(line_no, stripped)| {
            line.find('+').map(|at| Pos {
                start_line: line_no,
                end_line: line_no,
                start_column: document_column(stripped, line[..at].chars().count()),
                end_column: document_column(stripped, line[..at].chars().count() + 1),
                start_offset: 0,
                end_offset: 0,
                file: None,
            })
        });
        let mut content = trim_ascii(line);
        if let Some(stripped) = content.strip_prefix('+') {
            content = stripped;
        }
        // The same rule as the row's own closer above: the escape is honored
        // wherever it appears, and a continuation row's last pipe is not an
        // exception (markup-carve/carve#1293).
        content = strip_row_closing_pipe(content);
        let content_off = base.map(|_| char_offset_of(line, content));
        let split = split_table_cells_seeded(content, open_len, open_run_at);
        open_len = split.open_len;
        open_run_at = split.open_run_at;
        for (idx, cell) in split.cells.iter().enumerate() {
            let text = trim_cell_padding(&cell.text); // PART 7: cell padding is U+0020 only.
            if text.is_empty() {
                continue;
            }
            // Trimming moved the start; count what it took so the anchor lands
            // on the first character of the text and not on the padding before
            // it.
            let lead =
                cell.text.chars().count() - trim_cell_padding_start(&cell.text).chars().count(); // PART 7: cell padding is U+0020 only.
            let anchor = match (base, content_off) {
                (Some((line_no, stripped)), Some(off)) => {
                    Some((*line_no, stripped + (off + cell.start + lead) as isize))
                }
                _ => None,
            };
            if let Some(slot) = fragments.get_mut(idx) {
                let separator_pos = if idx > 0 {
                    base.zip(content_off).map(|((line_no, stripped), off)| Pos {
                        start_line: line_no,
                        end_line: line_no,
                        start_column: document_column(stripped, off + cell.start - 1),
                        end_column: document_column(stripped, off + cell.start),
                        start_offset: 0,
                        end_offset: 0,
                        file: None,
                    })
                } else {
                    None
                };
                slot.push(CellFragment {
                    text: text.to_string(),
                    anchor,
                    marker_pos: marker_pos.clone(),
                    separator_pos,
                    line: line_index + 1,
                    row: 0,
                    col: 0,
                });
            }
        }
    }
}

/// A cell as it sits in the row: its resolved text, and the CHAR range of the
/// source it came from.
///
/// The range is not derivable from the text: `\|` resolves to one character, so
/// a cell holding an escaped pipe is shorter than the source it spans.
pub(in crate::parse) struct CellSlice {
    text: String,
    pub(in crate::parse) start: usize,
    pub(in crate::parse) end: usize,
}

pub(in crate::parse) fn split_table_cells(content: &str) -> Vec<String> {
    split_table_cells_ranged(content)
        .cells
        .into_iter()
        .map(|c| c.text)
        .collect()
}

/// A row line cut into cells, together with the verbatim run its closing pipe
/// did NOT close.
///
/// The leftover run is what a `+` continuation row is scanned with. A row's
/// closing pipe closes the row even with a run still open (carve#1284), so the
/// run survives the row boundary - and by construction it sits in the LAST
/// cell, since once open it swallows every `|` but the closer.
pub(in crate::parse) struct RowSplit {
    pub(in crate::parse) cells: Vec<CellSlice>,
    /// The WIDTH of the verbatim run still open at end of line, or `None` when
    /// none is.
    ///
    /// THE WIDTH, not a flag. A run closes on a run of EXACTLY its own length
    /// (PART 9 §22), which is the rule this scanner already applies WITHIN a
    /// line - and it used to carry only "a run is open" across the row
    /// boundary, re-seeding the continuation at one backtick. So a run opened
    /// with two was closed by a single one on the continuation row, the pipe
    /// behind it split again, and the segment after it had no column to join:
    /// content loss, by a different route than the fresh scanner the per-column
    /// carry replaced (carve-rs#1051).
    open_len: Option<usize>,
    /// The cell index that run is open in: the last one.
    open_run_at: usize,
}

pub(in crate::parse) fn split_table_cells_ranged(content: &str) -> RowSplit {
    split_table_cells_seeded(content, None, 0)
}

/// `seed_len` / `open_run_at` seed the scanner with a verbatim run left OPEN by
/// the row this line CONTINUES (carve#1293), and with the WIDTH that run was
/// opened at.
pub(in crate::parse) fn split_table_cells_seeded(
    content: &str,
    seed_len: Option<usize>,
    open_run_at: usize,
) -> RowSplit {
    if seed_len.is_none()
        && content.is_ascii()
        && !content
            .as_bytes()
            .iter()
            .any(|&byte| byte == b'\\' || byte == b'`')
    {
        let mut cells = Vec::new();
        let mut start = 0;
        for (at, _) in content.match_indices('|') {
            cells.push(CellSlice {
                text: content[start..at].to_string(),
                start,
                end: at,
            });
            start = at + 1;
        }
        cells.push(CellSlice {
            text: content[start..].to_string(),
            start,
            end: content.len(),
        });
        return RowSplit {
            open_run_at: cells.len() - 1,
            cells,
            open_len: None,
        };
    }
    let mut cells = Vec::new();
    let mut buf = String::new();
    let mut open_len: Option<usize> = if open_run_at == 0 { seed_len } else { None };
    let mut index = 0usize;
    let mut cell_start = 0usize;
    let mut chars = content.chars().peekable();
    while let Some(ch) = chars.next() {
        index += 1;
        if open_len.is_none()
            && ch == '\\'
            && chars.peek().is_some_and(|next| next.is_ascii_punctuation())
        {
            buf.push(ch);
            buf.push(chars.next().unwrap());
            index += 1;
            continue;
        }
        if ch == '`' {
            let mut run = 1usize;
            buf.push(ch);
            while chars.peek() == Some(&'`') {
                chars.next();
                index += 1;
                run += 1;
                buf.push('`');
            }
            match open_len {
                None => open_len = Some(run),
                Some(width) if width == run => open_len = None,
                Some(_) => {}
            }
            continue;
        }
        if ch == '\\' {
            // An escaped PIPE does not split the row, but the escape is KEPT
            // rather than resolved here: the inline parser turns it into an
            // `escaped_text` node, which is what carve-js publishes and what
            // the vocabulary defines. Resolving it here produced a single
            // `text` node holding a bare `|`, losing both the node and the
            // author's intent - and, because the cell text was then no longer
            // a verbatim slice of the row, nothing inside the cell could carry
            // a position either (carve-rs#333).
            //
            // Every other backslash escape was already preserved for the same
            // reason; this only makes the pipe consistent with them.
            if chars.peek() == Some(&'|') {
                buf.push('\\');
                buf.push('|');
                chars.next();
                index += 1;
            } else {
                buf.push('\\');
            }
            continue;
        }
        if ch == '|' && open_len.is_none() {
            cells.push(CellSlice {
                text: std::mem::take(&mut buf),
                start: cell_start,
                // The separator is not part of the cell.
                end: index - 1,
            });
            cell_start = index;
            if cells.len() == open_run_at {
                // Re-seed at the column the row above left open, AT THE WIDTH
                // that row opened it. `None` here is not a seed and leaves the
                // scanner outside a run, which is the same thing it was doing
                // for every other column.
                open_len = seed_len;
            }
            continue;
        }
        buf.push(ch);
    }
    cells.push(CellSlice {
        text: buf,
        start: cell_start,
        end: index,
    });
    let open_run_at = cells.len() - 1;
    RowSplit {
        cells,
        open_len,
        open_run_at,
    }
}

/// Parse a cell's inline content, anchored when the caller knows where the
/// cell sits in the document.
///
/// `slice` must be a SUB-SLICE of `cell` (it always is here: trimming and
/// marker removal only ever narrow it), so the byte distance between the two
/// pointers is exact and converts to a char offset without re-scanning.
pub(in crate::parse) fn parse_cell_inlines(
    cell: &str,
    slice: &str,
    options: &Options<'_>,
    anchor: Option<(usize, isize)>,
    extra: &[CellFragment],
) -> Vec<InlineNode> {
    let base = anchor
        .map(|(line_no, base_col)| (line_no, base_col + char_offset_of(cell, slice) as isize));
    parse_cell_content(slice, options, base, extra)
}

/// The cell's content as ONE inline block, its `+` continuation fragments
/// included.
///
/// THE CELL IS THE BLOCK (carve#1293). An unclosed inline verbatim run runs to
/// the end of the block it is in, and a `+` continuation extends the cell, so
/// the run has to be able to close on the continuation - which it cannot do if
/// each fragment is parsed by itself and the results concatenated. That is what
/// published `a <code>b</code> c<code></code>`: the run closed at the row's
/// pipe and a fresh one opened for the continuation, leaving an empty span no
/// clause in this language produces.
///
/// The JOINER is a manufactured space, because the source has a line break here
/// and the cell does not. It belongs to no source line, so it gets an anchor
/// segment of its own carrying NO anchor: a node whose span crosses it can then
/// place neither end and carries no position at all, which is the rule already
/// in force for a run joined across a gap (PART 12 section 4, absent beats
/// wrong). A node that lies wholly inside one fragment keeps its own position.
pub(in crate::parse) fn parse_cell_content(
    slice: &str,
    options: &Options<'_>,
    base: Option<(usize, isize)>,
    extra: &[CellFragment],
) -> Vec<InlineNode> {
    if extra.is_empty() {
        let Some(anchor) = base else {
            return parse_inline_with_options(slice, options);
        };
        return parse_inline_lines_with_anchor(slice, options, vec![Some(anchor)]);
    }
    let mut text = String::from(slice);
    let mut lines = vec![base];
    let mut breaks = Vec::new();
    let mut wraps = Vec::new();
    let mut previous_line = None;
    for fragment in extra {
        if !text.is_empty() || fragment.line > 1 {
            let marker_pos = if previous_line == Some(fragment.line) {
                fragment.separator_pos.clone()
            } else {
                fragment.marker_pos.clone()
            };
            wraps.push((
                text.len(),
                CellWrap {
                    line: fragment.line,
                    row: (fragment.row > 0).then_some(fragment.row),
                    col: (fragment.col > 0).then_some(fragment.col),
                    pos: marker_pos,
                },
            ));
            // The joiner is the LAST character of the segment before it, which
            // is where a newline sits in an ordinary multi-line text. Opening
            // the next segment on it instead puts the exclusive end of a span
            // that stops just short of it into the following segment, and every
            // node at the end of a fragment loses its position.
            if !text.is_empty() {
                text.push(' ');
            }
        }
        breaks.push(text.len());
        text.push_str(&fragment.text);
        lines.push(fragment.anchor);
        previous_line = Some(fragment.line);
    }
    let leading_wrap = wraps
        .first()
        .filter(|(offset, _)| *offset == 0)
        .map(|(_, wrap)| wrap.clone());
    let outer = CELL_WRAPS.with(|cell| cell.replace(wraps));
    let mut parsed = parse_inline_segments_with_anchor(&text, options, lines, breaks);
    CELL_WRAPS.with(|cell| cell.replace(outer));
    if let Some(wrap) = leading_wrap {
        parsed.insert(0, InlineNode::CellWrap(wrap));
    }
    parsed
}

pub(in crate::parse) fn parse_table_cell(
    cell: &str,
    options: &Options<'_>,
    anchor: Option<(usize, isize)>,
    extra: &[CellFragment],
) -> TableCell {
    // A leading `=` marks a HEADER cell, but only when GLUED to the `|` (no
    // leading whitespace), per grammar §20. `| =x |` (space before `=`) is a
    // literal `<td>`, matching carve-js / carve-php; check the RAW cell, not
    // the trimmed one.
    let mut header = cell.starts_with('=');
    // NOT trimmed: the alignment marker is GLUED to the opening `|` (grammar
    // §20, `data_cell` / `header_cell`), so whether whitespace precedes it is
    // the whole distinction. Reading the marker off the TRIMMED cell threw that
    // away and made `| < x |` left-aligned where carve-js and carve-php both
    // render a literal `<`, and made `| << |` a colspan by stripping the first
    // `<` as alignment and matching the second as a lone span marker
    // (carve-rs#459).
    let body = if header { &cell[1..] } else { cell };
    let trimmed = trim_cell_padding(body); // PART 7: cell padding is U+0020 only.
                                           // A whitespace-delimited lone marker is a span cell rather than alignment,
                                           // which is the one case a glued marker does not win: `|<|` is a colspan in
                                           // every engine. A header cell is already marked by its `=`, so a marker
                                           // glued after it is alignment even when it is the whole content (`|=<|`).
                                           // A lone span glyph wins over an empty alignment run in every padding form.
    let lone_span = !header && matches!(trimmed, "^" | "<");
    let mut after_markers = body;
    let (mut align, mut valign) = if lone_span {
        (None, None)
    } else {
        let run = body
            .bytes()
            .take_while(|marker| matches!(marker, b'>' | b'<' | b'~' | b'^' | b'v' | b'?'))
            .count();
        let inherited_horizontal = run == 2
            && body.as_bytes().first() == Some(&b'?')
            && body
                .as_bytes()
                .get(1)
                .is_some_and(|marker| matches!(marker, b'^' | b'~' | b'v'));
        let mut saw_horizontal = false;
        let mut saw_vertical = false;
        let mut axes_valid = !matches!(body.as_bytes().first(), Some(b'^' | b'v'))
            && !(body.as_bytes().first() == Some(&b'~')
                && body
                    .as_bytes()
                    .get(1)
                    .is_some_and(|marker| matches!(marker, b'>' | b'<')));
        for (index, marker) in body.bytes().take(run).enumerate() {
            if marker == b'?' {
                if inherited_horizontal && index == 0 {
                    continue;
                }
                axes_valid = false;
                break;
            } else if marker == b'~'
                && !saw_horizontal
                && !saw_vertical
                && body
                    .as_bytes()
                    .get(index + 1)
                    .is_some_and(|next| matches!(next, b'>' | b'<'))
            {
                saw_vertical = true;
            } else if matches!(marker, b'>' | b'<' | b'~') {
                if !saw_horizontal {
                    saw_horizontal = true;
                } else if marker == b'~' && !saw_vertical {
                    saw_vertical = true;
                } else {
                    axes_valid = false;
                    break;
                }
            } else if !saw_vertical {
                saw_vertical = true;
            } else {
                axes_valid = false;
                break;
            }
        }
        let markers = &body.as_bytes()[..run];
        let terminated = body
            .as_bytes()
            .get(run)
            .is_some_and(|b| *b == b' ' || *b == b'{');
        let valid = run > 0 && axes_valid && (saw_horizontal || inherited_horizontal) && terminated;
        if valid {
            after_markers = &body[run..];
        }
        let horizontal_marker = (valid && !inherited_horizontal)
            .then(|| {
                markers.iter().enumerate().find_map(|(index, marker)| {
                    let vertical_first_middle = *marker == b'~'
                        && index == 0
                        && markers
                            .get(1)
                            .is_some_and(|next| matches!(next, b'>' | b'<'));
                    (!vertical_first_middle && matches!(marker, b'>' | b'<' | b'~'))
                        .then_some(*marker)
                })
            })
            .flatten();
        let align = horizontal_marker.map(|marker| match marker {
            b'>' => TableAlign::Right,
            b'<' => TableAlign::Left,
            _ => TableAlign::Center,
        });
        let valign = if !valid {
            None
        } else if markers.contains(&b'^') {
            Some(TableVerticalAlign::Top)
        } else if markers.contains(&b'v') {
            Some(TableVerticalAlign::Bottom)
        } else if markers.len() == 2 {
            Some(TableVerticalAlign::Middle)
        } else {
            None
        };
        (align, valign)
    };
    // CELL ATTRIBUTES BIND LAST (grammar §20 T10, corpus
    // 319-cell-attributes-bind-after-the-kind-and-alignment-markers). A `{...}`
    // block sets the cell's attributes and is GLUED to whatever precedes it: to
    // the marker run where the cell has one (`|=<{.x} ...`), to the opening `|`
    // where it has none (`|{.x} ...`). One order, both productions.
    //
    // Reading it AHEAD of the markers instead left an attributed HEADER cell
    // unspellable: the only shape available, `|{#x}=R|`, is ambiguous by
    // construction and this grammar reads it as a data cell whose content
    // starts with `=`, so the canonical writer's spelling for `<th id="x">R</th>`
    // came back as `<td id="x">=R</td>` and the PART 11 round-trip invariant
    // failed on it. Once `=` has committed the cell to header, everything after
    // it is unambiguous.
    //
    // `read_attrs_at` is quote-aware and validates the whole payload, so a
    // partially-invalid or empty block reads as None and the `{` stays content.
    // A space before the brace (`| {.x}`, `|= {.x}`) is also ordinary content.
    let mut attrs = None;
    let mut rest = after_markers;
    if after_markers.as_bytes().first() == Some(&b'{') {
        let after_bytes = after_markers.as_bytes();
        let last_close_brace = after_bytes.iter().rposition(|&b| b == b'}');
        if let Some((read, next)) = read_attrs_at(after_bytes, 0, last_close_brace) {
            attrs = Some(read);
            rest = &after_markers[next..];
        }
    }
    // A MARKER RUN ENDS AT A SPACE (grammar §20 T11, corpus
    // 390-a-table-cell-s-marker-run-ends-at-a-space). The kind marker, the
    // alignment run and the attribute block are ONE run, and a cell carrying
    // any of them must follow it with one literal space. Without that space
    // there is no run and every byte of it is content, so `|=hot= |` is the
    // highlight its author wrote rather than a header cell holding `hot=`.
    //
    // The run is ATOMIC, which is why this clears `header` too: a rejected
    // alignment run takes the `=` with it, instead of leaving the "first marker
    // wins, the rest is content" reading the clause replaces. The closing pipe
    // is not a terminator (`|= |` is the empty header cell) and neither is a
    // tab, which PART 7 gives no padding role inline.
    let run_len = cell.len() - rest.len();
    if run_len > 0 && rest.as_bytes().first() != Some(&b' ') {
        header = false;
        align = None;
        valign = None;
        attrs = None;
        rest = cell;
    }
    let text = trim_cell_padding(rest); // PART 7: cell padding is U+0020 only.
                                        // `span_cell` is an ALTERNATIVE to `data_cell` in the grammar, not a suffix
                                        // of one, so a cell that already carried an alignment marker cannot also be
                                        // a span: whatever follows the marker is content. Without this, `|<<|` read
                                        // its first `<` as alignment and its second as a lone colspan marker and
                                        // emitted an empty cell (carve-rs#459). A cell carrying attributes is never
                                        // a bare span marker either -- its content is literal even if it is just
                                        // `^` or `<`.
    let span = match text {
        _ if header || align.is_some() || valign.is_some() || attrs.is_some() => None,
        "^" => Some(TableCellSpan::Rowspan),
        "<" => Some(TableCellSpan::Colspan),
        _ => None,
    };
    TableCell {
        colspan: None,
        rowspan: None,
        header,
        span,
        align,
        valign,
        attrs,
        children: if span.is_some() {
            // A span cell renders nothing of its own - it widens a neighbour -
            // so its own content is empty. A continuation still files its
            // fragments under this column, and they stay in the tree where they
            // always were.
            parse_cell_content("", options, None, extra)
        } else {
            parse_cell_inlines(cell, text, options, anchor, extra)
        },
        blocks: None,
        // The caller places the cell: it knows where the row line sits.
        pos: None,
    }
}
