use super::*;

/// Follow the nested lead fence over ONE line, in the body's own coordinates
/// (the description marker already stripped). `collect_definition_body` keeps
/// this state alive as it collects rather than deciding it once from the seed:
/// the verbatim body owns lines only while the fence is OPEN. A closer at or
/// past the fence's column closes it (carve#1970), and a nonblank line below
/// that column leaves the item that owns it (PART 0 S4, carve#4468).
pub(in crate::parse) fn advance_nested_lead_fence(
    open: &mut Option<(FenceOpen, usize)>,
    raw: &str,
) {
    let line = strip_lazy(raw);
    let indent = indent_columns(line);
    let content = trim_ascii_start(innermost_marker_content(line));
    match *open {
        Some((fence, col)) => {
            if (!is_blank_line(line) && indent < col)
                || (indent >= col && is_fence_close(content, fence))
            {
                *open = None;
            }
        }
        None => {
            // ONLY a fence NESTED past a list marker owns the flush-left body.
            // A fence at the body's OWN column (`:  ``` ` directly on the
            // description marker) is the body's own lead fence and closes on a
            // below-column line, exactly as a single list item's does (corpus
            // 276); the marker-content column is 0 there and nonzero here.
            let marker_col = innermost_marker_content_col(line);
            if marker_col > 0 {
                if let Some(fence) = detect_fence_open(content) {
                    *open = Some((fence, indent + marker_col));
                }
            }
        }
    }
}

pub(in crate::parse) fn item_block_opener(line: &str) -> bool {
    item_block_opener_with_invisible_arms(line, true)
}

/// Would `line` open a block at its container's column? The lint for a line
/// folded into a definition term asks this; attribute lines are left out.
pub(crate) fn opens_block_for_term_lint(line: &str) -> bool {
    let colons = line.bytes().take_while(|b| *b == b':').count();
    item_block_opener_with_invisible_arms(line, false)
        || (colons >= 3 && matches!(line.as_bytes().get(colons), None | Some(b' ')))
        || parse_footnote_def_line(line).is_some()
        || parse_link_def_line(line).is_some()
}

/// `item_block_opener`, with the INVISIBLE arms switchable.
///
/// `invisible_arms` is false only for a container's BELOW-COLUMN band, where §10
/// I5 makes a footnote definition and a block-attribute line lazy paragraph text
/// OF THAT CONTAINER rather than openers (markup-carve/carve#1809). The amended
/// BELOW THE BODY'S COLUMN THE BODY ENDS bullet is explicit that it is about
/// openers, and a non-opener still folds - so the question the band asks is
/// "is this an opener", and these two kinds stop being one there.
///
/// PARAMETERIZED RATHER THAN COPIED. A second copy of the opener set is
/// how one rule acquires two spellings and they drift; the band's exclusion is
/// one argument, and every other caller keeps the full set.
///
/// The COMMENT FENCE stays in both sets: a comment is column-exempt (PART 9 §24)
/// and renders nothing at any column, which corpus 430-5 pins.
pub(in crate::parse) fn item_block_opener_with_invisible_arms(
    line: &str,
    invisible_arms: bool,
) -> bool {
    detect_fence_open(line).is_some()
        || detect_comment_fence_line(line).is_some()
        || thematic_break_marker(line).is_some()
        || detect_heading(line).is_some()
        || strip_blockquote_prefix(line).is_some()
        || is_table_start(line)
        || is_definition_list_start(line)
        || detect_container_open(line).is_some()
        || detect_quote_block_open(line).is_some()
        || detect_line_block_open(line).is_some()
        || (invisible_arms && parse_footnote_def_line(line).is_some())
        || (invisible_arms && parse_standalone_attrs(line).is_some())
        || detect_block_image(line).is_some()
}

/// Put the blocks [`parse_item_chunk`] handed back into the item.
///
/// Through the worklist when there is one, so the chunk keeps its place among
/// the item's other chunks: an item's own body is usually still waiting when a
/// sub-list or a continuation lands beside it, and appending straight to
/// `children` here would put this chunk in front of a body written above it.
pub(in crate::parse) fn place_item_chunk(
    deferred: &mut Vec<ItemBody>,
    item: usize,
    blocks: Vec<BlockNode>,
    carried: Vec<PendingBody>,
) {
    deferred.push(ItemBody {
        item,
        chunk: Chunk::Parsed { blocks, carried },
        // Already parsed, so no state has to be reinstated for it.
        depth: 0,
        in_group: false,
        attrs: None,
        drop_empty_paragraphs: false,
        suppress_tree_looseness: false,
        item_shown: true,
        widen_list: false,
        join_previous_definition: false,
    });
}

/// A continuation can add a term to the definition list in the preceding chunk.
pub(in crate::parse) fn place_plus_item_chunk(
    deferred: &mut Vec<ItemBody>,
    item: usize,
    block: BlockNode,
) {
    deferred.push(ItemBody {
        item,
        join_previous_definition: matches!(block, BlockNode::DefinitionList(_)),
        chunk: Chunk::Parsed {
            blocks: vec![block],
            carried: Vec::new(),
        },
        depth: 0,
        in_group: false,
        attrs: None,
        drop_empty_paragraphs: false,
        suppress_tree_looseness: false,
        item_shown: true,
        widen_list: false,
    });
}

/// Resolve a list's own pending chunks right here, for a caller that cannot
/// hand them upwards.
///
/// [`parse_blocks`] knows where the list node lands in its output and rebases
/// the chunks onto it, which is what keeps the descent flat. The three other
/// `parse_block` callers - the attached-block probe and the two `+`
/// continuation paths - hold the node themselves, so their chunks are resolved
/// against a one-element arena instead. Same worklist, one level of it.
pub(in crate::parse) fn drained_list_bodies(
    node: Box<BlockNode>,
    deferred: Vec<ItemBody>,
    options: &Options<'_>,
) -> Box<BlockNode> {
    if deferred.is_empty() {
        return node;
    }
    let mut out = vec![*node];
    let pending = deferred
        .into_iter()
        .map(|body| PendingBody {
            index: 0,
            body: Body::Item(body),
        })
        .collect();
    resolve_pending_bodies(&mut out, pending, options);
    Box::new(out.pop().expect("the node the worklist just filled in"))
}

/// Run `parse` and resolve every body it left pending, so the blocks handed
/// back are complete. Every entry point above the level loop goes through here;
/// only the worklist itself parses a level without draining it.
pub(in crate::parse) fn drained<F>(options: &Options<'_>, parse: F) -> Vec<BlockNode>
where
    F: FnOnce(&mut Vec<PendingBody>) -> Vec<BlockNode>,
{
    let mut pending = Vec::new();
    let mut out = parse(&mut pending);
    resolve_pending_bodies(&mut out, pending, options);
    out
}

/// Resolve container bodies without a stack frame per level, then stitch child
/// slots into their parents in reverse discovery order. Each child is filled
/// before its parent.
pub(in crate::parse) fn resolve_pending_bodies(
    out: &mut Vec<BlockNode>,
    pending: Vec<PendingBody>,
    options: &Options<'_>,
) {
    if pending.is_empty() {
        return;
    }
    // Slot 0 is the caller's own output; every other slot is one body.
    let mut arena: Vec<Vec<BlockNode>> = vec![std::mem::take(out)];
    let mut links: Vec<ResolvedBody> = Vec::new();
    let mut work: Vec<(usize, PendingBody)> =
        pending.into_iter().map(|body| (0usize, body)).collect();
    while let Some((slot, entry)) = work.pop() {
        let mut nested = Vec::new();
        let (children, item, join_previous_definition, loose_inside_chunk, layout) = match entry
            .body
        {
            Body::Colon(body) => {
                // The body parses under the state its OPENER stood in, not the
                // state the worklist happens to be in - see `ContainerBody`.
                let _depth = DepthScope::set(body.depth);
                let _group = FigureGroupScope::set(body.in_group);
                (
                    parse_colon_view(body.inner, options, &mut nested),
                    None,
                    false,
                    false,
                    None,
                )
            }
            Body::Item(body) => {
                let target = (body.item, body.widen_list);
                let join_previous_definition = body.join_previous_definition;
                let (children, loose, layout) = parse_item_body_into(body, options, &mut nested);
                (
                    children,
                    Some(target),
                    join_previous_definition,
                    loose,
                    layout,
                )
            }
        };
        let child_slot = arena.len();
        arena.push(children);
        links.push(ResolvedBody {
            slot,
            index: entry.index,
            item,
            join_previous_definition,
            child_slot,
            loose_inside_chunk,
            layout,
        });
        work.extend(nested.into_iter().map(|body| (child_slot, body)));
    }
    for link in links.into_iter().rev() {
        let mut children = std::mem::take(&mut arena[link.child_slot]);
        let mut loose_inside_chunk = link.loose_inside_chunk;
        if let Some(layout) = link.layout {
            // A `+` after a blank below a paragraph-only sublist attaches
            // inside that sublist. The blank is not between this item's
            // visible blocks. An opaque first block can leave the `+` for
            // this item instead, so retain that case here.
            loose_inside_chunk |= layout.outer_plus_after_blank
                && children.iter().any(|child| match child {
                    BlockNode::List(list) => list.items.iter().any(|item| {
                        item.children
                            .iter()
                            .any(|block| !matches!(block, BlockNode::Paragraph(_)))
                    }),
                    _ => false,
                });
            loose_inside_chunk |= item_chunk_tree_looseness(
                &layout.source,
                &layout.opaque_blank_lines,
                &layout.separator_lines,
                layout.discarded_outer_blank,
                &children,
                &layout.extents,
                layout.item_shown,
            );
            if layout.discarded_outer_blank && !loose_inside_chunk {
                mark_nested_discarded_gap(&mut children);
            }
        }
        let node = &mut arena[link.slot][link.index];
        match link.item {
            None => fill_container_children(node, children),
            Some((item, widen_list)) => append_item_children(
                node,
                item,
                children,
                widen_list,
                loose_inside_chunk,
                link.join_previous_definition,
            ),
        }
    }
    *out = std::mem::take(&mut arena[0]);
}

pub(in crate::parse) fn parse_blocks(
    cur: &mut LineCursor,
    options: &Options<'_>,
    pending: &mut Vec<PendingBody>,
) -> Vec<BlockNode> {
    // Recursion cap (see MAX_NESTING_DEPTH). Over the cap, flatten everything
    // still in the cursor into one paragraph rather than recursing further,
    // matching the carve-php degrade behavior. The degrade itself lives in
    // `over_cap_paragraph`, out of this frame - see its doc comment.
    let Some(_depth) = DepthGuard::enter() else {
        return over_cap_paragraph(cur, options);
    };
    #[cfg(test)]
    BLOCK_SEQUENCE_PARSES.with(|count| count.set(count.get() + 1));
    let mut out = Vec::new();
    let mut pending_attrs: Box<Option<Attrs>> = Box::new(None);
    let mut pending_attr_slots = None;
    // Where the pending run STARTED, for §15 A4's diagnostic. The FIRST block of
    // a stacked run, because that is where the author began writing attributes
    // that reach nothing - the run merges into one set and is reported once.
    let mut pending_attrs_pos: Box<Option<Pos>> = Box::new(None);
    while !cur.eof() {
        let line = cur.peek().unwrap();
        // A standalone `{attr}` block opener fires only at the container's
        // content column (flush-left here, since the caller has dedented to that
        // column). An INDENTED `{attr}` line does NOT attach to the following
        // block; it folds as literal paragraph text (strict column-0 rule,
        // docs/divergence-from-djot.md §11), matching carve-php / carve-js.
        let line_flush = !line.starts_with([' ', '\t']);
        if is_blank_line(line) {
            cur.consume();
            continue;
        }
        // BOTH comment spellings, shared with `parse_block` so a comment
        // reached through a `+` continuation is the same node as one written at
        // top level. It was not: this loop had the arm and the single-block
        // parser did not, so a `+`-attached `%% c` fell through to a paragraph
        // wrapping an INLINE comment (carve-rs#678).
        let comment_start = cur.pos;
        match take_comment_block(cur, options) {
            CommentBlock::NotAComment => {}
            CommentBlock::Consumed(None) => continue,
            CommentBlock::Consumed(Some(node)) => {
                out.push(*node);
                if let Some(extents) = cur.block_extents.as_mut() {
                    extents.push((comment_start, cur.pos));
                }
                cur.last_block_start = Some(comment_start);
                if pending_attrs.is_none() {
                    if let Some(starts) = cur.block_starts.as_mut() {
                        starts.push(comment_start);
                    }
                }
                continue;
            }
        }
        if (line_flush
            || (trim_ascii_start(line).starts_with('{')
                && indented_wrapped_attrs_block_len(
                    (cur.pos..cur.lines.len()).map_while(|i| cur.lines.line(i)),
                )
                .is_some()))
            && take_standalone_attrs(
                cur,
                options,
                &mut pending_attrs,
                &mut pending_attr_slots,
                &mut pending_attrs_pos,
            )
        {
            continue;
        }
        let start_line = cur.pos;
        // The container's body is COLLECTED here and parsed later, on the heap,
        // so nesting does not descend the stack (see `PendingBody`).
        let mut opened = None;
        // And so are a list's item bodies. `parse_list` collects each chunk but
        // does not know where its own node will land, so it hands the chunks
        // back and THIS loop - which does know - rebases them (`ItemBody`).
        let mut item_bodies = Vec::new();
        let parsed = if !line.starts_with([' ', '\t']) && detect_container_open(line).is_some() {
            let (node, body) = open_container(cur, options);
            opened = Some(body);
            Some(node)
        } else {
            parse_block(cur, options, &mut item_bodies)
        };
        // Pending attributes float past it to the next visible block.
        if let Some(comments) = parsed
            .as_deref()
            .and_then(crate::ast::comment_only_paragraph_blocks)
        {
            for comment in comments {
                out.push(comment);
                if let Some(extents) = cur.block_extents.as_mut() {
                    extents.push((start_line, cur.pos));
                }
                if pending_attrs.is_none() {
                    if let Some(starts) = cur.block_starts.as_mut() {
                        starts.push(start_line);
                    }
                }
            }
            cur.last_block_start = Some(start_line);
            continue;
        }
        if let Some(node) = parsed {
            if let Some(extents) = cur.block_extents.as_mut() {
                extents.push((start_line, cur.pos));
            }
            if pending_attrs.is_none() {
                if let Some(starts) = cur.block_starts.as_mut() {
                    starts.push(start_line);
                }
            }
            append_parsed_block(
                &mut out,
                node,
                &mut pending_attrs,
                &mut pending_attrs_pos,
                start_line,
                cur,
                options,
            );
            cur.last_block_start = Some(start_line);
            // `append_parsed_block` pushes exactly one node, so the container
            // just emitted is the last one - and stays there, because this loop
            // never removes or reorders what it has pushed.
            let index = out.len() - 1;
            if let Some(body) = opened {
                pending.push(PendingBody {
                    index,
                    body: Body::Colon(body),
                });
            }
            pending.extend(item_bodies.into_iter().map(|body| PendingBody {
                index,
                body: Body::Item(body),
            }));
        }
    }
    // DROP IF DANGLING, AND SAY SO (§15 A4, markup-carve/carve#1281). The loop
    // has run out of lines, so there is no next block for this set to float to.
    // This one site covers both ways of running out, because a container's body
    // is parsed by its own call: at document level the input ended, and inside a
    // quote, an item, a `dd` or a footnote body the CONTAINER ended. Nothing is
    // emitted for the attribute either way - what changes is that the processor
    // now reports it rather than discarding it silently.
    if pending_attrs.is_some() {
        note_unattached_block_attrs(pending_attrs_pos.take());
    }
    out
}

/// Kept out of `parse_blocks`, whose frame every nesting level pays: the parsed
/// `Attrs` would otherwise hold a slot in it.
#[inline(never)]
fn take_standalone_attrs(
    cur: &mut LineCursor,
    options: &Options<'_>,
    pending_attrs: &mut Option<Attrs>,
    pending_attr_slots: &mut Option<HashSet<AttrSlot>>,
    pending_attrs_pos: &mut Option<Pos>,
) -> bool {
    let attrs_start = cur.pos;
    let Some(attrs) = parse_standalone_attrs_block(cur) else {
        return false;
    };
    merge_attrs_indexed(pending_attrs, attrs, pending_attr_slots);
    if pending_attrs_pos.is_none() {
        *pending_attrs_pos = span_of(cur, attrs_start, cur.pos, options);
    }
    true
}

/// True if `attrs` already carries a `title` key (case-insensitive, since HTML
/// attribute names are case-insensitive).
pub(in crate::parse) fn attrs_have_title(attrs: &Option<Attrs>) -> bool {
    attrs
        .as_ref()
        .is_some_and(|a| a.key_values.keys().any(|k| k.eq_ignore_ascii_case("title")))
}

/// Copy a code fence's opener `title` to the `title` attribute (unless a
/// preceding `{title=...}` line already set one, which wins). The `title` field
/// is left in place as the source of truth for non-HTML renderers.
pub(in crate::parse) fn copy_title_to_attr(cb: &mut CodeBlock) {
    let Some(title) = cb.title.clone() else {
        return;
    };
    if attrs_have_title(&cb.attrs) {
        return;
    }
    let attrs = cb.attrs.get_or_insert_with(Attrs::default);
    attrs.key_values.insert("title".to_string(), title);
    // NO ORDER SLOT. `attrs.order` is the source-appearance order of the slots
    // in a `{#id .class key=value}` block, and this title was written as fence
    // metadata - ``` rust "Example" - not as a slot in one. Recording it
    // claimed a position in a block the author never wrote (carve#785).
    // A title written in a real attribute block still takes its slot, because
    // that one goes through the attribute parser rather than here.
}

/// Resolve a code fence's opener title onto the node attrs so it renders
/// uniformly and survives a caption Figure and a FencedRender extension (which
/// clones the code block's attrs). For a captioned block a `{title=...}` line
/// attaches to the figure and wins, so the inner block's title is dropped.
pub(in crate::parse) fn resolve_code_title(node: &mut BlockNode) {
    match node {
        BlockNode::CodeBlock(cb) => copy_title_to_attr(cb),
        BlockNode::Figure(f) => {
            if let FigureTarget::CodeBlock(cb) = &mut *f.target {
                if attrs_have_title(&f.attrs) {
                    cb.title = None;
                } else {
                    copy_title_to_attr(cb);
                }
            }
        }
        _ => {}
    }
}

#[inline(never)]
pub(in crate::parse) fn boxed_block(
    cur: &mut LineCursor,
    options: &Options<'_>,
    parser: fn(&mut LineCursor, &Options<'_>) -> BlockNode,
) -> Box<BlockNode> {
    Box::new(parser(cur, options))
}

#[inline(never)]
pub(in crate::parse) fn parse_fenced_block(
    cur: &mut LineCursor,
    fence_marker: FenceOpen,
    options: &Options<'_>,
) -> Box<BlockNode> {
    let fence_at = cur.pos;
    let block = parse_fence(cur, fence_marker, options);
    if let BlockNode::CodeBlock(cb) = block {
        if let Some(caption) = consume_caption(cur, options) {
            return Box::new(BlockNode::Figure(Figure {
                attrs: None,
                target: Box::new(FigureTarget::CodeBlock(cb)),
                rendered_target: None,
                caption,
                short_caption: None,
                pos: span_of(cur, fence_at, cur.pos, options),
            }));
        }
        return Box::new(BlockNode::CodeBlock(cb));
    }
    Box::new(block)
}

#[inline(never)]
pub(in crate::parse) fn parse_heading_block(
    cur: &mut LineCursor,
    level: u8,
    first_text: &str,
    options: &Options<'_>,
) -> Box<BlockNode> {
    let span_start = cur.pos;
    cur.consume();
    let joined = first_text.to_string();
    let anchors = options
        .positions
        .then(|| vec![inline_anchor_for_line(cur, span_start, first_text)]);
    let pos = span_of(cur, span_start, cur.pos, options);
    let inline_text = trim_ascii_end(&joined);
    let children = if let Some(anchors) = anchors {
        parse_inline_lines_with_anchor(inline_text, options, anchors)
    } else {
        parse_inline_with_options(inline_text, options)
    };
    Box::new(BlockNode::Heading(Heading {
        attrs: None,
        level,
        children,
        pos,
    }))
}

pub(in crate::parse) fn parse_block(
    cur: &mut LineCursor,
    options: &Options<'_>,
    item_bodies: &mut Vec<ItemBody>,
) -> Option<Box<BlockNode>> {
    // Checked FIRST, and through the same helper `parse_blocks` uses, so a
    // comment reached by a `+` continuation is the node the identical line
    // produces at top level (carve-rs#678).
    match take_comment_block(cur, options) {
        CommentBlock::NotAComment => {}
        CommentBlock::Consumed(node) => return node,
    }
    let line = cur.peek()?;
    if let Some(fence_marker) = detect_fence_open(line) {
        return Some(parse_fenced_block(cur, fence_marker, options));
    }
    if let Some(marker) = thematic_break_marker(line) {
        let span_start = cur.pos;
        cur.consume();
        return Some(Box::new(BlockNode::ThematicBreak(ThematicBreak {
            marker: (marker != '-').then_some(marker),
            pos: span_of(cur, span_start, cur.pos, options),
            ..Default::default()
        })));
    }
    if let Some((level, first_text)) = detect_heading(line) {
        return Some(parse_heading_block(cur, level, first_text, options));
    }
    if strip_blockquote_prefix(line).is_some()
        || (cur.colon_view.is_some_and(|view| view.marker_hosted)
            && strip_blockquote_prefix(trim_ascii_start(line)).is_some())
    {
        return Some(parse_blockquote(cur, options));
    }
    if is_list_marker(line) {
        return Some(parse_list(cur, options, item_bodies));
    }
    // A table row opens a table only when FLUSH-LEFT (like a heading, quote or
    // `:: ` def-list term). `is_table_start` trims leading whitespace, so an
    // INDENTED row (`  |a|`) would otherwise wrongly open a table where the
    // reference renders a paragraph; a genuine table sits at its container's
    // content column and is already dedented to column 0 here.
    if !line.starts_with([' ', '\t']) && is_table_start(line) {
        return Some(boxed_block(cur, options, parse_table));
    }
    if is_definition_list_start(line)
        || (cur.colon_view.is_some_and(|view| view.marker_hosted)
            && is_definition_list_start(trim_ascii_start(line)))
    {
        return Some(boxed_block(cur, options, parse_definition_list));
    }
    // A `::: |` line block or `::: \` hard-break block opens ONLY flush-left
    // (at its container's content column), exactly like the div / admonition
    // container check below. `detect_line_block_open` and
    // `detect_hardbreaks_block_open` trim leading whitespace, so an INDENTED
    // colon fence (above the content column) would otherwise still open; the
    // strict column-0 rule (docs/divergence-from-djot.md §11) requires it to
    // fold as literal paragraph text instead. `line` is already dedented to
    // the content column here, so a fence sitting AT that column still opens.
    if !line.starts_with([' ', '\t']) {
        if detect_line_block_open(line).is_some() {
            return Some(boxed_block(cur, options, parse_line_block));
        }
        if detect_hardbreaks_block_open(line).is_some() {
            return Some(boxed_block(cur, options, parse_hardbreaks_block));
        }
        if detect_quote_block_open(line).is_some() {
            return Some(boxed_block(cur, options, parse_quote_block));
        }
    }
    // FLUSH-LEFT only: `detect_container_open` trims leading whitespace, so an
    // indented `::: note` below/above a list item's content column must fold as
    // lazy paragraph text (§24 C3), not open a nested container -- uniform with
    // the quote/heading/table checks. `line` is already dedented to the content
    // column here, so a `:::` at the content column opens.
    if !line.starts_with([' ', '\t']) && detect_container_open(line).is_some() {
        return Some(parse_container(cur, options));
    }
    if let Some(mut abbr) = detect_abbreviation_def(line) {
        let abbr_at = cur.pos;
        cur.consume();
        // Its own line, the same way the block image below takes one. #517
        // started publishing this node and it went out with no span at all,
        // which PART 12 §4 allows only for a node that was reassembled and has
        // no honest one - a definition the author wrote on line 1 has one.
        abbr.pos = span_of(cur, abbr_at, abbr_at + 1, options);
        if cur.at_document_level {
            return Some(Box::new(BlockNode::AbbreviationDef(abbr)));
        }
        // A definition in every container, with document scope (carve#4865).
        hoist_abbreviation(abbr);
        return None;
    }
    if let Some(mut img) = detect_block_image(line) {
        if image_is_block(cur) {
            let image_at = cur.pos;
            cur.consume();
            // The image's own line. An INLINE image gets its span from the
            // inline parser; a block image never goes through it, so it had
            // none at all.
            img.pos = span_of(cur, image_at, image_at + 1, options);
            if let Some(caption) = consume_caption(cur, options) {
                return Some(Box::new(BlockNode::Figure(Figure {
                    attrs: None,
                    target: Box::new(FigureTarget::Image(img)),
                    rendered_target: None,
                    caption,
                    short_caption: None,
                    // The figure runs from the image to the end of the caption
                    // the cursor just consumed.
                    pos: span_of(cur, image_at, cur.pos, options),
                })));
            }
            return Some(Box::new(BlockNode::Paragraph(Paragraph {
                attrs: None,
                children: vec![InlineNode::Image(img)],
                at_content_column: !line.starts_with([' ', '\t']),
                block_image: true,
                pos: span_of(cur, image_at, cur.pos, options),
            })));
        }
        // Not standalone: the image folds into a paragraph with the following
        // content (parse_paragraph below); a sole-image paragraph is still
        // promoted to a bare block image afterwards.
    } else if let Some(figure) = quiet_captioned_image(cur, options, line) {
        return Some(Box::new(figure));
    }
    if let Some(matched) = try_extension_block(cur, options) {
        return Some(Box::new(matched));
    }
    // A block whose sole content is a display-math span (`$$`…``) followed by a
    // caption is a numbered EQUATION. Diverted before the paragraph fallback so
    // parse_paragraph does not fold the caption line into the math paragraph.
    if trim_ascii_start(line).starts_with("$$`")
        || (trim_ascii_start(line).starts_with("{%") && line.contains("$$`"))
    {
        if let Some(eq) = parse_equation_block(cur, options) {
            return Some(Box::new(eq));
        }
    }
    Some(boxed_block(cur, options, parse_paragraph))
}

/// A display-math span over several lines takes a caption too (PART 9 §10 I7):
/// the paragraph's lines up to the caption must be that one span. Only the first
/// caret line can decide it, since the text after it folds into the same run.
fn parse_multiline_equation(cur: &mut LineCursor, options: &Options<'_>) -> Option<BlockNode> {
    let start = cur.pos;
    let mut end = start + 1;
    let caption_at = loop {
        let line = cur.lines.line(end)?;
        if is_blank_line(line) {
            let next = end + 1;
            cur.lines.line(next).and_then(caption_content)?;
            break end;
        }
        if caption_content(line).is_some() {
            break end;
        }
        cur.pos = end;
        let interrupts = interrupts_paragraph(cur, line);
        cur.pos = start;
        if interrupts {
            return None;
        }
        end += 1;
    };
    let lines: Vec<&str> = (start..caption_at)
        .filter_map(|at| cur.lines.line(at))
        .map(|line| trim_ascii_end(trim_ascii_start(line)))
        .collect();
    let joined = lines
        .iter()
        .map(|line| strip_lazy(line))
        .collect::<Vec<_>>()
        .join("\n");
    let inline = if options.positions {
        let anchors = lines
            .iter()
            .enumerate()
            .map(|(at, line)| inline_anchor_for_line(cur, start + at, line))
            .collect();
        parse_inline_lines_with_anchor(&joined, options, anchors)
    } else {
        parse_inline_with_options(&joined, options)
    };
    if !only_display_math(&inline) {
        return None;
    }
    cur.pos = caption_at;
    let target = FigureTarget::Paragraph(Paragraph {
        attrs: None,
        children: inline,
        pos: span_of(cur, start, caption_at, options),
        ..Default::default()
    });
    let Some(caption) = consume_caption(cur, options) else {
        cur.pos = start;
        return None;
    };
    Some(BlockNode::Figure(Figure {
        attrs: None,
        target: Box::new(target),
        rendered_target: None,
        caption,
        short_caption: None,
        pos: span_of(cur, start, cur.pos, options),
    }))
}

/// The inline content is one display math node; a comment beside it holds no
/// content but stays the paragraph's child.
fn only_display_math(inline: &[InlineNode]) -> bool {
    let mut content = inline
        .iter()
        .filter(|node| !matches!(node, InlineNode::Comment(_)));
    matches!(content.next(), Some(InlineNode::Math(m)) if m.display) && content.next().is_none()
}

/// Parse a standalone display-math line, wrapping it in a figure when a caption
/// follows (a numbered equation). Returns `None` when the line is not solely
/// display math, or when non-blank prose follows with no blank line (so the
/// line belongs to a normal multi-line paragraph instead).
pub(in crate::parse) fn parse_equation_block(
    cur: &mut LineCursor,
    options: &Options<'_>,
) -> Option<BlockNode> {
    let line = cur.peek()?;
    let inline_text = trim_ascii_start(line);
    let inline = if options.positions {
        parse_inline_lines_with_anchor(
            inline_text,
            options,
            vec![inline_anchor_for_line(cur, cur.pos, inline_text)],
        )
    } else {
        parse_inline_with_options(inline_text, options)
    };
    if !only_display_math(&inline) {
        return None;
    }
    // Non-blank, non-caption prose on the very next line: let parse_paragraph
    // fold the math and that text into one paragraph (preserve existing behavior).
    if let Some(next) = cur.lines.line(cur.pos + 1) {
        if !is_blank_line(next) && caption_content(next).is_none() {
            return parse_multiline_equation(cur, options);
        }
    }
    // Standalone display math: consume the line, then attach a caption if one
    // follows (directly or across a single blank line).
    let math_at = cur.pos;
    cur.consume();
    let target = FigureTarget::Paragraph(Paragraph {
        attrs: None,
        children: inline,
        // The paragraph is one line: the display-math line itself. It was built
        // with `..Default::default()`, so it had no span whether or not a
        // caption followed.
        pos: span_of(cur, math_at, math_at + 1, options),
        ..Default::default()
    });
    if let Some(caption) = consume_caption(cur, options) {
        return Some(BlockNode::Figure(Figure {
            attrs: None,
            target: Box::new(target),
            rendered_target: None,
            caption,
            short_caption: None,
            // Through the end of the caption, like the listing above.
            pos: span_of(cur, math_at, cur.pos, options),
        }));
    }
    match target {
        FigureTarget::Paragraph(p) => Some(BlockNode::Paragraph(p)),
        _ => unreachable!(),
    }
}

pub(in crate::parse) fn detect_heading(line: &str) -> Option<(u8, &str)> {
    let bytes = line.as_bytes();
    let mut hashes = 0usize;
    while hashes < bytes.len() && bytes[hashes] == b'#' {
        hashes += 1;
    }
    if !(1..=6).contains(&hashes) {
        return None;
    }
    if hashes >= bytes.len() || bytes[hashes] != b' ' {
        return None;
    }
    // Skip all spaces after the marker (the delimiter is one-or-more spaces;
    // per the Carve grammar it is a space, not a tab).
    let mut start = hashes;
    while start < bytes.len() && bytes[start] == b' ' {
        start += 1;
    }
    // Return the content VERBATIM (leading tab kept, trailing kept): first-line
    // trailing is interior once continuation lines fold in, so it is stripped
    // only from the final assembled content (§756). The empty gate still tests a
    // trailing-stripped view so `# `, `#  `, `# \t` are not headings.
    let rest = &line[start..];
    if trim_ascii_end(rest).is_empty() {
        return None;
    }
    Some((hashes as u8, rest))
}

/// Any ATX heading marker line (`#`..`######` followed by a space or EOL) —
/// such a line starts a NEW heading rather than continuing the current one.
pub(in crate::parse) fn is_heading_marker_line(line: &str) -> bool {
    let bytes = line.as_bytes();
    let mut hashes = 0usize;
    while hashes < bytes.len() && bytes[hashes] == b'#' {
        hashes += 1;
    }
    (1..=6).contains(&hashes) && (hashes == bytes.len() || bytes[hashes] == b' ')
}

/// A line that opens an ATX heading WITH content: `#`..`######`, then a single
/// space, then at least one non-whitespace character. Used to decide whether a
/// list item's marker-line remainder (`- # H`) opens a heading block. Bare (`#`)
/// or whitespace-only (`# `, `#  `) remainders and a tab separator (`#\tH`) are
/// NOT headings here -- they stay inline text, matching carve-js / carve-php on
/// the settled cases (the all-whitespace remainder is tracked separately).
pub(in crate::parse) fn heading_content_starts(line: &str) -> bool {
    let bytes = line.as_bytes();
    let mut hashes = 0usize;
    while hashes < bytes.len() && bytes[hashes] == b'#' {
        hashes += 1;
    }
    if !(1..=6).contains(&hashes) || bytes.get(hashes) != Some(&b' ') {
        return false;
    }
    line[hashes + 1..].bytes().any(|b| b != b' ' && b != b'\t')
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::parse) struct CommentFenceOpen {
    pub(in crate::parse) fence_len: usize,
    pub(in crate::parse) tail: String,
}

/// The same fence seen from a position that CONSUMES it.
///
/// A comment is recognized at ANY column (carve#624), but the strict form above
/// still decides where a line ENDS an item or OPENS a block, so an indented
/// fence does not close the list it sits in. Reading it only at column 0 where
/// it is consumed left an indented opener to the `%%` line-comment path, which
/// took the opener and the closer one line at a time and rendered everything
/// between them - a comment that hid its delimiters and showed its contents
/// (carve-rs#573, the same defect carve-js#630 reported there).
pub(in crate::parse) fn detect_comment_fence_line_any_column(
    line: &str,
) -> Option<CommentFenceOpen> {
    detect_comment_fence_line(trim_ascii_start(line))
}

/// The run length of a comment span still OPEN in a container's collected
/// lines, if one is (PART 9 §28, markup-carve/carve#2488).
///
/// A container's collector ends at a comment written below its content column,
/// which is right for an OPENER and for the `%%` line form and wrong INSIDE a
/// span the container already holds: §28 pairs the delimiters, indentation is
/// part of neither (markup-carve/carve#2471), and everything between them is
/// payload. Ending there SPLITS the span - the container's own parse then reads
/// an opener with no closer, §28 makes that one line comment, and the payload
/// reaches the page while both delimiters do not (markup-carve/carve-rs#2113).
///
/// Asked of the COLLECTED LINES rather than of a live fence tracker, because a
/// tracker reads each line at its own frame's dedent while the answer must not
/// turn on that: it is the same question about the same span either way.
///
/// A code fence's payload is opaque, so a `%%%` written inside one is content.
///
/// §10 I2: a list marker never interrupts, so it opens a container only where a
/// block may begin. Both directions of getting that wrong publish a payload - an
/// invented opaque body hides a real opener, and a missed one invents a span
/// that claims a real delimiter as its closer - so the block-start flag is
/// carried rather than assumed (markup-carve/carve#2505). A CLOSER is read from
/// the line as written either way: a marker line starts a fresh item rather than
/// closing the block already open.
pub(in crate::parse) struct BodyCommentState {
    pub(in crate::parse) code: Option<FenceOpen>,
    pub(in crate::parse) comment: Option<usize>,
    pub(in crate::parse) at_block_start: bool,
    /// An open line block: its payload lines open nothing (PART 2 `line_block`).
    pub(in crate::parse) line_block: Option<usize>,
    /// A `%%%` span open in that line block's payload.
    line_block_span: Option<usize>,
    /// Nonblank lines as (column, is a description term or lead), each shadowing
    /// the earlier ones at or right of its column.
    entries: Vec<(usize, bool)>,
}

impl BodyCommentState {
    /// The width of the open `%%%` span, a line block's included.
    pub(in crate::parse) fn held_span(&self) -> Option<usize> {
        self.comment.or(self.line_block_span)
    }

    pub(in crate::parse) fn new() -> Self {
        Self {
            code: None,
            comment: None,
            at_block_start: true,
            line_block: None,
            line_block_span: None,
            entries: Vec::new(),
        }
    }

    /// A description or note marker line opens a body, so a span on it is the
    /// body's (carve-php#4508, carve#4996): the line past its lead.
    fn after_body_lead<'l>(&self, line: &'l str, col: usize) -> Option<&'l str> {
        if let Some(rest) = line.strip_prefix("[^") {
            let close = rest.find(']')?;
            let after = rest[close + 1..].strip_prefix(':')?;
            let text = after.trim_start_matches(' ');
            return (close > 0 && text.len() < after.len() && !text.trim().is_empty())
                .then_some(text);
        }
        let after = line.strip_prefix(':')?;
        let text = after.trim_start_matches(' ');
        if text.len() == after.len() || text.is_empty() || text.starts_with('\t') {
            return None;
        }
        let above = self
            .entries
            .iter()
            .rev()
            .find(|&&(above, _)| above <= col)?;
        (above.0 == col && above.1).then_some(text)
    }

    pub(in crate::parse) fn observe(&mut self, raw: &str) {
        let line = trim_ascii_start(strip_lazy(raw));
        let lead = if line.is_empty() {
            None
        } else {
            let col = indent_columns(strip_lazy(raw));
            let lead = self.after_body_lead(line, col);
            while self.entries.last().is_some_and(|&(above, _)| above >= col) {
                self.entries.pop();
            }
            self.entries
                .push((col, line.starts_with(": ") || line.starts_with(":: ")));
            lead
        };
        if let Some(open) = self.code {
            if is_fence_close(line, open) {
                self.code = None;
                self.at_block_start = true;
            }
            return;
        }
        if let Some(run) = self.comment {
            if is_comment_fence_close(line, run) {
                self.comment = None;
                self.at_block_start = true;
            }
            return;
        }
        if line.is_empty() {
            self.at_block_start = true;
            return;
        }
        let mut unquoted = line;
        while let Some(rest) = strip_blockquote_prefix(unquoted) {
            unquoted = trim_ascii_start(rest);
        }
        if let Some(run) = self.line_block {
            if let Some(span) = self.line_block_span {
                if is_comment_fence_close(unquoted, span) {
                    self.line_block_span = None;
                }
            } else if exact_colon_fence_len(unquoted) == Some(run) {
                self.line_block = None;
                self.at_block_start = true;
            } else if let Some(open) = detect_comment_fence_line(unquoted) {
                self.line_block_span = Some(open.fence_len);
            }
            return;
        }
        let opener = if let Some(text) = lead {
            trim_ascii_start(innermost_marker_content(text))
        } else if self.at_block_start {
            trim_ascii_start(innermost_marker_content(line))
        } else {
            line
        };
        self.at_block_start = false;
        if let Some(run) = detect_line_block_open(opener).or_else(|| {
            detect_line_block_open(trim_ascii_start(innermost_marker_content(unquoted)))
        }) {
            self.line_block = Some(run);
            return;
        }
        if let Some(open) = detect_fence_open(opener) {
            self.code = Some(open);
            return;
        }
        if let Some(open) = detect_comment_fence_line(opener) {
            self.comment = Some(open.fence_len);
        }
    }
}

pub(in crate::parse) fn body_open_comment_run<S: AsRef<str>>(
    body_lines: impl IntoIterator<Item = S>,
) -> Option<usize> {
    let mut state = BodyCommentState::new();
    for raw in body_lines {
        state.observe(raw.as_ref());
    }
    state.comment
}

/// The width of a `%%%` span the collected lines still hold, a line block's included.
pub(in crate::parse) fn body_held_span_run<S: AsRef<str>>(
    body_lines: impl IntoIterator<Item = S>,
) -> Option<usize> {
    let mut state = BodyCommentState::new();
    for raw in body_lines {
        state.observe(raw.as_ref());
    }
    state.held_span()
}

/// `closes_a_held_comment_span`, counting a line block's span too.
pub(in crate::parse) fn closes_a_held_span<S: AsRef<str>>(body_lines: &[S], line: &str) -> bool {
    body_held_span_run(body_lines).is_some_and(|run| is_comment_fence_close_any_column(line, run))
}

/// Is a comment-shaped line inside a `%%%` span the collected lines still hold,
/// a comment block's or a line block's? Such a line is the span's payload or
/// closer at any column (PART 0 S1, PART 9 §28). Reads only the lines added
/// since the last call, so the body must only grow.
pub(in crate::parse) struct HeldCommentSpan {
    seen: usize,
    state: BodyCommentState,
}

impl HeldCommentSpan {
    pub(in crate::parse) fn new() -> Self {
        Self {
            seen: 0,
            state: BodyCommentState::new(),
        }
    }

    pub(in crate::parse) fn reset_at(&mut self, seen: usize) {
        self.seen = seen;
        self.state = BodyCommentState::new();
    }

    pub(in crate::parse) fn holds<S: AsRef<str>>(&mut self, body_lines: &[S], line: &str) -> bool {
        // Any comment-shaped line is span payload, not only a delimiter (carve#5021).
        if !trim_ascii_start(line).starts_with("%%") {
            return false;
        }
        if body_lines.len() < self.seen {
            *self = Self::new();
        }
        for raw in &body_lines[self.seen..] {
            self.state.observe(raw.as_ref());
        }
        self.seen = body_lines.len();
        self.state.held_span().is_some()
    }
}

/// Is `line` the closer of a comment span the collected lines still hold?
pub(in crate::parse) fn closes_a_held_comment_span<S: AsRef<str>>(
    body_lines: &[S],
    line: &str,
) -> bool {
    body_open_comment_run(body_lines)
        .is_some_and(|run| is_comment_fence_close_any_column(line, run))
}

/// A held span's delimiter as the container keeps it: ONE column of the authored
/// indentation survives, so the container's own parse cannot read a delimiter
/// written below its column back as an authored column-0 one.
pub(in crate::parse) fn kept_comment_delimiter(line: &str) -> String {
    let rest = trim_ascii_start(line);
    if rest.len() == line.len() {
        rest.to_string()
    } else {
        format!(" {rest}")
    }
}

/// The closer counterpart of `detect_comment_fence_line_any_column`.
pub(in crate::parse) fn is_comment_fence_close_any_column(line: &str, fence_len: usize) -> bool {
    is_comment_fence_close(trim_ascii_start(line), fence_len)
}

/// The opener as the definition PRE-PASSES may read it: past leading
/// whitespace, and past a list marker on the fence's own line.
pub(in crate::parse) fn detect_comment_fence_opener_in_container(
    line: &str,
    columns: &ContentColumns,
) -> Option<(CommentFenceOpen, usize)> {
    let (open, col) = detect_comment_fence_opener_at_any_column(line)?;
    if col > 0 && columns.reached_by(col) == 0 {
        return None;
    }
    Some((open, col))
}

/// What bounds the body of a comment fence a definition pre-pass just opened.
///
/// The two containers end for different reasons, so they take different
/// questions. A COLUMN-scoped fence ends where a later line dedents past the
/// column its container holds. A QUOTE-scoped one ends at a blank line: a
/// blockquote does not survive one, so `> a` / blank / `> b` is two quotes, and
/// a run after the blank wears the same marker while belonging to a different
/// quote. For a quoted scope the blank line is therefore the whole test, where a
/// column scope keeps the dedent test - there a dedented line really can be a
/// lazy continuation.
#[derive(Clone, Copy)]
pub(in crate::parse) enum CommentFenceScope {
    /// The content column the `%` run reaches. Zero is the document level.
    Column(usize),
    /// The number of blockquote markers the `%` run was written behind, and
    /// the column inside the quote it reaches (past a marker on its line).
    Quoted { depth: usize, floor: usize },
}

impl CommentFenceScope {
    pub(in crate::parse) fn floor(self) -> usize {
        match self {
            CommentFenceScope::Column(floor) | CommentFenceScope::Quoted { floor, .. } => floor,
        }
    }

    pub(in crate::parse) fn quote_depth(self) -> usize {
        match self {
            CommentFenceScope::Quoted { depth, .. } => depth,
            CommentFenceScope::Column(_) => 0,
        }
    }
}

/// The fence a definition pre-pass opens, together with the scope bounding it.
///
/// `detect_comment_fence_opener_in_container` walks past a list marker but not
/// past a blockquote marker, so `> %%%` was not an opener to either pre-pass at
/// all - they walked into its body and registered from it. §28 makes a comment
/// fence's body verbatim wherever the fence sits and §24 C3 states the rule for
/// both delimiter spellings; neither is scoped by container, so nothing
/// distinguishes a fence reached through a `>` prefix from one reached through
/// indentation, and the block parser consumed the quoted one all along. That
/// left `> %%%` / `> [r]: /url` / `> %%%` with `r` live in the link table while
/// the comment rendered nothing (markup-carve/carve#1341).
pub(in crate::parse) fn detect_comment_fence_opener_scoped(
    line: &str,
    columns: &ContentColumns,
) -> Option<(CommentFenceOpen, CommentFenceScope)> {
    let (depth, _, quoted) = prepass_quote_scope(line);
    if depth > 0 {
        // Inside the quote the delimiter is read at any column, the same way the
        // column-scoped opener above reads it - the quote is the container, so
        // its own indentation does not have to reach anything further.
        let (open, floor) = detect_comment_fence_opener_at_any_column(quoted)?;
        return Some((open, CommentFenceScope::Quoted { depth, floor }));
    }
    let (open, col) = detect_comment_fence_opener_in_container(line, columns)?;
    Some((open, CommentFenceScope::Column(columns.reached_by(col))))
}

/// A comment fence a definition pre-pass has entered: its width and the maximum
/// quote depth a closer can carry.
#[derive(Clone, Copy)]
pub(in crate::parse) struct OpenCommentFence {
    pub(in crate::parse) fence_len: usize,
    pub(in crate::parse) quote_depth: usize,
    /// The content column the opener reached; zero at the document level.
    pub(in crate::parse) floor: usize,
}

/// S1 (carve#4907): a span with no closer ends at a nonblank line that misses
/// its owner. A `%%` line is payload and misses nothing.
pub(in crate::parse) fn misses_comment_owner(line: &str, open: OpenCommentFence) -> bool {
    let rest = if open.quote_depth > 0 {
        let (depth, _, rest) = prepass_quote_scope(line);
        if is_blank_line(line) || depth < open.quote_depth {
            return true;
        }
        rest
    } else {
        line
    };
    open.floor > 0
        && !is_blank_line(rest)
        && !trim_ascii_start(rest).starts_with("%%")
        && indent_columns(rest) < open.floor
}

/// A matched comment closer is read before quote ownership. It may carry any
/// prefix up to the opener's depth; a deeper quoted run remains payload.
pub(in crate::parse) fn closes_open_comment_fence(line: &str, open: OpenCommentFence) -> bool {
    let (depth, _, rest) = prepass_quote_scope(line);
    depth <= open.quote_depth && is_comment_fence_close_any_column(rest, open.fence_len)
}

pub(in crate::parse) fn detect_comment_fence_opener_at_any_column(
    line: &str,
) -> Option<(CommentFenceOpen, usize)> {
    let mut rest = line;
    let mut col = 0usize;
    loop {
        let trimmed = trim_ascii_start(rest);
        col = advance_columns(&rest[..rest.len() - trimmed.len()], col);
        rest = trimmed;
        let Some(marker) = detect_list_marker_full(rest) else {
            break;
        };
        let consumed = marker.content.as_ptr() as usize - rest.as_ptr() as usize;
        if consumed == 0 {
            break;
        }
        col = advance_columns(&rest[..consumed], col);
        rest = marker.content;
    }
    detect_comment_fence_line(rest).map(|open| (open, col))
}

/// `indent_columns`, continued from a column already reached. Tab stops are
/// measured from the start of the line, so a marker's own width has to be walked
/// rather than counted in bytes.
pub(in crate::parse) fn advance_columns(prefix: &str, mut col: usize) -> usize {
    for byte in prefix.bytes() {
        if byte == b'\t' {
            col += 4 - (col % 4);
        } else {
            col += 1;
        }
    }
    col
}

// Counts the lines the container-comment dedent walk visits.
//
// The whole point of the index below is that this stays proportional to the
// DOCUMENT rather than to openers times container length, and a count is the
// only way to state that without a clock - see the note on `quote_prefix_calls`
// for why this repo counts work instead of timing it. Test-only, so a release
// build carries nothing.
#[cfg(test)]
thread_local! {
    pub(crate) static CONTAINER_DEDENT_STEPS: std::cell::Cell<u64> =
        const { std::cell::Cell::new(0) };
}

/// Find a comment closer within its host's extent.
///
/// An ordinary dedented line ends the container. A matching comment closer at
/// that boundary still belongs to the comment, so the closer may equal the
/// first dedented line (corpus 512). A closer after an earlier dedent belongs
/// outside the container, and the opener degrades to a line comment.
///
/// List markers inside the comment body do not end it (carve-rs#1053).
///
/// Closer positions are indexed by fence width and authored quote depth. The first
/// dedent is memoized per column so repeated openers do not rescan the same
/// container body.
#[derive(Default)]
pub(in crate::parse) struct ContainerCommentClosers {
    /// Line index of every comment-fence closer, keyed by its exact `%` run AND
    /// the quote depth it was written at, ascending within each key.
    by_width: HashMap<(usize, usize), Vec<usize>>,
    /// column -> (query point the answer was computed from, the answer).
    dedents: HashMap<usize, (usize, usize)>,
    note_dedents: HashMap<(usize, usize), (usize, usize)>,
}

impl ContainerCommentClosers {
    pub(in crate::parse) fn build(lines: &[&str]) -> Self {
        let mut by_width: HashMap<(usize, usize), Vec<usize>> = HashMap::new();
        for (index, line) in lines.iter().enumerate() {
            // The same reading `comment_fence_close_index` uses, taken once per
            // quote DEPTH: leading whitespace is not part of the delimiter, a
            // tail is ignored, and nothing but the blockquote markers comes off.
            //
            // Depth is half the key because stripping the markers collapses
            // `> > %%%` and `> %%%` onto one run. A matched span's closer may
            // be shallower than its opener (S1), but a deeper run is payload.
            //
            // Depth 0 is exactly the set this index held before depth was
            // tracked, so every column-scoped query answers as it did.
            let (depth, _, rest) = prepass_quote_scope(line);
            let run = comment_fence_width(trim_ascii_end(trim_ascii_start(rest)));
            if run >= 3 {
                by_width.entry((run, depth)).or_default().push(index);
            }
        }
        Self {
            by_width,
            dedents: HashMap::new(),
            note_dedents: HashMap::new(),
        }
    }

    pub(in crate::parse) fn closes_in_cursor(
        &mut self,
        lines: CursorLines<'_>,
        from: usize,
        floor: usize,
        width: usize,
    ) -> bool {
        let Some(closer) = self.next_closer(from, width, 0) else {
            return false;
        };
        let boundary = match self.dedents.get(&floor).copied() {
            Some((start, end)) if start <= from && from < end => end,
            _ => {
                let end = (from + 1..lines.len())
                    .find(|&at| {
                        #[cfg(test)]
                        CONTAINER_DEDENT_STEPS.with(|count| count.set(count.get() + 1));
                        let line = lines.at(at);
                        !is_blank_line(line)
                            && !is_line_comment_any_column(line)
                            && indent_columns(line) < floor
                    })
                    .unwrap_or(lines.len());
                self.dedents.insert(floor, (from, end));
                end
            }
        };
        closer <= boundary
    }

    pub(in crate::parse) fn closes_in_note(
        &mut self,
        lines: &[&str],
        open_at: usize,
        depth: usize,
        floor: usize,
        fence_len: usize,
    ) -> bool {
        let Some(closer) = self.next_closer(open_at, fence_len, depth) else {
            return false;
        };
        let key = (depth, floor);
        let boundary = match self.note_dedents.get(&key) {
            Some(&(from, at)) if from <= open_at && open_at < at => at,
            _ => {
                let mut boundary = lines.len();
                for (offset, raw) in lines[open_at + 1..].iter().enumerate() {
                    #[cfg(test)]
                    CONTAINER_DEDENT_STEPS.with(|c| c.set(c.get() + 1));
                    if strip_quote_levels(raw, depth).map_or(true, |line| {
                        !is_blank_line(line) && indent_columns(line) < floor
                    }) {
                        boundary = open_at + 1 + offset;
                        break;
                    }
                }
                self.note_dedents.insert(key, (open_at, boundary));
                boundary
            }
        };
        closer < boundary
    }

    pub(in crate::parse) fn next_closer(
        &self,
        open_at: usize,
        fence_len: usize,
        depth: usize,
    ) -> Option<usize> {
        let positions = self.by_width.get(&(fence_len, depth))?;
        let next = positions.partition_point(|at| *at <= open_at);
        positions.get(next).copied()
    }
}

#[cfg(test)]
thread_local! {
    pub(in crate::parse) static COMMENT_FENCE_BYTES: Cell<u64> = const { Cell::new(0) };
}

pub(in crate::parse) fn comment_fence_width(line: &str) -> usize {
    let width = line.bytes().take_while(|b| *b == b'%').count();
    #[cfg(test)]
    COMMENT_FENCE_BYTES.with(|count| {
        // Include the first non-percent byte that stops the scan, if present.
        count.set(count.get() + (width + usize::from(width < line.len())) as u64);
    });
    width
}

pub(in crate::parse) fn detect_comment_fence_line(line: &str) -> Option<CommentFenceOpen> {
    let line = trim_ascii_end(line);
    let fence_len = comment_fence_width(line);
    if fence_len < 3 {
        return None;
    }
    Some(CommentFenceOpen {
        fence_len,
        tail: trim_ascii_start(&line[fence_len..]).to_string(),
    })
}

/// A comment-fence closer matches by exact leading `%` run length; its tail is
/// ignored and discarded.
pub(in crate::parse) fn is_comment_fence_close(line: &str, fence_len: usize) -> bool {
    let line = trim_ascii_end(line);
    comment_fence_width(line) == fence_len
}

pub(in crate::parse) fn detect_thematic_break(line: &str) -> bool {
    thematic_break_marker(line).is_some()
}

pub(in crate::parse) fn thematic_break_marker(line: &str) -> Option<char> {
    // Grammar thematic_break: a col-0 run of 3+ of the SAME `-`/`*`/`_`,
    // followed only by trailing whitespace. A `-` or `*` run may instead be
    // spaced by single spaces (`* * *`, carve#4814); a wider gap, a leading
    // indent or a mixed run is not a break.
    let bytes = line.as_bytes();
    let marker = match bytes.first() {
        Some(&b @ (b'-' | b'*' | b'_')) => b,
        _ => return None,
    };
    let mut count = 0usize;
    let mut i = 0;
    while i < bytes.len() && bytes[i] == marker {
        count += 1;
        i += 1;
    }
    if count == 1 && marker != b'_' {
        while i + 1 < bytes.len() && bytes[i] == b' ' && bytes[i + 1] == marker {
            count += 1;
            i += 2;
        }
    }
    if count < 3 {
        return None;
    }
    // Only trailing whitespace may follow the contiguous marker run.
    bytes[i..]
        .iter()
        .all(|&b| b == b' ' || b == b'\t')
        .then_some(marker as char)
}

#[derive(Debug, Clone, Copy)]
pub(in crate::parse) struct FenceOpen {
    pub(in crate::parse) fence_char: u8,
    pub(in crate::parse) fence_len: usize,
    pub(in crate::parse) content_col: usize,
    pub(in crate::parse) quoted: bool,
    pub(in crate::parse) lang_start: usize,
    pub(in crate::parse) lang_end: usize,
    pub(in crate::parse) title_start: Option<usize>,
    pub(in crate::parse) title_end: Option<usize>,
    pub(in crate::parse) label_start: Option<usize>,
    pub(in crate::parse) label_end: Option<usize>,
}

pub(in crate::parse) fn detect_fence_open(line: &str) -> Option<FenceOpen> {
    valid_fence_open(line).or_else(|| recovered_fence_open(line))
}

/// PART 9 §2 RECOVERY (carve#4807): a language token that ends where metadata
/// would start keeps the code block and drops the rest, unless the rest of the
/// line holds a run of the opener's character as long as the opener.
fn recovered_fence_open(line: &str) -> Option<FenceOpen> {
    let line = trim_ascii_end(line);
    let bytes = line.as_bytes();
    let fence_char = *bytes.first().filter(|b| matches!(b, b'`' | b'~'))?;
    let fence_len = bytes.iter().take_while(|&&b| b == fence_char).count();
    if fence_len < 3 {
        return None;
    }
    let lang_start = fence_len + usize::from(bytes.get(fence_len) == Some(&b' '));
    let lang_end = lang_start
        + bytes[lang_start..]
            .iter()
            .take_while(|&&b| is_fence_language_byte(b))
            .count();
    if lang_end == lang_start {
        return None;
    }
    let rest = &line[lang_end..];
    let ends_token = rest.chars().next().is_some_and(|c| {
        c.is_whitespace() || matches!(c, '\u{feff}' | '"' | '{' | '[' | '\u{201c}' | '\u{201d}')
    });
    if !ends_token || line[fence_len..].contains(&line[..fence_len]) {
        return None;
    }
    Some(FenceOpen {
        fence_char,
        fence_len,
        content_col: 0,
        quoted: false,
        lang_start,
        lang_end,
        title_start: None,
        title_end: None,
        label_start: None,
        label_end: None,
    })
}

fn is_fence_language_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'+' | b'#' | b'.' | b'/')
}

/// Whether a fence opener only opens through the §2 recovery, with the format
/// word a legacy `raw FORMAT` opener names.
pub(in crate::parse) fn recovered_fence_format(line: &str) -> Option<Option<&str>> {
    if valid_fence_open(line).is_some() {
        return None;
    }
    let open = recovered_fence_open(line)?;
    if &line[open.lang_start..open.lang_end] != "raw" {
        return Some(None);
    }
    let rest = line[open.lang_end..].trim_start();
    let word = rest
        .find(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '_' | '-')))
        .map_or(rest, |end| &rest[..end]);
    let names_format = word.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_');
    Some(names_format.then_some(word))
}

fn valid_fence_open(line: &str) -> Option<FenceOpen> {
    let line = trim_ascii_end(line);
    let bytes = line.as_bytes();
    let mut i = 0;
    if bytes.is_empty() {
        return None;
    }
    let fence_char = bytes[i];
    if fence_char != b'`' && fence_char != b'~' {
        return None;
    }
    let fence_start = i;
    while i < bytes.len() && bytes[i] == fence_char {
        i += 1;
    }
    let fence_len = i - fence_start;
    if fence_len < 3 {
        return None;
    }
    // Optional whitespace then info string:
    //   [language] ["header"] [[label]]
    // in that fixed order. With no language, a header or label may sit
    // directly against the fence; after a language each following token must
    // be whitespace-separated.
    //
    // THIS SLOT IS EXACTLY ONE SPACE. `fenced_code_block = code_fence_open,
    // [space], [code_fence_info]` spells it as one, and carve#912 ruled the
    // production right (carve#912). A two-space opener therefore matches no
    // shape and the INVALID-FENCE FALLBACK applies: an inline verbatim span in
    // a paragraph.
    //
    // The two metadata slots INSIDE `code_fence_info` are spelled `space+` and
    // are NOT in scope - a run stays legal at both, and the `separated` loops
    // below are deliberately left alone. The cardinality answer is
    // per-production, not global (carve#892 keeps the colon fence's separator a
    // run for the same reason).
    //
    // A run with no info after it is the line ending rather than this slot: the
    // remaining spaces are eaten by the `separated` loop and the fence stays
    // valid, which is what keeps ```` ```<SP><SP> ```` an ordinary empty fence.
    if bytes.get(i) == Some(&b' ') {
        i += 1;
    }
    let lang_start = i;
    // Raw passthrough block: `=FORMAT` (§4.15, djot raw-block syntax) -- a
    // leading `=` immediately followed by the format name. The `=` is the block
    // parallel of the inline raw `{=format}` attribute; it is never part of a
    // language token, so this is unambiguous against an ordinary code block.
    // parse_fence recovers raw blocks by the leading `=` in this span. The `=`
    // and format name must be adjacent (`=html`); `= html` is not raw.
    if i < bytes.len() && bytes[i] == b'=' {
        i += 1;
        if i >= bytes.len() || !bytes[i].is_ascii_alphabetic() {
            return None;
        }
        i += 1;
        while i < bytes.len()
            && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' || bytes[i] == b'-')
        {
            i += 1;
        }
        let lang_end = i;
        // Must be only whitespace after the format name
        while i < bytes.len() && bytes[i] == b' ' {
            i += 1;
        }
        if i != bytes.len() {
            return None;
        }
        return Some(FenceOpen {
            fence_char,
            fence_len,
            content_col: 0,
            quoted: false,
            lang_start,
            lang_end,
            title_start: None,
            title_end: None,
            label_start: None,
            label_end: None,
        });
    }
    // Language token charset covers real-world tags with punctuation
    // (c++, c#, f#, asp.net, text/html); the token is still anchored (no
    // whitespace), so a multiword/quoted info string is not a fence. `/` is
    // allowed so MIME-like tags stay a single language token.
    while i < bytes.len() && is_fence_language_byte(bytes[i]) {
        i += 1;
    }
    let lang_end = i;
    let has_lang = lang_start < lang_end;
    let mut title_start = None;
    let mut title_end = None;
    let mut label_start = None;
    let mut label_end = None;
    let mut separated = false;
    while i < bytes.len() && bytes[i] == b' ' {
        separated = true;
        i += 1;
    }
    if i < bytes.len() && bytes[i] == b'"' {
        if has_lang && !separated {
            return None;
        }
        i += 1;
        let start = i;
        while i < bytes.len() {
            if bytes[i] == b'"' {
                title_start = Some(start);
                title_end = Some(i);
                i += 1;
                break;
            }
            i += 1;
        }
        title_start?;
        separated = false;
        while i < bytes.len() && bytes[i] == b' ' {
            separated = true;
            i += 1;
        }
    }
    if i < bytes.len() && bytes[i] == b'[' {
        if (has_lang || title_start.is_some()) && !separated {
            return None;
        }
        // The fence info line's label is the colon opener's `label` production,
        // so it closes where `parse_bare_label` closes one: balanced, escape-aware
        // and opaque inside a verbatim span. Written flat here, the two spellings
        // of the one production disagreed, and `[a `]` b]` lost its label on a
        // fence while keeping it on a div (markup-carve/carve#2576).
        let (_, end) = read_bracketed(bytes, i)?;
        label_start = Some(i + 1);
        label_end = Some(end - 1);
        i = end;
    }
    // Must be only whitespace after the info string
    while i < bytes.len() && bytes[i] == b' ' {
        i += 1;
    }
    if i != bytes.len() {
        return None;
    }
    Some(FenceOpen {
        fence_char,
        fence_len,
        content_col: 0,
        quoted: false,
        lang_start,
        lang_end,
        title_start,
        title_end,
        label_start,
        label_end,
    })
}

pub(in crate::parse) fn parse_fence(
    cur: &mut LineCursor,
    open: FenceOpen,
    options: &Options<'_>,
) -> BlockNode {
    let span_start = cur.pos;
    let mut closed = false;
    let open_line = cur.consume().unwrap();
    let open_trim = open_line[open.lang_start..].trim();
    let raw_format = open_trim.strip_prefix('=').map(|f| f.trim().to_string());
    let lang = if raw_format.is_none() && open.lang_start < open.lang_end {
        Some(open_line[open.lang_start..open.lang_end].to_string())
    } else {
        None
    };
    let title = open
        .title_start
        .zip(open.title_end)
        .map(|(start, end)| open_line[start..end].to_string());
    let label = open
        .label_start
        .zip(open.label_end)
        .map(|(start, end)| open_line[start..end].to_string());
    let mut content_lines: Vec<String> = Vec::new();
    let mut last_payload_line = None;
    while let Some(line) = cur.peek() {
        if is_fence_close(line, open)
            || (cur.at_document_level
                && line.starts_with(LAZY)
                && is_fence_close(strip_lazy(line), open))
        {
            cur.consume();
            closed = true;
            break;
        }
        last_payload_line = cur.source_line(cur.pos);
        cur.consume();
        content_lines.push(line.to_string());
    }
    // Container collection can synthesize a final separator. The original
    // line map distinguishes that separator from an authored payload break.
    let unterminated = UNTERMINATED_SOURCE_LINE.with(|slot| {
        slot.get()
            .is_some_and(|last| last_payload_line == Some(last))
    });
    // The span covers the opener, the body and the closer - the whole block as
    // the author wrote it, not just its content. A blank final payload has no
    // codepoint except its line ending, which belongs to an open fence.
    let mut pos = span_of(cur, span_start, cur.pos, options);
    let lint_pos = pos.clone();
    if !closed
        && !unterminated
        && content_lines
            .last()
            .is_some_and(|line| is_blank_line(strip_lazy(line)))
    {
        if let Some(pos) = pos.as_mut() {
            if LAST_NONBLANK_SOURCE_LINE
                .with(|slot| slot.get().is_some_and(|last| pos.end_line < last))
            {
                pos.end_line += 1;
                pos.end_column = 1;
            }
        }
    }
    if !closed {
        if let Some(pos) = lint_pos {
            let marker = std::iter::repeat(open.fence_char as char)
                .take(open.fence_len)
                .collect();
            let end_line = cur.source_line(cur.pos).unwrap_or(pos.end_line + 1);
            note_lint_event(LintEvent::Fence {
                pos,
                marker,
                end_line,
            });
        }
    }
    let mut content = content_lines
        .iter()
        .map(|l| strip_lazy(l))
        .collect::<Vec<_>>()
        .join("\n");
    if !content_lines.is_empty() && !unterminated {
        content.push('\n');
    }
    if let Some(format) = raw_format {
        BlockNode::RawBlock(RawBlock {
            attrs: None,
            format,
            content,
            pos,
        })
    } else {
        BlockNode::CodeBlock(CodeBlock {
            attrs: None,
            lang,
            title,
            label,
            content,
            pos,
        })
    }
}

/// A CLOSER TAKES NO CONTENT, so its tail is the LINE ENDING and not a slot.
///
/// PART 2's NO TRAILING WHITESPACE governs it: the run is `whitespace`,
/// `' ' | '\t'`, it is dropped, and it is not content. That is the whole reason
/// a tab is accepted here while the OPENER refuses one - the opener's tab sits
/// before an info string, where it is a separator and MARKER SEPARATORS AND
/// PADDING SLOTS spells the terminal `space` alone. Position decides, not the
/// construct (carve#1295), so the two halves of the fence disagree on purpose.
///
/// A tab with content after it is still not a closer: the loop stops at the
/// first non-whitespace byte and the line then fails the end-of-line test, so
/// ```` ```<TAB>php ```` closes nothing.
pub(in crate::parse) fn is_fence_close(line: &str, open: FenceOpen) -> bool {
    let bytes = line.as_bytes();
    let mut i = 0;
    let start = i;
    while i < bytes.len() && bytes[i] == open.fence_char {
        i += 1;
    }
    if i - start < open.fence_len {
        return false;
    }
    while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
        i += 1;
    }
    i == bytes.len()
}

pub(in crate::parse) fn exact_colon_fence_len(line: &str) -> Option<usize> {
    let t = trim_ascii_end(line);
    if !t.is_empty() && t.bytes().all(|b| b == b':') {
        Some(t.len())
    } else {
        None
    }
}

/// The prefixes carrying an open colon-body paragraph, outermost first: bit
/// `i` of `quotes` is set for a quote (`>`) and clear for an item (`-`), whose
/// content column is `columns[i]`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(in crate::parse) struct OpenParagraph {
    depth: u8,
    quotes: u32,
    columns: [u16; 32],
}

impl OpenParagraph {
    const PLAIN: Self = Self {
        depth: 0,
        quotes: 0,
        columns: [0; 32],
    };

    fn outer_is_quote(self) -> Option<bool> {
        (self.depth > 0).then_some(self.quotes & 1 == 1)
    }

    fn outer_item_column(self) -> Option<usize> {
        (self.outer_is_quote() == Some(false)).then_some(usize::from(self.columns[0]))
    }

    fn inner(self) -> Self {
        let mut columns = [0; 32];
        columns[..31].copy_from_slice(&self.columns[1..]);
        Self {
            depth: self.depth - 1,
            quotes: self.quotes >> 1,
            columns,
        }
    }

    fn under(self, quote: bool, column: usize) -> Self {
        let mut columns = [0; 32];
        columns[1..].copy_from_slice(&self.columns[..31]);
        columns[0] = u16::try_from(column).unwrap_or(u16::MAX);
        Self {
            depth: (self.depth + 1).min(32),
            quotes: (self.quotes << 1) | u32::from(quote),
            columns,
        }
    }
}

/// Whether a paragraph is open after this colon-body line, for the §10 I4
/// gate on an unclosed fence. Blank, colon, comment, block-attribute, table-row,
/// heading and thematic-break lines leave none open. A quote line always reads its content;
/// an item line does only under an item's paragraph, where it starts the next
/// item, and is lazy text under any other. A line reaching the open item's
/// content column is read there (`in_item`), so a block at that column closes
/// the item's paragraph (CARVE-P0-011).
pub(in crate::parse) fn opens_colon_body_paragraph(
    line: &str,
    was_open: Option<OpenParagraph>,
    in_item: bool,
) -> Option<OpenParagraph> {
    let t = trim_ascii_end(trim_ascii_start(line));
    let block = if in_item {
        trim_ascii_start(line)
    } else {
        line
    };
    if t.is_empty()
        || t.starts_with("%%")
        || (block.starts_with('{') && parse_standalone_attrs(block).is_some())
        || (block.starts_with('|') && is_table_start(block))
        || exact_colon_fence_len(t).is_some()
        || detect_container_open(t).is_some()
    {
        return None;
    }
    let hashes = t.bytes().take_while(|b| *b == b'#').count();
    if (1..=6).contains(&hashes) && matches!(t.as_bytes().get(hashes), None | Some(b' ' | b'\t')) {
        return None;
    }
    let first = t.as_bytes()[0];
    if matches!(first, b'-' | b'*' | b'_') && t.len() >= 3 && t.bytes().all(|b| b == first) {
        return None;
    }
    if let Some(rest) = t.strip_prefix('>') {
        match rest.as_bytes().first() {
            None => return None,
            Some(b' ' | b'\t') => {
                let quoted = was_open
                    .filter(|open| open.outer_is_quote() == Some(true))
                    .map(OpenParagraph::inner);
                return opens_colon_body_paragraph(&rest[1..], quoted, false)
                    .map(|open| open.under(true, 0));
            }
            Some(_) => {}
        }
    }
    if let Some(open) = was_open {
        if let Some(column) = open.outer_item_column() {
            if line.bytes().take_while(|b| *b == b' ').count() >= column {
                return opens_colon_body_paragraph(&line[column..], Some(open.inner()), true)
                    .map(|inner| inner.under(false, column));
            }
        }
    }
    let item = if t.starts_with('>') {
        None
    } else {
        detect_list_marker_full(t)
    };
    if let Some(open) = was_open {
        if item.is_none() || (open.outer_is_quote() != Some(false) && !in_item) {
            return Some(open);
        }
    }
    match item {
        None => Some(OpenParagraph::PLAIN),
        Some(item) => {
            let column = marker_content_col(line).unwrap_or(0);
            opens_colon_body_paragraph(item.content, None, false)
                .map(|open| open.under(false, column))
        }
    }
}

#[cfg(test)]
thread_local! {
    pub(in crate::parse) static COLON_COPIED_BYTES: Cell<usize> = const { Cell::new(0) };
    pub(in crate::parse) static COLON_CACHE_ENABLED: Cell<bool> = const { Cell::new(true) };
    pub(in crate::parse) static COLON_REFERENCE_ENTRIES: Cell<usize> = const { Cell::new(0) };
    pub(in crate::parse) static COLON_CLOSER_LINES: Cell<usize> = const { Cell::new(0) };
    pub(in crate::parse) static COMMENT_INDEX_LINES: Cell<usize> = const { Cell::new(0) };
    pub(in crate::parse) static CODE_INDEX_LINES: Cell<usize> = const { Cell::new(0) };
    pub(in crate::parse) static LITERAL_CODE_LINES: Cell<usize> = const { Cell::new(0) };
}

pub(in crate::parse) fn push_current_line(inner: &mut LineBuffer, cur: &LineCursor<'_>) {
    #[cfg(test)]
    COLON_COPIED_BYTES.with(|count| count.set(count.get() + cur.peek().unwrap().len()));
    inner.push_at(
        cur.peek().unwrap().to_string(),
        cur.source_line(cur.pos),
        cur.source_col(cur.pos),
    );
}

/// Copy the opener line, then every line up to and including its closer.
///
/// The opener is taken BEFORE the first closer test on purpose. An opener with
/// no info string is closer-shaped itself - a bare ``` or a `%%%` - so testing
/// it would end the span on its own line and hand the span's contents back to
/// the block parser, where a fence-shaped line inside it closes the container
/// around it (carve#450).
pub(in crate::parse) fn take_opaque_span_into(
    inner: &mut LineBuffer,
    cur: &mut LineCursor<'_>,
    is_close: impl Fn(&str) -> bool,
) {
    push_current_line(inner, cur);
    cur.consume();
    while let Some(line) = cur.peek() {
        push_current_line(inner, cur);
        cur.consume();
        if is_close(line) {
            break;
        }
    }
}

pub(in crate::parse) fn skip_opaque_span_into(
    inner: &mut LineBuffer,
    cur: &mut LineCursor<'_>,
) -> bool {
    let Some(line) = cur.peek() else {
        return false;
    };
    if let Some(open) = detect_fence_open(line) {
        take_opaque_span_into(inner, cur, |candidate| is_fence_close(candidate, open));
        return true;
    }
    if let Some(open) = detect_comment_fence_line(line) {
        if cur.has_comment_closer_after(cur.pos + 1, open.fence_len) {
            let fence_len = open.fence_len;
            take_opaque_span_into(inner, cur, move |candidate| {
                is_comment_fence_close_any_column(candidate, fence_len)
            });
            return true;
        }
    }
    false
}

pub(in crate::parse) fn reuse_colon_views() -> bool {
    #[cfg(test)]
    {
        COLON_CACHE_ENABLED.with(Cell::get)
    }
    #[cfg(not(test))]
    {
        true
    }
}

/// Returns the collected body and whether the container's own CLOSER was
/// consumed - `false` means end of input closed it (PART 9 §12). The flag
/// exists for the figure group, whose caption slot hangs on the closing fence
/// (§4c): a group closed by end of input has no closer line for a caption.
pub(in crate::parse) fn collect_shared_colon_body(
    cur: &mut LineCursor<'_>,
    opener_len: usize,
) -> (ColonView, bool) {
    let marker_hosted = cur.colon_view.map_or(
        cur.in_item_body || cur.sublists_carry_authored_base,
        |view| view.marker_hosted,
    );
    #[cfg(test)]
    let cached = COLON_CACHE_ENABLED.with(Cell::get);
    #[cfg(not(test))]
    let cached = true;
    if let Some(view) = cur.colon_view.filter(|_| cached) {
        let opener = view.start + cur.pos - 1;
        if let Some(&(end, closed)) = view.source.boundaries.get(opener).and_then(Option::as_ref) {
            if end <= view.end && (!closed || end < view.end) {
                let start = view.start + cur.pos;
                cur.pos = end - view.start + usize::from(closed);
                return (
                    ColonView {
                        source: view.source.clone(),
                        start,
                        end,
                        marker_hosted,
                    },
                    closed,
                );
            }
        }
    }
    let (inner, closed, boundaries) = collect_indexed_colon_body(cur, opener_len);
    let end = inner.lines.len();
    (
        ColonView {
            source: std::rc::Rc::new(ColonSource {
                inner,
                boundaries,
                fence_closers: std::cell::OnceCell::new(),
            }),
            start: 0,
            end,
            marker_hosted,
        },
        closed,
    )
}

pub(in crate::parse) fn collect_indexed_colon_body(
    cur: &mut LineCursor<'_>,
    opener_len: usize,
) -> (LineBuffer, bool, Vec<Option<(usize, bool)>>) {
    let mut inner = LineBuffer::default();
    let mut closed = false;
    let mut stack = vec![opener_len];
    let mut origins = vec![None];
    let mut boundaries = Vec::new();
    let mut cacheable = true;
    let mut paragraph_open = None;
    // A span opened past column 0 (`  %%%`): its closer may sit at any
    // column, so a column-0 closer is payload, not a new span (PART 0 S1).
    let mut indented_span: Option<(usize, usize)> = None;
    let mut indented_code: Option<FenceOpen> = None;
    while cur.peek().is_some() {
        let top = *stack.last().unwrap();
        let paragraph_was_open = paragraph_open;
        paragraph_open = opens_colon_body_paragraph(cur.peek().unwrap(), paragraph_was_open, false);
        // A CLOSER OF AN OPEN CONTAINER IS NOT ABSORBABLE. §12's absorption is
        // about a paragraph swallowing a would-be OPENER; the container this
        // line closes was opened BEFORE the malformed fence was ever read, so
        // no absorption can take its closer away (carve-rs#719).
        if exact_colon_fence_len(cur.peek().unwrap()) == Some(top) {
            if stack.len() == 1 {
                cur.consume();
                closed = true;
                break;
            }
            let end = inner.lines.len();
            if let Some(origin) = origins.pop().flatten() {
                boundaries[origin] = Some((end, true));
            }
            push_current_line(&mut inner, cur);
            cur.consume();
            stack.pop();
            continue;
        }
        let line = cur.peek().unwrap();
        if let Some(open) = indented_code {
            if is_fence_close(trim_ascii_start(line), open) {
                indented_code = None;
            }
        } else if let Some((run, column)) = indented_span {
            if is_comment_fence_close_any_column(line, run) {
                indented_span = None;
                push_current_line(&mut inner, cur);
                cur.consume();
                paragraph_open = None;
                continue;
            }
            if !is_blank_line(line)
                && indent_columns(line) < column
                && !trim_ascii_start(line).starts_with("%%")
            {
                indented_span = None;
            }
        } else if line.starts_with([' ', '\t']) {
            let rest = trim_ascii_start(line);
            if let Some(open) = detect_fence_open(rest) {
                indented_code = Some(open);
            } else if let Some(open) = detect_comment_fence_line(rest) {
                indented_span = Some((open.fence_len, indent_columns(line)));
                push_current_line(&mut inner, cur);
                cur.consume();
                paragraph_open = None;
                continue;
            }
        }
        if indented_span.is_none() && skip_opaque_span_into(&mut inner, cur) {
            paragraph_open = None;
            continue;
        }
        let line = cur.peek().unwrap();
        // An unclosed fence is code up to the innermost container's own closer;
        // every other colon line before it is content (A CLOSER IS REQUIRED,
        // carve#3201). Under an open paragraph it is text (§10 I4).
        if paragraph_was_open.is_none() && detect_fence_open(line).is_some() {
            push_current_line(&mut inner, cur);
            cur.consume();
            while let Some(next) = cur.peek() {
                if exact_colon_fence_len(next) == Some(top) {
                    break;
                }
                push_current_line(&mut inner, cur);
                cur.consume();
            }
            paragraph_open = None;
            continue;
        }
        if !line.starts_with([' ', '\t']) {
            if let Some(len) = detect_container_open(line)
                .map(|open| open.fence_len)
                .or_else(|| detect_line_block_open(line))
                .or_else(|| detect_hardbreaks_block_open(line))
                .or_else(|| detect_quote_block_open(line))
            {
                // Past the cap an opener is paragraph text (§25), but it still
                // pairs with its closer, as in the spec model's closer walk:
                // capping this stack let a walk started deeper pair the same
                // closers differently (carve-rs#530, carve-rs#3891).
                if stack.len() >= MAX_NESTING_DEPTH {
                    cacheable = false;
                }
                let origin = inner.lines.len();
                boundaries.resize(origin + 1, None);
                origins.push(Some(origin));
                stack.push(len);
                push_current_line(&mut inner, cur);
                cur.consume();
                continue;
            }
        }
        // No state survives a line here. This walk decides where the container
        // ENDS, and nothing a body line does can change that: the only line
        // that ends it is its own closer, and a closer is not absorbable. The
        // glued-colon tracking that used to sit here suppressed exactly that
        // closer, which is the defect (carve-rs#719).
        push_current_line(&mut inner, cur);
        cur.consume();
    }
    let end = inner.lines.len();
    for origin in origins.into_iter().flatten() {
        boundaries[origin] = Some((end, false));
    }
    if !cacheable {
        boundaries.clear();
    }
    (inner, closed, boundaries)
}

pub(in crate::parse) fn find_line_block_end(
    lines: CursorLines<'_>,
    start: usize,
    fence_len: usize,
) -> usize {
    find_colon_fence_end(lines, start, fence_len, true, &mut None, &mut None)
}

pub(in crate::parse) fn find_colon_container_end(
    lines: CursorLines<'_>,
    start: usize,
    fence_len: usize,
) -> usize {
    find_colon_fence_end(lines, start, fence_len, false, &mut None, &mut None)
}

pub(in crate::parse) fn find_colon_fence_end(
    lines: CursorLines<'_>,
    start: usize,
    fence_len: usize,
    literal: bool,
    closer_index: &mut Option<HashMap<u8, Vec<usize>>>,
    comment_closers: &mut Option<HashMap<usize, usize>>,
) -> usize {
    find_colon_fence_end_with(
        lines,
        start,
        fence_len,
        ColonScanMode {
            literal,
            boundary: None,
        },
        closer_index,
        comment_closers,
        |at| std::borrow::Cow::Borrowed(lines.at(at)),
    )
}

pub(in crate::parse) fn find_colon_fence_end_bounded(
    lines: CursorLines<'_>,
    start: usize,
    fence_len: usize,
    literal: bool,
    closer_index: &mut Option<HashMap<u8, Vec<usize>>>,
    comment_closers: &mut Option<HashMap<usize, usize>>,
    boundary: &mut dyn FnMut(&str, usize) -> bool,
) -> usize {
    find_colon_fence_end_with(
        lines,
        start,
        fence_len,
        ColonScanMode {
            literal,
            boundary: Some(boundary),
        },
        closer_index,
        comment_closers,
        |at| std::borrow::Cow::Borrowed(lines.at(at)),
    )
}

/// Scan a line block after removing its host's structural indentation. Lines
/// are measured only as the scan reaches them, so short blocks do not copy the
/// rest of a large item for each opener.
pub(in crate::parse) fn find_measured_line_block_end(
    lines: CursorLines<'_>,
    start: usize,
    fence_len: usize,
    column: usize,
    comment_closers: &mut Option<HashMap<usize, usize>>,
) -> usize {
    find_colon_fence_end_with(
        lines,
        start,
        fence_len,
        ColonScanMode {
            literal: true,
            boundary: None,
        },
        &mut None,
        comment_closers,
        |at| std::borrow::Cow::Owned(slice_columns(lines.at(at), column, true)),
    )
}

type ColonBoundary<'a> = &'a mut dyn FnMut(&str, usize) -> bool;

struct ColonScanMode<'a> {
    literal: bool,
    boundary: Option<ColonBoundary<'a>>,
}

fn find_colon_fence_end_with<'a>(
    lines: CursorLines<'a>,
    start: usize,
    fence_len: usize,
    mut mode: ColonScanMode<'_>,
    closer_index: &mut Option<HashMap<u8, Vec<usize>>>,
    _comment_closers: &mut Option<HashMap<usize, usize>>,
    mut measured: impl FnMut(usize) -> std::borrow::Cow<'a, str>,
) -> usize {
    let literal = mode.literal;
    if !literal && mode.boundary.is_none() {
        if let Some(end) = lines.colon_end(start) {
            return end;
        }
    }
    // Built on the first code fence seen, not up front: this function is called
    // often, and an unconditional O(lines) build here made a document of
    // unterminated `%%%` openers - which never reach the fence branch at all -
    // quadratic, as tests/perf_regressions.rs caught.
    let mut stack = vec![fence_len];
    let mut idx = start + 1;
    let mut paragraph_open = None;
    while idx < lines.len() {
        let line = measured(idx);
        let line = line.as_ref();
        if mode.boundary.as_mut().is_some_and(|f| f(line, idx)) {
            return idx;
        }
        let top = *stack.last().unwrap();
        let paragraph_was_open = paragraph_open;
        paragraph_open = if literal {
            None
        } else {
            opens_colon_body_paragraph(line, paragraph_was_open, false)
        };
        if exact_colon_fence_len(line) == Some(top) {
            stack.pop();
            idx += 1;
            if stack.is_empty() {
                return idx;
            }
            continue;
        }
        if let Some(open) = detect_fence_open(if literal {
            trim_ascii_start(line)
        } else {
            line
        }) {
            // Same rule as `collect_indexed_colon_body`, which walks this body
            // for real: a fence that closes is opaque to every colon line, and
            // an unclosed one is code up to the innermost container's own
            // closer (carve#515, carve#3201). The two walks must agree about
            // where the container ends.
            //
            // The index makes the `any` below skip-able. Without it this scan
            // runs from every opener to the end of the input, which is
            // quadratic in the number of unterminated openers.
            let possible = lines
                .code_after(idx, open.fence_char, open.fence_len)
                .unwrap_or_else(|| {
                    let index =
                        closer_index.get_or_insert_with(|| build_code_closer_last_index(&lines));
                    code_closer_exists_after(index, idx, open.fence_char, open.fence_len)
                });
            let has_closer = possible
                && (idx + 1..lines.len()).any(|at| {
                    let candidate = measured(at);
                    is_fence_close(
                        if literal {
                            trim_ascii_start(&candidate)
                        } else {
                            &candidate
                        },
                        open,
                    )
                });
            if has_closer {
                idx += 1;
                while idx < lines.len() {
                    let candidate = measured(idx);
                    idx += 1;
                    if is_fence_close(
                        if literal {
                            trim_ascii_start(&candidate)
                        } else {
                            &candidate
                        },
                        open,
                    ) {
                        break;
                    }
                }
                paragraph_open = None;
                continue;
            }
            if !literal && paragraph_was_open.is_none() {
                idx += 1;
                while idx < lines.len() && exact_colon_fence_len(&measured(idx)) != Some(top) {
                    idx += 1;
                }
                paragraph_open = None;
                continue;
            }
        }
        if let Some(open) = detect_comment_fence_line(line) {
            match (idx + 1..lines.len())
                .find(|&j| is_comment_fence_close_any_column(&measured(j), open.fence_len))
            {
                Some(close)
                    if !literal || !(idx + 1..close).any(|at| measured(at).starts_with(LAZY)) =>
                {
                    idx = close + 1;
                    paragraph_open = None;
                    continue;
                }
                Some(_) => {}
                // A lazy line ends an unclosed verse span (S1, carve#4956).
                None if literal
                    && (idx + 1..lines.len()).any(|at| measured(at).starts_with(LAZY)) => {}
                // An unclosed span hides the block's own closer (carve#4907).
                None => {
                    for at in idx + 1..lines.len() {
                        let line = measured(at);
                        if mode.boundary.as_mut().is_some_and(|f| f(line.as_ref(), at)) {
                            return at;
                        }
                    }
                    return lines.len();
                }
            }
        }
        if !literal && !line.starts_with([' ', '\t']) {
            if let Some(len) = detect_container_open(line)
                .map(|open| open.fence_len)
                .or_else(|| detect_line_block_open(line))
                .or_else(|| detect_hardbreaks_block_open(line))
                .or_else(|| detect_quote_block_open(line))
            {
                stack.push(len);
                idx += 1;
                continue;
            }
        }
        idx += 1;
    }
    lines.len()
}

/// The column a footnote body's continuation lines must reach.
pub(in crate::parse) fn footnote_body_floor(def_line: &str) -> usize {
    indent_columns(def_line) + 2
}

pub(in crate::parse) fn leading_ws(line: &str) -> usize {
    line.bytes()
        .take_while(|b| *b == b' ' || *b == b'\t')
        .count()
}

// Visual column of the leading whitespace, expanding tabs to the next
// CommonMark tab stop (a multiple of 4). For space-only indentation this
// equals leading_ws(). Used for list-nesting comparisons.
// COUNTED work probes for carve-rs#2301. Test-only, so a release build carries
// nothing.
#[cfg(test)]
thread_local! {
    /// Leading-whitespace bytes `indent_columns` has stepped over.
    pub(crate) static INDENT_BYTES_WALKED: std::cell::Cell<u64> =
        const { std::cell::Cell::new(0) };
    /// Lines pushed into a collected item body, counting every re-collection.
    pub(crate) static BODY_LINES_COLLECTED: std::cell::Cell<u64> =
        const { std::cell::Cell::new(0) };
    /// Footnote comment followers looked up in the indexed fence closers.
    pub(in crate::parse) static FOOTNOTE_FENCE_CLOSER_LOOKUPS: std::cell::Cell<u64> =
        const { std::cell::Cell::new(0) };
}

pub(in crate::parse) fn indent_columns(line: &str) -> usize {
    let mut col = 0;
    #[cfg(test)]
    let mut walked = 0u64;
    for b in line.bytes() {
        match b {
            b' ' => col += 1,
            b'\t' => col += 4 - (col % 4),
            _ => break,
        }
        #[cfg(test)]
        {
            walked += 1;
        }
    }
    #[cfg(test)]
    INDENT_BYTES_WALKED.with(|c| c.set(c.get() + walked));
    col
}

pub(in crate::parse) fn slice_columns(line: &str, cols: usize, keep_residual: bool) -> String {
    slice_columns_mapped(line, cols, keep_residual).0
}

/// `slice_columns`, plus what a caller needs to map its output back to source.
///
/// Returns `(sliced, consumed, synthetic)`: the characters taken off the front,
/// and the number of SPACES written in their place for a straddling tab's
/// residual. Those spaces are not in the source, so a caller that treats the
/// result as a plain suffix charges offsets to characters that do not exist,
/// and a span near the end runs past the end of the document (carve-rs#700).
///
/// The line's real content still IS a suffix, though, so the mapping is exact
/// with one subtraction: output position `p >= synthetic` sits at source
/// position `consumed + p - synthetic`. Only a position INSIDE the synthetic
/// run has no source, and nothing starts there - the run is whitespace and the
/// marker follows it. carve-js#771 fixed the same defect the same way.
pub(in crate::parse) fn slice_columns_mapped(
    line: &str,
    cols: usize,
    keep_residual: bool,
) -> (String, usize, usize) {
    let bytes = line.as_bytes();
    let mut col = 0;
    let mut i = 0;
    while i < bytes.len() && col < cols {
        match bytes[i] {
            b' ' => {
                col += 1;
                i += 1;
            }
            b'\t' => {
                col += 4 - (col % 4);
                i += 1;
            }
            _ => break,
        }
    }
    let consumed = line[..i].chars().count();
    if keep_residual && col > cols {
        let synthetic = col - cols;
        let mut s = " ".repeat(synthetic);
        s.push_str(&line[i..]);
        (s, consumed, synthetic)
    } else {
        (line[i..].to_string(), consumed, 0)
    }
}

/// Whether the quoted body so far leaves a paragraph open.
pub(in crate::parse) enum ParaOpen<'a> {
    /// Closed, decided without consulting any line.
    Closed,
    /// Open iff `line`'s innermost quoted content is paragraph text, under the
    /// absorption state it inherits. `answer` caches the verdict,
    /// so re-reading the same state walks once.
    Deferred {
        line: &'a str,
        inherited_absorption: bool,
        answer: Option<Verdict>,
    },
}

/// What one resolved quoted line says about the paragraph and the absorption.
#[derive(Clone, Copy)]
pub(in crate::parse) struct Verdict {
    /// Whether the line leaves a paragraph open.
    open: bool,
    /// Whether the line is a glued colon fence that starts §12's absorption,
    /// which only an OPEN paragraph can carry (so this implies `open`).
    opens_absorption: bool,
}

impl<'a> ParaOpen<'a> {
    pub(in crate::parse) fn from_line(line: &'a str, inherited_absorption: bool) -> Self {
        ParaOpen::Deferred {
            line,
            inherited_absorption,
            answer: None,
        }
    }

    /// Whether resolving this line can change the absorption carried by the
    /// next one. Keeping this cheap guard preserves the deep-quote deferral.
    pub(in crate::parse) fn may_carry_absorption(&self) -> bool {
        match self {
            ParaOpen::Closed => false,
            ParaOpen::Deferred {
                line,
                inherited_absorption,
                ..
            } => *inherited_absorption || line.contains(":::"),
        }
    }

    /// Absorption inherited by the next quoted line.
    pub(in crate::parse) fn absorption(&mut self) -> bool {
        let inherited = match self {
            ParaOpen::Closed => return false,
            ParaOpen::Deferred {
                inherited_absorption,
                ..
            } => *inherited_absorption,
        };
        let verdict = self.resolve();
        verdict.open && (inherited || verdict.opens_absorption)
    }

    pub(in crate::parse) fn get(&mut self) -> bool {
        self.resolve().open
    }

    fn resolve(&mut self) -> Verdict {
        let ParaOpen::Deferred {
            line,
            inherited_absorption,
            answer,
        } = self
        else {
            return Verdict {
                open: false,
                opens_absorption: false,
            };
        };
        if let Some(known) = answer {
            return *known;
        }
        let suppressed = *inherited_absorption;
        // Look THROUGH any further quote markers before deciding. A lazy line
        // continues the innermost OPEN PARAGRAPH, however many containers it
        // failed to match, so what matters is whether the innermost quoted
        // content leaves a paragraph open - not whether this line opens
        // another quote.
        //
        // Without this, `>> b` stripped to `> b`, which reads as a container
        // opener, so the paragraph closed and a following bare line could not
        // fold. That made laziness work at depth 1 and not at depth 2, which
        // is not a reading of any rule: PART 1 S4's strict wording would close
        // the quote at depth 1 too, and nothing does that (markup-carve/carve#506).
        let mut innermost = *line;
        while let Some(rest) = strip_blockquote_prefix(innermost) {
            innermost = rest;
        }
        // A definition at the reached container's content column closes its
        // paragraph even when it was written past that column (PART 0 S3).
        let at_content = trim_ascii_start(innermost);
        if is_definition_placeholder(at_content)
            || parse_link_def_line(at_content).is_some()
            || parse_footnote_def_line(at_content).is_some()
        {
            let verdict = Verdict {
                open: false,
                opens_absorption: false,
            };
            *answer = Some(verdict);
            return verdict;
        }
        let absorbed = suppressed && is_suppressed_colon_fence_line(innermost);
        let open = !is_blank_line(innermost)
            && (absorbed
                || ((innermost.starts_with([' ', '\t'])
                    || detect_container_open(innermost).is_none())
                    && !trim_ascii_start(innermost).starts_with("%%")
                    && !interrupts_paragraph_with_rest(innermost, &[])));
        let verdict = Verdict {
            open,
            opens_absorption: false,
        };
        *answer = Some(verdict);
        verdict
    }
}
