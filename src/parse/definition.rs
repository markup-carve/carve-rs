use super::*;

thread_local! {
    static SKIPPED_SIBLING_ABBREVIATIONS: std::cell::RefCell<Vec<Vec<AbbreviationDef>>> = const { std::cell::RefCell::new(Vec::new()) };
}

pub(in crate::parse) struct SkippedSiblingAbbreviationsGuard;

impl SkippedSiblingAbbreviationsGuard {
    pub(in crate::parse) fn install() -> Self {
        SKIPPED_SIBLING_ABBREVIATIONS.with(|cell| cell.borrow_mut().push(Vec::new()));
        Self
    }
}

impl Drop for SkippedSiblingAbbreviationsGuard {
    fn drop(&mut self) {
        SKIPPED_SIBLING_ABBREVIATIONS.with(|cell| {
            cell.borrow_mut().pop();
        });
    }
}

#[cfg(feature = "experimental-event-parser")]
pub(in crate::parse) fn append_skipped_sibling_abbreviations(
    doc: &mut Document,
    line_starts: Option<&CodepointLineStarts>,
    source: &str,
) {
    SKIPPED_SIBLING_ABBREVIATIONS.with(|cell| {
        if let Some(defs) = cell.borrow_mut().last_mut() {
            if defs.is_empty() {
                return;
            }
            let lines: Vec<&str> = if line_starts.is_some() {
                original_lines(source)
            } else {
                Vec::new()
            };
            place_hoisted_abbreviations(
                &mut doc.children,
                std::mem::take(defs).into_iter().map(|mut def| {
                    if let (Some(pos), Some(starts)) = (&mut def.pos, line_starts) {
                        apply_offsets(pos, starts);
                        anchor_on_authored_line(pos, &def.abbr, &lines, starts);
                    }
                    def
                }),
            );
            order_abbreviation_blocks(&mut doc.children, source);
        }
    });
}

/// Place hoisted definitions after the top-level block they stood among, or at
/// the end when they carry no position.
pub(in crate::parse) fn place_hoisted_abbreviations(
    blocks: &mut Vec<BlockNode>,
    defs: impl IntoIterator<Item = AbbreviationDef>,
) {
    let (mut placed, unplaced): (Vec<_>, Vec<_>) =
        defs.into_iter().partition(|def| def.pos.is_some());
    if !placed.is_empty() {
        placed.sort_by_key(|def| def.pos.as_ref().map(|pos| pos.start_line));
        let mut placed = placed.into_iter().peekable();
        let mut merged = Vec::with_capacity(blocks.len() + placed.len());
        for block in std::mem::take(blocks) {
            if let Some(start) = crate::ast_json::block_pos(&block).map(|pos| pos.start_line) {
                while let Some(def) =
                    placed.next_if(|def| def.pos.as_ref().is_some_and(|pos| pos.start_line < start))
                {
                    merged.push(BlockNode::AbbreviationDef(def));
                }
            }
            merged.push(block);
        }
        merged.extend(placed.map(BlockNode::AbbreviationDef));
        *blocks = merged;
    }
    blocks.extend(unplaced.into_iter().map(BlockNode::AbbreviationDef));
}

/// `line` with every container prefix stripped, as the prepass reads it.
fn behind_container_prefixes(line: &str) -> &str {
    let mut rest = trim_ascii_start(line);
    loop {
        let next = trim_ascii_start(strip_container_prefixes(rest, true).bare);
        if next.len() == rest.len() {
            return rest;
        }
        rest = next;
    }
}

/// Whether a line defines an abbreviation, behind any container prefixes.
pub(in crate::parse) fn source_may_hoist_abbreviation(source: &str) -> bool {
    source.contains("*[")
        && source
            .lines()
            .any(|line| detect_abbreviation_def(behind_container_prefixes(line)).is_some())
}

/// Put definitions back in authored order after a skipped one was appended
/// beyond its containing list. The HTML expansion rule uses the last spelling.
pub(in crate::parse) fn order_abbreviation_blocks(blocks: &mut [BlockNode], source: &str) {
    let mut lines: BTreeMap<(String, String), std::collections::VecDeque<usize>> = BTreeMap::new();
    for (index, line) in source.lines().enumerate() {
        // A container definition counts at its authored line too (carve#4865).
        if let Some(def) = detect_abbreviation_def(behind_container_prefixes(line)) {
            lines
                .entry((def.abbr, def.expansion))
                .or_default()
                .push_back(index);
        }
    }
    let mut defs: Vec<_> = blocks
        .iter()
        .filter_map(|block| {
            let BlockNode::AbbreviationDef(def) = block else {
                return None;
            };
            let key = (def.abbr.clone(), def.expansion.clone());
            let order = def
                .pos
                .as_ref()
                .map(|pos| pos.start_line - 1)
                .or_else(|| {
                    lines
                        .get_mut(&key)
                        .and_then(std::collections::VecDeque::pop_front)
                })
                .unwrap_or(usize::MAX);
            Some((order, def.clone()))
        })
        .collect();
    defs.sort_by_key(|(order, _)| *order);
    let mut ordered = defs.into_iter();
    for block in blocks.iter_mut() {
        if matches!(block, BlockNode::AbbreviationDef(_)) {
            *block = BlockNode::AbbreviationDef(ordered.next().unwrap().1);
        }
    }
}

pub(in crate::parse) fn with_skipped_sibling_abbreviations<T>(
    f: impl FnOnce() -> T,
) -> (T, Vec<AbbreviationDef>) {
    let _guard = SkippedSiblingAbbreviationsGuard::install();
    let result = f();
    let definitions = SKIPPED_SIBLING_ABBREVIATIONS
        .with(|cell| std::mem::take(cell.borrow_mut().last_mut().unwrap()));
    (result, definitions)
}

pub(in crate::parse) fn record_skipped_sibling_abbreviation(
    cur: &LineCursor<'_>,
    options: &Options<'_>,
) {
    if !cur.at_document_level {
        return;
    }
    let Some(mut def) = cur.peek().and_then(detect_abbreviation_def) else {
        return;
    };
    def.pos = span_of(cur, cur.pos, cur.pos + 1, options);
    SKIPPED_SIBLING_ABBREVIATIONS.with(|cell| {
        if let Some(defs) = cell.borrow_mut().last_mut() {
            defs.push(def);
        }
    });
}

thread_local! {
    static PREPASS_ABBREVIATIONS: std::cell::RefCell<Vec<(usize, AbbreviationDef)>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// A container definition the link prepass cut out, with its body line.
pub(in crate::parse) fn record_prepass_abbreviation(line: usize, def: AbbreviationDef) {
    PREPASS_ABBREVIATIONS.with(|cell| cell.borrow_mut().push((line, def)));
}

pub(in crate::parse) fn clear_prepass_abbreviations() {
    PREPASS_ABBREVIATIONS.with(|cell| cell.borrow_mut().clear());
}

pub(in crate::parse) fn take_prepass_abbreviations() -> Vec<(usize, AbbreviationDef)> {
    PREPASS_ABBREVIATIONS.with(|cell| std::mem::take(&mut *cell.borrow_mut()))
}

/// A column-0 definition a `+` skips: it defines in the document (carve#4449).
pub(in crate::parse) fn is_document_abbreviation(line: &str) -> bool {
    detect_abbreviation_def(line).is_some()
}

/// Consume the definition at the cursor and hoist it to the document.
pub(in crate::parse) fn hoist_document_abbreviation(
    cur: &mut LineCursor<'_>,
    options: &Options<'_>,
) {
    let at = cur.pos;
    if let Some(mut def) = cur.peek().and_then(detect_abbreviation_def) {
        cur.consume();
        def.pos = span_of(cur, at, at + 1, options);
        hoist_abbreviation(def);
    }
}

/// The authored span of a container definition on 1-based `line`, from its `*`.
pub(in crate::parse) fn container_abbreviation_pos(lines: &[&str], line: usize, abbr: &str) -> Pos {
    let text = lines.get(line - 1).copied().unwrap_or("");
    let lead = text
        .find(&format!("*[{abbr}]:"))
        .map_or(0, |at| text[..at].chars().count());
    Pos {
        start_line: line,
        end_line: line,
        start_column: lead + 1,
        end_column: text.chars().count() + 1,
        start_offset: 0,
        end_offset: 0,
        file: None,
    }
}

/// Start a definition's filled span at its own `*` on the authored line: a
/// container prefix stays outside, and a walked line may have been re-indented
/// (Rule A), so the column cannot come from the line the reader saw.
pub(in crate::parse) fn anchor_on_authored_line(
    pos: &mut Pos,
    abbr: &str,
    lines: &[&str],
    line_starts: &CodepointLineStarts,
) {
    let Some(text) = lines.get(pos.start_line.wrapping_sub(1)) else {
        return;
    };
    let bom = usize::from(pos.start_line == 1 && line_starts.has_leading_bom());
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let Some(at) = text.find(&format!("*[{abbr}]:")) else {
        return;
    };
    let lead = text[..at].chars().count();
    pos.start_column = 1 + bom + lead;
    if let Some(start) = line_starts.get(pos.start_line - 1) {
        pos.start_offset = start + lead;
    }
}

/// Re-anchor every document-level definition once offsets are filled.
pub(in crate::parse) fn anchor_abbreviation_defs(
    blocks: &mut [BlockNode],
    source: &str,
    line_starts: &CodepointLineStarts,
) {
    let mut lines = None;
    for block in blocks {
        if let BlockNode::AbbreviationDef(def) = block {
            if let Some(pos) = &mut def.pos {
                let lines = lines.get_or_insert_with(|| original_lines(source));
                anchor_on_authored_line(pos, &def.abbr, lines, line_starts);
            }
        }
    }
}

/// The prepass's container definitions as nodes; `authored` maps a body line to
/// its 1-based source line.
pub(in crate::parse) fn prepass_abbreviation_nodes(
    source: &str,
    positions: bool,
    authored: impl Fn(usize) -> Option<usize>,
) -> Vec<AbbreviationDef> {
    let defs = take_prepass_abbreviations();
    let lines: Vec<&str> = if positions && !defs.is_empty() {
        original_lines(source)
    } else {
        Vec::new()
    };
    defs.into_iter()
        .map(|(line, mut def)| {
            def.pos = positions
                .then(|| {
                    authored(line).map(|line| container_abbreviation_pos(&lines, line, &def.abbr))
                })
                .flatten();
            def
        })
        .collect()
}

/// Hoist the prepass's container definitions through the sibling-gap list.
#[cfg(feature = "experimental-event-parser")]
pub(in crate::parse) fn hoist_prepass_abbreviations(
    source: &str,
    positions: bool,
    authored: impl Fn(usize) -> Option<usize>,
) -> bool {
    let defs = prepass_abbreviation_nodes(source, positions, authored);
    let any = !defs.is_empty();
    defs.into_iter().for_each(hoist_abbreviation);
    any
}

/// Hoist a definition read inside a container to the document (carve#4865).
pub(in crate::parse) fn hoist_abbreviation(def: AbbreviationDef) {
    // A throwaway parse publishes nothing.
    if probing() {
        return;
    }
    SKIPPED_SIBLING_ABBREVIATIONS.with(|cell| {
        if let Some(defs) = cell.borrow_mut().last_mut() {
            defs.push(def);
        }
    });
}

/// Returns the `%%%` spans in the gap, which the caller keeps in the entry
/// before it, as carve-js does.
fn consume_invisible_definition_gap(
    cur: &mut LineCursor<'_>,
    end: usize,
    options: &Options<'_>,
) -> Vec<Comment> {
    let mut spans = Vec::new();
    while cur.pos < end {
        let span = cur.peek().and_then(detect_comment_fence_line_any_column);
        if let Some(open) = span {
            let start = cur.pos;
            if let CommentBlock::Consumed(Some(block)) = take_comment_block(cur, options) {
                if let BlockNode::Comment(comment) = *block {
                    if comment.block && cur.pos <= end {
                        spans.push(comment);
                        continue;
                    }
                }
            }
            cur.pos = start;
            cur.consume();
            while cur.pos < end {
                let close = cur
                    .peek()
                    .is_some_and(|line| is_comment_fence_close_any_column(line, open.fence_len));
                cur.consume();
                if close {
                    break;
                }
            }
        } else {
            record_skipped_sibling_abbreviation(cur, options);
            cur.consume();
        }
    }
    spans
}

/// Spans read after a term are the first blocks of the description that
/// follows (CARVE-P12-027, carve#5167). The parse keeps them in the term
/// before them until that description exists; a term only holds a block
/// comment that way. [`start_descriptions_at_their_comments`] places the span.
pub(in crate::parse) fn move_term_gap_comments(
    terms: &mut [DefinitionTerm],
    def: &mut DefinitionDef,
) {
    let mut moved = Vec::new();
    for term in terms.iter_mut() {
        let (gap, text): (Vec<_>, Vec<_>) = std::mem::take(&mut term.children)
            .into_iter()
            .partition(|child| matches!(child, InlineNode::Comment(c) if c.block));
        term.children = text;
        moved.extend(gap.into_iter().filter_map(|child| match child {
            InlineNode::Comment(comment) => Some(BlockNode::Comment(comment)),
            _ => None,
        }));
    }
    def.children.splice(0..0, moved);
}

/// A description holding comments written above its `:` starts at the first
/// of them, or omits `pos` when a term stands between (CARVE-P12-022). Runs
/// once every comment is in place, so each placement still sees the marker.
pub(in crate::parse) fn start_descriptions_at_their_comments(blocks: &mut [BlockNode]) {
    use crate::include_walk::{visit_block_children, SubtreeVisitor};
    struct Walk;
    impl SubtreeVisitor for Walk {
        fn blocks(&mut self, blocks: &mut Vec<BlockNode>) {
            start_descriptions_at_their_comments(blocks);
        }
    }
    let starts = |pos: &Pos| (pos.start_line, pos.start_column);
    for block in blocks.iter_mut() {
        if let BlockNode::DefinitionList(list) = block {
            for item in &mut list.items {
                let Some(def) = item.definitions.first_mut() else {
                    continue;
                };
                let Some(marker) = def.pos.as_ref().map(starts) else {
                    continue;
                };
                let first = def
                    .children
                    .iter()
                    .take_while(|child| matches!(child, BlockNode::Comment(_)))
                    .filter_map(crate::ast_json::block_pos)
                    .map(starts)
                    .find(|&at| at < marker);
                let Some(first) = first else {
                    continue;
                };
                let term_between = item
                    .terms
                    .iter()
                    .any(|term| term.pos.as_ref().is_some_and(|pos| first < starts(pos)));
                if term_between {
                    def.pos = None;
                } else if let Some(pos) = def.pos.as_mut() {
                    (pos.start_line, pos.start_column) = first;
                }
            }
        }
        visit_block_children(block, &mut Walk);
    }
}

/// Keep gap spans in the description before them, or in the last term.
fn keep_gap_comments(
    terms: &mut [DefinitionTerm],
    defs: &mut [DefinitionDef],
    spans: Vec<Comment>,
) {
    if let Some(def) = defs.last_mut() {
        def.children
            .extend(spans.into_iter().map(BlockNode::Comment));
    } else if let Some(term) = terms.last_mut() {
        term.children
            .extend(spans.into_iter().map(InlineNode::Comment));
    }
}

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
    // One column of indent is still below any description's content column.
    Some(
        if marker_hosted || column > 0 || indent_columns(line) == 1 {
            trim_ascii_start(line)
        } else {
            line
        },
    )
}

/// Parse a term's inline content. A term is one line, like a heading
/// (carve#4861): the next line is read on its own.
fn term_inlines(
    options: &Options<'_>,
    text: &str,
    anchor: Option<Option<(usize, isize)>>,
) -> Vec<InlineNode> {
    match anchor {
        Some(anchor) => parse_inline_lines_with_anchor(text, options, vec![anchor]),
        None => parse_inline_with_options(text, options),
    }
}

/// Find a compatible marker beyond lines that emit no visible block. Their
/// definitions have already been registered by the prepass; this walk only
/// decides whether the next visible line still belongs to this list.
fn previous_definition_line_invisible(cur: &LineCursor<'_>) -> bool {
    cur.pos.checked_sub(1).is_some_and(|at| {
        let line = cur.lines.at(at);
        !is_footnote_definition_placeholder(line)
            && (is_line_comment_any_column(line)
                || is_definition_placeholder(line)
                || detect_comment_fence_line_any_column(line).is_some())
    })
}

fn definition_sibling_after_invisible(
    cur: &LineCursor<'_>,
    marker_hosted: bool,
    list_column: usize,
    description: bool,
    allow_blank: bool,
) -> Option<usize> {
    let mut at = cur.pos;
    let mut blanks = 0;
    while let Some(line) = cur.lines.line(at) {
        if is_blank_line(line) {
            if !allow_blank {
                return None;
            }
            blanks += 1;
            if blanks >= 3 {
                return None;
            }
            at += 1;
            continue;
        }
        if let Some(open) = detect_comment_fence_line_any_column(line) {
            // An unclosed span hides the rest (carve#4907).
            let closer = (at + 1..cur.lines.len()).find(|&index| {
                is_comment_fence_close_any_column(cur.lines.at(index), open.fence_len)
            });
            at = closer.map_or(cur.lines.len(), |closer| closer + 1);
            continue;
        }
        if is_footnote_definition_placeholder(line) {
            return None;
        }
        if is_line_comment_any_column(line)
            || is_definition_placeholder(line)
            || (cur.at_document_level && detect_abbreviation_def(line).is_some())
        {
            // A definition placeholder whose source span includes hidden body
            // lines is a nested owner, not an invisible line between siblings.
            // The source map exposes that span as a jump to the next line.
            if is_definition_placeholder(line)
                && cur
                    .source_line(at)
                    .zip(cur.source_line(at + 1))
                    .is_some_and(|(current, next)| next > current + 1)
            {
                return None;
            }
            at += 1;
            continue;
        }
        if line.starts_with(ITEM_PARAGRAPH)
            || (!line.starts_with(LAZY)
                && cur.carried_reach(at) == Some(false)
                && at == cur.pos
                && !previous_definition_line_invisible(cur))
        {
            return None;
        }
        let entry = definition_entry_at(strip_lazy(line), marker_hosted, list_column)?;
        return if description {
            strip_definition_marker(entry).is_some().then_some(at)
        } else {
            is_definition_list_start(entry).then_some(at)
        };
    }
    None
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
        // Separator padding is spaces only. A tab after it begins the term's
        // content (PART 2 CARVE-P2-010), including its source position.
        let term = term.trim_start_matches(' ');
        let term_source_line = cur.source_line(cur.pos);
        let term_start = cur.pos;
        cur.consume();
        let anchor = options
            .positions
            .then(|| inline_anchor_for_line(cur, term_start, term));
        let children = term_inlines(options, trim_ascii_end(term), anchor);
        // The span covers the `:: ` marker and the term line, the same way a
        // heading's covers its `#`.
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
        while let Some(next_at) =
            definition_sibling_after_invisible(cur, marker_hosted, list_column, false, false)
        {
            let spans = consume_invisible_definition_gap(cur, next_at, options);
            keep_gap_comments(&mut terms, &mut [], spans);
            let Some(next) = cur.peek() else {
                break;
            };
            let Some(next_term) = definition_entry_at(strip_lazy(next), marker_hosted, list_column)
                .and_then(|line| line.strip_prefix(":: "))
            else {
                break;
            };
            if is_blank_line(next_term) {
                break;
            }
            // Every term keeps a content tab, not just the first in a run.
            let next_term = next_term.trim_start_matches(' ');
            let next_source_line = cur.source_line(cur.pos);
            let next_start = cur.pos;
            cur.consume();
            let anchor = options
                .positions
                .then(|| inline_anchor_for_line(cur, next_start, next_term));
            let children = term_inlines(options, trim_ascii_end(next_term), anchor);
            terms.push(DefinitionTerm {
                attrs: source_line_attrs(None, next_source_line, options),
                children,
                pos: span_of(cur, next_start, cur.pos, options),
            });
        }

        let mut defs = Vec::new();

        while let Some(next_at) =
            definition_sibling_after_invisible(cur, marker_hosted, list_column, true, true)
        {
            let spans = consume_invisible_definition_gap(cur, next_at, options);
            keep_gap_comments(&mut terms, &mut defs, spans);
            let Some(line) = cur.peek() else {
                break;
            };
            // The nested parser sees the marker after its containing body has
            // normalized indentation. A marker that did not reach that body's
            // column belongs to the surviving outer owner, even if rebasing
            // placed it at this list's apparent marker column.
            if !line.starts_with(LAZY)
                && cur.carried_reach(cur.pos) == Some(false)
                && !previous_definition_line_invisible(cur)
            {
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
            let first_block_plus = is_plus_marker(def_trimmed)
                && cur
                    .continuation_decision(def_start)
                    .is_some_and(|decision| {
                        decision.owner == Some(continuation_marker::Kind::Description)
                    });
            let mut body = if first_block_plus {
                let mut fb = LineBuffer::default();
                let lines = cur.lines;
                // Only a synthetic separator can stand between an accepted `+` and its block.
                while cur.peek().is_some_and(is_blank_line) {
                    cur.consume();
                }
                // The `+` attaches past invisible lines (carve#4449).
                let mut first_visible = cur.pos;
                while cur.lines.line(first_visible).is_some_and(|line| {
                    is_document_definition_placeholder(line)
                        || is_document_abbreviation(line)
                        || is_line_comment_any_column(line)
                }) {
                    first_visible += 1;
                }
                // A list marker inside an attached colon block is its content.
                let attached_list = cur.lines.line(first_visible).is_some_and(|line| {
                    is_list_marker(line)
                        || detect_container_open(line).is_some()
                        || detect_line_block_open(line).is_some()
                        || detect_hardbreaks_block_open(line).is_some()
                        || detect_quote_block_open(line).is_some()
                });
                let mut boundary = |a: &str, _| {
                    is_blank_line(a)
                        || is_plus_marker(a)
                        // A list marker ends an attached block that is no list (PART 0 S3, carve#4860).
                        || (!attached_list && is_list_marker(trim_ascii_start(a)))
                        || strip_lazy(a).strip_prefix(":: ").is_some()
                        || strip_definition_marker(strip_lazy(a)).is_some()
                };
                let classified = cur.continuation_decision(def_start);
                let end = match classified {
                    Some(decision)
                        if decision.owner == Some(continuation_marker::Kind::Description) =>
                    {
                        let end = attached_block_past_invisible(
                            lines,
                            first_visible,
                            &mut cur.comment_closer_last_index,
                            &mut cur.code_closer_last_index,
                            options,
                            &mut boundary,
                        );
                        if end > first_visible {
                            end
                        } else {
                            cur.pos
                        }
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
                let seed = if def_trimmed.starts_with('\t') {
                    format!("{ITEM_PARAGRAPH}{def_trimmed}")
                } else {
                    def_trimmed.to_string()
                };
                MappedSource::new_line_at(seed, def_source_line, def_source_col)
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
                // An attached fence's blank line ends its flush payload; the body
                // column still reaches it (carve#4392).
                open: if first_block_plus {
                    let source = body.source.as_str();
                    let mut lines = source.lines();
                    lines
                        .next()
                        .and_then(detect_fence_open)
                        .filter(|open| !lines.any(|line| is_fence_close(line, *open)))
                } else {
                    detect_fence_open(def_trimmed)
                },
                closed_last: false,
                line_block: if first_block_plus {
                    None
                } else {
                    detect_line_block_open(def_trimmed)
                },
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

        if let Some(def) = defs.first_mut() {
            move_term_gap_comments(&mut terms, def);
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

        let Some(next_at) =
            definition_sibling_after_invisible(cur, marker_hosted, list_column, false, true)
        else {
            break;
        };
        let spans = consume_invisible_definition_gap(cur, next_at, options);
        if let Some(item) = items.last_mut() {
            keep_gap_comments(&mut item.terms, &mut item.definitions, spans);
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
    /// An open line block's colon run: its payload opens nothing (PART 2
    /// `line_block`, carve#4748).
    line_block: Option<usize>,
}

/// Colon containers still open in a collected definition or note body. The
/// body's comment state handles opaque code and comment lines separately.
#[derive(Default)]
pub(in crate::parse) struct OpenBodyColons {
    widths: Vec<usize>,
    literal: Option<usize>,
}

impl OpenBodyColons {
    pub(in crate::parse) fn from_source(source: &str) -> Self {
        let mut result = Self::default();
        let mut comments = BodyCommentState::new();
        for line in source.lines() {
            let opaque = comments.code.is_some() || comments.comment.is_some();
            comments.observe(line);
            if !opaque && comments.code.is_none() && comments.comment.is_none() {
                result.observe(line);
            }
        }
        result
    }

    pub(in crate::parse) fn observe(&mut self, raw: &str) {
        let line = trim_ascii_start(strip_lazy(raw));
        if let Some(width) = self.literal {
            if exact_colon_fence_len(line) == Some(width) {
                self.literal = None;
            }
            return;
        }
        if let Some(width) = detect_line_block_open(line)
            .or_else(|| detect_hardbreaks_block_open(line))
            .or_else(|| detect_quote_block_open(line))
        {
            self.literal = Some(width);
            return;
        }
        track_collected_colon_fence(&mut self.widths, line, true);
    }

    pub(in crate::parse) fn closing_widths(&self) -> impl Iterator<Item = usize> + '_ {
        self.widths.iter().rev().copied()
    }
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
        if self.open.is_none() {
            let line = trim_ascii_start(dedented);
            if let Some(len) = self.line_block {
                if exact_colon_fence_len(line) == Some(len) {
                    self.line_block = None;
                }
                self.closed_last = false;
                return;
            }
            if let Some(len) = detect_line_block_open(line) {
                self.line_block = Some(len);
                self.closed_last = false;
                return;
            }
        }
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
    if body_ends_in_line_block_payload(source) {
        return false;
    }
    if body_ends_with_an_attribute_line(source) {
        return true;
    }
    source
        .lines()
        .rev()
        .find(|line| !is_blank_line(line))
        .is_some_and(|last| trim_ascii_start(last).starts_with("%%"))
}

/// Does the body end on a payload line of a line block no closer has ended?
/// That line is stanza text whatever it looks like (PART 2 `line_block`).
pub(crate) fn innermost_host_tail(mut line: &str) -> (bool, &str) {
    let mut quoted = false;
    loop {
        let tail = innermost_marker_tail(line).1;
        if let Some((rest, _)) = strip_definition_marker(tail) {
            line = trim_ascii_start(rest);
        } else if let Some(rest) = strip_blockquote_prefix(tail) {
            quoted = true;
            line = trim_ascii_start(rest);
        } else {
            return (quoted, tail);
        }
    }
}

/// `Some(stanza open)` when the body ends inside an unclosed line block.
pub(crate) fn body_ends_inside_line_block(source: &str) -> Option<bool> {
    if !source.contains(":::") {
        return None;
    }
    let mut scan = LineBlockTail::default();
    for line in source.lines() {
        scan.step(line);
    }
    scan.in_block()
}

pub(crate) fn body_ends_in_line_block_payload(source: &str) -> bool {
    line_block_payload_tail(source).is_some()
}

/// `Some(quoted)` when the body ends on a line-block payload line, with
/// whether the line block was opened inside a quote.
pub(crate) fn line_block_payload_tail(source: &str) -> Option<bool> {
    if !source.contains(":::") {
        return None;
    }
    let mut scan = LineBlockTail::default();
    for line in source.lines() {
        scan.step(line);
    }
    scan.tail()
}

/// Line-by-line state behind [`line_block_payload_tail`].
#[derive(Default)]
pub(crate) struct LineBlockTail {
    open: Option<usize>,
    code: Option<FenceOpen>,
    comment: Option<usize>,
    quoted: bool,
    host_col: usize,
    last_is_payload: bool,
    span: Option<usize>,
    base: usize,
    had_span: bool,
    item_cols: Vec<usize>,
    prior_blank: bool,
    item_host: bool,
}

impl LineBlockTail {
    pub(crate) fn step(&mut self, line: &str) {
        let prior_blank = std::mem::replace(&mut self.prior_blank, is_blank_line(line));
        let mut local = trim_ascii_start(line);
        let line_quoted = local.starts_with('>');
        let mut lead = indent_columns(line);
        while let Some(rest) = strip_blockquote_prefix(local) {
            lead = indent_columns(rest);
            local = trim_ascii_start(rest);
        }
        if let Some(run) = self.comment {
            if crate::parse::verse::verse_comment_run(local) == Some(run) {
                self.comment = None;
            }
            return;
        }
        // Fence payload can contain a literal `::: |`. It is not an opener
        // for the stanza scan used by the quote's lazy-line decision.
        if let Some(code) = self.code {
            if is_fence_close(local, code) {
                self.code = None;
            }
            return;
        }
        if self.open.is_none() {
            if let Some(run) = crate::parse::verse::verse_comment_run(local) {
                self.comment = Some(run);
                return;
            }
            if let Some(code) = detect_fence_open(local) {
                self.code = Some(code);
                return;
            }
        }
        let stanza_open = self.last_is_payload;
        self.last_is_payload = false;
        // A comment-shaped line in an open span is payload at any column (carve#5021).
        let span_comment = self.span.is_some() && local.starts_with("%%");
        // A line that misses the line block's host ends it (PART 0 S1) unless
        // it is lazy text for a stanza with content (S4, carve#4956).
        if self.open.is_some()
            && !span_comment
            && !self.quoted
            && !line_quoted
            && !is_blank_line(line)
            && indent_columns(line) < self.host_col
            && !(stanza_open && line_starts_paragraph(local))
        {
            self.open = None;
        }
        // A `%%%` span leaves nothing: the stanza reads as it stood before the
        // span opened, and a line that missed the host ended it (carve#4956).
        let missed = !is_blank_line(line)
            && !span_comment
            && if self.quoted {
                !line_quoted
            } else {
                indent_columns(line) < self.host_col
            };
        if self.open.is_some() && exact_colon_fence_len(local) != self.open {
            if let Some(run) = self.span {
                if missed {
                    self.span = None;
                } else {
                    if crate::parse::verse::verse_comment_run(local) == Some(run) {
                        self.span = None;
                    }
                    self.last_is_payload = stanza_open;
                    return;
                }
            } else if let Some(run) = crate::parse::verse::verse_comment_run(local)
                .filter(|_| lead >= self.base && lead - self.base <= 3)
            {
                self.span = Some(run);
                self.had_span = true;
                self.last_is_payload = stanza_open;
                return;
            }
        }
        match self.open {
            Some(len) if exact_colon_fence_len(local) == Some(len) => {
                self.open = None;
                self.span = None;
            }
            Some(_) => self.last_is_payload = !is_blank_line(local),
            // A line block opened past list or description markers is still a
            // stanza of this body (carve#4748, row 910).
            None => {
                let (host_quoted, tail) = innermost_host_tail(local);
                let marked = tail.len() < local.len();
                // A bare opener is hosted by the item it sits in (PART 0 S1).
                if !line_quoted && !is_blank_line(line) {
                    let indent = indent_columns(line);
                    if marked || prior_blank {
                        self.item_cols.retain(|&col| col <= indent);
                    }
                    if marked && detect_list_marker_full(local).is_some() {
                        self.item_cols.push(line.len() - tail.len());
                    }
                }
                self.open = detect_line_block_open(tail);
                self.had_span = false;
                self.quoted = line_quoted || host_quoted;
                self.host_col = if marked {
                    local.len() - tail.len()
                } else if self.quoted {
                    0
                } else {
                    let indent = indent_columns(line);
                    self.item_cols
                        .iter()
                        .copied()
                        .filter(|&col| col <= indent)
                        .max()
                        .unwrap_or(0)
                };
                self.item_host = if marked {
                    detect_list_marker_full(local).is_some()
                } else {
                    !self.quoted && self.host_col > 0
                };
                // Verse indentation is relative to the opener, so only a `%`
                // at its column opens a span (`parse_line_block`).
                self.base = if self.host_col > 0 {
                    self.host_col
                } else {
                    lead
                };
            }
        }
    }

    /// Whether a `%%%` span has opened in the current line block.
    pub(crate) fn held_span(&self) -> bool {
        self.had_span
    }

    pub(crate) fn open_span_with_stanza(&self) -> bool {
        self.span.is_some() && self.open.is_some() && self.last_is_payload
    }

    /// Whether a list item inside the scanned body hosts the line block.
    pub(crate) fn item_hosted(&self) -> bool {
        self.item_host
    }

    /// `Some(stanza open)` while the scanned body is still inside a line block.
    pub(crate) fn in_block(&self) -> Option<bool> {
        self.open.map(|_| self.last_is_payload)
    }

    pub(crate) fn tail(&self) -> Option<bool> {
        self.last_is_payload.then_some(self.quoted)
    }
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
        if attrs_block_len_at(
            indent_columns(raw_lines[start]),
            lines[start],
            raw_lines[start + 1..].iter().copied(),
        ) == Some(end - start + 1)
        {
            // §15 A5: an indented wrapped opener is an attribute line too.
            return true;
        }
        if start == 0 || is_blank_line(lines[start - 1]) {
            return false;
        }
        start -= 1;
    }
}

pub(in crate::parse) fn definition_body_fold(source: &str, options: &Options<'_>) -> (bool, bool) {
    if body_ends_in_line_block_payload(source) {
        return (true, false);
    }
    // A span leaves nothing, so the body's own line block takes no lazy line
    // once a blank ended its stanza or before it has one (carve#4956).
    if source.contains(":::") {
        let mut scan = LineBlockTail::default();
        for line in source.lines() {
            scan.step(line);
        }
        if scan.host_col == 0 && scan.in_block() == Some(false) {
            return (false, false);
        }
    }
    if body_ends_with_an_unnoded_interrupter(source)
        || (quoted_body_ends_with_an_invisible_line(source)
            && !body_ends_in_line_block_payload(source))
    {
        return (false, false);
    }
    let blocks = probe_marker_body_blocks(source, options);
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
    /// The line before this one ended a paragraph or quote a `+` attached.
    after_plus_paragraph: bool,
    plus_paragraph_ended: bool,
    nested_attribute_closed: bool,
    retained_below_comment: bool,
    /// A lazy run is folding into a line-block stanza that held a `%%%` span.
    stanza_fold: bool,
    /// The line-block scan over the body, advanced only by new lines.
    stanza_scan: Option<(LineBlockTail, usize)>,
    retained_comment_column: usize,
    comment_state: BodyCommentState,
    open_colons: OpenBodyColons,
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
        let mut comment_state = BodyCommentState::new();
        for raw in seed.lines() {
            comment_state.observe(raw);
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
            after_plus_paragraph: false,
            plus_paragraph_ended: false,
            nested_attribute_closed: false,
            retained_below_comment: false,
            stanza_fold: false,
            stanza_scan: None,
            retained_comment_column: 0,
            comment_state,
            open_colons: OpenBodyColons::from_source(seed),
            retained_fence: None,
            nested_lead_fence,
        }
    }

    fn into_mapped_source(self) -> MappedSource {
        debug_assert_eq!(self.col_map.len(), self.lines.len());
        debug_assert_eq!(self.reached.len(), self.lines.len());
        // An open fence's trailing blank survives `str::lines()` (corpus 291).
        let mut source = self.lines.join("\n");
        if self.lines.last().is_some_and(|line| line.is_empty()) {
            source.push('\n');
        }
        MappedSource {
            newlines_counted: (0, 0),
            col_map: self.col_map,
            source,
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
        let opaque = self.comment_state.code.is_some() || self.comment_state.comment.is_some();
        self.comment_state.observe(&text);
        if !opaque && self.comment_state.code.is_none() && self.comment_state.comment.is_none() {
            self.open_colons.observe(&text);
        }
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
        self.after_plus_paragraph = std::mem::take(&mut self.plus_paragraph_ended);
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
            self.stanza_fold = false;
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
            // The `+` misses the fence's owner, so it ends an unclosed fence
            // before its block is attached (PART 0 S1).
            if let Some(open) = fence.open.take() {
                let closer = char::from(open.fence_char)
                    .to_string()
                    .repeat(open.fence_len);
                self.push(closer, None, None, false);
                fence.closed_last = true;
            }
            let closing: Vec<_> = self.open_colons.closing_widths().collect();
            for width in closing {
                self.push(":".repeat(width), None, None, false);
            }
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
            // A marker-line fence ends before a lower-column opener in the
            // outer body. The rebased stream puts that opener after its own
            // blank; leave the blank with the outer fence. A body-line fence
            // still owns a blank authored before the lower opener.
            let look = self.container.blank_run_end(cur.lines, cur.pos);
            if cur.lines.line(look).is_some_and(|after| {
                indent_columns(after) < self.content_column
                    && detect_fence_open(trim_ascii_start(after)).is_some()
                    && detect_fence_open(trim_ascii_start(self.seed)).is_some()
            }) {
                return BodyStep::Stop;
            }
            // A `+` after a blank does not continue the body, and an open fence
            // does not extend its reach (CARVE-P0-014).
            if cur.lines.line(look).is_some_and(|after| {
                self.container
                    .classify(after, || cur.continuation_decision(look))
                    == ContainerLine::OwnedContinuation
            }) {
                self.keep_blanks_for_open_fence(cur, look);
                return BodyStep::Stop;
            }
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
        // The shape test first: the opener walk below is linear in the body,
        // so asking it of every line made a long lazy body quadratic.
        let Some(open) = detect_comment_fence_line_any_column(line) else {
            return false;
        };
        if self.comment_state.comment.is_some() {
            return false;
        }
        let authored_opener_comment = || {
            cur.source_col(cur.pos).is_some_and(|comment_col| {
                (0..cur.pos)
                    .rev()
                    .find(|&at| {
                        #[cfg(test)]
                        DEFINITION_OPENER_WALK_LINES.with(|work| work.set(work.get() + 1));
                        strip_description_prefix(trim_ascii_start(cur.lines.at(at))).is_some()
                    })
                    .is_some_and(|at| cur.source_col(at) == Some(comment_col))
            })
        };
        ((!cur.in_item_body && !cur.sublists_carry_authored_base) || authored_opener_comment())
            && cur.has_comment_closer_after(cur.pos + 1, open.fence_len)
            && !cur
                .lines
                .line(cur.pos + 1)
                .is_some_and(|next| is_comment_fence_close_any_column(next, open.fence_len))
    }

    /// Rule A (carve#3885): a comment below the body's column ends the body,
    /// unless it closes a comment span the body holds.
    fn take_below_column_comment(&mut self, cur: &mut LineCursor<'_>, line: &str) -> BodyStep {
        // Inside a held span a `%%` line is payload, not a line that misses
        // the body (carve#4907).
        let payload = self.comment_state.comment.is_some();
        if !payload {
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
        let content_column = self.content_column;
        let attached_paragraph = cur
            .peek()
            .is_some_and(|line| line_starts_paragraph(trim_ascii_start(line)));
        let end = attached_block_past_invisible(
            cursor_lines,
            cur.pos,
            &mut cur.comment_closer_last_index,
            &mut cur.code_closer_last_index,
            self.options,
            &mut |a, _| {
                is_blank_line(a)
                    || is_plus_marker(a)
                    // A list marker ends an attached paragraph (PART 0 S3, carve#4860).
                    || (attached_paragraph && is_list_marker(trim_ascii_start(a)))
                    || a.strip_prefix(":: ").is_some()
                    || strip_definition_marker(a).is_some()
                    // Below the body's column a description marker opens a sibling (PART 0 S1).
                    || (indent_columns(a) < content_column
                        && strip_definition_marker(trim_ascii_start(a)).is_some())
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
        // A quote the `+` attached keeps its lazy text open the same way.
        self.plus_paragraph_ended = !attached.lines.is_empty()
            && (attached_paragraph || attached.lines[0].text.starts_with('>'));
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
        // §15 A5: a wrapped block past the body's column is an attribute line too.
        let wrapped_attribute = ((indent >= self.content_column || nested_attribute_owner)
            && fence.open.is_none()
            && opener.starts_with('{')
            && parse_standalone_attrs(opener).is_none())
        .then(|| {
            attrs_block_len_at(
                indent_columns(line),
                opener,
                (cur.pos + 1..cur.lines.len()).map_while(|i| cur.lines.line(i)),
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
        let accept_fence_opener =
            fence.open.is_none() && !self.in_line_block() && detect_fence_open(&sliced).is_some();
        // The nested description's `+`, or a quote's `> +`, already separates its next
        // block. A manufactured blank here would make it attach none.
        if accept_fence_opener
            && paragraph_open
            && !ends_in_refused_brace_run(&self.lines)
            && !self.last_line_opens_marker_fence()
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
        let opener_shift =
            indent.saturating_sub(self.lines.last().map_or(0, |l| indent_columns(l)));
        self.take_wrapped_attribute_tail(
            cur,
            wrapped_attribute,
            nested_attribute_owner,
            opener_shift,
        );
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

    /// Does the last body line open a fence on a list item's marker line? The
    /// line after it is that fence's payload or closer, not a block after a
    /// paragraph.
    fn last_line_opens_marker_fence(&self) -> bool {
        let last = self
            .lines
            .last()
            .map(String::as_str)
            .or_else(|| self.seed.lines().last());
        last.is_some_and(|line| {
            let line = trim_ascii_start(line);
            let bottom = innermost_marker_content(line);
            bottom.len() != line.len() && detect_fence_open(bottom).is_some()
        })
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
        opener_shift: usize,
    ) {
        if nested && wrapped_attribute.is_some_and(|len| len > 1) {
            self.nested_attribute_closed = true;
        }
        for _ in 1..wrapped_attribute.unwrap_or(1) {
            let next = cur.lines.at(cur.pos);
            // Shifted with its opener, so the run still reads past its brace (carve#5080).
            let pad = indent_columns(next).saturating_sub(opener_shift);
            let col = cur
                .source_col(cur.pos)
                .map(|col| col + leading_ws(next) as isize - pad as isize);
            self.push(
                format!("{}{}", " ".repeat(pad), trim_ascii_start(next)),
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
        if self.comment_state.comment.is_some_and(|run| {
            !is_comment_fence_close_any_column(line, run)
                && cur.has_comment_closer_after(cur.pos + 1, run)
        }) {
            return BodyStep::Stop;
        }
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
        // A caption after the fence opens inline text that folds like a
        // paragraph (CARVE-P2-021), even when the preceding block is closed.
        let caption_open = self
            .lines
            .last()
            .is_some_and(|line| caption_content(trim_ascii_start(line)).is_some());
        if fence.holds_no_paragraph() && !self.folded_a_lazy_line && !caption_open {
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
        // A list marker that misses a marker body closes it, and a quote in the
        // body takes no unmarked marker (PART 0 S3, carve#4860).
        if !line.starts_with(LAZY) && is_list_marker(trim_ascii_start(line)) {
            return BodyStep::Stop;
        }
        // So does a description marker after a `+` paragraph: it opens a sibling (PART 0 S1).
        if self.after_plus_paragraph
            && !line.starts_with(LAZY)
            && strip_definition_marker(trim_ascii_start(line)).is_some()
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
        // A list marker that misses a marker body closes it (PART 0 S3, carve#4860).
        if !line.starts_with(LAZY) && indent < self.content_column && is_list_marker(opener) {
            return BodyStep::Stop;
        }
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
        } else if self.folds_into_a_spanned_stanza() {
            self.stanza_fold = true;
            // The frame lets an unclosed `%%%` span end at this line (carve#4956).
            format!("{LAZY}{}", trim_ascii_start(strip_lazy(line)))
        } else if opens_retained_fence {
            owned.clone()
        } else if fold_below_column_opener || below_owner {
            format!("{LAZY}{}", trim_ascii_start(line))
        } else if below_the_column {
            line.to_string()
        } else if detect_list_marker_full(&owned).is_some() {
            // PART 0 S2: the marker misses the description, so it cannot join a list in it.
            format!("{LAZY}{owned}")
        } else if detect_fence_open(&owned).is_some() {
            // ONE COLUMN, AS THE LIST TWIN KEEPS IT. The body dedents
            // its own lines by the content margin, so a line pushed at
            // column 0 arrives AT that column on the reparse and the
            // fence shape opens a block one level in, where S10 I5 has
            // just made it the paragraph's text (carve-rs#2313).
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

    fn folds_into_a_spanned_stanza(&mut self) -> bool {
        if self.stanza_fold {
            return true;
        }
        self.in_line_block()
            && self
                .stanza_scan
                .as_ref()
                .is_some_and(|(scan, _)| scan.held_span() && scan.in_block() == Some(true))
    }

    fn in_line_block(&mut self) -> bool {
        let seed = self.seed;
        let (scan, seen) = self.stanza_scan.get_or_insert_with(|| {
            let mut scan = LineBlockTail::default();
            for line in seed.lines() {
                scan.step(line);
            }
            (scan, 0)
        });
        for line in &self.lines[*seen..] {
            scan.step(line);
        }
        *seen = self.lines.len();
        scan.in_block().is_some()
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
        // A verse span leaves its prior stanza unchanged, including across a
        // blank whose next line misses this description (CARVE-P9-047).
        if self.in_line_block()
            && self
                .stanza_scan
                .as_ref()
                .is_some_and(|(scan, _)| scan.open_span_with_stanza())
        {
            for _ in 0..look {
                let blank = cur.lines.at(cur.pos);
                self.push_sliced(cur, blank, self.content_column);
                cur.consume();
            }
            return BodyStep::Next;
        }
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
            // A blank inside a live span is payload; a comment line below the
            // column still closes it (PART 9 §28, carve#4996).
            Some(after)
                if trim_ascii_start(strip_lazy(after)).starts_with("%%")
                    && self.comment_state.comment.is_some() =>
            {
                for _ in 0..look {
                    let blank = cur.lines.at(cur.pos);
                    self.push_sliced(cur, blank, self.content_column);
                    cur.consume();
                }
                BodyStep::Next
            }
            _ => {
                // A fence the marker line opens past a list marker is open too (carve#4741).
                if self.nested_lead_fence.is_some() || body_lines_hold_open_code_fence(&self.lines)
                {
                    self.keep_blanks_for_open_fence(cur, cur.pos + look);
                }
                BodyStep::Stop
            }
        }
    }

    /// A blank run that ends the body still reaches its open fence (PART 0 S2),
    /// and stays with the cursor as the outer separator.
    fn keep_blanks_for_open_fence(&mut self, cur: &LineCursor<'_>, end: usize) {
        for at in cur.pos..end {
            let (residue, consumed, synthetic) =
                slice_columns_mapped(cur.lines.at(at), self.content_column, true);
            let col = cur
                .source_col(at)
                .map(|col| col + consumed as isize - synthetic as isize);
            self.push(residue, cur.source_line(at), col, false);
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
thread_local! {
    static DEFINITION_OPENER_WALK_LINES: Cell<usize> = const { Cell::new(0) };
}

#[cfg(test)]
mod definition_opener_walk_work {
    use super::DEFINITION_OPENER_WALK_LINES;

    fn walked_for(payload: &str, count: usize) -> usize {
        let mut source = format!("- - :: u\n    : ::: |\n      {payload}\n");
        source.push_str(&"b\n".repeat(count));
        DEFINITION_OPENER_WALK_LINES.with(|work| work.set(0));
        let _ = crate::to_html(&source);
        DEFINITION_OPENER_WALK_LINES.with(|work| work.get())
    }

    #[test]
    fn lazy_lines_under_a_nested_stanza_skip_the_opener_walk() {
        for payload in ["x", "{.k}", "[x]: /u"] {
            let small = walked_for(payload, 64);
            let large = walked_for(payload, 256);
            assert!(
                large <= small * 6,
                "{payload}: four times the lazy lines used more than six times the walk: {small}, {large}"
            );
        }
    }
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
