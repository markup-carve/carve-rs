use super::*;

#[derive(Clone)]
pub(crate) struct FootnoteEntry {
    pub(super) label: Option<String>,
    pub(super) inline: Option<Vec<InlineNode>>,
    pub(super) backrefs: Vec<String>,
}

pub(crate) fn collect_footnotes(doc: &mut Document, assign_ref_ids: bool) -> Vec<FootnoteEntry> {
    let mut order = Vec::new();
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    let mut def_labels: HashMap<String, String> = HashMap::new();
    for label in doc.footnote_defs.keys() {
        def_labels
            .entry(label_key(label))
            .or_insert_with(|| label.clone());
    }
    let mut label_indices: HashMap<String, usize> = HashMap::new();

    for block in &mut doc.children {
        collect_footnotes_block(
            assign_ref_ids,
            block,
            &def_labels,
            &mut label_indices,
            &mut seen,
            &mut order,
        );
    }

    let mut idx = 0;
    while idx < order.len() {
        let Some(label) = order[idx].label.clone() else {
            idx += 1;
            continue;
        };
        if let Some(blocks) = doc.footnote_defs.get_mut(&label) {
            for block in blocks {
                collect_footnotes_block(
                    assign_ref_ids,
                    block,
                    &def_labels,
                    &mut label_indices,
                    &mut seen,
                    &mut order,
                );
            }
        }
        idx += 1;
    }

    order
}

pub(crate) fn used_footnote_labels(
    children: &[BlockNode],
    definitions: &BTreeMap<String, Vec<BlockNode>>,
) -> std::collections::BTreeSet<String> {
    if definitions.is_empty() {
        return std::collections::BTreeSet::new();
    }
    let mut probe = Document {
        frontmatter: BTreeMap::new(),
        frontmatter_raw: None,
        footnote_defs: definitions.clone(),
        footnote_def_pos: BTreeMap::new(),
        children: children.to_vec(),
        source_len: 0,
        ingest_payload_len: 0,
    };
    collect_footnotes(&mut probe, false)
        .into_iter()
        .filter_map(|entry| entry.label)
        .collect()
}

pub(super) fn collect_footnotes_block(
    assign_ref_ids: bool,
    block: &mut BlockNode,
    def_labels: &HashMap<String, String>,
    label_indices: &mut HashMap<String, usize>,
    seen: &mut BTreeMap<String, usize>,
    order: &mut Vec<FootnoteEntry>,
) {
    match block {
        BlockNode::Heading(h) => collect_footnotes_inline(
            assign_ref_ids,
            &mut h.children,
            def_labels,
            label_indices,
            seen,
            order,
        ),
        BlockNode::Paragraph(p) => {
            collect_footnotes_inline(
                assign_ref_ids,
                &mut p.children,
                def_labels,
                label_indices,
                seen,
                order,
            );
        }
        BlockNode::List(l) => {
            for item in &mut l.items {
                for child in &mut item.children {
                    collect_footnotes_block(
                        assign_ref_ids,
                        child,
                        def_labels,
                        label_indices,
                        seen,
                        order,
                    );
                }
            }
        }
        BlockNode::BlockQuote(b) => {
            for child in &mut b.children {
                collect_footnotes_block(
                    assign_ref_ids,
                    child,
                    def_labels,
                    label_indices,
                    seen,
                    order,
                );
            }
        }
        BlockNode::Table(t) => {
            if let Some(caption) = &mut t.caption {
                collect_footnotes_inline(
                    assign_ref_ids,
                    caption,
                    def_labels,
                    label_indices,
                    seen,
                    order,
                );
            }
            for row in &mut t.rows {
                for cell in &mut row.cells {
                    collect_footnotes_inline(
                        assign_ref_ids,
                        &mut cell.children,
                        def_labels,
                        label_indices,
                        seen,
                        order,
                    );
                    if let Some(blocks) = &mut cell.blocks {
                        for block in blocks {
                            collect_footnotes_block(
                                assign_ref_ids,
                                block,
                                def_labels,
                                label_indices,
                                seen,
                                order,
                            );
                        }
                    }
                }
            }
        }
        BlockNode::Admonition(a) => {
            if let Some(title) = &mut a.title {
                collect_footnotes_inline(
                    assign_ref_ids,
                    title,
                    def_labels,
                    label_indices,
                    seen,
                    order,
                );
            }
            for child in &mut a.children {
                collect_footnotes_block(
                    assign_ref_ids,
                    child,
                    def_labels,
                    label_indices,
                    seen,
                    order,
                );
            }
        }
        BlockNode::FigureGroup(g) => {
            // Children first: the panels precede the group caption in the
            // rendered output, so their footnote references number first.
            for child in &mut g.children {
                collect_footnotes_block(
                    assign_ref_ids,
                    child,
                    def_labels,
                    label_indices,
                    seen,
                    order,
                );
            }
            if let Some(caption) = &mut g.caption {
                collect_footnotes_inline(
                    assign_ref_ids,
                    caption,
                    def_labels,
                    label_indices,
                    seen,
                    order,
                );
            }
        }
        BlockNode::LineBlock(lb) => {
            for child in &mut lb.children {
                collect_footnotes_block(
                    assign_ref_ids,
                    child,
                    def_labels,
                    label_indices,
                    seen,
                    order,
                );
            }
        }
        BlockNode::Directive(d) => {
            // The title first, like an admonition's: a reference written in the
            // opener is numbered where the opener stands, and a note referenced
            // ONLY there still reaches the endnotes section.
            if let Some(title) = &mut d.title {
                collect_footnotes_inline(
                    assign_ref_ids,
                    title,
                    def_labels,
                    label_indices,
                    seen,
                    order,
                );
            }
            for child in &mut d.children {
                collect_footnotes_block(
                    assign_ref_ids,
                    child,
                    def_labels,
                    label_indices,
                    seen,
                    order,
                );
            }
        }
        BlockNode::Div(d) => {
            for child in &mut d.children {
                collect_footnotes_block(
                    assign_ref_ids,
                    child,
                    def_labels,
                    label_indices,
                    seen,
                    order,
                );
            }
        }
        BlockNode::Section(d) => {
            for child in &mut d.children {
                collect_footnotes_block(
                    assign_ref_ids,
                    child,
                    def_labels,
                    label_indices,
                    seen,
                    order,
                );
            }
        }
        BlockNode::DefinitionList(d) => {
            for item in &mut d.items {
                for term in &mut item.terms {
                    collect_footnotes_inline(
                        assign_ref_ids,
                        term,
                        def_labels,
                        label_indices,
                        seen,
                        order,
                    );
                }
                for definition in &mut item.definitions {
                    for child in definition {
                        collect_footnotes_block(
                            assign_ref_ids,
                            child,
                            def_labels,
                            label_indices,
                            seen,
                            order,
                        );
                    }
                }
            }
        }
        BlockNode::Figure(f) => {
            collect_footnotes_inline(
                assign_ref_ids,
                &mut f.caption,
                def_labels,
                label_indices,
                seen,
                order,
            );
            match &mut *f.target {
                FigureTarget::BlockQuote(b) => {
                    for child in &mut b.children {
                        collect_footnotes_block(
                            assign_ref_ids,
                            child,
                            def_labels,
                            label_indices,
                            seen,
                            order,
                        );
                    }
                }
                FigureTarget::Table(t) => {
                    if let Some(caption) = &mut t.caption {
                        collect_footnotes_inline(
                            assign_ref_ids,
                            caption,
                            def_labels,
                            label_indices,
                            seen,
                            order,
                        );
                    }
                    for row in &mut t.rows {
                        for cell in &mut row.cells {
                            collect_footnotes_inline(
                                assign_ref_ids,
                                &mut cell.children,
                                def_labels,
                                label_indices,
                                seen,
                                order,
                            );
                            if let Some(blocks) = &mut cell.blocks {
                                for block in blocks {
                                    collect_footnotes_block(
                                        assign_ref_ids,
                                        block,
                                        def_labels,
                                        label_indices,
                                        seen,
                                        order,
                                    );
                                }
                            }
                        }
                    }
                }
                FigureTarget::Paragraph(p) => {
                    collect_footnotes_inline(
                        assign_ref_ids,
                        &mut p.children,
                        def_labels,
                        label_indices,
                        seen,
                        order,
                    );
                }
                FigureTarget::Image(_) | FigureTarget::CodeBlock(_) => {}
            }
        }
        BlockNode::ExtensionCarrier(e) => {
            // The carrier holds the title MOVED off the container it stands in,
            // and the carrier is what renders, so the reference has to be
            // numbered here. Collecting it on the container instead numbers a
            // copy nobody emits.
            if let Some(summary) = &mut e.summary {
                collect_footnotes_inline(
                    assign_ref_ids,
                    summary,
                    def_labels,
                    label_indices,
                    seen,
                    order,
                );
            }
            for child in &mut e.children {
                collect_footnotes_block(
                    assign_ref_ids,
                    child,
                    def_labels,
                    label_indices,
                    seen,
                    order,
                );
            }
        }
        _ => {}
    }
}

/// Walk an inline subtree that the document itself reaches, numbering the notes
/// in it.
///
/// A block is never inside a link, so nothing a block walker hands over is
/// discarded text; `discarded` is raised only while descending, by the one arm
/// that can degrade its own children (see [`collect_footnotes_inline_scoped`]).
pub(super) fn collect_footnotes_inline(
    assign_ref_ids: bool,
    nodes: &mut [InlineNode],
    def_labels: &HashMap<String, String>,
    label_indices: &mut HashMap<String, usize>,
    seen: &mut BTreeMap<String, usize>,
    order: &mut Vec<FootnoteEntry>,
) {
    collect_footnotes_inline_scoped(
        assign_ref_ids,
        nodes,
        def_labels,
        label_indices,
        seen,
        order,
        false,
    );
}

/// `discarded` says the nodes sit in text the document throws away.
///
/// PART 9R R2, `A NOTE INSIDE AN UNRESOLVED REFERENCE IS NOT A REFERENCE`
/// (markup-carve/carve#1198). R1 degrades an unresolved reference to its
/// literal SOURCE, so the link text built for it never reaches the reader. The
/// subtree is still WALKED rather than skipped, because a note in there must
/// have any stale number cleared the same way a reference whose definition went
/// away has its own cleared (carve-rs#641).
#[allow(clippy::too_many_arguments)]
pub(super) fn collect_footnotes_inline_scoped(
    assign_ref_ids: bool,
    nodes: &mut [InlineNode],
    def_labels: &HashMap<String, String>,
    label_indices: &mut HashMap<String, usize>,
    seen: &mut BTreeMap<String, usize>,
    order: &mut Vec<FootnoteEntry>,
    discarded: bool,
) {
    for node in nodes {
        match node {
            InlineNode::Footnote(f) => {
                // The reference degraded to its literal source, so the text
                // holding this note was discarded: it draws no number, a
                // definition it was the only use of stays unreferenced and is
                // dropped, and no endnotes section is written on its account.
                // Numbering it anyway is what a pipeline does when it resolves
                // footnotes before it knows the reference failed, and the
                // numbering says so out loud - the note a reader CAN see then
                // reads as a repeat of a reference the document does not
                // contain.
                if discarded {
                    f.number = None;
                    f.ref_id = None;
                    continue;
                }
                if let Some(inline) = &f.inline {
                    let number = order.len() + 1;
                    let ref_id = authored_ref_id(f).unwrap_or_else(|| format!("fnref{number}"));
                    f.number = Some(number);
                    if assign_ref_ids {
                        f.ref_id = Some(ref_id.clone());
                    }
                    order.push(FootnoteEntry {
                        label: None,
                        inline: Some(inline.clone()),
                        backrefs: vec![ref_id],
                    });
                    continue;
                }

                // CLEAR FIRST, so that either gate below leaves the reference
                // unnumbered rather than keeping a number from an earlier run.
                // This pass is re-run on a document whose definitions the
                // profile filter took away, and a reference that no longer
                // resolves must not keep the number it had while it did
                // (carve-rs#641). The resolved path overwrites this.
                f.number = None;
                let Some(id) = &f.id else {
                    continue;
                };
                if id.contains(['\r', '\n']) {
                    continue;
                }
                let key = label_key(id);
                let Some(definition_label) = def_labels.get(&key) else {
                    continue;
                };
                let idx = label_indices.get(&key).copied().unwrap_or_else(|| {
                    order.push(FootnoteEntry {
                        label: Some(definition_label.clone()),
                        inline: None,
                        backrefs: Vec::new(),
                    });
                    let idx = order.len() - 1;
                    label_indices.insert(key.clone(), idx);
                    idx
                });
                let number = idx + 1;
                let occurrence = seen.entry(key).or_insert(0);
                *occurrence += 1;
                // An authored id replaces the generated one, and the backlink
                // follows it (PART 9 §16).
                let ref_id = authored_ref_id(f).unwrap_or_else(|| {
                    if *occurrence == 1 {
                        format!("fnref{number}")
                    } else {
                        format!("fnref{number}-{occurrence}")
                    }
                });
                f.number = Some(number);
                if assign_ref_ids {
                    f.ref_id = Some(ref_id.clone());
                }
                order[idx].backrefs.push(ref_id);
            }
            InlineNode::Emphasis(e) => collect_footnotes_inline_scoped(
                assign_ref_ids,
                &mut e.children,
                def_labels,
                label_indices,
                seen,
                order,
                discarded,
            ),
            // The one arm that can raise `discarded`: a reference this document
            // never resolved renders its literal source and never writes these
            // children (see `render_link`).
            InlineNode::Link(l) => {
                let discarded = discarded || crate::parse::is_unresolved_reference(l);
                collect_footnotes_inline_scoped(
                    assign_ref_ids,
                    &mut l.children,
                    def_labels,
                    label_indices,
                    seen,
                    order,
                    discarded,
                );
            }
            InlineNode::Span(s) => collect_footnotes_inline_scoped(
                assign_ref_ids,
                &mut s.children,
                def_labels,
                label_indices,
                seen,
                order,
                discarded,
            ),
            InlineNode::Ruby(r) => {
                for pair in &mut r.pairs {
                    collect_footnotes_inline_scoped(
                        assign_ref_ids,
                        &mut pair.base,
                        def_labels,
                        label_indices,
                        seen,
                        order,
                        discarded,
                    );
                    collect_footnotes_inline_scoped(
                        assign_ref_ids,
                        &mut pair.annotation,
                        def_labels,
                        label_indices,
                        seen,
                        order,
                        discarded,
                    );
                }
            }
            InlineNode::Extension(e) => collect_footnotes_inline_scoped(
                assign_ref_ids,
                &mut e.children,
                def_labels,
                label_indices,
                seen,
                order,
                discarded,
            ),
            InlineNode::CriticInsert(c) => {
                collect_footnotes_inline_scoped(
                    assign_ref_ids,
                    &mut c.children,
                    def_labels,
                    label_indices,
                    seen,
                    order,
                    discarded,
                );
            }
            InlineNode::CriticDelete(c) => {
                collect_footnotes_inline_scoped(
                    assign_ref_ids,
                    &mut c.children,
                    def_labels,
                    label_indices,
                    seen,
                    order,
                    discarded,
                );
            }
            InlineNode::CriticSubstitute(c) => {
                for half in [&mut c.old, &mut c.new] {
                    collect_footnotes_inline_scoped(
                        assign_ref_ids,
                        half,
                        def_labels,
                        label_indices,
                        seen,
                        order,
                        discarded,
                    );
                }
            }
            InlineNode::CitationGroup(g) => {
                for item in &mut g.items {
                    if let Some(prefix) = &mut item.prefix {
                        collect_footnotes_inline_scoped(
                            assign_ref_ids,
                            prefix,
                            def_labels,
                            label_indices,
                            seen,
                            order,
                            discarded,
                        );
                    }
                    if let Some(locator) = &mut item.locator {
                        collect_footnotes_inline_scoped(
                            assign_ref_ids,
                            locator,
                            def_labels,
                            label_indices,
                            seen,
                            order,
                            discarded,
                        );
                    }
                }
            }
            _ => {}
        }
    }
}

/// Source line stamped on a block during parsing, if any. Used to anchor the
/// endnote `<li>` to its definition's line (carve-php / carve-js parity).
pub(super) fn block_source_line(block: &BlockNode) -> Option<&str> {
    let attrs = match block {
        BlockNode::LinkReferenceDefinition(n) => n.attrs.as_ref(),
        BlockNode::CitationDefinition(n) => n.attrs.as_ref(),
        BlockNode::Heading(n) => n.attrs.as_ref(),
        BlockNode::Paragraph(n) => n.attrs.as_ref(),
        BlockNode::ThematicBreak(n) => n.attrs.as_ref(),
        BlockNode::CodeBlock(n) => n.attrs.as_ref(),
        BlockNode::List(n) => n.attrs.as_ref(),
        BlockNode::BlockQuote(n) => n.attrs.as_ref(),
        BlockNode::Table(n) => n.attrs.as_ref(),
        BlockNode::Admonition(n) => n.attrs.as_ref(),
        BlockNode::Directive(n) => n.attrs.as_ref(),
        BlockNode::Div(n) => n.attrs.as_ref(),
        BlockNode::Section(n) => n.attrs.as_ref(),
        BlockNode::LineBlock(n) => n.attrs.as_ref(),
        BlockNode::DefinitionList(n) => n.attrs.as_ref(),
        BlockNode::Figure(n) => n.attrs.as_ref(),
        BlockNode::FigureGroup(n) => n.attrs.as_ref(),
        BlockNode::BlockExtension(n) => n.attrs.as_ref(),
        BlockNode::ExtensionCarrier(n) => n.attrs.as_ref(),
        BlockNode::BlockImage(n) => n.attrs.as_ref(),
        BlockNode::AbbreviationDef(_) | BlockNode::RawBlock(_) | BlockNode::Comment(_) => None,
    }?;
    attrs.key_values.get("data-source-line").map(String::as_str)
}

pub(super) fn render_footnotes_section(
    doc: &Document,
    footnotes: &[FootnoteEntry],
    options: &Options<'_>,
    state: &mut RenderState,
) -> String {
    // Suppress `::: footnotes` placement while rendering footnote bodies, so a
    // nested block renders as an ordinary div instead of writing a marker.
    let was_rendering_footnotes = state.rendering_footnotes;
    state.rendering_footnotes = true;
    let mut out = String::new();
    if let Some(FootnotesHeading { title, label, id }) = state.footnotes_heading.clone() {
        out.push_str("<section role=\"doc-endnotes\"");
        if let Some(id) = &id {
            write!(out, " aria-labelledby=\"{}\"", escape_attr(id)).unwrap();
        } else {
            write!(
                out,
                " aria-label=\"{}\"",
                escape_attr(options.label(crate::extension::LABEL_ENDNOTES))
            )
            .unwrap();
        }
        out.push('>');
        if let Some(title) = &title {
            out.push_str("\n  <p class=\"admonition-title\"");
            if let Some(id) = &id {
                write!(out, " id=\"{}\"", escape_attr(id)).unwrap();
            }
            out.push('>');
            render_inlines(&mut out, title, options, state);
            out.push_str("</p>");
        }
        if let Some(label) = label {
            out.push_str("\n  <p class=\"div-label\">");
            out.push_str(&render_container_label(&label, options));
            out.push_str("</p>");
        }
    } else {
        out.push_str("<section role=\"doc-endnotes\" aria-label=\"");
        out.push_str(&escape_attr(
            options.label(crate::extension::LABEL_ENDNOTES),
        ));
        out.push_str("\">");
    }
    out.push_str("\n  <hr>\n  <ol>");
    for (idx, entry) in footnotes.iter().enumerate() {
        let num = idx + 1;
        out.push('\n');
        // Anchor the endnote item to its definition's source line (taken from
        // the first stamped body block), matching carve-php and carve-js.
        let li_source_line = if options.source_lines {
            entry
                .label
                .as_ref()
                .and_then(|label| doc.footnote_defs.get(label))
                .and_then(|blocks| blocks.iter().find_map(block_source_line))
        } else {
            None
        };
        out.push_str(&format!("    <li id=\"fn{num}\""));
        // PART 10 §13a: the authored label rides the endnote item as well as the
        // reference, and a presence carrier says the endnote came from an
        // INLINE footnote - which has no label and no definition line, so
        // nothing else in the rendered document distinguishes it from a
        // referenced one.
        if options.html_carriers() {
            if let Some(label) = &entry.label {
                write_carrier(
                    &mut out,
                    crate::roundtrip::FOOTNOTE_LABEL,
                    label,
                    &None,
                    None,
                    crate::render_loss::RawNodeType::Block,
                );
            }
            if entry.inline.is_some() {
                write_carrier(
                    &mut out,
                    crate::roundtrip::INLINE_FOOTNOTE,
                    "1",
                    &None,
                    None,
                    crate::render_loss::RawNodeType::Block,
                );
            }
        }
        // `data-source-line` is a render annotation and stays last.
        if let Some(line) = li_source_line {
            out.push_str(&format!(" data-source-line=\"{line}\""));
        }
        out.push('>');
        if let Some(inline) = &entry.inline {
            out.push('\n');
            out.push_str("      <p>");
            render_inlines(&mut out, inline, options, state);
            out.push_str(&render_backlinks(&entry.backrefs, options));
            out.push_str("</p>");
        } else if let Some(label) = &entry.label {
            let blocks: &[BlockNode] = doc
                .footnote_defs
                .get(label)
                .map(Vec::as_slice)
                .unwrap_or_default();
            let mut visible: Vec<(&BlockNode, String)> = Vec::new();
            for block in blocks {
                let mut rendered = String::new();
                render_block(&mut rendered, block, 3, options, state);
                if rendered.is_empty() {
                    continue;
                }
                visible.push((block, rendered));
            }
            if visible.is_empty() {
                out.push('\n');
                indent(&mut out, 3);
                out.push_str("<p>");
                out.push_str(&render_backlinks(&entry.backrefs, options));
                out.push_str("</p>");
            }
            let visible_len = visible.len();
            for (block_idx, (block, mut rendered)) in visible.into_iter().enumerate() {
                out.push('\n');
                if block_idx + 1 == visible_len {
                    let backlink = render_backlinks(&entry.backrefs, options);
                    if matches!(block, BlockNode::Paragraph(_)) {
                        if let Some(pos) = rendered.rfind("</p>") {
                            rendered.insert_str(pos, &backlink);
                        } else {
                            rendered.push_str(&backlink);
                        }
                    } else {
                        rendered.push('\n');
                        indent(&mut rendered, 3);
                        rendered.push_str("<p>");
                        rendered.push_str(&backlink);
                        rendered.push_str("</p>");
                    }
                }
                out.push_str(&rendered);
            }
        }
        out.push('\n');
        out.push_str("    </li>");
    }
    out.push_str("\n  </ol>\n</section>");
    state.rendering_footnotes = was_rendering_footnotes;
    out
}

pub(super) fn render_backlinks(backrefs: &[String], options: &Options<'_>) -> String {
    // A note referenced once gets a plain `↩`; a note referenced N>1 times gets
    // one numbered backlink per reference (`↩<sup>k</sup>`, space-separated) so
    // each return arrow is distinct (matches carve-php + pandoc).
    //
    // The accessible name is the label plus WHAT THE LINK VISIBLY SAYS (PART 9
    // §16, markup-carve/carve#1455): the label alone for a lone `↩`, the label
    // plus k for the k-th of several. Matching the visible text is WCAG 2.5.3,
    // and it is why the number is the REFERENCE ORDINAL rather than the note's
    // - the note number appears nowhere in this link's text.
    let label = options.label(crate::extension::LABEL_FOOTNOTE_BACKLINK);
    if backrefs.len() <= 1 {
        let name = crate::escape::escape_attr(label);
        return backrefs
            .iter()
            .map(|ref_id| {
                let ref_id = crate::escape::escape_attr(ref_id);
                format!("<a href=\"#{ref_id}\" role=\"doc-backlink\" aria-label=\"{name}\">↩</a>")
            })
            .collect();
    }
    backrefs
        .iter()
        .enumerate()
        .map(|(k, ref_id)| {
            let name = crate::escape::escape_attr(&format!("{label} {}", k + 1));
            let ref_id = crate::escape::escape_attr(ref_id);
            format!(
                "<a href=\"#{ref_id}\" role=\"doc-backlink\" aria-label=\"{name}\">↩<sup>{}</sup></a>",
                k + 1
            )
        })
        .collect::<Vec<_>>()
        .join(" ")
}
