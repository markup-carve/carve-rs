use crate::ast::*;
use crate::ast_json::block_pos;
use crate::render::MAX_RENDER_DEPTH;
use crate::render_text::{trim_end_non_nbsp, trim_non_nbsp};
use std::collections::{HashMap, HashSet};

/// A definition the author wrote ON a definition list's description line.
///
/// Collecting it empties the `dd` (spec markup-carve/carve#801), and an empty
/// description has no source spelling - the production requires content after
/// the marker - so the writer emitted a bare `:` line, which re-parses as a
/// continuation of the term above it. That is `to_html(fmt(x)) == to_html(x)`
/// failing, PART 11 section 1 (markup-carve/carve#805).
///
/// Nothing new is needed in the language. The description keeps the span of its
/// own marker line and the hoisted definition keeps the span it was written at
/// (PART 12 section 4); the two name the SAME line, so the description writes
/// the definition back on it and the document-level pass skips what a
/// description already claimed.
#[derive(Debug, Clone)]
enum DefinitionAtLine {
    Link(Box<LinkReferenceDefinition>),
    Footnote(String, Vec<BlockNode>),
}

struct CarveContext {
    block_depth: usize,
    inline_depth: usize,
    list_depth: usize,
    /// Depth of line-block nesting, so the inline writer drops the explicit
    /// backslash: inside a `::: |` fence every newline already IS a hard break.
    line_block_depth: usize,
    colon_fence_depth: usize,
    /// Inside a table cell, where a leading `^` cannot open a caption: a
    /// caption marker is a BLOCK line, and a cell's content is not one.
    table_cell_depth: usize,
    after_caption_host: bool,
    paragraph_starts_after_caption_host: bool,
    escape_mode: EscapeMode,
    /// Definitions written on a description line, keyed by that line.
    definitions_by_line: HashMap<usize, DefinitionAtLine>,
    /// The lines a description has already written back.
    ///
    /// PER PASS, because `render_carve_once` renders the document up to three
    /// times and picks between the forms (PART 11 section 4). A set that
    /// survived one pass would tell the next that every definition is already
    /// placed - the description emits a bare `:` again and the document-level
    /// arm emits nothing, deleting the definition outright.
    written_in_place: HashSet<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EscapeMode {
    Minimal,
    Conservative,
}

/// Render a tree that did NOT come from the parser, refusing at the ceiling.
///
/// `Err` when the tree nests deeper than [`crate::MAX_RENDER_DEPTH`], naming the
/// renderer and the bound (PART 9 §25). A parser-produced tree cannot reach it -
/// the parse cap sits below the ceiling - so this fails only for a tree built
/// through the API or read by `from_json`, which is the caller who can act on it.
pub fn render_carve(doc: &Document) -> Result<String, crate::RenderDepthError> {
    let watch = crate::render_depth::RenderDepthWatch::new();
    watch.into_result(protect_leading_bom(render_carve_unguarded(doc)))
}

/// A U+FEFF that would land at the head of the OUTPUT is written one column in.
///
/// `normalize_source` strips a single leading byte order mark before the parser
/// sees it, so a document whose first content character is one cannot be
/// written back flush left: the re-parse eats it and the document comes back
/// empty. The character is content - PART 2 keeps it, and corpus
/// `268-trailing-whitespace-on-a-content-line-is-dropped-8` is a paragraph
/// holding exactly one - so the writer has to put it somewhere a re-parse can
/// still read it.
///
/// One leading SPACE does that and nothing else: it is INDENTATION on re-parse,
/// which a paragraph drops, so the tree round-trips unchanged. It does not
/// violate PART 11 §7 either, which forbids a line whose ONLY content is
/// whitespace - this line has content, and the space is in front of it.
///
/// Idempotent by construction: the second pass sees the same tree and writes
/// the same leading space.
fn protect_leading_bom(out: String) -> String {
    if out.starts_with('\u{feff}') {
        return format!(" {out}");
    }
    out
}

thread_local! {
    /// Heading ids that a fresh parse would re-derive, so the writer must not
    /// turn them into source.
    ///
    /// PART 12 §5 publishes a heading's slugged id and PART 11 §1 writes the
    /// DOCUMENT back, so the two have to be told apart: an AUTHORED id carries
    /// an `#id` slot, a GENERATED one carries none. Dropping every unslotted id
    /// would be wrong as well - an ingested tree whose heading text was edited
    /// carries an id the text no longer slugs to, and there the id is the only
    /// place that information lives. So the test is MINIMAL FORM, the same one
    /// PART 11 §4 uses for escapes: write it only where dropping it would
    /// change the document (carve-js#741).
    static REDUNDANT_IDS: std::cell::RefCell<std::collections::BTreeSet<String>> =
        const { std::cell::RefCell::new(std::collections::BTreeSet::new()) };
}

/// The ids a fresh parse would assign, for headings that carry an unslotted id.
///
/// Computed with `assigned_heading_ids` - the pass the renderer itself uses -
/// over a copy with those ids removed, so this cannot answer differently from
/// the parse it is predicting.
fn redundant_heading_ids(doc: &Document) -> std::collections::BTreeSet<String> {
    let mut stripped = doc.clone();
    let mut had_any = false;
    strip_generated_ids(&mut stripped.children, &mut had_any);
    if !had_any {
        return std::collections::BTreeSet::new();
    }
    let fresh = crate::document_ids::assigned_heading_ids(&stripped, false);
    let mut present = Vec::new();
    collect_heading_ids(&doc.children, &mut present);
    present
        .into_iter()
        .zip(fresh)
        .filter_map(|(current, fresh)| match current {
            Some(id) if id == fresh => Some(id),
            _ => None,
        })
        .collect()
}

fn strip_generated_ids(blocks: &mut [BlockNode], had_any: &mut bool) {
    for block in blocks.iter_mut() {
        if let BlockNode::Heading(h) = block {
            if let Some(attrs) = h.attrs.as_mut() {
                let unslotted = attrs.id.is_some()
                    && !attrs.order.iter().any(|slot| matches!(slot, AttrSlot::Id));
                if unslotted {
                    attrs.id = None;
                    *had_any = true;
                }
            }
        }
        match block {
            BlockNode::BlockQuote(b) => strip_generated_ids(&mut b.children, had_any),
            BlockNode::Div(d) => strip_generated_ids(&mut d.children, had_any),
            BlockNode::Admonition(a) => strip_generated_ids(&mut a.children, had_any),
            BlockNode::List(l) => {
                for item in l.items.iter_mut() {
                    strip_generated_ids(&mut item.children, had_any);
                }
            }
            BlockNode::Figure(f) => {
                if let FigureTarget::BlockQuote(b) = &mut f.target {
                    strip_generated_ids(&mut b.children, had_any);
                }
            }
            BlockNode::DefinitionList(dl) => {
                for entry in dl.items.iter_mut() {
                    for definition in entry.definitions.iter_mut() {
                        strip_generated_ids(&mut definition.children, had_any);
                    }
                }
            }
            _ => {}
        }
    }
}

fn collect_heading_ids(blocks: &[BlockNode], out: &mut Vec<Option<String>>) {
    for block in blocks {
        if let BlockNode::Heading(h) = block {
            out.push(h.attrs.as_ref().and_then(|a| a.id.clone()));
        }
        match block {
            BlockNode::BlockQuote(b) => collect_heading_ids(&b.children, out),
            BlockNode::Div(d) => collect_heading_ids(&d.children, out),
            BlockNode::Admonition(a) => collect_heading_ids(&a.children, out),
            BlockNode::List(l) => {
                for item in l.items.iter() {
                    collect_heading_ids(&item.children, out);
                }
            }
            BlockNode::Figure(f) => {
                if let FigureTarget::BlockQuote(b) = &f.target {
                    collect_heading_ids(&b.children, out);
                }
            }
            BlockNode::DefinitionList(dl) => {
                for entry in dl.items.iter() {
                    for definition in entry.definitions.iter() {
                        collect_heading_ids(&definition.children, out);
                    }
                }
            }
            _ => {}
        }
    }
}

fn render_carve_unguarded(doc: &Document) -> String {
    // One render with the default sentinels. If the document turns out to
    // contain one of them itself, the counts disagree and the whole render is
    // repeated with a character it does not contain (see SENTINELS). Only a
    // document that actually holds a private-use sentinel pays for the second
    // pass, and nothing else changes: the retry runs the same code.
    let first = render_carve_once(doc);
    let current = SENTINELS.with(|s| s.get());
    let inserted = INSERTED.with(|c| c.get());
    let seen = SEEN.with(|c| c.get());
    if (0..5).all(|i| seen[i] <= inserted[i]) {
        return first;
    }
    // Choose against the STAGED text: `first` has been through restore, so an
    // authored occurrence is no longer visible in it.
    let staged = STAGED.with(|c| c.borrow().clone());
    let mut next = current;
    for i in 0..5 {
        if seen[i] > inserted[i] {
            next[i] = free_sentinel(&staged, &next);
        }
    }
    SENTINELS.with(|s| s.set(next));
    let second = render_carve_once(doc);
    SENTINELS.with(|s| s.set(SENTINEL_DEFAULTS));
    second
}

/// One full render, with the insertion counters reset for it.
fn render_carve_once(doc: &Document) -> String {
    let redundant = redundant_heading_ids(doc);
    REDUNDANT_IDS.with(|cell| *cell.borrow_mut() = redundant);
    INSERTED.with(|c| c.set([0; 5]));
    SEEN.with(|c| c.set([0; 5]));
    STAGED.with(|c| c.borrow_mut().clear());
    let minimal = render_with_escapes(doc, EscapeMode::Minimal);
    let conservative = render_with_escapes(doc, EscapeMode::Conservative);
    if minimal == conservative || escaping_is_redundant(&minimal, &conservative) {
        return minimal;
    }
    conservative
}

/// The lines every EMPTIED description sits on, anywhere in the tree.
///
/// Empty is the only case that matters: a description holding content writes
/// that content and needs nothing from here. Collecting the set first keeps the
/// map below empty - and its clones unmade - for every document that has no
/// such description, which is all but two of the 638 corpus documents.
fn emptied_description_lines(blocks: &[BlockNode], into: &mut HashSet<usize>) {
    for block in blocks {
        match block {
            BlockNode::DefinitionList(list) => {
                for item in &list.items {
                    for def in &item.definitions {
                        if def.children.is_empty() {
                            if let Some(pos) = &def.pos {
                                into.insert(pos.start_line);
                            }
                        } else {
                            emptied_description_lines(&def.children, into);
                        }
                    }
                }
            }
            BlockNode::BlockQuote(quote) => emptied_description_lines(&quote.children, into),
            BlockNode::Admonition(admonition) => {
                emptied_description_lines(&admonition.children, into);
            }
            BlockNode::Div(div) => emptied_description_lines(&div.children, into),
            // The two other walks over this tree (`normalize_escapes_block` and
            // `redundant_heading_ids`) both descend into a figure's block-quote
            // target, so this one does too. No input reaches it today - a `dd`
            // inside a block quote is not emptied here, because the definition
            // in it is not collected - but the asymmetry would be a trap the
            // moment that changes.
            BlockNode::Figure(figure) => {
                if let FigureTarget::BlockQuote(quote) = &figure.target {
                    emptied_description_lines(&quote.children, into);
                }
            }
            BlockNode::List(list) => {
                for item in &list.items {
                    // A definition the author wrote BETWEEN two of an item's
                    // blocks is the same case one level over: collecting it
                    // empties the line, and here that emptied line is what
                    // SPLIT one paragraph into two (corpus 228). Dropping it
                    // rejoins them, which is a different document. Nothing is
                    // left to carry the line, so the GAP between the two
                    // neighbours names it.
                    for pair in item.children.windows(2) {
                        let (Some(from), Some(to)) = (block_pos(&pair[0]), block_pos(&pair[1]))
                        else {
                            continue;
                        };
                        for line in (from.end_line + 1)..to.start_line {
                            into.insert(line);
                        }
                    }
                    emptied_description_lines(&item.children, into);
                }
            }
            BlockNode::Extension(extension) => {
                emptied_description_lines(&extension.children, into);
            }
            _ => {}
        }
    }
}

/// Hoisted definitions that sit on one of those lines, keyed by the line.
///
/// "Those lines" is both cases: an emptied description's own line, and a line
/// inside an item's gap. A definition on either belongs back on it.
fn definitions_by_description_line(doc: &Document) -> HashMap<usize, DefinitionAtLine> {
    let mut lines = HashSet::new();
    emptied_description_lines(&doc.children, &mut lines);
    let mut out = HashMap::new();
    if lines.is_empty() {
        return out;
    }
    for child in &doc.children {
        if let BlockNode::LinkReferenceDefinition(def) = child {
            if let Some(pos) = &def.pos {
                if lines.contains(&pos.start_line) {
                    // First writer wins for a line, which cannot normally
                    // collide: two definitions on one line is not a shape the
                    // parser produces.
                    out.entry(pos.start_line)
                        .or_insert_with(|| DefinitionAtLine::Link(Box::new(def.clone())));
                }
            }
        }
    }
    // A footnote definition is not in `children` - it hangs off the document in
    // its own map - so its line is the line its body starts on, which is the
    // definition line by production. That is the line
    // `footnote_defs_in_source_order` orders by, too.
    for (label, blocks) in &doc.footnote_defs {
        let Some(line) = blocks.first().and_then(block_pos).map(|pos| pos.start_line) else {
            continue;
        };
        if lines.contains(&line) {
            out.entry(line)
                .or_insert_with(|| DefinitionAtLine::Footnote(label.clone(), blocks.clone()));
        }
    }
    out
}

thread_local! {
    /// Whether a HYPHEN-spelled thematic break would be misread in this render.
    ///
    /// PART 11 §6 writes the marker the author used, now that the AST records it
    /// (carve#976, carve-rs#843). The one document that gets another spelling is
    /// the one whose emitted bytes would open a frontmatter block it does not
    /// have, and `render_with_escapes` is where that is decided.
    static HYPHEN_BREAKS_ARE_UNSAFE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Render, and fall back to a break spelling that cannot be read as frontmatter
/// when the finished bytes would be.
///
/// A frontmatter block is an opening fence AT BYTE 0 plus a bare `---` CLOSER
/// anywhere below it, so the collision is a property of the WHOLE emitted
/// document rather than of its first line. Two writer decisions reach it, and
/// this seam is the only thing they share:
///
/// - a break the author spelled `---` opens the document and gains a closer from
///   any later `---` break. `---` / blank / `---` is an EMPTY frontmatter block
///   rendering nothing where the input rendered two rules (carve-rs#732).
/// - §7 writes a hoisted link or footnote definition after the body, promoting
///   whatever stood second to byte 0 - and that block can be a PARAGRAPH whose
///   first line is `---yaml`-shaped. NO HEAD-OF-DOCUMENT RESPELLING REPAIRS
///   THAT ONE, because the paragraph's text is not the writer's to change. It is
///   saved by respelling the CLOSER instead, which is why the fallback moves
///   every hyphen break in the document rather than the one at the head
///   (carve-rs#819).
///
/// The second is why the previous seam is replaced rather than extended. That
/// one asked whether the FIRST RENDERED BLOCK was the string `---` and rewrote
/// that single line; a `---yaml` paragraph is not that block, and the break that
/// has to move is four lines further down.
///
/// THE DEPARTURE IS THE SMALLEST ONE THAT RESTORES §1, which is what §1a asks
/// for: only the HYPHEN spelling can be read as a fence, so only hyphen breaks
/// move and every other authored marker survives untouched. A document whose
/// breaks are all `***` or `___` never reaches the second pass at all.
///
/// The FINISHED bytes are handed to the PARSER'S own opener test, twice: once to
/// ask whether the authored spelling is misread, and once to confirm the
/// fallback is not. A document still misread with `***` keeps the authored
/// spelling rather than paying a respelling that buys nothing, which is the case
/// where the `---` closer came from somewhere other than a break, such as the
/// inside of a fenced block.
///
/// A leading `---` break with nothing below it to close a block keeps its
/// marker, which is what corpus
/// `132-thematic-break-requires-contiguous-markers-4` asks for. It is a CONTROL:
/// no mutation of this fallback moves it.
///
/// The `doc.frontmatter` arm is a COST GATE, not a correctness one, and saying
/// so is the honest reading. A document that really carries frontmatter has it
/// written by `render_frontmatter`, whose closer is not a break, so the fallback
/// pass opens frontmatter too and the authored form is returned anyway. Removing
/// the arm changes no output, only the number of renders paid by every document
/// that has frontmatter. Verified by mutation.
///
/// The test runs on the output of `normalize`, which is where `restore_verbatim`
/// turns staged content back into the bytes the next parse will actually see.
fn render_with_escapes(doc: &Document, escape_mode: EscapeMode) -> String {
    let authored = render_with_escapes_once(doc, escape_mode);
    if !doc.frontmatter.is_empty() || !crate::parse::opens_frontmatter(&authored) {
        return authored;
    }
    HYPHEN_BREAKS_ARE_UNSAFE.with(|unsafe_| unsafe_.set(true));
    let fallback = render_with_escapes_once(doc, escape_mode);
    HYPHEN_BREAKS_ARE_UNSAFE.with(|unsafe_| unsafe_.set(false));
    if crate::parse::opens_frontmatter(&fallback) {
        authored
    } else {
        fallback
    }
}

fn render_with_escapes_once(doc: &Document, escape_mode: EscapeMode) -> String {
    let mut ctx = CarveContext {
        block_depth: 0,
        inline_depth: 0,
        list_depth: 0,
        line_block_depth: 0,
        colon_fence_depth: 0,
        table_cell_depth: 0,
        after_caption_host: false,
        paragraph_starts_after_caption_host: false,
        escape_mode,
        definitions_by_line: definitions_by_description_line(doc),
        written_in_place: HashSet::new(),
    };
    let mut parts = Vec::new();
    if !doc.frontmatter.is_empty() {
        parts.push(render_frontmatter(&doc.frontmatter));
    }
    // §7 puts hoisted definitions after the body, ordered among themselves by
    // source position, and PART 11 §6 binds the writer to the order the tree
    // holds: "fmt does not reorder ... those are the author's choices and the
    // AST records them".
    //
    // Rendering `children` and then the footnote map wrote every link definition
    // ahead of every footnote, and the footnotes themselves in LABEL order,
    // because the map is a BTreeMap - so `[^b]` written first came out after
    // `[^a]` (carve-rs#682). The ordering is the encoder's own
    // `ordered_document_entries`, reused rather than reimplemented, so the
    // written source and the published tree cannot disagree.
    let footnote_defs = crate::ast_json::footnote_defs_in_source_order(doc);
    let mut rendered = Vec::new();
    // The document level joins its own entries rather than going through
    // `render_blocks`, so the adjacent-sibling-list offset is applied here too.
    // See the note beside `lists_would_merge`; without it a top-level pair --
    // which is where authors actually write one -- still merged (carve#1088).
    let mut previous_list: Option<&List> = None;
    let mut list_offset = 0usize;
    for entry in crate::ast_json::ordered_document_entries(doc, &footnote_defs) {
        let text = match entry {
            crate::ast_json::DocEntry::Block(child) => {
                ctx.paragraph_starts_after_caption_host = ctx.after_caption_host;
                let text = render_block(child, &mut ctx);
                ctx.after_caption_host = hosts_caption(child);
                if let BlockNode::List(list) = child {
                    list_offset = match previous_list {
                        Some(previous) if lists_would_merge(previous, list) => list_offset + 1,
                        _ => 0,
                    };
                    previous_list = Some(list);
                    if list_offset > 0 {
                        indent_lines(&text, list_offset)
                    } else {
                        text
                    }
                } else {
                    if !text.is_empty() {
                        previous_list = None;
                        list_offset = 0;
                    }
                    text
                }
            }
            crate::ast_json::DocEntry::FootnoteDef(label, blocks, _) => {
                ctx.after_caption_host = false;
                // Unless a definition list already wrote it where the author put
                // it (markup-carve/carve#805).
                if blocks
                    .first()
                    .and_then(block_pos)
                    .is_some_and(|pos| ctx.written_in_place.contains(&pos.start_line))
                {
                    String::new()
                } else {
                    render_footnote_def_source(label, blocks, &mut ctx)
                }
            }
        };
        if !text.is_empty() {
            rendered.push(text);
        }
    }
    // THE FALLBACK SPELLING IS DECIDED IN `render_with_escapes`, on the finished
    // bytes, not here. It used to be decided here, from the FIRST RENDERED
    // BLOCK, and that could only see the shape where the break itself opens the
    // document - so a hoisted definition promoting a `---yaml`-shaped PARAGRAPH
    // to byte 0 walked straight past it (carve-rs#819).
    //
    // What stays here is the ORDER: §7 puts hoisted definitions after the body,
    // which is the decision that does the promoting.
    if !rendered.is_empty() {
        parts.push(rendered.join("\n\n"));
    }
    normalize(&parts.join("\n\n"))
}

fn escaping_is_redundant(minimal: &str, conservative: &str) -> bool {
    let parsed = std::panic::catch_unwind(|| {
        (
            comparable_document(crate::parse::parse_for_carve_shape(minimal)),
            comparable_document(crate::parse::parse_for_carve_shape(conservative)),
        )
    });
    parsed.is_ok_and(|(minimal_doc, conservative_doc)| minimal_doc == conservative_doc)
}

fn comparable_document(mut doc: Document) -> Document {
    doc.source_len = 0;
    for block in &mut doc.children {
        normalize_escapes_block(block);
    }
    // Footnote definitions are NOT in `children` -- they hang off the document in
    // their own map. Leaving them un-normalized meant any escape inside one made
    // the two renders differ, so W4 escalated the WHOLE document to conservative:
    // `a.` alone formatted as `a.`, but the same paragraph beside a `[^f]: b.`
    // definition came back `a\.` (carve#352, corpus 22-footnotes).
    for blocks in doc.footnote_defs.values_mut() {
        for block in blocks.iter_mut() {
            normalize_escapes_block(block);
        }
    }
    doc
}

/// Collapse adjacent text and escaped-text nodes into one text node.
///
/// An escape is exactly what this comparison is deciding, so the two renders
/// must not be told apart BY it. Escaping a character both retypes the node and
/// SPLITS the run it sat in - `blue.` is one text node, `blue\.` is a text node
/// plus an escaped-text node - so without this every candidate character would
/// report a difference and escalate the whole document to conservative
/// escaping.
///
/// What survives the merge is the question worth asking: same characters, same
/// order, same surrounding structure - does dropping the escapes change
/// anything ELSE? PART 11 section 1 states this as the invariant's own
/// definition of equality.
fn normalize_escapes_inlines(nodes: &mut Vec<InlineNode>) {
    let mut merged: Vec<InlineNode> = Vec::with_capacity(nodes.len());
    for node in nodes.drain(..) {
        let text = match node {
            InlineNode::Text(t) => Some(t.value),
            InlineNode::EscapedText(t) => Some(t.value),
            other => {
                let mut other = other;
                normalize_escapes_nested(&mut other);
                merged.push(other);
                None
            }
        };
        if let Some(t) = text {
            if let Some(InlineNode::Text(previous)) = merged.last_mut() {
                previous.value.push_str(&t);
            } else {
                merged.push(InlineNode::text(t));
            }
        }
    }
    *nodes = merged;
}

/// Recurse into an inline node that carries inline children of its own.
fn normalize_escapes_nested(node: &mut InlineNode) {
    match node {
        InlineNode::Comment(_) => {}
        InlineNode::Emphasis(e) => normalize_escapes_inlines(&mut e.children),
        InlineNode::Link(l) => normalize_escapes_inlines(&mut l.children),
        InlineNode::Span(s) => normalize_escapes_inlines(&mut s.children),
        // An inline extension carries inline children too, and omitting it meant
        // an escape inside one made the two renders differ and escalated the
        // WHOLE document: `Press :kbd[Ctrl+C] to copy.` came back
        // `Press :kbd[Ctrl\+C] to copy\.` (carve#352, corpus 45-inline-extensions).
        InlineNode::Extension(e) => normalize_escapes_inlines(&mut e.children),
        // Editorial insert and delete carry inline children too. Omitting them
        // escalated any document containing an escape inside one: `{++a++}{.a}`
        // came back `{+\+a\++}{.a}`, over-escaping content the HTML target shows
        // as a literal `+a+` (carve#352, corpus 126).
        InlineNode::CriticInsert(i) => normalize_escapes_inlines(&mut i.children),
        InlineNode::CriticDelete(d) => normalize_escapes_inlines(&mut d.children),
        InlineNode::Footnote(f) => {
            if let Some(inline) = &mut f.inline {
                normalize_escapes_inlines(inline);
            }
        }
        // Listed rather than caught by `_`, so a new inline node that carries
        // children fails to compile here instead of being silently skipped. That
        // catch-all is how the extension gap (carve-rs#310) and the editorial gap
        // above both survived: adding a node type with children was enough to
        // introduce an over-escaping bug, with nothing to notice it.
        InlineNode::Text(_)
        | InlineNode::EscapedText(_)
        | InlineNode::SmartPunctuation(_)
        | InlineNode::Code(_)
        | InlineNode::Image(_)
        | InlineNode::Math(_)
        | InlineNode::RawInline(_)
        | InlineNode::LiteralInline(_)
        | InlineNode::Symbol(_)
        | InlineNode::AutoLink(_)
        | InlineNode::CrossRef(_)
        | InlineNode::CaptionNumber(_)
        | InlineNode::Mention(_)
        | InlineNode::Tag(_)
        | InlineNode::CitationGroup(_)
        | InlineNode::Abbreviation(_)
        | InlineNode::SoftBreak(_)
        | InlineNode::HardBreak(_)
        | InlineNode::CriticSubstitute(_)
        | InlineNode::CriticComment(_) => {}
    }
}

fn normalize_escapes_block(block: &mut BlockNode) {
    match block {
        // No inline children: the label, destination and title are plain strings.
        BlockNode::LinkReferenceDefinition(_) => {}
        BlockNode::Heading(h) => normalize_escapes_inlines(&mut h.children),
        BlockNode::Paragraph(p) => normalize_escapes_inlines(&mut p.children),
        BlockNode::List(l) => {
            for item in &mut l.items {
                for child in &mut item.children {
                    normalize_escapes_block(child);
                }
            }
        }
        BlockNode::BlockQuote(b) => {
            if let Some(attribution) = &mut b.attribution {
                normalize_escapes_inlines(attribution);
            }
            for child in &mut b.children {
                normalize_escapes_block(child);
            }
        }
        BlockNode::Table(t) => {
            if let Some(cap) = &mut t.caption {
                normalize_escapes_inlines(cap);
            }
            for row in &mut t.rows {
                for cell in &mut row.cells {
                    normalize_escapes_inlines(&mut cell.children);
                }
            }
        }
        BlockNode::Admonition(a) => {
            if let Some(title) = &mut a.title {
                normalize_escapes_inlines(title);
            }
            for child in &mut a.children {
                normalize_escapes_block(child);
            }
        }
        BlockNode::LineBlock(lb) => {
            for child in &mut lb.children {
                normalize_escapes_block(child);
            }
        }
        BlockNode::Div(d) => {
            for child in &mut d.children {
                normalize_escapes_block(child);
            }
        }
        BlockNode::DefinitionList(dl) => {
            for item in &mut dl.items {
                for term in &mut item.terms {
                    normalize_escapes_inlines(term);
                }
                for def in &mut item.definitions {
                    for child in def.iter_mut() {
                        normalize_escapes_block(child);
                    }
                }
            }
        }
        BlockNode::Figure(f) => {
            normalize_escapes_inlines(&mut f.caption);
            normalize_escapes_figure_target(f);
        }
        BlockNode::Extension(e) => {
            for child in &mut e.children {
                normalize_escapes_block(child);
            }
        }
        BlockNode::CodeBlock(_)
        | BlockNode::AbbreviationDef(_)
        | BlockNode::RawBlock(_)
        | BlockNode::Comment(_)
        | BlockNode::BlockImage(_)
        | BlockNode::ThematicBreak(_) => {}
    }
}

fn normalize_escapes_figure_target(f: &mut crate::ast::Figure) {
    match &mut f.target {
        FigureTarget::BlockQuote(b) => {
            for child in &mut b.children {
                normalize_escapes_block(child);
            }
        }
        FigureTarget::Table(t) => {
            if let Some(cap) = &mut t.caption {
                normalize_escapes_inlines(cap);
            }
            for row in &mut t.rows {
                for cell in &mut row.cells {
                    normalize_escapes_inlines(&mut cell.children);
                }
            }
        }
        FigureTarget::Paragraph(p) => normalize_escapes_inlines(&mut p.children),
        FigureTarget::Image(_) | FigureTarget::CodeBlock(_) => {}
    }
}

/// Whether two adjacent sibling lists would read back as ONE list.
///
/// PART 9 §11 N1's axes: the kind, the plain-vs-task classification, and the
/// marker character the author chose -- the ordered delimiter and dialect, or
/// the bullet. Where any of them differs the lists separate on their own and
/// the writer owes them nothing, which is what carve#286 established.
fn lists_would_merge(a: &List, b: &List) -> bool {
    if a.ordered != b.ordered || is_task_list(a) != is_task_list(b) {
        return false;
    }
    if a.ordered {
        return a.delim.unwrap_or('.') == b.delim.unwrap_or('.') && a.ol_type == b.ol_type;
    }
    a.bullet_char.unwrap_or('-') == b.bullet_char.unwrap_or('-')
}

fn is_task_list(list: &List) -> bool {
    list.items.iter().any(|item| item.checked.is_some())
}

/// Every non-blank line of `text`, prefixed with `columns` spaces.
fn indent_lines(text: &str, columns: usize) -> String {
    let pad = " ".repeat(columns);
    text.split('\n')
        .map(|line| {
            if line.is_empty() {
                String::new()
            } else {
                format!("{pad}{line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_blocks(blocks: &[BlockNode], ctx: &mut CarveContext) -> String {
    if ctx.block_depth >= MAX_RENDER_DEPTH {
        crate::render_depth::record("carve");
        return String::new();
    }
    ctx.block_depth += 1;
    let previous_host = ctx.after_caption_host;
    let previous_paragraph_start = ctx.paragraph_starts_after_caption_host;
    ctx.after_caption_host = false;
    let mut rendered = Vec::new();
    // TWO ADJACENT SIBLING LISTS NEED SOMETHING BETWEEN THEM. Written at the
    // same column with matching markers they merge on re-parse, so
    // `parse(fmt(x)) == parse(x)` is false for a document the parser reads as
    // two lists (carve#1088). carve#286 spent the marker axis -- emit the marker
    // as authored -- which separates them only while the markers DIFFER; when
    // both are `1.` at column 0 there is nothing left to preserve and
    // indentation is the axis remaining.
    //
    // ONE SPACE, CUMULATIVE, RELATIVE TO THE LIST BEFORE IT. One space is the
    // only offset safe for both kinds: a bullet's content column is 2, so two
    // spaces already NEST the second list inside the first. And the step is per
    // list rather than per run -- a flat +1 leaves the second and third at the
    // same column, where they merge with each other.
    let mut previous_list: Option<&List> = None;
    let mut list_offset = 0usize;
    for block in blocks {
        ctx.paragraph_starts_after_caption_host = ctx.after_caption_host;
        let text = render_block(block, ctx);
        ctx.after_caption_host = hosts_caption(block);
        if let BlockNode::List(list) = block {
            list_offset = match previous_list {
                Some(previous) if lists_would_merge(previous, list) => list_offset + 1,
                _ => 0,
            };
            previous_list = Some(list);
        } else if !text.is_empty() {
            previous_list = None;
            list_offset = 0;
        }
        if !text.is_empty() {
            rendered.push(if list_offset > 0 {
                indent_lines(&text, list_offset)
            } else {
                text
            });
        }
    }
    let out = rendered.join("\n\n");
    ctx.after_caption_host = previous_host;
    ctx.paragraph_starts_after_caption_host = previous_paragraph_start;
    ctx.block_depth -= 1;
    out
}

fn hosts_caption(block: &BlockNode) -> bool {
    match block {
        BlockNode::Table(_)
        | BlockNode::CodeBlock(_)
        | BlockNode::BlockQuote(_)
        | BlockNode::BlockImage(_) => true,
        BlockNode::Paragraph(paragraph) if paragraph.children.len() == 1 => {
            match &paragraph.children[0] {
                InlineNode::Image(image) => !image.src.is_empty(),
                InlineNode::Math(math) => math.display,
                _ => false,
            }
        }
        _ => false,
    }
}

fn with_reset_colon_fence_depth<T>(
    ctx: &mut CarveContext,
    f: impl FnOnce(&mut CarveContext) -> T,
) -> T {
    let saved = ctx.colon_fence_depth;
    ctx.colon_fence_depth = 0;
    let out = f(ctx);
    ctx.colon_fence_depth = saved;
    out
}

fn render_inside_colon_container(blocks: &[BlockNode], ctx: &mut CarveContext) -> String {
    ctx.colon_fence_depth += 1;
    let body = render_blocks(blocks, ctx);
    ctx.colon_fence_depth -= 1;
    body
}

/// Render a list item's children. A loose item separates every block with a
/// blank line. A tight item joins its blocks with a single newline so the
/// re-parse stays tight - EXCEPT it keeps the blank line adjacent to a nested
/// list child, whose own loose/tight rendering (and the continuation-indent
/// logic below) needs it. Without the tight join, a tight item with more than
/// one child (e.g. text after a fenced block, corpus 162) would be loosened by
/// the blank lines, breaking to_html(fmt(x)) == to_html(x); without the
/// nested-list exception, a tight item whose child is a nested list (corpus
/// 142) would stop being idempotent.
/// The definition the author wrote on a line strictly between two blocks.
///
/// The description case can ask its own node for the line; here the node is
/// gone, so the neighbours' spans name it. Marked written the same way, so the
/// document-level pass skips it and the label is not defined twice.
fn definition_in_gap(
    before: &BlockNode,
    after: &BlockNode,
    ctx: &mut CarveContext,
) -> Option<String> {
    let from = block_pos(before)?.end_line;
    let to = block_pos(after)?.start_line;
    let (line, definition) = ((from + 1)..to).find_map(|line| {
        ctx.definitions_by_line
            .get(&line)
            .filter(|_| !ctx.written_in_place.contains(&line))
            .cloned()
            .map(|definition| (line, definition))
    })?;
    // MARKED AFTER RENDERING, not before. `render_block` returns an empty
    // string for a definition already marked written, so marking it first made
    // the gap render nothing and the document-level pass skip it too - the
    // definition disappeared from the document entirely.
    let written = match definition {
        DefinitionAtLine::Link(def) => render_block(&BlockNode::LinkReferenceDefinition(*def), ctx),
        DefinitionAtLine::Footnote(label, blocks) => {
            render_footnote_def_source(&label, &blocks, ctx)
        }
    };
    if written.is_empty() {
        return None;
    }
    ctx.written_in_place.insert(line);
    Some(written)
}

/// Sentinel marking a line to be written at the ITEM's marker column.
///
/// The list writer prefixes an item's continuation lines with its content
/// column. A `+` continuation marker and the block it attaches are the two
/// things that must NOT get that prefix (§17 L3), and they are produced deep
/// inside the item body where the prefix is not yet known - so they are tagged
/// here and the prefix loop honours the tag.
const MARKER_COLUMN: char = '\u{e005}';

fn at_marker_column(text: &str) -> String {
    text.split('\n')
        .map(|line| format!("{MARKER_COLUMN}{line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn adjacent_blocks_merge(left: &BlockNode, right: &BlockNode) -> bool {
    match (left, right) {
        (BlockNode::BlockQuote(_), BlockNode::BlockQuote(_))
        | (BlockNode::Table(_), BlockNode::Table(_))
        | (BlockNode::LineBlock(_), BlockNode::LineBlock(_))
        | (BlockNode::DefinitionList(_), BlockNode::DefinitionList(_)) => true,
        (BlockNode::List(left), BlockNode::List(right)) => {
            left.ordered == right.ordered
                && left.delim == right.delim
                && left.bullet_char == right.bullet_char
                && left.ol_type == right.ol_type
        }
        _ => false,
    }
}

fn render_item_blocks(blocks: &[BlockNode], tight: bool, ctx: &mut CarveContext) -> String {
    if !tight {
        return render_blocks(blocks, ctx);
    }
    if ctx.block_depth >= MAX_RENDER_DEPTH {
        crate::render_depth::record("carve");
        return String::new();
    }
    ctx.block_depth += 1;
    let mut out = String::new();
    let mut prev: Option<&BlockNode> = None;
    let mut prev_at_marker_column = false;
    for (index, block) in blocks.iter().enumerate() {
        let next = blocks.get(index + 1);
        let rendered = render_block(block, ctx);
        if rendered.is_empty() {
            continue;
        }
        let mut separated = false;
        if let Some(prev_block) = prev {
            // A tight item joins every child with a single newline, including a
            // nested list. The blank line that used to be kept here existed to
            // work around nested looseness propagating to the outer item; with
            // that fixed in line_starts_paragraph, keeping it would insert a
            // blank the author never wrote and diverge from carve-js/carve-php.
            out.push('\n');
            if let Some(written) = definition_in_gap(prev_block, block, ctx) {
                out.push_str(&written);
                out.push('\n');
                // A definition written back BETWEEN the two blocks already ends
                // the paragraph above it, so the marker below is not needed -
                // and emitting it anyway changes the canonical form of corpus
                // 228, whose point is that a line at the definition's own
                // column forms its own tight block.
                separated = true;
            }
        }
        // §17 L3: a block after a paragraph needs its continuation marker
        // written back whenever the block's own first line would FOLD into that
        // paragraph. Indented under the item it is a lazy continuation of the
        // paragraph above (§10 I2), so the item comes back holding ONE block
        // where the author wrote two (carve#861).
        //
        // "ONLY A PARAGRAPH REACHES THIS" WAS FALSE, and the comment that said
        // so said why the corpus never caught it: it pins a fence and a quote,
        // both of which OPEN a block at the item's content column and so never
        // needed the marker. An IMAGE line opens nothing. `- x` / `+` /
        // `![a](i.png)` / `^ cap` came back as `- x` / `  ![a](i.png)` /
        // `  ^ cap`, where the image is no longer a standalone image paragraph,
        // PART 9 §4 does not attach the caption, and the `<figure>` is gone with
        // the caption left as literal text (carve-rs#819). The bare image
        // without a caption loses its block just the same, and that one is not
        // on the ticket.
        //
        // So the test is the PARSER'S OWN opener test on the bytes about to be
        // emitted, rather than a list of block kinds maintained by hand here -
        // the same deviation `markup-carve/carve#961` records for the leading
        // thematic break.
        let folds_into_the_paragraph_above = rendered
            .lines()
            .next()
            .is_some_and(crate::parse::line_starts_paragraph);
        // ONCE ONE CHILD IS AT THE MARKER COLUMN, EVERY LATER ONE IN THE RUN
        // MUST BE.
        //
        // The marker column is the ITEM's column, to the LEFT of the item's
        // content column, so a later child written at the content column is
        // INDENTED relative to the block above it - it becomes that block's
        // lazy continuation (§10 I2) or is absorbed into it outright. `- x` /
        // `+` / image / `+` / image came back as an item holding ONE image
        // paragraph with the second image's source as literal text; with a
        // caption on each, the second figure's whole source landed inside the
        // first one's `<figcaption>` (carve-rs#819).
        //
        // The condition is the PREVIOUS child's COLUMN, not its kind. Its kind
        // is what the arm above already asks, and that answers a different
        // question - whether this child folds into an open PARAGRAPH. This one
        // is about where the child sits relative to the block before it, which
        // no property of the child alone can decide.
        let continues_a_run_at_the_marker_column = prev.is_some() && prev_at_marker_column;
        if !separated
            && (continues_a_run_at_the_marker_column
                || next.is_some_and(|next_block| adjacent_blocks_merge(block, next_block))
                || (matches!(prev, Some(BlockNode::Paragraph(_)))
                    && folds_into_the_paragraph_above))
        {
            out.push_str(&at_marker_column("+"));
            out.push('\n');
            out.push_str(&at_marker_column(&rendered));
            prev = Some(block);
            prev_at_marker_column = true;
            continue;
        }
        out.push_str(&rendered);
        prev = Some(block);
        prev_at_marker_column = false;
    }
    ctx.block_depth -= 1;
    out
}

fn render_block(node: &BlockNode, ctx: &mut CarveContext) -> String {
    match node {
        BlockNode::LinkReferenceDefinition(def) => {
            // Unless a definition list already wrote it on its own description
            // line, where the author put it - writing it twice would define the
            // label twice (markup-carve/carve#805).
            if def
                .pos
                .as_ref()
                .is_some_and(|pos| ctx.written_in_place.contains(&pos.start_line))
            {
                return String::new();
            }
            // PART 12 §10 gave this a node precisely so the writer can put the
            // line back. Before that there was nowhere to write it from, which is
            // why every resolved reference was INLINED instead (carve-rs#631).
            let title = def
                .title
                .as_ref()
                .map(|t| format!(" \"{}\"", escape_quoted(t)))
                .unwrap_or_default();
            let attrs = render_attrs(&def.attrs);
            let attrs = if attrs.is_empty() {
                String::new()
            } else {
                format!(" {attrs}")
            };
            format!("[{}]: {}{title}{attrs}", def.label, def.href)
        }
        BlockNode::Heading(heading) => {
            // A heading is SINGLE-LINE (PART 2), so its text must not contain a
            // newline: emitting one would end the heading and silently re-parse
            // the remainder as a following block. No parse builds such a
            // heading, but an ingested AST can - PART 12 lets any inline sit in
            // a heading, break nodes included - so a break collapses to a
            // single space here rather than corrupting the document it is
            // written back to. Matches carve-js.
            let rendered = render_inlines(&heading.children, ctx);
            let text = collapse_breaks(trim_non_nbsp(&rendered));
            let body = format!("{} {}", "#".repeat(heading.level as usize), text);
            // A generated id a fresh parse would re-derive is not the author's
            // source (carve-js#741); one it would not - an edited ingested tree -
            // is written, because the id lives nowhere else.
            let attrs = match heading.attrs.as_ref() {
                Some(attrs) => match attrs.id.as_ref() {
                    Some(id)
                        if !attrs.order.iter().any(|slot| matches!(slot, AttrSlot::Id))
                            && REDUNDANT_IDS.with(|cell| cell.borrow().contains(id)) =>
                    {
                        let mut without = attrs.clone();
                        without.id = None;
                        Some(without)
                    }
                    _ => Some(attrs.clone()),
                },
                None => None,
            };
            with_block_attrs(&attrs, &body)
        }
        BlockNode::Paragraph(paragraph) => {
            let caption_can_open = render_attrs(&paragraph.attrs).is_empty()
                && ctx.paragraph_starts_after_caption_host;
            let body = guard_thematic_break_lines(&render_inlines_with_caption(
                &paragraph.children,
                ctx,
                caption_can_open,
            ));
            with_block_attrs(&paragraph.attrs, &body)
        }
        BlockNode::CodeBlock(code) => {
            let fence = safe_fence(&code.content, 3);
            let info = code_fence_info(
                code.lang.as_deref(),
                code.title.as_deref(),
                code.label.as_deref(),
            );
            // The opener's quoted title is resolved onto `attrs.title` at parse
            // time so it reaches every consumer, but the fence carries it too -
            // emitting both says it twice and re-parses with an attribute ORDER
            // slot the source never had (carve#369). The fence is the authored
            // spelling, so it wins.
            let attrs = match (&code.title, &code.attrs) {
                (Some(title), Some(a)) if a.key_values.get("title") == Some(title) => {
                    without_key(a, "title")
                }
                _ => code.attrs.clone(),
            };
            with_block_attrs(
                &attrs,
                &format!(
                    "{fence}{info}\n{}\n{fence}",
                    protect_verbatim(&code.content)
                ),
            )
        }
        BlockNode::BlockQuote(quote) => {
            let inner =
                with_reset_colon_fence_depth(ctx, |ctx| render_blocks(&quote.children, ctx));
            let body = inner
                .split('\n')
                .map(|line| {
                    if line.is_empty() {
                        ">".to_string()
                    } else {
                        format!("> {line}")
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            // PART 9 §4a: an attribution is written back as the `^` line it was
            // read from. Dropping it would lose content, which PART 11 §1 forbids.
            let body = match &quote.attribution {
                Some(attribution) => {
                    format!("{body}\n^ {}", render_inlines(attribution, ctx))
                }
                None => body,
            };
            with_block_attrs(&quote.attrs, &body)
        }
        BlockNode::List(list) => with_block_attrs(
            &list.attrs,
            &with_reset_colon_fence_depth(ctx, |ctx| render_list(list, ctx)),
        ),
        // PART 11 §6 writes the marker the author used, now that the AST
        // records it (carve#976, carve-rs#843). Only the HYPHEN spelling can be
        // read back as a frontmatter fence, so it is the only one the fallback
        // moves, and only for a document whose emitted bytes would really be
        // misread - see `render_with_escapes`, where that is decided.
        BlockNode::ThematicBreak(rule) => {
            let mut marker = rule.marker.unwrap_or('-');
            if marker == '-' && HYPHEN_BREAKS_ARE_UNSAFE.with(|unsafe_| unsafe_.get()) {
                marker = '*';
            }
            with_block_attrs(&rule.attrs, &marker.to_string().repeat(3))
        }
        BlockNode::Table(table) => with_block_attrs(&table.attrs, &render_table(table, ctx)),
        BlockNode::Admonition(admonition) => {
            let title = admonition
                .title
                .as_ref()
                .map(|title| format!(" \"{}\"", escape_quoted(&render_inlines(title, ctx))))
                .unwrap_or_default();
            let label = admonition
                .label
                .as_ref()
                .map(|label| format!(" [{}]", escape_bracket_text(label)))
                .unwrap_or_default();
            let fence = colon_fence_for(ctx);
            let body = render_inside_colon_container(&admonition.children, ctx);
            with_block_attrs(
                &admonition.attrs,
                &format!("{fence} {}{title}{label}\n{body}\n{fence}", admonition.kind),
            )
        }
        BlockNode::LineBlock(lb) => {
            // `::: |` is the line-block opener (PART 3, line_block_open).
            // Emitting a bare `:::` and tagging the node with a `.line-block`
            // class instead re-parsed as an ordinary div, so the node type
            // changed across a format round trip and
            // `parse(fmt(x)) == parse(x)` did not hold (carve issue 359).
            //
            // Inside the fence every newline IS a hard break (PART 3,
            // line_block_body), so the explicit backslash the inline writer
            // emits for a HardBreak would double it on re-parse.
            ctx.line_block_depth += 1;
            let fence = colon_fence_for(ctx);
            let body = render_inside_colon_container(&lb.children, ctx);
            ctx.line_block_depth -= 1;
            with_block_attrs(&lb.attrs, &format!("{fence} |\n{body}\n{fence}"))
        }
        BlockNode::Div(div) => {
            let label = div
                .label
                .as_ref()
                .map(|label| format!(" [{}]", escape_bracket_text(label)))
                .unwrap_or_default();
            let fence = colon_fence_for(ctx);
            let body = render_inside_colon_container(&div.children, ctx);
            with_block_attrs(&div.attrs, &format!("{fence}{label}\n{body}\n{fence}"))
        }
        BlockNode::DefinitionList(list) => with_block_attrs(
            &list.attrs,
            &with_reset_colon_fence_depth(ctx, |ctx| render_definition_list(&list.items, ctx)),
        ),
        BlockNode::Figure(figure) => with_block_attrs(&figure.attrs, &render_figure(figure, ctx)),
        BlockNode::BlockImage(image) => render_image(image),
        BlockNode::RawBlock(raw) => {
            let fence = safe_fence(&raw.content, 3);
            format!(
                "{fence}={}\n{}\n{fence}",
                escape_format(&raw.format),
                protect_verbatim(&raw.content)
            )
        }
        BlockNode::AbbreviationDef(abbr) => {
            format!(
                "*[{}]: {}",
                escape_abbr(&abbr.abbr),
                escape_plain_line(&abbr.expansion)
            )
        }
        BlockNode::Comment(comment) => {
            if comment.block {
                render_block_comment(&comment.content)
            } else {
                format!("%% {}", comment.content)
            }
        }
        BlockNode::Extension(extension) => {
            with_block_attrs(&extension.attrs, &render_blocks(&extension.children, ctx))
        }
    }
}

/// A copy of `attrs` without one key-value, dropping the slot from `order`.
/// Returns `None` when the removal leaves nothing to render.
fn without_key(attrs: &Attrs, key: &str) -> Option<Attrs> {
    let mut next = attrs.clone();
    next.key_values.remove(key);
    next.order
        .retain(|slot| !matches!(slot, AttrSlot::Key(k) if k == key));
    if next.id.is_none() && next.classes.is_empty() && next.key_values.is_empty() {
        return None;
    }
    Some(next)
}

fn with_block_attrs(attrs: &Option<Attrs>, body: &str) -> String {
    let rendered = render_attrs(attrs);
    if rendered.is_empty() {
        body.to_string()
    } else {
        format!("{rendered}\n{body}")
    }
}

fn render_list(node: &List, ctx: &mut CarveContext) -> String {
    ctx.list_depth += 1;
    let mut out = String::new();
    let mut counter = node.start.unwrap_or(1);
    // The marker is semantic (§11: a different bullet char / ordered delim
    // starts a new list), so emit it as authored - normalizing would merge
    // adjacent sibling lists on re-parse (carve issue 286).
    let delim = node.delim.unwrap_or('.');
    let bullet = node.bullet_char.unwrap_or('-');
    for (idx, item) in node.items.iter().enumerate() {
        // NO absolute depth term. The parent item's continuation prefix is
        // already the child list's indentation, so adding `"  " * (depth - 1)`
        // on top indented every level twice - and the two-space strip below was
        // compensating for it. Output grew as O(depth^3) where the source is
        // O(depth^2), and `05-lists-5` came back with four spaces where it was
        // written with two (carve-rs#594, the same defect carve-js fixed in its
        // #653).
        let mut prefix = if node.ordered {
            let marker = if node.bare_marker {
                String::new()
            } else {
                ordered_marker(counter, node.ol_type)
            };
            counter += 1;
            format!("{marker}{delim} ")
        } else if let Some(checked) = item.checked {
            format!("{bullet} [{}] ", if checked { "x" } else { " " })
        } else {
            format!("{bullet} ")
        };
        let item_attrs = render_attrs(&item.attrs);
        if !item_attrs.is_empty() {
            prefix = if node.ordered {
                format!("{}{item_attrs} ", prefix.trim_end())
            } else if let Some(checked) = item.checked {
                format!(
                    "{bullet}{item_attrs} [{}] ",
                    if checked { "x" } else { " " }
                )
            } else {
                format!("{bullet}{item_attrs} ")
            };
        }
        let mut content = render_item_blocks(&item.children, node.tight, ctx);
        let trimmed_content = trim_non_nbsp(&content);
        if trimmed_content.is_empty()
            || (trimmed_content.starts_with("[^") && trimmed_content.contains(": "))
        {
            content = "+".to_string();
        }
        let content = trim_non_nbsp(&content).to_string();
        let mut lines = if content.is_empty() {
            vec!["".to_string()]
        } else {
            content.split('\n').map(str::to_string).collect()
        };
        let first = lines.remove(0);
        out.push_str(&format!("{prefix}{first}\n"));
        let continuation = " ".repeat(prefix.len());
        for line in lines {
            if line.is_empty() || line.chars().eq([verbatim_blank()]) {
                // A blank continuation line is emitted EMPTY, never indented to
                // the content column: PART 11 section 7 forbids a whitespace-only
                // line, because editors and CI that strip trailing whitespace
                // rewrite one, and `fmt` would then report a diff on a file
                // nobody edited (carve#375).
                //
                // A blank line INSIDE verbatim content arrives as the sentinel
                // rather than as "", because `protect_verbatim` encodes it to
                // keep whole-document normalization off it. That made it look
                // like content here, so it was indented, and the indent stayed
                // behind when the sentinel was restored to nothing - a
                // whitespace-only line, from a code block in a list item
                // (carve-rs#440). The sentinel is written through UNindented so
                // it keeps protecting the line it stands for.
                out.push_str(&line);
                out.push('\n');
            } else if let Some(rest) = line.strip_prefix(MARKER_COLUMN) {
                // The continuation marker and its attached block sit at the
                // ITEM's marker column, not its content column (§17 L3).
                out.push_str(&format!("{rest}\n"));
            } else {
                out.push_str(&format!("{continuation}{line}\n"));
            }
        }
        let ends_with_nested_list = content.lines().last().is_some_and(|line| {
            line.starts_with(' ') && is_rendered_list_marker(line.trim_start())
        });
        if !node.tight && idx < node.items.len() - 1 && !ends_with_nested_list {
            out.push('\n');
        }
    }
    ctx.list_depth -= 1;
    trim_end_non_nbsp(&out).to_string()
}

fn ordered_marker(n: usize, ty: Option<OrderedListType>) -> String {
    match ty {
        Some(OrderedListType::LowerAlpha) => alpha_marker(n, false),
        Some(OrderedListType::UpperAlpha) => alpha_marker(n, true),
        Some(OrderedListType::LowerRoman) => roman_marker(n).to_ascii_lowercase(),
        Some(OrderedListType::UpperRoman) => roman_marker(n),
        None => n.to_string(),
    }
}

fn is_rendered_list_marker(line: &str) -> bool {
    line.starts_with("- ")
        || line.starts_with("* ")
        || line.starts_with("- [")
        || line.starts_with("* [")
        || [". ", ") "].iter().any(|sep| {
            line.split_once(sep).is_some_and(|(marker, _)| {
                !marker.is_empty() && marker.chars().all(|ch| ch.is_ascii_alphanumeric())
            })
        })
}

fn alpha_marker(n: usize, upper: bool) -> String {
    let base = ((n.saturating_sub(1) % 26) as u8) + if upper { b'A' } else { b'a' };
    (base as char).to_string()
}

fn roman_marker(mut n: usize) -> String {
    let values = [
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];
    let mut out = String::new();
    for (value, token) in values {
        while n >= value {
            out.push_str(token);
            n -= value;
        }
    }
    if out.is_empty() {
        "I".to_string()
    } else {
        out
    }
}

fn render_definition_list(items: &[DefinitionItem], ctx: &mut CarveContext) -> String {
    let mut out = Vec::new();
    for item in items {
        for term in &item.terms {
            out.push(format!(":: {}", render_inlines(term, ctx)));
        }
        for def in &item.definitions {
            // An EMPTY description whose line carries a hoisted definition is one
            // the author wrote that definition on: write it back there
            // (markup-carve/carve#805). Without this the line came out as a bare
            // `:`, which re-parses into the term above it.
            if def.children.is_empty() {
                let line = def.pos.as_ref().map(|pos| pos.start_line);
                let written = line
                    .and_then(|line| ctx.definitions_by_line.get(&line).cloned())
                    .map(|definition| match definition {
                        DefinitionAtLine::Link(def) => {
                            render_block(&BlockNode::LinkReferenceDefinition(*def), ctx)
                        }
                        DefinitionAtLine::Footnote(label, blocks) => {
                            render_footnote_def_source(&label, &blocks, ctx)
                        }
                    });
                if let (Some(line), Some(written)) = (line, written) {
                    ctx.written_in_place.insert(line);
                    let mut written_lines = written.split('\n');
                    out.push(format!(":  {}", written_lines.next().unwrap_or_default()));
                    // A footnote body can be multi-line; its continuation lines
                    // carry the body's own indent and sit under the description.
                    for written_line in written_lines {
                        out.push(format!("   {written_line}"));
                    }
                    continue;
                }
            }
            let body = trim_non_nbsp(&render_blocks(def, ctx)).to_string();
            let mut lines = body.split('\n');
            out.push(format!(":  {}", lines.next().unwrap_or_default()));
            for line in lines {
                out.push(format!("   {line}"));
            }
        }
    }
    out.join("\n")
}

fn colon_fence_for(ctx: &CarveContext) -> String {
    ":".repeat(3 + ctx.colon_fence_depth)
}

/// Tables prefer the NATIVE header form: an `=` on each header cell, plus the
/// per-cell `<`/`>`/`~` alignment markers.
///
/// The GFM delimiter row is an accepted alias on input, but it says something
/// the AST does not: its alignment applies to the WHOLE column, header and body
/// alike (PART 9 T7), while alignment on the AST belongs to each cell. Writing a
/// delimiter row for the ordinary shape - an aligned header over unaligned body
/// cells - brought every body cell back aligned, so `parse(fmt(x)) == parse(x)`
/// did not hold (carve issue 359).
///
/// Two header shapes have no native spelling, because `header_cell` in the
/// grammar is `'=' [alignment_marker] content` and admits neither an attribute
/// block nor a span marker:
///
/// ```text
/// | < | b |     a span marker promoted to a header cell
/// |{.x} a | b | a header cell carrying attributes
/// ```
///
/// Those still need a delimiter row to promote the first row. It is emitted BARE
/// (`|---|---|`), never with colons: the cells keep their own alignment markers,
/// so the delimiter contributes structure only and cannot spill alignment down
/// the column.
fn render_table(node: &Table, ctx: &mut CarveContext) -> String {
    let mut rows = Vec::new();
    let header_row = node
        .rows
        .first()
        .is_some_and(|row| !row.cells.is_empty() && row.cells.iter().all(|cell| cell.header));
    let needs_delimiter = header_row
        && node.rows.first().is_some_and(|row| {
            row.cells
                .iter()
                .any(|cell| cell.span.is_some() || cell.attrs.is_some())
        });

    for (row_index, row) in node.rows.iter().enumerate() {
        let mut cells = Vec::new();
        for cell in &row.cells {
            // In the delimiter form the promoted row is written as ordinary
            // data cells - the row after it is what makes them headers.
            let mark_header = !(needs_delimiter && row_index == 0);
            cells.push(render_table_cell(cell, ctx, mark_header));
        }
        rows.push(render_table_row(&cells, &render_attrs(&row.attrs)));
    }
    if needs_delimiter {
        let sep = vec!["---"; node.rows[0].cells.len()].join("|");
        rows.insert(1, format!("|{sep}|"));
    }
    if let Some(caption) = &node.caption {
        rows.push(format!("^ {}", render_inlines(caption, ctx)));
    }
    rows.join("\n")
}

struct RenderedCell {
    text: String,
    tight: bool,
}

fn render_table_row(cells: &[RenderedCell], attrs: &str) -> String {
    format!(
        "|{}|{}",
        cells
            .iter()
            .map(|cell| {
                if cell.tight {
                    cell.text.clone()
                } else {
                    format!(" {} ", cell.text)
                }
            })
            .collect::<Vec<_>>()
            .join("|"),
        attrs
    )
}

fn render_table_cell(cell: &TableCell, ctx: &mut CarveContext, mark_header: bool) -> RenderedCell {
    let attrs = render_attrs(&cell.attrs);
    // A lone span marker keeps a SPACE before it. Glued to the opening pipe, `<`
    // is also the left-alignment sigil, and the two readings differ: the
    // executable spec reads `|<|` as alignment on an empty cell where all three
    // engines read a colspan (markup-carve/carve#710). `alignment_marker` is defined
    // as glued and `colspan_marker` may carry surrounding whitespace, so the
    // padded form means the same thing to every reader and the writer must not
    // emit the ambiguous one. `^` is not an alignment sigil, but takes the same
    // shape so a row of span cells stays readable.
    //
    // A cell attribute stays GLUED to the pipe, where the grammar puts it; the
    // space goes between it and the marker.
    if let Some(span) = cell.span {
        let marker = if span == TableCellSpan::Rowspan {
            "^"
        } else {
            "<"
        };
        return if attrs.is_empty() {
            RenderedCell {
                text: marker.to_string(),
                tight: false,
            }
        } else {
            RenderedCell {
                text: format!("{attrs} {marker}"),
                tight: true,
            }
        };
    }
    let align = align_marker(cell.align);
    let prefix = format!(
        "{}{}{}",
        attrs,
        if cell.header && mark_header { "=" } else { "" },
        align
    );
    ctx.table_cell_depth += 1;
    let content = render_inlines(&cell.children, ctx);
    ctx.table_cell_depth -= 1;
    // A MARKER AND THE CONTENT'S FIRST CHARACTER CAN MERGE INTO A LONGER MARKER
    // RUN. The header `=` is read glued to the pipe and the ALIGNMENT sigil is
    // read glued after it, off the UNTRIMMED cell - so a prefix carrying no
    // alignment of its own hands its next character to the alignment reader.
    // `| ~x~ |`, a header cell holding a strikethrough, came back as `|=~x~|`,
    // which re-reads as a CENTERED column holding the text `x~`: the
    // strikethrough gone and every cell in the column centered by a marker
    // nobody wrote (carve-rs#819).
    //
    // ONE SPACE PARTS THEM, and it costs nothing: only the marker's position
    // relative to the PIPE is significant, and the reader trims the padding
    // after it. Escaping the character instead would be wrong - `~` opens a real
    // strikethrough here, so a backslash would change the content rather than
    // protect it. `<` and `>` reach this already escaped as literal text, which
    // is why `~` was the only spelling that broke.
    let separator =
        if !prefix.is_empty() && align.is_empty() && content.starts_with(['<', '>', '~']) {
            " "
        } else {
            ""
        };
    RenderedCell {
        text: format!("{prefix}{separator}{content}"),
        tight: !prefix.is_empty(),
    }
}

fn render_figure(node: &Figure, ctx: &mut CarveContext) -> String {
    let target = match &node.target {
        FigureTarget::Image(image) => render_image(image),
        FigureTarget::Table(table) => render_table(table, ctx),
        FigureTarget::BlockQuote(quote) => render_block(&BlockNode::BlockQuote(quote.clone()), ctx),
        FigureTarget::CodeBlock(code) => render_block(&BlockNode::CodeBlock(code.clone()), ctx),
        FigureTarget::Paragraph(paragraph) => {
            render_block(&BlockNode::Paragraph(paragraph.clone()), ctx)
        }
    };
    format!("{target}\n^ {}", render_inlines(&node.caption, ctx))
}

fn render_footnote_def_source(label: &str, blocks: &[BlockNode], ctx: &mut CarveContext) -> String {
    // A bare `[^label]:` is paragraph text, not a definition. PART 11 §7b
    // gives an empty definition an explicit spelling so formatting preserves
    // the definition and references to it keep resolving.
    if blocks.is_empty() {
        return format!("[^{}]: {{empty}}", escape_footnote_label(label));
    }
    let raw_body = render_blocks(blocks, ctx);
    let single_body;
    let body = trim_non_nbsp(if blocks.len() == 1 {
        single_body = raw_body.replace("\n\n", "\n");
        &single_body
    } else {
        &raw_body
    })
    .to_string();
    // A body holding NO blocks takes the SENTINEL `{empty}` (PART 11 §7b).
    //
    // `[^f]:` with nothing after the colon is not a definition at all -- MARKER
    // REQUIRES CONTENT (PART 2) -- so writing it degrades the definition to a
    // paragraph and every reference to it to literal text. §1a is what licenses
    // departing from the per-construct spelling: the emitted bytes have to
    // re-parse to the tree they came from.
    //
    // The sentinel has to be a VALID ATTRIBUTE BLOCK, which is why it is not
    // `{ }` or `{}`: a block-attribute line requires at least one attribute, so
    // both of those stay literal text inside the note. `{empty}` is a boolean
    // attribute, collected on the definition line and discarded with the rest
    // of the note's pending attributes, so it reaches neither the endnote item
    // nor anything after it.
    if body.is_empty() {
        return format!("[^{}]: {{empty}}", escape_footnote_label(label));
    }
    let mut lines = body.split('\n');
    let mut def_lines = vec![format!(
        "[^{}]: {}",
        escape_footnote_label(label),
        lines.next().unwrap_or_default()
    )];
    for line in lines {
        // TWO spaces, the body's own column (PART 9 §16). A wider indent is legal
        // continuation but leaves the body's blocks at a relative column above
        // zero, and an indented block opener does not open a block - so a table
        // or a quote written at three came back as a paragraph.
        def_lines.push(format!("  {line}"));
    }
    def_lines.join("\n")
}

fn render_inlines(nodes: &[InlineNode], ctx: &mut CarveContext) -> String {
    render_inlines_with_caption(nodes, ctx, false)
}

fn render_inlines_with_caption(
    nodes: &[InlineNode],
    ctx: &mut CarveContext,
    mut caption_can_open: bool,
) -> String {
    if ctx.inline_depth >= MAX_RENDER_DEPTH {
        crate::render_depth::record("carve");
        return String::new();
    }
    ctx.inline_depth += 1;
    let mut out = String::new();
    let mut first_line = true;
    let mut line_node_count = 0usize;
    let mut line_hosts_caption = false;
    for (idx, node) in nodes.iter().enumerate() {
        let prev = idx
            .checked_sub(1)
            .and_then(|i| last_boundary(&nodes[i]))
            .unwrap_or_default();
        let next = nodes
            .get(idx + 1)
            .and_then(first_boundary)
            .unwrap_or_default();
        let rendered = render_inline(node, ctx, prev, next, caption_can_open);
        // A COMMENT'S SEPARATING SPACE IS DECIDED ON THE EMITTED BYTES, not on
        // the previous NODE (carve#1028). `%%` opens a comment only at the
        // start of a line or after whitespace, so the writer owes one space
        // whenever anything has already been written on this line. Asking the
        // previous node for its last character cannot answer that: emphasis, a
        // link, an image and a span all report NO boundary character, which is
        // indistinguishable from "nothing precedes me" - so `{,y,} %% c` came
        // back as `{,y,}%% c`, and re-parsing carve-rs's own output turned the
        // comment into literal text. PART 11 section 1a states the test: read
        // the bytes the writer just produced, not the source it came from.
        if matches!(node, InlineNode::Comment(_)) && needs_comment_space(&out) {
            out.push(' ');
        }
        out.push_str(&rendered);
        if matches!(node, InlineNode::SoftBreak(_)) {
            caption_can_open = first_line && line_node_count == 1 && line_hosts_caption;
            first_line = false;
            line_node_count = 0;
            line_hosts_caption = false;
        } else {
            line_node_count += 1;
            line_hosts_caption = line_node_count == 1 && inline_hosts_caption(node);
            caption_can_open = false;
        }
    }
    ctx.inline_depth -= 1;
    out
}

fn inline_hosts_caption(node: &InlineNode) -> bool {
    match node {
        InlineNode::Image(image) => !image.src.is_empty(),
        InlineNode::Math(math) => math.display,
        _ => false,
    }
}

fn render_inline(
    node: &InlineNode,
    ctx: &mut CarveContext,
    prev_char: char,
    next_char: char,
    caption_can_open: bool,
) -> String {
    match node {
        // The one target that publishes it: the author wrote `%% note`, and
        // the canonical form writes it back verbatim. The parser drops the
        // whitespace before the marker (it is not part of the text); the space
        // that puts it back is decided in `render_inlines`, on the bytes
        // already emitted for this line.
        InlineNode::Comment(c) => format!("%% {}", c.content),
        InlineNode::Text(text) => escape_text(
            &resolve_nbsp_placeholder(&text.value, ctx.line_block_depth > 0),
            ctx.escape_mode,
            // Does this node's first character sit at the start of a block
            // line? Only there can a `^` be read back as a caption marker.
            (prev_char == '\0' || prev_char == '\n') && ctx.table_cell_depth == 0,
            caption_can_open && ctx.table_cell_depth == 0,
            prev_char,
            next_char,
        ),
        InlineNode::EscapedText(text) => format!("\\{}", text.value),
        InlineNode::SmartPunctuation(s) => s.value.clone(),
        InlineNode::Emphasis(emphasis) => {
            let content = render_inlines(&emphasis.children, ctx);
            let (delim, body) = match emphasis.kind {
                EmphasisKind::Italic => ("/", render_emphasis("/", &content, prev_char, next_char)),
                EmphasisKind::Strong => ("*", render_emphasis("*", &content, prev_char, next_char)),
                EmphasisKind::Underline => {
                    ("_", render_emphasis("_", &content, prev_char, next_char))
                }
                EmphasisKind::Strike => ("~", render_emphasis("~", &content, prev_char, next_char)),
                EmphasisKind::Super => ("^", render_forced_emphasis("^", &content)),
                EmphasisKind::Sub => (",", render_forced_emphasis(",", &content)),
                EmphasisKind::Highlight => {
                    ("=", render_emphasis("=", &content, prev_char, next_char))
                }
                EmphasisKind::BoldItalic => ("", format!("/*{content}*/")),
            };
            let _ = delim;
            format!("{body}{}", render_attrs(&emphasis.attrs))
        }
        InlineNode::Code(code) => {
            format!("{}{}", render_code(&code.value), render_attrs(&code.attrs))
        }
        InlineNode::Link(link) => render_link(link, ctx),
        InlineNode::Image(image) => render_image(image),
        InlineNode::Span(span) => {
            let attrs = render_attrs(&span.attrs);
            format!(
                "[{}]{}",
                render_inlines(&span.children, ctx),
                if attrs.is_empty() { "{}" } else { &attrs }
            )
        }
        InlineNode::Math(math) => format!(
            "{}{}{}",
            if math.display { "$$" } else { "$" },
            render_code(&math.content),
            render_attrs(&math.attrs)
        ),
        InlineNode::RawInline(raw) => {
            format!(
                "{}{{={}}}",
                render_code(&raw.content),
                escape_format(&raw.format)
            )
        }
        InlineNode::LiteralInline(lit) => {
            // §27: `!` prefix on a verbatim span. A trailing attribute block is
            // the ordinary inline attribute block (same as a code span carries).
            // `render_code` widens the backtick fence when the content holds
            // backticks, so the round-trip re-parses identically.
            format!("!{}{}", render_code(&lit.content), render_attrs(&lit.attrs))
        }
        InlineNode::Symbol(symbol) => format!(
            ":{}:{}",
            escape_symbol_name(&symbol.name),
            render_attrs(&symbol.attrs)
        ),
        InlineNode::AutoLink(link) => {
            // Emit the raw autolink content verbatim (keeps a URI scheme like
            // `mailto:`), so it re-parses to the same autolink.
            format!(
                "<{}>{}",
                escape_autolink_href(&link.text),
                render_attrs(&link.attrs)
            )
        }
        InlineNode::Mention(mention) => format!("@{}", escape_name(&mention.user)),
        InlineNode::Tag(tag) => format!("#{}", escape_name(&tag.name)),
        InlineNode::Extension(extension) => format!(
            ":{}[{}]{}",
            escape_identifier(&extension.name),
            render_inlines(&extension.children, ctx),
            render_attrs(&extension.attrs)
        ),
        InlineNode::Abbreviation(abbr) => {
            escape_text(&abbr.abbr, ctx.escape_mode, false, false, '\0', '\0')
        }
        InlineNode::Footnote(footnote) => {
            let body = if let Some(inline) = &footnote.inline {
                format!("^[{}]", render_inlines(inline, ctx))
            } else {
                format!(
                    "[^{}]",
                    escape_footnote_label(footnote.id.as_deref().unwrap_or_default())
                )
            };
            format!("{body}{}", render_attrs(&footnote.attrs))
        }
        InlineNode::SoftBreak(_) => "\n".to_string(),
        InlineNode::HardBreak(_) => {
            if ctx.line_block_depth > 0 {
                "\n".to_string()
            } else {
                "\\\n".to_string()
            }
        }
        InlineNode::CriticInsert(insert) => {
            format!(
                "{{+{}+}}{}",
                render_inlines(&insert.children, ctx),
                render_attrs(&insert.attrs)
            )
        }
        InlineNode::CriticDelete(delete) => {
            format!(
                "{{-{}-}}{}",
                render_inlines(&delete.children, ctx),
                render_attrs(&delete.attrs)
            )
        }
        InlineNode::CriticSubstitute(sub) => {
            format!(
                "{{~{}~>{}~}}",
                escape_critic_text(&sub.old_text),
                escape_critic_text(&sub.new_text)
            )
        }
        InlineNode::CriticComment(comment) => {
            format!("{{#{}#}}", escape_critic_text(&comment.text))
        }
        InlineNode::CrossRef(crossref) => {
            format!("</#{}>", escape_crossref_target(&crossref.target))
        }
        InlineNode::CaptionNumber(_) => "#".to_string(),
        InlineNode::CitationGroup(group) => group.raw.clone(),
    }
}

fn render_link(node: &Link, ctx: &mut CarveContext) -> String {
    // UNRESOLVED means no destination, not "carries a label": PART 12 §3a keeps
    // `ref` and `raw_ref` on a RESOLVED reference too, so the label alone no
    // longer answers this and a working reference round-tripped as its own
    // source instead of normalizing to the inline form (carve#597).
    // The AUTHORED source, in two cases. UNRESOLVED: there is no destination to
    // write instead. HEADING-DERIVED (PART 11 R1, carve#478): there is no
    // definition line, so the reference is the only record of what the author
    // wrote, and resolving it bakes a generated id into the source on every fmt
    // pass. An explicit definition normalizes to the inline form - its
    // definition line is dropped either way.
    //
    // A RESOLVED explicit reference now takes this path too. Inlining it
    // satisfied to_html(fmt(x)) == to_html(x) and broke PART 11 §1: `ref` and
    // `raw_ref` were absent from the reparse, and one destination became N after
    // a single pass - the duplication the definition form exists to avoid. The
    // definition line is no longer "dropped either way": §10 gives it a node and
    // render_block above writes it (carve-rs#631, carve#642).
    if node.ref_label.is_some() && node.raw_ref.is_some() {
        return node.raw_ref.clone().unwrap_or_default();
    }
    if node.from_crossref {
        if let Some(target) = node.href.strip_prefix('#') {
            return format!("</#{}>", escape_crossref_target(target));
        }
    }
    let text = render_inlines(&node.children, ctx);
    let title = node
        .title
        .as_ref()
        .map(|title| format!(" \"{}\"", escape_quoted(title)))
        .unwrap_or_default();
    format!(
        "[{text}]({}{title}){}",
        escape_destination(&node.href),
        render_attrs(&node.attrs)
    )
}

fn render_image(node: &Image) -> String {
    // An unresolved reference image round-trips via its verbatim source, exactly
    // like an unresolved reference link (render_link); `![alt]()` would change
    // the rendered text and break the to_html(fmt(x)) == to_html(x) invariant.
    //
    // A RESOLVED reference image keeps its authored form too, for the same reason
    // as a link: §10 gives the definition a node and render_block writes the line,
    // so there is no longer anything to gain by inlining - and inlining lost
    // `ref`/`raw_ref` and duplicated the destination (carve-rs#631).
    if node.ref_label.is_some() && node.raw_ref.is_some() {
        return node.raw_ref.clone().unwrap_or_default();
    }
    let title = node
        .title
        .as_ref()
        .map(|title| format!(" \"{}\"", escape_quoted(title)))
        .unwrap_or_default();
    format!(
        "![{}]({}{title}){}",
        escape_image_alt(&node.alt),
        escape_destination(&node.src),
        render_attrs(&node.attrs)
    )
}

fn render_frontmatter(frontmatter: &std::collections::BTreeMap<String, String>) -> String {
    let mut out = String::from("---");
    for (key, value) in frontmatter {
        out.push('\n');
        out.push_str(key);
        out.push_str(": ");
        out.push_str(&protect_verbatim(value));
    }
    out.push_str("\n---");
    out
}

fn render_block_comment(content: &str) -> String {
    let mut longest = 0usize;
    let mut current = 0usize;
    for ch in content.chars() {
        if ch == '%' {
            current += 1;
            longest = longest.max(current);
        } else {
            current = 0;
        }
    }
    let fence = "%".repeat(3.max(longest + 1));
    format!("{fence}\n{}\n{fence}", protect_verbatim(content))
}

// Superscript and subscript have no bare delimiter form -- always emit the
// braced `{^x^}` / `{,x,}` form.
fn render_forced_emphasis(delim: &str, content: &str) -> String {
    format!("{{{delim}{content}{delim}}}")
}

fn render_emphasis(delim: &str, content: &str, prev_char: char, next_char: char) -> String {
    let needs_forced = is_word_boundary(prev_char)
        || is_word_boundary(next_char)
        || content.starts_with(delim)
        || content.ends_with(delim)
        || content.starts_with(' ')
        || content.ends_with(' ')
        || content.is_empty();
    if needs_forced {
        format!("{{{delim}{content}{delim}}}")
    } else {
        format!("{delim}{content}{delim}")
    }
}

fn is_word_boundary(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

fn render_code(content: &str) -> String {
    let fence = safe_fence(content, 1);
    // Pad exactly where the parser strips, so the strip is reversible and fmt
    // stays idempotent; the padding sits inside the fence, so a trailing
    // attribute block still attaches to the closing run. The parser strips one
    // leading and one trailing space when the content BOTH begins and ends with
    // a space but is NOT entirely spaces (see strip_verbatim_padding in
    // parse.rs), and needs a space around backtick-adjacent content. All-space
    // content must therefore NOT be padded: it is emitted verbatim and read back
    // unchanged. Padding it instead grew the span by two spaces on every fmt
    // pass. One-sided space is left as-is (the parser only strips when both
    // sides are spaces).
    let needs_pad = content.starts_with('`')
        || content.ends_with('`')
        || (content.starts_with(' ')
            && content.ends_with(' ')
            && !content.chars().all(|c| c == ' '));
    if needs_pad {
        format!("{fence} {content} {fence}")
    } else {
        format!("{fence}{content}{fence}")
    }
}

fn code_fence_info(lang: Option<&str>, title: Option<&str>, label: Option<&str>) -> String {
    let mut parts = Vec::new();
    if let Some(lang) = lang.filter(|s| !s.is_empty()) {
        parts.push(escape_fence_token(lang));
    }
    if let Some(title) = title {
        parts.push(format!("\"{}\"", escape_quoted(title)));
    }
    if let Some(label) = label {
        parts.push(format!("[{}]", escape_bracket_text(label)));
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!(" {}", parts.join(" "))
    }
}

fn safe_fence(content: &str, min: usize) -> String {
    let mut longest = 0usize;
    let mut current = 0usize;
    for ch in content.chars() {
        if ch == '`' {
            current += 1;
            longest = longest.max(current);
        } else {
            current = 0;
        }
    }
    "`".repeat(min.max(longest + 1))
}

fn render_attrs(attrs: &Option<Attrs>) -> String {
    let Some(attrs) = attrs else {
        return String::new();
    };
    let mut parts = Vec::new();
    let id_as_key = attrs.id.as_ref().is_some_and(|id| !is_attr_identifier(id));
    let mut seen_keys: Vec<&str> = Vec::new();
    let emit_id = |parts: &mut Vec<String>| {
        if let Some(id) = &attrs.id {
            if id_as_key {
                parts.push(format!("id={}", quote_attr_value(id)));
            } else {
                parts.push(format!("#{}", escape_attr_name_value(id)));
            }
        }
    };
    let emit_classes = |parts: &mut Vec<String>| {
        for cls in &attrs.classes {
            parts.push(format!(".{}", escape_attr_name_value(cls)));
        }
    };
    let emit_key = |parts: &mut Vec<String>, key: &str| {
        if let Some(value) = attrs.key_values.get(key) {
            // EXACT key match, not case-insensitive: `LANG` and `lang` are
            // different attribute names, so folding here rewrote
            // `[x]{LANG=fr}` into `[x]{:fr}` and changed the name, which
            // breaks PART 11 §1 (carve#1137).
            if key == "lang" && is_language_tag(value) {
                parts.push(format!(":{value}"));
            } else if value.is_empty() && is_attr_identifier(key) {
                // PART 11 §6c: a value-less attribute comes back as the bare
                // name, which is the production the language has for it. A key
                // needing escaping has no bare spelling to fall back to.
                parts.push(escape_attr_key(key));
            } else {
                parts.push(format!(
                    "{}={}",
                    escape_attr_key(key),
                    quote_attr_value(value)
                ));
            }
        }
    };
    if attrs.order.is_empty() {
        emit_id(&mut parts);
        emit_classes(&mut parts);
        for key in attrs.key_values.keys() {
            emit_key(&mut parts, key);
        }
    } else {
        for slot in &attrs.order {
            match slot {
                AttrSlot::Id => emit_id(&mut parts),
                AttrSlot::Class => emit_classes(&mut parts),
                AttrSlot::Key(key) => {
                    if !seen_keys.contains(&key.as_str()) {
                        emit_key(&mut parts, key);
                        seen_keys.push(key);
                    }
                }
            }
        }
        for key in attrs.key_values.keys() {
            if !seen_keys.contains(&key.as_str()) {
                emit_key(&mut parts, key);
            }
        }
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!("{{{}}}", parts.join(" "))
    }
}

fn is_language_tag(value: &str) -> bool {
    value.is_empty()
        || value.split('-').all(|subtag| {
            !subtag.is_empty()
                && subtag.len() <= 8
                && subtag.bytes().all(|byte| byte.is_ascii_alphanumeric())
        })
}

fn quote_attr_value(value: &str) -> String {
    if !value.is_empty()
        && value
            .chars()
            .all(|ch| !ch.is_whitespace() && !matches!(ch, '"' | '\'' | '{' | '}'))
    {
        value.to_string()
    } else {
        format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
    }
}

fn align_marker(align: Option<TableAlign>) -> &'static str {
    match align {
        Some(TableAlign::Left) => "<",
        Some(TableAlign::Right) => ">",
        Some(TableAlign::Center) => "~",
        None => "",
    }
}

/// The staging characters an AUTHORED occurrence can be mistaken for.
///
/// Five, in two groups, and both groups have the same failure:
///
///   VERBATIM_BLANK  a line that was blank inside verbatim content
///   THEMATIC_GUARD  prefixes a line that would re-parse as a thematic break
///   ESCAPED_SPACE   stands in for `\ ` until normalize expands it
///   STAGED_SPACE    a space that must survive escaping
///   STAGED_TAB      a tab that must survive escaping
///
/// Why `ESCAPED_SPACE` exists at all: an escaped space is written back AS an
/// escape, not as a real U+00A0. Resolving it to the character lost the
/// distinction the parser draws - `10\ kg` came back carrying a literal nbsp,
/// which re-parses as text rather than as an escape, so the node differed even
/// though the HTML did not (carve#352, corpus 29-non-breaking-space). It
/// resolves in `normalize` rather than during rendering because the backslash
/// it expands to is itself an unconditional escape, and expanding earlier let
/// the escaper double it.
///
/// The last three live at U+E010 and up deliberately. U+E000 is a PUBLISHED
/// value - the no-break-space placeholder a parsed document carries - so a
/// writer marker sharing it would be indistinguishable from document content.
/// They used to sit at U+E001 and U+E002 (carve-rs#404).
///
/// The first two are undone BY POSITION - a line that is nothing but the
/// marker, and a line prefix. The last three are undone by a GLOBAL replace,
/// because each has more than one insertion site. Either way a character the
/// author wrote is indistinguishable from one the writer inserted, and restore
/// ate it: carve-rs#607 for the positional pair, carve-rs#630 for the rest.
///
/// Narrowing the positional pair to its exact sites (carve-rs#613) fixed every
/// INLINE placement and could not fix the line-alone one, because that
/// ambiguity IS positional. The global three have no narrowing available at
/// all. So the CHARACTER moves instead: the writer counts what it inserts, and
/// if the document holds more than that, the extra ones are the author's and
/// the render repeats with characters the document does not contain. carve-js
/// reached the same place from the other side in markup-carve/carve-js#666.
///
/// A document with no private-use character - every real one - takes the first
/// render and pays five integer compares.
const SENTINEL_DEFAULTS: [char; 5] = ['\u{e003}', '\u{e004}', '\u{e010}', '\u{e011}', '\u{e012}'];

const S_BLANK: usize = 0;
const S_GUARD: usize = 1;
const S_ESCAPED_SPACE: usize = 2;
const S_STAGED_SPACE: usize = 3;
const S_STAGED_TAB: usize = 4;

thread_local! {
    static SENTINELS: std::cell::Cell<[char; 5]> = const { std::cell::Cell::new(SENTINEL_DEFAULTS) };
    /// How many of each the writer inserted during the current render.
    static INSERTED: std::cell::Cell<[usize; 5]> = const { std::cell::Cell::new([0; 5]) };
    /// How many of each were actually PRESENT just before restore ran. Counted
    /// there and not at the end, because restore is what consumes them: by the
    /// time the render returns, an authored one has already been eaten and is
    /// indistinguishable from never having been there.
    static SEEN: std::cell::Cell<[usize; 5]> = const { std::cell::Cell::new([0; 5]) };
    /// The pre-restore text, kept so a replacement can be chosen against what
    /// the document actually holds.
    static STAGED: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
}

fn sentinel(which: usize) -> char {
    SENTINELS.with(|s| s.get()[which])
}

fn note_inserted(which: usize) {
    INSERTED.with(|c| {
        let mut n = c.get();
        n[which] += 1;
        c.set(n);
    });
}

fn verbatim_blank() -> char {
    sentinel(S_BLANK)
}

fn thematic_guard() -> char {
    sentinel(S_GUARD)
}

fn escaped_space() -> String {
    sentinel(S_ESCAPED_SPACE).to_string()
}

fn staged_space() -> char {
    sentinel(S_STAGED_SPACE)
}

fn staged_tab() -> char {
    sentinel(S_STAGED_TAB)
}

fn free_sentinel(text: &str, taken: &[char; 5]) -> char {
    ('\u{e020}'..='\u{f8ff}')
        .find(|c| !taken.contains(c) && !text.contains(*c))
        .unwrap_or('\u{f8ff}')
}

fn resolve_nbsp_placeholder(text: &str, in_line_block: bool) -> String {
    if !in_line_block {
        let marker = escaped_space();
        for _ in text.matches(crate::NBSP_PLACEHOLDER) {
            note_inserted(S_ESCAPED_SPACE);
        }
        return text.replace(crate::NBSP_PLACEHOLDER, &marker);
    }
    text.split('\n')
        .map(stage_line_block_layout)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Write a line block's preserved whitespace back as plain spaces.
///
/// The runs staged here are exactly the ones the parser reproduces from plain
/// spaces: a LEADING run of any width, and a medial or trailing run of two or
/// more (grammar §23). A lone medial placeholder can then only have come from
/// an escaped space, so `a\ b` still round-trips as written. Two ADJACENT
/// escaped spaces are the one form that changes - `a\ \ b` is written back as
/// `a  b` - because inside a line block those are the same document: both parse
/// to the same pair of placeholders.
fn stage_line_block_layout(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut seen_content = false;
    let mut chars = line.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch != crate::NBSP_PLACEHOLDER {
            out.push(ch);
            seen_content = true;
            continue;
        }

        let mut run = 1usize;
        while chars.peek() == Some(&crate::NBSP_PLACEHOLDER) {
            chars.next();
            run += 1;
        }

        if !seen_content || run >= 2 {
            for _ in 0..run {
                note_inserted(S_STAGED_SPACE);
                out.push(staged_space());
            }
        } else {
            // A single placeholder mid-line is an escaped space, not layout.
            note_inserted(S_ESCAPED_SPACE);
            out.push_str(&escaped_space());
        }
    }

    out
}

fn normalize(text: &str) -> String {
    // Count the escaped-space marker BEFORE the replace below consumes it.
    // Everything else is counted further down, just before `restore_verbatim`,
    // but this one is resolved first and would already be gone by then - which
    // is exactly how an authored U+E010 went on being eaten after the other
    // four were fixed (carve-rs#630).
    let marker = escaped_space();
    STAGED.with(|c| c.borrow_mut().push_str(text));
    SEEN.with(|c| {
        let mut n = c.get();
        n[S_ESCAPED_SPACE] += text.matches(&marker).count();
        c.set(n);
    });
    // U+E010 marks an escaped space, and it resolves HERE rather than during
    // rendering because the backslash it expands to is itself an unconditional
    // escape: expanding earlier let escapeText double it, giving `10\\ kg`.
    // An escaped space at end of line has already lost its trailing SPACE by
    // PART 11 §2a: canonical source must not depend on editors preserving that
    // byte. Expand it to the bare backslash in every container, not only at
    // document level. The list writer used to indent first and preserve the
    // expanded space as mid-paragraph content (carve-rs#855).
    let mut expanded = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == sentinel(S_ESCAPED_SPACE) {
            expanded.push('\\');
            if !matches!(chars.peek(), None | Some('\n')) {
                expanded.push(' ');
            }
        } else {
            expanded.push(ch);
        }
    }
    let text = expanded;
    // Strip a line's trailing whitespace only where it cannot be content. At the
    // end of a paragraph the parser drops it too, so the writer must; before a
    // SOFT BREAK the parser keeps it, and stripping it there changed the
    // rendered output (carve#359). A line whose successor is blank ends its
    // block; one followed by more text is mid-paragraph.
    let trimmed = trim_non_nbsp(&text);
    let raw: Vec<&str> = trimmed.split('\n').collect();
    let lines = raw
        .iter()
        .enumerate()
        .map(|(i, line)| {
            // A line whose only content is ASCII space or tab is emitted EMPTY,
            // wherever it sits (PART 11 section 7). Editors and CI that strip
            // trailing whitespace rewrite such a line, so `fmt` would report a
            // diff on a file nobody edited (carve#375). This is separate from
            // the block-final rule below, which is about a line WITH content:
            // that whitespace can be document content, and stripping it before
            // a soft break changed rendered output (carve#359).
            if !line.is_empty() && line.trim_matches([' ', '\t']).is_empty() {
                return String::new();
            }
            let ends_block = raw.get(i + 1).map_or(true, |next| next.trim().is_empty());
            if ends_block {
                trim_end_non_nbsp(line).to_string()
            } else {
                (*line).to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    let staged = trim_non_nbsp(&collapse_blank_lines(&lines)).to_string();
    let current = SENTINELS.with(|s| s.get());
    STAGED.with(|c| c.borrow_mut().push_str(&staged));
    SEEN.with(|c| {
        let mut n = c.get();
        for i in 0..5 {
            // The escaped-space marker was counted at the top of `normalize`,
            // before the replace that consumes it.
            if i != S_ESCAPED_SPACE {
                n[i] += staged.matches(current[i]).count();
            }
        }
        c.set(n);
    });
    format!("{}\n", restore_verbatim(&staged))
}

/// Whole-document normalization (trailing-whitespace strip, blank-line
/// collapsing) must not reach inside verbatim content - code blocks, raw
/// blocks, frontmatter, and block comments reproduce their content byte-exact
/// (carve-js issue 340). Sentinel-encode the vulnerable bytes before the
/// content joins the document string; `normalize` restores them at the end.
/// U+E000 is already the NBSP sentinel; U+E001..U+E003 extend the scheme.
fn protect_verbatim(content: &str) -> String {
    let mut lines = Vec::new();
    for line in content.split('\n') {
        if line.is_empty() {
            note_inserted(S_BLANK);
            lines.push(verbatim_blank().to_string());
            continue;
        }
        let stripped = line.trim_end_matches([' ', '\t']);
        let tail: String = line[stripped.len()..]
            .chars()
            .map(|ch| {
                if ch == ' ' {
                    note_inserted(S_STAGED_SPACE);
                    staged_space()
                } else {
                    note_inserted(S_STAGED_TAB);
                    staged_tab()
                }
            })
            .collect();
        lines.push(format!("{stripped}{tail}"));
    }
    lines.join("\n")
}

/// Protect a paragraph line that would re-parse as a thematic break.
///
/// Source indentation is not in the AST, so an indented `---` - a paragraph
/// holding an em dash - is emitted at column 0, where it stops being a
/// paragraph and becomes a thematic break.
///
/// Text nodes are already covered: the conservative form escapes the hyphens,
/// so the round-trip check sees the difference and picks that form. A
/// smart-punctuation run is not, because its source run is emitted verbatim in
/// BOTH forms - that is the point of the node - so the check never has a
/// difference to act on. Escaping the run in the conservative form does not
/// work either: it would make that form change the document, after which the
/// check could never prefer the minimal one.
///
/// It marks rather than escapes: escaping would split the run (a leading
/// escaped hyphen plus an en dash) and change the document just as surely,
/// while a leading space keeps the line a paragraph and keeps the em dash -
/// which is what the source said. The marker is a sentinel because normalize()
/// trims the document's leading whitespace, which would silently undo the guard
/// whenever the paragraph is the first block.
fn guard_thematic_break_lines(body: &str) -> String {
    if !body.contains('-') {
        return body.to_string();
    }
    body.split('\n')
        .map(|line| {
            let trimmed = line.trim_end_matches([' ', '\t']);
            if trimmed.len() >= 3 && trimmed.chars().all(|c| c == '-') {
                note_inserted(S_GUARD);
                format!("{}{line}", thematic_guard())
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Undo `protect_verbatim` and the thematic-break guard, POSITIONALLY.
///
/// This used to be four global `replace` calls, which cannot tell a sentinel the
/// writer inserted from one the AUTHOR wrote. So an authored U+E003 was deleted
/// and an authored U+E004 became a space - in 16 of 17 constructs measured, not
/// just in a code block (carve-rs#607).
///
/// Each sentinel is only ever inserted in ONE position, so each is only undone
/// there:
///
///   VERBATIM_BLANK  a line consisting of nothing else (protect_verbatim emits it
///                   for an empty line, and never inside one)
///   U+E004          a line PREFIX (guard_thematic_break_lines prepends it)
///   STAGED_SPACE    within the TRAILING whitespace run of a line, which is the
///   STAGED_TAB      only place protect_verbatim stages them
///
/// That leaves a much smaller residue than the global form: an authored sentinel
/// still collides if it sits in the exact position the writer uses one. Closing
/// that needs the insertion COUNTS, which is the design sketched on carve-rs#607
/// - this is the part that needs no bookkeeping and no AST traversal.
fn restore_verbatim(text: &str) -> String {
    text.split('\n')
        .map(|line| {
            // The marker may arrive INDENTED: inside a container the host adds
            // its columns before this runs, so the line is `  ` + marker rather
            // than the marker alone. Testing for the marker by itself missed
            // those and left a raw U+E003 in the output - caught by
            // `verbatim_content_stable_inside_containers` and by the corpus
            // formatter's semantic check on
            // `69-opaque-spans-inside-a-container-6`.
            //
            // Drop the marker. A marker sitting next to real text is left alone,
            // which is the point.
            let prefix = line.trim_end_matches(verbatim_blank());
            if prefix.len() != line.len()
                && prefix.chars().all(|c| c == ' ' || c == '\t' || c == '>')
            {
                // `>` belongs in the set: inside a block quote the line reaching
                // here is `> ` + marker, not the marker alone, and requiring pure
                // whitespace left a raw U+E003 in the output - which
                // `verbatim_content_stable_inside_containers` and the corpus
                // formatter's semantic check both caught. A line that is nothing
                // but container prefix plus the marker is the blank the marker
                // stands for, at any nesting.
                //
                // A PURELY WHITESPACE PREFIX IS DROPPED WITH IT. PART 11 section
                // 7 emits the STRUCTURAL INDENT of an empty verbatim line as
                // nothing: "when the verbatim content on that line is EMPTY the
                // indent alone is what remains -- that is layout, and it is
                // omitted". Keeping it left a whitespace-only line, which editors
                // that strip on save, `git apply --whitespace=fix` and CI
                // whitespace checks all rewrite behind the formatter.
                //
                // The comment here used to say "a later trim removes a
                // whitespace-only line". Nothing does: `normalize` runs its
                // whitespace-only pass BEFORE this function, when the line still
                // carries the marker and so is not whitespace-only yet. That was
                // a check that could not fail, and a blank line inside a fenced
                // block under a footnote definition or a definition-list
                // description came out indented (carve#1040).
                //
                // The block-quote prefix is not layout and stays: an EMPTY line
                // would close the quote, taking the open fence with it.
                if prefix.chars().all(|c| c == ' ' || c == '\t') {
                    return String::new();
                }
                return prefix.to_string();
            }
            let line = match line.strip_prefix(thematic_guard()) {
                Some(rest) => format!(" {rest}"),
                None => line.to_string(),
            };
            // The staged pair IS positional, once you read both insertion sites
            // together rather than looking for one position:
            //
            //   protect_verbatim stages a line's TRAILING run (any length)
            //   the line-block layout path stages a LEADING run (any length) or
            //     any run of TWO OR MORE - `!seen_content || run >= 2`
            //
            // So a run the writer inserted is always leading, trailing, or at
            // least two long. A SINGLE staged character sitting mid-line is
            // therefore never the writer's, and is left alone - which is the case
            // an author hits by typing one U+E011 or U+E012 in a code block.
            //
            // My earlier attempt at this restored only the trailing run, dropped
            // the medial case and broke `line_block_medial_gaps`; the note left
            // behind said separate sentinels were needed. They are not - the
            // run-length half of the layout condition is what was missing.
            //
            // RESIDUE, stated rather than implied: an authored run of two or more,
            // or a single one at the start or end of a line, still collides. That
            // needs the insertion counts.
            restore_staged_runs(&line)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Undo the staged whitespace pair only where the writer inserts it: a LEADING
/// run, a TRAILING run, or any run of two or more (see `restore_verbatim`).
///
/// A leading run is measured past the container prefix the host may have added
/// before this runs (spaces, tabs, `>`), the same allowance the blank-line marker
/// makes a few lines above.
fn restore_staged_runs(line: &str) -> String {
    let chars: Vec<char> = line.chars().collect();
    let staged = |c: char| c == staged_space() || c == staged_tab();
    let prefix_end = chars
        .iter()
        .position(|&c| !(c == ' ' || c == '\t' || c == '>'))
        .unwrap_or(chars.len());
    let mut out = String::with_capacity(line.len());
    let mut i = 0usize;
    while i < chars.len() {
        if !staged(chars[i]) {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        let start = i;
        while i < chars.len() && staged(chars[i]) {
            i += 1;
        }
        let run = i - start;
        let writer_inserted = start == prefix_end || i == chars.len() || run >= 2;
        for &ch in &chars[start..i] {
            if writer_inserted {
                out.push(if ch == staged_space() { ' ' } else { '\t' });
            } else {
                out.push(ch);
            }
        }
    }
    out
}

fn collapse_blank_lines(text: &str) -> String {
    let mut out = String::new();
    let mut newlines = 0usize;
    for ch in text.chars() {
        if ch == '\n' {
            newlines += 1;
            if newlines <= 2 {
                out.push(ch);
            }
        } else {
            newlines = 0;
            out.push(ch);
        }
    }
    out
}

/// Fold every line break in `text` (a hard break's `\` included) to one space,
/// then trim. Used where the target construct occupies exactly one line, so a
/// break in the tree would otherwise be written out as a real newline and
/// change the block structure on re-parse.
fn collapse_breaks(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut slashes = 0usize;
    while let Some(c) = chars.next() {
        if c == '\\' {
            slashes += 1;
            out.push(c);
            continue;
        }
        if c != '\n' {
            slashes = 0;
            out.push(c);
            continue;
        }
        // Only an ODD run of backslashes before the newline is a hard break's
        // marker; an even run is literal backslashes that happen to end the
        // line. Dropping one unconditionally turned `a\` plus a soft break into
        // `a\ b`, where the escape swallows the space and the backslash is lost.
        if slashes % 2 == 1 {
            out.pop();
        }
        slashes = 0;
        // Emit one space for the break and swallow the next line's indentation.
        out.push(' ');
        while chars.peek().is_some_and(|c| *c == ' ' || *c == '\t') {
            chars.next();
        }
    }
    trim_non_nbsp(&out).to_string()
}

fn escape_text(
    text: &str,
    mode: EscapeMode,
    opens_block_line: bool,
    caption_can_open: bool,
    previous_boundary: char,
    next_boundary: char,
) -> String {
    let mut out = String::new();
    // A `^` is only dangerous where a caption marker could be read: at the
    // start of a line. Anywhere else it is literal text - superscript is
    // braced-only (`{^x^}`), so `10^6^` carries no markup - and forcing the
    // escape there put `10\^6\^` in the output where the other two engines
    // write `10^6^`. PART 11 §4 asks for the minimal form when dropping the
    // escape changes nothing, and this one changed nothing (carve-rs#555).
    //
    // Line-initial stays forced rather than left to the minimal/conservative
    // vote, because that vote is per DOCUMENT: letting `^ Figure 1` render
    // unescaped in the minimal pass makes it a caption, the two passes differ,
    // and the whole document escalates to conservative - which then escapes
    // every candidate in it, including the `:` that needs nothing. The corpus
    // pins that exact shape at 158-indented-image-and-caption-stay-literal.
    let mut at_line_start = opens_block_line;
    let mut chars = text.chars().peekable();
    let mut previous = previous_boundary;
    while let Some(ch) = chars.next() {
        // A CONTROL CHARACTER IS CONTENT, and the writer has to write it back.
        // This dropped 61 codepoints - every C0 control but tab/newline/return,
        // DEL, and the whole C1 block - none of which the parser or the HTML
        // renderer drops, so `to_html(fmt(x)) == to_html(x)` failed on any
        // document holding one. PART 2 keeps a FORM FEED and a VERTICAL TAB
        // explicitly (carve#926), and corpus
        // `261-a-blank-line-holds-spaces-and-tabs-and-nothing-else-3` pins a
        // line holding one as CONTENT rather than as a blank.
        //
        // U+0000 stays dropped, and only it: `normalize_source` removes it
        // before the parser sees it, so keeping it here would write back a byte
        // no re-parse can read. Every other control survives the round trip
        // because it survives the parse.
        //
        // This is not the Trojan-Source hardening, which is a different set in
        // a different place: `escape::is_bidi_control` strips the bidi
        // overrides and isolates (U+202A-E, U+2066-9), none of which are in the
        // range this line held.
        if ch == '\u{0000}' {
            continue;
        }
        // The caption marker is `^` followed by a SPACE. `^sup^` at the start
        // of a line is not one - superscript is braced-only, so it is literal
        // text and needs no escape, which two of this repo's own tests already
        // pinned.
        let next = chars.peek().copied().unwrap_or(next_boundary);
        // SPACE ONLY, which is what the comment above already said and what the
        // code did not do. A tab after the marker leaves the line as prose -
        // corpus
        // `231-a-tab-after-a-heading-quote-or-caption-marker-leaves-the-line-as-prose-2`
        // is that document - so `^<TAB>` re-parses as text either way and PART 11
        // §4 asks for the minimal form when dropping the escape changes nothing.
        let caret_opens_a_caption = ch == '^' && at_line_start && caption_can_open && next == ' ';
        let caret_opens_inline = ch == '^' && (next == '[' || previous == '{' || next == '}');
        // A `:` opens something only where a marker can START: `:: term`,
        // `:  def` and `::: fence` are all recognized at the beginning of a
        // line, so the FIRST colon of that run is the one that has to be
        // escaped and the rest cannot open anything.
        //
        // The conservative pass used to escape every candidate character it
        // saw, so a literal `:::` came out `\:\:\:` where carve-js and
        // carve-php write `\:::`, and `\[x\]: /u` picked up an escape on a
        // colon that no rule can read (carve-rs#566). PART 11 §4 asks for the
        // minimal form when dropping the escape changes nothing, and it
        // changes nothing for every colon but the first.
        //
        // Same shape as the caret above: ask what the character could open
        // HERE, rather than escaping the class it belongs to.
        let colon_cannot_open = ch == ':' && !at_line_start;
        at_line_start = ch == '\n';
        let unconditional =
            matches!(ch, '\\' | '`' | '"' | '\'') || caret_opens_a_caption || caret_opens_inline;
        let candidate = matches!(
            ch,
            '*' | '_'
                | '{'
                | '}'
                | '['
                | ']'
                | '('
                | ')'
                | '#'
                | '+'
                | '-'
                | '.'
                | '!'
                | '~'
                | '/'
                | '<'
                | '>'
                | '@'
                | '%'
                | '|'
                | '='
                | ':'
                | ';'
        );
        if unconditional || (mode == EscapeMode::Conservative && candidate && !colon_cannot_open) {
            out.push('\\');
        }
        out.push(ch);
        previous = ch;
    }
    out
}

fn escape_plain_line(text: &str) -> String {
    text.replace('\n', " ")
}

fn escape_image_alt(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('[', "\\[")
        .replace(']', "\\]")
}

/// Which characters the destination scan would read differently if emitted
/// bare: a parenthesis with no partner, and a backslash sitting in front of one
/// of the three escapable characters. Balanced parentheses are deliberately
/// absent -- they re-parse as themselves, and escaping them would be churn
/// against the minimal-escaping rule in PART 11 section 4.
fn unbalanced_destination_chars(text: &str) -> std::collections::HashSet<usize> {
    let mut openers: Vec<usize> = Vec::new();
    let mut marked = std::collections::HashSet::new();
    for (i, ch) in text.char_indices() {
        if ch == '(' {
            openers.push(i);
        } else if ch == ')' && openers.pop().is_none() {
            marked.insert(i);
        }
    }
    marked.extend(openers);
    marked
}

fn escape_destination(text: &str) -> String {
    let sanitize_blank = dangerous_destination_scheme(text);
    // Almost every destination holds neither a parenthesis nor a backslash, so
    // there is nothing for the scan to misread and nothing to mark. Skipping
    // the walk keeps that case free of the set entirely.
    let needs_marking = text
        .as_bytes()
        .iter()
        .any(|&b| matches!(b, b'(' | b')' | b'\\'));
    let marked = if needs_marking {
        unbalanced_destination_chars(text)
    } else {
        std::collections::HashSet::new()
    };
    let bytes = text.as_bytes();
    let mut out = String::new();
    for (i, ch) in text.char_indices() {
        let escapable =
            ch == '\\' && matches!(bytes.get(i + 1), Some(b'(') | Some(b')') | Some(b'\\'));
        if (marked.contains(&i) || escapable) && !sanitize_blank {
            out.push('\\');
        }
        match ch {
            // Whitespace is percent-encoded (it would end the destination
            // otherwise). A backslash before anything the scan does not treat
            // as an escape is emitted verbatim, so URLs carrying backslashes
            // need no doubling.
            ch if ch.is_whitespace() => {
                if ch == ' ' {
                    out.push_str("%20");
                } else {
                    out.push_str(&format!("%{:02X}", ch as u32));
                }
            }
            '(' if sanitize_blank => out.push_str("%28"),
            ')' if sanitize_blank => out.push_str("%29"),
            _ => out.push(ch),
        }
    }
    out
}

fn dangerous_destination_scheme(text: &str) -> bool {
    let trimmed = text.trim_start_matches(|ch: char| {
        ch <= '\u{0020}'
            || matches!(
                ch,
                '\u{00a0}' | '\u{1680}' | '\u{2000}'
                    ..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}'
            )
    });
    let Some(colon) = trimmed.find(':') else {
        return false;
    };
    let scheme = &trimmed[..colon];
    !scheme.is_empty()
        && scheme
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic())
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '-'))
        && matches!(
            scheme.to_ascii_lowercase().as_str(),
            "javascript" | "vbscript" | "data" | "file"
        )
}

fn escape_quoted(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

fn escape_bracket_text(text: &str) -> String {
    text.replace('\\', "\\\\").replace(']', "\\]")
}

fn escape_footnote_label(text: &str) -> String {
    escape_bracket_text(text)
}

fn escape_abbr(text: &str) -> String {
    escape_bracket_text(text)
}

fn escape_identifier(text: &str) -> String {
    text.chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '-')
        .collect()
}

// A symbol name may contain `+` and `-` (so `:+1:` / `:-1:` round-trip),
// unlike an extension identifier.
fn escape_symbol_name(text: &str) -> String {
    text.chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '+' || *ch == '-')
        .collect()
}

fn escape_name(text: &str) -> String {
    let trimmed = text.trim_matches('.');
    trimmed
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '.' || *ch == '-')
        .collect()
}

fn escape_format(text: &str) -> String {
    let safe: String = text
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '-')
        .collect();
    if safe.is_empty() {
        "text".to_string()
    } else {
        safe
    }
}

fn escape_fence_token(text: &str) -> String {
    text.split_whitespace()
        .next()
        .unwrap_or_default()
        .replace('`', "")
}

fn escape_attr_key(text: &str) -> String {
    let mut out = String::new();
    let mut started = false;
    for ch in text.chars() {
        if !started {
            if ch.is_ascii_alphabetic() || ch == '_' {
                out.push(ch);
                started = true;
            }
        } else if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
            out.push(ch);
        }
    }
    if out.is_empty() {
        "x".to_string()
    } else {
        out
    }
}

fn escape_attr_name_value(text: &str) -> String {
    text.chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
                ch
            } else {
                '-'
            }
        })
        .collect()
}

fn is_attr_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    chars
        .next()
        .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
}

fn escape_autolink_href(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('<', "\\<")
        .replace('>', "\\>")
}

fn escape_crossref_target(text: &str) -> String {
    text.replace('\\', "\\\\").replace('>', "\\>")
}

fn escape_critic_text(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('{', "\\{")
        .replace('}', "\\}")
}

fn first_boundary(node: &InlineNode) -> Option<char> {
    boundary_text(node).and_then(|s| {
        let mut chars = s.chars();
        match chars.next() {
            // In carve parse mode, text nodes preserve backslash escapes, so a
            // formatted `\_b\_` reaches us with a leading `\`. The escape marker
            // is not the adjacency-relevant character -- the escaped punctuation
            // char is. Skip a single leading backslash that escapes an ASCII
            // punctuation char so the emphasis bracing decision stays a function
            // of the semantic next character (e.g. `_`), matching `last_boundary`
            // (which already returns the escaped char) and keeping the formatter
            // idempotent and byte-identical to carve-js / carve-php.
            Some('\\') => match chars.next() {
                Some(next) if next.is_ascii_punctuation() => Some(next),
                _ => Some('\\'),
            },
            other => other,
        }
    })
}

/// Does an inline comment need a space before it, given what is already
/// emitted on its line?
///
/// Nothing emitted yet means the comment opens the run, and `%%` at the start
/// of a line is already a comment marker. Anything else that is not itself
/// whitespace has to be separated, or the marker glues to it and re-parses as
/// literal text.
fn needs_comment_space(emitted: &str) -> bool {
    match emitted.chars().next_back() {
        None => false,
        Some(last) => last != '\n' && !last.is_whitespace(),
    }
}

fn last_boundary(node: &InlineNode) -> Option<char> {
    boundary_text(node).and_then(|s| s.chars().next_back())
}

fn boundary_text(node: &InlineNode) -> Option<&str> {
    match node {
        InlineNode::Text(text) => Some(&text.value),
        // The CHARACTER, not the backslash that precedes it in the output. A
        // text node holding `_b_` and an escaped-text node holding `_` describe
        // the same neighbour, and the writer has to brace an adjacent delimiter
        // the same way for both - otherwise the first pass (plain text) and the
        // second (escaped text) disagree and `fmt(fmt(x)) != fmt(x)`.
        InlineNode::EscapedText(text) => Some(&text.value),
        InlineNode::SmartPunctuation(s) => Some(&s.value),
        InlineNode::Code(text) => Some(&text.value),
        InlineNode::Abbreviation(abbr) => Some(&abbr.abbr),
        InlineNode::Mention(mention) => Some(&mention.user),
        InlineNode::Tag(tag) => Some(&tag.name),
        InlineNode::Symbol(symbol) => Some(&symbol.name),
        _ => None,
    }
}
