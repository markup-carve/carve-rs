use super::*;

/// The content column of the innermost list item a line-based prepass is inside.
///
/// Both definition prepasses need it for the same reason: a definition on an
/// item's CONTINUATION line carries no marker, so `strip_container_prefixes`
/// leaves the item's indentation in front of the `[` and the line stops looking
/// like a definition. Stripping exactly this many columns - never more - is what
/// separates a definition AT the content column (collected) from one below it
/// (paragraph text that registers nothing, PART 9 §24 C3 as carve#624 states it).
///
/// A line-based approximation, as the note in `extract_link_defs` says: tab-vs-
/// space marker alignment is char-counted, the post-blank `baseIndent + 2` rule
/// is not modeled, and lists inside blockquotes are not fully modeled.
#[derive(Default)]
pub(super) struct ContentColumns {
    /// One frame per open BLOCKQUOTE level, `frames[0]` being document level.
    /// Each frame holds the list content columns live inside that container.
    ///
    /// A flat stack could not answer this: cases 1 and 2 of carve-rs#593 need an
    /// item's columns DISCARDED when a quote opens under it, cases 3 and 4 need
    /// them KEPT across a quote nested at the item's own column - and the indent
    /// a flat stack compares against is in a different coordinate system
    /// depending on how deep the quote nesting is. Scoping by container answers
    /// "which columns are live" from structure instead of from an indent
    /// measured in whichever coordinate the caller happened to strip to.
    pub(super) frames: Vec<ColumnFrame>,
    pub(super) prev_blank: bool,
}

/// One container's live content columns, plus the definition list open inside
/// it.
///
/// A DEFINITION BODY IS A CONTAINER WITH A CONTENT COLUMN, like a list item
/// (PART 9 §24 C3, markup-carve/carve#1350). The columns alone could not say so:
/// a `:  ` line opens a body whose content column is three past the marker, and
/// a definition written there is that body's block - so it registers, and it
/// interrupts. Without the open list recorded, a bare `:  x` line anywhere would
/// claim a column that no `dd` was ever opened at.
#[derive(Default)]
pub(super) struct ColumnFrame {
    pub(super) cols: Vec<usize>,
    /// Content columns opened by definition descriptions among `cols`.
    pub(super) body_columns: Vec<usize>,
    pub(super) list_columns: Vec<(usize, usize)>,
    pub(super) first_list_col: Option<(usize, usize)>,
    pub(super) first_list_start: Option<usize>,
    /// The indent of the innermost OPEN definition list, set by its `:: ` term.
    pub(super) def_list: Option<usize>,
}

impl ColumnFrame {
    pub(super) fn pop_column(&mut self) {
        let popped = self.cols.pop();
        if let Some(column) = popped {
            if self.body_columns.last() == Some(&column) {
                self.body_columns.pop();
            }
            if self
                .list_columns
                .last()
                .is_some_and(|(_, floor)| *floor == column)
            {
                self.list_columns.pop();
            }
        }
        if popped == self.first_list_col.map(|(_, column)| column) {
            self.first_list_col = None;
            self.first_list_start = None;
        }
    }
}

impl ContentColumns {
    pub(super) fn new() -> Self {
        Self {
            frames: vec![ColumnFrame::default()],
            prev_blank: true,
        }
    }

    /// Feed the next RAW line (quote markers included). `opaque` suppresses
    /// tracking inside a fence or a line block, where `- verse` is content
    /// rather than a marker and a `>` is text rather than a container.
    ///
    /// THE COLUMN IS REACHED BY COMPOSING THE STRIPS, NOT BY WALKING THE
    /// PREFIX (PART 1 S4). A `>` written at an item's content column is that
    /// item's container prefix exactly as a flush-left one is, so the walk
    /// peels ONE level at a time and records each level's markers in that
    /// level's own frame. Counting only the flush-left `>` runs left every
    /// list opened inside an indented quote invisible: `- > - - x` recorded
    /// the outer item and nothing else, so a definition at the inner item's
    /// content column read as lazy text - while the same body with one
    /// container peeled off (`> - - x`) registered (carve-rs#1096).
    pub(super) fn observe(&mut self, raw_line: &str, opaque: bool) -> usize {
        self.observe_indexed(raw_line, opaque, None)
    }

    pub(super) fn observe_indexed(
        &mut self,
        raw_line: &str,
        opaque: bool,
        line_index: Option<usize>,
    ) -> usize {
        let bare = without_blockquote_prefixes(raw_line);
        let was_prev_blank = self.prev_blank;
        self.prev_blank = trim_ascii(bare).is_empty();
        if opaque {
            return self.current().cols.last().copied().unwrap_or(0);
        }
        // A blank line opens no container, so it is fed to a frame as it stands
        // rather than walked. Whether it CLOSES one depends on how it is
        // written, and the two spellings part here.
        //
        // A BARE blank ends every open quote, and every column opened inside one
        // dies with it (PART 0, A NEW MARKER DOES NOT REACH A DEAD CONTAINER'S
        // COLUMN; carve#1892). The parser already reads it that way - `> - x`
        // over a blank over `>   [r]: /u` renders as two separate quotes - but
        // the frames outlived the quote that held them, so the second `>` line
        // reached the dead item's column, and its definition registered
        // document-wide while the page printed it as ordinary text. I5 permits
        // neither reading to do both. Level 0 is document level and survives:
        // a list item IS transparent across a blank.
        //
        // A QUOTE-MARKED blank (`>`) closes nothing - it is the quote's own
        // continuation - so its frames stand.
        if trim_ascii(bare).is_empty() {
            if trim_ascii(raw_line).is_empty() {
                self.frames.truncate(1);
            }
            let level = self.frames.len() - 1;
            self.observe_segment(level, bare, was_prev_blank, line_index);
            return self.current().cols.last().copied().unwrap_or(0);
        }
        let mut rest = raw_line;
        let mut level = 0usize;
        let mut quote_at_content_column = false;
        loop {
            while let Some(inner) = strip_blockquote_prefix(rest) {
                // A quote reached through an item content column stays in the item.
                // A flush quote at that depth leaves the item for a sibling quote.
                if !quote_at_content_column
                    && self
                        .frames
                        .get(level)
                        .is_some_and(|frame| !frame.cols.is_empty())
                {
                    self.frames.truncate(level + 1);
                    self.frames[level] = ColumnFrame::default();
                }
                rest = inner;
                level += 1;
                quote_at_content_column = false;
            }
            while self.frames.len() <= level {
                self.frames.push(ColumnFrame::default());
            }
            let prior_columns = self.frames[level].cols.len();
            self.observe_segment(level, rest, was_prev_blank, line_index);
            if self.frames[level].cols.len() < prior_columns {
                self.frames.truncate(level + 1);
            }
            let Some(offset) = self.next_quote_offset(level, rest) else {
                break;
            };
            rest = &rest[offset..];
            quote_at_content_column = true;
        }
        self.frames.truncate(level + 1);
        self.current().cols.last().copied().unwrap_or(0)
    }

    /// Where this level hands off to the NEXT container level, if it does: the
    /// byte offset of a `>` that is this level's own content column rather than
    /// text sitting in it.
    ///
    /// Two spellings reach the same column and both are container prefixes.
    /// `- > x` writes the quote straight after the marker, where the item's
    /// content begins. `  > x` writes it at the column that marker established,
    /// which is the spelling every CONTINUATION line uses - and the one a
    /// flush-left walk can never see.
    pub(super) fn next_quote_offset(&self, level: usize, line: &str) -> Option<usize> {
        let mut inside = line;
        let mut consumed_marker = false;
        while let Some(marker) = detect_list_marker_full(inside) {
            if marker.content.as_ptr() as usize <= inside.as_ptr() as usize {
                break;
            }
            inside = marker.content;
            consumed_marker = true;
        }
        if consumed_marker {
            // The item's content begins exactly here, so a `>` at this position
            // is written AT the content column.
            return inside
                .starts_with('>')
                .then(|| inside.as_ptr() as usize - line.as_ptr() as usize);
        }
        // A description's marker line (`: > - para`) hands off the same way.
        if let Some((indent, width)) =
            detect_prepass_def_body(line).filter(|(at, _)| self.frames[level].def_list == Some(*at))
        {
            let offset = indent + width;
            return line[offset..].starts_with('>').then_some(offset);
        }
        // EXACTLY a live content column, never past one: an indented `>` that
        // reaches no open item is ordinary text, and the block parser reads it
        // that way too (carve-rs#1082).
        let content_col = self.reached_by_at(level, leading_ws(line));
        let bytes = line.as_bytes();
        (content_col > 0
            && bytes.len() > content_col
            && bytes[..content_col].iter().all(|b| *b == b' ')
            && bytes[content_col] == b'>')
            .then_some(content_col)
    }

    /// Record ONE container level's own markers. `line` is the raw line with
    /// every prefix up to and including this level's quote markers removed, so
    /// its columns are measured inside that container.
    pub(super) fn observe_segment(
        &mut self,
        level: usize,
        line: &str,
        was_prev_blank: bool,
        line_index: Option<usize>,
    ) {
        // Ownership is measured in visual columns. A leading tab reaches
        // column four and must not close a definition body at column three.
        let indent = indent_columns(line);
        let raw_trimmed = trim_ascii(line);
        if line_index.is_some()
            && !raw_trimmed.is_empty()
            && self.frames[level]
                .first_list_col
                .is_some_and(|(base, floor)| {
                    indent < base || (indent < floor && !is_list_marker(raw_trimmed))
                })
        {
            self.frames[level].first_list_col = None;
            self.frames[level].first_list_start = None;
        }
        let starts_block = is_heading_marker_line(raw_trimmed)
            || standalone_attrs_block_len(&[raw_trimmed]).is_some()
            || raw_trimmed.starts_with('>')
            || detect_fence_open(raw_trimmed).is_some()
            || detect_thematic_break(raw_trimmed)
            // THE TABLE, which §24 C3 names in the same block-opener set as the
            // rest of this list and which was the one member missing. A row at
            // the document column ends the list above it, so the columns that
            // list opened are dead - left live, `* }` / `|b|` / `  [f]: t`
            // collected a definition at a column nothing reaches any more, and
            // the author's line vanished from the output (carve-rs#2096).
            || is_table_start(raw_trimmed);
        if let Some(term_indent) = detect_prepass_def_term(line) {
            // A term opens the list (or re-opens it at a new indent). Its own
            // line has no content column: `:: t` holds the TERM, and only the
            // `:  ` line below it opens a body.
            let frame = &mut self.frames[level];
            while frame.cols.last().is_some_and(|col| *col > term_indent) {
                frame.pop_column();
            }
            frame.def_list = Some(term_indent);
        } else if let Some((body_indent, marker_width)) =
            detect_prepass_def_body(line).filter(|(at, _)| self.frames[level].def_list == Some(*at))
        {
            // `:  ` is three columns wide, so the body's content begins three
            // past the marker - the same arithmetic the collector does when it
            // slices the continuation lines it takes.
            let frame = &mut self.frames[level];
            while frame.cols.last().is_some_and(|col| *col > body_indent) {
                frame.pop_column();
            }
            frame.cols.push(body_indent + marker_width);
            frame.body_columns.push(body_indent + marker_width);
        } else if let Some((marker_indent, marker_width)) = detect_prepass_list_marker(line) {
            let frame = &mut self.frames[level];
            while frame.cols.last().is_some_and(|col| *col > marker_indent) {
                frame.pop_column();
            }
            // One line can open SEVERAL items: `- - a` opens an outer item
            // whose content is another item, so BOTH content columns are
            // live under it (2 and 4). Recording only the outer one left a
            // definition at the INNER column looking like text, so it
            // registered nothing here while carve-js and carve-php read it
            // as that item's block (carve#655).
            let mut offset = marker_width;
            frame.cols.push(offset);
            if level > 0 {
                frame.list_columns.push((marker_indent, offset));
            }
            if frame.first_list_col.is_none() {
                frame.first_list_start = line_index;
            }
            frame.first_list_col.get_or_insert((marker_indent, offset));
            while let Some((nested_indent, nested_width)) =
                detect_prepass_list_marker(&line[offset..])
            {
                if nested_indent != 0 {
                    break;
                }
                let marker_column = offset;
                offset += nested_width;
                self.frames[level].cols.push(offset);
                if level > 0 {
                    self.frames[level]
                        .list_columns
                        .push((marker_column, offset));
                }
            }
        } else if !raw_trimmed.is_empty() && (was_prev_blank || starts_block) {
            let frame = &mut self.frames[level];
            while frame.cols.last().is_some_and(|col| *col > indent) {
                frame.pop_column();
            }
            // A line that dedents to or past the term's own column has left the
            // definition list, so the next `:  ` line down there opens no body
            // until another term does.
            if frame.def_list.is_some_and(|at| indent <= at) {
                frame.def_list = None;
            }
        }
    }

    pub(super) fn current(&self) -> &ColumnFrame {
        self.frames
            .last()
            .expect("the document frame is never popped")
    }

    /// The content column of the open item a line at `indent` actually reaches:
    /// the deepest one at or below it, or 0 when it reaches none.
    ///
    /// Not always the innermost. Under `- - a` a definition written at column 2
    /// belongs to the outer item, and one written between the two live columns
    /// belongs to the outer of them - the deeper item never opened on that
    /// line, so it cannot own it (markup-carve/carve#1896).
    ///
    /// Only the INNERMOST container's columns are consulted: a column measured
    /// outside the quote a line sits in is not a column that line can reach.
    pub(super) fn reached_by(&self, indent: usize) -> usize {
        self.reached_by_at(self.frames.len() - 1, indent)
    }

    /// `reached_by`, asked of a NAMED container level rather than the innermost
    /// one. The composed walk needs it: a `>` written at an outer item's content
    /// column is measured against THAT item's columns, and the innermost frame's
    /// columns are in a different coordinate system entirely.
    pub(super) fn reached_by_at(&self, level: usize, indent: usize) -> usize {
        self.frames
            .get(level)
            .and_then(|frame| {
                frame
                    .cols
                    .iter()
                    .copied()
                    .filter(|col| *col <= indent)
                    .max()
            })
            .unwrap_or(0)
    }
}
