use super::*;

/// Last line in a colon group rebased with its opener.
pub(in crate::parse) fn rebased_colon_group_end(
    lines: &[Cow<'_, str>],
    start: usize,
    base: usize,
    width: usize,
) -> usize {
    let mut end = start;
    // CARVE-P0-004: the closer is measured from the
    // authored base, so only the container's content column or that base
    // closes. A run in the band between them is payload, and the extent
    // must keep it rather than break and let the outer scan re-read it as
    // an opener of its own. The comment fence is the deliberate exception
    // and is left alone - both its delimiters carry a whitespace slot.
    let mut stack = vec![width];
    for (j, candidate) in lines.iter().enumerate().skip(start + 1) {
        end = j;
        let candidate = strip_lazy(candidate);
        let column = indent_columns(candidate);
        if !is_blank_line(candidate) && (column == 0 || column == base) {
            let local = strip_leading_columns(candidate, column);
            let trimmed = trim_ascii(&local);
            if trimmed.len() >= 3 && trimmed.bytes().all(|byte| byte == b':') {
                if stack.last() == Some(&trimmed.len()) {
                    stack.pop();
                    if stack.is_empty() {
                        break;
                    }
                } else {
                    stack.push(trimmed.len());
                }
            } else if let Some(open) = detect_container_open(trimmed) {
                if stack.len() < MAX_NESTING_DEPTH {
                    stack.push(open.fence_len);
                }
            }
        }
    }
    end
}

/// Last line in a list run before a dedent or a below-content-column block.
pub(in crate::parse) fn rebased_list_end(
    lines: &[Cow<'_, str>],
    start: usize,
    base: usize,
) -> usize {
    let mut end = start;
    let content = marker_content_col(&lines[start]).unwrap_or(base);
    let mut blank_seen = false;
    for (j, candidate) in lines.iter().enumerate().skip(start + 1) {
        if is_blank_line(candidate) {
            blank_seen = true;
            end = j;
            continue;
        }
        let indent = indent_columns(candidate);
        if indent < base {
            break;
        }
        // THE SECOND BOUND. Carrying a block short of the item's
        // content column into the run puts it in the LIST's coordinate
        // system one or two columns shy of that content column, where
        // the ordinary list rule reads it as lazy text - which is
        // exactly what the enclosing body's own block is not.
        //
        // `indent > base` rather than `>= base` is equivalent for known shapes:
        // widening it moves nothing, because the outer scan
        // picks a marker at the base up again as its own opener. It is
        // written this way because a line AT the marker's column is
        // where a sibling marker goes, which belongs in the run.
        if blank_seen && indent > base && indent < content {
            break;
        }
        end = j;
    }
    end
}

/// For each line, whether a fence opened above it is still open there, so the
/// line is that fence's body or closer. A nested line block's payload opens
/// nothing, so a fence-shaped line in it is skipped.
pub(in crate::parse) fn fences_open_before(lines: &[Cow<'_, str>]) -> Vec<bool> {
    let mut open: Option<(FenceOpen, usize)> = None;
    let mut line_block: Option<(usize, usize)> = None;
    let mut out = Vec::with_capacity(lines.len());
    for line in lines {
        let column = indent_columns(line);
        let local = trim_ascii_start(line);
        if let Some((width, at)) = line_block {
            if is_blank_line(line) || column >= at {
                out.push(false);
                if column == at && exact_colon_fence_len(local) == Some(width) {
                    line_block = None;
                }
                continue;
            }
            line_block = None;
        }
        match open {
            Some((fence, base)) => {
                out.push(true);
                if column == base && is_fence_close(local, fence) {
                    open = None;
                }
            }
            None => {
                out.push(false);
                line_block = detect_line_block_open(local)
                    .map(|width| (width, column))
                    .or_else(|| {
                        let marker = detect_list_marker_full_at(line, column)?;
                        let width = detect_line_block_open(marker.content)?;
                        Some((width, marker_content_col_at(line, column)?))
                    });
                if line_block.is_none() {
                    open = detect_fence_open(local).map(|fence| (fence, column));
                }
            }
        }
    }
    out
}

/// Last line owned by a rebased code fence.
pub(in crate::parse) fn rebased_code_fence_end(
    lines: &[Cow<'_, str>],
    start: usize,
    base: usize,
    open: FenceOpen,
) -> usize {
    let mut end = start;
    // CARVE-P0-004: only the container's content column and the opener's
    // own base close it, so an unterminated fence runs to the container's
    // end and a run in between stays payload at its authored column.
    for (j, candidate) in lines.iter().enumerate().skip(start + 1) {
        end = j;
        let column = indent_columns(candidate);
        if !is_blank_line(candidate)
            && (column == 0 || column == base)
            && is_fence_close(&strip_leading_columns(candidate, column), open)
        {
            break;
        }
    }
    end
}

/// Last line owned by a rebased comment fence.
pub(in crate::parse) fn rebased_comment_end(
    lines: &[Cow<'_, str>],
    start: usize,
    base: usize,
    width: usize,
) -> usize {
    let mut end = start;
    for (j, candidate) in lines.iter().enumerate().skip(start + 1) {
        if !is_blank_line(candidate) && indent_columns(candidate) < base {
            break;
        }
        end = j;
        if !is_blank_line(candidate)
            && is_comment_fence_close(&strip_leading_columns(candidate, base), width)
        {
            break;
        }
    }
    end
}

/// Last line owned by a rebased line block.
pub(in crate::parse) fn rebased_line_block_end(
    lines: &[Cow<'_, str>],
    start: usize,
    base: usize,
    width: usize,
    closers: &mut Option<HashMap<u8, Vec<usize>>>,
) -> usize {
    let mut end = start;
    let mut j = start + 1;
    while j < lines.len() {
        let candidate = &lines[j];
        end = j;
        if let Some(open) = detect_fence_open(trim_ascii_start(candidate)) {
            let index = closers.get_or_insert_with(|| {
                let views: Vec<_> = lines.iter().map(|line| line.as_ref()).collect();
                build_code_closer_last_index(&views)
            });
            if code_closer_exists_after(index, j, open.fence_char, open.fence_len) {
                if let Some(close) = (j + 1..lines.len())
                    .find(|&at| is_fence_close(trim_ascii_start(&lines[at]), open))
                {
                    end = close;
                    j = close + 1;
                    continue;
                }
            }
        }
        let column = indent_columns(candidate);
        if (column == 0 || column == base)
            && exact_colon_fence_len(&strip_leading_columns(candidate, column)) == Some(width)
        {
            break;
        }
        j += 1;
    }
    end
}

/// Last line owned by a rebased definition list.
pub(in crate::parse) fn rebased_definition_list_end(
    lines: &[Cow<'_, str>],
    start: usize,
    base: usize,
) -> usize {
    let mut end = start;
    let mut content = definition_body_content_col(&lines[start]);
    for (j, candidate) in lines.iter().enumerate().skip(start + 1) {
        if is_blank_line(candidate) {
            end = j;
            continue;
        }
        let indent = indent_columns(candidate);
        if indent < base {
            break;
        }
        if indent == base {
            // A sibling term or description marker re-opens the run's
            // innermost body, so the column to measure against moves
            // with it.
            if let Some(column) = definition_body_content_col(candidate) {
                content = Some(column);
            }
            end = j;
            continue;
        }
        if content.is_some_and(|column| indent < column)
            && item_block_opener(&strip_leading_columns(candidate, indent))
        {
            break;
        }
        end = j;
    }
    end
}

/// Last line in the footnote metadata group, retaining intervening blanks.
#[inline(always)]
pub(in crate::parse) fn rebased_footnote_end(
    lines: &[Cow<'_, str>],
    start: usize,
    base: usize,
) -> usize {
    let mut end = start;
    // The authored base belongs to the remainder of this metadata
    // group, not only to the definition opener. A later paragraph at
    // the same authored column is a sibling in the enclosing footnote
    // body; leaving its residual indent made it a continuation of the
    // nested definition after the opener itself had been rebased.
    for (j, candidate) in lines.iter().enumerate().skip(start + 1) {
        if !is_blank_line(candidate) && indent_columns(candidate) < base {
            break;
        }
        end = j;
    }
    end
}

/// Last quote prefix or lazy continuation before a blank or dedent.
#[inline(always)]
pub(in crate::parse) fn rebased_quote_end(
    lines: &[Cow<'_, str>],
    start: usize,
    base: usize,
) -> usize {
    let mut end = start;
    // Quote prefixes and their lazy paragraph continuations share the
    // opener's authored base until a blank or a dedent ends the run.
    for (j, candidate) in lines.iter().enumerate().skip(start + 1) {
        if is_blank_line(candidate) || indent_columns(candidate) < base {
            break;
        }
        end = j;
    }
    end
}

/// Last table row or continuation at the opener's authored base.
#[inline(always)]
pub(in crate::parse) fn rebased_table_end(
    lines: &[Cow<'_, str>],
    start: usize,
    base: usize,
) -> usize {
    let mut end = start;
    for (j, candidate) in lines.iter().enumerate().skip(start + 1) {
        if is_blank_line(candidate) || indent_columns(candidate) != base {
            break;
        }
        let local = strip_leading_columns(candidate, base);
        if !is_table_start(&local) && !is_table_continuation(&local) {
            break;
        }
        end = j;
    }
    end
}

#[cfg(test)]
thread_local! {
    pub(in crate::parse) static NAMED_COLON_REBASE_LINES: Cell<usize> = const { Cell::new(0) };
}

pub(in crate::parse) fn rebase_named_colon_extent(
    lines: &mut [Cow<'_, str>],
    columns: &mut [Option<isize>],
    start: usize,
    end: usize,
    base: usize,
    width: usize,
) {
    // Each payload line is rewritten once, even when named children nest.
    let mut frames = vec![(width, base, 0, false, false)];
    let mut items: Vec<usize> = Vec::new();
    let mut comment_retains_frame = false;
    let mut code: Option<(FenceOpen, usize, bool)> = None;
    let mut comment: Option<usize> = None;
    for (at, line) in lines.iter_mut().enumerate().take(end + 1).skip(start) {
        let column = indent_columns(line);
        let local = trim_ascii_start(line);
        let Some(&(frame_width, frame_base, parent_base, literal, paragraph)) = frames.last()
        else {
            break;
        };
        let mut strip = frame_base;
        let mut opaque = false;
        if let Some((open, opener_column, payload)) = code {
            if !is_blank_line(line) && column < opener_column && !payload {
                code = None;
            } else {
                if (payload || column == opener_column) && is_fence_close(local, open) {
                    code = None;
                }
                if !payload {
                    strip = opener_column;
                }
                opaque = true;
            }
        }
        if let Some(width) = comment {
            if is_comment_fence_close_any_column(line, width) {
                comment = None;
            }
            opaque = true;
        }
        if at > start && !opaque {
            if is_blank_line(line) {
                comment_retains_frame = false;
                if let Some(frame) = frames.last_mut() {
                    frame.4 = false;
                }
            } else {
                let comment_is_body_owned = items.is_empty();
                let comment_line = is_line_comment_any_column(line);
                if !comment_line && column <= frame_base {
                    comment_retains_frame = false;
                }
                if comment_line {
                    comment_retains_frame = items.last().is_some_and(|content| column < *content);
                }
                if comment_retains_frame
                    && !comment_line
                    && column >= frame_base
                    && item_block_opener(local)
                {
                    comment_retains_frame = false;
                }
                while !comment_retains_frame
                    && items.last().is_some_and(|content| column < *content)
                {
                    items.pop();
                }
                let in_item = items.last().is_some_and(|content| column >= *content);
                let held_by_item = in_item || comment_retains_frame;
                if let Some(open) = detect_comment_fence_line_any_column(line) {
                    if comment_is_body_owned {
                        comment_retains_frame = false;
                    }
                    comment = Some(open.fence_len);
                } else if let Some(open) =
                    detect_fence_open(local).filter(|_| literal || column >= frame_base || in_item)
                {
                    let inline_code = !literal && !in_item && column > frame_base;
                    code = Some((open, column, literal || inline_code));
                    if let Some(frame) = frames.last_mut() {
                        frame.4 = inline_code;
                    }
                } else if !held_by_item && !literal {
                    if exact_colon_fence_len(local) == Some(frame_width)
                        && (column == frame_base || column == parent_base)
                    {
                        strip = column;
                        frames.pop();
                    } else if column >= frame_base {
                        if let Some(next_width) = detect_container_open(local)
                            .map(|open| open.fence_len)
                            .or_else(|| detect_line_block_open(local))
                            .or_else(|| detect_hardbreaks_block_open(local))
                            .or_else(|| detect_quote_block_open(local))
                        {
                            if frames.len() < MAX_NESTING_DEPTH {
                                let next_literal = detect_line_block_open(local).is_some()
                                    || detect_quote_block_open(local).is_some();
                                frames.push((next_width, column, frame_base, next_literal, false));
                                strip = column;
                            }
                        } else if let Some(frame) = frames.last_mut() {
                            frame.4 =
                                detect_fence_open(local).is_some() || !item_block_opener(local);
                        }
                    }
                } else if literal
                    && exact_colon_fence_len(local) == Some(frame_width)
                    && column == frame_base
                {
                    frames.pop();
                }
                if !literal && (!paragraph || in_item) {
                    if let Some(content) = marker_content_col_at(line, column) {
                        items.push(content);
                    }
                }
            }
        }
        if !is_blank_line(line) && column >= strip {
            #[cfg(test)]
            NAMED_COLON_REBASE_LINES.with(|count| count.set(count.get() + 1));
            let (residue, consumed, synthetic) = slice_columns_mapped(line, strip, true);
            *line = Cow::Owned(residue);
            if let Some(Some(column)) = columns.get_mut(at) {
                *column += consumed as isize - synthetic as isize;
            }
        }
    }
}

/// Apply an authored block base after a container's minimum content column has
/// been stripped. List-item calls leave sublists alone because their residual
/// indentation expresses another list level; definition and footnote bodies
/// include them under carve#1729's shared rule.
pub(in crate::parse) fn rebase_overindented_blocks(
    source: &mut MappedSource,
    include_sublists: bool,
    options: &Options<'_>,
) {
    rebase_body_blocks(source, include_sublists, options, false, include_sublists);
}

pub(in crate::parse) fn rebase_body_blocks(
    source: &mut MappedSource,
    include_sublists: bool,
    options: &Options<'_>,
    absorbed_note_marker: bool,
    description_body: bool,
) {
    // No authored base can move when every line already starts at column zero.
    // This also keeps a quote replay's one-line probes from replaying again.
    if !source
        .source
        .lines()
        .any(|line| line.starts_with([' ', '\t']))
    {
        return;
    }
    // The lines borrow the text while the scan edits the source's maps.
    let text = std::mem::take(&mut source.source);
    let trailing_newline = text.ends_with('\n');
    let mut lines: Vec<Cow<'_, str>> = text.lines().map(Cow::Borrowed).collect();
    if trailing_newline {
        lines.push(Cow::Borrowed(""));
    }
    let mode = BodyRebaseMode {
        include_sublists,
        absorbed_note_marker,
        description_body,
    };
    let mut scan = BodyRebase::new(source, options, mode, &text, lines);
    while scan.i < scan.lines.len() {
        scan.step();
    }
    let mut lines = scan.lines;
    if trailing_newline && lines.last().is_some_and(|line| line.is_empty()) {
        lines.pop();
    }
    let mut rebased = lines.join("\n");
    if trailing_newline {
        rebased.push('\n');
    }
    source.source = rebased;
    source.marker_tail = None;
}

#[derive(Clone, Copy)]
struct BodyRebaseMode {
    include_sublists: bool,
    absorbed_note_marker: bool,
    description_body: bool,
}

/// The block an authored-base opener starts, which decides its extent and
/// how each of its lines is dedented.
#[derive(Clone, Copy)]
struct AuthoredOpener {
    code: Option<FenceOpen>,
    comment_fence_len: Option<usize>,
    line_block: Option<usize>,
    colon: Option<usize>,
    footnote: bool,
}

impl AuthoredOpener {
    fn detect(opener: &str) -> Self {
        let code = detect_fence_open(opener);
        let comment_fence_len = code
            .is_none()
            .then(|| detect_comment_fence_line(opener))
            .flatten()
            .map(|open| open.fence_len);
        let line_block = detect_line_block_open(opener);
        let colon = if code.is_none() && comment_fence_len.is_none() {
            detect_container_open(opener)
                .map(|open| open.fence_len)
                .or_else(|| detect_quote_block_open(opener))
        } else {
            None
        };
        let footnote = code.is_none()
            && comment_fence_len.is_none()
            && colon.is_none()
            && parse_footnote_def_line(opener).is_some();
        Self {
            code,
            comment_fence_len,
            line_block,
            colon,
            footnote,
        }
    }
}

/// Walk state of a colon container opened at the container's minimum column.
struct ColonGroupWalk {
    marker_body: bool,
    paragraph: bool,
    code: Option<(FenceOpen, usize)>,
    comment: Option<usize>,
    items: Vec<usize>,
    comment_retains_frame: bool,
    rebased_until: usize,
    stack: Vec<usize>,
}

struct BodyRebase<'a, 't, 'o> {
    source: &'a mut MappedSource,
    options: &'a Options<'o>,
    mode: BodyRebaseMode,
    text: &'t str,
    lines: Vec<Cow<'t, str>>,
    // Read on the authored lines: ownership is settled before any rebase here.
    authored: Vec<Cow<'t, str>>,
    // Dedenting keeps every line's `%` run, so this index outlives the edits.
    entry_closers: std::cell::OnceCell<HashMap<usize, Vec<usize>>>,
    // Each edit ends before the next iteration, so future columns stay indexed.
    code_columns: std::cell::OnceCell<BodyCodeColumns>,
    quote_lines: std::cell::OnceCell<Vec<&'t str>>,
    fences_before: std::cell::OnceCell<Vec<bool>>,
    payload_closers: Option<HashMap<u8, Vec<usize>>>,
    i: usize,
    nested_columns: Vec<usize>,
    nested_list_marker_bases: Vec<Option<usize>>,
    descendant_code: Option<(FenceOpen, usize)>,
    orphaned_code: Option<(FenceOpen, usize)>,
    after_blank: bool,
    paragraph_open: bool,
    // The open paragraph is the container's own, not a descendant's.
    own_paragraph: bool,
    item_separator_paragraph: bool,
    opaque_quote_tail: bool,
    // A code-shaped run outside the preceding verse's bases is inline text.
    // Keep its authored column through repeated rebase passes.
    last_literal_base: Option<usize>,
    term_boundary: bool,
    description_tail: bool,
    // Has a line AT the container's minimum column been read since the scan
    // last stood inside a descendant? It is ownership evidence exactly as an
    // open paragraph is: the scan is back in the parent's coordinate system,
    // so the next over-indented opener was authored there rather than left
    // behind by a child whose own column it fell short of. See the guard
    // in `authored_base_line` for what only counting the paragraph cost
    // (carve-rs#1415).
    block_at_minimum: bool,
    paragraph_after_invisible_base: Option<usize>,
    invisible_end: Option<usize>,
}

impl<'a, 't, 'o> BodyRebase<'a, 't, 'o> {
    fn new(
        source: &'a mut MappedSource,
        options: &'a Options<'o>,
        mode: BodyRebaseMode,
        text: &'t str,
        lines: Vec<Cow<'t, str>>,
    ) -> Self {
        Self {
            source,
            options,
            mode,
            text,
            authored: lines.clone(),
            lines,
            entry_closers: std::cell::OnceCell::new(),
            code_columns: std::cell::OnceCell::new(),
            quote_lines: std::cell::OnceCell::new(),
            fences_before: std::cell::OnceCell::new(),
            payload_closers: None,
            i: 0,
            nested_columns: Vec::new(),
            nested_list_marker_bases: Vec::new(),
            descendant_code: None,
            orphaned_code: None,
            after_blank: false,
            paragraph_open: false,
            own_paragraph: false,
            item_separator_paragraph: false,
            opaque_quote_tail: false,
            last_literal_base: None,
            term_boundary: false,
            description_tail: false,
            block_at_minimum: false,
            paragraph_after_invisible_base: None,
            invisible_end: None,
        }
    }

    fn reached(&self, at: usize) -> bool {
        self.source.reached.get(at).copied().unwrap_or(false)
    }

    fn below_descendant(&self, base: usize) -> bool {
        self.nested_columns
            .last()
            .is_some_and(|column| base < *column)
    }

    fn within_descendant(&self, base: usize) -> bool {
        self.nested_columns
            .last()
            .is_some_and(|column| base >= *column)
    }

    fn comment_closers(&self) -> &HashMap<usize, Vec<usize>> {
        self.entry_closers
            .get_or_init(|| comment_closers_by_run(&self.lines))
    }

    fn next_comment_close(&self, fence_len: usize, after: usize) -> Option<usize> {
        self.comment_closers()
            .get(&fence_len)
            .and_then(|at| at.get(at.partition_point(|&at| at <= after)).copied())
    }

    fn code_closes_after(&self, at: usize, column: usize, open: FenceOpen) -> bool {
        self.code_columns
            .get_or_init(|| BodyCodeColumns::new(&self.lines))
            .closes_after(at, column, open)
    }

    fn step(&mut self) {
        let i = self.i;
        if is_blank_line(&self.lines[i]) {
            self.paragraph_after_invisible_base = None;
            self.last_literal_base = None;
            self.opaque_quote_tail = false;
            self.after_blank = true;
            self.paragraph_open = false;
            self.item_separator_paragraph = false;
            self.i += 1;
            return;
        }
        if !self.paragraph_open {
            self.item_separator_paragraph = false;
        }
        let base = indent_columns(&self.lines[i]);
        let local = strip_leading_columns(&self.lines[i], base);
        if self.code_shaped_line(base, &local) || self.first_line_residual(base) {
            return;
        }
        // Did the line REACH the container's own content column? A line one
        // column past it and a line below it both arrive with a single
        // residual column and only `reached` separates them - see
        // `MappedSource::reached`. Unknown reads as "below", which is what
        // this scan assumed before markup-carve/carve#1896.
        let reaches_container = self.reached(i);
        if self.lazy_line(base, &local, reaches_container)
            || self.below_descendant_line(base, &local)
        {
            return;
        }
        while self.below_descendant(base) {
            self.nested_columns.pop();
            self.nested_list_marker_bases.pop();
        }
        if self.descendant_opener(base, &local) {
            return;
        }
        if base == 0 {
            self.minimum_column_line(reaches_container);
        } else {
            self.authored_base_line(base, reaches_container);
        }
    }

    /// Fence-shaped lines that are inline text, descendant code, or an
    /// orphaned descendant fence's closer. True when the line is consumed.
    fn code_shaped_line(&mut self, base: usize, local: &str) -> bool {
        let i = self.i;
        if self.mode.include_sublists && self.paragraph_open {
            if let Some(open) = detect_fence_open(local) {
                if !self.code_closes_after(i, 0, open) && !self.code_closes_after(i, base, open) {
                    // An unmatched run is inline text and establishes no block base.
                    self.after_blank = false;
                    self.i += 1;
                    return true;
                }
            }
        }
        let mut closed_descendant_code = false;
        if let Some((open, column)) = self.descendant_code {
            if base < column {
                self.orphaned_code = self.descendant_code.take();
            } else if base == column && is_fence_close(local, open) {
                self.descendant_code = None;
                closed_descendant_code = true;
            }
        }
        if base == 0 && heading_content_starts(local) {
            self.orphaned_code = None;
        }
        if self
            .orphaned_code
            .is_some_and(|(open, column)| base == column && is_fence_close(local, open))
        {
            self.orphaned_code = None;
            self.paragraph_open = true;
            self.own_paragraph = false;
            self.after_blank = false;
            self.i += 1;
            return true;
        }
        if !closed_descendant_code && self.descendant_code.is_none() && self.within_descendant(base)
        {
            if let Some(open) = detect_fence_open(local) {
                self.descendant_code = Some((open, base));
            }
        }
        if base > 0 && self.last_literal_base.is_some_and(|column| base != column) {
            if let Some(open) = detect_fence_open(local) {
                self.i = rebased_code_fence_end(&self.lines, i, base, open) + 1;
                self.paragraph_open = true;
                self.own_paragraph = false;
                return true;
            }
        }
        if base == 0 {
            self.last_literal_base = None;
        }
        false
    }

    fn first_line_residual(&mut self, base: usize) -> bool {
        let i = self.i;
        if i != 0 || base == 0 || self.source.authored_base_at_start {
            return false;
        }
        // A residual first-line marker still opens a descendant. Register
        // its content column before preserving the residual line so a
        // post-blank block at or past that column remains the child's.
        if let Some(column) = marker_content_col(&self.lines[i]) {
            self.nested_columns.push(column);
            self.nested_list_marker_bases.push(
                detect_list_marker_full(&self.lines[i])
                    .filter(|_| base > 0)
                    .map(|_| base),
            );
        }
        self.paragraph_open = !is_line_comment_any_column(&self.lines[i]);
        self.own_paragraph = false;
        self.block_at_minimum = !self.paragraph_open && self.reached(i);
        self.i += 1;
        true
    }

    /// Lines that stay lazy text of a paragraph or quote left open above.
    fn lazy_line(&mut self, base: usize, local: &str, reaches_container: bool) -> bool {
        let i = self.i;
        let include_sublists = self.mode.include_sublists;
        if !include_sublists && self.opaque_quote_tail && base > 0 {
            self.i += 1;
            return true;
        }
        // A closed fence short of the child column belongs to the outer item.
        // Only an unclosed run remains lazy text of the child's paragraph.
        let closed_fence_below_descendant = self.below_descendant(base)
            && self.nested_list_marker_bases.last() == Some(&Some(base))
            && detect_fence_open(local).is_some_and(|open| self.code_closes_after(i, base, open));
        if !include_sublists
            && self.paragraph_open
            && base > 0
            && item_block_opener(local)
            && !(self
                .paragraph_after_invisible_base
                .is_some_and(|column| base >= column)
                && is_table_start(local))
            && !(self.reached(i) && strip_blockquote_prefix(local).is_some())
            && !closed_fence_below_descendant
            && !(self.item_separator_paragraph && self.reached(i))
        {
            self.i += 1;
            return true;
        }
        // Reaching the outer container does not move a line out of a deeper
        // item's still-open paragraph. It remains below that item's content
        // column, and the next line at the column still belongs to that item.
        if self.paragraph_open
            && !self.after_blank
            && self.below_descendant(base)
            && !item_block_opener(local)
        {
            self.i += 1;
            return true;
        }
        // §10 I5's invisible kinds are lazy paragraph text of THIS container
        // only BELOW its content column (markup-carve/carve#1809, corpus
        // 430-2/430-3). At or past the column the same line is the container's
        // own block content and rebases like any other opener
        // (markup-carve/carve#1911, corpus 444-7) - the residual the slice
        // leaves in front of it is what made the two look alike.
        if self.paragraph_open
            && base > 0
            && !reaches_container
            && item_block_opener(local)
            && !item_block_opener_with_invisible_arms(local, false)
        {
            self.i += 1;
            return true;
        }
        // A residual indent below EVERY live column is nobody's authored base:
        // it reaches no descendant, and not the container either, so it stays
        // the lazy line of whatever the line above left open.
        if !reaches_container
            && !self.after_blank
            && self.below_descendant(base)
            && (base > 0 || !item_block_opener(local))
        {
            self.block_at_minimum = false;
            self.i += 1;
            return true;
        }
        // It reached the container and fell short of every descendant, so no
        // descendant owns it (markup-carve/carve#1896): the deeper one opened
        // on a line above and never on this one. The scan is back in the
        // container's coordinate system exactly as a line at its minimum column
        // puts it there, and the opener written here is the container's own.
        if reaches_container && self.below_descendant(base) {
            self.block_at_minimum = true;
        }
        false
    }

    /// Invisible lines and refused lazy `+` markers below a descendant's column.
    fn below_descendant_line(&mut self, base: usize, local: &str) -> bool {
        if !self.below_descendant(base) {
            return false;
        }
        let i = self.i;
        // A line comment below a descendant's column leaves that item available.
        if is_line_comment_any_column(&self.lines[i]) {
            self.paragraph_open = false;
            self.i += 1;
            return true;
        }
        if self.comment_fence_below_descendant(base, local) {
            return true;
        }
        let refused_lazy_plus = is_plus_marker(trim_ascii_start(&self.lines[i]))
            && self
                .source
                .line_map
                .get(i)
                .copied()
                .flatten()
                .and_then(continuation_marker::decision)
                .is_some_and(|decision| decision.refusal.is_some())
            && (self.paragraph_open
                || i > 0
                    && detect_list_marker_full(&self.lines[i - 1])
                        .is_some_and(|marker| line_starts_paragraph(marker.content)));
        if refused_lazy_plus {
            self.paragraph_open = true;
            self.own_paragraph = false;
            self.after_blank = false;
            self.i += 1;
            return true;
        }
        false
    }

    fn comment_fence_below_descendant(&mut self, base: usize, local: &str) -> bool {
        let i = self.i;
        let Some(open) = detect_comment_fence_line(local) else {
            return false;
        };
        let Some(close) = self.next_comment_close(open.fence_len, i) else {
            return false;
        };
        let include_sublists = self.mode.include_sublists;
        let between_items = self
            .nested_list_marker_bases
            .last()
            .and_then(|marker_base| *marker_base)
            .is_some_and(|outer_col| base >= outer_col);
        if include_sublists && between_items {
            self.nested_columns.pop();
            self.nested_list_marker_bases.pop();
            self.paragraph_open = false;
            self.i = close + 1;
            return true;
        }
        if include_sublists || indent_columns(&self.lines[i]) < base || close == i + 1 {
            self.paragraph_open = false;
            self.i = close + 1;
            return true;
        }
        false
    }

    /// Lines that open, or stand inside, a descendant item, description or note.
    fn descendant_opener(&mut self, base: usize, local: &str) -> bool {
        let i = self.i;
        let include_sublists = self.mode.include_sublists;
        // §10 I2: a marker under the container's own paragraph folds into it
        // and opens no item, so it sets no column for the lines below.
        if include_sublists
            && self.own_paragraph
            && self.paragraph_open
            && !self.after_blank
            && is_list_marker(local)
        {
            self.i += 1;
            return true;
        }
        if self.list_marker_opener(base) {
            return true;
        }
        let bare_description = strip_definition_marker(trim_ascii_start(&self.lines[i])).is_some();
        if !(self.mode.absorbed_note_marker && bare_description && !self.term_boundary) {
            if let Some(column) = definition_body_content_col(&self.lines[i]) {
                self.nested_columns.push(column);
                self.nested_list_marker_bases.push(None);
            }
        }
        // AN OVER-INDENTED NESTED NOTE DEFINITION KEEPS ITS AUTHORED COLUMN.
        // A note's body column is measured from its OWN marker (§16, marker plus
        // two - carve-js#1664), so the recursive note pass needs the residual
        // indent that says how deep this marker sits. Flattening it to the
        // container's own column zero collapsed a two-level nesting into one, so
        // a trailing line below the inner note's own content column was claimed
        // by it instead of falling to the reachable ancestor note
        // (markup-carve/carve#1946, carve-php#1895). The floor is NOT registered
        // as a descendant column here: a note's opaque payload sits BELOW that
        // floor, and a live column above it would make the rebasing branch read
        // an indented fence there as a lazy line and strand its payload. A note
        // at the container's own column zero is left to the base-zero handling
        // below.
        if include_sublists && base > 0 && parse_footnote_def_line(local).is_some() {
            self.after_blank = false;
            self.block_at_minimum = false;
            self.i += 1;
            return true;
        }
        if self.within_descendant(base) {
            // A descendant's line can leave a paragraph or a term open for a
            // band marker below it, which S4 retains as its text.
            if include_sublists && (line_starts_paragraph(local) || is_definition_list_start(local))
            {
                self.paragraph_open = true;
                self.own_paragraph = false;
            }
            self.block_at_minimum = false;
            self.i += 1;
            return true;
        }
        false
    }

    fn list_marker_opener(&mut self, base: usize) -> bool {
        let i = self.i;
        let include_sublists = self.mode.include_sublists;
        let Some(column) = marker_content_col_at(&self.lines[i], base).filter(|_| {
            !(self.mode.absorbed_note_marker && self.paragraph_open && !self.after_blank)
        }) else {
            return false;
        };
        if include_sublists && base != 0 && self.nested_columns.is_empty() {
            return false;
        }
        let marker = detect_list_marker_full_at(&self.lines[i], base);
        let colon_folds = marker.as_ref().is_some_and(|marker| {
            detect_container_open(marker.content).is_some()
                || detect_line_block_open(marker.content).is_some()
                || detect_hardbreaks_block_open(marker.content).is_some()
                || detect_quote_block_open(marker.content).is_some()
        }) && self
            .lines
            .get(i + 1)
            .is_some_and(|next| !is_blank_line(next) && indent_columns(next) < column);
        self.own_paragraph = false;
        self.paragraph_open = colon_folds
            || (include_sublists
                && marker
                    .as_ref()
                    .is_some_and(|marker| line_starts_paragraph(marker.content)));
        self.nested_columns.push(column);
        self.nested_list_marker_bases
            .push(marker.as_ref().filter(|_| base > 0).map(|_| base));
        let mut inner_base = column;
        let mut inner_content = marker.as_ref().map(|marker| marker.content);
        while let Some(content) = inner_content.filter(|content| indent_columns(content) == 0) {
            let Some(inner_marker) = detect_list_marker_full_at(content, 0) else {
                break;
            };
            let Some(width) = marker_content_col_at(content, 0) else {
                break;
            };
            let inner_column = inner_base + width;
            self.nested_columns.push(inner_column);
            self.nested_list_marker_bases.push(Some(inner_base));
            inner_base = inner_column;
            inner_content = Some(inner_marker.content);
        }
        self.after_blank = false;
        self.block_at_minimum = false;
        self.i += 1;
        true
    }

    /// A line at the container's minimum column: skip the extent of an
    /// opaque or owning block, or record a paragraph's evidence.
    fn minimum_column_line(&mut self, reaches_container: bool) {
        let i = self.i;
        self.opaque_quote_tail = false;
        if strip_blockquote_prefix(&self.lines[i]).is_some() {
            self.skip_minimum_quote();
            return;
        }
        // An opaque group already at the container's minimum column owns
        // its payload. Do not reconsider a fence-shaped payload line as a
        // separate authored-base opener.
        if let Some(open) = detect_fence_open(&self.lines[i]) {
            self.i += 1;
            while self.i < self.lines.len() {
                let closes = is_fence_close(&self.lines[self.i], open);
                self.i += 1;
                if closes {
                    break;
                }
            }
            self.after_blank = false;
            self.paragraph_open = false;
            self.block_at_minimum = self.term_boundary;
            return;
        }
        if let Some(open) = detect_comment_fence_line(&self.lines[i]) {
            self.skip_minimum_comment(open.fence_len);
            return;
        }
        // Verse lines are inline content, including indented block markers.
        // Only the line block's own closer ends this opaque body.
        if let Some(width) = detect_line_block_open(&self.lines[i]) {
            self.last_literal_base = Some(0);
            self.i += 1;
            while self.i < self.lines.len() {
                let closes = exact_colon_fence_len(&self.lines[self.i]) == Some(width);
                self.i += 1;
                if closes {
                    break;
                }
            }
            self.after_blank = false;
            self.paragraph_open = false;
            // The completed block lets the next authored opener rebase.
            self.block_at_minimum = true;
            return;
        }
        if let Some(width) = detect_container_open(&self.lines[i])
            .map(|open| open.fence_len)
            .or_else(|| detect_quote_block_open(&self.lines[i]))
        {
            self.walk_minimum_colon_group(width);
            return;
        }
        // A definition entry at the container minimum remains the
        // innermost owner for its exact extent. Skip that extent so a
        // structural payload at the description column is not reconsidered
        // as an authored base of the outer body; the first line outside the
        // extent is deliberately classified by that outer body.
        if is_definition_list_start(&self.lines[i]) {
            let (end, term) = definition_entry_extent(&self.lines, i, 0, &self.entry_closers);
            self.term_boundary = term;
            self.description_tail = !term;
            self.i = end + 1;
            self.after_blank = false;
            self.paragraph_open = false;
            self.block_at_minimum = true;
            return;
        }
        self.paragraph_open = line_starts_paragraph(strip_lazy(&self.lines[i]));
        self.item_separator_paragraph = self.lines[i].starts_with(ITEM_PARAGRAPH);
        self.own_paragraph = self.paragraph_open;
        if self.paragraph_open {
            self.term_boundary = false;
        }
        self.after_blank = false;
        // A comment reaching this frame closes its paragraph. A lower
        // comment preserves the owner established before it; the next
        // opener must still reach this frame to use that evidence.
        self.block_at_minimum = self.block_at_minimum
            || reaches_container
            || (i == 0 && self.source.item_marker_at_start)
            || (i == 0 && is_collected_definition_placeholder(&self.lines[i]))
            || !is_line_comment_any_column(&self.lines[i]);
        self.i += 1;
    }

    /// Unmarked continuations belong to the quote before their residual
    /// indentation can become another block's base.
    fn skip_minimum_quote(&mut self) {
        let i = self.i;
        let marker_fence_quote = i == 0
            && self.source.item_marker_at_start
            && !self.source.authored_base_at_start
            && detect_fence_open(without_blockquote_prefixes(&self.lines[i])).is_some();
        let text = self.text;
        let original = self
            .quote_lines
            .get_or_init(|| text.lines().collect::<Vec<_>>());
        let mut cursor = LineCursor::new_with_cols(original, None, None);
        cursor.pos = i;
        cursor.in_item_body = !self.mode.include_sublists;
        cursor.reached = Some(&self.source.reached);
        {
            let _probing = ProbeGuard::enter();
            let (_, _, opaque_tail) = collect_blockquote_body(&mut cursor, self.options);
            self.opaque_quote_tail = marker_fence_quote && opaque_tail;
        }
        self.i = cursor.pos;
        self.after_blank = false;
        self.paragraph_open = false;
        self.block_at_minimum = true;
    }

    fn skip_minimum_comment(&mut self, fence_len: usize) {
        let comment_at = self.i;
        let close = self.next_comment_close(fence_len, comment_at);
        self.i = close.map_or(comment_at + 1, |at| at + 1);
        self.invisible_end = self.i.checked_sub(1);
        self.after_blank = false;
        self.paragraph_open = false;
        self.block_at_minimum = self.term_boundary
            || (close == Some(comment_at + 1) && self.block_at_minimum)
            || (close.is_none()
                && !self.mode.include_sublists
                && (self.block_at_minimum || comment_at == 0));
    }

    // A COLON CONTAINER AT THE MINIMUM COLUMN OWNS ITS EXTENT TOO. It
    // is not opaque, but its payload is still its own: CARVE-P0-004
    // keeps a run in the band between the container's column and a
    // fence base inside it at the authored column, and reading that run
    // again as an authored base dedents it onto the container's column,
    // where the container takes it as its closer. This pass runs twice
    // over one item body - `item_body` and then `parse_item_chunk` - so
    // the second run is where that happened (carve-rs#2095).
    //
    // The walk tracks colon widths only, as the base-past-zero arm
    // below does.
    fn walk_minimum_colon_group(&mut self, width: usize) {
        let mut walk = ColonGroupWalk {
            marker_body: self.i == 0 && !self.source.marker_colon_rebased,
            paragraph: false,
            code: None,
            comment: None,
            items: Vec::new(),
            comment_retains_frame: false,
            rebased_until: 0,
            stack: vec![width],
        };
        self.i += 1;
        while self.i < self.lines.len() {
            if self.colon_group_line(&mut walk) {
                break;
            }
        }
        self.after_blank = false;
        self.paragraph_open = false;
        self.block_at_minimum = true;
    }

    /// One line of a minimum-column colon group. True once the group closes.
    fn colon_group_line(&mut self, walk: &mut ColonGroupWalk) -> bool {
        let i = self.i;
        let base = indent_columns(&self.lines[i]);
        if self.colon_payload_line(walk, base) {
            return false;
        }
        let comment_is_body_owned = walk.items.is_empty();
        self.track_colon_frame(walk, base);
        let owned_by_item = walk.items.last().is_some_and(|column| base >= *column);
        if (walk.marker_body || !self.mode.include_sublists)
            && base > 0
            && !owned_by_item
            && !walk.comment_retains_frame
            && self.reached(i)
            && self.rebase_colon_band_opener(walk, base)
        {
            return false;
        }
        if self.skip_colon_line_block(walk, base, owned_by_item) {
            return false;
        }
        if let Some(open) = detect_fence_open(trim_ascii_start(&self.lines[i]))
            .filter(|_| base == 0 || owned_by_item)
        {
            walk.code = Some((open, base));
            self.i += 1;
            return false;
        }
        if self.open_colon_comment(walk, comment_is_body_owned) {
            return false;
        }
        self.colon_items_and_marker_body(walk, base, owned_by_item);
        self.close_colon_line(walk, base)
    }

    /// Code and comment payload inside the group opens nothing.
    fn colon_payload_line(&mut self, walk: &mut ColonGroupWalk, base: usize) -> bool {
        let i = self.i;
        if let Some((open, column)) = walk.code {
            if !is_blank_line(&self.lines[i]) && base < column {
                walk.code = None;
            } else {
                if base == column && is_fence_close(trim_ascii_start(&self.lines[i]), open) {
                    walk.code = None;
                }
                self.i += 1;
                return true;
            }
        }
        if let Some(width) = walk.comment {
            if is_comment_fence_close_any_column(&self.lines[i], width) {
                walk.comment = None;
            }
            self.i += 1;
            return true;
        }
        false
    }

    fn track_colon_frame(&self, walk: &mut ColonGroupWalk, base: usize) {
        let i = self.i;
        let comment_line = is_line_comment_any_column(&self.lines[i]);
        if is_blank_line(&self.lines[i]) || (!comment_line && base == 0) {
            walk.comment_retains_frame = false;
        }
        if comment_line {
            walk.comment_retains_frame = walk.items.last().is_some_and(|column| base < *column);
        }
        if walk.comment_retains_frame
            && !comment_line
            && base > 0
            && self.reached(i)
            && item_block_opener(trim_ascii_start(&self.lines[i]))
        {
            walk.comment_retains_frame = false;
            walk.paragraph = false;
        }
        while !walk.comment_retains_frame
            && walk.items.last().is_some_and(|column| base < *column)
            && !is_blank_line(&self.lines[i])
        {
            walk.items.pop();
        }
    }

    /// A block past the group's column that reached the container opens on
    /// its authored base. True when its extent was rebased.
    fn rebase_colon_band_opener(&mut self, walk: &mut ColonGroupWalk, base: usize) -> bool {
        let i = self.i;
        let local = trim_ascii_start(&self.lines[i]).to_owned();
        let code = detect_fence_open(&local);
        let colon = detect_container_open(&local)
            .map(|open| open.fence_len)
            .or_else(|| detect_line_block_open(&local))
            .or_else(|| detect_hardbreaks_block_open(&local))
            .or_else(|| detect_quote_block_open(&local));
        let end = if let Some(open) = code {
            let end = rebased_code_fence_end(&self.lines, i, base, open);
            (!walk.paragraph || is_fence_close(trim_ascii_start(&self.lines[end]), open))
                .then_some(end)
        } else if let Some(width) = colon.filter(|_| exact_colon_fence_len(&local).is_none()) {
            Some(rebased_colon_group_end(&self.lines, i, base, width))
        } else if heading_content_starts(&local)
            || detect_thematic_break(&local)
            || is_table_start(&local)
            || parse_standalone_attrs(&local).is_some()
        {
            Some(i)
        } else if !walk.paragraph && strip_blockquote_prefix(&local).is_some() {
            // CARVE-P0-011: past the box's column it opens on
            // its authored base, as a nested list does.
            Some(rebased_quote_end(&self.lines, i, base))
        } else if !walk.paragraph && parse_link_def_line_with_value(&local).is_some() {
            Some(i)
        } else {
            None
        };
        let Some(end) = end else {
            return false;
        };
        if let Some(width) = colon {
            rebase_named_colon_extent(
                &mut self.lines,
                &mut self.source.col_map,
                i,
                end,
                base,
                width,
            );
        } else {
            self.rebase_colon_band_payload(end, base, code);
        }
        self.source.marker_colon_rebased = true;
        walk.paragraph = false;
        self.i = end + 1;
        true
    }

    fn rebase_colon_band_payload(&mut self, end: usize, base: usize, code: Option<FenceOpen>) {
        let i = self.i;
        // An unclosed fence owns its body to the box's end
        // (CARVE-P0-014), so a colon run in it must not
        // land on the box's column as its closer.
        let unclosed = code.is_some_and(|open| {
            end == i || !is_fence_close(trim_ascii_start(&self.lines[end]), open)
        }) && !self
            .fences_before
            .get_or_init(|| fences_open_before(&self.authored))
            .get(i)
            .copied()
            .unwrap_or(false);
        for (at, line) in self.lines.iter_mut().enumerate().take(end + 1).skip(i) {
            // A run on the box's own column closes the box.
            let on_base = indent_columns(line) > 0;
            let (residue, consumed, synthetic) = slice_columns_mapped(line, base, true);
            let framed = unclosed && at > i && on_base && exact_colon_fence_len(&residue).is_some();
            *line = Cow::Owned(if framed {
                format!("{LAZY}{residue}")
            } else {
                residue
            });
            shift_mapped_column(
                self.source.col_map.get_mut(at),
                consumed as isize - synthetic as isize,
            );
        }
    }

    /// A nested line block is opaque: its verse lines open nothing, and its
    /// closer leaves no paragraph open.
    fn skip_colon_line_block(
        &mut self,
        walk: &mut ColonGroupWalk,
        base: usize,
        owned_by_item: bool,
    ) -> bool {
        let i = self.i;
        let nested_line_block = detect_line_block_open(trim_ascii_start(&self.lines[i]))
            .filter(|_| base > 0 && owned_by_item)
            .map(|width| (width, base, false))
            .or_else(|| {
                let marker = detect_list_marker_full_at(&self.lines[i], base)?;
                let width = detect_line_block_open(marker.content)?;
                Some((width, marker_content_col_at(&self.lines[i], base)?, true))
            });
        let Some((width, column, item)) = nested_line_block else {
            return false;
        };
        if item {
            walk.items.push(column);
        }
        self.i += 1;
        while self.i < self.lines.len() {
            let line = &self.lines[self.i];
            if !is_blank_line(line) && indent_columns(line) < column {
                break;
            }
            self.i += 1;
            if indent_columns(line) == column
                && exact_colon_fence_len(trim_ascii_start(line)) == Some(width)
            {
                break;
            }
        }
        walk.paragraph = false;
        true
    }

    fn open_colon_comment(
        &mut self,
        walk: &mut ColonGroupWalk,
        comment_is_body_owned: bool,
    ) -> bool {
        let i = self.i;
        let Some(open) = detect_comment_fence_line(trim_ascii_start(&self.lines[i])) else {
            return false;
        };
        let closes_later = self
            .comment_closers()
            .get(&open.fence_len)
            .is_some_and(|indices| indices.last().is_some_and(|last| *last > i));
        if !closes_later {
            return false;
        }
        if comment_is_body_owned {
            walk.comment_retains_frame = false;
        }
        walk.comment = Some(open.fence_len);
        walk.paragraph = false;
        self.i += 1;
        true
    }

    fn colon_items_and_marker_body(
        &mut self,
        walk: &mut ColonGroupWalk,
        base: usize,
        owned_by_item: bool,
    ) {
        let i = self.i;
        if (!walk.paragraph || !walk.items.is_empty())
            && detect_list_marker_full_at(&self.lines[i], base).is_some()
        {
            if let Some(column) = marker_content_col_at(&self.lines[i], base) {
                walk.items.push(column);
            }
        }
        if !(walk.marker_body
            && !walk.paragraph
            && !owned_by_item
            && !walk.comment_retains_frame
            && i >= walk.rebased_until
            && base > 0)
        {
            return;
        }
        let Some(width) =
            detect_container_open(trim_ascii_start(&self.lines[i])).map(|open| open.fence_len)
        else {
            return;
        };
        if exact_colon_fence_len(trim_ascii_start(&self.lines[i])).is_none() {
            let end = rebased_colon_group_end(&self.lines, i, base, width);
            walk.rebased_until = end + 1;
            self.source.marker_colon_rebased = true;
            rebase_named_colon_extent(
                &mut self.lines,
                &mut self.source.col_map,
                i,
                end,
                base,
                width,
            );
        }
    }

    /// Track the group's paragraph and nesting after an ordinary line. True
    /// once the outermost colon run closes.
    fn close_colon_line(&mut self, walk: &mut ColonGroupWalk, base: usize) -> bool {
        let line = &self.lines[self.i];
        // A bare colon run closes a nested container, which ends
        // its paragraph too (CARVE-P0-014: an unclosed fence below
        // then has nothing to interrupt).
        if is_blank_line(line)
            || is_line_comment_any_column(line)
            || (base > 0 && exact_colon_fence_len(trim_ascii_start(line)).is_some())
        {
            walk.paragraph = false;
        } else if detect_fence_open(trim_ascii_start(line)).is_some()
            || !item_block_opener(trim_ascii_start(line))
        {
            walk.paragraph = true;
        }
        self.i += 1;
        if exact_colon_fence_len(line) == Some(*walk.stack.last().unwrap()) {
            walk.paragraph = false;
            walk.stack.pop();
            return walk.stack.is_empty();
        }
        if line.starts_with([' ', '\t']) || walk.stack.len() >= MAX_NESTING_DEPTH {
            return false;
        }
        if let Some(len) = detect_container_open(line)
            .map(|open| open.fence_len)
            .or_else(|| detect_line_block_open(line))
            .or_else(|| detect_hardbreaks_block_open(line))
            .or_else(|| detect_quote_block_open(line))
        {
            walk.stack.push(len);
        }
        false
    }

    /// A line past the minimum column: plain text keeps its residual
    /// indent, an opener with ownership evidence moves to the minimum column.
    fn authored_base_line(&mut self, base: usize, reaches_container: bool) {
        let i = self.i;
        let include_sublists = self.mode.include_sublists;
        let opener = strip_leading_columns(&self.lines[i], base);
        if (is_list_marker(&opener) && !include_sublists)
            || (!is_list_marker(&opener) && !item_block_opener(&opener))
        {
            self.authored_text_line(base, &opener);
            return;
        }
        if i > 0
            && !self.after_blank
            && !self.paragraph_open
            && !(self.block_at_minimum
                && (reaches_container || !is_line_comment_any_column(&self.lines[i - 1])))
            && !self.source.authored_base_at_start
        {
            self.i += 1;
            return;
        }

        self.paragraph_open = false;
        self.paragraph_after_invisible_base = None;

        let kind = AuthoredOpener::detect(&opener);
        let end = self.authored_extent_end(base, &opener, kind);
        let end = self.caption_end(base, end);
        self.rebase_authored_extent(base, end, kind);
        // A separator below an authored definition list's column does not
        // continue it (carve#3451), so it is read as paragraph text.
        if self.mode.description_body && base > 0 && is_definition_list_start(&self.lines[i]) {
            if let Some(next) = self.lines.get_mut(end + 1) {
                if indent_columns(next) == 0 && strip_definition_marker(next).is_some() {
                    *next = Cow::Owned(format!("{ITEM_PARAGRAPH}{next}"));
                }
            }
        }
        self.i = end + 1;
        self.after_blank = false;
        // The block just moved to the minimum column, so it is evidence for
        // the next one exactly as an authored block written there is. Without
        // this a RUN of authored openers rebased only its first member.
        self.block_at_minimum = true;
    }

    fn authored_text_line(&mut self, base: usize, opener: &str) {
        let i = self.i;
        let paragraph = line_starts_paragraph(opener);
        if !self.description_tail && !self.mode.include_sublists {
            let follows_invisible = i > 0
                && (is_collected_definition_placeholder(&self.lines[i - 1])
                    || is_line_comment_any_column(&self.lines[i - 1])
                    || self.invisible_end == Some(i - 1));
            self.paragraph_after_invisible_base = if paragraph && follows_invisible {
                Some(base)
            } else if paragraph && self.paragraph_open {
                self.paragraph_after_invisible_base
            } else {
                None
            };
            self.paragraph_open = paragraph;
        }
        if paragraph {
            self.term_boundary = false;
        }
        self.after_blank = false;
        self.i += 1;
    }

    fn authored_extent_end(&mut self, base: usize, opener: &str, kind: AuthoredOpener) -> usize {
        let i = self.i;
        let lines = &self.lines;
        if let Some(open) = kind.code {
            rebased_code_fence_end(lines, i, base, open)
        } else if let Some(fence_len) = kind.comment_fence_len {
            rebased_comment_end(lines, i, base, fence_len)
        } else if let Some(width) = kind.line_block {
            self.last_literal_base = Some(base);
            rebased_line_block_end(lines, i, base, width, &mut self.payload_closers)
        } else if let Some(width) = kind.colon {
            rebased_colon_group_end(lines, i, base, width)
        } else if kind.footnote {
            rebased_footnote_end(lines, i, base)
        } else if is_list_marker(opener) {
            rebased_list_end(lines, i, base)
        } else if strip_blockquote_prefix(opener).is_some() {
            rebased_quote_end(lines, i, base)
        } else if is_table_start(opener) {
            rebased_table_end(lines, i, base)
        } else if is_definition_list_start(opener) {
            rebased_definition_list_end(lines, i, base)
        } else if parse_standalone_attrs(opener).is_some() {
            self.attribute_target_end(base)
        } else {
            i
        }
    }

    /// A floating attribute line and the list marker it targets share
    /// the authored base. Sublists are otherwise intentionally left to
    /// their own column logic, so carry this one marker with its
    /// attribute rather than leaving a residual indent between them.
    fn attribute_target_end(&self, base: usize) -> usize {
        let i = self.i;
        if let Some(candidate) = self.lines.get(i + 1) {
            let target = strip_leading_columns(candidate, base);
            if !is_blank_line(candidate)
                && indent_columns(candidate) >= base
                && (is_list_marker(&target)
                    || (self.mode.include_sublists && item_block_opener(&target)))
            {
                return i + 1;
            }
        }
        i
    }

    /// A caption is a structural continuation of the captionable block
    /// immediately above it, and therefore shares that opener's authored
    /// base. Leaving it at the authored column detaches it as literal text.
    fn caption_end(&self, base: usize, end: usize) -> usize {
        if let Some(candidate) = self.lines.get(end + 1) {
            if !is_blank_line(candidate) && indent_columns(candidate) >= base {
                let local = strip_leading_columns(candidate, base);
                if local.starts_with("^ ") && local[2..].chars().any(|ch| !ch.is_whitespace()) {
                    return end + 1;
                }
            }
        }
        end
    }

    fn rebase_authored_extent(&mut self, base: usize, end: usize, kind: AuthoredOpener) {
        let i = self.i;
        let comment_close = kind
            .comment_fence_len
            .and_then(|fence_len| self.next_comment_close(fence_len, i));
        for (j, line) in self.lines.iter_mut().enumerate().take(end + 1).skip(i) {
            if comment_close.is_some_and(|close| j > i && j < close) {
                continue;
            }
            rebase_authored_line(line, self.source.col_map.get_mut(j), j == end, base, kind);
        }
    }
}

fn rebase_authored_line(
    line: &mut Cow<'_, str>,
    col: Option<&mut Option<isize>>,
    last: bool,
    base: usize,
    kind: AuthoredOpener,
) {
    if is_blank_line(line) {
        if kind.code.is_some() {
            let (residue, consumed, synthetic) = slice_columns_mapped(line, base, true);
            *line = Cow::Owned(residue);
            shift_mapped_column(col, consumed as isize - synthetic as isize);
        }
        return;
    }
    if last
        && kind
            .code
            .is_some_and(|open| is_fence_close(strip_lazy(line), open))
    {
        *line = Cow::Owned(strip_lazy(line).to_string());
        return;
    }
    // Code payload below the fence's base loses its whole prefix
    // (CARVE-P0-004); a run there still closes nothing. Colon payload
    // keeps its residual indentation, so an in-band run cannot become
    // a closer.
    if indent_columns(line) < base {
        if let Some(open) = kind.code {
            let consumed = indent_columns(line);
            let residue = trim_ascii_start(line);
            *line = Cow::Owned(if is_fence_close(residue, open) {
                format!("{LAZY}{residue}")
            } else {
                residue.to_string()
            });
            shift_mapped_column(col, consumed as isize);
            return;
        }
        if kind.colon.is_some() || kind.line_block.is_some() {
            return;
        }
    }
    let (residue, consumed, synthetic) = slice_columns_mapped(line, base, true);
    *line = Cow::Owned(residue);
    // Positions count authored codepoints, excluding the spaces
    // synthesized when a tab crosses the reference column.
    shift_mapped_column(col, consumed as isize - synthetic as isize);
}

fn shift_mapped_column(col: Option<&mut Option<isize>>, delta: isize) {
    if let Some(Some(col)) = col {
        *col += delta;
    }
}
