use super::*;

pub(crate) fn lone_rendered_image(children: &[InlineNode]) -> Option<usize> {
    let mut image = None;
    for (index, child) in children.iter().enumerate() {
        match child {
            InlineNode::Comment(_) => continue,
            InlineNode::Image(img) if image.is_none() && !is_unresolved_image(img) => {
                image = Some(index)
            }
            _ => return None,
        }
    }
    image
}

/// Publish leading block attributes on the resolved image that owns a block
/// image paragraph. The paragraph remains the wire node for `blockImage`.
pub(crate) fn publish_block_image_attrs(paragraph: &mut Paragraph) {
    if !paragraph.block_image {
        return;
    }
    let Some(index) = lone_rendered_image(&paragraph.children) else {
        return;
    };
    if let Some(attrs) = paragraph.attrs.take() {
        if let InlineNode::Image(image) = &mut paragraph.children[index] {
            merge_leading_attrs(&mut image.attrs, attrs);
        }
    }
}

/// Collapse lone-image paragraphs for HTML regardless of source column.
/// Parsing retains an indented lone image as a paragraph; HTML rendering
/// promotes it on purpose (markup-carve/carve#1660).
pub(crate) fn collapse_lone_image_paragraphs(doc: &mut Document) {
    promote_block_images(&mut doc.children, false, false);
    for blocks in doc.footnote_defs.values_mut() {
        promote_block_images(blocks, false, false);
    }
}

/// Visit subtrees with a worklist to keep stack use independent of depth.
pub(in crate::parse) fn promote_block_images(
    blocks: &mut [BlockNode],
    figures_only: bool,
    lone_image_needs_content_column: bool,
) {
    let mut worklist: Vec<&mut [BlockNode]> = vec![blocks];
    while let Some(level) = worklist.pop() {
        for block in level {
            let single_image = !figures_only
                && !lone_image_needs_content_column
                && matches!(block, BlockNode::Paragraph(p) if lone_rendered_image(&p.children).is_some());
            if single_image {
                // Take the children out first so the paragraph borrow ends before
                // `block` is reassigned. A leading block-attribute line (`{#id}`)
                // landed on the paragraph; carry it onto the promoted block image
                // (its own inline attrs win on conflict, §15), matching a direct
                // block image -- otherwise the id would be lost with the wrapper.
                let (mut children, para_attrs) = match block {
                    BlockNode::Paragraph(p) => (std::mem::take(&mut p.children), p.attrs.take()),
                    _ => unreachable!(),
                };
                if let InlineNode::Image(mut img) =
                    children.remove(lone_rendered_image(&children).unwrap())
                {
                    if let Some(attrs) = para_attrs {
                        merge_leading_attrs(&mut img.attrs, attrs);
                    }
                    *block = BlockNode::BlockImage(img);
                }
                continue;
            }
            let ref_figure = matches!(
                block,
                BlockNode::Paragraph(p)
                    if p.at_content_column
                        && p.children.len() >= 3
                        && matches!(&p.children[0], InlineNode::Image(img) if !is_unresolved_image(img))
                        && matches!(p.children[1], InlineNode::SoftBreak(_))
                        && matches!(&p.children[2], InlineNode::Text(t) if caption_marker_len(&t.value).is_some())
                        && caption_first_line_has_content(&p.children)
            );
            if ref_figure {
                // Carry a leading block-attribute line (`{#id}` etc.) from the
                // paragraph onto the figure, matching a direct-image figure (which
                // takes the attrs at parse time) and carve-php -- otherwise
                // `carve fmt` would drop it.
                // The paragraph's own span IS the figure's: it opened at the
                // image and ran to the end of the caption, which is exactly the
                // construct the author wrote. Taken here, before the paragraph is
                // dismantled, because nothing downstream can reconstruct it -- the
                // image and the caption inlines are placed, but the figure's own
                // extent only exists on the node being replaced (carve-rs#737).
                let (mut children, attrs, para_pos) = match block {
                    BlockNode::Paragraph(p) => (
                        std::mem::take(&mut p.children),
                        p.attrs.take(),
                        p.pos.take(),
                    ),
                    _ => unreachable!(),
                };
                let InlineNode::Image(img) = children.remove(0) else {
                    unreachable!()
                };
                children.remove(0); // the soft break
                if let InlineNode::Text(t) = &mut children[0] {
                    let n = caption_marker_len(&t.value).unwrap();
                    let rest = t.value[n..].to_string();
                    if rest.is_empty() {
                        children.remove(0);
                    } else {
                        t.value = rest;
                        if let Some(pos) = &mut t.pos {
                            pos.start_offset += n;
                            pos.start_column += n;
                        }
                    }
                }
                mark_promoted_caption_number(&mut children);
                *block = BlockNode::Figure(Figure {
                    attrs,
                    target: Box::new(FigureTarget::Image(img)),
                    rendered_target: None,
                    caption: children,
                    short_caption: None,
                    // PART 12 §4 exempts a REASSEMBLED node, and this one is not:
                    // its lines are contiguous and the direct-image path publishes
                    // exactly this span for the same construct. markup-carve/carve#913
                    // rules `pos` markup-inclusive with a parent's span containing
                    // every child's, which the paragraph's span already satisfies.
                    pos: para_pos,
                });
                continue;
            }
            // The field records render semantics, so it is not column-gated.
            if let BlockNode::Paragraph(p) = block {
                p.block_image = lone_rendered_image(&p.children).is_some();
                // The source writer still needs the authored leading attribute
                // line on its paragraph. Published ASTs put those attributes
                // on the resolved image instead.
                if !figures_only {
                    publish_block_image_attrs(p);
                }
            }
            match block {
                BlockNode::BlockQuote(b) => worklist.push(b.children.as_mut_slice()),
                BlockNode::Admonition(a) => worklist.push(a.children.as_mut_slice()),
                // Descend into the group so an image-with-caption paragraph built
                // from a resolved reference image still becomes a panel (§4c).
                BlockNode::FigureGroup(g) => worklist.push(g.children.as_mut_slice()),
                BlockNode::Directive(d) => worklist.push(d.children.as_mut_slice()),
                BlockNode::Div(d) => worklist.push(d.children.as_mut_slice()),
                BlockNode::List(l) => {
                    for item in &mut l.items {
                        worklist.push(item.children.as_mut_slice());
                    }
                }
                BlockNode::DefinitionList(d) => {
                    for item in &mut d.items {
                        for def in &mut item.definitions {
                            worklist.push(def.as_mut_slice());
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

/// A caption promoted from a paragraph was first parsed as ordinary inline
/// text. Give its first bare top-level `#` the same meaning as a directly
/// parsed caption without reparsing the other inline nodes.
fn mark_promoted_caption_number(caption: &mut Vec<InlineNode>) {
    for index in 0..caption.len() {
        let InlineNode::Text(text) = &caption[index] else {
            continue;
        };
        if text
            .pos
            .as_ref()
            .is_some_and(|pos| pos.end_offset - pos.start_offset != text.value.chars().count())
        {
            continue;
        }
        let Some((byte, _)) = text.value.char_indices().find(|(at, ch)| {
            *ch == '#'
                && !text
                    .value
                    .as_bytes()
                    .get(at + 1)
                    .is_some_and(|next| is_attr_ident_part(*next))
        }) else {
            continue;
        };
        let before = text.value[..byte].to_string();
        let after = text.value[byte + 1..].to_string();
        let original = text.pos.clone();
        let number_pos = original.as_ref().map(|pos| {
            let mut at = pos.clone();
            at.start_offset += before.chars().count();
            at.start_column += before.chars().count();
            at.end_offset = at.start_offset + 1;
            at.end_column = at.start_column + 1;
            at
        });
        let mut replacement = Vec::with_capacity(3);
        if !before.is_empty() {
            let pos = original.as_ref().map(|pos| {
                let mut span = pos.clone();
                span.end_offset = number_pos.as_ref().unwrap().start_offset;
                span.end_column = number_pos.as_ref().unwrap().start_column;
                span
            });
            replacement.push(InlineNode::Text(Text { value: before, pos }));
        }
        replacement.push(InlineNode::CaptionNumber(CaptionNumber {
            number: None,
            pos: number_pos.clone(),
        }));
        if !after.is_empty() {
            let pos = original.map(|mut span| {
                span.start_offset = number_pos.as_ref().unwrap().end_offset;
                span.start_column = number_pos.as_ref().unwrap().end_column;
                span
            });
            replacement.push(InlineNode::Text(Text { value: after, pos }));
        }
        caption.splice(index..=index, replacement);
        return;
    }
}

/// An image the document never resolved: it carries a reference and no source.
///
/// PART 12 §3a keeps `ref` and `raw_ref` on a RESOLVED reference as well, so the
/// presence of a label stopped answering this on its own - a resolved reference
/// image stopped promoting to a figure (carve#597).
pub(crate) fn is_unresolved_image(image: &Image) -> bool {
    image.ref_label.is_some() && image.src.is_empty() && !image.resolved_empty_reference
}

pub(crate) fn is_collapsed_reference(link: &Link) -> bool {
    let Some(raw) = &link.raw_ref else {
        return false;
    };
    let bytes = raw.as_bytes();
    let Some((_, after_text)) = read_bracketed(bytes, 0) else {
        return false;
    };
    let Some((label, _)) = read_bracketed(bytes, after_text) else {
        return false;
    };
    label.is_empty()
}

/// Every explicit `{#id}` in these blocks, for the auto-slug skip below.
///
/// Mirrors `document_ids`'s pass A: an auto slug must not land on an id an
/// explicit one already claims, and deciding that needs the whole document
/// first, since the explicit id may appear after the heading that would
/// collide with it (#335).
/// A WORKLIST, not a descent (carve-rs#1186). This pass runs on EVERY parse and
/// walks a container's children whether or not the document has a heading, so
/// nesting depth cost host stack for a walk that visits nothing.
///
/// ORDER IS PRESERVED, which is what stops a worklist from being a rewrite of
/// the rule as well as of the shape: the remainder of the current level is
/// pushed BEFORE this block's children, so the children pop first and the visit
/// stays pre-order document order. Sibling child slices are pushed in reverse
/// for the same reason. `collect_explicit_ids` fills a set and would survive a
/// reordering; `collect_heading_titles` below shares this shape and would NOT -
/// its numbering and its "first occurrence wins" both read document order - so
/// the two use the same spelling rather than one relying on being order-blind.
pub(in crate::parse) fn collect_explicit_ids(
    blocks: &[BlockNode],
    out: &mut std::collections::BTreeSet<String>,
) {
    let mut worklist: Vec<&[BlockNode]> = vec![blocks];
    while let Some(level) = worklist.pop() {
        let Some((block, rest)) = level.split_first() else {
            continue;
        };
        if !rest.is_empty() {
            worklist.push(rest);
        }
        match block {
            BlockNode::Heading(h) => {
                if let Some(id) = h.attrs.as_ref().and_then(|a| a.id.as_ref()) {
                    out.insert(id.clone());
                }
            }
            BlockNode::Paragraph(p) => {
                if let Some(id) = p.attrs.as_ref().and_then(|a| a.id.as_ref()) {
                    out.insert(id.clone());
                }
            }
            BlockNode::List(l) => {
                for item in l.items.iter().rev() {
                    worklist.push(&item.children);
                }
            }
            BlockNode::BlockQuote(b) => worklist.push(&b.children),
            BlockNode::Admonition(a) => worklist.push(&a.children),
            BlockNode::FigureGroup(g) => worklist.push(&g.children),
            BlockNode::Directive(d) => worklist.push(&d.children),
            BlockNode::Div(d) => worklist.push(&d.children),
            BlockNode::DefinitionList(d) => {
                for item in d.items.iter().rev() {
                    for definition in item.definitions.iter().rev() {
                        worklist.push(definition);
                    }
                }
            }
            _ => {}
        }
    }
}

/// `in_blockquote`: the heading still gets an id and stays a `</#id>` target,
/// but PART 11 R1 declines it from the implicit `[label][]` reference index -
/// quoted text names the QUOTED document's headings, not this one's. Recorded
/// rather than skipped, because the two lookups share one walk.
/// The four accumulators `collect_heading_titles` threads through its walk,
/// grouped so the recursion carries one `&mut` rather than a widening argument
/// list.
pub(in crate::parse) struct HeadingScan<'a> {
    pub(in crate::parse) counts: &'a mut BTreeMap<String, usize>,
    pub(in crate::parse) titles: &'a mut BTreeMap<String, String>,
    /// Id -> the heading's own inline NODES, which is what a resolved
    /// cross-reference clones (PART 9R R4). See `CrossrefIndex::labels`.
    pub(in crate::parse) labels: &'a mut BTreeMap<String, CrossrefLabel>,
    pub(in crate::parse) quoted: &'a mut std::collections::BTreeSet<String>,
    /// Normalized heading text -> id, first occurrence wins (PART 11 R1).
    pub(in crate::parse) by_text: &'a mut BTreeMap<String, String>,
    /// Optional document-order ids for callers that also stamp the parsed AST.
    pub(in crate::parse) assigned: Option<&'a mut Vec<String>>,
    /// Build the title/node maps only when a cross-reference can consume them.
    pub(in crate::parse) collect_crossrefs: bool,
    /// Build the rendered-heading-text map only for collapsed references.
    pub(in crate::parse) collect_heading_refs: bool,
}

/// A WORKLIST, for the same reason and with the same order discipline as
/// `collect_explicit_ids` above (carve-rs#1186). This one carries the
/// `in_blockquote` flag on each queued level, because a block quote changes it
/// for its subtree and nothing else does.
///
/// DOCUMENT ORDER IS LOAD-BEARING HERE, not incidental: `scan.counts` numbers a
/// duplicate heading id by how many it has already seen, and `scan.by_text`
/// resolves PART 11 R1's "the FIRST heading with that text" with an
/// `or_insert`. A LIFO pop that reversed siblings would renumber every
/// duplicate and hand R1 the last occurrence instead of the first.
pub(in crate::parse) fn collect_heading_titles(
    blocks: &[BlockNode],
    scan: &mut HeadingScan<'_>,
    id_opts: HeadingIdOptions,
    explicit_ids: &std::collections::BTreeSet<String>,
    in_blockquote: bool,
) {
    let mut worklist: Vec<(&[BlockNode], bool)> = vec![(blocks, in_blockquote)];
    while let Some((level, in_blockquote)) = worklist.pop() {
        let Some((block, rest)) = level.split_first() else {
            continue;
        };
        if !rest.is_empty() {
            worklist.push((rest, in_blockquote));
        }
        {
            match block {
                BlockNode::Heading(h) => {
                    let title = plain_inlines_parse(&h.children);
                    let base = h
                        .attrs
                        .as_ref()
                        .and_then(|attrs| attrs.id.clone())
                        .unwrap_or_else(|| slugify_parse(&title, id_opts));
                    // Same numbering the renderer uses, INCLUDING the skip past an
                    // id an explicit `{#id}` already claims. Without it this index
                    // assigned `API-2` to a heading the renderer calls `API-3`, so a
                    // cross-reference resolved to the wrong heading - or, once the
                    // renderer was fixed, to none at all (#335).
                    let has_explicit = h.attrs.as_ref().is_some_and(|a| a.id.is_some());
                    let id = crate::document_ids::allocate_heading_id(
                        base,
                        has_explicit,
                        scan.counts,
                        |id| explicit_ids.contains(id),
                    );
                    if let Some(assigned) = scan.assigned.as_deref_mut() {
                        assigned.push(id.clone());
                    }
                    if scan.collect_heading_refs && in_blockquote {
                        scan.quoted.insert(id.clone());
                    } else if scan.collect_heading_refs {
                        // First occurrence wins - R1 resolves to "the FIRST heading
                        // with that text". This walk is in document order, so an
                        // `or_insert` is that rule. A scan.quoted heading is not indexed
                        // at all, so a later unquoted one with the same text still
                        // wins rather than being shadowed by a declined entry.
                        let text_key = normalize_heading_label(&title);
                        if !text_key.is_empty() {
                            scan.by_text.entry(text_key).or_insert_with(|| id.clone());
                        }
                    }
                    if scan.collect_crossrefs {
                        scan.labels
                            .insert(id.clone(), crossref_label_nodes(&h.children));
                        scan.titles.insert(id, title);
                    } else if scan.collect_heading_refs {
                        // `resolve_ref` returns both the id and its title even
                        // though implicit references only consume the id today.
                        scan.titles.insert(id, title);
                    }
                }
                BlockNode::List(l) => {
                    for item in l.items.iter().rev() {
                        worklist.push((&item.children, in_blockquote));
                    }
                }
                BlockNode::BlockQuote(b) => worklist.push((&b.children, true)),
                BlockNode::Admonition(a) => worklist.push((&a.children, in_blockquote)),
                BlockNode::FigureGroup(g) => worklist.push((&g.children, in_blockquote)),
                BlockNode::Directive(d) => worklist.push((&d.children, in_blockquote)),
                BlockNode::Div(d) => worklist.push((&d.children, in_blockquote)),
                BlockNode::DefinitionList(d) => {
                    for item in d.items.iter().rev() {
                        for definition in item.definitions.iter().rev() {
                            worklist.push((definition, in_blockquote));
                        }
                    }
                }
                BlockNode::Figure(f) => match &*f.target {
                    FigureTarget::BlockQuote(b) => worklist.push((&b.children, true)),
                    FigureTarget::Table(_)
                    | FigureTarget::Image(_)
                    | FigureTarget::CodeBlock(_)
                    | FigureTarget::Paragraph(_) => {}
                },
                _ => {}
            }
        }
    }
}

pub(in crate::parse) fn number_captioned_blocks(
    blocks: &mut [BlockNode],
    counts: &mut BTreeMap<String, usize>,
    titles: &mut BTreeMap<String, String>,
) {
    for block in blocks {
        match block {
            BlockNode::Table(t) => number_table_caption(t, counts, titles),
            BlockNode::Figure(f) => {
                number_caption(&mut f.caption, f.attrs.as_ref(), counts, titles);
                match &mut *f.target {
                    FigureTarget::BlockQuote(b) => {
                        number_captioned_blocks(&mut b.children, counts, titles);
                    }
                    FigureTarget::Table(t) => number_table_caption(t, counts, titles),
                    FigureTarget::Image(_)
                    | FigureTarget::CodeBlock(_)
                    | FigureTarget::Paragraph(_) => {}
                }
            }
            BlockNode::FigureGroup(group) => {
                // THE GROUP IS ONE NUMBERING UNIT (§4c). Its caption draws
                // first - before anything inside the body, matching the
                // oracle - and that one draw is also what the panel ids
                // register under, with a letter by panel order.
                let drew = group.caption.as_mut().and_then(|caption| {
                    number_caption(caption, group.attrs.as_ref(), counts, titles)
                });
                if let Some((label, number)) = &drew {
                    register_panel_titles(&group.children, label, *number, titles);
                }
                // PANELS ARE NOT SEQUENCE UNITS: a panel's own caption draws
                // nothing (a `#` there stays literal, §4c), but content
                // inside a quote panel and every non-panel child numbers
                // normally.
                for child in &mut group.children {
                    match child {
                        BlockNode::Figure(f) => {
                            if let FigureTarget::BlockQuote(b) = &mut *f.target {
                                number_captioned_blocks(&mut b.children, counts, titles);
                            }
                        }
                        BlockNode::Table(_) => {}
                        other => {
                            number_captioned_blocks(std::slice::from_mut(other), counts, titles)
                        }
                    }
                }
            }
            BlockNode::List(l) => {
                for item in &mut l.items {
                    number_captioned_blocks(&mut item.children, counts, titles);
                }
            }
            BlockNode::BlockQuote(b) => number_captioned_blocks(&mut b.children, counts, titles),
            BlockNode::Admonition(a) => number_captioned_blocks(&mut a.children, counts, titles),
            BlockNode::Directive(d) => number_captioned_blocks(&mut d.children, counts, titles),
            BlockNode::Div(d) => number_captioned_blocks(&mut d.children, counts, titles),
            BlockNode::DefinitionList(d) => {
                for item in &mut d.items {
                    for definition in &mut item.definitions {
                        number_captioned_blocks(definition, counts, titles);
                    }
                }
            }
            _ => {}
        }
    }
}

pub(in crate::parse) fn number_table_caption(
    table: &mut Table,
    counts: &mut BTreeMap<String, usize>,
    titles: &mut BTreeMap<String, String>,
) {
    if let Some(caption) = &mut table.caption {
        number_caption(caption, table.attrs.as_ref(), counts, titles);
    }
}

/// Returns the label and number the caption drew, when it held a `#`
/// placeholder - the figure group's arm derives its panels' crossref text
/// from that draw (§4c).
pub(in crate::parse) fn number_caption(
    caption: &mut [InlineNode],
    attrs: Option<&Attrs>,
    counts: &mut BTreeMap<String, usize>,
    titles: &mut BTreeMap<String, String>,
) -> Option<(String, usize)> {
    let idx = caption
        .iter()
        .position(|node| matches!(node, InlineNode::CaptionNumber(_)))?;
    let label = plain_inlines_parse(&caption[..idx])
        .trim_end_matches(char::is_whitespace)
        .to_string();
    let next = counts.entry(label.clone()).or_insert(0);
    *next += 1;
    let number = *next;
    if let InlineNode::CaptionNumber(caption_number) = &mut caption[idx] {
        caption_number.number = Some(number);
    }
    if let Some(id) = attrs.and_then(|attrs| attrs.id.as_ref()) {
        titles
            .entry(id.clone())
            .or_insert_with(|| format!("{label} {number}"));
    }
    Some((label, number))
}

/// A panel's crossref letter by its order among the group's panels: a..z,
/// then aa, ab, ... (PART 9 §4c; the letters exist in crossref text only).
pub(in crate::parse) fn panel_letter(index: usize) -> String {
    let mut out = Vec::new();
    let mut n = index + 1;
    while n > 0 {
        n -= 1;
        out.push(b'a' + (n % 26) as u8);
        n /= 26;
    }
    out.reverse();
    String::from_utf8(out).expect("ascii letters")
}

/// Register a numbered group's panel ids as "Label N" plus a letter by panel
/// order (§4c). Panels are the `Figure` and `Table` nodes among the group's
/// direct children; an unnumbered group's panels stay plain anchors, exactly
/// as an id on an uncaptioned figure does.
pub(in crate::parse) fn register_panel_titles(
    children: &[BlockNode],
    label: &str,
    number: usize,
    titles: &mut BTreeMap<String, String>,
) {
    let mut panel_index = 0usize;
    for child in children {
        let id = match child {
            BlockNode::Figure(f) => f.attrs.as_ref().and_then(|attrs| attrs.id.clone()),
            BlockNode::Table(t) => t.attrs.as_ref().and_then(|attrs| attrs.id.clone()),
            _ => continue,
        };
        if let Some(id) = id {
            titles
                .entry(id)
                .or_insert_with(|| format!("{label} {number}{}", panel_letter(panel_index)));
        }
        panel_index += 1;
    }
}

pub(in crate::parse) fn collect_caption_titles(
    blocks: &[BlockNode],
    titles: &mut BTreeMap<String, String>,
) {
    for block in blocks {
        match block {
            BlockNode::Table(t) => collect_table_caption_title(t, titles),
            BlockNode::Figure(f) => {
                collect_caption_title(&f.caption, f.attrs.as_ref(), titles);
                match &*f.target {
                    FigureTarget::BlockQuote(b) => collect_caption_titles(&b.children, titles),
                    FigureTarget::Table(t) => collect_table_caption_title(t, titles),
                    FigureTarget::Image(_)
                    | FigureTarget::CodeBlock(_)
                    | FigureTarget::Paragraph(_) => {}
                }
            }
            BlockNode::FigureGroup(g) => {
                // The ingest twin of the group arm in `number_captioned_blocks`:
                // read the number the group's caption already carries, register
                // the group id and the panel letters from that one draw, and
                // skip the panel captions exactly as the numbering pass does.
                if let Some(caption) = &g.caption {
                    collect_caption_title(caption, g.attrs.as_ref(), titles);
                    if let Some((label, number)) = numbered_caption_draw(caption) {
                        register_panel_titles(&g.children, &label, number, titles);
                    }
                }
                for child in &g.children {
                    match child {
                        BlockNode::Figure(f) => {
                            if let FigureTarget::BlockQuote(b) = &*f.target {
                                collect_caption_titles(&b.children, titles);
                            }
                        }
                        BlockNode::Table(_) => {}
                        other => collect_caption_titles(std::slice::from_ref(other), titles),
                    }
                }
            }
            BlockNode::List(l) => {
                for item in &l.items {
                    collect_caption_titles(&item.children, titles);
                }
            }
            BlockNode::BlockQuote(b) => collect_caption_titles(&b.children, titles),
            BlockNode::Admonition(a) => collect_caption_titles(&a.children, titles),
            BlockNode::Directive(d) => collect_caption_titles(&d.children, titles),
            BlockNode::Div(d) => collect_caption_titles(&d.children, titles),
            BlockNode::DefinitionList(d) => {
                for item in &d.items {
                    for definition in &item.definitions {
                        collect_caption_titles(definition, titles);
                    }
                }
            }
            _ => {}
        }
    }
}

/// The label and number an ALREADY-NUMBERED caption carries, read without
/// assigning anything - the ingest-side twin of `number_caption`'s return.
pub(in crate::parse) fn numbered_caption_draw(caption: &[InlineNode]) -> Option<(String, usize)> {
    let idx = caption
        .iter()
        .position(|node| matches!(node, InlineNode::CaptionNumber(_)))?;
    let number = match &caption[idx] {
        InlineNode::CaptionNumber(n) => n.number?,
        _ => return None,
    };
    let label = plain_inlines_parse(&caption[..idx])
        .trim_end_matches(char::is_whitespace)
        .to_string();
    Some((label, number))
}

pub(in crate::parse) fn collect_table_caption_title(
    table: &Table,
    titles: &mut BTreeMap<String, String>,
) {
    if let Some(caption) = &table.caption {
        collect_caption_title(caption, table.attrs.as_ref(), titles);
    }
}

pub(in crate::parse) fn collect_caption_title(
    caption: &[InlineNode],
    attrs: Option<&Attrs>,
    titles: &mut BTreeMap<String, String>,
) {
    let Some(idx) = caption
        .iter()
        .position(|node| matches!(node, InlineNode::CaptionNumber(_)))
    else {
        return;
    };
    let Some(number) = caption.get(idx).and_then(|node| match node {
        InlineNode::CaptionNumber(n) => n.number,
        _ => None,
    }) else {
        return;
    };
    if let Some(id) = attrs.and_then(|attrs| attrs.id.as_ref()) {
        let label = plain_inlines_parse(&caption[..idx])
            .trim_end_matches(char::is_whitespace)
            .to_string();
        titles
            .entry(id.clone())
            .or_insert_with(|| format!("{label} {number}"));
    }
}

/// Enforce "links never nest" (CommonMark: a link may not contain another
/// link). This is a single post-resolution pass: it runs AFTER reference-link
/// resolution because reference links turn into `Link` nodes at that stage. A
/// link found inside another link is unwrapped to its (recursively cleaned)
/// text, so only the outermost destination applies; an autolink inside a link becomes plain text
/// (the display form the renderer would emit, with a leading `mailto:` scheme
/// stripped). A footnote body renders in the endnotes section, outside any
/// anchor, so its links are not nested -- the walk re-enters a footnote body
/// with `inside_link = false`.
/// PART 12 §1a: no node's children hold two adjacent `text` nodes.
///
/// The parser splits a run wherever it had to make a decision. Those splits are
/// bookkeeping, not the document: publishing them lets two engines emit 1 node
/// and 4 for the same characters, both schema-valid, which is what §1's "read
/// another's output" rules out.
///
/// This runs on the TREE rather than on the way out, because §6 requires
/// `parse(x)` serialized and deserialized to equal `parse(x)`. Merging only at
/// the encoder would satisfy §1a and break §6 on the same document.
///
/// Runs merge only where they are CONTIGUOUS in the source. A span covering
/// text the node does not contain -- the `<`/`>` of an unwrapped autolink, the
/// delimiter between two halves of a wrapped table cell -- would not select its
/// own value, which §4 rates worse than no span at all.
pub(in crate::parse) fn coalesce_text_runs(doc: &mut Document) {
    for block in &mut doc.children {
        coalesce_block(block);
    }
    for body in doc.footnote_defs.values_mut() {
        for block in body {
            coalesce_block(block);
        }
    }
}

pub(in crate::parse) fn coalesce_block(block: &mut BlockNode) {
    match block {
        BlockNode::Heading(h) => coalesce_inlines(&mut h.children),
        BlockNode::Paragraph(p) => coalesce_inlines(&mut p.children),
        BlockNode::List(l) => {
            for item in &mut l.items {
                for child in &mut item.children {
                    coalesce_block(child);
                }
            }
        }
        BlockNode::BlockQuote(b) => {
            for child in &mut b.children {
                coalesce_block(child);
            }
        }
        BlockNode::Admonition(a) => {
            if let Some(title) = &mut a.title {
                coalesce_inlines(title);
            }
            for child in &mut a.children {
                coalesce_block(child);
            }
        }
        BlockNode::FigureGroup(g) => {
            if let Some(caption) = &mut g.caption {
                coalesce_inlines(caption);
            }
            for child in &mut g.children {
                coalesce_block(child);
            }
        }
        BlockNode::Directive(d) => {
            for child in &mut d.children {
                coalesce_block(child);
            }
        }
        BlockNode::Div(d) => {
            for child in &mut d.children {
                coalesce_block(child);
            }
        }
        BlockNode::LineBlock(l) => {
            for child in &mut l.children {
                coalesce_block(child);
            }
        }
        BlockNode::DefinitionList(d) => {
            for item in &mut d.items {
                for term in &mut item.terms {
                    coalesce_inlines(&mut term.children);
                }
                for definition in &mut item.definitions {
                    for child in &mut definition.children {
                        coalesce_block(child);
                    }
                }
            }
        }
        BlockNode::Table(t) => {
            if let Some(caption) = &mut t.caption {
                coalesce_inlines(caption);
            }
            for row in &mut t.rows {
                for cell in &mut row.cells {
                    coalesce_inlines(&mut cell.children);
                }
            }
        }
        BlockNode::ExtensionCarrier(e) => {
            // Inline content does not only live in `children`: a `before_render`
            // rewrite stashes a parsed title in `summary`, and an extension can
            // wrap already-parsed blocks. A walk that stops at the carrier node
            // leaves both uncoalesced.
            if let Some(summary) = &mut e.summary {
                coalesce_inlines(summary);
            }
            for child in &mut e.children {
                coalesce_block(child);
            }
        }
        BlockNode::Figure(f) => {
            coalesce_inlines(&mut f.caption);
            match &mut *f.target {
                FigureTarget::BlockQuote(b) => {
                    for child in &mut b.children {
                        coalesce_block(child);
                    }
                }
                FigureTarget::Table(t) => {
                    if let Some(caption) = &mut t.caption {
                        coalesce_inlines(caption);
                    }
                    for row in &mut t.rows {
                        for cell in &mut row.cells {
                            coalesce_inlines(&mut cell.children);
                        }
                    }
                }
                FigureTarget::Paragraph(p) => coalesce_inlines(&mut p.children),
                FigureTarget::Image(_) | FigureTarget::CodeBlock(_) => {}
            }
        }
        _ => {}
    }
}

pub(in crate::parse) fn coalesce_inlines(nodes: &mut Vec<InlineNode>) {
    for node in nodes.iter_mut() {
        match node {
            InlineNode::Emphasis(n) => coalesce_inlines(&mut n.children),
            InlineNode::Link(n) => coalesce_inlines(&mut n.children),
            InlineNode::Span(n) => coalesce_inlines(&mut n.children),
            InlineNode::Extension(n) => coalesce_inlines(&mut n.children),
            InlineNode::CriticInsert(n) => coalesce_inlines(&mut n.children),
            InlineNode::CriticDelete(n) => coalesce_inlines(&mut n.children),
            InlineNode::CriticSubstitute(n) => {
                coalesce_inlines(&mut n.old);
                coalesce_inlines(&mut n.new);
            }
            InlineNode::Footnote(n) => {
                if let Some(inline) = &mut n.inline {
                    coalesce_inlines(inline);
                }
            }
            // A citation item carries THREE inline arrays beside `children`:
            // `prefix`, `locator` and `suffix`. `[see [missing][nope] @a]`
            // publishes a prefix of two adjacent text nodes without this.
            InlineNode::CitationGroup(group) => {
                for item in &mut group.items {
                    for field in [&mut item.prefix, &mut item.locator, &mut item.suffix]
                        .into_iter()
                        .flatten()
                    {
                        coalesce_inlines(field);
                    }
                }
            }
            _ => {}
        }
    }

    if nodes.len() < 2 {
        return;
    }
    nodes.dedup_by(|current, previous| {
        let (InlineNode::Text(current), InlineNode::Text(previous)) = (current, previous) else {
            return false;
        };
        previous.pos = merged_text_pos(previous.pos.clone(), current.pos.clone());
        previous.value.push_str(&current.value);
        true
    });
}

/// The span of two merged text runs, or None when it would not be truthful.
pub(in crate::parse) fn merged_text_pos(left: Option<Pos>, right: Option<Pos>) -> Option<Pos> {
    let (left, right) = (left?, right?);
    if left.end_offset != right.start_offset {
        return None;
    }
    Some(Pos {
        end_line: right.end_line,
        end_column: right.end_column,
        end_offset: right.end_offset,
        ..left
    })
}

/// The span of the text a nested autolink unwraps to.
///
/// A link cannot contain a link, so `[pre <http://h> post](/u)` keeps only the
/// autolink's DISPLAY text - and that text is a sub-slice of what the autolink
/// occupied, not the whole of it. Handing over the autolink's own span would
/// cover the `<` and `>` too, so the span would not select the text it belongs
/// to, which is worse than leaving it unplaced.
///
/// The narrowing is only applied when the arithmetic is unambiguous: the source
/// is either exactly the display text (a bare URL) or the display text inside
/// one delimiter on each side (`<...>`). Anything else - a `mailto:` the author
/// wrote out, an unusual spelling - yields None rather than a guess.
pub(in crate::parse) fn unwrapped_autolink_pos(link: &AutoLink, display: &str) -> Option<Pos> {
    let pos = link.pos.clone()?;
    let width = pos.end_column.checked_sub(pos.start_column)?;
    let shown = display.chars().count();

    if width == shown {
        return Some(pos);
    }
    if width == shown + 2 && pos.start_line == pos.end_line {
        return Some(Pos {
            start_column: pos.start_column + 1,
            end_column: pos.end_column - 1,
            start_offset: pos.start_offset + 1,
            end_offset: pos.end_offset - 1,
            ..pos
        });
    }

    None
}

/// "Links never nest" is a RENDERING rule that binds the renderer and not the
/// encoder (PART 12 section 3a, markup-carve/carve#817), so the tree keeps the
/// link or autolink the author wrote inside a label and every renderer unwraps
/// it here.
pub(crate) fn unwrap_nested_anchors(children: &[InlineNode]) -> std::borrow::Cow<'_, [InlineNode]> {
    if holds_nested_anchor(children) {
        std::borrow::Cow::Owned(enforce_no_nesting_inline(children.to_vec(), true))
    } else {
        std::borrow::Cow::Borrowed(children)
    }
}

/// The ONE spelling of "this link never resolved". Its readers shadow each
/// other: the fast path below and the fold itself short-circuit in sequence, so
/// with the predicate written twice, flipping EITHER copy alone changes no
/// output and a mutation on either comes back green while the pair is
/// load-bearing. The footnote numbering pass reads it for the same reason - it
/// has to reach the same answer `render_link` does about the same node, and two
/// spellings of that are two things to keep in step (PART 9R R2).
pub(crate) fn is_unresolved_reference(link: &Link) -> bool {
    link.ref_label.is_some() && link.href.is_empty() && !link.resolved_empty_reference
}

pub(in crate::parse) fn holds_nested_anchor(nodes: &[InlineNode]) -> bool {
    nodes.iter().any(|node| match node {
        InlineNode::AutoLink(_) => true,
        InlineNode::Link(link) => {
            !is_unresolved_reference(link) || holds_nested_anchor(&link.children)
        }
        InlineNode::Emphasis(emphasis) => holds_nested_anchor(&emphasis.children),
        InlineNode::Span(span) => holds_nested_anchor(&span.children),
        InlineNode::Ruby(r) => r
            .pairs
            .iter()
            .any(|pair| holds_nested_anchor(&pair.base) || holds_nested_anchor(&pair.annotation)),
        InlineNode::Extension(ext) => holds_nested_anchor(&ext.children),
        InlineNode::CriticInsert(critic) => holds_nested_anchor(&critic.children),
        InlineNode::CriticDelete(critic) => holds_nested_anchor(&critic.children),
        InlineNode::CriticSubstitute(critic) => {
            holds_nested_anchor(&critic.old) || holds_nested_anchor(&critic.new)
        }
        InlineNode::Text(_)
        | InlineNode::EscapedText(_)
        | InlineNode::SmartPunctuation(_)
        | InlineNode::Code(_)
        | InlineNode::Image(_)
        | InlineNode::Math(_)
        | InlineNode::RawInline(_)
        | InlineNode::LiteralInline(_)
        | InlineNode::Symbol(_)
        | InlineNode::CrossRef(_)
        | InlineNode::CaptionNumber(_)
        | InlineNode::Mention(_)
        | InlineNode::Tag(_)
        | InlineNode::CitationGroup(_)
        | InlineNode::Abbreviation(_)
        | InlineNode::Footnote(_)
        | InlineNode::NonBreakingSpace(_)
        | InlineNode::SoftBreak(_)
        | InlineNode::CellWrap(_)
        | InlineNode::HardBreak(_)
        | InlineNode::CriticComment(_)
        | InlineNode::Comment(_) => false,
    })
}

pub(in crate::parse) fn enforce_no_nesting_inline(
    nodes: Vec<InlineNode>,
    inside_link: bool,
) -> Vec<InlineNode> {
    let mut out = Vec::with_capacity(nodes.len());
    for node in nodes {
        match node {
            InlineNode::Link(mut link) => {
                let unresolved = is_unresolved_reference(&link);
                let children = enforce_no_nesting_inline(link.children, true);
                if inside_link && !unresolved {
                    // A nested link is dropped; only its (cleaned) text remains
                    // because the outermost destination already applies.
                    out.extend(children);
                } else {
                    link.children = children;
                    out.push(InlineNode::Link(link));
                }
            }
            InlineNode::AutoLink(a) => {
                if inside_link {
                    let display = a.text.clone();
                    out.push(InlineNode::Text(Text {
                        pos: unwrapped_autolink_pos(&a, &display),
                        value: display,
                    }));
                } else {
                    out.push(InlineNode::AutoLink(a));
                }
            }
            InlineNode::Footnote(mut f) => {
                // A footnote body renders outside the anchor, so its links are
                // not nested: re-enter with inside_link = false.
                if let Some(inline) = f.inline.take() {
                    f.inline = Some(enforce_no_nesting_inline(inline, false));
                }
                out.push(InlineNode::Footnote(f));
            }
            InlineNode::Emphasis(mut e) => {
                e.children = enforce_no_nesting_inline(e.children, inside_link);
                out.push(InlineNode::Emphasis(e));
            }
            InlineNode::Span(mut s) => {
                s.children = enforce_no_nesting_inline(s.children, inside_link);
                out.push(InlineNode::Span(s));
            }
            InlineNode::Ruby(mut r) => {
                for pair in &mut r.pairs {
                    pair.base =
                        enforce_no_nesting_inline(std::mem::take(&mut pair.base), inside_link);
                    pair.annotation = enforce_no_nesting_inline(
                        std::mem::take(&mut pair.annotation),
                        inside_link,
                    );
                    if pair.base.is_empty() {
                        pair.base.push(InlineNode::text(""));
                    }
                }
                out.push(InlineNode::Ruby(r));
            }
            InlineNode::Extension(mut ext) => {
                ext.children = enforce_no_nesting_inline(ext.children, inside_link);
                out.push(InlineNode::Extension(ext));
            }
            InlineNode::CriticInsert(mut c) => {
                c.children = enforce_no_nesting_inline(c.children, inside_link);
                out.push(InlineNode::CriticInsert(c));
            }
            InlineNode::CriticDelete(mut c) => {
                c.children = enforce_no_nesting_inline(c.children, inside_link);
                out.push(InlineNode::CriticDelete(c));
            }
            InlineNode::CriticSubstitute(mut c) => {
                c.old = enforce_no_nesting_inline(c.old, inside_link);
                c.new = enforce_no_nesting_inline(c.new, inside_link);
                out.push(InlineNode::CriticSubstitute(c));
            }
            other => out.push(other),
        }
    }
    out
}

pub(in crate::parse) fn plain_inlines_parse(nodes: &[InlineNode]) -> String {
    let mut out = String::new();
    for node in nodes {
        match node {
            InlineNode::NonBreakingSpace(_) => out.push('\u{00a0}'),
            InlineNode::Text(s) => out.push_str(&s.value),
            // An escaped character is VISIBLE prose - `\*` renders as `*` - so it
            // contributes to every text derived from this run, exactly as the
            // text around it does. Without this arm the `_ => {}` below swallowed
            // it, and a heading the author escaped got a title, an id and a
            // PART 9R R1 `by_text` key with the escaped characters missing
            // (carve-rs#800). carve-js `inlineText` and carve-php
            // `inlineTextLeaf` both carry the same arm.
            InlineNode::EscapedText(e) => out.push_str(&e.value),
            InlineNode::SmartPunctuation(s) => out.push_str(smart_punctuation_glyph(s)),
            InlineNode::Emphasis(e) => out.push_str(&plain_inlines_parse(&e.children)),
            InlineNode::Span(s) if !s.injected => out.push_str(&plain_inlines_parse(&s.children)),
            InlineNode::Code(s) => out.push_str(&s.value),
            // An inline literal renders as visible prose (§27), so it feeds the
            // parse-time cross-reference slug like a code span does.
            InlineNode::LiteralInline(l) => out.push_str(&l.content),
            // Math is verbatim text the reader sees, so it feeds the parse-time
            // slug exactly as a code span does. MEASURED: this arm feeds PART 9R
            // R1's `by_text` index, and `plain_inlines_typography` is the one the
            // rendered id derives through. Without this one a heading published
            // `id="a-x-b"` while `[a $`x` b][]` still resolved to `a-b`, linking
            // to an anchor no element carried (carve#1283).
            InlineNode::Math(m) => out.push_str(&m.content),
            InlineNode::Link(l) => out.push_str(&plain_inlines_parse(&l.children)),
            InlineNode::AutoLink(a) => out.push_str(&a.text),
            InlineNode::Image(i) => out.push_str(&i.alt),
            InlineNode::Extension(e) => out.push_str(&plain_inlines_parse(&e.children)),
            InlineNode::CriticInsert(c) => out.push_str(&plain_inlines_parse(&c.children)),
            InlineNode::CriticDelete(c) => out.push_str(&plain_inlines_parse(&c.children)),
            InlineNode::CriticSubstitute(c) => {
                out.push_str(&plain_inlines_parse(&c.old));
                out.push_str(&plain_inlines_parse(&c.new));
            }
            InlineNode::CitationGroup(g) => out.push_str(&g.raw),
            InlineNode::Abbreviation(a) => out.push_str(&a.abbr),
            InlineNode::Mention(m) => {
                out.push('@');
                out.push_str(&m.user);
            }
            InlineNode::Tag(t) => {
                out.push('#');
                out.push_str(&t.name);
            }
            InlineNode::CaptionNumber(n) => {
                if let Some(number) = n.number {
                    out.push_str(&number.to_string());
                }
            }
            // A soft/hard break is a word separator, so parse-time
            // cross-reference slugs match the rendered heading id. (A heading
            // cannot hold one from a parse any more -- headings end at the
            // newline -- but an ingested AST can carry any inline, PART 12.)
            InlineNode::SoftBreak(_) | InlineNode::HardBreak(_) => out.push(' '),
            _ => {}
        }
    }
    out
}

/// Carve "Automatic Identifiers" slug (spec #73). The single canonical
/// implementation, shared by the HTML and Markdown renderers so all id
/// derivation in carve-rs stays byte-identical (and identical to carve-js /
/// carve-php).
/// Reverse smart-typography substitutions to their ASCII source, so a heading
/// id never depends on presentational typography. The inverse of the parser's
/// smart tokens plus smart quotes and dashes; the recovered ASCII punctuation
/// then collapses in the slug run. Kept byte-identical to carve-js / carve-php.
pub(in crate::parse) fn de_typography(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '↔' => out.push_str("<-->"),
            '™' => out.push_str("(tm)"),
            '…' => out.push_str("..."),
            '→' => out.push_str("-->"),
            '←' => out.push_str("<--"),
            // The CANONICAL spellings: `=>` no longer parses as an arrow, so
            // recovering it would produce ASCII that does not round-trip.
            '⇒' => out.push_str("==>"),
            '⇐' => out.push_str("<=="),
            '⇔' => out.push_str("<=>"),
            '≤' => out.push_str("<="),
            '≥' => out.push_str(">="),
            '≠' => out.push_str("!="),
            '±' => out.push_str("+-"),
            '©' => out.push_str("(c)"),
            '®' => out.push_str("(r)"),
            '–' | '—' => out.push('-'),
            '‘' | '’' | '‚' | '‛' => out.push('\''),
            '“' | '”' | '„' | '‟' | '«' | '»' => out.push('"'),
            other => out.push(other),
        }
    }
    out
}

/// Code points removed from a heading-id source before slugging: the
/// bidi-override / isolate controls (also stripped from rendered text, see
/// `escape::is_bidi_control`) plus the zero-width characters that are NOT
/// stripped from text but must never leak into an `id="..."`.
pub(in crate::parse) fn is_id_strippable(c: char) -> bool {
    matches!(
        c,
        '\u{202A}'..='\u{202E}'   // bidi LRE/RLE/PDF/LRO/RLO
        | '\u{2066}'..='\u{2069}' // bidi isolates LRI/RLI/FSI/PDI
        | '\u{200B}'              // zero-width space
        | '\u{200C}'              // zero-width non-joiner
        | '\u{200D}'              // zero-width joiner
        | '\u{2060}'              // word joiner
        | '\u{FEFF}'              // zero-width no-break space / BOM
        | '\u{00AD}'              // soft hyphen
    )
}

/// NFC-normalize, then drop the invisible/dangerous controls (see
/// `is_id_strippable`). The pre-slug transform that makes a generated id
/// deterministic and Trojan-Source-safe (corpus 117). Parity with carve-js
/// Give every heading the id the renderer will assign it, where the source
/// wrote none.
///
/// The id takes no `order` slot: `order` is the source-appearance order of the
/// slots in a `{#id .class key=value}` block, and a slugged id never appeared in
/// one. `render_carve` reads that back - an id with no slot, that a fresh parse
/// would re-derive, is not written into the source it produces.
pub(in crate::parse) fn stamp_generated_heading_ids(doc: &mut Document, id_opts: HeadingIdOptions) {
    let ids = crate::document_ids::assigned_heading_ids(doc, id_opts);
    stamp_generated_heading_ids_from(doc, ids);
}

pub(in crate::parse) fn stamp_generated_heading_ids_from(doc: &mut Document, ids: Vec<String>) {
    if ids.is_empty() {
        return;
    }
    let mut next = ids.into_iter();
    stamp_heading_ids_in(&mut doc.children, &mut next);
    let keys: Vec<String> = doc.footnote_defs.keys().cloned().collect();
    for key in keys {
        if let Some(blocks) = doc.footnote_defs.get_mut(&key) {
            stamp_heading_ids_in(blocks, &mut next);
        }
    }
}

pub(in crate::parse) fn stamp_heading_ids_in(
    blocks: &mut [BlockNode],
    next: &mut impl Iterator<Item = String>,
) {
    for block in blocks.iter_mut() {
        match block {
            BlockNode::Heading(h) => {
                let Some(id) = next.next() else {
                    return;
                };
                if h.attrs.as_ref().and_then(|a| a.id.as_ref()).is_none() {
                    h.attrs.get_or_insert_with(Attrs::default).id = Some(id);
                }
            }
            BlockNode::BlockQuote(b) => stamp_heading_ids_in(&mut b.children, next),
            BlockNode::Directive(d) => stamp_heading_ids_in(&mut d.children, next),
            BlockNode::Div(d) => stamp_heading_ids_in(&mut d.children, next),
            BlockNode::Admonition(a) => stamp_heading_ids_in(&mut a.children, next),
            BlockNode::FigureGroup(g) => stamp_heading_ids_in(&mut g.children, next),
            BlockNode::List(l) => {
                for item in l.items.iter_mut() {
                    stamp_heading_ids_in(&mut item.children, next);
                }
            }
            BlockNode::Figure(f) => {
                if let FigureTarget::BlockQuote(b) = &mut *f.target {
                    stamp_heading_ids_in(&mut b.children, next);
                }
            }
            BlockNode::DefinitionList(dl) => {
                for entry in dl.items.iter_mut() {
                    for definition in entry.definitions.iter_mut() {
                        stamp_heading_ids_in(&mut definition.children, next);
                    }
                }
            }
            _ => {}
        }
    }
}

/// `sanitizeIdSource`.
pub(in crate::parse) fn sanitize_id_source(text: &str) -> String {
    text.nfc().filter(|c| !is_id_strippable(*c)).collect()
}

pub(crate) fn slugify_parse(text: &str, opts: HeadingIdOptions) -> String {
    let ascii = text.is_ascii();
    // ASCII needs no normalization, invisible-control stripping, or glyph conversion.
    let mut out = if ascii {
        slug_run(text, false)
    } else {
        let sanitized = sanitize_id_source(text);
        let detyped = de_typography(&sanitized);
        slug_run(&detyped, false)
    };
    if !ascii {
        match opts.ascii {
            AsciiHeadingIds::Off => {}
            mode => {
                let mut folded = String::with_capacity(out.len());
                for ch in out.chars() {
                    match crate::translit_map::translit(ch) {
                        Some(ascii) => folded.push_str(ascii),
                        None => folded.push(ch),
                    }
                }
                out = slug_run(&folded, mode == AsciiHeadingIds::Strict);
            }
        }
    }
    if opts.lowercase {
        if ascii {
            out.make_ascii_lowercase();
        } else {
            out = out.chars().flat_map(char::to_lowercase).collect::<String>();
        }
    }
    // A leading Unicode number (\p{N}: Nd/Nl/No) is a valid HTML id but not a
    // bare CSS selector, so prefix 's-'. Empty -> 's'. Matches carve-js/php.
    if out.chars().next().is_some_and(char::is_numeric) {
        out = format!("s-{out}");
    }
    if out.is_empty() {
        "s".to_string()
    } else {
        out
    }
}

/// One slug run: keep what the rule keeps, collapse every other maximal run to
/// a single `-`, and trim.
///
/// `ascii_only` is the STRICT variant: a non-ASCII code point is a separator
/// rather than a kept character, so residue the transliteration table could not
/// map becomes a hyphen instead of surviving. Both variants are needed because
/// the fold runs this twice - once over the source, once over the table's
/// output - and only the second run may narrow what counts as a character.
pub(in crate::parse) fn slug_run(text: &str, ascii_only: bool) -> String {
    let mut out = String::with_capacity(text.len());
    let mut last_dash = false;
    for ch in text.chars() {
        let kept = ch.is_ascii_alphanumeric() || (!ascii_only && ch as u32 >= 0x80);
        if kept {
            out.push(ch);
            last_dash = false;
        } else if !last_dash && !out.is_empty() {
            out.push('-');
            last_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    out
}

/// Forced intraword emphasis `{X…X}` (spec §22): emits the same node as the bare
/// delimiter X, but with no word-boundary condition. X is one of `/ * _ ^ , ~ =`.
/// The closing `X}` is the first one after at least one content byte, mirroring
/// the non-greedy `^\{(X)([\s\S]+?)\1\}` match. `{=html}` (no trailing `=`) does
/// not match, so raw-format attribute blocks are unaffected.
pub(in crate::parse) fn parse_forced_emphasis(
    bytes: &[u8],
    i: usize,
    options: &Options<'_>,
    in_footnote: bool,
    bounds: &InlineBounds<'_>,
    positions: Option<&InlinePositionMap<'_>>,
    base: usize,
) -> Option<(InlineNode, usize)> {
    let delim = bytes.get(i + 1).copied()?;
    let kind = match delim {
        b'/' => EmphasisKind::Italic,
        b'*' => EmphasisKind::Strong,
        b'_' => EmphasisKind::Underline,
        b'^' => EmphasisKind::Super,
        b',' => EmphasisKind::Sub,
        b'~' => EmphasisKind::Strike,
        b'=' => EmphasisKind::Highlight,
        _ => return None,
    };
    // The span closes on a `delim}` pair; with no such pair ahead the scan could
    // only walk to end-of-text and fail, so bail in O(1) (keeps `{/`×n linear).
    if !bounds.has_delim_brace_from(delim, i) {
        return None;
    }
    let content_start = i + 2;
    if followed_by_grapheme_extender(bytes, content_start) {
        return None;
    }
    let word_close = if options.extensions.is_empty() {
        let end = bytes[content_start..]
            .iter()
            .take(65)
            .take_while(|byte| byte.is_ascii_alphanumeric())
            .count();
        (end > 0
            && end <= 64
            && bytes.get(content_start + end) == Some(&delim)
            && bytes.get(content_start + end + 1) == Some(&b'}'))
        .then_some(content_start + end)
    } else {
        None
    };
    let j = word_close.or_else(|| bounds.braced_closers(bytes).close(bytes, i, delim))?;
    if j == content_start {
        return None; // empty content: `+?` requires at least one byte
    }
    // The closer has to sit in the opener's bracket run. PART 8 resolves a run
    // before an emphasis marker, so a `delim}` inside one is that run's content
    // and cannot close a span opened outside it (markup-carve/carve#2577,
    // carve-rs#2173). The bare and the combined forms read the same rule.
    if !braced_closer_shares_the_run(bounds, i, j) {
        return None;
    }
    if NESTING_DEPTH.with(Cell::get) >= MAX_NESTING_DEPTH {
        return Some((
            InlineNode::text(std::str::from_utf8(&bytes[i..j + 2]).ok()?.to_string()),
            j + 2 - i,
        ));
    }
    let inner = std::str::from_utf8(&bytes[content_start..j]).ok()?;
    let inherited = inner
        .as_bytes()
        .windows(2)
        .any(|pair| {
            pair[0] == b'{' && matches!(pair[1], b'/' | b'*' | b'_' | b'^' | b',' | b'~' | b'=')
        })
        .then(|| {
            bounds
                .braced_closers(bytes)
                .view(bytes, content_start, j - content_start)
        });
    // A braced inline starts its own scope (ruling markup-carve/carve#2091).
    OpenKinds::pass_only(OpenKinds::bit(delim));
    Some((
        InlineNode::Emphasis(Emphasis {
            attrs: None,
            kind,
            children: parse_inline_context_with_pairs(
                inner,
                options,
                false,
                in_footnote,
                positions,
                base + content_start,
                inherited,
            ),
            pos: None,
        }),
        j + 2 - i,
    ))
}

/// The `~}` and the top-level `~>` of the substitution opening at `open`.
///
/// Only an arrow outside verbatim content and outside a comment splits the pair
/// (ruling B on markup-carve/carve#2083), and an escaped one never does. A pair
/// with no such arrow is a forced strike.
pub(in crate::parse) fn substitution_at(
    bytes: &[u8],
    open: usize,
    bounds: &InlineBounds<'_>,
) -> Option<(usize, usize)> {
    bounds.braced_closers(bytes).substitution(bytes, open)
}

thread_local! {
    static IN_LINE_BLOCK: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Whether the inline run being read is a line block's, where a line break is
/// content rather than layout (PART 9 §23).
pub(in crate::parse) struct InLineBlock {
    outer: bool,
}

impl InLineBlock {
    pub(in crate::parse) fn inside() -> bool {
        IN_LINE_BLOCK.with(|flag| flag.get())
    }

    pub(in crate::parse) fn enter() -> Self {
        Self {
            outer: IN_LINE_BLOCK.with(|flag| flag.replace(true)),
        }
    }
}

impl Drop for InLineBlock {
    fn drop(&mut self) {
        IN_LINE_BLOCK.with(|flag| flag.set(self.outer));
    }
}

thread_local! {
    static OPEN_KINDS: std::cell::Cell<u8> = const { std::cell::Cell::new(0) };
    static PASSED_KINDS: std::cell::Cell<u8> = const { std::cell::Cell::new(0) };
}

/// The emphasis kinds open around the inline run being scanned (PART 9 §9 E3).
///
/// An emphasis arm passes its kinds on to the `parse_inline_context` call that
/// reads its content; every other call starts from none, so a link label or a
/// table cell opens every kind again.
pub(in crate::parse) struct OpenKinds {
    outer: u8,
}

impl OpenKinds {
    pub(in crate::parse) fn bit(delim: u8) -> u8 {
        match delim {
            b'/' => 1,
            b'*' => 2,
            b'_' => 4,
            b'^' => 8,
            b',' => 16,
            b'~' => 32,
            b'=' => 64,
            _ => 0,
        }
    }

    pub(in crate::parse) fn is_open(delim: u8) -> bool {
        OPEN_KINDS.with(|kinds| kinds.get() & Self::bit(delim) != 0)
    }

    /// Set while the run is combined `/*` content, nested bare spans included:
    /// there a `%%` comment stops before a `*/` (`biSpComment`).
    pub(in crate::parse) const COMBINED: u8 = 128;

    /// Whether the run is the content of an emphasis span.
    pub(in crate::parse) fn any_open() -> bool {
        OPEN_KINDS.with(|kinds| kinds.get() != 0)
    }

    pub(in crate::parse) fn in_combined() -> bool {
        OPEN_KINDS.with(|kinds| kinds.get() & Self::COMBINED != 0)
    }

    /// Hand the open kinds plus `added` to the next `parse_inline_context`.
    pub(in crate::parse) fn pass_on(added: u8) {
        let open = OPEN_KINDS.with(|kinds| kinds.get());
        PASSED_KINDS.with(|passed| passed.set(open | added));
    }

    /// Hand only `kinds` to the next `parse_inline_context`.
    fn pass_only(kinds: u8) {
        PASSED_KINDS.with(|passed| passed.set(kinds));
    }

    pub(in crate::parse) fn enter() -> Self {
        let passed = PASSED_KINDS.with(|passed| passed.replace(0));
        Self {
            outer: OPEN_KINDS.with(|kinds| kinds.replace(passed)),
        }
    }
}

impl Drop for OpenKinds {
    fn drop(&mut self) {
        OPEN_KINDS.with(|kinds| kinds.set(self.outer));
    }
}

pub(in crate::parse) fn find_seq(bytes: &[u8], from: usize, marker: &[u8]) -> Option<usize> {
    if marker.is_empty() || from + marker.len() > bytes.len() {
        return None;
    }
    let mut j = from;
    while j + marker.len() <= bytes.len() {
        if &bytes[j..j + marker.len()] == marker {
            return Some(j);
        }
        j += 1;
    }
    None
}

/// Number of distinct single-char emphasis delimiters (`/ * _ ~ =`), plus one
/// catch-all slot and one for the combined token. Sizes the
/// per-`parse_inline_context` no-close memo.
pub(in crate::parse) const EMPHASIS_DELIM_SLOTS: usize = 7;

/// The closer kind of the combined `/*` token: `*/`. No UTF-8 byte is 0xFF,
/// so it never equals a byte the walk reads.
pub(in crate::parse) const COMBINED_CLOSER: u8 = 0xFF;

#[inline]
pub(in crate::parse) fn emphasis_delim_index(delim: u8) -> usize {
    match delim {
        b'/' => 0,
        b'*' => 1,
        b'_' => 2,
        b'~' => 3,
        b'=' => 4,
        COMBINED_CLOSER => 6,
        _ => 5,
    }
}

/// Per-text memo for the bare closing scan.
#[derive(Default)]
pub(in crate::parse) struct EmphasisMemo {
    /// Per delimiter, the scan positions known to reach no closer. A later scan
    /// stepping onto one fails too, since the scan's only state is its position;
    /// an opener inside a skipped plain brace group is not a suffix scan.
    failed: [Option<Vec<bool>>; EMPHASIS_DELIM_SLOTS],
    last_brace: Option<Option<usize>>,
    last_comment_close: Option<Option<usize>>,
    pub(in crate::parse) atomic: Option<std::rc::Rc<[(usize, usize)]>>,
}

/// Whether the `*/` at `at` can close the combined token (`biCloser`).
pub(in crate::parse) fn combined_closer_at(bytes: &[u8], at: usize) -> bool {
    !followed_by_grapheme_extender(bytes, at + 2)
        && !bytes.get(at + 2).is_some_and(u8::is_ascii_alphanumeric)
}

/// The `*/` closing the combined `/*` token whose content starts at `start`.
pub(in crate::parse) fn bold_italic_close(
    bytes: &[u8],
    start: usize,
    memo: &mut EmphasisMemo,
    bounds: &InlineBounds<'_>,
) -> Option<usize> {
    // The content is never empty, so a `*/` right at `start` is content. A
    // `*` opens nothing the walk skips, so starting past it is safe.
    let from = start + usize::from(bytes.get(start..start + 2) == Some(b"*/"));
    cached_find_emphasis_close(bytes, from, COMBINED_CLOSER, memo, bounds)
}

/// `find_emphasis_close` behind the failure memo, which bounds `_a](`×n and
/// `*a](`×n at O(n) while keeping output byte-identical.
pub(in crate::parse) fn cached_find_emphasis_close(
    bytes: &[u8],
    from: usize,
    delim: u8,
    memo: &mut EmphasisMemo,
    bounds: &InlineBounds<'_>,
) -> Option<usize> {
    let idx = emphasis_delim_index(delim);
    if memo.failed[idx].as_ref().is_some_and(|f| f[from]) {
        return None;
    }
    let mut failed = memo.failed[idx].take();
    let mut visited = Vec::new();
    let close = find_emphasis_close(
        bytes,
        from,
        delim,
        memo,
        bounds,
        failed.as_deref(),
        &mut visited,
    );
    if close.is_none() {
        let regions = memo.atomic.as_deref().unwrap_or(&[]);
        let opener_run = emphasis_run_at(bounds, regions, from - 1);
        let marks = failed.get_or_insert_with(|| vec![false; bytes.len() + 1]);
        for j in visited {
            if emphasis_run_at(bounds, regions, j) == opener_run {
                marks[j] = true;
            }
        }
    }
    memo.failed[idx] = failed;
    close
}

/// Autolinks and cross-references outside code spans, as `[start, end)` pairs.
/// PART 8 resolves them before links, so a `[` inside one opens no run.
pub(in crate::parse) fn atomic_link_regions(
    bytes: &[u8],
    bounds: &InlineBounds<'_>,
) -> Vec<(usize, usize)> {
    let mut regions = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        match bytes[at] {
            b'\\' => at += 2,
            b'`' => {
                let open_start = at;
                while at < bytes.len() && bytes[at] == b'`' {
                    at += 1;
                }
                let open_len = at - open_start;
                let mut closed = false;
                while at < bytes.len() {
                    if bytes[at] != b'`' {
                        at += 1;
                        continue;
                    }
                    let close_start = at;
                    while at < bytes.len() && bytes[at] == b'`' {
                        at += 1;
                    }
                    if at - close_start == open_len {
                        closed = true;
                        break;
                    }
                }
                if !closed {
                    break;
                }
            }
            b'<' if bounds.has_gt_from(at) => {
                let end =
                    crossref_close(bytes, at, bounds).or_else(|| scanned_autolink_end(bytes, at));
                match end {
                    Some(end) => {
                        regions.push((at, end + 1));
                        at = end + 1;
                    }
                    None => at += 1,
                }
            }
            _ => at += 1,
        }
    }
    regions
}

pub(in crate::parse) fn in_atomic_region(regions: &[(usize, usize)], pos: usize) -> bool {
    let i = regions.partition_point(|&(_, end)| end <= pos);
    regions.get(i).is_some_and(|&(start, _)| start < pos)
}

/// The bracket run holding `pos` for emphasis pairing: a run opened inside an
/// autolink or cross-reference does not count, so its enclosing run answers.
pub(in crate::parse) fn emphasis_run_at(
    bounds: &InlineBounds<'_>,
    regions: &[(usize, usize)],
    pos: usize,
) -> Option<usize> {
    let mut run = bounds.bracket_run_at(pos);
    while let Some(open) = run {
        if !in_atomic_region(regions, open) {
            break;
        }
        run = open
            .checked_sub(1)
            .and_then(|before| bounds.bracket_run_at(before));
        if run == Some(open) {
            return None;
        }
    }
    run
}

/// Whether the `_` or `/` before the opener at `at` is an open entry where
/// that opener stands, so it does not guard it (carve#3973 X1). At `from` it is
/// the walk's own opener.
fn guard_stands(bytes: &[u8], at: usize, from: usize, delim: u8) -> bool {
    if at == from {
        return true;
    }
    let Some(guard) = at.checked_sub(1) else {
        return false;
    };
    let d = bytes[guard];
    let before = guard.checked_sub(1).map(|b| bytes[b]);
    if d == delim || OpenKinds::is_open(d) || before == Some(d) || before == Some(b'\\') {
        return false;
    }
    if before.is_some_and(|b| b.is_ascii_alphanumeric())
        || bytes.get(at).map_or(true, |&b| is_carve_ws(b))
    {
        return false;
    }
    if (before == Some(b'_') || (before == Some(b'/') && matches!(d, b'/' | b'_')))
        && !guard_stands(bytes, guard, from, delim)
    {
        return false;
    }
    true
}

/// Whether the bare delimiter at `at` opens: 0 no, 1 yes, 2 only if the `_`
/// or `/` before it is text.
fn bare_opener_state(bytes: &[u8], at: usize) -> u8 {
    let ch = bytes[at];
    let Some(&next) = bytes.get(at + 1) else {
        return 0;
    };
    let before = at.checked_sub(1).map(|b| bytes[b]);
    if is_carve_ws(next) || next == ch || before == Some(ch) || (ch == b'=' && next == b'>') {
        return 0;
    }
    match before {
        None => 1,
        Some(b'_') => 2,
        Some(b'/') if matches!(ch, b'/' | b'_') => 2,
        Some(b) if b.is_ascii_alphanumeric() => 0,
        _ => 1,
    }
}

/// Whether `pos` sits in a bracket run of this inline run. A kind open
/// outside the run is not reachable inside it for E3 (carve#3897).
pub(in crate::parse) fn in_bracket_run(
    bytes: &[u8],
    pos: usize,
    memo: &mut EmphasisMemo,
    bounds: &InlineBounds<'_>,
) -> bool {
    bounds.bracket_run_at(pos).is_some()
        && emphasis_run_at(
            bounds,
            memo.atomic
                .get_or_insert_with(|| atomic_link_regions(bytes, bounds).into()),
            pos,
        )
        .is_some()
}

pub(in crate::parse) fn find_emphasis_close(
    bytes: &[u8],
    from: usize,
    delim: u8,
    memo: &mut EmphasisMemo,
    bounds: &InlineBounds<'_>,
    failed: Option<&[bool]>,
    visited: &mut Vec<usize>,
) -> Option<usize> {
    find_emphasis_close_walk(bytes, from, delim, memo, bounds, failed, visited)
}

#[allow(clippy::too_many_arguments)]
fn find_emphasis_close_walk(
    bytes: &[u8],
    from: usize,
    delim: u8,
    memo: &mut EmphasisMemo,
    bounds: &InlineBounds<'_>,
    failed: Option<&[bool]>,
    visited: &mut Vec<usize>,
) -> Option<usize> {
    // The run the OPENER sits in, read at the byte before `from`: the
    // delimiter, or a `*` of the combined token. `from` itself may be a `]`.
    let regions = memo
        .atomic
        .get_or_insert_with(|| atomic_link_regions(bytes, bounds).into())
        .clone();
    let opener_run = emphasis_run_at(bounds, &regions, from - 1);
    let opener_end = opener_run.map(|open| bounds.matches[open]);
    let mut attached = HashMap::new();
    // A guard `_` or `/` that opens a span is markup, so the opener after it
    // starts at START: `from`, then each guard opener chained behind it.
    let mut start = from;
    // The opener behind a guard that opened mid-walk; its pair is a guarded one.
    let mut guarded_at = None;
    // The last opener of each other kind the walk has passed.
    let mut last_opener: [Option<usize>; 5] = [None; 5];
    let mut j = from;
    while j < bytes.len() {
        if opener_end.is_some_and(|end| j > end) {
            return None;
        }
        if failed.is_some_and(|f| f[j]) && emphasis_run_at(bounds, &regions, j) == opener_run {
            return None;
        }
        visited.push(j);
        let ch = bytes[j];
        // The closer of a pair whose block is already attached opens nothing.
        if matches!(ch, b'/' | b'*' | b'_' | b'~' | b'=')
            && ch != delim
            && !attached.contains_key(&(j + 1))
            && !(delim == COMBINED_CLOSER && matches!(ch, b'/' | b'*'))
            && (!OpenKinds::is_open(ch) || emphasis_run_at(bounds, &regions, j).is_some())
        {
            let guarded = (j == start || guarded_at == Some(j))
                && j > 0
                && matches!(bytes[j - 1], b'/' | b'_');
            let pair = if guarded {
                bounds
                    .braced_closers(bytes)
                    .bare_attributes_at_start(bytes, j)
            } else {
                bounds.braced_closers(bytes).bare_attributes(bytes, j)
            };
            // A span of another kind open at this opener that closes inside
            // its pair demotes the opener (E2), so the block is content. An
            // opener the walk's own delimiter guards stays open only if the
            // walk closes without that demotion.
            let pair = pair.filter(|&(close, _)| {
                !last_opener.iter().zip(*b"/*_~=").any(|(&open, d)| {
                    let Some(open) = open else { return false };
                    if d == ch {
                        return false;
                    }
                    if bare_opener_state(bytes, open) != 1
                        && !guard_stands(bytes, open, from, delim)
                    {
                        return false;
                    }
                    cached_find_emphasis_close(bytes, open + 1, d, memo, bounds)
                        .is_some_and(|outer| outer > j && outer < close)
                })
            });
            if let Some((close, end)) = pair {
                attached.insert(close + 1, end);
            }
            // Only an attribute block behind the next opener makes the guard's
            // own closer matter, so the recursion stays off plain text.
            if (j == start
                || guarded_at == Some(j)
                || j.checked_sub(1).is_some_and(|prev| {
                    !(bytes[prev].is_ascii_alphanumeric()
                        || matches!(bytes[prev], b'_' | b'/')
                        || bytes[prev] == ch)
                }))
                && matches!(ch, b'/' | b'_')
                && bytes
                    .get(j + 1)
                    .is_some_and(|&next| !is_carve_ws(next) && next != ch)
                && bounds
                    .braced_closers(bytes)
                    .bare_attributes_at_start(bytes, j + 1)
                    .is_some()
            {
                let opens = guard_stands(bytes, j + 1, from, delim);
                if opens && j == start {
                    start = j + 1;
                } else if opens {
                    guarded_at = Some(j + 1);
                }
            }
        }
        if matches!(ch, b'/' | b'*' | b'_' | b'~' | b'=')
            && ch != delim
            && !OpenKinds::is_open(ch)
            && bare_opener_state(bytes, j) != 0
        {
            last_opener[emphasis_delim_index(ch)] = Some(j);
        }
        if let Some(&end) = attached.get(&j) {
            j = end;
            continue;
        }
        if ch == b']'
            && bounds
                .openers
                .get(j)
                .is_some_and(|&open| open != NO_BRACKET_MATCH && !in_atomic_region(&regions, open))
        {
            let end = bounds.braced_closers(bytes).after_attributes(bytes, j + 1);
            if end > j + 1 {
                j = end;
                continue;
            }
        }
        if ch == b'\\' && j + 1 < bytes.len() {
            j += 2;
            continue;
        }
        if ch == b'`' {
            // Skip a verbatim span: opener run of N backticks closes on a run
            // of exactly N. An unclosed run is opaque to end of text, so no
            // emphasis closer can follow it.
            let open_start = j;
            while j < bytes.len() && bytes[j] == b'`' {
                j += 1;
            }
            let open_len = j - open_start;
            let mut found = false;
            while j < bytes.len() {
                if bytes[j] == b'`' {
                    let close_start = j;
                    while j < bytes.len() && bytes[j] == b'`' {
                        j += 1;
                    }
                    if j - close_start == open_len {
                        found = true;
                        break;
                    }
                } else {
                    j += 1;
                }
            }
            if !found {
                return None;
            }
            j = bounds.braced_closers(bytes).after_attributes(bytes, j);
            continue;
        }
        // E2a: a link destination and an autolink are opaque too. A link
        // LABEL is not, and neither is a plain brace group.
        if ch == b']'
            && bytes.get(j + 1) == Some(&b'(')
            && opens_a_link(bytes, j, bounds)
            && !in_atomic_region(&regions, bounds.openers[j])
        {
            if let Some(end) = link_destination_end(bytes, j + 1) {
                j = bounds
                    .braced_closers(bytes)
                    .after_attributes(bytes, end + 1);
                continue;
            }
        }
        if ch == b'<' && bounds.has_gt_from(j) {
            // A cross-reference ranks with autolinks (PART 8 item 3).
            if let Some(end) = crossref_close(bytes, j, bounds) {
                j = end + 1;
                continue;
            }
            if let Some(end) = scanned_autolink_end(bytes, j) {
                j = bounds
                    .braced_closers(bytes)
                    .after_attributes(bytes, end + 1);
                continue;
            }
        }
        if ch == b'{' && bytes.get(j + 1) == Some(&b'%') && !decorated_braced_opener(bytes, j) {
            let last = *memo
                .last_comment_close
                .get_or_insert_with(|| bytes.windows(2).rposition(|w| w == b"%}"));
            if let Some(end) = last
                .filter(|&l| l >= j + 2)
                .and_then(|_| find_seq(bytes, j + 2, b"%}"))
            {
                j = end + 2;
                continue;
            }
        }
        let run = bounds.bracket_run_at(j);
        if ch == b'%'
            && bytes.get(j + 1) == Some(&b'%')
            && (j == run.map_or(0, |open| open + 1) || matches!(bytes[j - 1], b' ' | b'\n'))
        {
            let run_close = run.map(|open| bounds.matches[open]);
            if (delim == COMBINED_CLOSER || OpenKinds::in_combined())
                && j > 0
                && bytes[j - 1] == b' '
            {
                // Bounded by the candidate, so a long line is not rescanned.
                let close = find_seq(bytes, j + 2, b"*/").filter(|&close| {
                    trailing_comment_end(&bytes[..close], j + 2, run_close)
                        .map_or(true, |end| end >= close)
                });
                if let Some(close) = close {
                    // Comment text is raw, so a bracket in it opens no run.
                    if delim == COMBINED_CLOSER && combined_closer_at(bytes, close) {
                        return Some(close);
                    }
                    // The comment's own space is not one before the `*/`.
                    j = close + usize::from(delim == COMBINED_CLOSER);
                    continue;
                }
            }
            let end = trailing_comment_end(bytes, j + 2, run_close);
            j = end?;
            continue;
        }
        // A raw inline's format block is not a braced highlight: the main
        // loop builds it with the code span in front of it, so only the
        // token is opaque (markup-carve/carve-rs#1652).
        if ch == b'{' && j > 0 && bytes[j - 1] == b'`' {
            if let Some(end) = raw_inline_format_end(bytes, j) {
                j = bounds
                    .braced_closers(bytes)
                    .after_attributes(bytes, end + 1);
                continue;
            }
        }
        if ch == b'{' {
            if let Some(end) = braced_inline_end(bytes, j, memo, delim, bounds) {
                j = bounds
                    .braced_closers(bytes)
                    .after_attributes(bytes, end + 1);
                continue;
            }
        }
        if delim == COMBINED_CLOSER
            && ch == b'*'
            && bytes.get(j + 1) == Some(&b'/')
            && emphasis_run_at(bounds, &regions, j) == opener_run
        {
            // A space or tab before a `*/` ends the token unclosed
            // (`spaceChar ~("*/")`); a line break does not.
            if matches!(bytes[j - 1], b' ' | b'\t') {
                return None;
            }
            if combined_closer_at(bytes, j) {
                return Some(j);
            }
        }
        if ch == delim {
            // PART 8 resolves a bracket run before an emphasis marker, so a
            // marker inside a run is that run's content and cannot answer an
            // opener outside it - nor the reverse (markup-carve/carve#2577,
            // carve-rs#2161). `runs` holds the runs open here and it was seeded
            // with the runs open at the OPENER, so the innermost run agreeing
            // is the two delimiters sharing one. Not whether a run is open at
            // all: a pair both of whose halves sit inside one still pairs, and a
            // run BETWEEN two halves has been popped by the time we get here.
            if emphasis_run_at(bounds, &regions, j) != opener_run {
                j += 1;
                continue;
            }
            let prev = bytes.get(j.wrapping_sub(1)).copied().unwrap_or(b' ');
            if is_carve_ws(prev) {
                j += 1;
                continue;
            }
            // Word-boundary closer (spec §9): no bare delimiter closes when
            // followed by an alphanumeric. Applies to every delimiter.
            // NOTE: a `=` is NOT excluded here when it abuts a smart operator
            // (`=b=>`): both reference impls (carve-js, carve-php) let the
            // highlight close there, so rs matches them rather than being the
            // lone grammar-pedantic outlier on this unpinned corner. The
            // operator exclusion applies only to the OPENER (the corpus-pinned
            // `a => b` case).
            if let Some(&next) = bytes.get(j + 1) {
                if next.is_ascii_alphanumeric() || followed_by_grapheme_extender(bytes, j + 1) {
                    j += 1;
                    continue;
                }
            }
            return Some(j);
        }
        j += 1;
    }
    None
}

/// Whether the `]` at `close` ends a bracket a link or image could use: it
/// has an opener, and that opener is not a footnote reference or an inline
/// note.
pub(in crate::parse) fn opens_a_link(
    bytes: &[u8],
    close: usize,
    bounds: &InlineBounds<'_>,
) -> bool {
    let Some(&open) = bounds.openers.get(close) else {
        return false;
    };
    if open == NO_BRACKET_MATCH {
        return false;
    }
    if bytes.get(open + 1) == Some(&b'^') {
        return false;
    }
    // The `^` of a `{^` opener belongs to that opener, not to a note `^[`.
    if open > 0 && bytes[open - 1] == b'^' && !braced_superscript_marker(bytes, open - 1) {
        return false;
    }
    !is_reference_label_slot(bytes, open, bounds)
}

/// Whether the bracket opening at `open` is the label half of a reference
/// pair: glued brackets pair up left to right, so `[a]` in `[b][a](u)` is a
/// label and its `(u)` is text, while `[c]` in `[b][a][c](u)` is a link.
fn is_reference_label_slot(bytes: &[u8], open: usize, bounds: &InlineBounds<'_>) -> bool {
    let mut label = false;
    let mut at = open;
    while at > 0 && bytes[at - 1] == b']' {
        match bounds.openers.get(at - 1) {
            Some(&prev) if prev != NO_BRACKET_MATCH && bytes.get(prev + 1) != Some(&b'^') => {
                label = !label;
                at = prev;
            }
            _ => break,
        }
    }
    label
}

fn braced_superscript_marker(bytes: &[u8], caret: usize) -> bool {
    let Some(brace) = caret.checked_sub(1).filter(|&brace| bytes[brace] == b'{') else {
        return false;
    };
    let escapes = bytes[..brace]
        .iter()
        .rev()
        .take_while(|&&byte| byte == b'\\')
        .count();
    escapes % 2 == 0
}

/// The `)` closing the destination that opens at `open`, or `None` when what
/// follows is not one: parentheses balance and escape, the destination is not
/// empty, and it holds no space outside a title.
///
/// A destination OPENING WITH `<` was refused too, on the premise that an
/// angle-wrapped one cannot be a destination (carve-rs#1665). Carve takes it
/// literally instead - `[a](<b>)` resolves here and in the oracle with `<b>` as
/// its href, brackets and all - so the premise was wrong and the refusal hid a
/// real destination from the closing scan. A marker inside one then closed there:
/// `~[a](<b~>) c` lost the link (carve-rs#2174). The shapes that genuinely cannot
/// be a destination are still refused by the rules below, which is why dropping
/// this one changes nothing for `(<b c>)`, `(<>)` or an unclosed `(<b`.
pub(in crate::parse) fn link_destination_end(bytes: &[u8], open: usize) -> Option<usize> {
    if matches!(bytes.get(open + 1), None | Some(b')')) {
        return None;
    }
    let mut depth = 1usize;
    let mut quote = 0u8;
    let mut i = open + 1;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'\\' && i + 1 < bytes.len() {
            i += 2;
            continue;
        }
        if b == b'\n' {
            return None;
        }
        if quote != 0 {
            if b == quote {
                quote = 0;
            }
            i += 1;
            continue;
        }
        match b {
            b'"' | b'\'' => quote = b,
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            // The one space a title may sit behind; any other space means no
            // destination, so stop rather than scan on.
            b' ' | b'\t' if !matches!(bytes.get(i + 1), Some(b'"') | Some(b'\'')) => {
                return None;
            }
            _ => {}
        }
        i += 1;
    }
    None
}

pub(in crate::parse) fn autolink_closer(bytes: &[u8], open: usize) -> Option<usize> {
    let tail = bytes.get(open + 1..)?;
    let stop = tail
        .iter()
        .position(|&b| matches!(b, b'>' | b'<' | b' ' | b'\t' | b'\r' | b'\n'))?;
    (tail[stop] == b'>').then_some(open + 1 + stop)
}

/// The `>` closing the autolink at `open`, or `None` when the body between
/// the angle brackets is not a URL or email autolink target.
pub(in crate::parse) fn scanned_autolink_end(bytes: &[u8], open: usize) -> Option<usize> {
    let close = autolink_closer(bytes, open)?;
    let target = std::str::from_utf8(&bytes[open + 1..close]).ok()?;
    (is_url_autolink_target(target) || is_email_autolink_target(target)).then_some(close)
}

/// The `}` of a raw inline's format token at `open`.
pub(in crate::parse) fn raw_inline_format_end(bytes: &[u8], open: usize) -> Option<usize> {
    if bytes.get(open) != Some(&b'{') || bytes.get(open + 1) != Some(&b'=') {
        return None;
    }
    let mut i = open + 2;
    if !bytes.get(i)?.is_ascii_alphabetic() {
        return None;
    }
    while i < bytes.len()
        && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'-' || bytes[i] == b'_')
    {
        i += 1;
    }
    (bytes.get(i) == Some(&b'}')).then_some(i)
}

/// The last byte of the braced inline opening at `open`, matched the way the
/// main loop matches these constructs (E2a, markup-carve/carve#2027).
pub(in crate::parse) fn braced_inline_end(
    bytes: &[u8],
    open: usize,
    memo: &mut EmphasisMemo,
    scanning: u8,
    bounds: &InlineBounds<'_>,
) -> Option<usize> {
    let last = *memo
        .last_brace
        .get_or_insert_with(|| bytes.iter().rposition(|&b| b == b'}'));
    if last.map_or(true, |l| l < open) {
        return None;
    }
    braced_inline_scan(bytes, open, scanning, bounds)
}

pub(in crate::parse) fn braced_inline_scan(
    bytes: &[u8],
    open: usize,
    _scanning: u8,
    bounds: &InlineBounds<'_>,
) -> Option<usize> {
    if decorated_braced_opener(bytes, open) {
        return None;
    }
    let delim = bytes.get(open + 1).copied()?;
    let content = open + 2;
    // `{~ ~> ~}` is matched before the forced strike, as the main loop does.
    if delim == b'~' {
        if let Some((pair, _)) = substitution_at(bytes, open, bounds) {
            return Some(pair + 1);
        }
    }
    let pair: [u8; 2] = match delim {
        b'/' | b'*' | b'_' | b'^' | b',' | b'~' | b'=' | b'+' | b'-' | b'#' => [delim, b'}'],
        _ => return None,
    };
    let close = if matches!(delim, b'#') {
        if !bounds.has_delim_brace_from(b'#', open) {
            return None;
        }
        find_seq(bytes, content, &pair)?
    } else {
        bounds.braced_closers(bytes).close(bytes, open, delim)?
    };
    // An empty group `{XX}` forms no span (CARVE-P3-014).
    if close == content {
        return None;
    }
    Some(
        bounds
            .braced_closers(bytes)
            .after_attributes(bytes, close + 2)
            - 1,
    )
}
