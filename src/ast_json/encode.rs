use super::*;

pub(super) struct Writer<'a> {
    out: &'a mut String,
    first: bool,
}

impl<'a> Writer<'a> {
    fn new(out: &'a mut String) -> Self {
        out.push('{');
        Self { out, first: true }
    }

    fn field(&mut self, name: &str, write: impl FnOnce(&mut String)) {
        if !self.first {
            self.out.push(',');
        }
        self.first = false;
        write_string(self.out, name);
        self.out.push(':');
        write(self.out);
    }

    fn finish(self) {
        self.out.push('}');
    }
}

pub(super) fn write_document(out: &mut String, doc: &Document) {
    let mut w = Writer::new(out);
    w.field("type", |out| write_string(out, "document"));
    w.field("children", |out| {
        out.push('[');
        let mut first = true;
        if let Some(raw) = &doc.frontmatter_raw {
            write_comma(out, &mut first);
            write_frontmatter(out, raw);
        }
        // Definitions come AFTER the content and in source order (PART 12 §7).
        //
        // `footnote_defs_in_source_order` rather than a second copy of that
        // sort: this function had its own, and the two then had to be kept
        // agreeing by hand - which is how the `carve` writer's copy came to
        // order definitions differently from the encoder's (carve-rs#685).
        let footnote_defs = footnote_defs_in_source_order(doc);
        for entry in ordered_document_entries(doc, &footnote_defs) {
            write_comma(out, &mut first);
            match entry {
                DocEntry::Block(child) => write_block(out, child),
                DocEntry::FootnoteDef(label, children, pos) => {
                    write_footnote_def(out, label, children, pos)
                }
            }
        }
        out.push(']');
    });
    w.field("srcByteLength", |out| write_usize(out, doc.source_len));
    w.finish();
}

/// One direct child of the serialized document.
///
/// A footnote definition is a map on the runtime document and only becomes a
/// sibling on the wire, so the two kinds have to be lined up before either is
/// written.
pub(crate) enum DocEntry<'a> {
    Block(&'a BlockNode),
    /// Label, body, and where the definition LINE was written - which the body
    /// does not record and an empty body cannot stand in for.
    FootnoteDef(&'a String, &'a Vec<BlockNode>, Option<&'a Pos>),
}

/// PART 12 §7: "Definitions appear in DOCUMENT ORDER by source position."
///
/// AN OFFSET ONLY ORDERS WITHIN ONE FILE. After an include expansion the map
/// holds definitions from several, each measured in its own coordinate space,
/// so two children that both define a note near their own start tie on offset
/// and fall back to the LABEL - which put `[^m]` from the second include above
/// `[^n]` from the first. The file each definition came from is ordered by
/// where that file's content first appears in the assembled document, which is
/// what "document order" means once the tree has more than one source.
pub(crate) fn footnote_defs_in_source_order(doc: &Document) -> Vec<(&String, &Vec<BlockNode>)> {
    let file_order = file_appearance_order(doc);
    let mut defs: Vec<(&String, &Vec<BlockNode>)> = doc.footnote_defs.iter().collect();
    defs.sort_by_key(|(label, children)| {
        let pos = doc
            .footnote_def_pos
            .get(label.as_str())
            .or_else(|| first_block_pos(children));
        let file_rank = match (&file_order, pos.and_then(|p| p.file.as_ref())) {
            // No includes in this document: every definition is in the one
            // file, so the offset alone is the whole key, exactly as before.
            (None, _) => 0,
            (Some(_), None) => 0,
            (Some(order), Some(file)) => order.get(file.as_str()).copied().unwrap_or(usize::MAX),
        };
        (
            file_rank,
            footnote_def_start(doc, label, children).unwrap_or(usize::MAX),
            label.as_str(),
        )
    });

    defs
}

/// Which file each piece of the assembled document came from, ranked by first
/// appearance. `None` when nothing carries a file identity, which is every
/// document that used no includes - the caller then skips the whole question.
pub(super) fn file_appearance_order(
    doc: &Document,
) -> Option<std::collections::HashMap<String, usize>> {
    let mut order: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut next = 1usize;
    crate::includes::walk_blocks(&doc.children, &mut |block| {
        if let Some(file) = block_pos(block).and_then(|pos| pos.file.as_ref()) {
            order.entry(file.as_str().to_string()).or_insert_with(|| {
                let rank = next;
                next += 1;
                rank
            });
        }
    });
    if order.is_empty() {
        return None;
    }
    Some(order)
}

/// Where a footnote definition starts, for §7's document order.
///
/// The DEFINITION LINE first, the body only as a fallback. Ordering by the body
/// alone put a definition with no blocks last whatever the author wrote, because
/// there was no block to read a start from - so `[^a]: {empty}` written above
/// `[^b]: x` was published below it, and §7's "document order by source
/// position" was decided by which definition happened to have content
/// (markup-carve/carve#1023).
///
/// The two never disagree: `footnote_def_pos` holds a line only for a
/// definition whose body places nothing, so exactly one of the two is available
/// per definition and the order is read from whichever that is.
pub(super) fn footnote_def_start(
    doc: &Document,
    label: &str,
    children: &[BlockNode],
) -> Option<usize> {
    doc.footnote_def_pos
        .get(label)
        .or_else(|| first_block_pos(children))
        .map(|pos| pos.start_offset)
}

pub(crate) fn ordered_document_entries<'a>(
    doc: &'a Document,
    footnote_defs: &[(&'a String, &'a Vec<BlockNode>)],
) -> Vec<DocEntry<'a>> {
    let mut entries: Vec<DocEntry<'a>> = doc.children.iter().map(DocEntry::Block).collect();
    entries.extend(footnote_defs.iter().map(|(label, body)| {
        DocEntry::FootnoteDef(label, body, doc.footnote_def_pos.get(label.as_str()))
    }));

    let slots: Vec<usize> = entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| collected_definition_offset(entry).is_some())
        .map(|(i, _)| i)
        .collect();
    if slots.len() < 2 {
        return entries;
    }

    let mut moved: Vec<DocEntry<'a>> = Vec::with_capacity(slots.len());
    for &i in slots.iter().rev() {
        moved.push(entries.remove(i));
    }
    moved.reverse();
    // FILE FIRST, then offset. After an include expansion these entries come
    // from several files, each measured in its own coordinate space, so two
    // children defining a note near their own start tie on offset - and the tie
    // decided the published order. See `file_appearance_order`.
    let file_order = file_appearance_order(doc);
    moved.sort_by_key(|entry| {
        (
            definition_file_rank(entry, file_order.as_ref()),
            collected_definition_offset(entry).unwrap_or(usize::MAX),
        )
    });
    for (&i, entry) in slots.iter().zip(moved) {
        entries.insert(i, entry);
    }
    entries
}

/// The published start offset of a COLLECTED definition, or `None` for anything
/// §7 does not collect. A collected definition with no placed position sorts
/// last rather than being given an invented one.
/// Where this entry's file sits in the assembled document, for the sort above.
/// Zero for a document with no includes, and for anything authored in the file
/// being rendered.
pub(super) fn definition_file_rank(
    entry: &DocEntry<'_>,
    file_order: Option<&std::collections::HashMap<String, usize>>,
) -> usize {
    let Some(order) = file_order else {
        return 0;
    };
    let file = match entry {
        DocEntry::Block(BlockNode::LinkReferenceDefinition(n)) => {
            n.pos.as_ref().and_then(|pos| pos.file.as_ref())
        }
        DocEntry::FootnoteDef(_, body, pos) => pos
            .or_else(|| first_block_pos(body))
            .and_then(|pos| pos.file.as_ref()),
        DocEntry::Block(_) => None,
    };
    match file {
        None => 0,
        Some(file) => order.get(file.as_str()).copied().unwrap_or(usize::MAX),
    }
}

pub(super) fn collected_definition_offset(entry: &DocEntry<'_>) -> Option<usize> {
    match entry {
        DocEntry::Block(BlockNode::LinkReferenceDefinition(n)) => {
            Some(n.pos.as_ref().map_or(usize::MAX, |pos| pos.start_offset))
        }
        DocEntry::FootnoteDef(_, body, pos) => Some(
            pos.or_else(|| first_block_pos(body))
                .map_or(usize::MAX, |pos| pos.start_offset),
        ),
        DocEntry::Block(_) => None,
    }
}

pub(crate) fn first_block_pos(children: &[BlockNode]) -> Option<&Pos> {
    children.iter().find_map(block_pos)
}

pub(crate) fn block_pos(node: &BlockNode) -> Option<&Pos> {
    match node {
        BlockNode::LinkReferenceDefinition(n) => n.pos.as_ref(),
        BlockNode::CitationDefinition(n) => n.pos.as_ref(),
        BlockNode::Heading(n) => n.pos.as_ref(),
        BlockNode::Paragraph(n) => n.pos.as_ref(),
        BlockNode::CodeBlock(n) => n.pos.as_ref(),
        BlockNode::List(n) => n.pos.as_ref(),
        BlockNode::BlockQuote(n) => n.pos.as_ref(),
        BlockNode::Table(n) => n.pos.as_ref(),
        BlockNode::Admonition(n) => n.pos.as_ref(),
        BlockNode::Directive(n) => n.pos.as_ref(),
        BlockNode::Div(n) => n.pos.as_ref(),
        BlockNode::Section(n) => n.pos.as_ref(),
        BlockNode::LineBlock(n) => n.pos.as_ref(),
        BlockNode::DefinitionList(n) => n.pos.as_ref(),
        BlockNode::Figure(n) => n.pos.as_ref(),
        BlockNode::FigureGroup(n) => n.pos.as_ref(),
        BlockNode::AbbreviationDef(n) => n.pos.as_ref(),
        BlockNode::RawBlock(n) => n.pos.as_ref(),
        BlockNode::Comment(n) => n.pos.as_ref(),
        BlockNode::BlockExtension(n) => n.pos.as_ref(),
        BlockNode::ExtensionCarrier(n) => n.pos.as_ref(),
        BlockNode::BlockImage(n) => n.pos.as_ref(),
        BlockNode::ThematicBreak(n) => n.pos.as_ref(),
    }
}

pub(super) fn write_frontmatter(out: &mut String, raw: &Frontmatter) {
    let mut w = Writer::new(out);
    w.field("type", |out| write_string(out, "frontmatter"));
    w.field("format", |out| write_string(out, &raw.format));
    w.field("content", |out| write_string(out, &raw.content));
    write_pos_field(&mut w, &raw.pos);
    w.finish();
}

pub(super) fn write_footnote_def(
    out: &mut String,
    label: &str,
    children: &[BlockNode],
    def_pos: Option<&Pos>,
) {
    let mut w = Writer::new(out);
    w.field("type", |out| write_string(out, "footnote"));
    w.field("label", |out| write_string(out, label));
    w.field("children", |out| write_blocks(out, children));
    let pos = match def_pos
        .cloned()
        .or_else(|| first_block_pos(children).cloned())
    {
        Some(mut pos) => {
            // THE SAME FUNCTION the parser ends every other closerless
            // container with, rather than a second spelling of it beside it.
            // This one was a widen and the parser's was a widen, so both agreed
            // and both were half the rule; an ingested definition (§6) reaches
            // only this one, so a copy of it has to end where the parse did.
            let last = children.iter().rev().find_map(block_pos).cloned();
            crate::parse::end_at_last_placed_child(&mut pos, last);
            Some(pos)
        }
        None => None,
    };
    write_pos_field(&mut w, &pos);
    w.finish();
}

pub(super) type EncodeFinish<'a> = Box<dyn FnOnce(&mut String, &mut Vec<EncodeTask<'a>>) + 'a>;

pub(super) enum EncodeTask<'a> {
    Block(&'a BlockNode, usize),
    Inline(&'a InlineNode, usize),
    RubyPair(&'a RubyPair, usize),
    ListItem(&'a ListItem, usize),
    TableRow(&'a TableRow, usize),
    TableCell(&'a TableCell, usize),
    DefinitionTerm(&'a DefinitionTerm, usize),
    DefinitionDef(&'a DefinitionDef, usize),
    FigureTarget(&'a FigureTarget, usize),
    Citation(&'a Citation, usize),
    Finish(EncodeFinish<'a>),
    Char(char),
}

pub(super) fn push_array<'a, T>(
    tasks: &mut Vec<EncodeTask<'a>>,
    values: &'a [T],
    mut task: impl FnMut(&'a T) -> EncodeTask<'a>,
) {
    tasks.push(EncodeTask::Char(']'));
    for (index, value) in values.iter().enumerate().rev() {
        tasks.push(task(value));
        if index != 0 {
            tasks.push(EncodeTask::Char(','));
        }
    }
}

pub(super) fn refuse_encode_depth(depth: usize) -> bool {
    ENCODE_DEPTH.with(|current| current.set(depth + 1));
    if depth >= MAX_JSON_DEPTH / 2 {
        ENCODE_REFUSED.with(|refused| refused.set(true));
        true
    } else {
        false
    }
}

pub(super) fn write_block(out: &mut String, node: &BlockNode) {
    run_encode_tasks(out, EncodeTask::Block(node, 0));
}

pub(super) fn run_encode_tasks<'a>(out: &mut String, first: EncodeTask<'a>) {
    let mut tasks = vec![first];
    while let Some(task) = tasks.pop() {
        match task {
            EncodeTask::Char(c) => out.push(c),
            EncodeTask::Finish(f) => f(out, &mut tasks),
            EncodeTask::Block(node, depth) => {
                if refuse_encode_depth(depth) {
                    continue;
                }
                encode_block_task(out, &mut tasks, node, depth);
            }
            EncodeTask::Inline(node, depth) => {
                if refuse_encode_depth(depth) {
                    continue;
                }
                encode_inline_task(out, &mut tasks, node, depth);
            }
            EncodeTask::RubyPair(pair, depth) => {
                out.push('{');
                let mut w = Writer { out, first: true };
                w.field("base", |out| out.push('['));
                tasks.push(EncodeTask::Finish(Box::new(move |out, tasks| {
                    let mut w = Writer { out, first: false };
                    w.field("annotation", |out| out.push('['));
                    tasks.push(EncodeTask::Char('}'));
                    push_array(tasks, &pair.annotation, |node| {
                        EncodeTask::Inline(node, depth)
                    });
                })));
                push_array(&mut tasks, &pair.base, |node| {
                    EncodeTask::Inline(node, depth)
                });
            }
            EncodeTask::ListItem(n, depth) => {
                let mut w = typed(out, "list_item");
                w.field("children", |out| out.push('['));
                tasks.push(EncodeTask::Finish(Box::new(move |out, _| {
                    let mut w = Writer { out, first: false };
                    if let Some(checked) = n.checked {
                        w.field("checked", |out| write_bool(out, checked));
                    }
                    if let Some(state) = n.task_state {
                        w.field("taskState", |out| write_string(out, &state.to_string()));
                    }
                    write_attrs_field(&mut w, &n.attrs);
                    write_pos_field(&mut w, &n.pos);
                    w.finish();
                })));
                push_array(&mut tasks, &n.children, |node| {
                    EncodeTask::Block(node, depth + 1)
                });
            }
            EncodeTask::TableRow(n, depth) => {
                let mut w = typed(out, "table_row");
                w.field("cells", |out| out.push('['));
                tasks.push(EncodeTask::Finish(Box::new(move |out, _| {
                    let mut w = Writer { out, first: false };
                    if let Some(lines) = n.continuation_lines {
                        w.field("continuationLines", |out| write_usize(out, lines));
                    }
                    write_attrs_field(&mut w, &n.attrs);
                    write_pos_field(&mut w, &n.pos);
                    w.finish();
                })));
                push_array(&mut tasks, &n.cells, |cell| {
                    EncodeTask::TableCell(cell, depth)
                });
            }
            EncodeTask::TableCell(n, depth) => {
                let mut w = typed(out, "table_cell");
                w.field("header", |out| write_bool(out, n.header));
                w.field(
                    if n.blocks.is_some() {
                        "blocks"
                    } else {
                        "children"
                    },
                    |out| out.push('['),
                );
                tasks.push(EncodeTask::Finish(Box::new(move |out, _| {
                    let mut w = Writer { out, first: false };
                    if let Some(span) = n.span {
                        w.field("span", |out| {
                            write_string(
                                out,
                                match span {
                                    TableCellSpan::Rowspan => "rowspan",
                                    TableCellSpan::Colspan => "colspan",
                                },
                            )
                        });
                    }
                    if let Some(n2) = n.colspan.filter(|count| *count != 1) {
                        w.field("colspan", |out| write_usize(out, n2));
                    }
                    if let Some(n2) = n.rowspan.filter(|count| *count != 1) {
                        w.field("rowspan", |out| write_usize(out, n2));
                    }
                    if let Some(align) = n.align {
                        w.field("align", |out| write_string(out, align_json(align)));
                    }
                    if let Some(valign) = n.valign {
                        w.field("valign", |out| write_string(out, valign_json(valign)));
                    }
                    write_attrs_field(&mut w, &n.attrs);
                    write_pos_field(&mut w, &n.pos);
                    w.finish();
                })));
                if let Some(blocks) = &n.blocks {
                    push_array(&mut tasks, blocks, |node| {
                        EncodeTask::Block(node, depth + 1)
                    });
                } else {
                    push_array(&mut tasks, &n.children, |node| {
                        EncodeTask::Inline(node, depth + 1)
                    });
                }
            }
            EncodeTask::DefinitionTerm(n, depth) => {
                let mut w = typed(out, "definition_term");
                w.field("children", |out| out.push('['));
                tasks.push(EncodeTask::Finish(Box::new(move |out, _| {
                    let mut w = Writer { out, first: false };
                    write_attrs_field(&mut w, &n.attrs);
                    write_pos_field(&mut w, &n.pos);
                    w.finish();
                })));
                push_array(&mut tasks, &n.children, |node| {
                    EncodeTask::Inline(node, depth + 1)
                });
            }
            EncodeTask::DefinitionDef(n, depth) => {
                let mut w = typed(out, "definition_description");
                w.field("children", |out| out.push('['));
                tasks.push(EncodeTask::Finish(Box::new(move |out, _| {
                    let mut w = Writer { out, first: false };
                    write_attrs_field(&mut w, &n.attrs);
                    write_pos_field(&mut w, &n.pos);
                    w.finish();
                })));
                push_array(&mut tasks, &n.children, |node| {
                    EncodeTask::Block(node, depth + 1)
                });
            }
            EncodeTask::FigureTarget(target, depth) => match target {
                FigureTarget::Image(n) => write_image(out, n),
                FigureTarget::CodeBlock(n) => write_code_block(out, n),
                FigureTarget::BlockQuote(n) => encode_block_quote_task(out, &mut tasks, n, depth),
                FigureTarget::Table(n) => encode_table_task(out, &mut tasks, n, depth),
                FigureTarget::Paragraph(n) => encode_paragraph_task(out, &mut tasks, n, depth),
            },
            EncodeTask::Citation(n, depth) => encode_citation_task(out, &mut tasks, n, depth),
        }
    }
}

pub(super) fn finish_attrs_pos<'a>(
    tasks: &mut Vec<EncodeTask<'a>>,
    attrs: &'a Option<Attrs>,
    pos: &'a Option<Pos>,
) {
    tasks.push(EncodeTask::Finish(Box::new(move |out, _| {
        let mut w = Writer { out, first: false };
        write_attrs_field(&mut w, attrs);
        write_pos_field(&mut w, pos);
        w.finish();
    })));
}

pub(super) fn encode_paragraph_task<'a>(
    out: &mut String,
    tasks: &mut Vec<EncodeTask<'a>>,
    n: &'a Paragraph,
    depth: usize,
) {
    let mut w = typed(out, "paragraph");
    // Written ONLY when true (PART 12 section 23): a paragraph that is not a
    // block image omits the field rather than carrying `false`, which is what
    // the schema's `const: true` says. Ahead of `children` so the streaming and
    // the direct writer below emit the same key order.
    if n.block_image {
        w.field("blockImage", |out| write_bool(out, true));
    }
    w.field("children", |out| out.push('['));
    finish_attrs_pos(tasks, &n.attrs, &n.pos);
    push_array(tasks, &n.children, |node| {
        EncodeTask::Inline(node, depth + 1)
    });
}

pub(super) fn encode_block_quote_task<'a>(
    out: &mut String,
    tasks: &mut Vec<EncodeTask<'a>>,
    n: &'a BlockQuote,
    depth: usize,
) {
    let mut w = typed(out, "block_quote");
    if n.fenced {
        w.field("fenced", |out| write_bool(out, true));
    }
    w.field("children", |out| out.push('['));
    finish_attrs_pos(tasks, &n.attrs, &n.pos);
    push_array(tasks, &n.children, |node| {
        EncodeTask::Block(node, depth + 1)
    });
}

pub(super) fn encode_block_task<'a>(
    out: &mut String,
    tasks: &mut Vec<EncodeTask<'a>>,
    node: &'a BlockNode,
    depth: usize,
) {
    match node {
        BlockNode::Heading(n) => {
            let mut w = typed(out, "heading");
            w.field("level", |out| write_usize(out, n.level as usize));
            w.field("children", |out| out.push('['));
            finish_attrs_pos(tasks, &n.attrs, &n.pos);
            push_array(tasks, &n.children, |node| {
                EncodeTask::Inline(node, depth + 1)
            });
        }
        BlockNode::Paragraph(n) => encode_paragraph_task(out, tasks, n, depth),
        BlockNode::List(n) => {
            let mut w = typed(out, "list");
            w.field("ordered", |out| write_bool(out, n.ordered));
            w.field("tight", |out| write_bool(out, n.effective_tight()));
            w.field("items", |out| out.push('['));
            tasks.push(EncodeTask::Finish(Box::new(move |out, _| {
                let mut w = Writer { out, first: false };
                // PART 12 §22: 1 IS THE VALUE THE SCHEMA CALLS ABSENT, so an
                // ingested tree that spells it out is normalized rather than
                // re-emitted. `resources/ast-schema.json` documents the field as
                // "First number of an ordered list, WHEN IT IS NOT 1", which is
                // a statement about a conformant tree, and the encoder is what
                // honors it. A parse never reaches here with 1, so the only
                // source is a hand-built payload, a patch tool or an editor.
                // §6's round trip is scoped to `parse(x)` and gives no cover to
                // preserving it. NOT "drop `start` always": 0 and 2 ride through
                // unchanged, and dropping those breaks the field instead.
                if let Some(start) = n.start.filter(|start| *start != 1) {
                    w.field("start", |out| write_usize(out, start));
                }
                if let Some(ol_type) = n.ol_type {
                    w.field("olType", |out| write_string(out, ol_type_json(ol_type)));
                }
                if let Some(delim) = n.delim {
                    w.field("delim", |out| write_string(out, &delim.to_string()));
                }
                if let Some(bullet) = n.bullet_char {
                    w.field("bulletChar", |out| write_string(out, &bullet.to_string()));
                }
                if n.bare_marker {
                    w.field("bareMarker", |out| out.push_str("true"));
                }
                write_attrs_field(&mut w, &n.attrs);
                write_pos_field(&mut w, &n.pos);
                w.finish();
            })));
            push_array(tasks, &n.items, |item| EncodeTask::ListItem(item, depth));
        }
        BlockNode::BlockQuote(n) => encode_block_quote_task(out, tasks, n, depth),
        BlockNode::Table(n) => encode_table_task(out, tasks, n, depth),
        BlockNode::Admonition(n) => {
            let mut w = typed(out, "admonition");
            w.field("kind", |out| write_string(out, &n.kind));
            if let Some(title) = &n.title {
                w.field("title", |out| out.push('['));
                tasks.push(EncodeTask::Finish(Box::new(move |out, tasks| {
                    let mut w = Writer { out, first: false };
                    if let Some(label) = &n.label {
                        w.field("label", |out| write_string(out, label));
                    }
                    w.field("children", |out| out.push('['));
                    finish_attrs_pos(tasks, &n.attrs, &n.pos);
                    push_array(tasks, &n.children, |node| {
                        EncodeTask::Block(node, depth + 1)
                    });
                })));
                push_array(tasks, title, |node| EncodeTask::Inline(node, depth + 1));
            } else {
                if let Some(label) = &n.label {
                    w.field("label", |out| write_string(out, label));
                }
                w.field("children", |out| out.push('['));
                finish_attrs_pos(tasks, &n.attrs, &n.pos);
                push_array(tasks, &n.children, |node| {
                    EncodeTask::Block(node, depth + 1)
                });
            }
        }
        BlockNode::Directive(n) => {
            let mut w = typed(out, "directive");
            w.field("kind", |out| write_string(out, &n.kind));
            if let Some(title) = &n.title {
                w.field("title", |out| out.push('['));
                tasks.push(EncodeTask::Finish(Box::new(move |out, tasks| {
                    let mut w = Writer { out, first: false };
                    if let Some(label) = &n.label {
                        w.field("label", |out| write_string(out, label));
                    }
                    w.field("children", |out| out.push('['));
                    finish_attrs_pos(tasks, &n.attrs, &n.pos);
                    push_array(tasks, &n.children, |node| {
                        EncodeTask::Block(node, depth + 1)
                    });
                })));
                push_array(tasks, title, |node| EncodeTask::Inline(node, depth + 1));
            } else {
                w.field("children", |out| out.push('['));
                tasks.push(EncodeTask::Finish(Box::new(move |out, _| {
                    let mut w = Writer { out, first: false };
                    if let Some(label) = &n.label {
                        w.field("label", |out| write_string(out, label));
                    }
                    write_attrs_field(&mut w, &n.attrs);
                    write_pos_field(&mut w, &n.pos);
                    w.finish();
                })));
                push_array(tasks, &n.children, |node| {
                    EncodeTask::Block(node, depth + 1)
                });
            }
        }
        BlockNode::Div(n) => {
            let mut w = typed(out, "div");
            w.field("children", |out| out.push('['));
            tasks.push(EncodeTask::Finish(Box::new(move |out, _| {
                let mut w = Writer { out, first: false };
                if let Some(label) = &n.label {
                    w.field("label", |out| write_string(out, label));
                }
                write_attrs_field(&mut w, &n.attrs);
                write_pos_field(&mut w, &n.pos);
                w.finish();
            })));
            push_array(tasks, &n.children, |node| {
                EncodeTask::Block(node, depth + 1)
            });
        }
        BlockNode::Section(n) => {
            let mut w = typed(out, "section");
            w.field("children", |out| out.push('['));
            tasks.push(EncodeTask::Finish(Box::new(move |out, _| {
                let mut w = Writer { out, first: false };
                if let Some(level) = n.level {
                    w.field("level", |out| write_usize(out, level as usize));
                }
                write_attrs_field(&mut w, &n.attrs);
                write_pos_field(&mut w, &n.pos);
                w.finish();
            })));
            push_array(tasks, &n.children, |node| {
                EncodeTask::Block(node, depth + 1)
            });
        }
        BlockNode::LineBlock(n) => {
            let mut w = typed(out, "line_block");
            w.field("children", |out| out.push('['));
            tasks.push(EncodeTask::Finish(Box::new(move |out, _| {
                let mut w = Writer { out, first: false };
                if let Some(lines) = &n.lines {
                    w.field("lines", |out| {
                        write_array(out, lines, |out, line| write_string_array(out, line))
                    });
                }
                write_attrs_field(&mut w, &n.attrs);
                write_pos_field(&mut w, &n.pos);
                w.finish();
            })));
            push_array(tasks, &n.children, |node| {
                EncodeTask::Block(node, depth + 1)
            });
        }
        BlockNode::DefinitionList(n) => {
            let mut w = typed(out, "definition_list");
            w.field("items", |out| out.push('['));
            finish_attrs_pos(tasks, &n.attrs, &n.pos);
            if n.loose {
                // PART 12 §8 publishes the field only when true, the same way
                // `list.bareMarker` rides the wire: absent is the derived
                // wrapper every other definition list gets, and only the SPELLED
                // fact is underivable from the tree.
                //
                // AS A DEFERRED TASK, not a `w.field` here: `items` is still an
                // OPEN array at this point - its entries are the tasks pushed
                // below - so anything written now lands INSIDE it. The task
                // stack pops in reverse, so pushing between `finish_attrs_pos`
                // and the `]` puts the field after the array closes and before
                // `attrs`/`pos`, which is the schema's own order.
                tasks.push(EncodeTask::Finish(Box::new(|out, _| {
                    let mut w = Writer { out, first: false };
                    w.field("loose", |out| out.push_str("true"));
                })));
            }
            tasks.push(EncodeTask::Char(']'));
            let mut entries = Vec::new();
            for item in &n.items {
                entries.extend(
                    item.terms
                        .iter()
                        .map(|term| EncodeTask::DefinitionTerm(term, depth)),
                );
                entries.extend(
                    item.definitions
                        .iter()
                        .map(|def| EncodeTask::DefinitionDef(def, depth)),
                );
            }
            for (index, entry) in entries.into_iter().enumerate().rev() {
                tasks.push(entry);
                if index != 0 {
                    tasks.push(EncodeTask::Char(','));
                }
            }
        }
        BlockNode::Figure(n) => {
            let mut w = typed(out, "figure");
            w.field("target", |_| {});
            tasks.push(EncodeTask::Finish(Box::new(move |out, tasks| {
                let mut w = Writer { out, first: false };
                w.field("caption", |out| out.push('['));
                tasks.push(EncodeTask::Finish(Box::new(move |out, tasks| {
                    let mut w = Writer { out, first: false };
                    if let Some(short) = &n.short_caption {
                        w.field("shortCaption", |out| out.push('['));
                        finish_attrs_pos(tasks, &n.attrs, &n.pos);
                        push_array(tasks, short, |node| EncodeTask::Inline(node, depth + 1));
                    } else {
                        write_attrs_field(&mut w, &n.attrs);
                        write_pos_field(&mut w, &n.pos);
                        w.finish();
                    }
                })));
                push_array(tasks, &n.caption, |node| {
                    EncodeTask::Inline(node, depth + 1)
                });
            })));
            tasks.push(EncodeTask::FigureTarget(&n.target, depth));
        }
        BlockNode::FigureGroup(n) => {
            let mut w = typed(out, "figure_group");
            w.field("children", |out| out.push('['));
            tasks.push(EncodeTask::Finish(Box::new(move |out, tasks| {
                let mut w = Writer { out, first: false };
                if let Some(caption) = &n.caption {
                    w.field("caption", |out| out.push('['));
                    finish_attrs_pos(tasks, &n.attrs, &n.pos);
                    push_array(tasks, caption, |node| EncodeTask::Inline(node, depth + 1));
                } else {
                    write_attrs_field(&mut w, &n.attrs);
                    write_pos_field(&mut w, &n.pos);
                    w.finish();
                }
            })));
            push_array(tasks, &n.children, |node| {
                EncodeTask::Block(node, depth + 1)
            });
        }
        BlockNode::CitationDefinition(n) => {
            let mut w = typed(out, "citation_definition");
            w.field("key", |out| write_string(out, &n.key));
            w.field("children", |out| out.push('['));
            finish_attrs_pos(tasks, &n.attrs, &n.pos);
            push_array(tasks, &n.children, |node| {
                EncodeTask::Inline(node, depth + 1)
            });
        }
        BlockNode::BlockExtension(n) => {
            let mut w = typed(out, "block_extension");
            w.field("name", |out| write_string(out, &n.name));
            if let Some(version) = &n.version {
                w.field("version", |out| write_string(out, version));
            }
            w.field("fallback", |_| {});
            tasks.push(EncodeTask::Finish(Box::new(move |out, _| {
                let mut w = Writer { out, first: false };
                write_payload_field(&mut w, &n.payload);
                write_attrs_field(&mut w, &n.attrs);
                write_pos_field(&mut w, &n.pos);
                w.finish();
            })));
            tasks.push(EncodeTask::Block(n.fallback.as_ref(), depth + 1));
        }
        // The render-stage carrier is published as the `div` it degrades to on
        // every other target, NOT under the schema's `block_extension` - that
        // name belongs to CARVE-P12-055, and sharing it meant the engine could
        // read back neither node (carve-rs#1865). Its `name` and `summary` have
        // no home on a div, which is the same loss the canonical Carve writer
        // already takes.
        BlockNode::ExtensionCarrier(n) => {
            let mut w = typed(out, "div");
            w.field("children", |out| out.push('['));
            tasks.push(EncodeTask::Finish(Box::new(move |out, _| {
                let mut w = Writer { out, first: false };
                if let Some(label) = &n.label {
                    w.field("label", |out| write_string(out, label));
                }
                write_attrs_field(&mut w, &n.attrs);
                write_pos_field(&mut w, &n.pos);
                w.finish();
            })));
            push_array(tasks, &n.children, |node| {
                EncodeTask::Block(node, depth + 1)
            });
        }
        _ => write_block_leaf(out, node),
    }
}

pub(super) fn encode_table_task<'a>(
    out: &mut String,
    tasks: &mut Vec<EncodeTask<'a>>,
    n: &'a Table,
    depth: usize,
) {
    let mut w = typed(out, "table");
    w.field("rows", |out| out.push('['));
    tasks.push(EncodeTask::Finish(Box::new(move |out, tasks| {
        let mut w = Writer { out, first: false };
        let derived;
        let columns = if n.columns.is_empty() {
            derived = columns_from_table_attrs(n.attrs.as_ref());
            &derived
        } else {
            &n.columns
        };
        if !columns.is_empty() {
            w.field("columns", |out| {
                write_array(out, columns, write_table_column)
            });
        }
        write_separator_widths(&mut w, n);
        if let Some(caption) = &n.caption {
            w.field("caption", |out| out.push('['));
            tasks.push(EncodeTask::Finish(Box::new(move |out, tasks| {
                let mut w = Writer { out, first: false };
                if let Some(short) = &n.short_caption {
                    w.field("shortCaption", |out| out.push('['));
                    tasks.push(EncodeTask::Finish(Box::new(move |out, _| {
                        let mut w = Writer { out, first: false };
                        if let Some(groups) = &n.row_groups {
                            w.field("rowGroups", |out| write_row_groups(out, groups));
                        }
                        if let Some(format) = &n.source_format {
                            w.field("sourceFormat", |out| write_string(out, format.as_str()));
                        }
                        write_attrs_field(&mut w, &n.attrs);
                        write_pos_field(&mut w, &n.pos);
                        w.finish();
                    })));
                    push_array(tasks, short, |node| EncodeTask::Inline(node, depth + 1));
                } else {
                    if let Some(groups) = &n.row_groups {
                        w.field("rowGroups", |out| write_row_groups(out, groups));
                    }
                    if let Some(format) = &n.source_format {
                        w.field("sourceFormat", |out| write_string(out, format.as_str()));
                    }
                    write_attrs_field(&mut w, &n.attrs);
                    write_pos_field(&mut w, &n.pos);
                    w.finish();
                }
            })));
            push_array(tasks, caption, |node| EncodeTask::Inline(node, depth + 1));
        } else if let Some(short) = &n.short_caption {
            w.field("shortCaption", |out| out.push('['));
            tasks.push(EncodeTask::Finish(Box::new(move |out, _| {
                let mut w = Writer { out, first: false };
                if let Some(groups) = &n.row_groups {
                    w.field("rowGroups", |out| write_row_groups(out, groups));
                }
                if let Some(format) = &n.source_format {
                    w.field("sourceFormat", |out| write_string(out, format.as_str()));
                }
                write_attrs_field(&mut w, &n.attrs);
                write_pos_field(&mut w, &n.pos);
                w.finish();
            })));
            push_array(tasks, short, |node| EncodeTask::Inline(node, depth + 1));
        } else {
            if let Some(groups) = &n.row_groups {
                w.field("rowGroups", |out| write_row_groups(out, groups));
            }
            if let Some(format) = &n.source_format {
                w.field("sourceFormat", |out| write_string(out, format.as_str()));
            }
            write_attrs_field(&mut w, &n.attrs);
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
    })));
    push_array(tasks, &n.rows, |row| EncodeTask::TableRow(row, depth));
}

pub(super) fn encode_inline_task<'a>(
    out: &mut String,
    tasks: &mut Vec<EncodeTask<'a>>,
    node: &'a InlineNode,
    depth: usize,
) {
    match node {
        InlineNode::Emphasis(n) => {
            let mut w = typed(out, emphasis_type(n.kind));
            if n.kind == EmphasisKind::BoldItalic {
                w.field("children", |out| {
                    out.push('[');
                    let mut inner = typed(out, "emphasis");
                    inner.field("children", |out| out.push('['));
                });
                let inner_pos = n.pos.as_ref().filter(|_| n.attrs.is_none()).map(|p| Pos {
                    start_line: p.start_line,
                    end_line: p.end_line,
                    start_column: p.start_column + 2,
                    end_column: p.end_column.saturating_sub(2),
                    start_offset: p.start_offset + 2,
                    end_offset: p.end_offset.saturating_sub(2),
                    file: None,
                });
                tasks.push(EncodeTask::Finish(Box::new(move |out, _| {
                    let mut inner = Writer { out, first: false };
                    write_pos_field(&mut inner, &inner_pos);
                    inner.finish();
                    out.push(']');
                    let mut w = Writer { out, first: false };
                    w.field("boldItalic", |out| write_bool(out, true));
                    write_attrs_field(&mut w, &n.attrs);
                    write_pos_field(&mut w, &n.pos);
                    w.finish();
                })));
            } else {
                w.field("children", |out| out.push('['));
                finish_attrs_pos(tasks, &n.attrs, &n.pos);
            }
            push_array(tasks, &n.children, |node| {
                EncodeTask::Inline(node, depth + 1)
            });
        }
        InlineNode::Link(n) => {
            let mut w = typed(out, "link");
            w.field("href", |out| write_string(out, &n.href));
            w.field("children", |out| out.push('['));
            tasks.push(EncodeTask::Finish(Box::new(move |out, _| {
                let mut w = Writer { out, first: false };
                if let Some(title) = &n.title {
                    w.field("title", |out| write_string(out, title));
                }
                if let Some(label) = &n.ref_label {
                    w.field("ref", |out| write_string(out, label));
                }
                if n.ref_label.is_some() && n.href.is_empty() && n.resolved_empty_reference {
                    w.field("resolved", |out| write_bool(out, true));
                }
                if let Some(raw) = &n.raw_ref {
                    w.field("rawRef", |out| write_string(out, raw));
                }
                write_attrs_field(&mut w, &n.attrs);
                write_pos_field(&mut w, &n.pos);
                w.finish();
            })));
            push_array(tasks, &n.children, |node| {
                EncodeTask::Inline(node, depth + 1)
            });
        }
        InlineNode::Span(n) => {
            let mut w = typed(out, "span");
            w.field("children", |out| out.push('['));
            finish_attrs_pos(tasks, &n.attrs, &n.pos);
            push_array(tasks, &n.children, |node| {
                EncodeTask::Inline(node, depth + 1)
            });
        }
        InlineNode::Ruby(n) => {
            let mut w = typed(out, "ruby");
            w.field("pairs", |out| out.push('['));
            finish_attrs_pos(tasks, &n.attrs, &n.pos);
            push_array(tasks, &n.pairs, |pair| {
                EncodeTask::RubyPair(pair, depth + 1)
            });
        }
        InlineNode::Extension(n) => {
            let mut w = typed(out, "inline_extension");
            w.field("name", |out| write_string(out, &n.name));
            w.field("content", |out| out.push('['));
            finish_attrs_pos(tasks, &n.attrs, &n.pos);
            push_array(tasks, &n.children, |node| {
                EncodeTask::Inline(node, depth + 1)
            });
        }
        InlineNode::Footnote(n) if n.inline.is_some() => {
            let inline = n.inline.as_ref().unwrap();
            let mut w = typed(out, "inline_footnote");
            w.field("inline", |out| out.push('['));
            tasks.push(EncodeTask::Finish(Box::new(move |out, _| {
                let mut w = Writer { out, first: false };
                if let Some(number) = n.number {
                    w.field("number", |out| write_usize(out, number));
                }
                write_attrs_field(&mut w, &n.attrs);
                write_pos_field(&mut w, &n.pos);
                w.finish();
            })));
            push_array(tasks, inline, |node| EncodeTask::Inline(node, depth + 1));
        }
        InlineNode::CriticInsert(n) => {
            let mut w = typed(out, "insert");
            w.field("children", |out| out.push('['));
            finish_attrs_pos(tasks, &n.attrs, &n.pos);
            push_array(tasks, &n.children, |node| {
                EncodeTask::Inline(node, depth + 1)
            });
        }
        InlineNode::CriticDelete(n) => {
            let mut w = typed(out, "delete");
            w.field("children", |out| out.push('['));
            finish_attrs_pos(tasks, &n.attrs, &n.pos);
            push_array(tasks, &n.children, |node| {
                EncodeTask::Inline(node, depth + 1)
            });
        }
        InlineNode::CitationGroup(n) => {
            let mut w = typed(out, "citation_group");
            w.field("items", |out| out.push('['));
            tasks.push(EncodeTask::Finish(Box::new(move |out, _| {
                let mut w = Writer { out, first: false };
                w.field("raw", |out| write_string(out, &n.raw));
                if n.integral() {
                    w.field("mode", |out| write_string(out, "integral"));
                }
                write_pos_field(&mut w, &n.pos);
                w.finish();
            })));
            push_array(tasks, &n.items, |citation| {
                EncodeTask::Citation(citation, depth)
            });
        }
        InlineNode::CriticSubstitute(n) => {
            let mut w = typed(out, "substitution");
            w.field("old", |out| out.push('['));
            let pos = &n.pos;
            let new_half = &n.new;
            tasks.push(EncodeTask::Finish(Box::new(move |out, tasks| {
                let mut w = Writer { out, first: false };
                w.field("new", |out| out.push('['));
                tasks.push(EncodeTask::Finish(Box::new(move |out, _| {
                    let mut w = Writer { out, first: false };
                    write_pos_field(&mut w, pos);
                    w.finish();
                })));
                push_array(tasks, new_half, |node| EncodeTask::Inline(node, depth + 1));
            })));
            push_array(tasks, &n.old, |node| EncodeTask::Inline(node, depth + 1));
        }
        _ => write_inline_leaf(out, node),
    }
}

pub(super) fn encode_citation_task<'a>(
    out: &mut String,
    tasks: &mut Vec<EncodeTask<'a>>,
    n: &'a Citation,
    depth: usize,
) {
    let mut w = typed(out, "citation");
    w.field("key", |out| write_string(out, &n.key));
    encode_citation_inline_fields(out, tasks, n, depth, 0);
}

pub(super) fn encode_citation_inline_fields<'a>(
    out: &mut String,
    tasks: &mut Vec<EncodeTask<'a>>,
    n: &'a Citation,
    depth: usize,
    stage: usize,
) {
    let (name, values) = match stage {
        0 => ("prefix", n.prefix.as_deref()),
        1 => ("locator", n.locator.as_deref()),
        2 => {
            let mut w = Writer { out, first: false };
            if let Some(label) = &n.locator_label {
                w.field("locatorLabel", |out| write_string(out, label));
            }
            if let Some(value) = &n.locator_value {
                w.field("locatorValue", |out| write_string(out, value));
            }
            return encode_citation_inline_fields(out, tasks, n, depth, 3);
        }
        3 => ("suffix", n.suffix.as_deref()),
        _ => {
            let mut w = Writer { out, first: false };
            w.field("suppressAuthor", |out| write_bool(out, n.suppress_author));
            if let Some(number) = n.number {
                w.field("number", |out| write_usize(out, number));
            }
            if let Some(index) = n.use_index {
                w.field("useIndex", |out| write_usize(out, index));
            }
            if n.mode == Some(CitationItemMode::Integral) {
                w.field("mode", |out| write_string(out, "integral"));
            }
            write_pos_field(&mut w, &n.pos);
            w.finish();
            return;
        }
    };
    if let Some(values) = values {
        let mut w = Writer { out, first: false };
        w.field(name, |out| out.push('['));
        tasks.push(EncodeTask::Finish(Box::new(move |out, tasks| {
            encode_citation_inline_fields(out, tasks, n, depth, stage + 1);
        })));
        push_array(tasks, values, |node| EncodeTask::Inline(node, depth + 1));
    } else {
        encode_citation_inline_fields(out, tasks, n, depth, stage + 1);
    }
}

pub(super) fn write_block_leaf(out: &mut String, node: &BlockNode) {
    match node {
        BlockNode::Heading(n) => {
            let mut w = typed(out, "heading");
            w.field("level", |out| write_usize(out, n.level as usize));
            w.field("children", |out| write_inlines(out, &n.children));
            write_attrs_field(&mut w, &n.attrs);
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        BlockNode::Paragraph(n) => write_paragraph(out, n),
        BlockNode::CodeBlock(n) => write_code_block(out, n),
        BlockNode::List(n) => {
            let mut w = typed(out, "list");
            w.field("ordered", |out| write_bool(out, n.ordered));
            w.field("tight", |out| write_bool(out, n.effective_tight()));
            w.field("items", |out| write_array(out, &n.items, write_list_item));
            // Same normalization as the task encoder above (PART 12 §22). Kept
            // in step deliberately: this arm is only reachable if `List` ever
            // stops being matched in `encode_block_task`, and a spelling of one
            // rule that drifts from the other is how three engines diverged
            // here in the first place.
            if let Some(start) = n.start.filter(|start| *start != 1) {
                w.field("start", |out| write_usize(out, start));
            }
            if let Some(ol_type) = n.ol_type {
                w.field("olType", |out| write_string(out, ol_type_json(ol_type)));
            }
            if let Some(delim) = n.delim {
                w.field("delim", |out| write_string(out, &delim.to_string()));
            }
            if let Some(bullet) = n.bullet_char {
                w.field("bulletChar", |out| write_string(out, &bullet.to_string()));
            }
            // Author choice like `delim` and `bulletChar`, so it rides the wire
            // beside them (PART 12 §3, carve#480). Absent at the default, which
            // is why this is `true`-only rather than a boolean field: without it
            // `. a` decoded as `1. a` and no engine could do better, because
            // none of them had anywhere to put the distinction.
            if n.bare_marker {
                w.field("bareMarker", |out| out.push_str("true"));
            }
            write_attrs_field(&mut w, &n.attrs);
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        BlockNode::BlockQuote(n) => write_block_quote(out, n),
        BlockNode::Table(n) => write_table(out, n),
        BlockNode::Admonition(n) => {
            let mut w = typed(out, "admonition");
            w.field("kind", |out| write_string(out, &n.kind));
            if let Some(title) = &n.title {
                w.field("title", |out| write_inlines(out, title));
            }
            if let Some(label) = &n.label {
                w.field("label", |out| write_string(out, label));
            }
            w.field("children", |out| write_blocks(out, &n.children));
            write_attrs_field(&mut w, &n.attrs);
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        BlockNode::Directive(n) => {
            let mut w = typed(out, "directive");
            w.field("kind", |out| write_string(out, &n.kind));
            if let Some(title) = &n.title {
                w.field("title", |out| write_inlines(out, title));
            }
            if let Some(label) = &n.label {
                w.field("label", |out| write_string(out, label));
            }
            w.field("children", |out| write_blocks(out, &n.children));
            write_attrs_field(&mut w, &n.attrs);
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        BlockNode::Div(n) => {
            let mut w = typed(out, "div");
            w.field("children", |out| write_blocks(out, &n.children));
            if let Some(label) = &n.label {
                w.field("label", |out| write_string(out, label));
            }
            write_attrs_field(&mut w, &n.attrs);
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        BlockNode::Section(n) => {
            let mut w = typed(out, "section");
            w.field("children", |out| write_blocks(out, &n.children));
            if let Some(level) = n.level {
                w.field("level", |out| write_usize(out, level as usize));
            }
            write_attrs_field(&mut w, &n.attrs);
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        BlockNode::LineBlock(n) => {
            let mut w = typed(out, "line_block");
            w.field("children", |out| write_blocks(out, &n.children));
            if let Some(lines) = &n.lines {
                w.field("lines", |out| {
                    write_array(out, lines, |out, line| write_string_array(out, line))
                });
            }
            write_attrs_field(&mut w, &n.attrs);
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        BlockNode::DefinitionList(n) => {
            let mut w = typed(out, "definition_list");
            w.field("items", |out| write_definition_entries(out, &n.items));
            if n.loose {
                w.field("loose", |out| out.push_str("true"));
            }
            write_attrs_field(&mut w, &n.attrs);
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        BlockNode::Figure(n) => {
            let mut w = typed(out, "figure");
            w.field("target", |out| write_figure_target(out, &n.target));
            w.field("caption", |out| write_inlines(out, &n.caption));
            if let Some(short_caption) = &n.short_caption {
                w.field("shortCaption", |out| write_inlines(out, short_caption));
            }
            write_attrs_field(&mut w, &n.attrs);
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        BlockNode::FigureGroup(n) => {
            // PART 12 §16: `children` in source order (a consumer derives the
            // panel list by type, the way the renderer does - there is no
            // second `panels` key to disagree with them), `caption` only when
            // the closer hosted one - absent means uncaptioned, never an
            // empty placeholder.
            let mut w = typed(out, "figure_group");
            w.field("children", |out| write_blocks(out, &n.children));
            if let Some(caption) = &n.caption {
                w.field("caption", |out| write_inlines(out, caption));
            }
            write_attrs_field(&mut w, &n.attrs);
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        BlockNode::LinkReferenceDefinition(n) => {
            // PART 12 §10: `label` and `href` are required, `title` and `attrs`
            // ride along when the definition line carried them.
            let mut w = typed(out, "link_reference_definition");
            w.field("label", |out| write_string(out, &n.label));
            w.field("href", |out| write_string(out, &n.href));
            if let Some(title) = &n.title {
                w.field("title", |out| write_string(out, title));
            }
            write_attrs_field(&mut w, &n.attrs);
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        BlockNode::CitationDefinition(n) => {
            // PART 12 section 18: `key` and `children` are required, `attrs`
            // rides along when the definition line carried a metadata block.
            // Shaped after section 10's link reference definition, so the entry
            // is INLINE content - a footnote body holds blocks and this does
            // not.
            let mut w = typed(out, "citation_definition");
            w.field("key", |out| write_string(out, &n.key));
            w.field("children", |out| write_inlines(out, &n.children));
            write_attrs_field(&mut w, &n.attrs);
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        BlockNode::AbbreviationDef(n) => {
            let mut w = typed(out, "abbreviation_def");
            w.field("abbr", |out| write_string(out, &n.abbr));
            w.field("expansion", |out| write_string(out, &n.expansion));
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        BlockNode::RawBlock(n) => {
            let mut w = typed(out, "raw_block");
            w.field("format", |out| write_string(out, &n.format));
            w.field("content", |out| write_string(out, &n.content));
            write_attrs_field(&mut w, &n.attrs);
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        BlockNode::Comment(n) => {
            let mut w = typed(out, "comment");
            w.field("block", |out| write_bool(out, n.block));
            if n.delimited {
                w.field("delimited", |out| write_bool(out, true));
            }
            w.field("form", |out| write_string(out, comment_form(n)));
            w.field("content", |out| write_string(out, &n.content));
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        BlockNode::BlockExtension(n) => {
            let mut w = typed(out, "block_extension");
            w.field("name", |out| write_string(out, &n.name));
            if let Some(version) = &n.version {
                w.field("version", |out| write_string(out, version));
            }
            w.field("fallback", |out| write_block(out, &n.fallback));
            write_payload_field(&mut w, &n.payload);
            write_attrs_field(&mut w, &n.attrs);
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        // See the arm in `encode_block_task`: the carrier publishes as a div.
        BlockNode::ExtensionCarrier(n) => {
            let mut w = typed(out, "div");
            w.field("children", |out| write_blocks(out, &n.children));
            if let Some(label) = &n.label {
                w.field("label", |out| write_string(out, label));
            }
            write_attrs_field(&mut w, &n.attrs);
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        BlockNode::BlockImage(n) => write_image(out, n),
        BlockNode::ThematicBreak(n) => {
            let mut w = typed(out, "thematic_break");
            if let Some(marker @ ('*' | '_')) = n.marker {
                w.field("marker", |out| write_string(out, &marker.to_string()));
            }
            write_attrs_field(&mut w, &n.attrs);
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
    }
}

pub(super) fn write_paragraph(out: &mut String, n: &Paragraph) {
    let mut w = typed(out, "paragraph");
    if n.block_image {
        w.field("blockImage", |out| write_bool(out, true));
    }
    w.field("children", |out| write_inlines(out, &n.children));
    write_attrs_field(&mut w, &n.attrs);
    write_pos_field(&mut w, &n.pos);
    w.finish();
}

pub(super) fn write_code_block(out: &mut String, n: &CodeBlock) {
    let mut w = typed(out, "code_block");
    w.field("content", |out| write_string(out, &n.content));
    if let Some(lang) = &n.lang {
        w.field("lang", |out| write_string(out, lang));
    }
    if let Some(title) = &n.title {
        w.field("header", |out| write_string(out, title));
    }
    if let Some(label) = &n.label {
        w.field("label", |out| write_string(out, label));
    }
    write_attrs_field(&mut w, &n.attrs);
    write_pos_field(&mut w, &n.pos);
    w.finish();
}

pub(super) fn write_block_quote(out: &mut String, n: &BlockQuote) {
    let mut w = typed(out, "block_quote");
    w.field("children", |out| write_blocks(out, &n.children));
    // Only ever written when true, and the schema pins it to `true`, so a
    // prefixed quote's payload is byte for byte what it was before the fenced
    // spelling existed (markup-carve/carve#1718).
    if n.fenced {
        w.field("fenced", |out| write_bool(out, true));
    }
    // The ordinary attribute slot, like every other node's (PART 12 §3). This
    // cited a "PART 9 §4a" for the source of the quotation: PART 9 has 4b and 4c
    // and no 4a, and what this line writes is the attrs slot rather than an
    // attribution of any kind.
    write_attrs_field(&mut w, &n.attrs);
    write_pos_field(&mut w, &n.pos);
    w.finish();
}

pub(super) fn write_list_item(out: &mut String, n: &ListItem) {
    let mut w = typed(out, "list_item");
    w.field("children", |out| write_blocks(out, &n.children));
    if let Some(checked) = n.checked {
        w.field("checked", |out| write_bool(out, checked));
    }
    if let Some(state) = n.task_state {
        w.field("taskState", |out| write_string(out, &state.to_string()));
    }
    write_attrs_field(&mut w, &n.attrs);
    write_pos_field(&mut w, &n.pos);
    w.finish();
}

pub(super) fn write_table(out: &mut String, n: &Table) {
    let mut w = typed(out, "table");
    w.field("rows", |out| write_array(out, &n.rows, write_table_row));
    let derived;
    let columns = if n.columns.is_empty() {
        derived = columns_from_table_attrs(n.attrs.as_ref());
        &derived
    } else {
        &n.columns
    };
    if !columns.is_empty() {
        w.field("columns", |out| {
            write_array(out, columns, write_table_column)
        });
    }
    write_separator_widths(&mut w, n);
    if let Some(caption) = &n.caption {
        w.field("caption", |out| write_inlines(out, caption));
    }
    if let Some(short_caption) = &n.short_caption {
        w.field("shortCaption", |out| write_inlines(out, short_caption));
    }
    if let Some(groups) = &n.row_groups {
        w.field("rowGroups", |out| write_row_groups(out, groups));
    }
    if let Some(format) = &n.source_format {
        w.field("sourceFormat", |out| write_string(out, format.as_str()));
    }
    write_attrs_field(&mut w, &n.attrs);
    write_pos_field(&mut w, &n.pos);
    w.finish();
}

/// PART 12 §3's author-choice field records separator spelling independently
/// of the table rows. The receiving schema validates its nonempty entries.
pub(super) fn write_separator_widths(w: &mut Writer<'_>, n: &Table) {
    let Some(widths) = &n.separator_widths else {
        return;
    };
    w.field("separatorWidths", |out| {
        out.push('[');
        for (index, width) in widths.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            out.push_str(&width.to_string());
        }
        out.push(']');
    });
}

pub(super) fn columns_from_table_attrs(attrs: Option<&Attrs>) -> Vec<TableColumn> {
    let values = |key| {
        attrs
            .and_then(|a| a.key_values.get(key))
            .map(|v| v.split(',').collect::<Vec<_>>())
            .unwrap_or_default()
    };
    let aligns = values("aligns");
    let valigns = values("valigns");
    let widths = values("widths");
    let len = aligns.len().max(valigns.len()).max(widths.len());
    (0..len)
        .map(|i| TableColumn {
            align: aligns.get(i).and_then(|v| match *v {
                "left" => Some(TableAlign::Left),
                "right" => Some(TableAlign::Right),
                "center" => Some(TableAlign::Center),
                _ => None,
            }),
            valign: valigns.get(i).and_then(|v| match *v {
                "top" => Some(TableVerticalAlign::Top),
                "middle" => Some(TableVerticalAlign::Middle),
                "bottom" => Some(TableVerticalAlign::Bottom),
                _ => None,
            }),
            width: widths.get(i).and_then(|v| crate::table_width::fraction(v)),
        })
        .collect()
}

pub(super) fn write_table_column(out: &mut String, n: &TableColumn) {
    let mut w = Writer::new(out);
    if let Some(align) = n.align {
        w.field("align", |out| write_string(out, align_json(align)));
    }
    if let Some(valign) = n.valign {
        w.field("valign", |out| write_string(out, valign_json(valign)));
    }
    if let Some(width) = n.width {
        w.field("width", |out| out.push_str(&width.to_string()));
    }
    w.finish();
}

pub(super) fn write_row_groups(out: &mut String, n: &TableRowGroups) {
    let mut w = Writer::new(out);
    w.field("headRows", |out| write_usize(out, n.head_rows));
    w.field("bodies", |out| {
        write_array(out, &n.bodies, write_body_group)
    });
    w.field("footRows", |out| write_usize(out, n.foot_rows));
    if let Some(attrs) = &n.head_attrs {
        w.field("headAttrs", |out| write_attrs(out, attrs));
    }
    if let Some(attrs) = &n.foot_attrs {
        w.field("footAttrs", |out| write_attrs(out, attrs));
    }
    w.finish();
}

pub(super) fn write_body_group(out: &mut String, n: &TableBodyGroup) {
    let mut w = Writer::new(out);
    w.field("headRows", |out| write_usize(out, n.head_rows));
    w.field("bodyRows", |out| write_usize(out, n.body_rows));
    if let Some(columns) = n.row_head_columns {
        w.field("rowHeadColumns", |out| write_usize(out, columns));
    }
    write_attrs_field(&mut w, &n.attrs);
    w.finish();
}

pub(super) fn write_table_row(out: &mut String, n: &TableRow) {
    let mut w = typed(out, "table_row");
    w.field("cells", |out| write_array(out, &n.cells, write_table_cell));
    if let Some(lines) = n.continuation_lines {
        w.field("continuationLines", |out| write_usize(out, lines));
    }
    write_attrs_field(&mut w, &n.attrs);
    write_pos_field(&mut w, &n.pos);
    w.finish();
}

pub(super) fn write_table_cell(out: &mut String, n: &TableCell) {
    let mut w = typed(out, "table_cell");
    w.field("header", |out| write_bool(out, n.header));
    if let Some(blocks) = &n.blocks {
        w.field("blocks", |out| write_blocks(out, blocks));
    } else {
        w.field("children", |out| write_inlines(out, &n.children));
    }
    if let Some(span) = n.span {
        w.field("span", |out| {
            write_string(
                out,
                match span {
                    TableCellSpan::Rowspan => "rowspan",
                    TableCellSpan::Colspan => "colspan",
                },
            )
        });
    }
    if let Some(count) = n.colspan.filter(|count| *count != 1) {
        w.field("colspan", |out| write_usize(out, count));
    }
    if let Some(count) = n.rowspan.filter(|count| *count != 1) {
        w.field("rowspan", |out| write_usize(out, count));
    }
    if let Some(align) = n.align {
        w.field("align", |out| write_string(out, align_json(align)));
    }
    if let Some(valign) = n.valign {
        w.field("valign", |out| write_string(out, valign_json(valign)));
    }
    write_attrs_field(&mut w, &n.attrs);
    write_pos_field(&mut w, &n.pos);
    w.finish();
}

/// A definition list's entries, FLATTENED into the `<dt>` / `<dd>` sequence the
/// wire carries (PART 12).
///
/// This engine groups terms with their definitions, which is convenient in
/// memory and not something to publish: `definition_term` and
/// `definition_description` are in the normative block vocabulary, so a profile
/// can name them, and a plain `{terms, definitions}` object can carry no `pos` -
/// leaving a term the only content in a serialized document an editor cannot
/// navigate to.
///
/// The grouping was also not AGREED. Given `:: a` / `:: b` / `:  x` / `:  y`
/// this engine produced three entries and carve-js produced one, while both
/// rendered the same `<dl>`. A structure two producers disagree about, which no
/// output depends on, is an internal.
pub(super) fn write_definition_entries(out: &mut String, items: &[DefinitionItem]) {
    out.push('[');
    let mut first = true;
    for item in items {
        for term in &item.terms {
            write_comma(out, &mut first);
            let mut w = typed(out, "definition_term");
            w.field("children", |out| write_inlines(out, &term.children));
            write_attrs_field(&mut w, &term.attrs);
            write_pos_field(&mut w, &term.pos);
            w.finish();
        }
        for definition in &item.definitions {
            write_comma(out, &mut first);
            let mut w = typed(out, "definition_description");
            w.field("children", |out| write_blocks(out, &definition.children));
            write_attrs_field(&mut w, &definition.attrs);
            write_pos_field(&mut w, &definition.pos);
            w.finish();
        }
    }
    out.push(']');
}

pub(super) fn write_figure_target(out: &mut String, target: &FigureTarget) {
    match target {
        FigureTarget::Image(n) => write_image(out, n),
        FigureTarget::BlockQuote(n) => write_block_quote(out, n),
        FigureTarget::Table(n) => write_table(out, n),
        FigureTarget::CodeBlock(n) => write_code_block(out, n),
        FigureTarget::Paragraph(n) => write_paragraph(out, n),
    }
}

pub(super) fn write_inline(out: &mut String, node: &InlineNode) {
    run_encode_tasks(out, EncodeTask::Inline(node, 0));
}

pub(super) fn write_inline_leaf(out: &mut String, node: &InlineNode) {
    match node {
        InlineNode::Text(n) => {
            let mut w = typed(out, "text");
            w.field("value", |out| write_string(out, &n.value));
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        InlineNode::EscapedText(n) => {
            let mut w = typed(out, "escaped_text");
            w.field("value", |out| write_string(out, &n.value));
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        InlineNode::SmartPunctuation(n) => {
            let mut w = typed(out, "smart_punctuation");
            w.field("kind", |out| write_string(out, &n.kind));
            w.field("value", |out| write_string(out, &n.value));
            if let Some(glyph) = &n.glyph {
                w.field("glyph", |out| write_string(out, glyph));
            }
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        InlineNode::Emphasis(n) => {
            let mut w = typed(out, emphasis_type(n.kind));
            if n.kind == EmphasisKind::BoldItalic {
                let inner_pos = n.pos.as_ref().filter(|_| n.attrs.is_none()).map(|p| Pos {
                    start_line: p.start_line,
                    end_line: p.end_line,
                    start_column: p.start_column + 2,
                    end_column: p.end_column.saturating_sub(2),
                    start_offset: p.start_offset + 2,
                    end_offset: p.end_offset.saturating_sub(2),
                    file: None,
                });
                w.field("children", |out| {
                    out.push('[');
                    let mut inner = typed(out, "emphasis");
                    inner.field("children", |out| write_inlines(out, &n.children));
                    write_pos_field(&mut inner, &inner_pos);
                    inner.finish();
                    out.push(']');
                });
                w.field("boldItalic", |out| write_bool(out, true));
            } else {
                w.field("children", |out| write_inlines(out, &n.children));
            }
            write_attrs_field(&mut w, &n.attrs);
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        InlineNode::Code(n) => {
            let mut w = typed(out, "code");
            w.field("value", |out| write_string(out, &n.value));
            write_cell_wraps_field(&mut w, &n.cell_wraps);
            write_attrs_field(&mut w, &n.attrs);
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        InlineNode::Link(n) => write_link(out, n),
        InlineNode::Image(n) => write_image(out, n),
        InlineNode::Span(n) => {
            let mut w = typed(out, "span");
            w.field("children", |out| write_inlines(out, &n.children));
            write_attrs_field(&mut w, &n.attrs);
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        InlineNode::Math(n) => {
            let mut w = typed(out, "math");
            w.field("display", |out| write_bool(out, n.display));
            w.field("content", |out| write_string(out, &n.content));
            write_cell_wraps_field(&mut w, &n.cell_wraps);
            if let Some(label) = &n.label {
                w.field("label", |out| write_string(out, label));
            }
            if let Some(number) = n.number {
                w.field("number", |out| write_usize(out, number));
            }
            write_attrs_field(&mut w, &n.attrs);
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        InlineNode::RawInline(n) => {
            let mut w = typed(out, "raw_inline");
            w.field("format", |out| write_string(out, &n.format));
            w.field("content", |out| write_string(out, &n.content));
            write_cell_wraps_field(&mut w, &n.cell_wraps);
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        InlineNode::LiteralInline(n) => {
            let mut w = typed(out, "literal_inline");
            w.field("content", |out| write_string(out, &n.content));
            write_attrs_field(&mut w, &n.attrs);
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        InlineNode::Symbol(n) => {
            let mut w = typed(out, "symbol");
            w.field("name", |out| write_string(out, &n.name));
            write_attrs_field(&mut w, &n.attrs);
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        InlineNode::AutoLink(n) => {
            let mut w = typed(out, "autolink");
            w.field("href", |out| write_string(out, &n.href));
            w.field("text", |out| write_string(out, &n.text));
            write_attrs_field(&mut w, &n.attrs);
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        InlineNode::CrossRef(n) => {
            let mut w = typed(out, "heading_ref");
            // The authored construct and its resolution, side by side
            // (PART 12 section 3a). `href` is absent where the crossref
            // resolved against nothing, which is what says so.
            w.field("target", |out| write_string(out, &n.target));
            if let Some(href) = &n.href {
                w.field("href", |out| write_string(out, href));
            }
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        InlineNode::CaptionNumber(n) => {
            let mut w = typed(out, "caption_number");
            if let Some(number) = n.number {
                w.field("n", |out| write_usize(out, number));
            }
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        InlineNode::Mention(n) => {
            let mut w = typed(out, "mention");
            write_attrs_field(&mut w, &n.attrs);
            w.field("user", |out| write_string(out, &n.user));
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        InlineNode::Tag(n) => {
            let mut w = typed(out, "tag");
            write_attrs_field(&mut w, &n.attrs);
            w.field("name", |out| write_string(out, &n.name));
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        InlineNode::CitationGroup(n) => {
            let mut w = typed(out, "citation_group");
            w.field("items", |out| write_array(out, &n.items, write_citation));
            w.field("raw", |out| write_string(out, &n.raw));
            if n.integral() {
                w.field("mode", |out| write_string(out, "integral"));
            }
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        InlineNode::Extension(n) => {
            let mut w = typed(out, "inline_extension");
            w.field("name", |out| write_string(out, &n.name));
            w.field("content", |out| write_inlines(out, &n.children));
            write_attrs_field(&mut w, &n.attrs);
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        InlineNode::Abbreviation(n) => {
            let mut w = typed(out, "abbreviation");
            w.field("abbr", |out| write_string(out, &n.abbr));
            w.field("expansion", |out| write_string(out, &n.expansion));
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        InlineNode::Footnote(n) => {
            if let Some(inline) = &n.inline {
                let mut w = typed(out, "inline_footnote");
                w.field("inline", |out| write_inlines(out, inline));
                if let Some(number) = n.number {
                    w.field("number", |out| write_usize(out, number));
                }
                write_attrs_field(&mut w, &n.attrs);
                write_pos_field(&mut w, &n.pos);
                w.finish();
            } else {
                let mut w = typed(out, "footnote_ref");
                if let Some(id) = &n.id {
                    // `label` on the wire, the name the definition already uses
                    // (PART 12 §25, carve#2193). The runtime field stays `id`.
                    w.field("label", |out| write_string(out, id));
                }
                if let Some(number) = n.number {
                    w.field("number", |out| write_usize(out, number));
                }
                write_attrs_field(&mut w, &n.attrs);
                write_pos_field(&mut w, &n.pos);
                w.finish();
            }
        }
        InlineNode::NonBreakingSpace(n) => {
            let mut w = typed(out, "non_breaking_space");
            write_attrs_field(&mut w, &n.attrs);
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        InlineNode::SoftBreak(n) => {
            let mut w = typed(out, "soft_break");
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        InlineNode::CellWrap(n) => {
            let mut w = typed(out, "cell_wrap");
            w.field("line", |out| write_usize(out, n.line));
            if let Some(row) = n.row {
                w.field("row", |out| write_usize(out, row));
            }
            if let Some(col) = n.col {
                w.field("col", |out| write_usize(out, col));
            }
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        InlineNode::HardBreak(n) => {
            let mut w = typed(out, "hard_break");
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        InlineNode::CriticInsert(n) => {
            let mut w = typed(out, "insert");
            w.field("children", |out| write_inlines(out, &n.children));
            write_attrs_field(&mut w, &n.attrs);
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        InlineNode::CriticDelete(n) => {
            let mut w = typed(out, "delete");
            w.field("children", |out| write_inlines(out, &n.children));
            write_attrs_field(&mut w, &n.attrs);
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        InlineNode::CriticSubstitute(n) => {
            let mut w = typed(out, "substitution");
            w.field("old", |out| write_inlines(out, &n.old));
            w.field("new", |out| write_inlines(out, &n.new));
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        InlineNode::Comment(n) => {
            let mut w = typed(out, "comment");
            // `block` is the SPELLING (`%%%` against `%%`), not the position:
            // an inline comment reaches this arm with `block: true` whenever a
            // fenced comment folds into a line that holds inline content, which
            // markup-carve/carve#2458 makes reachable under a definition term.
            w.field("block", |out| write_bool(out, n.block));
            if n.delimited {
                w.field("delimited", |out| write_bool(out, true));
            }
            w.field("form", |out| write_string(out, comment_form(n)));
            w.field("content", |out| write_string(out, &n.content));
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        InlineNode::CriticComment(n) => {
            let mut w = typed(out, "critic_comment");
            w.field("text", |out| write_string(out, &n.text));
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        InlineNode::Ruby(_) => unreachable!("ruby is handled by the streaming encoder"),
    }
}

pub(super) fn write_link(out: &mut String, n: &Link) {
    let mut w = typed(out, "link");
    w.field("href", |out| write_string(out, &n.href));
    w.field("children", |out| write_inlines(out, &n.children));
    if let Some(title) = &n.title {
        w.field("title", |out| write_string(out, title));
    }
    if let Some(ref_label) = &n.ref_label {
        w.field("ref", |out| write_string(out, ref_label));
    }
    if n.ref_label.is_some() && n.href.is_empty() && n.resolved_empty_reference {
        w.field("resolved", |out| write_bool(out, true));
    }
    if let Some(raw_ref) = &n.raw_ref {
        w.field("rawRef", |out| write_string(out, raw_ref));
    }
    // `from_crossref` is deliberately NOT written: it is a render-time fact
    // about how this link was produced, the schema does not name it, and PART 12
    // §11 ingest refuses any property the schema does not name - so emitting it
    // made this engine's own `--json` output unreadable by this engine
    // (carve-rs#776). The flag lives on in memory for the renderers.
    write_attrs_field(&mut w, &n.attrs);
    write_pos_field(&mut w, &n.pos);
    w.finish();
}

pub(super) fn write_image(out: &mut String, n: &Image) {
    let mut w = typed(out, "image");
    w.field("src", |out| write_string(out, &n.src));
    w.field("alt", |out| write_string(out, &n.alt));
    if let Some(title) = &n.title {
        w.field("title", |out| write_string(out, title));
    }
    if let Some(ref_label) = &n.ref_label {
        w.field("ref", |out| write_string(out, ref_label));
    }
    if n.ref_label.is_some() && n.src.is_empty() && n.resolved_empty_reference {
        w.field("resolved", |out| write_bool(out, true));
    }
    if let Some(raw_ref) = &n.raw_ref {
        w.field("rawRef", |out| write_string(out, raw_ref));
    }
    write_attrs_field(&mut w, &n.attrs);
    write_pos_field(&mut w, &n.pos);
    w.finish();
}

pub(super) fn write_citation(out: &mut String, n: &Citation) {
    let mut w = Writer::new(out);
    w.field("key", |out| write_string(out, &n.key));
    if let Some(prefix) = &n.prefix {
        w.field("prefix", |out| write_inlines(out, prefix));
    }
    if let Some(locator) = &n.locator {
        w.field("locator", |out| write_inlines(out, locator));
    }
    if let Some(locator_label) = &n.locator_label {
        w.field("locatorLabel", |out| write_string(out, locator_label));
    }
    if let Some(locator_value) = &n.locator_value {
        w.field("locatorValue", |out| write_string(out, locator_value));
    }
    if let Some(suffix) = &n.suffix {
        w.field("suffix", |out| write_inlines(out, suffix));
    }
    w.field("suppressAuthor", |out| write_bool(out, n.suppress_author));
    if let Some(number) = n.number {
        w.field("number", |out| write_usize(out, number));
    }
    if let Some(use_index) = n.use_index {
        w.field("useIndex", |out| write_usize(out, use_index));
    }
    if n.mode == Some(CitationItemMode::Integral) {
        w.field("mode", |out| write_string(out, "integral"));
    }
    w.finish();
}

pub(super) fn write_cell_wraps_field(w: &mut Writer<'_>, wraps: &Option<Vec<VerbatimCellWrap>>) {
    let Some(wraps) = wraps else { return };
    w.field("cellWraps", |out| {
        write_array(out, wraps, |out, wrap| {
            let mut w = Writer::new(out);
            w.field("offset", |out| write_usize(out, wrap.offset));
            w.field("line", |out| write_usize(out, wrap.line));
            if let Some(row) = wrap.row {
                w.field("row", |out| write_usize(out, row));
            }
            if let Some(col) = wrap.col {
                w.field("col", |out| write_usize(out, col));
            }
            w.finish();
        })
    });
}

pub(super) fn typed<'a>(out: &'a mut String, ty: &str) -> Writer<'a> {
    let mut w = Writer::new(out);
    w.field("type", |out| write_string(out, ty));
    w
}

pub(super) fn write_attrs_field(w: &mut Writer<'_>, attrs: &Option<Attrs>) {
    if let Some(attrs) = attrs {
        w.field("attrs", |out| write_attrs(out, attrs));
    }
}

/// `key_values` in the order the AUTHOR wrote them, which is what `attrs.order`
/// records.
///
/// The map behind it is a `BTreeMap`, so iterating it publishes an ALPHABETICAL
/// order that is this engine's storage choice and nothing the document said.
/// `[x]{b=1 a=2}` came out as `{"keyValues":{"a":"1","b":"2"},"order":["b","a"]}`,
/// which is one `attrs` object stating two different orders for the same three
/// characters, and the HTML renderer, which reads `order`, already agreed with
/// the second one. PART 12 §1 is explicit about which of the two is publishable:
/// an implementation whose internals differ MAPS on the way out, it does not
/// export its internals. `resources/ast-schema.json` calls `order`
/// "Source-appearance order of the slots", and PART 11 §6 makes the author's
/// attribute order a choice "the AST records".
///
/// A key `order` does not mention is still published, after the ones it does. An
/// `Attrs` built programmatically records no order at all (the schema says so),
/// and dropping its attributes to protect an ordering would lose the document to
/// save the bookkeeping.
pub(super) fn ordered_key_values(attrs: &Attrs) -> Vec<(&str, &str)> {
    let mut out: Vec<(&str, &str)> = Vec::with_capacity(attrs.key_values.len());
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for slot in &attrs.order {
        let AttrSlot::Key(key) = slot else {
            continue;
        };
        let Some((key, value)) = attrs.key_values.get_key_value(key.as_str()) else {
            continue;
        };
        if seen.insert(key.as_str()) {
            out.push((key.as_str(), value.as_str()));
        }
    }
    for (key, value) in &attrs.key_values {
        if seen.insert(key.as_str()) {
            out.push((key.as_str(), value.as_str()));
        }
    }
    out
}

pub(super) fn write_attrs(out: &mut String, attrs: &Attrs) {
    let mut w = Writer::new(out);
    if let Some(id) = &attrs.id {
        w.field("id", |out| write_string(out, id));
    }
    if !attrs.classes.is_empty() {
        w.field("classes", |out| write_string_array(out, &attrs.classes));
    }
    if !attrs.key_values.is_empty() {
        w.field("keyValues", |out| {
            let mut w = Writer::new(out);
            for (key, value) in ordered_key_values(attrs) {
                w.field(key, |out| write_string(out, value));
            }
            w.finish();
        });
    }
    if !attrs.order.is_empty() {
        w.field("order", |out| {
            out.push('[');
            let mut first = true;
            for slot in &attrs.order {
                write_comma(out, &mut first);
                match slot {
                    AttrSlot::Id => write_string(out, "#id"),
                    AttrSlot::Class => write_string(out, ".class"),
                    AttrSlot::Key(key) => write_string(out, key),
                }
            }
            out.push(']');
        });
    }
    w.finish();
}

pub(super) fn write_pos_field(w: &mut Writer<'_>, pos: &Option<Pos>) {
    if let Some(pos) = pos {
        w.field("pos", |out| write_pos(out, pos));
    }
}

pub(super) fn write_pos(out: &mut String, pos: &Pos) {
    let mut w = Writer::new(out);
    w.field("startLine", |out| write_usize(out, pos.start_line));
    w.field("endLine", |out| write_usize(out, pos.end_line));
    w.field("startColumn", |out| write_usize(out, pos.start_column));
    w.field("endColumn", |out| write_usize(out, pos.end_column));
    w.field("startOffset", |out| write_usize(out, pos.start_offset));
    w.field("endOffset", |out| write_usize(out, pos.end_offset));
    if let Some(file) = &pos.file {
        w.field("file", |out| write_string(out, file.as_str()));
    }
    w.finish();
}

/// A [`BlockExtension`]'s payload. `value` is held as the JSON text it arrived
/// as and written back verbatim: the schema calls it opaque, and nothing in this
/// crate reads inside one.
pub(super) fn write_payload_field(w: &mut Writer<'_>, payload: &Option<ExtensionPayload>) {
    let Some(payload) = payload else {
        return;
    };
    w.field("payload", |out| {
        let mut inner = Writer::new(out);
        inner.field("format", |out| write_string(out, &payload.format));
        if let Some(value) = &payload.value {
            inner.field("value", |out| out.push_str(value));
        }
        inner.finish();
    });
}

pub(super) fn write_blocks(out: &mut String, blocks: &[BlockNode]) {
    write_array(out, blocks, write_block);
}

pub(super) fn write_inlines(out: &mut String, inlines: &[InlineNode]) {
    write_array(out, inlines, write_inline);
}

pub(super) fn write_string_array(out: &mut String, values: &[String]) {
    write_array(out, values, |out, s| write_string(out, s));
}

pub(super) fn write_array<T>(out: &mut String, values: &[T], mut f: impl FnMut(&mut String, &T)) {
    out.push('[');
    let mut first = true;
    for value in values {
        write_comma(out, &mut first);
        f(out, value);
    }
    out.push(']');
}

pub(super) fn write_comma(out: &mut String, first: &mut bool) {
    if *first {
        *first = false;
    } else {
        out.push(',');
    }
}

pub(super) fn write_string(out: &mut String, value: &str) {
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            c if c <= '\u{1f}' => {
                out.push_str("\\u");
                out.push(hex((c as u32 >> 12) & 0xf));
                out.push(hex((c as u32 >> 8) & 0xf));
                out.push(hex((c as u32 >> 4) & 0xf));
                out.push(hex(c as u32 & 0xf));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

pub(super) fn hex(n: u32) -> char {
    char::from_digit(n, 16).unwrap()
}

pub(super) fn write_bool(out: &mut String, value: bool) {
    out.push_str(if value { "true" } else { "false" });
}

pub(super) fn write_usize(out: &mut String, value: usize) {
    out.push_str(&value.to_string());
}

pub(super) fn ol_type_json(t: OrderedListType) -> &'static str {
    match t {
        OrderedListType::LowerAlpha => "a",
        OrderedListType::UpperAlpha => "A",
        OrderedListType::LowerRoman => "i",
        OrderedListType::UpperRoman => "I",
    }
}

pub(super) fn align_json(t: TableAlign) -> &'static str {
    match t {
        TableAlign::Left => "left",
        TableAlign::Right => "right",
        TableAlign::Center => "center",
    }
}

pub(super) fn valign_json(t: TableVerticalAlign) -> &'static str {
    match t {
        TableVerticalAlign::Top => "top",
        TableVerticalAlign::Middle => "middle",
        TableVerticalAlign::Bottom => "bottom",
    }
}

pub(crate) fn emphasis_type(t: EmphasisKind) -> &'static str {
    match t {
        EmphasisKind::Italic => "emphasis",
        EmphasisKind::Strong | EmphasisKind::BoldItalic => "strong",
        EmphasisKind::Underline => "underline",
        EmphasisKind::Strike => "strike",
        EmphasisKind::Super => "superscript",
        EmphasisKind::Sub => "subscript",
        EmphasisKind::Highlight => "highlight",
        EmphasisKind::SmallCaps => "small_caps",
    }
}
