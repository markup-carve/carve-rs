use super::*;

pub(in crate::parse) fn detect_abbreviation_def(line: &str) -> Option<AbbreviationDef> {
    let rest = line.strip_prefix("*[")?;
    let (abbr, expansion) = rest.split_once("]:")?;
    let expansion = expansion.strip_prefix(' ')?;
    // THE SEPARATOR IS A RUN, AND IT IS ASCII SPACES. PART 5 spells it
    // `space+`: a tab is still not a separator, so `*[HTML]:<TAB>x` stayed a
    // paragraph at the `strip_prefix` above, but a run of spaces is one
    // separator rather than a separator plus content (carve#892).
    //
    // The first character that is not a space ENDS the separator and BEGINS the
    // content, which is the half this engine had wrong: the expansion was
    // `trim()`ed, so a no-break space or a tab after the run was eaten and
    // `*[HTML]: <NBSP>Hyper Text` expanded to a title the author did not write.
    // An `abbreviation_expansion` is a raw string, so both survive into it.
    //
    // Trimmed with `trim_start_matches(' ')` and not a whitespace `trim_start`
    // for exactly that reason - the terminal is the ASCII space, not the
    // Unicode property.
    let expansion = expansion.trim_start_matches(' ');
    // ASCII, because `letter` is: the grammar enumerates it as `a`..`z` plus
    // `A`..`Z`, and `digit` as `0`..`9`. `char::is_alphanumeric` is
    // Unicode-aware, so `*[ß]:` and `*[日本]:` were definitions here and
    // paragraph text in carve-js and carve-php - and a definition renders
    // nothing, so the document lost either the line or the expansion when it
    // moved (carve#791). Same reading that makes `*[e.g.]:` a paragraph, which
    // all three already agree on, and the same ASCII rule
    // `is_attr_ident_start` below applies to attribute identifiers.
    if abbr.is_empty() || !abbr.chars().all(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    // `abbreviation_expansion = {character - newline}+` - ONE or more. An empty
    // expansion is not a definition, and consuming the line DELETED it from the
    // document (carve-js keeps it as `<p>*[A]:</p>`).
    //
    // Deliberately `is_empty()` and not a trimmed test: a space is a character,
    // so `*[A]:··` has a one-character expansion and IS a definition. That is
    // the production as written, and it is what carve-js does.
    if expansion.is_empty() {
        return None;
    }
    Some(AbbreviationDef {
        abbr: abbr.to_string(),
        // Only the TRAILING side is trimmed here: the leading run was consumed
        // above as the separator it is. The TERMINAL is space-or-tab, the one
        // whitespace definition (PART 7, carve#977) - `trim_end` is the Unicode
        // White_Space property, which ate a trailing NO-BREAK SPACE, VERTICAL
        // TAB or FORM FEED out of the expansion, all three of them content. The
        // note that used to stand here deferred this to carve#926; PART 7 is
        // where the terminal was settled.
        expansion: trim_ascii_end(expansion).to_string(),
        pos: None,
    })
}

pub(in crate::parse) fn is_definition_list_start(line: &str) -> bool {
    line.strip_prefix(":: ")
        .is_some_and(|term| !is_blank_line(term))
}

fn definition_entry_at(line: &str, marker_hosted: bool, column: usize) -> Option<&str> {
    if (marker_hosted || column > 0) && indent_columns(line) != column {
        return None;
    }
    Some(if marker_hosted || column > 0 {
        trim_ascii_start(line)
    } else {
        line
    })
}

/// Fold a term's continuation lines and parse its inline content.
///
/// A term folds a following plain line as a soft break, so a wrapped term line
/// does not strand the definition; a blank line, a new marker (`::` / `:  `), a
/// list marker or a block opener ends it. A comment past the container column
/// stays a comment and does not end the term (carve#2411). As in a paragraph,
/// inline content never reaches across it, so each run of lines between
/// comments is parsed on its own.
pub(in crate::parse) fn fold_term(
    cur: &mut LineCursor,
    options: &Options<'_>,
    first: String,
    first_anchor: Option<Option<(usize, isize)>>,
    entry_column: usize,
    marker_hosted: bool,
) -> Vec<InlineNode> {
    enum Part {
        Run(String, Option<Vec<Option<(usize, isize)>>>),
        Comment(Comment),
        InvisibleComment(Option<Pos>),
    }
    let mut parts = vec![Part::Run(first, first_anchor.map(|anchor| vec![anchor]))];
    while let Some(next) = cur.peek() {
        if next.starts_with([' ', '\t'])
            && trim_ascii_start(next).starts_with("%%")
            && !is_definition_placeholder(next)
        {
            let comment_start = cur.pos;
            match take_comment_block(cur, options) {
                CommentBlock::Consumed(Some(node)) => {
                    if let BlockNode::Comment(comment) = *node {
                        parts.push(Part::Comment(comment));
                    }
                    continue;
                }
                CommentBlock::Consumed(None) => {
                    parts.push(Part::InvisibleComment(span_of(
                        cur,
                        comment_start,
                        cur.pos,
                        options,
                    )));
                    continue;
                }
                CommentBlock::NotAComment => {}
            }
        }
        let Some(next) = cur.peek() else { break };
        // A description marker left of the body column returns to the outer
        // description. Nested body collection can retain its residual indent
        // without a LAZY frame; that indent must not turn the marker into term
        // text. A framed line belongs to the list-item lazy rule instead.
        if !next.starts_with(LAZY)
            && cur.carried_reach(cur.pos) == Some(false)
            && strip_definition_marker(trim_ascii_start(next)).is_some()
        {
            break;
        }
        let same_column_entry = !next.starts_with(LAZY)
            && definition_entry_at(strip_lazy(next), marker_hosted, entry_column).is_some_and(
                |line| is_definition_list_start(line) || strip_definition_marker(line).is_some(),
            );
        if is_blank_line(next)
            || next.starts_with(ITEM_PARAGRAPH)
            || (!next.starts_with(LAZY) && is_definition_list_start(next))
            || strip_lazy(next)
                .strip_prefix(":: ")
                .is_some_and(|rest| is_blank_line(rest) && rest.contains('\t'))
            || (!next.starts_with(LAZY) && strip_definition_marker(strip_lazy(next)).is_some())
            || same_column_entry
            || is_list_marker(next)
            || caption_content(next).is_some()
        {
            break;
        }
        let owned = next.to_string();
        // AS A CONTAINER, which waives the abbreviation arm. A term's
        // continuation is a position for INLINE CONTENT, not for a block, so §7
        // never gets to recognize `*[A]: b` there - and it is a content line at
        // the document column too, which is why this cannot ask
        // `at_document_level`. Asked as a paragraph, `:: t` / `*[A]: b` lost the
        // author's second line and defined an abbreviation (carve-rs#2096).
        // A code opener at the container column ends a term even without a
        // closer (CARVE-P2-028). Paragraph continuation requires a closer.
        if detect_fence_open(&owned).is_some() || interrupts_paragraph_as_container(cur, &owned) {
            break;
        }
        // A term's continuation line is a CONTENT LINE, so its trailing
        // whitespace run does not reach the output - the same rule the term's
        // FIRST line follows (markup-carve/carve#926, markup-carve/carve#1289).
        // Spaces INSIDE a verbatim run end at its closing delimiter, so an
        // all-space `` `  ` `` term is untouched by a trim that only ever sees
        // the end of a line.
        let text = strip_lazy(&owned);
        let text = if owned.starts_with(LAZY)
            && !owned.starts_with(LAZY_INDENT)
            && cur.carried_reach(cur.pos) != Some(true)
        {
            trim_ascii_start(text)
        } else {
            text
        };
        let line = trim_ascii_end(text).to_string();
        let anchor = options
            .positions
            .then(|| inline_anchor_for_line(cur, cur.pos, &owned));
        match parts.last_mut() {
            Some(Part::Run(text, anchors)) => {
                text.push('\n');
                text.push_str(&line);
                if let (Some(anchors), Some(anchor)) = (anchors.as_mut(), anchor) {
                    anchors.push(anchor);
                }
            }
            _ => parts.push(Part::Run(line, anchor.map(|anchor| vec![anchor]))),
        }
        cur.consume();
    }
    let mut children = Vec::new();
    let mut boundaries = Vec::new();
    for (index, part) in parts.into_iter().enumerate() {
        if index > 0 {
            let target = match &part {
                Part::InvisibleComment(pos) => pos.clone(),
                _ => None,
            };
            boundaries.push((children.len(), target));
            children.push(InlineNode::SoftBreak(Break { pos: None }));
        }
        match part {
            Part::Comment(comment) => children.push(InlineNode::Comment(comment)),
            Part::InvisibleComment(_) => {}
            Part::Run(text, Some(anchors)) => {
                children.extend(parse_inline_lines_with_anchor(&text, options, anchors));
            }
            Part::Run(text, None) => children.extend(parse_inline_with_options(&text, options)),
        }
    }
    // A part-boundary break is placed once both its neighbours exist: from the
    // preceding sibling's end to the following sibling's start, the span the
    // fold's own in-part breaks already publish (carve#2469).
    for (index, target) in boundaries {
        let pos = index
            .checked_sub(1)
            .and_then(|previous| children.get(previous))
            .and_then(InlineNode::pos)
            .zip(
                children
                    .get(index + 1)
                    .and_then(InlineNode::pos)
                    .or(target.as_ref()),
            )
            .map(|(prev, next)| Pos {
                start_line: prev.end_line,
                end_line: next.start_line,
                start_column: prev.end_column,
                end_column: next.start_column,
                start_offset: prev.end_offset,
                end_offset: next.start_offset,
                file: prev.file.clone(),
            });
        if let InlineNode::SoftBreak(br) = &mut children[index] {
            br.pos = pos;
        }
    }
    children
}

pub(in crate::parse) fn parse_definition_list(
    cur: &mut LineCursor,
    options: &Options<'_>,
) -> BlockNode {
    let list_start = cur.pos;
    let marker_hosted = cur.colon_view.is_some_and(|view| view.marker_hosted);
    let list_column = cur.peek().map_or(0, indent_columns);
    let mut items = Vec::new();
    while let Some(line) = cur.peek() {
        // A LAZY-FRAMED marker line (`LAZY:: u`) reached this open list by the
        // fold and carries no column (PART 0), so the frame is stripped before
        // the marker is recognized - at column 0, whatever the container depth
        // the line was written at (markup-carve/carve-rs#1538). The frame is a
        // no-op on an ordinary flush-left term, so unframed lists are unchanged.
        let Some(term) = definition_entry_at(strip_lazy(line), marker_hosted, list_column)
            .and_then(|line| line.strip_prefix(":: "))
        else {
            break;
        };
        if is_blank_line(term) {
            break;
        }
        // The SEPARATOR is the space, and whitespace past it is not content -
        // the same rule `#`, `>` and `-` already follow, and the one carve#513
        // and carve#530 restated twice as a property of the separator rather
        // than of any one marker. `stripped_col` locates the column by matching
        // this slice as a suffix of the raw line, so trimming here moves the
        // term's own position with it instead of leaving it on the marker.
        let term = trim_ascii_start(term);
        let term_source_line = cur.source_line(cur.pos);
        let term_start = cur.pos;
        cur.consume();
        // A term folds a following plain line like a heading (soft break), so a
        // wrapped term line does not strand the definition. A blank line, a new
        // marker (`::` / `:  `), a list marker, or a block opener ends the term.
        let anchor = options
            .positions
            .then(|| inline_anchor_for_line(cur, term_start, term));
        let children = fold_term(
            cur,
            options,
            trim_ascii_end(term).to_string(),
            anchor,
            list_column,
            marker_hosted,
        );
        // The span covers the `:: ` marker and every line the term folded, the
        // same way a heading's covers its `#`.
        let mut terms = vec![DefinitionTerm {
            attrs: source_line_attrs(None, term_source_line, options),
            children,
            pos: span_of(cur, term_start, cur.pos, options),
        }];

        // CONSECUTIVE terms share an entry, which is what `:: a` / `:: b` on
        // adjacent lines means and what the rendered `<dl>` shows - a run of
        // `<dt>` followed by the `<dd>`s they share. This engine used to open a
        // new entry per term line, so the same document grouped differently
        // here and in carve-js while both rendered the same list; the grouping
        // was an internal nobody could see, until the AST became something
        // engines hand each other (PART 12).
        while let Some(next) = cur.peek() {
            let Some(next_term) = definition_entry_at(strip_lazy(next), marker_hosted, list_column)
                .and_then(|line| line.strip_prefix(":: "))
            else {
                break;
            };
            if is_blank_line(next_term) {
                break;
            }
            // Same separator rule as the item's first term above. A list can
            // hold several terms and each one strips its own padding.
            let next_term = trim_ascii_start(next_term);
            let next_source_line = cur.source_line(cur.pos);
            let next_start = cur.pos;
            cur.consume();
            let anchor = options
                .positions
                .then(|| inline_anchor_for_line(cur, next_start, next_term));
            let children = fold_term(
                cur,
                options,
                trim_ascii_end(next_term).to_string(),
                anchor,
                list_column,
                marker_hosted,
            );
            terms.push(DefinitionTerm {
                attrs: source_line_attrs(None, next_source_line, options),
                children,
                pos: span_of(cur, next_start, cur.pos, options),
            });
        }

        let mut defs = Vec::new();

        loop {
            // A blank line before a `:  ` definition is a separator (djot
            // parity): a definition may be separated from its term or a
            // previous definition by a blank line. A blank not followed by a
            // `:  ` definition ends the entry.
            if matches!(cur.peek(), Some(l) if is_blank_line(l)) {
                let mut look = 0usize;
                while matches!(cur.lines.line(cur.pos + look), Some(l) if is_blank_line(l)) {
                    look += 1;
                }
                match cur.lines.line(cur.pos + look) {
                    Some(after)
                        if !after.starts_with(ITEM_PARAGRAPH)
                            && definition_entry_at(
                                strip_lazy(after),
                                marker_hosted,
                                list_column,
                            )
                            .is_some_and(|line| strip_definition_marker(line).is_some()) =>
                    {
                        for _ in 0..look {
                            cur.consume();
                        }
                    }
                    _ => break,
                }
            }
            let Some(line) = cur.peek() else {
                break;
            };
            // The nested parser sees the marker after its containing body has
            // normalized indentation. A marker that did not reach that body's
            // column belongs to the surviving outer owner, even if rebasing
            // placed it at this list's apparent marker column.
            if !line.starts_with(LAZY) && cur.carried_reach(cur.pos) == Some(false) {
                break;
            }
            let Some((def, definition_column)) = (!line.starts_with(ITEM_PARAGRAPH))
                .then(|| {
                    definition_entry_at(strip_lazy(line), marker_hosted, list_column)
                        .and_then(strip_definition_marker)
                })
                .flatten()
            else {
                break;
            };
            if is_blank_line(def) {
                break;
            }
            let def_source_line = cur.source_line(cur.pos);
            let def_start = cur.pos;
            // The `:  ` marker is three codepoints; add whatever an enclosing
            // container already took so a nested block maps back to the document.
            let def_source_col = cur
                .source_col(cur.pos)
                .map(|c| c + (list_column + definition_column) as isize);
            cur.consume();
            let def_trimmed = trim_ascii_end(def);
            // First-block form (`:  +`, mirroring the list `- +`): when the sole
            // content is a lone `+`, seed the body with the FOLLOWING flush-left
            // block (no `+` literal), with no indentation. `:  \+` stays literal.
            let mut body = if is_plus_marker(def_trimmed) {
                let mut fb = LineBuffer::default();
                let lines = cur.lines;
                let mut boundary = |a: &str, _| {
                    is_blank_line(a)
                        || is_plus_marker(a)
                        || (indent_columns(a) == 0 && is_list_marker(a))
                        || strip_lazy(a).strip_prefix(":: ").is_some()
                        || strip_definition_marker(strip_lazy(a)).is_some()
                };
                let classified = cur.continuation_decision(def_start);
                let end = match classified {
                    Some(decision)
                        if decision.owner == Some(continuation_marker::Kind::Description)
                            && decision.refusal.is_none() =>
                    {
                        attached_block_lines_classified(
                            lines,
                            cur.pos,
                            &mut cur.comment_closer_last_index,
                            &mut cur.code_closer_last_index,
                            options,
                            &mut boundary,
                        )
                    }
                    _ => cur.pos,
                };
                while cur.pos < end {
                    let a = cur.lines.at(cur.pos);
                    fb.push_at(
                        a.to_string(),
                        cur.source_line(cur.pos),
                        cur.source_col(cur.pos),
                    );
                    cur.consume();
                }
                fb.into_source()
            } else {
                MappedSource::new_line_at(def_trimmed.to_string(), def_source_line, def_source_col)
            };
            // A FENCED BODY IS NOT A PARAGRAPH, and a definition body is such a
            // container (PART 0 S4, markup-carve/carve#956). The guard is on the
            // OPEN FENCE rather than on where the fence was opened, so it is
            // seeded from the `:  ` MARKER line - which no collector ever sees -
            // and then followed by the collector on the lines it takes. The
            // first-block (`:  +`) form seeds nothing: its body is the following
            // flush-left block, which supplies no indentation for the rule to
            // measure against.
            let mut fence = DefinitionBodyFence {
                open: if is_plus_marker(def_trimmed) {
                    None
                } else {
                    detect_fence_open(def_trimmed)
                },
                closed_last: false,
            };
            let seed = body.source.clone();
            body.append(collect_definition_body(
                cur,
                &mut fence,
                &seed,
                list_column + definition_column,
                options,
            ));
            // The span covers the `:  ` marker through the last line the body
            // consumed, so a multi-line definition is one region rather than
            // just its opening line. `collect_definition_body` has already
            // advanced the cursor past those lines.
            // A DEFINITION BODY carries authored base for its sublists, so a list
            // marker past the body's content column anchors its span at the
            // marker rather than at the placing indent (markup-carve/carve#1980,
            // corpus 461-...-sibling-4).
            body.sublists_carry_authored_base = true;
            let children = if body.source.trim_end_matches([' ', '\t', '\n']) == "{empty}" {
                Vec::new()
            } else {
                parse_mapped_source(&body, options)
            };
            let mut pos = span_of(cur, def_start, cur.pos, options);
            if let (Some(pos), Some(last)) = (
                pos.as_mut(),
                children.iter().rev().find_map(crate::ast_json::block_pos),
            ) {
                pos.end_line = last.end_line;
                pos.end_column = last.end_column;
                pos.end_offset = last.end_offset;
            }
            defs.push(DefinitionDef {
                attrs: source_line_attrs(None, def_source_line, options),
                children,
                pos,
            });
        }

        items.push(DefinitionItem {
            // NOT placed, though the cursor could say where it is. The wire
            // format flattens items into a flat run of definition_term and
            // definition_description nodes, so the item is regrouped on the way
            // back in and any span here would not survive a round-trip
            // (PART 12 section 6). A field that is Some before a round-trip and
            // None after is worse than one that is always None.
            terms,
            definitions: defs,
            pos: None,
        });

        let saved = cur.pos;
        while matches!(cur.peek(), Some(line) if is_blank_line(line)) {
            cur.consume();
        }
        if !cur.peek().is_some_and(|l| {
            definition_entry_at(strip_lazy(l), marker_hosted, list_column)
                .is_some_and(is_definition_list_start)
        }) {
            cur.pos = saved;
            break;
        }
    }
    BlockNode::DefinitionList(DefinitionList {
        attrs: None,
        // §17 L7's key is CONSUMED off the preceding attribute line, which this
        // constructor never sees - `apply_attrs_to_block` sets the flag.
        loose: false,
        // The cursor has rolled back past the trailing blanks it looked through
        // for another item, so it points one line past the last definition -
        // the span stops at the content, not at the gap after it.
        pos: span_of(cur, list_start, cur.pos, options),
        items,
    })
}

/// A lone `+` (optionally followed by spaces/tabs) is the continuation marker
/// (PART 9 §17): it attaches the following flush-left block to the open
/// container.
pub(in crate::parse) fn is_plus_marker(line: &str) -> bool {
    line.strip_prefix('+')
        .is_some_and(|rest| rest.bytes().all(|b| b == b' ' || b == b'\t'))
}

/// Collect the continuation of a definition body. A definition continues like a
/// list item (PART 9 §17): form A folds an indented block in (a blank line is
/// tolerated when a later line still continues), form B attaches a lone `+`
/// pull-left flush-left block with no indentation, and a flush-left line with no
/// blank before it that does not start an interrupting block lazily continues
/// the open paragraph (matching list items, block quotes and djot). Returned
/// lines carry blank separators so the block sub-parse yields multiple paragraphs.
///
/// The body's own fenced code block, for the S4 test below. `open` carries it
/// while it is still OPEN; `closed_last` records that the last thing collected at
/// the body's column was its CLOSER, with nothing after it yet. Both states leave
/// the body without an open paragraph (PART 0 S4, markup-carve/carve#956).
pub(in crate::parse) struct DefinitionBodyFence {
    open: Option<FenceOpen>,
    closed_last: bool,
}

impl DefinitionBodyFence {
    /// S4's lazy branch continues an open PARAGRAPH, and a verbatim body is not
    /// one - neither while it runs nor once it has finished. A CLOSED code block
    /// is a finished block, not a paragraph, so a below-column line after it has
    /// nothing to fold into either.
    fn holds_no_paragraph(&self) -> bool {
        self.open.is_some() || self.closed_last
    }

    /// Follow the fence over one line collected at the body's column.
    fn track(&mut self, dedented: &str, opener_column: usize) {
        let was_open = self.open.is_some();
        // CARVE-P0-004: an opener PAST the body's column is measured from its own
        // authored column, and so is its closer. The body's own column closes it
        // too, and nothing in between does.
        let at_base = self
            .open
            .is_some_and(|open| opener_column == open.content_col);
        let tracked = if was_open && !at_base {
            dedented
        } else {
            trim_ascii_start(dedented)
        };
        track_collected_fence(&mut self.open, tracked, true, opener_column);
        // The closer is the line that took the fence from open to closed; any
        // other line at the column is content, and content after a closed block
        // opens a paragraph the fold can reach again.
        self.closed_last = was_open && self.open.is_none();
    }
}

/// Does the definition body collected so far end in something a flush-left line
/// can FOLD INTO?
pub(in crate::parse) fn body_ends_with_an_unnoded_interrupter(source: &str) -> bool {
    if body_ends_with_an_attribute_line(source) {
        return true;
    }
    source
        .lines()
        .rev()
        .find(|line| !is_blank_line(line))
        .is_some_and(|last| trim_ascii_start(last).starts_with("%%"))
}

pub(in crate::parse) fn body_ends_with_an_attribute_line(source: &str) -> bool {
    let raw_lines: Vec<&str> = source.lines().collect();
    let lines: Vec<&str> = raw_lines
        .iter()
        .map(|line| trim_ascii_start(line))
        .collect();
    let Some(end) = lines.iter().rposition(|line| !is_blank_line(line)) else {
        return false;
    };
    let start = lines[..end]
        .iter()
        .rposition(|line| is_blank_line(line))
        .map_or(0, |index| index + 1);
    if leading_ws(raw_lines[end]) > 0 && lines[start].starts_with('>') {
        return false;
    }
    if !trim_ascii_end(lines[end]).ends_with('}') {
        return false;
    }
    // The WRAPPED spelling (`{.k` / `#x}`) counts too. This used to test the
    // last line alone, so a block that closed on it but OPENED on an earlier one
    // was invisible here and the body was read as ending in a paragraph
    // (markup-carve/carve#1281's `329-...-6`, markup-carve/carve-rs#1039).
    //
    // The scan walks back only over the current run of non-blank lines, because
    // an attribute block is refused at a blank line - so the start can never be
    // further back than that.
    let mut start = end;
    loop {
        if standalone_attrs_block_len(&lines[start..]) == Some(end - start + 1) {
            // A wrapped opener past the body's column is paragraph text (carve#3421).
            return start == end || leading_ws(raw_lines[start]) == 0;
        }
        if start == 0 || is_blank_line(lines[start - 1]) {
            return false;
        }
        start -= 1;
    }
}

pub(in crate::parse) fn definition_body_fold(source: &str, options: &Options<'_>) -> (bool, bool) {
    if body_ends_with_an_unnoded_interrupter(source)
        || quoted_body_ends_with_an_invisible_line(source)
    {
        return (false, false);
    }
    let blocks = probe_blocks(source, options);
    if block_ends_with_open_paragraph(blocks.last(), colon_fences_left_open(source), false) {
        let frame = open_frame(&blocks);
        return (true, frame.contains_quote && frame.ends_in_paragraph);
    }
    let mut end = blocks.len();
    // Past a trailing run of comments, for the reason
    // `nested_ends_with_open_paragraph` gives: a comment renders nothing, so it
    // closes no paragraph and cannot be what the body ends in (S24 C3).
    while end > 0 && matches!(blocks[end - 1], BlockNode::Comment(_)) {
        end -= 1;
    }
    if end == 0 {
        return (false, false);
    }
    // NO HEADING ARM. There used to be one, folding a flush-left line into a
    // body whose last block is a heading written AT the content column, on the
    // reading that carve#1280 left that half open. markup-carve/carve#1911
    // closed it: an opener at the body's column and an opener past it are both
    // the body's own block content, §10 I1 closes the paragraph for either, and
    // the follower has nothing left to fold into. Corpus 444 rows 4 and 15 are
    // that half, and this engine was the only reader still folding them.
    (
        matches!(
            blocks[end - 1],
            // AN IMAGE BLOCK IS ONLY A BLOCK UNTIL SOMETHING FOLDS INTO IT.
            // `image_is_block` makes a bare image line a block ONLY when the
            // next line does not fold, and a `^ ` caption is an inline
            // continuation a following line extends. Both are decided by the
            // line AFTER the body, which is exactly the line this predicate is
            // being asked about and which the collected source therefore does
            // not contain yet. Reading the block off a body that stops one line
            // early turned `:  ![a](i.png)` / `lazy` into a standalone image
            // plus a top-level paragraph, where the list twin folds - the same
            // read-the-body-so-far trap the fence guard exists to avoid.
            BlockNode::BlockImage(_) | BlockNode::Figure(_)
        ),
        false,
    )
}

/// `fence` carries the body's own fenced code block, seeded by the caller from
/// the `:  ` marker line - which no collector ever sees - and followed here on
/// every line collected at the body's column. `seed` is that same marker-line
/// content (or, for the `:  +` form, the attached flush-left block), because the
/// paragraph test below needs the WHOLE body and this collector only ever sees
/// the lines after it.
pub(in crate::parse) fn collect_definition_body(
    cur: &mut LineCursor,
    fence: &mut DefinitionBodyFence,
    seed: &str,
    content_column: usize,
    options: &Options<'_>,
) -> MappedSource {
    let container = IndentContainer {
        marker_column: content_column.saturating_sub(2),
        content_column,
        owner: continuation_marker::Kind::Description,
    };
    let mut body = DefinitionBodyCollector::new(seed, content_column, container, options);
    run_indented_body!(cur.lines, cur.pos, container, |line, admission| {
        match body.take_line(cur, fence, line, admission.blank) {
            BodyStep::Next => continue,
            BodyStep::Stop => break,
        }
    });
    body.into_mapped_source()
}

/// What the collector does after a line: keep reading, or end the body.
enum BodyStep {
    Next,
    Stop,
}

/// Loop state of [`collect_definition_body`]: the collected lines with their
/// maps, and what the body read so far has left open.
struct DefinitionBodyCollector<'s, 'o> {
    seed: &'s str,
    content_column: usize,
    container: IndentContainer,
    options: &'s Options<'o>,
    lines: Vec<String>,
    line_map: Vec<Option<usize>>,
    // Codepoints taken off the front of each line, kept in lockstep with
    // `lines`. `None` means unknown, and a block starting there gets no
    // position rather than a guessed one (PART 12 section 4).
    col_map: Vec<Option<isize>>,
    // Did each collected line REACH the body's content column? See
    // `MappedSource::reached`: the slice at the column leaves a line one past
    // it and a line below it with the SAME residual, and only this tells them
    // apart. `:  ` bodies recorded nothing, so the rebase read an attribute
    // block written PAST the column as §10 I5's below-column lazy text
    // (markup-carve/carve#1911, corpus 444-7 against 430-3).
    reached: Vec<bool>,
    nested_definition_open: bool,
    nested_definition_column: Option<usize>,
    paragraph_column: Option<usize>,
    paragraph_marker_column: Option<usize>,
    paragraph_owner_is_item: bool,
    // A description marker can open several list levels on one line.
    // Later siblings at those columns must reach the list parser as markers.
    marker_line_sibling_columns: Vec<usize>,
    item_content_columns: Vec<usize>,
    owner_item_opens_block: bool,
    pending_first_block_column: Option<usize>,
    descendant_marker_fence: Option<(FenceOpen, usize)>,
    descendant_block_ended: bool,
    // A lazy fold leaves a paragraph open. Rechecking by parsing all collected
    // lines after each fold would make a long paragraph quadratic. Other ways
    // of extending the body clear this hint because they may open a block.
    folded_a_lazy_line: bool,
    folded_into_quote: bool,
    nested_attribute_closed: bool,
    retained_below_comment: bool,
    retained_comment_column: usize,
    retained_fence: Option<(FenceOpen, bool)>,
    // Seeded from the marker line's own content: does the body's LEAD open a
    // code fence NESTED past a list marker? Its verbatim body is the flush-left
    // lines below, which cannot close it - but an INDENTED closer written at or
    // past the fence's column does (the re-emitted, blank-less form of corpus
    // 455, carve#1970). So the state is kept alive and advanced by
    // `advance_nested_lead_fence` on each line collected, cheaply: a flush-left
    // line never closes the fence, so the blank-terminated form still asks the
    // question effectively once (the shape #1547 guards against).
    nested_lead_fence: Option<(FenceOpen, usize)>,
}

fn marker_line_sibling_columns(seed: Option<&str>, base: usize) -> Vec<usize> {
    let mut columns = Vec::new();
    let Some(mut line) = seed else { return columns };
    let mut column = base;
    while let Some(marker) = detect_list_marker_full(line) {
        let Some(width) = marker_content_col(line) else {
            break;
        };
        column += width;
        line = marker.content;
        if detect_list_marker_full(line).is_some() {
            columns.push(column);
        }
    }
    columns
}

impl<'s, 'o> DefinitionBodyCollector<'s, 'o> {
    fn new(
        seed: &'s str,
        content_column: usize,
        container: IndentContainer,
        options: &'s Options<'o>,
    ) -> Self {
        let last = seed.lines().last();
        let paragraph_owner_is_item =
            last.is_some_and(|line| is_list_marker(trim_ascii_start(line)));
        let mut nested_lead_fence = None;
        for raw in seed.lines() {
            advance_nested_lead_fence(&mut nested_lead_fence, raw);
        }
        Self {
            seed,
            content_column,
            container,
            options,
            lines: Vec::new(),
            line_map: Vec::new(),
            col_map: Vec::new(),
            reached: Vec::new(),
            nested_definition_open: seed.lines().any(is_definition_list_start),
            nested_definition_column: seed
                .lines()
                .rev()
                .find(|line| is_definition_list_start(trim_ascii_start(line)))
                .map(indent_columns),
            paragraph_column: last
                .and_then(|line| paragraph_owner_column(line, content_column))
                .filter(|column| *column > content_column),
            paragraph_marker_column: last
                .filter(|line| is_list_marker(trim_ascii_start(line)))
                .map(|_| content_column),
            paragraph_owner_is_item,
            marker_line_sibling_columns: marker_line_sibling_columns(last, content_column),
            item_content_columns: if paragraph_owner_is_item {
                last.and_then(|line| paragraph_owner_column(line, content_column))
                    .into_iter()
                    .collect()
            } else {
                Vec::new()
            },
            owner_item_opens_block: paragraph_owner_is_item,
            pending_first_block_column: last
                .filter(|line| is_plus_marker(innermost_marker_content(line)))
                .and_then(|line| paragraph_owner_column(line, content_column)),
            descendant_marker_fence: None,
            descendant_block_ended: false,
            folded_a_lazy_line: false,
            folded_into_quote: false,
            nested_attribute_closed: false,
            retained_below_comment: false,
            retained_comment_column: 0,
            retained_fence: None,
            nested_lead_fence,
        }
    }

    fn into_mapped_source(self) -> MappedSource {
        debug_assert_eq!(self.col_map.len(), self.lines.len());
        debug_assert_eq!(self.reached.len(), self.lines.len());
        MappedSource {
            col_map: self.col_map,
            source: self.lines.join("\n"),
            line_map: self.line_map,
            item_marker_at_start: false,
            marker_tail: None,
            reached: self.reached,
            opaque_blank_lines: Vec::new(),
            discarded_outer_blank: false,
            outer_plus_after_blank: false,
            in_quoted_body: false,
            sublists_carry_authored_base: false,
        }
    }

    fn push(&mut self, text: String, line: Option<usize>, col: Option<isize>, reached: bool) {
        self.lines.push(text);
        self.line_map.push(line);
        self.col_map.push(col);
        self.reached.push(reached);
    }

    /// Push the current line sliced at `columns`, as a line that did not reach.
    fn push_sliced(&mut self, cur: &LineCursor<'_>, line: &str, columns: usize) {
        let (residue, consumed, synthetic) = slice_columns_mapped(line, columns, true);
        let col = cur
            .source_col(cur.pos)
            .map(|col| col + consumed as isize - synthetic as isize);
        self.push(residue, cur.source_line(cur.pos), col, false);
    }

    /// The seed plus every collected line, as one source.
    fn body_so_far(&self) -> String {
        let mut so_far = self.seed.to_owned();
        for collected in &self.lines {
            so_far.push('\n');
            so_far.push_str(collected);
        }
        so_far
    }

    fn take_line(
        &mut self,
        cur: &mut LineCursor<'_>,
        fence: &mut DefinitionBodyFence,
        line: &str,
        blank: bool,
    ) -> BodyStep {
        let column = indent_columns(line);
        if self.nested_attribute_closed && !blank {
            let attached_plus = is_plus_marker(trim_ascii_start(line))
                && self
                    .container
                    .owns_continuation(line, cur.continuation_decision(cur.pos));
            if column < self.content_column && !attached_plus {
                return BodyStep::Stop;
            }
            self.nested_attribute_closed = false;
        }
        if let Some(retained) = self.retained_fence {
            return self.take_retained_fence_line(cur, fence, line, retained);
        }
        if is_definition_placeholder(line) && indent_columns(line) < self.content_column {
            return BodyStep::Stop;
        }
        if blank {
            self.retained_below_comment = false;
            self.paragraph_column = None;
            self.paragraph_marker_column = None;
        }
        if !blank && fence.open.is_none() {
            while self
                .item_content_columns
                .last()
                .is_some_and(|owner| column < *owner)
            {
                self.item_content_columns.pop();
            }
        }
        let opens_descendant_here = self.track_paragraph_owner(fence, line, column);
        if !opens_descendant_here
            && !blank
            && !is_line_comment_any_column(line)
            && self
                .pending_first_block_column
                .is_some_and(|owner| column >= owner)
        {
            self.pending_first_block_column = None;
        }
        if column == 0 && self.opens_comment_span_outside_body(cur, line) {
            return BodyStep::Stop;
        }
        let below_comment =
            column < self.content_column && trim_ascii_start(line).starts_with("%%");
        // Rule A (carve#3885): a comment below the body's column closes the body.
        if below_comment {
            return self.take_below_column_comment(cur, line);
        }
        if column >= self.content_column {
            self.retained_below_comment = false;
        }
        // Form B: `+` pull-left continuation.
        // A nested description at this column owns its own `+`. Keep it in
        // the outer body's stream so the inner description can pull the block.
        if is_plus_marker(trim_ascii_start(line))
            && self.nested_definition_column != Some(column)
            && self
                .container
                .owns_continuation(line, cur.continuation_decision(cur.pos))
        {
            self.take_plus_block(cur, column);
            return BodyStep::Next;
        }
        // Form A: an indented continuation line (no intervening blank).
        if !is_blank_line(line) {
            if column >= self.content_column {
                return self.take_column_line(cur, fence, line, column, opens_descendant_here);
            }
            return self.take_below_column_line(cur, fence, line, column);
        }
        if fence.open.is_some() {
            // The body's own column, like every other line this collector takes:
            // an opener PAST it leaves its authored residue for the rebase, which
            // would otherwise dedent this line a second time.
            self.push_sliced(cur, line, self.content_column);
            cur.consume();
            return BodyStep::Next;
        }
        self.take_blank_run(cur)
    }

    fn take_retained_fence_line(
        &mut self,
        cur: &mut LineCursor<'_>,
        fence: &mut DefinitionBodyFence,
        line: &str,
        (open, has_closer): (FenceOpen, bool),
    ) -> BodyStep {
        let payload_floor = if open.content_col == 0 {
            self.content_column
        } else {
            open.content_col
        };
        if !has_closer && !line.starts_with(LAZY) && indent_columns(line) < payload_floor {
            return BodyStep::Stop;
        }
        self.push_sliced(cur, line, open.content_col);
        if is_fence_close(trim_ascii_start(line), open) {
            self.retained_fence = None;
            fence.closed_last = true;
        }
        cur.consume();
        BodyStep::Next
    }

    /// Follow which paragraph owner (a nested item or description) the body's
    /// last paragraph sits in. True when this line opens a descendant.
    fn track_paragraph_owner(
        &mut self,
        fence: &DefinitionBodyFence,
        line: &str,
        column: usize,
    ) -> bool {
        if is_blank_line(line)
            || fence.open.is_some()
            || self.nested_lead_fence.is_some()
            || self.descendant_marker_fence.is_some()
        {
            return false;
        }
        let local = trim_ascii_start(line);
        let content_column = self.content_column;
        let below_owner =
            self.paragraph_column.is_some_and(|owner| column < owner) && column >= content_column;
        if below_owner && is_reference_definition(local) {
            self.end_descendant_block();
            false
        } else if below_owner
            && (column == content_column
                || !self.paragraph_owner_is_item
                || self.marker_line_sibling_columns.contains(&column)
                || is_definition_list_start(local))
            && (is_list_marker(local) || item_block_opener_with_invisible_arms(local, true))
        {
            self.adopt_paragraph_owner(local, column);
            true
        } else if (self.paragraph_marker_column == Some(column) && is_list_marker(local))
            || (column >= content_column
                && self.paragraph_column.map_or(true, |owner| column >= owner)
                && (is_list_marker(local) || strip_definition_marker(local).is_some()))
        {
            self.adopt_paragraph_owner(local, column);
            self.paragraph_column.is_some()
        } else {
            if self.paragraph_column.is_some_and(|owner| column >= owner)
                && (item_block_opener_with_invisible_arms(local, true)
                    || is_line_comment_any_column(line))
            {
                self.end_descendant_block();
            }
            false
        }
    }

    fn adopt_paragraph_owner(&mut self, local: &str, column: usize) {
        self.paragraph_owner_is_item = is_list_marker(local);
        self.owner_item_opens_block = self.lines.last().is_some_and(|prior| is_blank_line(prior));
        self.paragraph_column =
            paragraph_owner_column(local, column).filter(|owner| *owner > self.content_column);
        if self.paragraph_owner_is_item {
            while self
                .item_content_columns
                .last()
                .is_some_and(|owner| *owner > column)
            {
                self.item_content_columns.pop();
            }
            if let Some(owner) = self.paragraph_column {
                self.item_content_columns.push(owner);
            }
        }
        self.paragraph_marker_column = is_list_marker(local).then_some(column);
        self.pending_first_block_column = is_plus_marker(innermost_marker_content(local))
            .then_some(self.paragraph_column)
            .flatten();
    }

    fn end_descendant_block(&mut self) {
        self.paragraph_column = None;
        self.paragraph_marker_column = None;
        self.descendant_block_ended = true;
    }

    /// A flush-left comment fence that opens a span closing further down
    /// belongs to the surrounding container, not to this body.
    fn opens_comment_span_outside_body(&self, cur: &mut LineCursor<'_>, line: &str) -> bool {
        let authored_opener_comment = cur.source_col(cur.pos).is_some_and(|comment_col| {
            (0..cur.pos)
                .rev()
                .find(|&at| strip_description_prefix(trim_ascii_start(cur.lines.at(at))).is_some())
                .is_some_and(|at| cur.source_col(at) == Some(comment_col))
        });
        ((!cur.in_item_body && !cur.sublists_carry_authored_base) || authored_opener_comment)
            && detect_comment_fence_line_any_column(line).is_some_and(|open| {
                cur.has_comment_closer_after(cur.pos + 1, open.fence_len)
                    && !cur
                        .lines
                        .line(cur.pos + 1)
                        .is_some_and(|next| is_comment_fence_close_any_column(next, open.fence_len))
            })
    }

    /// Rule A (carve#3885): a comment below the body's column ends the body,
    /// unless it closes a comment span the body holds.
    fn take_below_column_comment(&mut self, cur: &mut LineCursor<'_>, line: &str) -> BodyStep {
        if !closes_a_held_comment_span(&self.lines, line) {
            return BodyStep::Stop;
        }
        let kept = trim_ascii_start(line).to_owned();
        let col = cur
            .source_col(cur.pos)
            .map(|col| col + (line.len() - kept.len()) as isize);
        self.push(kept, cur.source_line(cur.pos), col, false);
        cur.consume();
        BodyStep::Next
    }

    fn take_plus_block(&mut self, cur: &mut LineCursor<'_>, column: usize) {
        cur.consume();
        let mut attached = LineBuffer::default();
        let cursor_lines = cur.lines;
        let attached_start = cur.pos;
        let refused_first_plus = cur
            .continuation_decision(attached_start)
            .is_some_and(|decision| decision.refusal.is_some())
            && cursor_lines
                .line(attached_start + 1)
                .map_or(true, is_blank_line);
        let end = attached_block_lines_classified(
            cursor_lines,
            cur.pos,
            &mut cur.comment_closer_last_index,
            &mut cur.code_closer_last_index,
            self.options,
            &mut |a, at| {
                is_blank_line(a)
                    || (is_plus_marker(a) && !(at == attached_start && refused_first_plus))
                    || a.strip_prefix(":: ").is_some()
                    || strip_definition_marker(a).is_some()
            },
        );
        while cur.pos < end {
            let a = cur.lines.at(cur.pos);
            attached.push_at(
                slice_columns(a, column, true),
                cur.source_line(cur.pos),
                cur.source_col(cur.pos).map(|col| col + column as isize),
            );
            cur.consume();
        }
        if !attached.lines.is_empty() {
            self.folded_a_lazy_line = false;
            // A `+` block is pulled in FLUSH LEFT, so it has no residual
            // for the rebase to read either way.
            self.push(String::new(), None, None, false);
            for line in attached.lines {
                let source_line = line.source_line();
                self.push(line.text, source_line, line.stripped, false);
            }
        }
    }

    fn take_column_line(
        &mut self,
        cur: &mut LineCursor<'_>,
        fence: &mut DefinitionBodyFence,
        line: &str,
        indent: usize,
        opens_descendant_here: bool,
    ) -> BodyStep {
        let opener = trim_ascii_start(line);
        let nested_attribute_owner =
            opener.starts_with('{') && self.item_content_columns.contains(&indent);
        let wrapped_attribute = ((indent == self.content_column || nested_attribute_owner)
            && fence.open.is_none()
            && opener.starts_with('{')
            && parse_standalone_attrs(opener).is_none())
        .then(|| {
            standalone_attrs_block_len_iter(
                std::iter::once(opener)
                    .chain((cur.pos + 1..cur.lines.len()).map_while(|i| cur.lines.line(i))),
            )
        })
        .flatten();
        // A STRADDLING TAB LEAVES ITS RESIDUAL COLUMNS IN FRONT OF THE
        // LINE (PART 9 §24 C1 gives a tab a column value, carve-rs#793).
        // With `keep_residual` false a single tab - which reaches column
        // 4, one PAST the body's column - was consumed whole and the
        // residue landed FLUSH LEFT, where a `>` is a block opener. The
        // four-space spelling of the same column keeps its fourth space,
        // so the line sits at column 1 and is lazy text. Same column,
        // two answers.
        //
        // The residual is written back as the spaces the tab bought past
        // the margin, which is what makes the two spellings agree.
        let mut sliced = slice_columns(line, self.content_column.min(indent), true);
        let term_past_column = self
            .nested_definition_column
            .is_some_and(|term| indent < term)
            && indent > self.content_column
            && !self.descendant_block_ended
            && !is_list_marker(opener);
        if term_past_column {
            sliced.insert_str(0, LAZY);
        } else if !opens_descendant_here
            && wrapped_attribute.is_none()
            && self.folds_past_column_into_owner(line, indent)
        {
            sliced = format!("{LAZY}{}", trim_ascii_start(&sliced));
        }
        // Count what was actually removed rather than assuming three:
        // `slice_columns` works in COLUMNS, and a tab is one codepoint
        // spanning several of them. The difference in LENGTH is the
        // right quantity for both cases: with a residual it is
        // `consumed - synthetic`, which is exactly the base the mapping
        // in `slice_columns_mapped` documents.
        // As in a list item, an unterminated fence at the definition
        // body's content column stays in its open paragraph (§10 I4).
        // Tracking one as a real fence would eject the next lazy line
        // from the `<dd>` (corpus 367). Keep the established behavior
        // when a closer is written in this body, as for lists.
        //
        // §10 I4'S LOOKAHEAD IS CONDITIONAL ON A PARAGRAPH BEING OPEN,
        // and a trailing blank clears it. Asked unconditionally, it
        // declined an opener the block parser accepts, so the body folded
        // the run below its own column in as verbatim payload
        // (carve-rs#2092).
        if let Some((open, column)) = self.descendant_marker_fence {
            if indent < column || (indent == column && is_fence_close(trim_ascii_start(line), open))
            {
                self.descendant_marker_fence = None;
                self.descendant_block_ended = true;
            }
        }
        let paragraph_open = self.paragraph_open();
        let accept_fence_opener = fence.open.is_none() && detect_fence_open(&sliced).is_some();
        // The nested description's `+`, or a quote's `> +`, already separates its next
        // block. A manufactured blank here would make it attach none.
        if accept_fence_opener
            && paragraph_open
            && !(self.nested_definition_open
                && self.lines.last().is_some_and(|line| is_plus_marker(line)))
            && !self
                .lines
                .last()
                .is_some_and(|line| quote_opens_with_plus(line))
        {
            // The blank is MANUFACTURED, so it stands for no source
            // line and takes no column. Pushing the sliced line's own
            // column before the blank gave the blank that column and
            // left the fence line with none, and a block that opens on
            // a line whose column is unknown is published UNPLACED
            // (`span_of`) - which is how a fenced code block in a
            // description body lost its position and narrowed the
            // `definition_description` around it to its last placed
            // child (markup-carve/carve-rs#1833).
            self.push(String::new(), None, None, true);
        }
        // A tab can leave more synthetic spaces than consumed source
        // characters. Keep that signed difference for the later rebase.
        let sliced_col = cur.source_col(cur.pos).map(|c| {
            c + line.chars().count() as isize - strip_lazy(&sliced).chars().count() as isize
        });
        if fence.open.is_some()
            || !paragraph_open
            || detect_fence_open(trim_ascii_start(&sliced)).is_none()
            || accept_fence_opener
        {
            fence.track(&sliced, indent);
        }
        self.follow_column_line(fence, &sliced, indent);
        self.folded_a_lazy_line = false;
        self.push(sliced, cur.source_line(cur.pos), sliced_col, true);
        if indent == self.content_column
            && (cur.in_item_body || cur.sublists_carry_authored_base)
            && authored_below_body(cur, indent)
            && ends_with_closed_comment_fence(&self.lines)
        {
            self.retained_below_comment = true;
            self.retained_comment_column = indent;
        }
        cur.consume();
        self.take_wrapped_attribute_tail(cur, wrapped_attribute, nested_attribute_owner);
        BodyStep::Next
    }

    /// Does a line past the body's column, below an open descendant's own
    /// column, fold into that descendant's paragraph as lazy text?
    fn folds_past_column_into_owner(&self, line: &str, indent: usize) -> bool {
        let local = trim_ascii_start(line);
        !self
            .pending_first_block_column
            .is_some_and(|column| indent < column)
            && !(self.nested_definition_column == Some(indent) && is_plus_marker(local))
            && !is_line_comment_any_column(line)
            && detect_comment_fence_line_any_column(line).is_none()
            && detect_fence_open(local).is_none()
            && self.paragraph_column.is_some_and(|owner| indent < owner)
            && indent != self.content_column
            // An item past the body column does not reach an opener below its own column (carve#3450).
            && (self.paragraph_owner_is_item
                && (is_list_marker(local)
                    || self.owner_item_opens_block
                        && self
                            .paragraph_marker_column
                            .is_some_and(|marker| marker <= self.content_column))
                || !item_block_opener_with_invisible_arms(local, true))
    }

    fn paragraph_open(&self) -> bool {
        !self.descendant_block_ended
            && match self.lines.last() {
                Some(last) => !is_blank_line(last),
                None => self
                    .seed
                    .lines()
                    .last()
                    .is_some_and(|line| !is_blank_line(line)),
            }
    }

    /// Advance the nested lead fence and the nested description state past a
    /// line collected at the body's column.
    fn follow_column_line(&mut self, fence: &DefinitionBodyFence, sliced: &str, indent: usize) {
        // A line collected AT the body's column can be the closer of a
        // nested lead fence (its content sits at or past the fence's
        // column). Following it here releases the fence's ownership of
        // the flush-left lines below, so a `:: ` opener under a closed
        // nested fence starts a new item instead of being absorbed
        // (carve#1970). `sliced` is in the body's own coordinates, as
        // the seed and `advance_nested_lead_fence` both are.
        let prior = self
            .lines
            .last()
            .map(String::as_str)
            .or_else(|| self.seed.lines().last());
        if self.nested_lead_fence.is_some() || prior.map_or(true, is_blank_line) {
            advance_nested_lead_fence(&mut self.nested_lead_fence, sliced);
        }
        if fence.open.is_some() {
            return;
        }
        let local = trim_ascii_start(sliced);
        if is_definition_list_start(local) {
            self.nested_definition_open = true;
            self.nested_definition_column = Some(indent);
        }
        if self.nested_definition_open {
            if let Some(body) = strip_description_prefix(local) {
                let marker_column = innermost_marker_content_col(body);
                if marker_column > 0 {
                    if let Some(open) =
                        detect_fence_open(trim_ascii_start(innermost_marker_content(body)))
                    {
                        self.descendant_marker_fence =
                            Some((open, indent + local.len() - body.len() + marker_column));
                        self.descendant_block_ended = false;
                    }
                }
            }
        }
        if indent == self.content_column
            && detect_fence_open(local).is_none()
            && !is_reference_definition(local)
            && line_starts_paragraph(local)
        {
            self.descendant_block_ended = false;
        }
    }

    /// A wrapped attribute opened at the body's column is one construct:
    /// its later lines belong to it at any column (carve#3421).
    fn take_wrapped_attribute_tail(
        &mut self,
        cur: &mut LineCursor<'_>,
        wrapped_attribute: Option<usize>,
        nested: bool,
    ) {
        if nested && wrapped_attribute.is_some_and(|len| len > 1) {
            self.nested_attribute_closed = true;
        }
        for _ in 1..wrapped_attribute.unwrap_or(1) {
            let next = cur.lines.at(cur.pos);
            let col = cur
                .source_col(cur.pos)
                .map(|col| col + leading_ws(next) as isize);
            self.push(
                trim_ascii_start(next).to_owned(),
                cur.source_line(cur.pos),
                col,
                true,
            );
            cur.consume();
        }
    }

    fn take_below_column_line(
        &mut self,
        cur: &mut LineCursor<'_>,
        fence: &DefinitionBodyFence,
        line: &str,
        indent: usize,
    ) -> BodyStep {
        if self.ends_below_column(line) {
            return BodyStep::Stop;
        }
        // A COMMENT DELIMITER CLOSING A SPAN THIS BODY ALREADY HOLDS IS NOT
        // "a comment below the column" - see `body_open_comment_run`. The
        // shape test comes first because the predicate walks the collected
        // lines and a delimiter-shaped line is rare.
        if detect_comment_fence_line_any_column(line).is_some() {
            let mut probe: Vec<&str> = self.seed.lines().collect();
            probe.extend(self.lines.iter().map(String::as_str));
            if closes_a_held_comment_span(&probe, line) {
                let col = cur.source_col(cur.pos);
                self.push(
                    kept_comment_delimiter(line),
                    cur.source_line(cur.pos),
                    col,
                    false,
                );
                cur.consume();
                return BodyStep::Next;
            }
        }
        // A closed deepest fence leaves no paragraph for a below-column line.
        if fence.holds_no_paragraph() && !self.folded_a_lazy_line {
            return BodyStep::Stop;
        }
        if !self.body_takes_fold() {
            return BodyStep::Stop;
        }
        // A sibling marker normally returns to the item that supplied this
        // description. A still-open quote inside the body takes the marker's
        // lazy line first, including at that item's content column (P0 X2).
        if cur.carried_reach(cur.pos) == Some(true)
            && is_list_marker(trim_ascii_start(line))
            && !self.folded_into_quote
        {
            return BodyStep::Stop;
        }
        // A surviving quote owns this line before the definition column (CARVE-P0-004).
        if self.folded_into_quote
            && (indent > 0 || is_list_marker(trim_ascii_start(line)))
            && !trim_ascii_start(line).starts_with("%%")
        {
            self.folded_a_lazy_line = true;
            let col = cur.source_col(cur.pos);
            self.push(line.to_string(), cur.source_line(cur.pos), col, false);
            cur.consume();
            return BodyStep::Next;
        }
        self.take_lazy_line(cur, line, indent)
    }

    /// Early exits for a line below the body's column, before any fold test.
    fn ends_below_column(&self, line: &str) -> bool {
        // An unfinished nested marker fence owns no below-column line.
        if self.nested_lead_fence.is_some() {
            return true;
        }
        // A new term/definition marker ends the definition (the outer loop
        // picks it up). An EMPTY term marker opens nothing, whatever
        // whitespace trails it, so it folds like a bare `::`.
        (!line.starts_with(LAZY) && is_definition_list_start(strip_lazy(line)))
            || (!line.starts_with(LAZY) && strip_definition_marker(strip_lazy(line)).is_some())
    }

    /// AND NEITHER DOES ANY OTHER BODY THAT ENDS IN A FINISHED BLOCK.
    /// The fence guard is one shape of "the body holds no open paragraph"; an
    /// empty block quote, a closed div or admonition, a table, a thematic
    /// break, a line block and a body a block-attribute line left with no
    /// block at all are the others, and each of them folded here while the
    /// LIST twin closed (carve-rs#790). Same clause, same answer.
    fn body_takes_fold(&mut self) -> bool {
        if self.folded_a_lazy_line {
            return true;
        }
        let so_far = self.body_so_far();
        if self.retained_below_comment {
            return true;
        }
        let (takes_fold, quoted) = definition_body_fold(&so_far, self.options);
        if takes_fold {
            self.folded_into_quote = quoted;
        }
        takes_fold
    }

    fn take_lazy_line(&mut self, cur: &mut LineCursor<'_>, line: &str, indent: usize) -> BodyStep {
        // Below the body's column, a recognized structural opener returns
        // to the surviving outer container. Ordinary prose is still the
        // open definition paragraph's lazy continuation; indentation alone
        // does not manufacture a paragraph boundary.
        //
        // AND AN INVISIBLE LINE IS NOT AN OPENER HERE. §10 I5 makes a
        // footnote definition and a block-attribute line at a nonzero column
        // below the content column lazy paragraph text of THIS container
        // (markup-carve/carve#1809, corpus 430-2 and 430-3). This guard is
        // the one that ejected them: the interrupt test below never got the
        // line.
        let opener = trim_ascii_start(line);
        let fold_below_column_opener = self.folds_below_column_opener(opener, indent);
        if self.opener_ends_body(cur, line, indent, fold_below_column_opener) {
            return BodyStep::Stop;
        }
        // Lazy continuation: a flush-left line with no blank before it that
        // does not start an interrupting block folds into the open
        // paragraph (the same rule list items and block quotes use, matching
        // djot). A block opener ends the definition.
        // The plain lazy frame carries no authored residual indentation.
        let lazy_trim = if line.starts_with(LAZY) && !line.starts_with(LAZY_INDENT) {
            strip_lazy(line).len() - trim_ascii_start(strip_lazy(line)).len()
        } else {
            0
        };
        let owned = if line.starts_with(LAZY) && !line.starts_with(LAZY_INDENT) {
            format!("{LAZY}{}", trim_ascii_start(strip_lazy(line)))
        } else if indent > 0 {
            trim_ascii_start(line).to_string()
        } else {
            line.to_string()
        };
        let below_the_column = indent > 0;
        // List markers do not interrupt the open paragraph (§10 I2).
        // Fences and captions retain their separate boundary rules.
        if !below_the_column
            && cur.at_document_level
            && detect_fence_open(&owned).is_some()
            && !self.retained_below_comment
        {
            return BodyStep::Stop;
        }
        let opens_retained_fence = self.opens_retained_fence(cur, &owned, indent);
        let below_owner = self.below_owner_column(&owned, indent);
        if !(caption_content(&owned).is_some()
            || fold_below_column_opener
            || opens_retained_fence
            || below_owner
            || !interrupts_paragraph_in_band(cur, &owned, !below_the_column))
        {
            return BodyStep::Stop;
        }
        let keep = if caption_content(&owned).is_some() {
            format!("{LAZY}{owned}")
        } else if opens_retained_fence {
            owned.clone()
        } else if fold_below_column_opener || below_owner {
            format!("{LAZY}{}", trim_ascii_start(line))
        } else if below_the_column {
            line.to_string()
        } else if detect_fence_open(&owned).is_some() {
            // ONE COLUMN, AS THE LIST TWIN KEEPS IT. The body dedents
            // its own lines by the content margin, so a line pushed at
            // column 0 arrives AT that column on the reparse and the
            // fence shape opens a block one level in, where S10 I5 has
            // just made it the paragraph's text (carve-rs#2313). Only a
            // fence gets the column: a list marker given one opens a
            // second list where the fold continues the first.
            format!("{LAZY}{owned}")
        } else {
            owned
        };
        self.folded_a_lazy_line = true;
        if opens_retained_fence {
            self.retained_fence = detect_fence_open(&keep).map(|mut open| {
                open.content_col = indent;
                let has_closer = cur.lines.range(cur.pos + 1..cur.lines.len()).any(|next| {
                    indent_columns(next) >= indent && is_fence_close(trim_ascii_start(next), open)
                });
                (open, has_closer)
            });
            self.folded_a_lazy_line = false;
        }
        self.retained_below_comment = false;
        let kept_col = if opens_retained_fence {
            cur.source_col(cur.pos)
                .map(|col| col + (line.len() - keep.len()) as isize)
        } else if lazy_trim > 0 {
            cur.source_col(cur.pos).map(|col| col + lazy_trim as isize)
        } else if keep.starts_with(LAZY) && indent > 0 {
            cur.source_col(cur.pos).map(|col| col + indent as isize)
        } else {
            cur.source_col(cur.pos)
        };
        self.push(keep, cur.source_line(cur.pos), kept_col, false);
        cur.consume();
        BodyStep::Next
    }

    /// A structural opener in the band below the body's column that still
    /// folds in as text.
    fn folds_below_column_opener(&self, opener: &str, indent: usize) -> bool {
        indent > 0
            && indent < self.content_column
            && (detect_heading(opener).is_some()
                || thematic_break_marker(opener).is_some()
                || is_table_start(opener)
                || strip_blockquote_prefix(opener).is_some()
                || caption_content(opener).is_some())
    }

    fn opener_ends_body(
        &self,
        cur: &LineCursor<'_>,
        line: &str,
        indent: usize,
        fold_below_column_opener: bool,
    ) -> bool {
        let fence_after_retained_comment = self.retained_below_comment
            && (self
                .lines
                .last()
                .is_some_and(|line| is_line_comment_any_column(line))
                || ends_with_closed_comment_fence(&self.lines))
            && (indent <= self.retained_comment_column || self.retained_comment_column == 0)
            && detect_fence_open(trim_ascii_start(line)).is_some();
        if indent > 0
            && (indent < self.content_column
                || self.paragraph_column.map_or(true, |owner| indent >= owner))
            && item_block_opener_with_invisible_arms(trim_ascii_start(line), false)
            && !fold_below_column_opener
            && !fence_after_retained_comment
        {
            return true;
        }
        self.retained_below_comment
            && cur.reached.and_then(|r| r.get(cur.pos)).copied() == Some(true)
            && detect_fence_open(trim_ascii_start(line)).is_some_and(|open| {
                cur.lines
                    .range(cur.pos + 1..cur.lines.len())
                    .any(|next| is_fence_close(trim_ascii_start(next), open))
            })
    }

    fn opens_retained_fence(&self, cur: &LineCursor<'_>, owned: &str, indent: usize) -> bool {
        self.retained_below_comment
            && (self
                .lines
                .last()
                .is_some_and(|line| is_line_comment_any_column(line))
                || ends_with_closed_comment_fence(&self.lines))
            && indent <= self.retained_comment_column
            && !(cur.in_item_body
                && cur.reached.and_then(|r| r.get(cur.pos)).copied() == Some(false)
                && detect_fence_open(owned).is_some_and(|open| open.fence_len > 3))
            && detect_fence_open(owned).is_some()
    }

    /// Text between the body's column and an open descendant's own column.
    fn below_owner_column(&self, owned: &str, indent: usize) -> bool {
        indent > 0
            && indent != self.content_column
            && detect_fence_open(owned).is_none()
            && self.paragraph_column.is_some_and(|owner| indent < owner)
    }

    /// Blank line: absorb it as a paragraph separator ONLY when a later line
    /// still continues the definition (form A); otherwise leave it for the
    /// entry separator / outer block stream.
    fn take_blank_run(&mut self, cur: &mut LineCursor<'_>) -> BodyStep {
        let container = self.container;
        let look = container.blank_run_end(cur.lines, cur.pos) - cur.pos;
        match cur.lines.line(cur.pos + look) {
            Some(after)
                if container.classify(after, || cur.continuation_decision(cur.pos + look))
                    == ContainerLine::Content =>
            {
                self.folded_a_lazy_line = false;
                for _ in 0..look {
                    let blank = cur.lines.at(cur.pos);
                    self.push_sliced(cur, blank, self.content_column);
                    cur.consume();
                }
                BodyStep::Next
            }
            _ => BodyStep::Stop,
        }
    }
}

/// Is the comment line at `indent` written below the column of the
/// description marker that opened this body?
fn authored_below_body(cur: &LineCursor<'_>, indent: usize) -> bool {
    cur.source_col(cur.pos).is_some_and(|comment_col| {
        (0..cur.pos)
            .rev()
            .find_map(|at| {
                strip_definition_marker(trim_ascii_start(cur.lines.at(at))).and_then(
                    |(_, width)| {
                        cur.source_col(at).map(|base| {
                            base + indent_columns(cur.lines.at(at)) as isize + width as isize
                        })
                    },
                )
            })
            .is_some_and(|body_col| comment_col + (indent as isize) < body_col)
    })
}

/// `> +` (or stacked `> > +`): the quote's first-block marker, whose next block it attaches.
fn quote_opens_with_plus(line: &str) -> bool {
    let Some(mut rest) = strip_blockquote_prefix(trim_ascii_start(line)) else {
        return false;
    };
    while let Some(inner) = strip_blockquote_prefix(rest) {
        rest = inner;
    }
    is_plus_marker(rest)
}

#[cfg(test)]
thread_local! {
    static DEFINITION_PAST_FENCE_SCAN_LINES: Cell<usize> = const { Cell::new(0) };
}

#[cfg(test)]
mod definition_past_fence_work {
    use super::DEFINITION_PAST_FENCE_SCAN_LINES;

    fn scanned_for(count: usize) -> (usize, usize) {
        let mut source = String::from(":: t\n: a\n");
        for _ in 0..count {
            source.push_str("     ```\n");
        }
        source.push_str(" ```\n");
        DEFINITION_PAST_FENCE_SCAN_LINES.with(|work| work.set(0));
        let _ = crate::to_html(&source);
        let scanned = DEFINITION_PAST_FENCE_SCAN_LINES.with(|work| work.get());
        (source.len(), scanned)
    }

    #[test]
    fn below_column_fence_probes_have_bounded_work() {
        let small = scanned_for(32);
        let large = scanned_for(128);
        assert!(
            small.1 <= small.0 * 8 && large.1 <= large.0 * 8,
            "scanned source lines exceed eight times input bytes: {small:?}, {large:?}"
        );
        assert!(
            large.1 <= small.1 * 6,
            "four times the source used more than six times the scan work: {small:?}, {large:?}"
        );
    }

    #[test]
    fn quote_boundary_fence_probes_have_bounded_work() {
        let measure = |count| {
            let mut source = String::from(":: t\n: a\n");
            for _ in 0..count {
                source.push_str("  ```\n");
            }
            source.push_str("> payload\n  ```\nnext\n");
            DEFINITION_PAST_FENCE_SCAN_LINES.with(|work| work.set(0));
            let _ = crate::to_html(&source);
            (
                source.len(),
                DEFINITION_PAST_FENCE_SCAN_LINES.with(|work| work.get()),
            )
        };
        let small = measure(32);
        let large = measure(128);
        assert!(small.1 <= small.0 * 8 && large.1 <= large.0 * 8);
        assert!(
            large.1 <= small.1 * 6,
            "scan work grew too fast: {small:?}, {large:?}"
        );
    }

    #[test]
    fn footnote_boundary_fence_probes_have_bounded_work() {
        let measure = |count| {
            let mut source = String::from(":: t\n: a\n");
            for _ in 0..count {
                source.push_str("  ```lang\n");
            }
            source.push_str("[^n]: note\npayload\n  ```\n");
            DEFINITION_PAST_FENCE_SCAN_LINES.with(|work| work.set(0));
            let _ = crate::to_html(&source);
            (
                source.len(),
                DEFINITION_PAST_FENCE_SCAN_LINES.with(|work| work.get()),
            )
        };
        let small = measure(32);
        let large = measure(128);
        assert!(small.1 <= small.0 * 8 && large.1 <= large.0 * 8);
        assert!(large.1 <= small.1 * 6, "{small:?}, {large:?}");
    }
}
