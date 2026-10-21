use super::*;

pub(in crate::parse) fn needs_quote_marker_line_map(source: &str) -> bool {
    if !source.contains('+') {
        return false;
    }
    let mut previous_quote = false;
    for line in source.lines() {
        if previous_quote && is_plus_marker(trim_ascii_start(line)) {
            return true;
        }
        previous_quote = strip_blockquote_prefix(trim_ascii_start(line)).is_some()
            || detect_list_marker_full(line)
                .is_some_and(|marker| strip_blockquote_prefix(marker.content).is_some());
    }
    false
}

/// Where the block parser starts DOCUMENT-LEVEL blocks in `lines`, with no
/// block attribute pending, plus the end of a closed container opening on line 0.
///
/// Container and item bodies are collected but never parsed: a top-level block
/// boundary is decided before any body is, so these are the parser's own.
pub(in crate::parse) struct DocumentBlockStarts {
    pub(in crate::parse) starts: Vec<usize>,
    pub(in crate::parse) closed_initial_container_end: Option<usize>,
}

pub(in crate::parse) fn document_block_starts<'a>(
    lines: &'a [&'a str],
    options: &Options<'_>,
) -> DocumentBlockStarts {
    let _recording = definition_regions::RecordingPause::enter();
    let _probing = ProbeGuard::enter();
    let _source_ending = SourceEndingGuard::fragment();
    let want_lines = options.source_lines || options.positions || needs_source_ending_map();
    let line_map: Vec<Option<usize>> = if want_lines {
        (1..=lines.len()).map(Some).collect()
    } else {
        Vec::new()
    };
    let col_map: Vec<Option<isize>> = if options.positions {
        vec![Some(0); lines.len()]
    } else {
        Vec::new()
    };
    let mut cursor = LineCursor::new_with_cols(
        lines,
        want_lines.then_some(line_map.as_slice()),
        options.positions.then_some(col_map.as_slice()),
    );
    cursor.at_document_level = true;
    cursor.reached = Some(&[]);
    cursor.block_starts = Some(Vec::new());
    parse_blocks(&mut cursor, options, &mut Vec::new());
    DocumentBlockStarts {
        starts: cursor.block_starts.take().unwrap_or_default(),
        closed_initial_container_end: cursor.closed_initial_container_end,
    }
}

#[derive(Clone, Copy)]
pub(in crate::parse) enum CursorLines<'a> {
    Slice(&'a [&'a str]),
    Colon {
        source: &'a [SourceLine],
        view: &'a ColonView,
        base: usize,
        materialized: &'a std::cell::OnceCell<Vec<&'a str>>,
    },
}

impl<'a> CursorLines<'a> {
    pub(in crate::parse) fn len(self) -> usize {
        match self {
            Self::Slice(lines) => lines.len(),
            Self::Colon { source, .. } => source.len(),
        }
    }

    pub(in crate::parse) fn at(self, index: usize) -> &'a str {
        self.line(index).expect("cursor line index")
    }

    pub(in crate::parse) fn range(
        self,
        range: std::ops::Range<usize>,
    ) -> impl DoubleEndedIterator<Item = &'a str> {
        range.map(move |index| self.at(index))
    }

    pub(in crate::parse) fn is_empty(self) -> bool {
        self.len() == 0
    }

    pub(in crate::parse) fn window<'b>(
        self,
        range: std::ops::Range<usize>,
        materialized: &'b std::cell::OnceCell<Vec<&'b str>>,
    ) -> CursorLines<'b>
    where
        'a: 'b,
    {
        match self {
            Self::Slice(lines) => CursorLines::Slice(&lines[range]),
            Self::Colon {
                source, view, base, ..
            } => CursorLines::Colon {
                source: &source[range.clone()],
                view,
                base: base + range.start,
                materialized,
            },
        }
    }

    pub(in crate::parse) fn colon_end(self, start: usize) -> Option<usize> {
        if !reuse_colon_views() {
            return None;
        }
        let Self::Colon {
            source, view, base, ..
        } = self
        else {
            return None;
        };
        let &(end, closed) = view.source.boundaries.get(base + start)?.as_ref()?;
        (end <= base + source.len() && (!closed || end < base + source.len()))
            .then_some(end - base + usize::from(closed))
    }

    pub(in crate::parse) fn code_after(
        self,
        start: usize,
        character: u8,
        width: usize,
    ) -> Option<bool> {
        if !reuse_colon_views() {
            return None;
        }
        match self {
            Self::Slice(_) => None,
            Self::Colon {
                source, view, base, ..
            } => Some(view.source.fence_closers().code_in(
                base + start + 1..base + source.len(),
                character,
                width,
            )),
        }
    }

    pub(in crate::parse) fn comment_after(self, start: usize, width: usize) -> Option<bool> {
        if !reuse_colon_views() {
            return None;
        }
        match self {
            Self::Slice(_) => None,
            Self::Colon {
                source, view, base, ..
            } => Some(
                view.source
                    .fence_closers()
                    .comment_in(base + start..base + source.len(), width),
            ),
        }
    }

    pub(in crate::parse) fn line(self, index: usize) -> Option<&'a str> {
        match self {
            Self::Slice(lines) => lines.get(index).copied(),
            Self::Colon { source, .. } => source.get(index).map(|line| line.text.as_str()),
        }
    }
}

impl<'a> std::ops::Deref for CursorLines<'a> {
    type Target = [&'a str];

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Slice(lines) => lines,
            Self::Colon {
                source,
                materialized,
                ..
            } => materialized.get_or_init(|| {
                #[cfg(test)]
                COLON_REFERENCE_ENTRIES.with(|count| count.set(count.get() + source.len()));
                source.iter().map(|line| line.text.as_str()).collect()
            }),
        }
    }
}

pub(in crate::parse) struct LineCursor<'a> {
    pub(in crate::parse) colon_view: Option<&'a ColonView>,
    pub(in crate::parse) colon_geometry: Option<&'a [SourceLine]>,
    pub(in crate::parse) marker_tail: Option<MarkerTail>,
    pub(in crate::parse) lines: CursorLines<'a>,
    pub(in crate::parse) line_map: Option<&'a [Option<usize>]>,
    /// Columns already stripped from the front of each line by an enclosing
    /// container, so a nested strip accumulates rather than resetting. `None`
    /// for a line whose stripped width is not known - a block starting there
    /// gets no position rather than a wrong one.
    pub(in crate::parse) col_map: Option<&'a [Option<isize>]>,
    pub(in crate::parse) pos: usize,
    pub(in crate::parse) last_block_start: Option<usize>,
    /// Every line this cursor started a block on with no block attributes
    /// pending, recorded only for [`document_block_starts`].
    pub(in crate::parse) block_starts: Option<Vec<usize>>,
    /// Parsed top-level block extents in this cursor's line coordinates.
    /// Collected only when a list item needs to derive looseness from the
    /// blocks it actually accepted, including opaque fence payloads.
    pub(in crate::parse) block_extents: Option<Vec<(usize, usize)>>,
    pub(in crate::parse) closed_initial_container_end: Option<usize>,
    pub(in crate::parse) at_document_level: bool,
    pub(in crate::parse) in_quoted_body: bool,
    /// True for the cursor reading a LIST ITEM's own body, and that body only.
    pub(in crate::parse) in_item_body: bool,
    /// Whether the INVISIBLE arms of `interrupts_paragraph` are live for the
    /// question being asked. True everywhere except a container's BELOW-COLUMN
    /// band, where §10 I5 makes a block-attribute line lazy paragraph text of
    /// that container rather than a floating attribute (§10 I5 DEFINITION
    /// OWNERSHIP IS COLUMN-SCOPED, markup-carve/carve#1809).
    ///
    /// PASSED, NOT INHERITED, for the reason `in_item_body` gives: it describes
    /// one question about one line, not the region. Only
    /// `interrupts_paragraph_in_band` clears it, and it restores it.
    pub(in crate::parse) invisible_arms: bool,
    /// Did each line of the source this cursor reads REACH the content column
    /// of the container that collected it? `None` at the document level and
    /// wherever the answer was not carried down.
    ///
    /// THE COLLECTOR'S DEDENT DESTROYS THE COLUMN, so a nested re-parse cannot
    /// recompute it: a line one column past a container's column and a line
    /// below it both arrive with a single residual column. `MappedSource`
    /// records the answer per line and this is where the nested parse reads it
    /// (markup-carve/carve-rs#1518, the same fact markup-carve/carve#1896 added
    /// `MappedSource::reached` for one layer up).
    pub(in crate::parse) reached: Option<&'a [bool]>,
    /// This cursor reads a FOOTNOTE or DEFINITION body, where an over-indented
    /// sublist carries its own authored base: a list marker past the body's
    /// content column anchors its span at the marker instead of at the placing
    /// indent (see `MappedSource::sublists_carry_authored_base`,
    /// markup-carve/carve#1980). Never inherited by a nested container body,
    /// which collects into its own `MappedSource`.
    pub(in crate::parse) sublists_carry_authored_base: bool,
    pub(in crate::parse) comment_closer_last_index: Option<HashMap<usize, usize>>,
    pub(in crate::parse) future_quoted_comments: RefCell<Option<FutureQuotedComments>>,
    pub(in crate::parse) literal_comments: Option<ContainerCommentClosers>,
    pub(in crate::parse) literal_code: LiteralCodeLookup,
    /// Last consumed comment closer and the column of its opener.
    pub(in crate::parse) closed_comment_location: Option<(usize, usize)>,
    /// The last collected item body closed its marker-line colon container.
    pub(in crate::parse) marker_colon_closed: bool,
    pub(in crate::parse) code_closer_last_index: Option<HashMap<u8, Vec<usize>>>,
    pub(in crate::parse) list_fence_closer_last_index: Option<HashMap<u8, Vec<usize>>>,
    pub(in crate::parse) list_code_closers: Option<Box<ListCodeClosers>>,
}

impl<'a> LineCursor<'a> {
    pub(in crate::parse) fn new_with_cols(
        lines: &'a [&'a str],
        line_map: Option<&'a [Option<usize>]>,
        col_map: Option<&'a [Option<isize>]>,
    ) -> Self {
        Self::from_lines(CursorLines::Slice(lines), line_map, col_map)
    }

    pub(in crate::parse) fn from_lines(
        lines: CursorLines<'a>,
        line_map: Option<&'a [Option<usize>]>,
        col_map: Option<&'a [Option<isize>]>,
    ) -> Self {
        LineCursor {
            colon_view: None,
            colon_geometry: None,
            marker_tail: None,
            lines,
            line_map,
            col_map,
            pos: 0,
            last_block_start: None,
            block_starts: None,
            block_extents: None,
            closed_initial_container_end: None,
            at_document_level: false,
            in_quoted_body: false,
            in_item_body: false,
            invisible_arms: true,
            reached: None,
            sublists_carry_authored_base: false,
            comment_closer_last_index: None,
            future_quoted_comments: RefCell::new(None),
            literal_comments: None,
            literal_code: LiteralCodeLookup::default(),
            closed_comment_location: None,
            marker_colon_closed: false,
            code_closer_last_index: None,
            list_fence_closer_last_index: None,
            list_code_closers: None,
        }
    }

    pub(in crate::parse) fn peek(&self) -> Option<&'a str> {
        self.lines.line(self.pos)
    }
    pub(in crate::parse) fn consume(&mut self) -> Option<&'a str> {
        let line = self.peek();
        if line.is_some() {
            self.pos += 1;
        }
        line
    }
    pub(in crate::parse) fn eof(&self) -> bool {
        self.pos >= self.lines.len()
    }
    pub(in crate::parse) fn source_line(&self, pos: usize) -> Option<usize> {
        if let Some(lines) = self.colon_geometry {
            return lines.get(pos).and_then(|line| line.source_line());
        }
        self.line_map
            .and_then(|map| map.get(pos).copied().flatten())
    }

    pub(in crate::parse) fn continuation_decision(
        &self,
        pos: usize,
    ) -> Option<continuation_marker::Decision> {
        self.source_line(pos)
            .and_then(continuation_marker::decision)
    }

    /// Columns stripped from the front of the line at `pos`, when known.
    pub(in crate::parse) fn source_col(&self, pos: usize) -> Option<isize> {
        if let Some(lines) = self.colon_geometry.filter(|_| self.col_map.is_some()) {
            return lines.get(pos).and_then(|line| line.stripped);
        }
        self.col_map.and_then(|map| map.get(pos).copied().flatten())
    }

    /// Did the line at `pos` reach the content column of the container that
    /// collected it?
    ///
    /// The carried answer wins when there is one. Without it the local column
    /// arithmetic is the best available reading, which is what every frame did
    /// before the flag was threaded - and what made a line that reached the
    /// OUTER item indistinguishable from one that reached nothing.
    pub(in crate::parse) fn line_reached(
        &self,
        pos: usize,
        indent: usize,
        strip_cols: usize,
    ) -> bool {
        match self.reached.and_then(|flags| flags.get(pos).copied()) {
            Some(reached) => reached,
            None => indent >= strip_cols,
        }
    }

    /// `line_reached` WITHOUT the local fallback: `None` at the outermost frame,
    /// where no enclosing collection recorded an answer.
    ///
    /// The distinction matters where the fallback and the absence of a carried
    /// flag mean opposite things. A degraded fence ends a frame for a line an
    /// ancestor already claimed, and at the outermost frame there is no ancestor
    /// to have claimed one.
    pub(in crate::parse) fn carried_reach(&self, pos: usize) -> Option<bool> {
        self.reached.and_then(|flags| flags.get(pos).copied())
    }

    /// Is there a comment-fence closer of exactly `fence_len` at or after `start`?
    ///
    /// A closer must match the opener width EXACTLY, so ANY later line carrying a
    /// fence of that width IS a valid closer: the question is exactly "last index
    /// for this width >= start". One pass builds the width -> last index map and
    /// every lookup after that is O(1).
    ///
    /// There used to be a per-width negative cache in front of this map. It could
    /// never change an outcome: the map already answers in O(1), and its own hit
    /// condition (a second opener of the same width after a proven-no-closer
    /// point) is unreachable, because a second line of the same width IS a closer
    /// for the first.
    /// Whether any line after `start` could close a code fence of this char and
    /// width. Deliberately OVER-approximate: it ignores indentation, which the
    /// real closer test does not. A `false` is therefore final and a `true` only
    /// means "worth scanning", so the caller keeps its exact scan behind this.
    ///
    /// Without it the exact scan runs from every opener to the end of the
    /// document, which is quadratic in the number of unterminated openers - the
    /// same shape `comment_closer_last_index` was added to remove for `%%%`.
    /// Code fences never got the equivalent.
    pub(in crate::parse) fn has_code_closer_after(
        &mut self,
        start: usize,
        fence_char: u8,
        fence_len: usize,
    ) -> bool {
        if let Some(possible) = self.lines.code_after(start, fence_char, fence_len) {
            return possible;
        }
        if self.code_closer_last_index.is_none() {
            self.code_closer_last_index = Some(build_code_closer_last_index(&self.lines));
        }
        self.code_closer_last_index
            .as_ref()
            .is_some_and(|index| code_closer_exists_after(index, start, fence_char, fence_len))
    }

    pub(in crate::parse) fn has_list_fence_closer_after(
        &mut self,
        start: usize,
        open: FenceOpen,
    ) -> bool {
        if self.list_fence_closer_last_index.is_none() {
            let mut lines = HashMap::<u8, Vec<usize>>::new();
            for index in 0..self.lines.len() {
                let line = strip_lazy(self.lines.at(index));
                if indent_columns(line) != 0 {
                    continue;
                }
                let bytes = line.as_bytes();
                let Some(&fence_char @ (b'`' | b'~')) = bytes.first() else {
                    continue;
                };
                let run = bytes.iter().take_while(|&&byte| byte == fence_char).count();
                if !bytes[run..]
                    .iter()
                    .all(|&byte| byte == b' ' || byte == b'\t')
                {
                    continue;
                }
                let by_len = lines.entry(fence_char).or_default();
                if by_len.len() <= run {
                    by_len.resize(run + 1, 0);
                }
                by_len[run] = index + 1;
            }
            for by_len in lines.values_mut() {
                for len in (0..by_len.len().saturating_sub(1)).rev() {
                    by_len[len] = by_len[len].max(by_len[len + 1]);
                }
            }
            self.list_fence_closer_last_index = Some(lines);
        }
        self.list_fence_closer_last_index
            .as_ref()
            .and_then(|lines| lines.get(&open.fence_char))
            .and_then(|by_len| by_len.get(open.fence_len))
            .is_some_and(|&last| last > start + 1)
    }

    pub(in crate::parse) fn has_comment_closer_after(
        &mut self,
        start: usize,
        fence_len: usize,
    ) -> bool {
        if let Some(possible) = self.lines.comment_after(start, fence_len) {
            return possible;
        }
        if self.comment_closer_last_index.is_none() {
            self.comment_closer_last_index = Some(build_comment_closer_last_index(&self.lines));
        }
        self.comment_closer_last_index
            .as_ref()
            .and_then(|last_index| last_index.get(&fence_len).copied())
            .is_some_and(|last| last >= start)
    }
}
