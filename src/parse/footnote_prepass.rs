use super::definition_regions::VerseLine;
use super::*;

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
    let mut i = 0;
    while i < lines.len() {
        i = scan.step(i);
    }
    scan.finish()
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
    closer_ahead: bool,
}

impl PrepassFenceState {
    fn ends_before(&self, line: &str, open: FenceOpen) -> bool {
        inline_fence_paragraph_ends(line, open, self.inline_paragraph.as_ref())
            || (self.description
                && (self.description_blank || !self.inline_marker)
                && description_fence_leaves_after_blank(line, open, &self.term_columns))
            || !inline_fence_lazy_line(line, self.inline_paragraph.as_ref())
                && ((self.inline_marker && is_blank_line(without_blockquote_prefixes(line)))
                    || (self.term_owned
                        && term_fence_ends_above(line, self.term_floor, &self.term_columns))
                    || prepass_fence_ends_above(line, open, self.closer_ahead))
    }
}

/// One definition's body as it is gathered: its lines, the document line each
/// came from, and (with positions on) the column each was stripped at.
struct NoteBody {
    lines: Vec<String>,
    line_map: Vec<Option<usize>>,
    col_map: Vec<Option<isize>>,
    positions: bool,
}

impl NoteBody {
    fn push_with(
        &mut self,
        line: String,
        source_line: Option<usize>,
        col: impl FnOnce(&str) -> Option<isize>,
    ) {
        if self.positions {
            self.col_map.push(col(&line));
        }
        self.lines.push(line);
        self.line_map.push(source_line);
    }

    fn push_blank(&mut self) {
        self.push_with(String::new(), None, |_| None);
    }
}

/// What one document line looks like once its container prefix is stripped.
struct LineScan<'a> {
    stripped: StrippedContainerLine<'a>,
    after_term: bool,
    retained_unclosed_fence: bool,
    fence_line: &'a str,
    /// The column of a fence written at a list item's content column.
    fence_col: Option<usize>,
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
    note_fence: Option<FenceOpen>,
    // Is this note's body still open for an INDENTED definition below
    // it? A flush line opens it and a block opener closes it. Only
    // whether, not where: a footnote definition registers wherever it
    // lands and is hoisted either way, so no column bound is enforced.
    body_open_below: bool,
    open_term: bool,
    retained_below_comment: bool,
    below_comment_span: Option<usize>,
    lazy_open: bool,
    /// The definition line starts at column 0, so no container hosts the note.
    flush_def: bool,
}

/// The quote / list / description host a container-hosted note sits in.
struct NoteHost {
    quote_indents: Vec<usize>,
    quote_depth: usize,
    prefix_cols: usize,
    note_floor: usize,
    marker_note: bool,
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

    // A marker note admits comments at its host column.
    fn reaches_floor(&self, raw: &str, admitted_fence: bool, comment_open: bool) -> bool {
        self.content(raw).is_some_and(|line| {
            admitted_fence
                || comment_open
                || indent_columns(line) >= self.note_floor
                || (self.quote_depth > 0
                    && indent_columns(line) > 0
                    && trim_ascii_start(line).starts_with("%%"))
                || (self.marker_note
                    && indent_columns(line) >= self.prefix_cols
                    && is_line_comment_any_column(line))
        })
    }
}

/// Comment and fence opacity of a container-hosted note body.
struct HostedWalk {
    track: bool,
    opacity: BodyCommentState,
    admitted_fence: Option<(FenceOpen, usize)>,
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
    // Where each definition was written. Lines and columns are final here; the
    // OFFSETS are filled by the caller, which is the only place that holds the
    // original text the offsets are measured in - the same split `fill_offsets`
    // already uses for every other block.
    def_positions: BTreeMap<String, Pos>,
    note_link_defs: BTreeMap<String, LinkDef>,
    body_opacity_candidates: Option<bool>,
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
    comment_fence_closers: HashMap<usize, usize>,
    closer_maxima: PrepassFenceClosers,
    folded_into_term: TermLineOwnership,
    // Built on the first CONTAINER-scoped opener and never for a document that
    // has none, which is every document that only ever writes `%%%` at column 0.
    container_closers: Option<ContainerCommentClosers>,
    comment_closers: Option<HashMap<usize, usize>>,
    closer_index: Option<HashMap<u8, Vec<usize>>>,
    // See ContentColumns: a definition on an item's CONTINUATION line carries no
    // marker, so without this the line kept its indentation, stopped looking
    // like a definition, and was neither collected nor rendered - the author's
    // line disappeared and a reference to it stayed literal (carve-rs#568).
    columns: ContentColumns,
    // The line the `after_term` gate below asks about. See its note: a blank
    // line between entries is transparent to a definition list, so the gate
    // reads past one.
    previous_non_blank: &'a str,
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
        let comment_fence_closers = comment_fence_close_index(lines);
        let closer_maxima = fence_closer_suffix_maxima(lines);
        let folded_into_term = term_folded_lines(lines, &closer_maxima, options);
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
            def_positions: BTreeMap::new(),
            note_link_defs: BTreeMap::new(),
            body_opacity_candidates: None,
            fence: PrepassFenceState::default(),
            verse_lines,
            in_comment_fence: None,
            comment_fence_closers,
            closer_maxima,
            folded_into_term,
            container_closers: None,
            comment_closers: None,
            closer_index: None,
            columns: ContentColumns::new(),
            previous_non_blank: "",
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

    /// Handles line `i` and returns the index of the next unhandled line.
    fn step(&mut self, i: usize) -> usize {
        let scan = self.observe_line(i);
        if self.verse_lines[i].is_owned() {
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
        let folded_fence = detect_fence_open(trim_ascii_start(line)).is_some_and(|open| {
            !self.closer_maxima.has_run_after(i, open)
                && prepass_paragraph_open_above(lines, i, &[], &self.folded_into_term)
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
        let retained_unclosed_fence = i > 0
            && is_line_comment_any_column(lines[i - 1])
            && indent_columns(line) == 0
            && self.columns.current().cols.last().is_some()
            && detect_fence_open(line)
                .is_some_and(|open| !self.closer_maxima.has_run_after(i, open));
        let opaque = self.fence.open.is_some()
            || folded_fence
            || folded_marker
            || self.verse_lines[i].is_body()
            || self.in_comment_fence.is_some()
            || prepass_quote_line_is_folded(&self.folded_into_term, i, line)
            || retained_unclosed_fence;
        let content_col = self.columns.observe(line, opaque);
        // A quote nested in a list item sits AT the item's content column, so
        // the prefix scan needs that column to see it (carve-rs#588).
        //
        // THE PREVIOUS NON-BLANK LINE, not the one directly above: a blank
        // between description entries does not end the list, it only makes it
        // loose. Asking the line above left the `:  ` marker unstripped after
        // one, so a definition written as that description's body was consumed
        // by the parser and collected by nobody (#1500).
        let after_term = opens_definition_entry(self.previous_non_blank);
        let stripped = strip_container_prefixes_at(line, &self.columns, after_term);
        remember_prepass_paragraph_line(&self.folded_into_term, i, line, &stripped, &self.columns);
        if !is_blank_line(line) {
            self.previous_non_blank = line;
        }
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
            None if content_col > 0 && !stripped.description => {
                // As `extract_link_defs_with_guard_full`: a fence at an item's
                // content column is payload for both collectors.
                let kept = strip_container_prefixes_keep_indent(line);
                let kept_indent = leading_ws(&kept);
                let authored = trim_ascii_start(&kept);
                if kept_indent >= content_col
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
            retained_unclosed_fence,
            fence_line,
            fence_col,
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
        if let Some(open) = self.in_comment_fence {
            end_prepass_paragraph_at(&self.folded_into_term, i);
            // ANY column, matching `comment_fence_close_index` and the block
            // parser. A comment fence closes at whatever indent its closer sits
            // at (PART 9 §24 C3, markup-carve/carve#629), so testing the strict
            // form here made this pass disagree with the one that decides: the
            // block parser closed the comment, this scan did not, and every
            // definition after it went unregistered and came back as visible
            // text (#574 regression). Any column AT THE OPENER'S QUOTE DEPTH -
            // see `closes_open_comment_fence`.
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
        // THE CONTAINER BOUNDS THE SPAN, NOT THE DELIMITER. For a column
        // scope the bound ends the container at the first line that dedents
        // below the column the fence REACHES, so measuring from where the
        // `%%%` happens to be written read a legal body line as the end:
        // `- item` / `    %%%` / `  [r]: /u` / `    %%%` put the fence at 4
        // and its body at 2, still inside the item, and the fence was
        // declined (carve-rs#1054). For a quote scope the blank line is the
        // bound - see `CommentFenceScope`.
        if !comment_fence_scope_closes(
            scope,
            self.lines,
            i,
            open.fence_len,
            &self.columns,
            &self.comment_fence_closers,
            &mut self.container_closers,
        ) {
            return false;
        }
        end_prepass_paragraph_at(&self.folded_into_term, i);
        self.in_comment_fence = Some(OpenCommentFence {
            fence_len: open.fence_len,
            quote_depth: scope.quote_depth(),
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
            open,
            &self.closer_maxima,
            &self.folded_into_term,
            &self.columns,
            scan.after_term,
        );
        if fence.opens || (scan.stripped.description && term_fence.is_none()) {
            if scan.retained_unclosed_fence {
                open.content_col = self.columns.current().cols.last().copied().unwrap_or(0);
            }
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
                closer_ahead: fence.closer_ahead,
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
        let (label, first) = parse_footnote_def_line(def_line)?;
        // PART 9R R1a: a line that folds into an open paragraph defines nothing,
        // and a probe that cannot afford to say so collects nothing either. The
        // two are one condition here - the fallback is conservative, so it may
        // decline a definition an affordable probe would have taken.
        let mut unaffordable = false;
        let quoted_note_paragraph = self.nested
            && stripped.structural.contains('>')
            && self.body.last().is_some_and(|previous: &String| {
                strip_blockquote_prefix(trim_ascii_start(previous)).is_some_and(|content| {
                    !is_blank_line(content) && !item_block_opener(trim_ascii_start(content))
                })
            });
        let folds = marker_line_may_be_lazy(line)
            && line_folds_into_an_open_paragraph(
                &self.body,
                line,
                self.options,
                &mut self.probe_budget,
                &mut unaffordable,
            );
        if folds
            || quoted_note_paragraph
            || unaffordable
            || self.folded_into_term.folded.contains(&i)
        {
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
        def: DefinitionLine<'a>,
    ) -> usize {
        let stripped = &scan.stripped;
        let in_container = !stripped.structural.is_empty();
        let normalized_label = label_key(def.label);
        let first_for_label = !self.definition_keys.contains_key(&normalized_label);
        let def_start_line = self.first_source_line + i;
        let raw_def_line = self.lines[i];
        if self.positions && first_for_label {
            self.record_definition_start(def.label, raw_def_line, def.def_line, def_start_line);
        }
        let mut i = i + 1;
        let mut note = NoteBody {
            lines: vec![def.first.to_string()],
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
            positions: self.positions,
        };
        let body_indent = footnote_body_floor(raw_def_line);
        let container = IndentContainer {
            marker_column: indent_columns(def.first),
            content_column: body_indent,
            owner: continuation_marker::Kind::Footnote,
        };
        if !in_container {
            i = self.lone_plus_first_block(&mut note, i, def.first, container, def_start_line);
        }
        // An authored quote may be deeper than the item's content column.
        // Its prefix is still a container for note-body lines.
        if !in_container && !def.authored_quote_def {
            let state = IndentedNote {
                label: def.label,
                first: def.first,
                container,
                body_indent,
                // A marker starts the body, so a fence on it opens (PART 0 O1).
                note_fence: detect_fence_open(def.first).map(|mut open| {
                    open.content_col = body_indent;
                    open
                }),
                body_open_below: false,
                open_term: term_start_column(def.first).is_some(),
                retained_below_comment: false,
                below_comment_span: None,
                lazy_open: false,
                flush_def: indent_columns(raw_def_line) == 0,
            };
            i = self.collect_indented_body(&mut note, i, state);
        } else if !self.nested {
            let structural_len = if def.authored_quote_def {
                raw_def_line.len() - def.def_line.len()
            } else {
                stripped.structural.len()
            };
            i = self.collect_hosted_body(&mut note, i, raw_def_line, &def, structural_len);
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
        if first_for_label {
            self.register_definition(normalized_label, &def, raw_def_line, note);
        }
        self.push_definition_replacement(raw_def_line, &def, stripped, def_start_line);
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

    // A lone marker-line `+` gives the note its first flush-left block.
    // The same block bound is used by a later standalone `+` in a note.
    fn lone_plus_first_block(
        &mut self,
        note: &mut NoteBody,
        mut i: usize,
        first: &str,
        container: IndentContainer,
        def_start_line: usize,
    ) -> usize {
        let lines = self.lines;
        if trim_ascii(first) != "+"
            || !container.owns_continuation(first, self.decision_at(def_start_line))
            || !lines.get(i).is_some_and(|line| !is_blank_line(line))
        {
            return i;
        }
        let end = attached_block_lines_classified(
            CursorLines::Slice(lines),
            i,
            &mut self.comment_closers,
            &mut self.closer_index,
            self.options,
            &mut |line, _| {
                is_blank_line(line)
                    || is_plus_marker(line)
                    || (indent_columns(line) == 0 && is_list_marker(line))
            },
        );
        if end > i {
            note.lines[0].clear();
            note.line_map[0] = None;
            if self.positions {
                note.col_map[0] = None;
            }
            while i < end {
                note.push_with(
                    lines[i].to_string(),
                    Some(self.first_source_line + i),
                    |_| Some(0),
                );
                i += 1;
            }
            note.push_blank();
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
            && closes_a_held_comment_span(&note.lines, line)
        {
            note.push_with(
                kept_comment_delimiter(line),
                Some(self.first_source_line + *i),
                |kept| stripped_col(Some(0), line, kept),
            );
            *i += 1;
            return Flow::Next;
        }
        // Lazy continuation, the same rule as every container (ruling on
        // carve#3885): a line that interrupts no paragraph joins an open one.
        let rest = trim_ascii_start(line);
        if !state.open_term
            && !self.nested
            && state.flush_def
            && state.note_fence.is_none()
            && !interrupts_paragraph_with_rest(line, &self.lines[*i + 1..])
            && !is_colon_fence_opener_shape(line)
            && exact_colon_fence_len(rest).is_none()
            && parse_link_def_line(rest).is_none()
            && !is_list_marker(rest)
            && (lazy_open || body_ends_with_open_paragraph(&note.lines.join("\n"), self.options))
        {
            let kept = format!("{LAZY}{}", trim_ascii_start(line));
            note.push_with(kept, Some(self.first_source_line + *i), |_| {
                stripped_col(Some(0), line, trim_ascii_start(line))
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
        if below_comment
            && state.below_comment_span.is_none()
            && state.note_fence.is_none()
            && detect_comment_fence_line_any_column(line).is_none()
            && !self.lines[*i + 1..]
                .iter()
                .find(|next| !is_blank_line(next) && !trim_ascii_start(next).starts_with("%%"))
                .is_some_and(|next| indent_columns(next) >= state.body_indent)
        {
            return Some(Flow::Stop);
        }
        if below_comment
            && column == 0
            && state.below_comment_span.is_none()
            && detect_comment_fence_line_any_column(line).is_some_and(|open| {
                self.comment_fence_closers
                    .get(&open.fence_len)
                    .is_some_and(|&last| last > *i)
            })
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
        if state.below_comment_span.is_none()
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
            }
        } else if let Some(open) = detect_comment_fence_line_any_column(line) {
            if self
                .comment_fence_closers
                .get(&open.fence_len)
                .is_some_and(|&last| last > *i)
            {
                state.below_comment_span = Some(open.fence_len);
            }
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
        state.retained_below_comment = true;
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
        if !resumes {
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
        let mut attached: Vec<String> = Vec::new();
        let attached_start = *i;
        let refused_first_plus = self
            .decision_at(self.first_source_line + attached_start)
            .is_some_and(|decision| decision.refusal.is_some())
            && lines
                .get(attached_start + 1)
                .map_or(true, |line| is_blank_line(line));
        let end = attached_block_lines_classified(
            CursorLines::Slice(lines),
            *i,
            &mut self.comment_closers,
            &mut self.closer_index,
            self.options,
            &mut |a, at| {
                is_blank_line(a)
                    || (is_plus_marker(a) && !(at == attached_start && refused_first_plus))
                    || parse_footnote_def_line(a).is_some()
            },
        );
        while *i < end {
            attached.push(slice_columns(lines[*i], column, true));
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
        let dedented = strip_leading_columns(line, state.body_indent);
        let trimmed: &str = &dedented;
        // A CODE FENCE inside the body is opaque, so a
        // definition-shaped line in it is content. Every engine
        // already agrees about that at the top level - none
        // registers `[r]: /u` written inside ``` - and the
        // top-level pass in `extract_link_defs` tracks fences
        // for exactly this reason. Without the state here the
        // line was consumed as a definition and vanished from
        // the code block.
        if let Some(open) = state.note_fence {
            if is_fence_close(trimmed, open) {
                state.note_fence = None;
            }
        } else if let Some(open) = detect_fence_open(trimmed)
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
        } else if (!state.open_term || indent_columns(trimmed) == 0)
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
        let trimmed: &str =
            if state.body_open_below && is_note_def && state.note_fence.is_none() && residual < 2 {
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
    ) -> usize {
        let (quote_indents, inside) = note_quote_host(raw_def_line, structural_len);
        let prefix_cols = inside.chars().count() - def.def_line.chars().count();
        let host = NoteHost {
            quote_depth: quote_indents.len(),
            quote_indents,
            prefix_cols,
            note_floor: prefix_cols + footnote_body_floor(def.def_line),
            marker_note: detect_list_marker_full(inside)
                .is_some_and(|marker| parse_footnote_def_line(marker.content).is_some()),
        };
        let mut i = self.attach_quoted_plus_block(&host, note, i, raw_def_line);
        let source = self.source;
        let track = *self
            .body_opacity_candidates
            .get_or_insert_with(|| source.contains("%%%") || source.contains("~~~"));
        let mut walk = HostedWalk {
            track,
            opacity: BodyCommentState::new(),
            admitted_fence: None,
        };
        if walk.track {
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
        if resume_at >= self.lines.len() || !walk.reaches_floor(&host, self.lines[resume_at]) {
            return i;
        }
        while i < self.lines.len() {
            if self.hosted_line(&host, &mut walk, note, &mut i) == Flow::Stop {
                break;
            }
        }
        i
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
                        && decision.refusal.is_none()
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
        [host.note_floor, column]
            .into_iter()
            .any(|closer_column| self.closer_maxima.closes_after_at(at, open, closer_column))
    }

    fn next_hosted_non_blank(&self, host: &NoteHost, mut at: usize) -> usize {
        while at < self.lines.len() && host.blank(self.lines[at]) {
            at += 1;
        }
        at
    }

    fn hosted_line(
        &mut self,
        host: &NoteHost,
        walk: &mut HostedWalk,
        note: &mut NoteBody,
        i: &mut usize,
    ) -> Flow {
        let raw = self.lines[*i];
        let Some(line) = host.content(raw) else {
            return Flow::Stop;
        };
        if is_blank_line(line) {
            return self.hosted_blank_run(host, walk, note, i);
        }
        if !walk.reaches_floor(host, raw) {
            return Flow::Stop;
        }
        if host.marker_note && parse_footnote_def_line(trim_ascii_start(line)).is_some() {
            return Flow::Stop;
        }
        let dedented = strip_leading_columns(line, host.note_floor);
        // A link definition at the note's own column is metadata,
        // even when an open marker-line paragraph precedes it.
        let definition_text = note_link_def_candidate(&dedented, note);
        if indent_columns(line) >= host.note_floor
            && walk.admitted_fence.is_none()
            && walk.opacity.comment.is_none()
            && walk.opacity.code.is_none()
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
        if walk.opacity.comment.is_none() {
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
        note.push_with(dedented, Some(self.first_source_line + *i), |_| col);
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
            return Flow::Stop;
        }
        while *i < after {
            let raw = lines[*i];
            let line = host.content(raw).expect("checked quote prefix");
            let prefix = raw.chars().count() - line.chars().count();
            let (residue, consumed, synthetic) = slice_columns_mapped(line, host.note_floor, true);
            if walk.track {
                walk.opacity.observe(&residue);
            }
            note.push_with(residue, Some(self.first_source_line + *i), |_| {
                Some((prefix + consumed) as isize - synthetic as isize)
            });
            *i += 1;
        }
        Flow::Next
    }

    fn register_definition(
        &mut self,
        normalized_label: String,
        def: &DefinitionLine<'a>,
        raw_def_line: &str,
        note: NoteBody,
    ) {
        self.definition_keys
            .insert(normalized_label, def.label.to_string());
        let mut definition_source = MappedSource {
            col_map: note.col_map,
            source: note.lines.join("\n"),
            line_map: note.line_map,
            authored_base_at_start: false,
            item_marker_at_start: false,
            marker_colon_rebased: false,
            marker_tail: None,
            reached: vec![true; note.lines.len()],
            opaque_blank_lines: Vec::new(),
            discarded_outer_blank: false,
            outer_plus_after_blank: false,
            // A FOOTNOTE BODY carries authored base for its sublists, so
            // a list marker past the body's content column anchors its
            // span at the marker (markup-carve/carve#1980).
            in_quoted_body: false,
            sublists_carry_authored_base: true,
        };
        let absorbed_note_marker =
            detect_list_marker_full(without_blockquote_prefixes(raw_def_line))
                .is_some_and(|marker| parse_footnote_def_line(marker.content).is_some());
        if absorbed_note_marker || parse_footnote_def_line(trim_ascii_start(def.first)).is_none() {
            rebase_body_blocks(
                &mut definition_source,
                true,
                self.options,
                absorbed_note_marker,
                false,
            );
        }
        self.defs.insert(def.label.to_string(), definition_source);
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
            } else {
                definition_placeholder()
            });
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
                // The document body's lines are top-level: the footnote-definition
                // extraction removes whole lines, never a prefix, so nothing has
                // been stripped from the front of the ones that remain.
                col_map: if self.positions {
                    vec![Some(0); self.body.len()]
                } else {
                    Vec::new()
                },
                source: joined_source(&self.body),
                authored_base_at_start: false,
                item_marker_at_start: false,
                marker_colon_rebased: false,
                marker_tail: None,
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

fn note_link_def_candidate<'a>(line: &'a str, note: &NoteBody) -> &'a str {
    let text = trim_ascii_start(line);
    if !text.starts_with('>') {
        return text;
    }
    // A definition after an open quote paragraph is paragraph text. A new
    // quote, or one whose previous block ended, admits a definition at its
    // content column just like any other note-body container.
    let paragraph_open = note.lines.last().is_some_and(|previous| {
        strip_blockquote_prefix(trim_ascii_start(previous)).is_some_and(|content| {
            !is_blank_line(content) && !item_block_opener(trim_ascii_start(content))
        })
    });
    if paragraph_open {
        text
    } else {
        without_blockquote_prefixes(text)
    }
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
        if let Some(content) = detect_list_marker_full(inside)
            .map(|marker| marker.content)
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
