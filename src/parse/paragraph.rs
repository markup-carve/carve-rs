use super::*;

pub(super) fn detect_block_image(line: &str) -> Option<Image> {
    let (img, after) = image_and_tail(line)?;
    trim_ascii(after).is_empty().then_some(img)
}

/// The image a line opens with, and the text after it.
fn image_and_tail(line: &str) -> Option<(Image, &str)> {
    if !line.starts_with("![") {
        return None;
    }
    let bytes = line.as_bytes();
    let (bracket_matches, bracket_run_at) = compute_bracket_matches(bytes);
    // Block-image detection runs once on a single line (not in a per-position
    // loop), so full-slice last-occurrence scans are fine here.
    let bracket_openers = compute_bracket_openers(&bracket_matches);
    let bounds = InlineBounds {
        matches: &bracket_matches,
        openers: &bracket_openers,
        run_at: &bracket_run_at,
        last_close_paren: bytes.iter().rposition(|&b| b == b')'),
        last_close_brace: bytes.iter().rposition(|&b| b == b'}'),
        last_close_bracket: bytes.iter().rposition(|&b| b == b']'),
        last_gt: bytes.iter().rposition(|&b| b == b'>'),
        crossref_invalid_range: Cell::new(None),
        delim_brace: [None; DELIM_BRACE_SLOTS],
        braced_closers: RefCell::new(None),
        native_pairs: None,
        footnote_refs: true,
    };
    let (img, consumed) = parse_image_at(bytes, 0, &bounds)?;
    Some((img, &line[consumed..]))
}

/// A lone image whose line also holds comments: they render nothing, so the
/// line is still a caption host (18:317, carve#4568). A braced comment holds its
/// place until the line end is trimmed, so a space before it still renders.
pub(super) fn quiet_block_image(line: &str) -> Option<Image> {
    if !line.starts_with("![") || !line.contains('%') {
        return None;
    }
    let (img, after) = image_and_tail(line)?;
    (!trim_ascii(after).is_empty() && quiet_image_tail(after)).then_some(img)
}

/// Whether the text after an image is only comments, read as the model's
/// `isCaptionableParagraph` reads the line: braced comments hold a place, a
/// line comment after whitespace runs to the end, and the end is trimmed.
fn quiet_image_tail(tail: &str) -> bool {
    let mut quiet = String::with_capacity(tail.len());
    let mut rest = tail;
    while let Some(open) = rest.find("{%") {
        let Some(close) = rest[open + 2..].find("%}") else {
            break;
        };
        quiet.push_str(&rest[..open]);
        quiet.push('\u{1}');
        rest = &rest[open + 2 + close + 2..];
    }
    quiet.push_str(rest);
    if let Some(at) = quiet
        .match_indices("%%")
        .map(|(at, _)| at)
        .find(|&at| at > 0 && matches!(quiet.as_bytes()[at - 1], b' ' | b'\t'))
    {
        quiet.truncate(at);
    }
    quiet
        .trim_end_matches([' ', '\t'])
        .replace('\u{1}', "")
        .is_empty()
}

/// The figure a [`quiet_block_image`] line makes with the caption below it.
/// Without a caption nothing is consumed: promotion already renders the image.
pub(super) fn quiet_captioned_image(
    cur: &mut LineCursor,
    options: &Options<'_>,
    line: &str,
) -> Option<BlockNode> {
    if !line.contains('%') || !line.contains("![") {
        return None;
    }
    let image_at = cur.pos;
    let anchor = inline_anchor_for_line(cur, image_at, line);
    let inline = parse_inline_lines_with_anchor(line, options, vec![anchor]);
    let image_index = lone_rendered_image(&inline)?;
    let InlineNode::Image(img) = &inline[image_index] else {
        unreachable!()
    };
    if img.ref_label.is_some() {
        return None;
    }
    cur.consume();
    let target_pos = span_of(cur, image_at, cur.pos, options);
    let Some(caption) = consume_caption(cur, options) else {
        cur.pos = image_at;
        return None;
    };
    Some(BlockNode::Figure(Figure {
        attrs: None,
        target: Box::new(captioned_image_target(inline, target_pos, true)),
        rendered_target: None,
        caption,
        short_caption: None,
        pos: span_of(cur, image_at, cur.pos, options),
    }))
}

/// Keep the whole caption host when invisible siblings accompany its image.
/// The single-image wire shape remains an image target.
pub(super) fn captioned_image_target(
    mut children: Vec<InlineNode>,
    pos: Option<Pos>,
    at_content_column: bool,
) -> FigureTarget {
    if children.len() == 1 {
        let InlineNode::Image(image) = children.remove(0) else {
            unreachable!()
        };
        FigureTarget::Image(image)
    } else {
        FigureTarget::Paragraph(Paragraph {
            attrs: None,
            children,
            at_content_column,
            block_image: true,
            pos,
        })
    }
}

/// Comments render nothing when deciding whether a paragraph has a single
/// image or display-math caption host. Visible whitespace stays in the run.
pub(super) fn retain_caption_content(inline: &mut Vec<InlineNode>) {
    inline.retain(|node| !matches!(node, InlineNode::Comment(_)));
}

pub(super) fn reference_image_line(line: &str) -> bool {
    if line.contains('%') && line.contains("![") {
        let mut inline = parse_inline_with_options(line, &Options::default());
        retain_caption_content(&mut inline);
        return matches!(inline.as_slice(), [InlineNode::Image(img)] if img.ref_label.is_some());
    }
    if !line.starts_with("![") {
        return false;
    }
    let bytes = line.as_bytes();
    let (bracket_matches, bracket_run_at) = compute_bracket_matches(bytes);
    let bracket_openers = compute_bracket_openers(&bracket_matches);
    let bounds = InlineBounds {
        matches: &bracket_matches,
        openers: &bracket_openers,
        run_at: &bracket_run_at,
        last_close_paren: bytes.iter().rposition(|&b| b == b')'),
        last_close_brace: bytes.iter().rposition(|&b| b == b'}'),
        last_close_bracket: bytes.iter().rposition(|&b| b == b']'),
        last_gt: bytes.iter().rposition(|&b| b == b'>'),
        crossref_invalid_range: Cell::new(None),
        delim_brace: [None; DELIM_BRACE_SLOTS],
        braced_closers: RefCell::new(None),
        native_pairs: None,
        footnote_refs: true,
    };
    parse_reference_image(bytes, 0, &bounds)
        .is_some_and(|(_, consumed)| trim_ascii(&line[consumed..]).is_empty())
}

pub(super) fn parse_paragraph(cur: &mut LineCursor, options: &Options<'_>) -> BlockNode {
    let span_start = cur.pos;
    // A caption at the content column can attach to an indented image-only
    // paragraph. An indented caption remains literal paragraph text.
    let image_lead = cur.peek().is_some_and(|line| {
        let content = trim_ascii_start(line);
        detect_block_image(content).is_some() || reference_image_line(content)
    });
    let image_opener = cur
        .peek()
        .is_some_and(|line| trim_ascii_start(line).starts_with("!["));
    let mut at_content_column = cur.peek().is_some_and(|l| !l.starts_with([' ', '\t']));
    let reference_caption_candidate = cur.peek().is_some_and(reference_image_line);
    let mut reference_caption_claimed = false;
    let mut lines: Vec<&str> = Vec::new();
    let mut line_indices = Vec::new();
    // PART 10 I2: a code include line at a block start ends at its own line.
    let code_include = cur
        .peek()
        .and_then(|line| crate::includes::code_include_line(strip_lazy(trim_ascii_start(line))));
    // A refused opener is literal text; following lines use ordinary block rules.
    let mut literal: Vec<(usize, usize)> = Vec::new();
    let mut comment_scanned_through = 0usize;
    // A closed block-position `{%` run past its `{` holds every line it spans (CARVE-P2-035).
    let mut comment_run_through = 0usize;
    let mut inline_span = InlineSpanScan::default();
    while let Some(line) = cur.peek() {
        if code_include.is_some() && !lines.is_empty() {
            break;
        }
        let brace_line = trim_ascii_start(strip_lazy(line));
        if !is_blank_line(line)
            && brace_line.starts_with('{')
            && !inline_span.open(&lines)
            && !(brace_line.starts_with("{%") && cur.pos < comment_scanned_through)
        {
            // A line-starting run remains at block position in a quote's lazy
            // paragraph. Its source column decides whether it is refused.
            match refused_brace_run_with_columns(
                (cur.pos..cur.lines.len()).map_while(|i| cur.lines.line(i)),
                true,
                |offset, raw| {
                    if offset == 0 && cur.pos == 0 {
                        if let Some(column) = cur.marker_brace_column {
                            return Some(column);
                        }
                    }
                    // Ownership can strip indentation from any line of a run,
                    // including its opener on a host's marker line. Compare
                    // all lines in the same authored coordinate system. A
                    // quote's lazy opener already carries its written column
                    // in the lazy marker; its continuations must use that
                    // same quote-inner coordinate system.
                    if cur.in_quoted_body && cur.peek().is_some_and(|first| first.starts_with(LAZY))
                    {
                        return None;
                    }
                    let at = cur.pos + offset;
                    let source_line = cur.source_line(at)?;
                    let stripped = cur.source_col(at)?;
                    let lead = strip_lazy(raw);
                    let indent_codepoints = lead
                        .chars()
                        .take_while(|ch| matches!(ch, ' ' | '\t'))
                        .count();
                    let walked = (stripped + indent_codepoints as isize).max(0) as usize;
                    Some(crate::parse::own_lines::authored_visual_column(
                        source_line,
                        walked + 1,
                    ))
                },
            ) {
                RefusedBraceRun::Lines(count) => {
                    let start = lines.len();
                    for _ in 0..count {
                        let Some(taken) = cur.peek() else { break };
                        cur.consume();
                        line_indices.push(cur.pos - 1);
                        lines.push(trim_ascii_end(trim_ascii_start(taken)));
                    }
                    literal.push((start, lines.len()));
                    inline_span.scanned = lines.len();
                    continue;
                }
                RefusedBraceRun::No(scanned) if brace_line.starts_with("{%") => {
                    comment_scanned_through = cur.pos.saturating_add(scanned);
                    if scanned != usize::MAX {
                        comment_run_through = comment_scanned_through;
                    }
                }
                RefusedBraceRun::No(_) => {}
            }
        }
        let forced = cur.pos < comment_run_through;
        if is_blank_line(line) {
            if image_lead
                && lines.len() == 1
                && cur
                    .lines
                    .line(cur.pos + 1)
                    .is_some_and(|next| caption_content(next).is_some())
            {
                cur.consume();
                continue;
            }
            break;
        }
        // First line is always part of the paragraph; from the second on, a
        // visible block opener interrupts (§10).
        // A brace line held by the paragraph's inline pairing cannot open a
        // block attribute, whether the brace form is wrapped or complete.
        let held_brace = brace_line.starts_with('{') && inline_span.holds_brace_line(&lines, cur);
        if !forced
            && !lines.is_empty()
            && ((reference_caption_claimed && caption_content(line).is_some())
                || (!held_brace && interrupts_paragraph(cur, line))
                || (cur.colon_view.is_some_and(|view| view.marker_hosted)
                    && strip_blockquote_prefix(trim_ascii_start(line)).is_some()))
        {
            break;
        }
        if image_opener
            && !lines.is_empty()
            && caption_content(line).is_some()
            && !line.starts_with([' ', '\t'])
        {
            at_content_column = true;
        }
        cur.consume();
        let trimmed = trim_ascii_end(trim_ascii_start(line));
        reference_caption_claimed |=
            reference_caption_candidate && lines.len() == 1 && caption_content(line).is_some();
        line_indices.push(cur.pos - 1);
        lines.push(trimmed);
    }
    // A paragraph never carries its OWN trailing attribute block: a standalone
    // `{...}` line floats forward (handled via interrupts_paragraph + the
    // pending-attrs loop), and a trailing same-line `{...}` with no abutting
    // host stays literal inline content (§14). Paragraph attributes come only
    // from a preceding block-attribute line (§15), applied by the caller.
    // Every line arrives already stripped at both ends (see the loop above), so
    // the join needs no further trimming. A HARD BREAK is unaffected: it is a
    // trailing BACKSLASH, which is content and not whitespace, so the line does
    // not end in whitespace at all and nothing is dropped in front of it. Carve
    // has no two-space hard break to lose - `a<SP><SP>` + newline + `b` is one
    // paragraph with a soft break in the executable spec.
    let mut joined = String::with_capacity(lines.iter().map(|l| l.len() + 1).sum());
    for (k, line) in lines.iter().enumerate() {
        if k > 0 {
            joined.push('\n');
        }
        joined.push_str(strip_lazy(line));
    }
    let at_content_column = at_content_column
        && !lines
            .iter()
            .skip(1)
            .any(|line| line.starts_with(LAZY) && caption_content(strip_lazy(line)).is_some());
    let joined = joined.as_str();
    let children = if !literal.is_empty() {
        let anchors: Option<Vec<_>> = options.positions.then(|| {
            lines
                .iter()
                .zip(&line_indices)
                .map(|(line, &index)| inline_anchor_for_line(cur, index, line))
                .collect()
        });
        literal_run_children(options, &lines, anchors.as_deref(), &literal)
    } else if options.positions {
        let anchors = lines
            .iter()
            .zip(&line_indices)
            .map(|(line, &index)| inline_anchor_for_line(cur, index, line))
            .collect();
        parse_inline_lines_with_anchor(joined, options, anchors)
    } else {
        parse_inline_with_options(joined, options)
    };
    // PART 10 I2: unexpanded, a bare-path line is literal text in every host.
    let children = if code_include == Some(false) {
        literal_line_children(&children, joined)
    } else {
        children
    };
    let pos = span_of(cur, span_start, cur.pos, options);
    BlockNode::Paragraph(Paragraph {
        attrs: None,
        children,
        at_content_column,
        // The promotion phase recomputes this after resolution; a paragraph
        // cannot know here whether its reference image resolves.
        block_image: false,
        pos,
    })
}

/// Whether the paragraph lines read so far leave an inline code span or a
/// braced comment open, so a line-initial `{` is inline content.
#[derive(Clone, Default)]
pub(in crate::parse) struct InlineSpanScan {
    pub(in crate::parse) scanned: usize,
    code_run: usize,
    comment: bool,
    /// The last forward search for a `%}`: the line it started on and the
    /// line it found, if any.
    closer: Option<(usize, Option<usize>)>,
}

impl InlineSpanScan {
    /// Whether the inline pairing of `lines` holds the cursor's brace line:
    /// an open code span, or an open braced comment whose `%}` stands on this
    /// line or a later one before the next blank (PART 9 §21a, carve#5740).
    pub(in crate::parse) fn holds_brace_line<S: AsRef<str>>(
        &mut self,
        lines: &[S],
        cur: &LineCursor<'_>,
    ) -> bool {
        if self.code_open(lines) {
            return true;
        }
        self.comment && self.comment_closes_from(cur)
    }

    fn comment_closes_from(&mut self, cur: &LineCursor<'_>) -> bool {
        let at = cur.pos;
        if let Some((from, found)) = self.closer {
            if from <= at && found.map_or(true, |k| k >= at) {
                return found.is_some();
            }
        }
        let mut found = None;
        let mut k = at;
        while let Some(line) = cur.lines.line(k) {
            if is_blank_line(line) {
                break;
            }
            if line.contains("%}") {
                found = Some(k);
                break;
            }
            k += 1;
        }
        self.closer = Some((at, found));
        found.is_some()
    }

    pub(in crate::parse) fn code_open<S: AsRef<str>>(&mut self, lines: &[S]) -> bool {
        self.open(lines);
        self.code_run != 0
    }

    pub(in crate::parse) fn open<S: AsRef<str>>(&mut self, lines: &[S]) -> bool {
        for line in &lines[self.scanned.min(lines.len())..] {
            let bytes = strip_lazy(line.as_ref()).as_bytes();
            let mut k = 0;
            while k < bytes.len() {
                if self.comment {
                    if bytes[k] == b'%' && bytes.get(k + 1) == Some(&b'}') {
                        self.comment = false;
                        k += 1;
                    }
                } else if bytes[k] == b'`' {
                    let width = bytes[k..].iter().take_while(|&&b| b == b'`').count();
                    if self.code_run == 0 {
                        self.code_run = width;
                    } else if self.code_run == width {
                        self.code_run = 0;
                    }
                    k += width - 1;
                } else if self.code_run == 0 && bytes[k] == b'\\' {
                    let width = bytes[k..].iter().take_while(|&&b| b == b'\\').count();
                    let escaped = matches!(bytes.get(k + width), Some(b'`' | b'{'));
                    k += width - 1 + usize::from(escaped && width % 2 == 1);
                } else if self.code_run == 0 && bytes[k] == b'{' && bytes.get(k + 1) == Some(&b'%')
                {
                    self.comment = true;
                    k += 1;
                } else if self.code_run == 0 && bytes[k] == b'{' {
                    // Backticks in an inline attribute's quoted value are
                    // value bytes, not code-span delimiters.
                    let mut quote = None;
                    let mut end = k + 1;
                    while end < bytes.len() {
                        match (quote, bytes[end]) {
                            (Some(_), b'\\') => end += 1,
                            (Some(q), ch) if q == ch => quote = None,
                            (None, b'"' | b'\'') => quote = Some(bytes[end]),
                            (None, b'{') => break,
                            (None, b'}') => break,
                            _ => {}
                        }
                        end += 1;
                    }
                    if bytes.get(end) == Some(&b'}')
                        && super::attrs::parse_standalone_attrs(&line.as_ref()[k..=end]).is_some()
                    {
                        k = end;
                    }
                }
                k += 1;
            }
        }
        self.scanned = lines.len();
        self.code_run != 0 || self.comment
    }
}

/// The braced comment state of a paragraph's lines, fed one line at a time.
#[derive(Default)]
pub(in crate::parse) struct OpenComment {
    scan: InlineSpanScan,
    line_start: bool,
}

impl OpenComment {
    pub(in crate::parse) fn feed(&mut self, line: &str) {
        let was_open = self.scan.comment;
        self.scan.scanned = 0;
        self.scan.open(&[line]);
        if self.scan.comment && line.contains("%}") {
            // The comment left open here opened after a closer on this line.
            self.line_start = false;
        } else if !was_open && self.scan.comment {
            self.line_start = trim_ascii_start(strip_lazy(line)).starts_with("{%");
        }
    }

    /// Whether the open comment holds the next line: any comment holds a brace
    /// line, only one opened at its line's start holds other lines.
    pub(in crate::parse) fn holds(&self, brace_line: bool) -> bool {
        self.scan.comment && self.scan.code_run == 0 && (brace_line || self.line_start)
    }
}

/// Parse the paragraph once while protecting each refused opener's `{` from
/// inline pairing. The rest of every line still participates in inline parsing.
pub(in crate::parse) fn literal_run_children<S: AsRef<str>>(
    options: &Options<'_>,
    lines: &[S],
    anchors: Option<&[Option<(usize, isize)>]>,
    literal: &[(usize, usize)],
) -> Vec<InlineNode> {
    const STAND_IN: char = '\u{e000}';
    let mut source: Vec<String> = lines
        .iter()
        .map(|line| strip_lazy(line.as_ref()).to_owned())
        .collect();
    for &(start, end) in literal {
        for line in source.iter_mut().take(end).skip(start) {
            if let Some(at) = line.find('{') {
                line.replace_range(at..at + 1, &STAND_IN.to_string());
            }
        }
    }
    let source = source.join("\n");
    let mut nodes = match anchors {
        Some(anchors) => parse_inline_lines_with_anchor(&source, options, anchors.to_vec()),
        None => parse_inline_with_options(&source, options),
    };
    struct Restore;
    impl crate::include_walk::SubtreeVisitor for Restore {
        fn inlines(&mut self, nodes: &mut Vec<InlineNode>) {
            for node in nodes {
                if let InlineNode::Text(text) = node {
                    if text.value.contains(STAND_IN) {
                        text.value = text.value.replace(STAND_IN, "{");
                    }
                }
                crate::include_walk::visit_inline_children(node, self);
            }
        }
    }
    use crate::include_walk::SubtreeVisitor;
    Restore.inlines(&mut nodes);
    nodes
}

/// Whether `line`, seen while accumulating a paragraph, ends it and starts a
/// new block (grammar §10, post-Markdown default).
///
/// A VISIBLE block interrupts an open paragraph with no blank line, at the top
/// level and nested: heading, thematic break, block quote, bullet/task list, a
/// valid table row, a fenced code block (closer or not, carve#4018), and
/// a valid flush-left colon-fence block. INVISIBLE constructs
/// (comments, abbreviation definitions) interrupt too. ORDERED lists do NOT
/// interrupt, `+` is the continuation marker not a bullet, and a bare image
/// stays inline.
pub(super) fn interrupts_paragraph(cur: &mut LineCursor<'_>, line: &str) -> bool {
    // §10 (post-Markdown default): a VISIBLE block interrupts an open paragraph
    // with no blank line. Invisible constructs (comments, abbreviation defs)
    // interrupt too. Ordered lists do NOT interrupt, `+` is the continuation
    // marker not a bullet, and a bare image stays inline.
    if trim_ascii_start(line).starts_with("%%")
        || (detect_abbreviation_def(line).is_some()
            && (cur.at_document_level || cur.invisible_arms))
    {
        return true;
    }
    if cur.invisible_arms
        && line.starts_with([' ', '\t'])
        && trim_ascii(line).starts_with('{')
        && cur.lines.line(cur.pos) == Some(line)
        && indented_wrapped_attrs_block_len(
            (cur.pos..cur.lines.len()).map_while(|i| cur.lines.line(i)),
        )
        .is_some()
    {
        return true;
    }
    if cur.invisible_arms && !line.starts_with([' ', '\t']) {
        if parse_standalone_attrs(line).is_some() {
            return true;
        }
        if cur.lines.line(cur.pos) == Some(line)
            && standalone_attrs_block_len_iter(
                (cur.pos..cur.lines.len()).map_while(|i| cur.lines.line(i)),
            )
            .is_some()
        {
            return true;
        }
    }
    // A MARKER AT A LIST ITEM'S CONTENT COLUMN OPENS A SUBLIST, first in the
    // item or not (§24 C3, markup-carve/carve#1517). These lines are the item's
    // body, so their column 0 IS the content column: "AT content_column ... a
    // list marker opens a sublist", "whether or not a blank line precedes the
    // child". §10 I2 defers to it by name rather than competing with it.
    //
    // COLUMN 0 EXACTLY, and the leading-whitespace guard is what says so: §24
    // C3's other band is explicit that BELOW the content column "a list marker
    // folds as lazy item text", and the collector forwards such a line with its
    // residual indent intact. `is_list_marker` trims, so without the guard the
    // two bands would answer the same.
    if cur.in_marker_body()
        && cur.carried_reach(cur.pos) != Some(false)
        && !line.starts_with([' ', '\t'])
        && is_list_marker(line)
    {
        return true;
    }
    // Symmetric §10: a list marker (bullet OR task OR ordered) does NOT
    // interrupt a paragraph -- a list needs a blank line before it. Only the
    // other visible blocks interrupt.
    if detect_heading(line).is_some()
        || detect_thematic_break(line)
        || strip_blockquote_prefix(line).is_some()
        // A definition-list term `:: ` is a first-class block opener (§24 C3):
        // it interrupts at column 0 and nests at the content column, uniform
        // with quote/heading/fence/table. `is_definition_list_start` requires a
        // flush-left `:: `, so an indented term folds as lazy text like the rest.
        || is_definition_list_start(line)
        // A table row interrupts only when FLUSH-LEFT, like the quote/heading
        // checks above -- `is_table_start` trims leading whitespace, so without
        // this guard an INDENTED row (`  |a|`) would interrupt where an indented
        // `> q` / `# h` does not. An indented row below/above a list item's
        // content column is lazy paragraph text (§24 C3), not a nested table.
        || (!line.starts_with([' ', '\t']) && is_table_start(line))
    {
        return true;
    }
    // A fence opener interrupts closer or not (carve#4018).
    if detect_fence_open(line).is_some() {
        return true;
    }
    if is_colon_fence_opener_shape(line) {
        return true;
    }
    false
}

pub(super) fn interrupts_lazy_continuation(cur: &mut LineCursor<'_>, line: &str) -> bool {
    let saved = cur.in_item_body;
    cur.in_item_body = false;
    let interrupts = interrupts_paragraph(cur, line);
    cur.in_item_body = saved;
    interrupts || is_colon_fence_opener_shape(line)
}

pub(super) fn interrupts_paragraph_as_container(cur: &mut LineCursor<'_>, line: &str) -> bool {
    let saved = cur.at_document_level;
    cur.at_document_level = false;
    let interrupts = interrupts_paragraph(cur, line);
    cur.at_document_level = saved;
    interrupts
}

/// `interrupts_paragraph` for a container's BELOW-COLUMN band.
///
/// Always as a container, so the abbreviation arm is waived (§7 recognizes that
/// definition only as a direct child of the document). `at_document_column`
/// additionally selects the band: at a NONZERO column below the content column
/// the block-attribute line is lazy paragraph text of that container too (§10 I5,
/// markup-carve/carve#1809), while at column 0 it is a floating attribute that
/// attaches forward.
///
/// The COMMENT is deliberately not waived: it is column-exempt (PART 9 §24) and
/// renders nothing at any column, which corpus 430-5 pins - waiving it would put
/// its characters on the page.
pub(super) fn interrupts_paragraph_in_band(
    cur: &mut LineCursor<'_>,
    line: &str,
    at_document_column: bool,
) -> bool {
    let saved_level = cur.at_document_level;
    let saved_arms = cur.invisible_arms;
    cur.at_document_level = false;
    cur.invisible_arms = at_document_column;
    let interrupts = interrupts_paragraph(cur, line);
    cur.at_document_level = saved_level;
    cur.invisible_arms = saved_arms;
    interrupts
}

pub(super) fn interrupts_lazy_continuation_as_container(
    cur: &mut LineCursor<'_>,
    line: &str,
) -> bool {
    let saved = cur.at_document_level;
    cur.at_document_level = false;
    let interrupts = interrupts_lazy_continuation(cur, line);
    cur.at_document_level = saved;
    interrupts
}

pub(super) fn is_colon_fence_opener_shape(line: &str) -> bool {
    // Only a FLUSH-LEFT colon fence ends lazy continuation (grammar PART 9
    // §10). An INDENTED colon-shaped line (the detectors trim leading
    // whitespace) is still within the container, so it folds as lazy text
    // instead of escaping the container.
    if line.starts_with(' ') || line.starts_with('\t') {
        return false;
    }
    detect_container_open(line).is_some()
        || detect_line_block_open(line).is_some()
        || detect_hardbreaks_block_open(line).is_some()
        || detect_quote_block_open(line).is_some()
}

/// Whether a glued colon fence earlier in the paragraph (`:::note`, `:::]`)
/// keeps THIS line from interrupting it.
///
/// Only a BARE fence (`:::`, nothing after the colons) is held back: it is
/// closer-shaped, and the paragraph it would close is literal text, so it stays
/// literal text too. A real opener - a type word, `::: |`, `::: \` - opens its
/// block as usual, glued predecessor or not (carve-rs#496). Matches carve-js
/// (`RE_ADMONITION_CLOSE` + `isLiteralColonFenceLine`) and carve-php.
pub(super) fn is_suppressed_colon_fence_line(line: &str) -> bool {
    is_colon_fence_opener_shape(line) && exact_colon_fence_len(line).is_some()
}

pub(super) fn interrupts_paragraph_with_rest(line: &str, rest: &[&str]) -> bool {
    if trim_ascii_start(line).starts_with("%%") {
        return true;
    }
    // Flush-left only (see interrupts_paragraph): an indented `{...}` line is
    // lazy paragraph text under the strict column-0 rule, not a floating attr.
    //
    // The WRAPPED spelling interrupts too, which is what `rest` is for. Only the
    // single-line form was tested here, so `{.k` opened nothing and folded into
    // the paragraph as literal text - the author's braces reached the page and
    // the attributes reached nothing (markup-carve/carve-rs#1039, and the half
    // of markup-carve/carve#1281's `329-...-6` that the container's end does not
    // answer on its own).
    //
    // The brace test comes FIRST so the common line costs nothing: this
    // predicate is asked of every blank-separated block, and copying `rest`
    // for a heading or a paragraph line would make that quadratic.
    if !line.starts_with([' ', '\t'])
        && trim_ascii(line).starts_with('{')
        && standalone_attrs_block_len_iter(std::iter::once(line).chain(rest.iter().copied()))
            .is_some()
    {
        return true;
    }
    if detect_heading(line).is_some()
        || detect_thematic_break(line)
        || strip_blockquote_prefix(line).is_some()
        // A definition-list term `:: ` is a first-class block opener (§24 C3):
        // it interrupts at column 0 and nests at the content column, uniform
        // with quote/heading/fence/table. `is_definition_list_start` requires a
        // flush-left `:: `, so an indented term folds as lazy text like the rest.
        || is_definition_list_start(line)
        // A table row interrupts only when FLUSH-LEFT, like the quote/heading
        // checks above -- `is_table_start` trims leading whitespace, so without
        // this guard an INDENTED row (`  |a|`) would interrupt where an indented
        // `> q` / `# h` does not. An indented row below/above a list item's
        // content column is lazy paragraph text (§24 C3), not a nested table.
        || (!line.starts_with([' ', '\t']) && is_table_start(line))
    {
        return true;
    }
    if let Some(open) = detect_fence_open(line) {
        if rest.iter().any(|l| is_fence_close(l, open)) {
            return true;
        }
    }
    // A FLUSH-LEFT colon-fence family opener interrupts blockquote lazy
    // continuation like any block opener. An INDENTED colon fence (above the
    // quote's content column) is literal paragraph text under the strict
    // column-0 rule, so lazy continuation stays inside the quote.
    if !line.starts_with([' ', '\t'])
        && (detect_container_open(line).is_some()
            || detect_line_block_open(line).is_some()
            || detect_hardbreaks_block_open(line).is_some()
            || detect_quote_block_open(line).is_some())
    {
        return true;
    }
    false
}

/// A `- ` / `* ` bullet, including the attributed form `-{.c} ` (NOT `+`, the
/// continuation marker; not ordered).
///
/// Delegates to `detect_unordered` so an attributed bullet interrupts a
/// paragraph just like a plain one (and an attributed task already does via
/// `detect_task`). Leading tabs are skipped as well as spaces: a bullet opens
/// a list at any indentation (Rule B), so a tab-indented bullet interrupts a
/// paragraph too.
/// A bare image line is a block image (or figure) ONLY when it stands alone --
/// the next line is blank / EOF, a `^ ` caption, or a paragraph interrupter.
/// When the next line FOLDS (plain text, list marker, another bare image), the
/// image stays inline in a paragraph with that content, per grammar §1722 I3
/// ("an image is not a block of its own; it stays inline in the paragraph").
pub(super) fn image_is_block(cur: &mut LineCursor) -> bool {
    let Some(next) = cur.lines.line(cur.pos + 1) else {
        return true;
    };
    if is_blank_line(next) || caption_content(next).is_some() {
        return true;
    }
    // Peek-1 interruption: test the next line as if it were current, then rewind.
    let next_owned = next.to_string();
    let saved = cur.pos;
    cur.pos += 1;
    let interrupts = interrupts_paragraph(cur, &next_owned);
    cur.pos = saved;
    interrupts
}

pub(super) fn consume_caption(
    cur: &mut LineCursor,
    options: &Options<'_>,
) -> Option<Vec<InlineNode>> {
    consume_caption_slot(cur, options, true)
}

fn consume_caption_slot(
    cur: &mut LineCursor,
    options: &Options<'_>,
    caption_context: bool,
) -> Option<Vec<InlineNode>> {
    let saved = cur.pos;
    // PART 9 §4 (NORMATIVE): `caption_slot = [blank_line], caption` carries at
    // most ONE optional blank line. A caption line adjacent to its host, or
    // separated from it by exactly one blank line, attaches; TWO blank lines
    // DETACH and leave the `^ ` line an ordinary paragraph. Scanning blank
    // lines in a loop attaches across any number of them (carve-rs#830), which
    // no corpus document could distinguish because every captioned document
    // had zero or one blank line.
    if matches!(cur.peek(), Some(line) if is_blank_line(line)) {
        cur.consume();
    }
    let Some(line) = cur.peek() else {
        cur.pos = saved;
        return None;
    };
    let Some(text) = caption_content(line) else {
        cur.pos = saved;
        return None;
    };
    let mut joined = text.to_string();
    // One anchor per folded line, the same shape a paragraph builds. The first
    // entry accounts for the `^ ` marker, which `inline_anchor_for_line`
    // derives by comparing the full line against the inline text.
    let mut anchors = options
        .positions
        .then(|| vec![inline_anchor_for_line(cur, cur.pos, text)]);
    cur.consume();
    // A caption is multi-line inline content, so it folds following lines like a
    // PARAGRAPH (§10), NOT like a heading: a list marker FOLDS in (djot -- a
    // list needs a blank line to interrupt), while a heading / blockquote /
    // table / fenced code / `:::` div / thematic break / `%%%` comment
    // interrupts and ends the caption. A blank line or a further `^ ` caption
    // line also ends it. Continuation lines join with `\n`.
    while let Some(next) = cur.peek() {
        if is_blank_line(next) || caption_content(next).is_some() {
            break;
        }
        let next_owned = next.to_string();
        if interrupts_paragraph(cur, &next_owned) {
            break;
        }
        joined.push('\n');
        joined.push_str(next);
        if let Some(anchors) = &mut anchors {
            anchors.push(inline_anchor_for_line(cur, cur.pos, next));
        }
        cur.consume();
    }
    // §756 (NORMATIVE): strip trailing whitespace - on EVERY line the caption
    // spans, not only its last (PART 2 NO TRAILING WHITESPACE, carve#926; a
    // table caption is one of the contexts that ruling names, and one the
    // executable spec itself was missing until it was measured). Stripping only
    // ever shortens a line's END, so it cannot shift any anchor.
    let joined = joined
        .split('\n')
        .map(|l| trim_ascii_end(strip_lazy(l)))
        .collect::<Vec<_>>()
        .join("\n");
    let text = trim_ascii_end(&joined);
    Some(match anchors {
        Some(anchors) => parse_caption_inline_with_anchor(text, options, anchors, caption_context),
        None => parse_caption_inline_with_options(text, options, caption_context),
    })
}

/// `text` as one literal text node spanning the parsed `children`.
pub(in crate::parse) fn literal_line_children(
    children: &[InlineNode],
    text: &str,
) -> Vec<InlineNode> {
    let pos = children
        .first()
        .and_then(InlineNode::pos)
        .zip(children.last().and_then(InlineNode::pos))
        .map(|(first, last)| Pos {
            end_line: last.end_line,
            end_column: last.end_column,
            end_offset: last.end_offset,
            ..first.clone()
        });
    vec![InlineNode::Text(Text {
        value: text.to_string(),
        pos,
    })]
}
