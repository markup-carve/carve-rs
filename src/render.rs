//! HTML renderer — emits the canonical output the spec corpus expects.
//!
//! Output style matches `carve-js/render-html.ts`: block elements on
//! their own line; inline content flows within the block tag. Lists
//! indent their `<li>` children two spaces.

use crate::abbr_budget::AbbrBudgetGuard;
use crate::ast::*;
use crate::escape::{
    escape_attr, escape_text, is_dangerous_attr_name, is_valid_attr_name, sanitize_attr_value,
    sanitize_url, write_escaped_attr, write_escaped_text,
};
use crate::extension::{Options, RenderContext};
use crate::parse::unwrap_nested_anchors;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt::Write as _;

/// The recursion bound every renderer shares, and it MUST sit ABOVE the
/// parser's (§25).
///
/// The guard is for trees that did not come from the parser: `from_json`
/// accepts a good deal deeper than the parser produces, and a tree built
/// through the API has no limit at all. It is not a language rule, and
/// equality with `parse::MAX_NESTING_DEPTH` made it one - a renderer past its
/// bound emits nothing, where the parser degrades an over-cap opener to
/// literal text, so the render path deleted content the parse path kept
/// (issue 517).
///
/// ONE constant, not one per renderer. Five copies of the same number is how
/// the HTML renderer kept the old bound through the first sweep: its copy was
/// already spelled symbolically, so a search for the literal missed it.
///
/// THE UNIT IS NOT THE PARSER'S, which is why the margin is a FACTOR and not
/// `+ 32`. §25 states the bound as a property - a parsed tree must not be able
/// to reach it - and these renderers count AST LEVELS while the parse cap
/// counts SOURCE nesting levels. One source level of a list costs two AST
/// levels (`list`, then `list_item`) before its body, so `MAX_NESTING_DEPTH +
/// 32` was reachable at about 120 nested items: this engine truncated a
/// document its own parser had just accepted, where carve-js and carve-php -
/// which count CONTAINER depth, one per level - rendered it whole. Same shape
/// as the ingest bound's unit trap (PART 12 §9(b), `MAX_JSON_DEPTH`).
///
/// The factor covers the deepest AST-per-source ratio the parser can produce
/// (a list or definition list at two levels each, plus the leaf paragraph),
/// with the same absolute margin on top for the blocks a container subtree adds.
pub const MAX_RENDER_DEPTH: usize = crate::parse::MAX_NESTING_DEPTH * 2 + 32;

/// Render a tree that did NOT come from the parser, refusing at the ceiling.
///
/// `Err` when the tree nests deeper than [`MAX_RENDER_DEPTH`], naming the
/// renderer and the bound (PART 9 §25). A tree the parser produced cannot reach
/// it - the parser caps nesting lower - so this fails only for a tree built
/// through the API or read by `from_json`, where the caller is the one who can
/// act on it.
pub fn render_html(doc: &Document) -> Result<String, crate::RenderDepthError> {
    render_html_with_options(doc, &Options::default())
}

/// Render a tree that did NOT come from the parser, refusing at the ceiling.
///
/// `Err` when the tree nests deeper than [`MAX_RENDER_DEPTH`], naming the
/// renderer and the bound (PART 9 §25). A tree the parser produced cannot reach
/// it - the parser caps nesting lower - so this fails only for a tree built
/// through the API or read by `from_json`, where the caller is the one who can
/// act on it.
pub fn render_html_with_options(
    doc: &Document,
    options: &Options<'_>,
) -> Result<String, crate::RenderDepthError> {
    let watch = crate::render_depth::RenderDepthWatch::new();
    watch.into_result(render_html_inner(doc, options))
}

fn render_html_inner(doc: &Document, options: &Options<'_>) -> String {
    let mut doc = doc.clone();
    let _abbr_guard = AbbrBudgetGuard::for_document(&doc);
    let _index_guard = crate::index_budget::IndexBudgetGuard::new(doc.expansion_budget_len());
    // Document id namespace (extensions contract §2.6): seeded with every
    // explicit `{#id}` attribute and every heading id this render will assign,
    // so extension-generated ids (citation anchors / reference entries) take
    // the next free suffix instead of emitting a duplicate DOM id.
    let _document_ids_guard =
        crate::document_ids::DocumentIdsGuard::new(&doc, options.lowercase_heading_ids);
    let mut state = RenderState {
        lowercase_heading_ids: options.lowercase_heading_ids,
        crossref_index: crate::parse::crossref_index_for_document(
            &doc,
            options.lowercase_heading_ids,
        ),
        ..RenderState::default()
    };
    let footnotes = collect_footnotes(&mut doc, true);
    let mut html = render_document_blocks(doc.children.as_slice(), options, &mut state);
    if !footnotes.is_empty() {
        let section = render_footnotes_section(&doc, &footnotes, options, &mut state);
        // `::: footnotes` placement: every footnote is numbered by now, so flush
        // the endnotes at the sentinel instead of appending at the end. A
        // document without the marker is byte-identical to before.
        if html.contains(FOOTNOTES_PLACEMENT_SENTINEL) {
            html = place_footnotes_section(html, &section);
        } else {
            html.push('\n');
            html.push_str(&section);
        }
    }
    // Sweep any sentinel that still remains and degrade it to an empty
    // placeholder: a `::: footnotes` nested INSIDE a footnote definition emits a
    // sentinel while the endnotes section renders (after the body check above),
    // and a marker in a document with no footnotes never hit the branch above.
    // The raw sentinel must never leak into output.
    if html.contains(FOOTNOTES_PLACEMENT_SENTINEL) {
        html = html.replace(
            FOOTNOTES_PLACEMENT_SENTINEL,
            "<div class=\"footnotes\"></div>",
        );
    }
    html
}

/// Private sentinel emitted for a `::: footnotes` placement block; the top-level
/// render swaps it for the endnotes section (relocated from the document end).
/// Uses NUL bytes, which cannot appear in rendered HTML output.
const FOOTNOTES_PLACEMENT_SENTINEL: &str = "\u{0}carve:footnotes-placement\u{0}";

/// Relocate the endnotes section to the first `::: footnotes` sentinel; any
/// additional sentinels degrade to an empty placeholder so a second block never
/// duplicates the section.
fn place_footnotes_section(html: String, section: &str) -> String {
    let Some(pos) = html.find(FOOTNOTES_PLACEMENT_SENTINEL) else {
        return html;
    };
    let mut out = String::with_capacity(html.len() + section.len());
    out.push_str(&html[..pos]);
    out.push_str(section);
    out.push_str(&html[pos + FOOTNOTES_PLACEMENT_SENTINEL.len()..]);
    out.replace(
        FOOTNOTES_PLACEMENT_SENTINEL,
        "<div class=\"footnotes\"></div>",
    )
}

// Entry point for `RenderContext::render_blocks` (the extension render helper).
// This starts a FRESH heading-id counter, so headings rendered through it are
// numbered independently of the surrounding document. A block-extension
// renderer that needs document-consistent heading ids (a duplicate slug getting
// its `-N` suffix) should instead use `RenderContext::render_blocks_at`, which
// continues the live document counter when invoked from a block extension (the
// `Details` extension relies on this for carve-js parity).
pub(crate) fn render_blocks_with_options(nodes: &[BlockNode], options: &Options<'_>) -> String {
    render_blocks_at_with_options(nodes, options, 0)
}

// Like `render_blocks_with_options`, but indents every block to `level`. Used
// by `RenderContext::render_blocks_at` so a block-extension renderer can place
// its children at the correct nesting depth (see the same KNOWN LIMITATION on
// the fresh heading-id counter noted above).
pub(crate) fn render_blocks_at_with_options(
    nodes: &[BlockNode],
    options: &Options<'_>,
    level: usize,
) -> String {
    let mut state = RenderState {
        lowercase_heading_ids: options.lowercase_heading_ids,
        ..RenderState::default()
    };
    render_blocks(nodes, level, options, &mut state)
}

// Render block nodes at `level`, continuing an existing `RenderState` so the
// shared heading-id counter keeps numbering across an extension boundary.
pub(crate) fn render_blocks_at_with_state(
    nodes: &[BlockNode],
    options: &Options<'_>,
    level: usize,
    state: &mut RenderState,
) -> String {
    render_blocks(nodes, level, options, state)
}

pub(crate) fn render_blocks_with_state_from_depth(
    nodes: &[BlockNode],
    options: &Options<'_>,
    depth: usize,
    state: &mut RenderState,
) -> String {
    let previous = state.block_depth_bias;
    state.block_depth_bias = depth;
    let output = render_blocks(nodes, 0, options, state);
    state.block_depth_bias = previous;
    output
}

fn render_blocks(
    nodes: &[BlockNode],
    level: usize,
    options: &Options<'_>,
    state: &mut RenderState,
) -> String {
    let mut out = String::new();
    let mut first = true;
    let mut previous_owns_separator = false;
    for block in nodes {
        if matches!(
            block,
            BlockNode::Comment(_)
                | BlockNode::AbbreviationDef(_)
                | BlockNode::LinkReferenceDefinition(_)
                | BlockNode::CitationDefinition(_)
        ) {
            continue;
        }
        if !first && !previous_owns_separator {
            out.push('\n');
        }
        render_block(&mut out, block, level, options, state);
        previous_owns_separator = is_all_blank_html_raw(block);
        first = false;
    }
    out
}

fn is_all_blank_html_raw(block: &BlockNode) -> bool {
    matches!(block, BlockNode::RawBlock(raw)
        if raw.format == "html"
            && !raw.content.is_empty()
            && raw.content.chars().all(|c| c == '\n'))
}

/// Render a container's children, dropping the ones that render to nothing.
///
/// A comment, a comment block, an abbreviation definition and a non-HTML raw
/// block all render as the empty string. Pushing the separating newline before
/// knowing that leaves a blank line where the block stood - a divergence from
/// carve-php, which never wrote one. A container whose whole body renders to
/// nothing gets an empty Vec, which each caller hands to the same path it uses
/// for a childless container.
fn rendered_children(
    nodes: &[BlockNode],
    level: usize,
    options: &Options<'_>,
    state: &mut RenderState,
) -> Vec<String> {
    let mut out = Vec::new();
    for child in nodes {
        let mut buf = String::new();
        render_block(&mut buf, child, level, options, state);
        if !buf.is_empty() {
            out.push(buf);
        }
    }
    out
}

#[derive(Default)]
pub(crate) struct RenderState {
    heading_counts: BTreeMap<String, usize>,
    crossref_index: crate::parse::CrossrefIndex,
    link_depth: usize,
    inline_depth: usize,
    block_depth_bias: usize,
    /// Mirrors `Options::lowercase_heading_ids` so the `<section id>` derived
    /// here matches the parse-time id index (and the resolved cross-ref hrefs).
    lowercase_heading_ids: bool,
    /// True while rendering the endnotes section's footnote bodies. A
    /// `::: footnotes` nested inside a footnote definition must NOT emit a
    /// placement sentinel (it renders as an ordinary div, matching carve-js).
    rendering_footnotes: bool,
    suppress_automatic_abbreviation: bool,
}

fn render_document_blocks(
    nodes: &[BlockNode],
    options: &Options<'_>,
    state: &mut RenderState,
) -> String {
    let mut out = String::new();
    let mut i = 0;
    let mut first = true;
    let mut previous_owns_separator = false;
    while i < nodes.len() {
        // Skipped BEFORE the separating newline, or a block that renders to
        // nothing leaves a blank line where it stood. An abbreviation
        // definition joined this list when it started SURVIVING into the tree
        // (#513): it was previously deleted once its expansions were
        // harvested, so nothing here had ever seen one.
        if matches!(
            nodes[i],
            BlockNode::Comment(_)
                | BlockNode::AbbreviationDef(_)
                | BlockNode::LinkReferenceDefinition(_)
                | BlockNode::CitationDefinition(_)
        ) {
            i += 1;
            continue;
        }
        if !first && !previous_owns_separator {
            out.push('\n');
        }
        let owns_separator = is_all_blank_html_raw(&nodes[i]);
        if matches!(nodes[i], BlockNode::Heading(_)) && options.sections {
            i = render_section(&mut out, nodes, i, 0, options, state);
        } else {
            render_block(&mut out, &nodes[i], 0, options, state);
            i += 1;
        }
        previous_owns_separator = owns_separator;
        first = false;
    }
    out
}

#[derive(Clone)]
pub(crate) struct FootnoteEntry {
    label: Option<String>,
    inline: Option<Vec<InlineNode>>,
    backrefs: Vec<String>,
}

pub(crate) fn collect_footnotes(doc: &mut Document, assign_ref_ids: bool) -> Vec<FootnoteEntry> {
    let mut order = Vec::new();
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    let def_labels: HashSet<String> = doc.footnote_defs.keys().cloned().collect();
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

fn collect_footnotes_block(
    assign_ref_ids: bool,
    block: &mut BlockNode,
    def_labels: &HashSet<String>,
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
        BlockNode::Extension(e) => {
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
fn collect_footnotes_inline(
    assign_ref_ids: bool,
    nodes: &mut [InlineNode],
    def_labels: &HashSet<String>,
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
fn collect_footnotes_inline_scoped(
    assign_ref_ids: bool,
    nodes: &mut [InlineNode],
    def_labels: &HashSet<String>,
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
                    let ref_id = format!("fnref{number}");
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
                if !def_labels.contains(id) {
                    continue;
                }
                let idx = label_indices.get(id).copied().unwrap_or_else(|| {
                    order.push(FootnoteEntry {
                        label: Some(id.clone()),
                        inline: None,
                        backrefs: Vec::new(),
                    });
                    let idx = order.len() - 1;
                    label_indices.insert(id.clone(), idx);
                    idx
                });
                let number = idx + 1;
                let occurrence = seen.entry(id.clone()).or_insert(0);
                *occurrence += 1;
                let ref_id = if *occurrence == 1 {
                    format!("fnref{number}")
                } else {
                    format!("fnref{number}-{occurrence}")
                };
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
fn block_source_line(block: &BlockNode) -> Option<&str> {
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
        BlockNode::Div(n) => n.attrs.as_ref(),
        BlockNode::LineBlock(n) => n.attrs.as_ref(),
        BlockNode::DefinitionList(n) => n.attrs.as_ref(),
        BlockNode::Figure(n) => n.attrs.as_ref(),
        BlockNode::FigureGroup(n) => n.attrs.as_ref(),
        BlockNode::Extension(n) => n.attrs.as_ref(),
        BlockNode::BlockImage(n) => n.attrs.as_ref(),
        BlockNode::AbbreviationDef(_) | BlockNode::RawBlock(_) | BlockNode::Comment(_) => None,
    }?;
    attrs.key_values.get("data-source-line").map(String::as_str)
}

fn render_footnotes_section(
    doc: &Document,
    footnotes: &[FootnoteEntry],
    options: &Options<'_>,
    state: &mut RenderState,
) -> String {
    // Suppress `::: footnotes` placement while rendering footnote bodies, so a
    // nested marker renders as an ordinary div instead of emitting a sentinel.
    let was_rendering_footnotes = state.rendering_footnotes;
    state.rendering_footnotes = true;
    let mut out = String::new();
    out.push_str("<section role=\"doc-endnotes\">\n  <hr>\n  <ol>");
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
        match li_source_line {
            Some(line) => out.push_str(&format!(
                "    <li id=\"fn{}\" data-source-line=\"{}\">",
                num, line
            )),
            None => out.push_str(&format!("    <li id=\"fn{}\">", num)),
        }
        if let Some(inline) = &entry.inline {
            out.push('\n');
            out.push_str("      <p>");
            render_inlines(&mut out, inline, options, state);
            out.push_str(&render_backlinks(&entry.backrefs));
            out.push_str("</p>");
        } else if let Some(label) = &entry.label {
            let blocks: &[BlockNode] = doc
                .footnote_defs
                .get(label)
                .map(Vec::as_slice)
                .unwrap_or_default();
            // A body with NO blocks at all still owes the reader a way back.
            // PART 9 §16 hangs the backlink on the body's last block and
            // synthesizes a wrapping paragraph when that block is not one
            // (markup-carve/carve#688); a body with no last block has nothing
            // to hang it on, so the whole paragraph is synthesized. The loop
            // below runs zero times here, which is how the anchor went missing
            // while the reference kept pointing at the note (carve-rs#826).
            //
            // Zero blocks is reachable from source (a body that is only a
            // block-attribute line, `[^f]: {x}`, whose line is consumed as
            // attributes), from AST-JSON ingest (`"type":"footnote"` with an
            // empty `children`), and from a profile whose disallowed action is
            // Strip removing every block of the body. All three arrive here.
            if blocks.is_empty() {
                out.push('\n');
                indent(&mut out, 3);
                out.push_str("<p>");
                out.push_str(&render_backlinks(&entry.backrefs));
                out.push_str("</p>");
            }
            for (block_idx, block) in blocks.iter().enumerate() {
                out.push('\n');
                let mut rendered = String::new();
                render_block(&mut rendered, block, 3, options, state);
                if block_idx + 1 == blocks.len() {
                    let backlink = render_backlinks(&entry.backrefs);
                    // The backlink goes INSIDE the body's last paragraph -
                    // but only when that last block IS a paragraph. When it
                    // is anything else the body gets a synthesized paragraph
                    // to carry it (PART 9 §16, spec markup-carve/carve#799,
                    // corpus 225).
                    //
                    // Searching the rendered string for the last `</p>` was
                    // wrong twice over: it appended a bare anchor after
                    // `</pre>` for a body ending in a fence, leaving the
                    // endnote ending in something that is not a block; and
                    // for a body ending in a quote or a list it found the
                    // paragraph nested INSIDE that block and put the
                    // backlink there, which reads as part of the quotation.
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

fn render_backlinks(backrefs: &[String]) -> String {
    // A note referenced once gets a plain `↩`; a note referenced N>1 times gets
    // one numbered backlink per reference (`↩<sup>k</sup>`, space-separated) so
    // each return arrow is distinct (matches carve-php + pandoc).
    if backrefs.len() <= 1 {
        return backrefs
            .iter()
            .map(|ref_id| format!("<a href=\"#{ref_id}\" role=\"doc-backlink\">↩</a>"))
            .collect();
    }
    backrefs
        .iter()
        .enumerate()
        .map(|(k, ref_id)| {
            format!(
                "<a href=\"#{ref_id}\" role=\"doc-backlink\">↩<sup>{}</sup></a>",
                k + 1
            )
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn render_section(
    out: &mut String,
    nodes: &[BlockNode],
    start: usize,
    level: usize,
    options: &Options<'_>,
    state: &mut RenderState,
) -> usize {
    let BlockNode::Heading(heading) = &nodes[start] else {
        return start + 1;
    };
    let section_id = next_heading_id(heading, state);
    indent(out, level);
    out.push_str(&format!("<section id=\"{}\">\n", escape_attr(&section_id)));
    render_heading_without_section_id(out, heading, level + 1, options, state);
    let mut i = start + 1;
    while i < nodes.len() {
        if let BlockNode::Heading(next) = &nodes[i] {
            if next.level <= heading.level {
                break;
            }
            out.push('\n');
            i = render_section(out, nodes, i, level + 1, options, state);
            continue;
        }
        // A node that renders NOTHING contributes no separator either - otherwise a
        // hoisted definition inside a section left a blank line before
        // `</section>` (corpus 173, carve-rs#631). Same skip the two loops above
        // make.
        if matches!(
            nodes[i],
            BlockNode::Comment(_)
                | BlockNode::AbbreviationDef(_)
                | BlockNode::LinkReferenceDefinition(_)
                | BlockNode::CitationDefinition(_)
        ) {
            i += 1;
            continue;
        }
        out.push('\n');
        render_block(out, &nodes[i], level + 1, options, state);
        i += 1;
    }
    out.push('\n');
    indent(out, level);
    out.push_str("</section>");
    i
}

fn indent(out: &mut String, level: usize) {
    for _ in 0..level {
        out.push_str("  ");
    }
}

/// Charge a rendered cross-reference label against the per-render expansion
/// budget, degrading an over-budget label to the authored target.
///
/// A crossref republishes the target heading's whole display text while the
/// reference costs only the slug, so K references to one long heading amplify
/// output K x heading_len. That is the abbreviation expansion's shape, so it
/// takes the abbreviation expansion's budget rather than a second one, and it
/// degrades the way that one does: to the text the author actually typed
/// (`markup-carve/carve-rs#805`).
fn charge_crossref_label(label: String, target: &str) -> String {
    if crate::abbr_budget::try_spend(label.len()) {
        return label;
    }
    let mut degraded = String::new();
    write_escaped_text(&mut degraded, target);
    degraded
}

fn render_block(
    out: &mut String,
    node: &BlockNode,
    level: usize,
    options: &Options<'_>,
    state: &mut RenderState,
) {
    if level.saturating_add(state.block_depth_bias) > MAX_RENDER_DEPTH {
        crate::render_depth::record("html");
        return;
    }
    match node {
        // PART 12 §10: the definition line renders NOTHING itself - it feeds every
        // link or image that resolves the label (PART 9R R1). carve-js and
        // carve-php emit nothing for it here too.
        BlockNode::LinkReferenceDefinition(_) => {}
        // PART 12 section 18: the same, for the same reason. The entry's text
        // renders in the references list the Citations extension builds, not
        // where the line was written.
        BlockNode::CitationDefinition(_) => {}
        BlockNode::Heading(h) => render_heading(out, h, level, options, state),
        BlockNode::Paragraph(p) => render_paragraph(out, p, level, options, state),
        BlockNode::CodeBlock(c) => render_code_block(out, c, level),
        BlockNode::List(l) => render_list(out, l, level, options, state),
        BlockNode::BlockQuote(b) => render_blockquote(out, b, level, options, state),
        BlockNode::Table(t) => render_table(out, t, level, options, state),
        BlockNode::Admonition(a) => render_admonition(out, a, level, options, state),
        BlockNode::Div(d) => render_div(out, d, level, options, state),
        BlockNode::LineBlock(lb) => render_line_block(out, lb, level, options, state),
        BlockNode::DefinitionList(d) => render_definition_list(out, d, level, options, state),
        BlockNode::Figure(f) => render_figure(out, f, level, options, state),
        BlockNode::FigureGroup(g) => render_figure_group(out, g, level, options, state),
        BlockNode::AbbreviationDef(_) => {}
        BlockNode::RawBlock(r) => {
            if r.format == "html" {
                indent(out, level);
                // Escape instead of emitting when raw HTML is disabled.
                if options.allow_raw_html {
                    out.push_str(&r.content);
                } else {
                    out.push_str(&escape_text(&r.content));
                }
            }
        }
        BlockNode::Comment(_) => {}
        BlockNode::Extension(e) => render_block_extension(out, e, level, options, state),
        BlockNode::BlockImage(img) => {
            indent(out, level);
            render_image(out, img);
        }
        BlockNode::ThematicBreak(n) => {
            indent(out, level);
            out.push_str("<hr");
            write_attrs(out, &n.attrs);
            out.push('>');
        }
    }
}

/// Pull the `data-source-line` stamp out of an attribute set, returning its
/// value. It is added by the parser when `source_lines` is on, but it is a
/// render annotation rather than something the author wrote, so a caller that
/// appends a generated attribute has to emit it after the stamp is removed and
/// put the stamp back last.
/// Whether an attribute set holds anything the AUTHOR wrote. The
/// `data-source-line` stamp is a render annotation the parser injects for
/// editor scroll-sync, so a block carrying only that is, for authoring
/// purposes, unattributed -- otherwise turning `source_lines` on would change
/// the HTML structure of every tight list item.
fn has_authored_attrs(attrs: &Option<Attrs>) -> bool {
    attrs.as_ref().is_some_and(|a| {
        a.id.is_some()
            || !a.classes.is_empty()
            || a.key_values.keys().any(|k| k != "data-source-line")
    })
}

fn take_source_line_attr(attrs: &mut Attrs) -> Option<String> {
    let value = attrs.key_values.remove("data-source-line")?;
    attrs
        .order
        .retain(|slot| !matches!(slot, AttrSlot::Key(k) if k == "data-source-line"));
    Some(value)
}

fn render_heading(
    out: &mut String,
    h: &Heading,
    level: usize,
    options: &Options<'_>,
    state: &mut RenderState,
) {
    // A heading rendered here carries no `<section>` wrapper - either because
    // it is nested inside another block (list item, blockquote, div, ...) or
    // because the `sections` option is off - so it emits its id directly on the
    // tag. The id is allocated from the same document-order counter as
    // top-level headings, so duplicate slugs are numbered consistently across
    // nesting levels.
    //
    // PART 10 §1 decides where that id sits. The author's own attributes keep
    // their source order, and a GENERATED attribute joins at the end; an id the
    // author WROTE is not generated, so it stays in the slot they wrote it in.
    // carve-rs used to put the id first in both cases, which agreed with no
    // other engine.
    let id = next_heading_id(h, state);
    // AUTHORED means the id took a SLOT in an attribute block, not merely that
    // the node carries one. Since carve#750 the parse stamps a heading's
    // GENERATED id onto the node, so presence no longer distinguishes them - and
    // testing for it put the generated id in the authored run, ahead of
    // `data-source-line`, which section_wrapping.rs pins the other way round.
    let authored_id = h.attrs.as_ref().is_some_and(|attrs| {
        attrs.id.is_some() && attrs.order.iter().any(|slot| matches!(slot, AttrSlot::Id))
    });
    indent(out, level);
    write!(out, "<h{}", h.level).unwrap();
    if authored_id {
        // Render through the normal attribute walk so the id lands in its
        // authored slot. The resolved id is the authored one (an explicit
        // heading id wins verbatim), but write it back rather than assume so.
        let mut attrs = h.attrs.clone();
        if let Some(attrs) = &mut attrs {
            attrs.id = Some(id.clone());
        }
        write_attrs(out, &attrs);
    } else {
        // `data-source-line` is a RENDER annotation, not something the author
        // wrote, and it is emitted last - so the generated id goes before it,
        // not after. carve-rs stamps it as an ordinary key-value at parse time,
        // which would otherwise carry it along in the authored run and put the
        // id behind it (`tests/source_lines.rs` catches exactly that).
        let mut authored = h.attrs.clone();
        let stamp = authored.as_mut().and_then(take_source_line_attr);
        out.push_str(&render_attrs_without_id(&authored));
        out.push_str(" id=\"");
        write_escaped_attr(out, &id);
        out.push('"');
        if let Some(line) = stamp {
            out.push_str(" data-source-line=\"");
            write_escaped_attr(out, &line);
            out.push('"');
        }
    }
    out.push('>');
    render_inlines(out, &h.children, options, state);
    write!(out, "</h{}>", h.level).unwrap();
}

fn render_heading_without_section_id(
    out: &mut String,
    h: &Heading,
    level: usize,
    options: &Options<'_>,
    state: &mut RenderState,
) {
    let mut attrs = h.attrs.clone();
    if let Some(attrs) = &mut attrs {
        attrs.id = None;
    }
    indent(out, level);
    write!(out, "<h{}", h.level).unwrap();
    write_attrs(out, &attrs);
    out.push('>');
    render_inlines(out, &h.children, options, state);
    write!(out, "</h{}>", h.level).unwrap();
}

fn next_heading_id(h: &Heading, state: &mut RenderState) -> String {
    let explicit = h.attrs.as_ref().and_then(|attrs| attrs.id.clone());
    let has_explicit = explicit.is_some();
    let base = explicit
        .unwrap_or_else(|| slugify(&plain_inlines(&h.children), state.lowercase_heading_ids));
    let mut count = state.heading_counts.get(&base).copied().unwrap_or(0);
    let id = loop {
        count += 1;
        let id = if count == 1 {
            base.clone()
        } else {
            format!("{base}-{count}")
        };
        // An explicit heading id wins verbatim; an auto slug skips any id an
        // explicit `{#id}` elsewhere already claimed (avoids a duplicate DOM
        // id). Mirrors the seeder's reserve_heading_id so the two agree.
        if has_explicit || !crate::document_ids::is_explicit_id(&id) {
            break id;
        }
    };
    state.heading_counts.insert(base, count);
    id
}

/// Flatten inline nodes to the plain-text projection used for heading-id slug
/// generation. This is the single source of truth: the heading-permalinks
/// extension reuses it so its anchor `href` can never diverge from the id the
/// core emits for the same heading (see `heading_permalinks::next_id`).
pub(crate) fn plain_inlines(nodes: &[InlineNode]) -> String {
    plain_inlines_typography(nodes, crate::extension::SmartTypographyMode::Glyph)
}

/// [`plain_inlines`], but resolving `smart_punctuation` through the document's
/// smart-typography mode instead of hardcoding the glyph.
///
/// The mode is DOCUMENT-GLOBAL and applies to EVERY target (PART 9 §19, AST
/// REPRESENTATION): with it set to source, "every trigger character survives as
/// the ASCII the author typed". A pre-render pass that derives DISPLAY text and
/// resolves the glyph itself defeats that switch before any renderer can honor
/// it, which is why the derived-text callers (the TOC entry, the numbered
/// cross-reference label) go through here.
///
/// SLUG derivation must NOT: an id may not depend on presentational typography,
/// so `plain_inlines` above keeps the glyph and every id stays byte-identical in
/// both modes (PART 9 §19 says so in as many words: "heading ids are
/// BYTE-IDENTICAL either way").
pub(crate) fn plain_inlines_typography(
    nodes: &[InlineNode],
    smart: crate::extension::SmartTypographyMode,
) -> String {
    plain_inlines_typography_at(nodes, smart, 0)
}

fn plain_inlines_typography_at(
    nodes: &[InlineNode],
    smart: crate::extension::SmartTypographyMode,
    depth: usize,
) -> String {
    if depth > MAX_RENDER_DEPTH {
        crate::render_depth::record("html");
        return String::new();
    }
    let mut out = String::new();
    let source = smart == crate::extension::SmartTypographyMode::Source;
    for node in nodes {
        match node {
            InlineNode::Text(s) => out.push_str(&s.value),
            // Visible prose, so it feeds derived text (carve-rs#800). This has to
            // move together with `plain_inlines_parse`'s arm: the parse-time
            // index and the render-time id are two spellings of one derivation,
            // and a heading whose id disagreed between them would publish an
            // anchor no cross-reference could resolve.
            InlineNode::EscapedText(e) => out.push_str(&e.value),
            InlineNode::SmartPunctuation(s) => {
                if source {
                    out.push_str(&s.value);
                } else {
                    out.push_str(smart_punctuation_glyph(s));
                }
            }
            InlineNode::Emphasis(e) => {
                out.push_str(&plain_inlines_typography_at(&e.children, smart, depth + 1))
            }
            InlineNode::Code(s) => out.push_str(&s.value),
            // An inline literal renders as visible prose (§27), so it contributes
            // its content to a heading slug -- otherwise `` # !`Cat` `` would
            // slug to the empty fallback and `</#cat>` could never resolve.
            InlineNode::LiteralInline(lit) => out.push_str(&lit.content),
            // Math is verbatim text the reader sees (carve-js groups its arm
            // with the inline literal for exactly this reason), so it feeds a
            // heading id like a code span does. Mirrors `plain_inlines_parse`.
            InlineNode::Math(m) => out.push_str(&m.content),
            // A `</#id>` cross-reference contributes nothing to a heading id: the
            // id is derived from the heading text as authored, before cross-ref
            // resolution turns the reference into a Link. Skipping it here keeps
            // the render-time id byte-identical to the parse-time id used to
            // build the cross-reference index (so `# A </#a>` keeps id `A`, not
            // `A-A`). Mirrors `plain_inlines_parse`, which never saw the Link.
            InlineNode::Link(l) if l.from_crossref => {}
            InlineNode::Link(l) => {
                out.push_str(&plain_inlines_typography_at(&l.children, smart, depth + 1))
            }
            InlineNode::AutoLink(a) => out.push_str(&a.text),
            InlineNode::Image(i) => out.push_str(&i.alt),
            InlineNode::Extension(e) => {
                out.push_str(&plain_inlines_typography_at(&e.children, smart, depth + 1))
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
            // A soft/hard break is a word separator for slug/plain-text
            // purposes, not a join. (No parse puts one in a heading now;
            // an ingested AST still can, PART 12.)
            InlineNode::SoftBreak(_) | InlineNode::HardBreak(_) => out.push(' '),
            _ => {}
        }
    }
    out
}

fn slugify(text: &str, lowercase: bool) -> String {
    // Delegate to the single canonical implementation so HTML, Markdown, and
    // the parser's id index never drift apart (or from carve-js / carve-php).
    crate::parse::slugify_parse(text, lowercase)
}

fn render_paragraph(
    out: &mut String,
    p: &Paragraph,
    level: usize,
    options: &Options<'_>,
    state: &mut RenderState,
) {
    indent(out, level);
    out.push_str("<p");
    write_attrs(out, &p.attrs);
    out.push('>');
    render_inlines(out, &p.children, options, state);
    out.push_str("</p>");
}

fn render_code_block(out: &mut String, c: &CodeBlock, level: usize) {
    indent(out, level);
    out.push_str("<pre");
    if let Some(title) = &c.title {
        if !attrs_has_key(&c.attrs, "title") {
            write_attr_key_value(out, "title", title);
        }
    }
    write_attrs(out, &c.attrs);
    out.push_str("><code");
    if let Some(lang) = &c.lang {
        out.push_str(" class=\"language-");
        out.push_str(lang);
        out.push('"');
    }
    out.push('>');
    // A code block's content is VERBATIM but not RAW: PART 12 §3 puts the
    // no-break-space sentinel U+E000 on `code_block.content` alongside
    // `text.value`, `code.value` and `literal_inline.content`, and a consumer
    // MUST map it rather than emit it. Only `raw_block.content` is excluded,
    // because that one is byte-for-byte passthrough. Writing the private-use
    // character through is not merely untidy - a downstream typesetter draws
    // the font's `.notdef` box for it, silently.
    write_escaped_text_nbsp(out, &c.content);
    out.push_str("\n</code></pre>");
}

fn attrs_has_key(attrs: &Option<Attrs>, key: &str) -> bool {
    attrs
        .as_ref()
        .is_some_and(|attrs| attrs.key_values.keys().any(|k| k.eq_ignore_ascii_case(key)))
}

fn render_list(
    out: &mut String,
    l: &List,
    level: usize,
    options: &Options<'_>,
    state: &mut RenderState,
) {
    indent(out, level);
    let tag = if l.ordered { "ol" } else { "ul" };
    out.push('<');
    out.push_str(tag);
    // A STRUCTURAL ATTRIBUTE LEADS (PART 11 section 5.1). `type` and `start` are
    // fixed by the first item's marker, so they are the element's own shape
    // rather than something added on top of what the author wrote, and they
    // precede the authored attributes. This wrote them after, reading the
    // "generated attribute joins at the end" rule as covering them -- carve-js,
    // carve-php and reference djot all lead with them (carve#1090).
    if l.ordered {
        if let Some(ol_type) = l.ol_type {
            let value = match ol_type {
                OrderedListType::LowerAlpha => "a",
                OrderedListType::UpperAlpha => "A",
                OrderedListType::LowerRoman => "i",
                OrderedListType::UpperRoman => "I",
            };
            write!(out, " type=\"{value}\"").unwrap();
        }
        if let Some(start) = l.start {
            write!(out, " start=\"{start}\"").unwrap();
        }
    }
    write_attrs(out, &l.attrs);
    out.push_str(">\n");
    for (i, item) in l.items.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        render_list_item(out, item, level + 1, l.tight, options, state);
    }
    out.push('\n');
    indent(out, level);
    out.push_str("</");
    out.push_str(tag);
    out.push('>');
}

fn render_list_item(
    out: &mut String,
    item: &ListItem,
    level: usize,
    tight: bool,
    options: &Options<'_>,
    state: &mut RenderState,
) {
    indent(out, level);
    out.push_str("<li");
    write_attrs(out, &item.attrs);
    out.push('>');
    let checkbox = match item.checked {
        None => "",
        Some(false) => "<input type=\"checkbox\" disabled> ",
        Some(true) => "<input type=\"checkbox\" checked disabled> ",
    };
    if item.children.is_empty() {
        out.push_str("</li>");
        return;
    }
    // Each child renders either as inlineable content (a paragraph: bare
    // inlines when the list is TIGHT, wrapped in <p> when LOOSE) or as a
    // block. A tight item never wraps ANY of its paragraphs, so trailing text
    // that follows a closed block (fenced code, `:::` div, admonition, table)
    // stays bare rather than becoming a fresh <p> -- matching carve-js and the
    // executable-spec oracle. The first inlineable part shares the <li> line
    // (after any checkbox); later inlineable parts sit at the child column.
    enum Part {
        Inline(String),
        Block(String),
    }
    let parts: Vec<Part> = item
        .children
        .iter()
        .map(|child| {
            if let BlockNode::Paragraph(p) = child {
                let mut html = String::new();
                render_inlines(&mut html, &p.children, options, state);
                // A tight item's paragraph renders bare -- unless it carries
                // AUTHORED attributes, which have nowhere to go without the
                // `<p>`. Reachable since a brace-only list-marker line became a
                // block attribute line (§15 A8): `- {.c}` / `  text` must not
                // silently drop the class. Matches carve-js.
                if tight && !has_authored_attrs(&p.attrs) {
                    Part::Inline(html)
                } else {
                    Part::Inline(format!("<p{}>{html}</p>", render_attrs(&p.attrs)))
                }
            } else {
                let mut block = String::new();
                render_block(&mut block, child, level + 1, options, state);
                Part::Block(block)
            }
        })
        .filter(|part| match part {
            // A block that renders to NOTHING contributes no line (#429), and
            // an item is not the exception: a comment or an abbreviation
            // definition inside one used to leave the `\n` and the child
            // indentation behind, so `- a` / `  %% c` published
            // `<li>a    </li>` where carve-js, carve-php and the spec publish
            // `<li>a</li>` (carve-rs#532).
            Part::Block(html) => !html.trim().is_empty(),
            // A PARAGRAPH that renders to nothing is the same case (#429), and
            // the exemption here was the reason it still showed: a `+`-attached
            // block whose whole content was a collected definition or a comment
            // parses to an empty paragraph, which survived as an empty Inline
            // and published a stray blank line inside the `<li>` - the "trace" a
            // collected definition must not leave (carve-rs#670, corpus 226).
            //
            // `is_empty`, NOT `trim().is_empty()`: Rust's `trim` takes Unicode
            // whitespace, so a no-break space would be dropped as blank, and an
            // item holding one is an item with content.
            Part::Inline(html) => !html.is_empty(),
        })
        .collect();
    if parts.is_empty() {
        out.push_str(checkbox);
        out.push_str("</li>");
        return;
    }
    match &parts[0] {
        Part::Inline(html) => {
            out.push_str(checkbox);
            out.push_str(html);
        }
        Part::Block(html) => {
            // THE CHECKBOX IS A PROPERTY OF THE ITEM, NOT OF ITS FIRST BLOCK.
            // It is written directly after the `<li>` opener whatever the
            // marker line goes on to open, and nothing about that block
            // reaches it. Only the CONTENT moves: it sits beside the checkbox
            // when the first block renders inline, and on its own indented
            // line below it when it does not. Deciding the checkbox's
            // placement from the block that follows it wrote it at column 0,
            // outside the indentation every other child of an `<li>` gets, for
            // every non-paragraph lead -- a quote, a heading, a thematic
            // break, a fence, a `:::` div, a table row (carve#1381,
            // corpus 363).
            out.push_str(checkbox);
            out.push('\n');
            out.push_str(html);
        }
    }
    if parts.len() == 1 {
        if let Part::Inline(_) = &parts[0] {
            out.push_str("</li>");
            return;
        }
    }
    for part in parts.iter().skip(1) {
        out.push('\n');
        match part {
            Part::Inline(html) => {
                indent(out, level + 1);
                out.push_str(html);
            }
            Part::Block(html) => out.push_str(html),
        }
    }
    out.push('\n');
    indent(out, level);
    out.push_str("</li>");
}

fn render_blockquote(
    out: &mut String,
    b: &BlockQuote,
    level: usize,
    options: &Options<'_>,
    state: &mut RenderState,
) {
    indent(out, level);
    if b.children.len() == 1 {
        if let BlockNode::Paragraph(p) = &b.children[0] {
            out.push_str("<blockquote");
            write_attrs(out, &b.attrs);
            out.push_str("><p");
            write_attrs(out, &p.attrs);
            out.push('>');
            render_inlines(out, &p.children, options, state);
            out.push_str("</p></blockquote>");
            return;
        }
    }
    out.push_str("<blockquote");
    write_attrs(out, &b.attrs);
    out.push_str(">\n");
    let mut first = true;
    for child in rendered_children(&b.children, level + 1, options, state) {
        if !first {
            out.push('\n');
        }
        out.push_str(&child);
        first = false;
    }
    out.push('\n');
    indent(out, level);
    out.push_str("</blockquote>");
}

fn render_table(
    out: &mut String,
    t: &Table,
    level: usize,
    options: &Options<'_>,
    state: &mut RenderState,
) {
    let mut resolved = t.clone();
    if resolved.columns.is_empty() {
        resolved.columns = columns_from_attrs(resolved.attrs.as_ref());
    }
    let t = &resolved;
    indent(out, level);
    out.push_str("<table");
    let mut table_attrs = t.attrs.clone();
    if let Some(attrs) = &mut table_attrs {
        attrs.key_values.retain(|k, _| {
            !matches!(
                k.as_str(),
                "aligns" | "valigns" | "widths" | "header-rows" | "footer-rows"
            )
        });
        attrs.order.retain(|s| !matches!(s, AttrSlot::Key(k) if matches!(k.as_str(), "aligns" | "valigns" | "widths" | "header-rows" | "footer-rows")));
    }
    write_attrs(out, &table_attrs);
    out.push('>');
    if let Some(caption) = &t.caption {
        out.push('\n');
        indent(out, level + 1);
        out.push_str("<caption>");
        render_inlines(out, caption, options, state);
        out.push_str("</caption>");
    }
    if t.columns.iter().any(|c| c.width.is_some()) {
        out.push('\n');
        indent(out, level + 1);
        out.push_str("<colgroup>");
        for column in &t.columns {
            out.push('\n');
            indent(out, level + 2);
            out.push_str("<col");
            if let Some(width) = column.width {
                write!(out, " style=\"width: {}%;\"", width * 100.0).unwrap();
            }
            out.push('>');
        }
        out.push('\n');
        indent(out, level + 1);
        out.push_str("</colgroup>");
    }
    // Computed once over ALL rows: a `^` in a body row extends the cell above
    // it even when that cell is in a header row, so a header cell can carry a
    // rowspan that crosses the thead/tbody boundary (matches carve-js).
    let (rowspan_cols, orphan_carets) = compute_rowspans(t);
    // The leading run of rows whose cells are ALL header cells forms <thead>.
    // A row that merely contains a header cell (a row header) stays in the body.
    //
    // A continuation that RESOLVES is transparent here, because it renders
    // nothing: the cell it continues is what occupies the column, and asking
    // whether the marker itself is a header asks about a cell that is not
    // there. Counting it dropped the row under a header rowspan out of the
    // head, so `|=H|=A|` over `| ^ |=B|` moved B into the body (carve-js skips
    // it the same way).
    let derived_header_count = t
        .rows
        .iter()
        .enumerate()
        .take_while(|(r, row)| {
            let consumed = consumed_rowspan_cols(*r, &rowspan_cols);
            let resolved = |i: usize, cell: &TableCell| match cell.span {
                Some(TableCellSpan::Rowspan) => !orphan_carets.contains(&(*r, i)),
                Some(TableCellSpan::Colspan) => colspan_target(row, i, &consumed).is_some(),
                None => false,
            };
            row.cells
                .iter()
                .enumerate()
                .any(|(i, cell)| !resolved(i, cell))
                && row
                    .cells
                    .iter()
                    .enumerate()
                    .all(|(i, cell)| cell.header || resolved(i, cell))
        })
        .count();
    let source_partition = t.attrs.as_ref().is_some_and(|attrs| {
        attrs.key_values.contains_key("header-rows") || attrs.key_values.contains_key("footer-rows")
    });
    let header_count = if source_partition {
        t.row_groups
            .as_ref()
            .map_or(derived_header_count, |groups| groups.head_rows)
    } else {
        derived_header_count
    };
    let footer_count = if source_partition {
        t.row_groups.as_ref().map_or(0, |groups| groups.foot_rows)
    } else {
        0
    };
    let has_header = header_count > 0;
    let body_start = header_count;
    if has_header {
        out.push('\n');
        indent(out, level + 1);
        out.push_str("<thead>");
        for (row_idx, header) in t.rows[..header_count].iter().enumerate() {
            render_table_row(
                out,
                header,
                true,
                options,
                row_idx,
                &rowspan_cols,
                &orphan_carets,
                state,
                t,
            );
        }
        out.push_str("</thead>");
    }
    // A header-only table (e.g. a GFM `| x |` + `|---|` with no body rows) emits
    // no <tbody>, matching carve-php.
    let footer_start = t.rows.len() - footer_count;
    if body_start < footer_start {
        out.push('\n');
        indent(out, level + 1);
        out.push_str("<tbody>");
        let mut body_ctx = TableBodyRenderContext {
            rowspan_cols: &rowspan_cols,
            orphan_carets: &orphan_carets,
            table: t,
            options,
            state,
        };
        for (row_idx, row) in t
            .rows
            .iter()
            .enumerate()
            .take(footer_start)
            .skip(body_start)
        {
            out.push('\n');
            indent(out, level + 2);
            render_table_body_row(out, row, row_idx, &mut body_ctx);
        }
        out.push('\n');
        indent(out, level + 1);
        out.push_str("</tbody>");
    }
    if footer_start < t.rows.len() {
        out.push('\n');
        indent(out, level + 1);
        out.push_str("<tfoot>");
        let mut foot_ctx = TableBodyRenderContext {
            rowspan_cols: &rowspan_cols,
            orphan_carets: &orphan_carets,
            table: t,
            options,
            state,
        };
        for (row_idx, row) in t.rows.iter().enumerate().skip(footer_start) {
            render_table_body_row(out, row, row_idx, &mut foot_ctx);
        }
        out.push_str("</tfoot>");
    }
    out.push('\n');
    indent(out, level);
    out.push_str("</table>");
}

fn columns_from_attrs(attrs: Option<&Attrs>) -> Vec<TableColumn> {
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
            width: widths
                .get(i)
                .and_then(|v| v.parse::<f64>().ok())
                .filter(|v| *v > 0.0 && *v <= 100.0)
                .map(|v| v / 100.0),
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn render_table_row(
    out: &mut String,
    row: &TableRow,
    in_head: bool,
    options: &Options<'_>,
    row_idx: usize,
    rowspan_cols: &BTreeMap<(usize, usize), usize>,
    orphan_carets: &BTreeSet<(usize, usize)>,
    state: &mut RenderState,
    table: &Table,
) {
    out.push_str("<tr");
    write_attrs(out, &row.attrs);
    out.push('>');
    // A head row resolves its continuations the same way a body row does. It
    // used to resolve neither: a `^` rendered an empty `<th>` BESIDE the
    // `rowspan` its origin already carried, and a `<` rendered an empty `<th>`
    // instead of widening the cell to its left - so a header cell spanning
    // columns lost the span and gained a column the table does not have.
    let consumed_cols = consumed_rowspan_cols(row_idx, rowspan_cols);
    let colspan_counts = compute_colspans(row, &consumed_cols);
    for (col, cell) in row.cells.iter().enumerate() {
        if cell.span == Some(TableCellSpan::Rowspan) {
            if orphan_carets.contains(&(row_idx, col)) {
                let scope = cell_scope_attr(cell, true, in_head);
                write!(out, "<th{scope}></th>").unwrap();
            }
            continue;
        }
        let tag = if in_head || cell.header { "th" } else { "td" };
        let mut extra = String::new();
        let mut emitted: Vec<&str> = Vec::new();
        // A header cell can carry a rowspan that extends down into the body
        // (a `^` below it), so the header row emits it too -- not just bodies.
        if let Some(span) = rowspan_cols.get(&(row_idx, col)) {
            extra.push_str(&format!(" rowspan=\"{}\"", span));
            emitted.push("rowspan");
        }
        if cell.span == Some(TableCellSpan::Colspan) {
            if colspan_target(row, col, &consumed_cols).is_none() {
                let scope = cell_scope_attr(cell, true, in_head);
                write!(out, "<{tag}{scope}></{tag}>").unwrap();
            }
            continue;
        }
        let colspan = colspan_counts.get(&col).copied().unwrap_or(1);
        if colspan > 1 {
            extra.push_str(&format!(" colspan=\"{}\"", colspan));
            emitted.push("colspan");
        }
        let style = table_cell_style(cell, table, col, row_align(row, col));
        if !style.is_empty() {
            emitted.push("style");
        }
        out.push('<');
        out.push_str(tag);
        out.push_str(&cell_scope_attr(cell, tag == "th", in_head));
        out.push_str(&render_cell_author_attrs(&cell.attrs, &emitted));
        out.push_str(&extra);
        out.push_str(&style);
        out.push('>');
        render_inlines(out, &cell.children, options, state);
        out.push_str("</");
        out.push_str(tag);
        out.push('>');
    }
    out.push_str("</tr>");
}

struct TableBodyRenderContext<'a, 'b> {
    rowspan_cols: &'a BTreeMap<(usize, usize), usize>,
    orphan_carets: &'a BTreeSet<(usize, usize)>,
    table: &'a Table,
    options: &'a Options<'a>,
    state: &'b mut RenderState,
}

fn render_table_body_row(
    out: &mut String,
    row: &TableRow,
    source_row_idx: usize,
    ctx: &mut TableBodyRenderContext<'_, '_>,
) {
    out.push_str("<tr");
    write_attrs(out, &row.attrs);
    out.push('>');
    let consumed_cols = consumed_rowspan_cols(source_row_idx, ctx.rowspan_cols);
    let colspan_counts = compute_colspans(row, &consumed_cols);
    for (cell_index, cell) in row.cells.iter().enumerate() {
        if cell.span == Some(TableCellSpan::Rowspan) {
            // A `^` that merged into a cell above renders nothing; one with
            // nothing to extend (no cell above) renders an EMPTY cell (§5).
            if ctx.orphan_carets.contains(&(source_row_idx, cell_index)) {
                let tag = if cell.header { "th" } else { "td" };
                let scope = cell_scope_attr(cell, cell.header, false);
                write!(out, "<{tag}{scope}></{tag}>").unwrap();
            }
            continue;
        }
        let mut attrs = String::new();
        let mut emitted: Vec<&str> = Vec::new();
        if let Some(span) = ctx.rowspan_cols.get(&(source_row_idx, cell_index)) {
            attrs.push_str(&format!(" rowspan=\"{}\"", span));
            emitted.push("rowspan");
        }
        if cell.span == Some(TableCellSpan::Colspan) {
            // A `<` that merged into a cell to its left renders nothing; one
            // with nothing to merge (first column / no real left cell) renders
            // an EMPTY cell (§5).
            if colspan_target(row, cell_index, &consumed_cols).is_none() {
                let tag = if cell.header { "th" } else { "td" };
                let scope = cell_scope_attr(cell, cell.header, false);
                write!(out, "<{tag}{scope}></{tag}>").unwrap();
            }
            continue;
        }
        let colspan = colspan_counts.get(&cell_index).copied().unwrap_or(1);
        if colspan > 1 {
            attrs.push_str(&format!(" colspan=\"{}\"", colspan));
            emitted.push("colspan");
        }
        let style = table_cell_style(
            cell,
            ctx.table,
            cell_index,
            table_column_align(ctx.table, cell_index),
        );
        if !style.is_empty() {
            attrs.push_str(&style);
            emitted.push("style");
        }
        // A `|=` cell in a body row is a row header: <th> inside <tbody>.
        let tag = if cell.header { "th" } else { "td" };
        out.push('<');
        out.push_str(tag);
        // Below the header run, so a header cell here heads its ROW.
        out.push_str(&cell_scope_attr(cell, cell.header, false));
        out.push_str(&render_cell_author_attrs(&cell.attrs, &emitted));
        out.push_str(&attrs);
        out.push('>');
        render_inlines(out, &cell.children, ctx.options, ctx.state);
        out.push_str("</");
        out.push_str(tag);
        out.push('>');
    }
    out.push_str("</tr>");
}

fn row_align(row: &TableRow, col: usize) -> Option<TableAlign> {
    row.cells.get(col).and_then(|c| c.align)
}

fn table_column_align(table: &Table, col: usize) -> Option<TableAlign> {
    table
        .rows
        .first()
        .and_then(|r| r.cells.get(col))
        .and_then(|c| c.align)
        .or_else(|| table.columns.get(col).and_then(|c| c.align))
}

fn table_cell_style(
    cell: &TableCell,
    table: &Table,
    col: usize,
    inherited_align: Option<TableAlign>,
) -> String {
    let align = cell
        .align
        .or(inherited_align)
        .or_else(|| table.columns.get(col).and_then(|c| c.align));
    let valign = cell
        .valign
        .or_else(|| {
            table
                .rows
                .first()
                .and_then(|r| r.cells.get(col))
                .and_then(|c| c.valign)
        })
        .or_else(|| table.columns.get(col).and_then(|c| c.valign));
    if align.is_none() && valign.is_none() {
        return String::new();
    }
    let mut declarations = String::new();
    if let Some(a) = align {
        declarations.push_str(match a {
            TableAlign::Left => "text-align: left;",
            TableAlign::Right => "text-align: right;",
            TableAlign::Center => "text-align: center;",
        });
    }
    if align.is_some() && valign.is_some() {
        declarations.push(' ');
    }
    if let Some(v) = valign {
        declarations.push_str(match v {
            TableVerticalAlign::Top => "vertical-align: top;",
            TableVerticalAlign::Middle => "vertical-align: middle;",
            TableVerticalAlign::Bottom => "vertical-align: bottom;",
        });
    }
    format!(" style=\"{declarations}\"")
}

/// Render a cell's author attributes, dropping any key that collides (case
/// -insensitively) with a structural attribute this renderer actually emits
/// for the cell (`rowspan` / `colspan` / `style`) -- the computed value is
/// authoritative. When no such structural attribute is emitted, the author's
/// value (e.g. a custom `style`) is preserved.
/// PART 10 SST9: a header cell states what it heads - `col` in the leading
/// header-row run, `row` below it. The language already distinguishes the two
/// positions, so this states an association the table has rather than adding a
/// concept; without it a screen reader guesses from position and guesses wrong
/// on any table carrying both kinds.
///
/// Empty when the author named a `scope` themselves. An authored value REPLACES
/// the default rather than joining it: emitting both gives
/// `<th scope="col" scope="colgroup">`, two attributes of one name and invalid
/// HTML. Suppressing it is also what keeps `colgroup` and `rowgroup` reachable,
/// since neither has a marker spelling here.
///
/// The test is case-INSENSITIVE, the one place this departs from Carve's
/// case-sensitive attribute names: `{Scope=...}` stays a different Carve
/// attribute and still reaches the output as `Scope`, but HTML attribute names
/// are not case-sensitive, so emitting the default beside it is the same
/// collision by another spelling.
fn cell_scope_attr(cell: &TableCell, is_header_cell: bool, in_header_run: bool) -> String {
    if !is_header_cell {
        return String::new();
    }
    if let Some(attrs) = &cell.attrs {
        if attrs
            .key_values
            .keys()
            .any(|key| key.eq_ignore_ascii_case("scope"))
        {
            return String::new();
        }
    }

    format!(" scope=\"{}\"", if in_header_run { "col" } else { "row" })
}

fn render_cell_author_attrs(attrs: &Option<Attrs>, emitted: &[&str]) -> String {
    let Some(a) = attrs else {
        return String::new();
    };
    let collides = |k: &str| emitted.contains(&k.to_ascii_lowercase().as_str());
    if emitted.is_empty() || !a.key_values.keys().any(|k| collides(k)) {
        return render_attrs(attrs);
    }
    let mut filtered = a.clone();
    filtered.key_values.retain(|k, _| !collides(k));
    filtered.order.retain(|slot| match slot {
        AttrSlot::Key(k) => !collides(k),
        _ => true,
    });
    render_attrs(&Some(filtered))
}

fn consumed_rowspan_cols(row_idx: usize, rowspan_cols: &RowspanCols) -> BTreeSet<usize> {
    rowspan_cols
        .iter()
        .filter_map(|(&(origin_row, col), &span)| {
            (row_idx > origin_row && row_idx < origin_row + span).then_some(col)
        })
        .collect()
}

/// Per-cell colspan counts for a row, keyed by the origin cell index. Computed in
/// a single left-to-right pass (mirroring `compute_rowspans`) so a row is
/// O(cells) rather than O(cells^2): each `<` extends the current chain origin
/// instead of every cell re-scanning the rest of the row.
type ColspanCounts = BTreeMap<usize, usize>;

/// Resolve every colspan origin in `row` to its total colspan count in one pass.
/// A real cell (`None` span, not consumed by a rowspan from above) starts a new
/// chain; each following `<` (Colspan) extends it; a rowspan cell or an orphan
/// `<` (no preceding real cell) breaks the chain so the next `<` resolves to
/// nothing. Consumed columns are transparent, matching `colspan_target`.
fn compute_colspans(row: &TableRow, consumed_cols: &BTreeSet<usize>) -> ColspanCounts {
    let mut counts: ColspanCounts = BTreeMap::new();
    let mut current_target: Option<usize> = None;
    for (i, cell) in row.cells.iter().enumerate() {
        if consumed_cols.contains(&i) {
            continue;
        }
        match cell.span {
            Some(TableCellSpan::Colspan) => {
                if let Some(target) = current_target {
                    *counts.entry(target).or_insert(1) += 1;
                }
            }
            Some(TableCellSpan::Rowspan) => {
                current_target = None;
            }
            None => {
                current_target = Some(i);
            }
        }
    }
    counts
}

/// Maps the origin cell `(row, col)` of each rowspan to its span count. Resolved
/// by carrying the current chain origin down per column, so an all-`^` table is
/// O(cells) rather than O(rows^2) (each `^` previously walked up every prior row
/// and the result list was scanned linearly per marker).
/// Rowspan counts keyed by origin cell `(row, col)`.
type RowspanCols = BTreeMap<(usize, usize), usize>;
/// Positions `(row, cell-index)` of orphan `^` markers (nothing above to extend).
type OrphanCarets = BTreeSet<(usize, usize)>;

/// Returns (rowspan counts keyed by origin (row, col), positions of orphan `^`
/// markers). An orphan `^` has no cell above it to extend, so it renders as an
/// EMPTY cell rather than being dropped (spec PART 9 §5). Positions are keyed
/// by (row, cell-index), matching the render loop's cell enumeration.
fn compute_rowspans(t: &Table) -> (RowspanCols, OrphanCarets) {
    let mut spans: BTreeMap<(usize, usize), usize> = BTreeMap::new();
    let mut orphan_carets: BTreeSet<(usize, usize)> = BTreeSet::new();
    // Per column: the origin row of the current rowspan chain (the most recent
    // non-`^` cell above). A `^` extends that origin; a real cell starts a new
    // chain.
    let mut base_for_col: BTreeMap<usize, usize> = BTreeMap::new();
    for (row_idx, row) in t.rows.iter().enumerate() {
        for (col, cell) in row.cells.iter().enumerate() {
            if cell.span == Some(TableCellSpan::Rowspan) {
                if let Some(&base) = base_for_col.get(&col) {
                    *spans.entry((base, col)).or_insert(1) += 1;
                } else {
                    orphan_carets.insert((row_idx, col));
                }
            } else {
                base_for_col.insert(col, row_idx);
            }
        }
    }
    (spans, orphan_carets)
}

/// Resolve a `<` colspan marker by walking left to the nearest real cell that is
/// not already occupied by a rowspan from above. Contiguous `<` markers are
/// transparent, as are columns consumed by rowspans. If the scan reaches the
/// table edge, the marker is orphaned and renders as an empty cell (spec §5).
fn colspan_target(row: &TableRow, i: usize, consumed_cols: &BTreeSet<usize>) -> Option<usize> {
    let mut j = i;
    while j > 0 {
        j -= 1;
        if consumed_cols.contains(&j) {
            continue;
        }
        match row.cells[j].span {
            Some(TableCellSpan::Colspan) => continue,
            Some(TableCellSpan::Rowspan) => return None,
            None => return Some(j),
        }
    }
    None
}

fn render_admonition(
    out: &mut String,
    a: &Admonition,
    level: usize,
    options: &Options<'_>,
    state: &mut RenderState,
) {
    // `::: footnotes` placement directive: emit a sentinel that the top-level
    // render replaces with the endnotes section, relocating it from the
    // document end. A document without this block is byte-identical to before.
    if a.kind == "footnotes" && !state.rendering_footnotes {
        // Preserve any blocks authored inside the placeholder before the
        // relocated endnotes (matching carve-js), then the sentinel.
        let body = render_blocks(&a.children, level, options, state);
        let body = body.trim_end_matches('\n');
        if !body.is_empty() {
            out.push_str(body);
            out.push('\n');
        }
        out.push_str(FOOTNOTES_PLACEMENT_SENTINEL);
        return;
    }
    let canonical = crate::profile::ADMONITION_TIER1_KINDS.contains(&a.kind.as_str());
    indent(out, level);
    // The type class is structural (`admonition {kind}` for Tier 1, the bare
    // `{kind}` for a Tier-2 div) and emitted first; the opener's own
    // attribute block merges its classes into it and contributes id /
    // key-values after (never a second class).
    let base = if canonical {
        format!("admonition {}", a.kind)
    } else {
        a.kind.clone()
    };
    let (class, rest) = match &a.attrs {
        Some(at) if !at.classes.is_empty() => (
            dedup_class_str(&format!("{} {}", base, at.classes.join(" "))),
            render_attrs_after_class(at),
        ),
        Some(at) => (base, render_attrs_after_class(at)),
        None => (base, String::new()),
    };
    let tag = if canonical { "aside" } else { "div" };
    out.push_str(&format!(
        "<{} class=\"{}\"{}>",
        tag,
        escape_attr(&class),
        rest
    ));
    if let Some(title) = &a.title {
        out.push('\n');
        indent(out, level + 1);
        out.push_str("<p class=\"admonition-title\">");
        render_inlines(out, title, options, state);
        out.push_str("</p>");
    }
    // Graceful degradation: when no extension consumed the grouping `[label]`,
    // surface it as a visible caption so the authored label is never silently
    // dropped in static output. Title (when present) renders first.
    if let Some(label) = &a.label {
        out.push('\n');
        indent(out, level + 1);
        out.push_str("<p class=\"div-label\">");
        out.push_str(&escape_text(label));
        out.push_str("</p>");
    }
    // carve-js renders the body as `>\n${title}${label}${body}\n${pad}</tag>`,
    // so an admonition with NO title, label, or children still emits one blank
    // line between the open and close tags (corpus 114-7). Mirror that: when the
    // body is otherwise empty, the missing content is a single empty line.
    let children = rendered_children(&a.children, level + 1, options, state);
    if a.title.is_none() && a.label.is_none() && children.is_empty() {
        out.push('\n');
    }
    for child in &children {
        out.push('\n');
        out.push_str(child);
    }
    out.push('\n');
    indent(out, level);
    out.push_str(if canonical { "</aside>" } else { "</div>" });
}

/// A line block renders as a div carrying the `line-block` class. The class is
/// part of the OUTPUT contract, not of the AST: the node type is what records
/// that every newline inside is a hard break, so a plain div an author gave
/// that class stays an ordinary div.
///
/// The structural class TRAILS the author's own attributes (`{.foo #v}` renders
/// `class="foo line-block" id="v"`), matching carve-php and carve-js.
fn render_line_block(
    out: &mut String,
    lb: &LineBlock,
    level: usize,
    options: &Options<'_>,
    state: &mut RenderState,
) {
    let mut attrs = lb.attrs.clone().unwrap_or_default();
    attrs.classes.push("line-block".to_string());
    if !attrs.order.contains(&AttrSlot::Class) {
        attrs.order.push(AttrSlot::Class);
    }

    indent(out, level);
    out.push_str(&format!("<div{}>", render_attrs(&Some(attrs))));
    for child in rendered_children(&lb.children, level + 1, options, state) {
        out.push('\n');
        out.push_str(&child);
    }
    out.push('\n');
    indent(out, level);
    out.push_str("</div>");
}

fn render_div(
    out: &mut String,
    d: &Div,
    level: usize,
    options: &Options<'_>,
    state: &mut RenderState,
) {
    indent(out, level);
    out.push_str(&format!("<div{}>", render_attrs(&d.attrs)));
    // Graceful degradation: surface an unconsumed grouping `[label]` as a
    // visible caption (see render_admonition for rationale).
    if let Some(label) = &d.label {
        out.push('\n');
        indent(out, level + 1);
        out.push_str("<p class=\"div-label\">");
        out.push_str(&escape_text(label));
        out.push_str("</p>");
    }
    for child in rendered_children(&d.children, level + 1, options, state) {
        out.push('\n');
        out.push_str(&child);
    }
    out.push('\n');
    indent(out, level);
    out.push_str("</div>");
}

fn render_definition_list(
    out: &mut String,
    d: &DefinitionList,
    level: usize,
    options: &Options<'_>,
    state: &mut RenderState,
) {
    indent(out, level);
    out.push_str(&format!("<dl{}>", render_attrs(&d.attrs)));
    for item in &d.items {
        for term in &item.terms {
            out.push('\n');
            indent(out, level + 1);
            out.push_str(&format!("<dt{}>", render_attrs(&term.attrs)));
            render_inlines(out, term, options, state);
            out.push_str("</dt>");
        }
        for def in &item.definitions {
            out.push('\n');
            indent(out, level + 1);
            // THE TIGHT FORM IS CHOSEN FROM WHAT PUBLISHES, not from the node
            // count (§17 L1a). A trailing comment is a node and reaches no
            // target, so counting nodes put `:  a` / `   %% c` in the loose
            // form while the LIST twin, which filters the same set before
            // deciding, stayed tight. Same clause, same answer - and the
            // filtered blocks are what render below either way, so the two
            // branches cannot disagree about which children exist.
            let mut published = def.iter().filter(|child| !publishes_nothing(child));
            if let (Some(BlockNode::Paragraph(p)), None) = (published.next(), published.next()) {
                out.push_str(&format!("<dd{}>", render_attrs(&def.attrs)));
                render_inlines(out, &p.children, options, state);
                out.push_str("</dd>");
                continue;
            }
            let blocks = rendered_children(def, level + 2, options, state);
            out.push_str(&format!("<dd{}>", render_attrs(&def.attrs)));
            // A definition whose whole body renders to nothing closes on its
            // own line, like the single-paragraph form above.
            if blocks.is_empty() {
                out.push_str("</dd>");
                continue;
            }
            for block in &blocks {
                out.push('\n');
                out.push_str(block);
            }
            out.push('\n');
            indent(out, level + 1);
            out.push_str("</dd>");
        }
    }
    out.push('\n');
    indent(out, level);
    out.push_str("</dl>");
}

fn render_figure(
    out: &mut String,
    f: &Figure,
    level: usize,
    options: &Options<'_>,
    state: &mut RenderState,
) {
    indent(out, level);
    out.push_str(&format!("<figure{}>", render_attrs(&f.attrs)));
    render_figure_contents(out, f, level, options, state);
}

/// The class-first attribute string a typed wrapper opens with: the structural
/// class leads, the author's classes merge after it, then the id and remaining
/// attributes in source order - the `admonition {kind}` convention, reused by
/// the figure group and its panels (PART 9 §4c).
fn class_first_attrs(base: &str, attrs: &Option<Attrs>) -> String {
    let (class, rest) = match attrs {
        Some(at) if !at.classes.is_empty() => (
            dedup_class_str(&format!("{} {}", base, at.classes.join(" "))),
            render_attrs_after_class(at),
        ),
        Some(at) => (base.to_string(), render_attrs_after_class(at)),
        None => (base.to_string(), String::new()),
    };
    format!(" class=\"{}\"{}", escape_attr(&class), rest)
}

/// Everything of a figure after its opening tag: the target, the caption and
/// the closing tag. Split out so a PANEL of a figure group renders the same
/// body under its class-first opener.
fn render_figure_contents(
    out: &mut String,
    f: &Figure,
    level: usize,
    options: &Options<'_>,
    state: &mut RenderState,
) {
    out.push('\n');
    match &*f.target {
        FigureTarget::Image(img) => {
            indent(out, level + 1);
            render_image(out, img);
        }
        FigureTarget::BlockQuote(b) => render_blockquote(out, b, level + 1, options, state),
        FigureTarget::Table(t) => render_table(out, t, level + 1, options, state),
        FigureTarget::CodeBlock(cb) => render_block(
            out,
            &BlockNode::CodeBlock(cb.clone()),
            level + 1,
            options,
            state,
        ),
        FigureTarget::Paragraph(p) => render_block(
            out,
            &BlockNode::Paragraph(p.clone()),
            level + 1,
            options,
            state,
        ),
    }
    out.push('\n');
    indent(out, level + 1);
    out.push_str("<figcaption>");
    render_inlines(out, &f.caption, options, state);
    out.push_str("</figcaption>");
    out.push('\n');
    indent(out, level);
    out.push_str("</figure>");
}

/// A composite figure (PART 9 §4c): one `<figure>` carrying the
/// `carve-figure-group` class first, its children DIRECTLY inside it, and the
/// group caption - when the closer hosted one - as the trailing
/// `<figcaption>`. Panels are the `Figure` and `Table` children, in source
/// order: a `Figure` renders its usual body under a class-first
/// `carve-figure-panel` opener, a `Table` is wrapped in an explicit panel
/// `<figure>` (a table does not render as a figure on its own) and keeps its
/// own `<caption>`. Everything else is preserved in place.
///
/// No wrapper element sits between the group and its panels. HTML's figure
/// content model is one figcaption, first or last, plus flow content - and a
/// figure is itself flow content, so the panel figures are exactly what the
/// group element admits and the intermediate div carried nothing a consumer
/// could not read from the panel class. It is also the shape Pandoc's writers
/// produce for native subfigures, so one stylesheet serves both.
fn render_figure_group(
    out: &mut String,
    g: &FigureGroup,
    level: usize,
    options: &Options<'_>,
    state: &mut RenderState,
) {
    indent(out, level);
    out.push_str(&format!(
        "<figure{}>",
        class_first_attrs("carve-figure-group", &g.attrs)
    ));
    for child in &g.children {
        let mut piece = String::new();
        match child {
            BlockNode::Figure(f) => {
                indent(&mut piece, level + 1);
                piece.push_str(&format!(
                    "<figure{}>",
                    class_first_attrs("carve-figure-panel", &f.attrs)
                ));
                render_figure_contents(&mut piece, f, level + 1, options, state);
            }
            BlockNode::Table(t) => {
                indent(&mut piece, level + 1);
                piece.push_str("<figure class=\"carve-figure-panel\">");
                piece.push('\n');
                render_table(&mut piece, t, level + 2, options, state);
                piece.push('\n');
                indent(&mut piece, level + 1);
                piece.push_str("</figure>");
            }
            // Preserved in place; a block that renders nothing (a comment, a
            // definition line) contributes no blank line to the group.
            other => render_block(&mut piece, other, level + 1, options, state),
        }
        if !piece.is_empty() {
            out.push('\n');
            out.push_str(&piece);
        }
    }
    if let Some(caption) = &g.caption {
        out.push('\n');
        indent(out, level + 1);
        out.push_str("<figcaption>");
        render_inlines(out, caption, options, state);
        out.push_str("</figcaption>");
    }
    out.push('\n');
    indent(out, level);
    out.push_str("</figure>");
}

fn render_block_extension(
    out: &mut String,
    node: &BlockExtension,
    level: usize,
    options: &Options<'_>,
    state: &mut RenderState,
) {
    // Share the live heading-id counter with the extension's render context so
    // a child heading rendered via `ctx.render_blocks_at` continues the
    // document's numbering (e.g. a duplicate slug inside a details block gets
    // its `-2` suffix) instead of restarting from a fresh counter.
    let shared = std::cell::RefCell::new(state);
    {
        let ctx = RenderContext::with_level_and_state(options, level, &shared);
        for ext in &options.extensions {
            if let Some(html) = ext.render_block_extension(node, &ctx) {
                indent(out, level);
                out.push_str(&html);
                return;
            }
        }
    }
    let mut state = shared.borrow_mut();
    indent(out, level);
    out.push_str(&format!("<div class=\"{}\">", escape_attr(&node.name)));
    let children = rendered_children(&node.children, level + 1, options, &mut state);
    if !children.is_empty() {
        out.push('\n');
        let mut first = true;
        for child in &children {
            if !first {
                out.push('\n');
            }
            out.push_str(child);
            first = false;
        }
        out.push('\n');
        indent(out, level);
    }
    out.push_str("</div>");
}

fn render_image(out: &mut String, img: &Image) {
    if img.ref_label.is_some() && img.src.is_empty() {
        out.push_str(&escape_text(img.raw_ref.as_deref().unwrap_or_default()));
        return;
    }
    out.push_str(&format!(
        "<img src=\"{}\" alt=\"{}\"",
        escape_attr(&sanitize_url(&img.src)),
        escape_attr(&img.alt)
    ));
    if let Some(title) = &img.title {
        out.push_str(&format!(" title=\"{}\"", escape_attr(title)));
    }
    out.push_str(&render_attrs_without_keys(&img.attrs, &["src"]));
    out.push('>');
}

// ---- Inline ----

pub(crate) fn render_inlines_with_options(nodes: &[InlineNode], options: &Options<'_>) -> String {
    render_inlines_at_link_depth(nodes, options, 0)
}

/// Render inline nodes that will be placed INSIDE an anchor the caller emits.
///
/// A derived display text is not free-standing prose: a table-of-contents entry
/// is written into an `<a href="#id">` the list builder emits itself, so a
/// construct in it that would open its own anchor has to be told, exactly as it
/// is told when it sits in a link's label. Without this the entry rendered at
/// depth 0 and a heading holding a mention, a tag or a cross-reference published
/// an `<a>` inside the entry's own `<a>` (PART 12 section 3a, LINKS NEVER NEST).
pub(crate) fn render_inlines_inside_anchor(nodes: &[InlineNode], options: &Options<'_>) -> String {
    render_inlines_at_link_depth(nodes, options, 1)
}

fn render_inlines_at_link_depth(
    nodes: &[InlineNode],
    options: &Options<'_>,
    link_depth: usize,
) -> String {
    let mut out = String::new();
    let mut state = RenderState {
        lowercase_heading_ids: options.lowercase_heading_ids,
        link_depth,
        ..RenderState::default()
    };
    render_inlines(&mut out, nodes, options, &mut state);
    out
}

fn render_inlines(
    out: &mut String,
    nodes: &[InlineNode],
    options: &Options<'_>,
    state: &mut RenderState,
) {
    render_inlines_stateful(out, nodes, options, state);
}

fn render_inlines_stateful(
    out: &mut String,
    nodes: &[InlineNode],
    options: &Options<'_>,
    state: &mut RenderState,
) {
    if state.inline_depth > MAX_RENDER_DEPTH {
        crate::render_depth::record("html");
        return;
    }
    state.inline_depth += 1;
    for node in nodes {
        render_inline_after(out, node, options, state);
    }
    state.inline_depth -= 1;
}

const SEMANTIC_SPAN_ORDER: [&str; 3] = ["abbr", "time", "kbd"];

/// The full order, including the four names an extension may add (PART 9 §10).
pub(crate) const EXTENDED_SEMANTIC_SPAN_ORDER: [&str; 7] =
    ["abbr", "time", "samp", "var", "kbd", "cite", "dfn"];

/// The attribute an authored VALUE on a semantic name reaches the output as.
///
/// `None` says the value only selects the wrapper and is dropped - which is
/// what `lint::lint_carve` reports as `semantic-attribute-value-ignored`. The
/// lint reads this rather than keeping a list of its own, so a name that starts
/// or stops carrying its value cannot be right in one place and stale in the
/// other (markup-carve/carve#1131).
pub(crate) fn semantic_value_target(name: &str) -> Option<&'static str> {
    match name {
        // `dfn` is the SemanticSpan extension's, and maps its value the same
        // way `abbr` does (docs/extensions.md §11.1). The mapping lives here
        // rather than in the extension for the same reason the order does: one
        // implementation, not two that drift.
        "abbr" | "dfn" => Some("title"),
        "time" => Some("datetime"),
        _ => None,
    }
}

/// The names this render consumes, in the canonical order: core's three plus
/// whatever a registered extension claims.
pub(crate) fn semantic_span_order(options: &Options<'_>) -> Vec<&'static str> {
    if options
        .extensions
        .iter()
        .all(|ext| ext.semantic_span_names().is_empty())
    {
        return SEMANTIC_SPAN_ORDER.to_vec();
    }
    EXTENDED_SEMANTIC_SPAN_ORDER
        .iter()
        .copied()
        .filter(|name| {
            SEMANTIC_SPAN_ORDER.contains(name)
                || options
                    .extensions
                    .iter()
                    .any(|ext| ext.semantic_span_names().contains(name))
        })
        .collect()
}

/// PART 10 §10 compact semantic attributes on an ordinary authored span.
fn render_semantic_span(
    out: &mut String,
    span: &Span,
    options: &Options<'_>,
    state: &mut RenderState,
) {
    let Some(attrs) = &span.attrs else {
        out.push_str("<span>");
        render_inlines_stateful(out, &span.children, options, state);
        out.push_str("</span>");
        return;
    };
    let order = semantic_span_order(options);
    let names: Vec<&str> = order
        .iter()
        .copied()
        .filter(|name| attrs.key_values.contains_key(*name))
        .collect();
    if names.is_empty() {
        out.push_str("<span");
        write_attrs(out, &span.attrs);
        out.push('>');
        render_inlines_stateful(out, &span.children, options, state);
        out.push_str("</span>");
        return;
    }

    let mut html = String::new();
    let previous_suppression = state.suppress_automatic_abbreviation;
    if attrs.key_values.contains_key("abbr") {
        state.suppress_automatic_abbreviation = true;
    }
    render_inlines_stateful(&mut html, &span.children, options, state);
    state.suppress_automatic_abbreviation = previous_suppression;
    let mut rest = attrs.clone();
    rest.key_values
        .retain(|name, _| !order.contains(&name.as_str()));
    rest.order.retain(|slot| match slot {
        AttrSlot::Key(name) => !order.contains(&name.as_str()),
        _ => true,
    });
    let outermost = *names.last().expect("semantic names is non-empty");
    for name in names {
        let value = &attrs.key_values[name];
        let mapped = if value.is_empty() {
            None
        } else {
            semantic_value_target(name).map(|key| (key, value.as_str()))
        };

        let mut own = Attrs::default();
        if let Some((key, value)) = mapped {
            // A derived attribute occupies the same slot as an authored one.
            // The authored value in `rest` therefore wins instead of producing
            // a duplicate attribute.
            if name != outermost || !rest.key_values.contains_key(key) {
                own.key_values.insert(key.to_string(), value.to_string());
                own.order.push(AttrSlot::Key(key.to_string()));
            }
        }
        if name == outermost {
            own.id = rest.id.clone();
            own.classes = rest.classes.clone();
            for (key, value) in &rest.key_values {
                own.key_values.insert(key.clone(), value.clone());
            }
            own.order.extend(rest.order.iter().cloned());
        }
        html = format!("<{name}{}>{html}</{name}>", render_attrs(&Some(own)));
    }
    out.push_str(&html);
}

/// Escape text content (`& < >`) and fold the no-break space U+00A0 into
/// `&nbsp;`, writing directly into `out`. Equivalent to
/// `escape_text(s).replace('\u{00a0}', "&nbsp;")` but in one pass with no
/// intermediate allocations.
fn write_escaped_text_nbsp(out: &mut String, input: &str) {
    let mut start = 0;
    for (i, ch) in input.char_indices() {
        let entity = match ch {
            '&' => "&amp;",
            '<' => "&lt;",
            '>' => "&gt;",
            // Both a literal U+00A0 and the generated-NBSP placeholder render
            // as `&nbsp;` in HTML; they only diverge in plain/ANSI output.
            '\u{00a0}' | crate::NBSP_PLACEHOLDER => "&nbsp;",
            // Trojan-Source bidi-override / isolate controls are REMOVED (not
            // escaped) from rendered text and code: an entity reference decodes
            // back to the raw control and still reorders the DOM, so only
            // physical removal is inert. See `escape::is_bidi_control`.
            c if crate::escape::is_bidi_control(c) => {
                out.push_str(&input[start..i]);
                start = i + ch.len_utf8();
                continue;
            }
            _ => continue,
        };
        out.push_str(&input[start..i]);
        out.push_str(entity);
        start = i + ch.len_utf8();
    }
    out.push_str(&input[start..]);
}

fn render_inline_after(
    out: &mut String,
    node: &InlineNode,
    options: &Options<'_>,
    state: &mut RenderState,
) {
    match node {
        // A comment renders to nothing on every target but the canonical
        // Carve writer, which is where the author gets it back.
        InlineNode::Comment(_) => {}
        InlineNode::Text(s) => {
            // Escape `& < >` AND fold U+00A0 to `&nbsp;` in a single pass over
            // `out`. None of the escaped chars is U+00A0, so the combined pass
            // is byte-identical to `escape_text(..).replace('\u{00a0}', ..)`.
            write_escaped_text_nbsp(out, &s.value);
        }
        InlineNode::EscapedText(s) => {
            // The backslash is authoring syntax; the reader sees the character.
            write_escaped_text_nbsp(out, &s.value);
        }
        InlineNode::SmartPunctuation(s) => {
            // Source mode reproduces what the author typed; the glyph is a
            // presentation choice a machine consumer cannot reverse. The node
            // carries both halves, so this is a rendering decision and the
            // tree is the same either way (divergence-from-djot section 12).
            //
            // Heading ids are deliberately NOT affected: they slug from
            // `plain_inlines`, which keeps reading the glyph and normalizes it
            // back to ASCII, so `# Don't repeat yourself` gives the same id in
            // both modes.
            let text = if options.smart_typography == crate::extension::SmartTypographyMode::Source
            {
                s.value.as_str()
            } else {
                smart_punctuation_glyph(s)
            };
            write_escaped_text_nbsp(out, text);
        }
        InlineNode::Emphasis(e) => render_emphasis(out, e, options, state),
        InlineNode::Code(s) => {
            out.push_str("<code");
            write_attrs(out, &s.attrs);
            out.push('>');
            // Escape `& < >` AND fold U+00A0 to `&nbsp;`, matching the prose
            // text path: a literal no-break space inside a code span serializes
            // as the named entity in HTML (corpus 49-non-breaking-space-3).
            write_escaped_text_nbsp(out, &s.value);
            out.push_str("</code>");
        }
        InlineNode::Link(l) => render_link(out, l, options, state),
        InlineNode::Image(img) => render_image(out, img),
        InlineNode::Span(s) => {
            render_semantic_span(out, s, options, state);
        }
        InlineNode::Math(m) => {
            let base = if m.display {
                "math display"
            } else {
                "math inline"
            };
            let open = if m.display { "\\[" } else { "\\(" };
            let close = if m.display { "\\]" } else { "\\)" };
            // PART 10 SS1: the `math {inline,display}` class is a mandatory BASE
            // class, so it is prepended INSIDE the author's class slot and the
            // slot keeps its first-appearance position. Emitting `class="..."`
            // unconditionally first put it ahead of an id the author wrote
            // before any class, which reorders what they wrote.
            //
            // carve#1168 fixed exactly this for the generic `ext-NAME` fallback
            // and left the helper below behind; the math span carries a base
            // class the same way and was missed, because no corpus case put an
            // id before a class on it (carve#1164).
            let attrs = render_attrs_with_base_class(&m.attrs, base);
            // Static mode: when a build-time math renderer is supplied, emit its
            // server-side output (MathML / HTML) inside the math span so the page
            // needs no client KaTeX / MathJax; the renderer output is trusted and
            // emitted verbatim. Absent a renderer (or in interactive mode), fall
            // back to the same delimiter-wrapped, HTML-escaped source - never
            // blank. Mirrors carve-js `render-html.ts` `case 'math'`.
            let body = match (options.is_static(), &options.renderers.math) {
                (true, Some(build)) => build(&m.content, m.display),
                _ => format!("{}{}{}", open, escape_text(&m.content), close),
            };
            out.push_str(&format!("<span{}>{}</span>", attrs, body,));
        }
        InlineNode::RawInline(r) => {
            if r.format.trim() == "html" {
                // Escape instead of emitting when raw HTML is disabled.
                if options.allow_raw_html {
                    out.push_str(&r.content);
                } else {
                    out.push_str(&escape_text(&r.content));
                }
            }
        }
        InlineNode::LiteralInline(lit) => {
            // §27: content is escaped and ALWAYS emitted (never target-routed
            // like raw passthrough), with the `<code>` wrapper dropped. A
            // `<span>` is emitted only when an attribute needs somewhere to live;
            // otherwise the escaped content is bare prose.
            if lit.attrs.is_some() {
                out.push_str("<span");
                write_attrs(out, &lit.attrs);
                out.push('>');
                write_escaped_text_nbsp(out, &lit.content);
                out.push_str("</span>");
            } else {
                write_escaped_text_nbsp(out, &lit.content);
            }
        }
        InlineNode::Symbol(e) => {
            if e.attrs.is_some() {
                out.push_str("<span");
                write_attrs(out, &e.attrs);
                out.push('>');
            }
            if let Some(value) = options.symbols.get(&e.name) {
                out.push_str(value);
            } else {
                out.push(':');
                write_escaped_text(out, &e.name);
                out.push(':');
            }
            if e.attrs.is_some() {
                out.push_str("</span>");
            }
        }
        InlineNode::AutoLink(a) => {
            // Display the raw autolink content (a URI autolink keeps its scheme).
            let display = a.text.as_str();
            out.push_str("<a href=\"");
            write_escaped_attr(out, &sanitize_url(&a.href));
            out.push('"');
            write_attrs(out, &a.attrs);
            out.push('>');
            write_escaped_text(out, display);
            out.push_str("</a>");
        }
        InlineNode::CrossRef(c) => {
            // The label is the target's cloned inline NODES, so it still
            // carries what the author typed and renders here exactly as it does
            // in the heading itself (PART 9R R4). A caption target has no nodes
            // - its label is LABEL + NUMBER - so that one stays a string.
            let resolved = state
                .crossref_index
                .resolve(&c.target)
                .map(|(id, title)| (id.to_string(), title.to_string()));
            if let Some((actual_id, title)) = resolved {
                let label = state.crossref_index.label(&actual_id);
                let opens_anchor = state.link_depth == 0;
                let mut text = String::new();
                match &label {
                    // Inside the anchor this link is about to open.
                    Some(nodes) => {
                        if opens_anchor {
                            state.link_depth += 1;
                        }
                        render_inlines(&mut text, nodes, options, state);
                        if opens_anchor {
                            state.link_depth -= 1;
                        }
                    }
                    None => write_escaped_text(&mut text, &title),
                }
                let text = charge_crossref_label(text, &c.target);
                if opens_anchor {
                    out.push_str("<a href=\"#");
                    write_escaped_attr(out, &actual_id);
                    out.push_str("\">");
                    out.push_str(&text);
                    out.push_str("</a>");
                } else {
                    out.push_str(&text);
                }
            } else {
                write_escaped_text(out, &format!("</#{}>", c.target));
            }
        }
        InlineNode::CaptionNumber(n) => {
            match n.number {
                Some(number) => out.push_str(&number.to_string()),
                // An unresolved placeholder stays the literal `#` the author
                // wrote - the visible failure this language prefers to a
                // silent one (PART 9 §4c names the panel-caption case), and
                // what the Markdown, plain and terminal targets already emit.
                None => out.push('#'),
            }
        }
        InlineNode::Mention(m) => {
            // LINKS NEVER NEST (PART 12 section 3a). A mention with a URL
            // template opens its own anchor, so inside one it renders the
            // template-less form instead - the same test the cross-reference
            // above makes, and the reason it is made HERE rather than by
            // pruning the node is that only the renderer knows whether a
            // template was configured at all.
            if let Some(template) = options
                .mention_url
                .as_ref()
                .filter(|_| state.link_depth == 0)
            {
                let encoded = percent_encode(&m.user);
                let href = template
                    .replace("{name}", &encoded)
                    .replace("{user}", &encoded);
                let (class, _) = structural_attrs("mention", &m.attrs);
                out.push_str("<a class=\"");
                write_escaped_attr(out, &class);
                out.push_str("\" href=\"");
                write_escaped_attr(out, &sanitize_url(&href));
                out.push('"');
                out.push_str(&render_attrs_after_class_without_keys(&m.attrs, &["href"]));
                out.push_str(">@");
                write_escaped_text(out, &m.user);
                out.push_str("</a>");
            } else {
                let (class, rest) = structural_attrs("mention", &m.attrs);
                out.push_str("<span class=\"");
                write_escaped_attr(out, &class);
                out.push('"');
                out.push_str(&rest);
                out.push_str("><strong>@");
                write_escaped_text(out, &m.user);
                out.push_str("</strong></span>");
            }
        }
        InlineNode::Tag(t) => {
            // LINKS NEVER NEST (PART 12 section 3a); see the mention above.
            if let Some(template) = options.tag_url.as_ref().filter(|_| state.link_depth == 0) {
                let encoded = percent_encode(&t.name);
                let href = template.replace("{name}", &encoded);
                let (class, _) = structural_attrs("tag", &t.attrs);
                out.push_str("<a class=\"");
                write_escaped_attr(out, &class);
                out.push_str("\" href=\"");
                write_escaped_attr(out, &sanitize_url(&href));
                out.push('"');
                out.push_str(&render_attrs_after_class_without_keys(&t.attrs, &["href"]));
                out.push_str(">#");
                write_escaped_text(out, &t.name);
                out.push_str("</a>");
            } else {
                let (class, rest) = structural_attrs("tag", &t.attrs);
                out.push_str("<span class=\"");
                write_escaped_attr(out, &class);
                out.push('"');
                out.push_str(&rest);
                out.push_str("><strong>#");
                write_escaped_text(out, &t.name);
                out.push_str("</strong></span>");
            }
        }
        InlineNode::CitationGroup(g) => render_citation_group(out, g, options, state),
        InlineNode::Extension(e) => render_inline_extension(out, e, options, state),
        InlineNode::Abbreviation(a) => {
            if state.suppress_automatic_abbreviation {
                write_escaped_text(out, &a.abbr);
                return;
            }
            // Bound cumulative expansion bytes: once the budget is exhausted,
            // degrade to plain key text (no `<abbr>`, no title) so a large
            // expansion repeated many times cannot amplify output without limit.
            if crate::abbr_budget::try_spend(a.expansion.len()) {
                out.push_str("<abbr title=\"");
                write_escaped_attr(out, &a.expansion);
                out.push_str("\">");
                write_escaped_text(out, &a.abbr);
                out.push_str("</abbr>");
            } else {
                write_escaped_text(out, &a.abbr);
            }
        }
        InlineNode::Footnote(f) => {
            if let (Some(number), Some(ref_id)) = (f.number, &f.ref_id) {
                out.push_str("<a id=\"");
                write_escaped_attr(out, ref_id);
                write!(out, "\" href=\"#fn{number}\" role=\"doc-noteref\"").unwrap();
                out.push_str(&render_attrs_without_id(&f.attrs));
                write!(out, "><sup>{number}</sup></a>").unwrap();
            } else if let Some(id) = &f.id {
                write_escaped_text(out, &format!("[^{id}]"));
            }
        }
        InlineNode::SoftBreak(_) => out.push('\n'),
        InlineNode::HardBreak(_) => out.push_str("<br>\n"),
        InlineNode::CriticInsert(c) => {
            out.push_str("<ins");
            write_attrs(out, &c.attrs);
            out.push('>');
            render_inlines_stateful(out, &c.children, options, state);
            out.push_str("</ins>");
        }
        InlineNode::CriticDelete(c) => {
            out.push_str("<del");
            write_attrs(out, &c.attrs);
            out.push('>');
            render_inlines_stateful(out, &c.children, options, state);
            out.push_str("</del>");
        }
        InlineNode::CriticSubstitute(c) => {
            out.push_str("<del>");
            write_escaped_text(out, &c.old_text);
            out.push_str("</del><ins>");
            write_escaped_text(out, &c.new_text);
            out.push_str("</ins>");
        }
        InlineNode::CriticComment(c) => out.push_str(&format!(
            "<span class=\"critic-comment\">{}</span>",
            escape_text(&c.text)
        )),
    }
}

fn render_citation_group(
    out: &mut String,
    g: &CitationGroup,
    options: &Options<'_>,
    state: &mut RenderState,
) {
    if g.items.iter().any(|item| item.label.is_none()) {
        out.push_str(&escape_text(&g.raw));
        return;
    }

    if g.integral {
        out.push_str("<span class=\"citation\" data-cite-mode=\"integral\">");
    }

    match g.mode.unwrap_or(CitationRenderMode::Numbered) {
        CitationRenderMode::Numbered => {
            out.push('[');
            for (idx, item) in g.items.iter().enumerate() {
                if idx > 0 {
                    out.push_str(", ");
                }
                render_citation_item(out, item, options, state);
            }
            out.push(']');
        }
        CitationRenderMode::AuthorDate => {
            out.push('(');
            for (idx, item) in g.items.iter().enumerate() {
                if idx > 0 {
                    out.push_str("; ");
                }
                render_citation_item(out, item, options, state);
            }
            out.push(')');
        }
    }

    if g.integral {
        out.push_str("</span>");
    }
}

/// Flatten inline nodes to plain text (for `data-*` attribute values).
fn flatten_text(nodes: &[InlineNode]) -> String {
    flatten_text_at(nodes, 0)
}

fn flatten_text_at(nodes: &[InlineNode], depth: usize) -> String {
    if depth > MAX_RENDER_DEPTH {
        crate::render_depth::record("html");
        return String::new();
    }
    let mut out = String::new();
    for node in nodes {
        match node {
            InlineNode::Text(t) => out.push_str(&t.value),
            InlineNode::SmartPunctuation(s) => out.push_str(smart_punctuation_glyph(s)),
            InlineNode::Emphasis(e) => out.push_str(&flatten_text_at(&e.children, depth + 1)),
            InlineNode::Link(l) => out.push_str(&flatten_text_at(&l.children, depth + 1)),
            InlineNode::Span(s) => out.push_str(&flatten_text_at(&s.children, depth + 1)),
            InlineNode::Extension(e) => out.push_str(&flatten_text_at(&e.children, depth + 1)),
            _ => {}
        }
    }
    out
}

fn render_citation_item(
    out: &mut String,
    item: &Citation,
    options: &Options<'_>,
    state: &mut RenderState,
) {
    if let Some(prefix) = &item.prefix {
        render_inlines(out, prefix, options, state);
        out.push(' ');
    }

    // Build data-* attribute string (order is normative per spec).
    let mut attrs = format!("data-cite-key=\"{}\"", escape_attr(&item.key));
    if item.suppress_author {
        attrs.push_str(" data-suppress-author=\"true\"");
    }
    let pfx = flatten_text(item.prefix.as_deref().unwrap_or(&[]));
    if !pfx.is_empty() {
        attrs.push_str(&format!(" data-cite-prefix=\"{}\"", escape_attr(&pfx)));
    }
    if let Some(ll) = &item.locator_label {
        attrs.push_str(&format!(" data-locator-label=\"{}\"", escape_attr(ll)));
    }
    if let Some(lv) = &item.locator_value {
        attrs.push_str(&format!(" data-locator=\"{}\"", escape_attr(lv)));
    }
    let sfx = flatten_text(item.suffix.as_deref().unwrap_or(&[]));
    if !sfx.is_empty() {
        attrs.push_str(&format!(" data-suffix=\"{}\"", escape_attr(&sfx)));
    }

    // A use_index is only set when a bibliography pool is active (#199); it adds
    // the per-use back-link anchor to the existing forward link. Both ids are
    // read from the per-render document id namespace (extensions contract
    // §2.6), so a collision with an explicit `{#id}` or a heading id bumps the
    // anchor id / href consistently with the references list.
    let ref_id = crate::document_ids::ref_id(&item.key);
    match item.use_index {
        Some(n) => out.push_str(&format!(
            "<a id=\"{}\" {} href=\"#{}\">{}</a>",
            escape_attr(&crate::document_ids::cite_id(&item.key, n)),
            attrs,
            escape_attr(&ref_id),
            escape_text(item.label.as_deref().unwrap_or_default())
        )),
        None => out.push_str(&format!(
            "<a {} href=\"#{}\">{}</a>",
            attrs,
            escape_attr(&ref_id),
            escape_text(item.label.as_deref().unwrap_or_default())
        )),
    }
    if let Some(locator) = &item.locator {
        out.push_str(", ");
        render_inlines(out, locator, options, state);
    }
}

fn percent_encode(input: &str) -> String {
    let mut out = String::new();
    for byte in input.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn render_emphasis(out: &mut String, e: &Emphasis, options: &Options<'_>, state: &mut RenderState) {
    let (open, close) = match e.kind {
        EmphasisKind::Italic => ("em", "em"),
        EmphasisKind::Strong => ("strong", "strong"),
        EmphasisKind::Underline => ("u", "u"),
        EmphasisKind::Strike => ("s", "s"),
        EmphasisKind::Super => ("sup", "sup"),
        EmphasisKind::Sub => ("sub", "sub"),
        EmphasisKind::Highlight => ("mark", "mark"),
        EmphasisKind::BoldItalic => ("<strong><em>", "</em></strong>"),
    };
    if e.kind == EmphasisKind::BoldItalic {
        out.push_str(open);
        render_inlines_stateful(out, &e.children, options, state);
        out.push_str(close);
    } else {
        out.push_str(&format!("<{}{}>", open, render_attrs(&e.attrs)));
        render_inlines_stateful(out, &e.children, options, state);
        out.push_str(&format!("</{}>", close));
    }
}

fn render_link(out: &mut String, l: &Link, options: &Options<'_>, state: &mut RenderState) {
    if l.ref_label.is_some() && l.href.is_empty() {
        out.push_str(&escape_text(l.raw_ref.as_deref().unwrap_or_default()));
        return;
    }
    // `href`, then the AUTHORED title, then the attribute block - the order the
    // author wrote them in, and the one carve-js, carve-php and the executable
    // spec emit. Attributes came first here, so `[E](/u "T"){.x}` published
    // `class` before `title` (carve-rs#543); an explicit `{title=Z}` beside a
    // `"T"` title still publishes both, in that order, in every engine.
    out.push_str(&format!(
        "<a href=\"{}\"",
        escape_attr(&sanitize_url(&l.href))
    ));
    if let Some(title) = &l.title {
        out.push_str(&format!(" title=\"{}\"", escape_attr(title)));
    }
    out.push_str(&render_attrs_without_keys(&l.attrs, &["href"]));
    out.push('>');
    state.link_depth += 1;
    // Render the label through the anchor-unwrapping view.
    let children = unwrap_nested_anchors(&l.children);
    render_inlines_stateful(out, children.as_ref(), options, state);
    state.link_depth -= 1;
    out.push_str("</a>");
}

fn render_inline_extension(
    out: &mut String,
    node: &InlineExtension,
    options: &Options<'_>,
    state: &mut RenderState,
) {
    let ctx = RenderContext::new(options);
    for ext in &options.extensions {
        if let Some(html) = ext.render_inline_extension(node, &ctx) {
            out.push_str(&html);
            return;
        }
    }
    // PART 9 §10: the SemanticSpan extension re-registers the seven names as a
    // SOFT-DEPRECATED spelling. Core registers none, so without the extension
    // every name falls through to the readable `ext-NAME` span below.
    if options
        .extensions
        .iter()
        .any(|ext| !ext.semantic_span_names().is_empty())
        && EXTENDED_SEMANTIC_SPAN_ORDER.contains(&node.name.as_str())
    {
        out.push_str(&format!("<{}{}>", node.name, render_attrs(&node.attrs)));
        render_inlines_stateful(out, &node.children, options, state);
        out.push_str(&format!("</{}>", node.name));
        return;
    }
    // The `ext-NAME` class is structural and emitted first; a trailing
    // attribute block merges its classes into the SAME `class` attribute
    // (`:foo[a]{.cls}` -> `class="ext-foo cls"`, never two `class` attrs) and
    // contributes id / key-values after. Matches the math-span merge and
    // carve-js / carve-php.
    let base = format!("ext-{}", node.name);
    out.push_str(&format!(
        "<span{}>",
        render_attrs_with_base_class(&node.attrs, &base)
    ));
    render_inlines_stateful(out, &node.children, options, state);
    out.push_str("</span>");
}

/// Write the ` id="..."` slot.
#[inline]
fn write_attr_id(out: &mut String, id: &str) {
    out.push_str(" id=\"");
    write_escaped_attr(out, id);
    out.push('"');
}

/// Write the ` class="..."` slot from a list of class names joined by spaces.
#[inline]
fn write_attr_class(out: &mut String, classes: &[String]) {
    out.push_str(" class=\"");
    let mut first = true;
    // Dedup repeated classes keeping first-occurrence order (`{.a .a}` ->
    // `class="a"`), matching carve-php / carve-js (§15).
    let mut seen: Vec<&str> = Vec::new();
    for class in classes {
        if seen.contains(&class.as_str()) {
            continue;
        }
        seen.push(class);
        if !first {
            out.push(' ');
        }
        write_escaped_attr(out, class);
        first = false;
    }
    out.push('"');
}

/// Dedup whitespace-separated classes, keeping first-occurrence order. Used
/// where a structural base class is merged with author classes (§15).
fn dedup_class_str(s: &str) -> String {
    let mut seen: Vec<&str> = Vec::new();
    for c in s.split_whitespace() {
        if !seen.contains(&c) {
            seen.push(c);
        }
    }
    seen.join(" ")
}

fn structural_attrs(base: &str, attrs: &Option<Attrs>) -> (String, String) {
    match attrs {
        Some(a) if !a.classes.is_empty() => (
            dedup_class_str(&format!("{} {}", base, a.classes.join(" "))),
            render_attrs_after_class(a),
        ),
        Some(a) => (base.to_string(), render_attrs_after_class(a)),
        None => (base.to_string(), String::new()),
    }
}

/// Write a ` key="value"` slot, applying the value sanitizer.
#[inline]
fn write_attr_key_value(out: &mut String, key: &str, value: &str) {
    out.push(' ');
    write_escaped_attr(out, key);
    out.push_str("=\"");
    // `sanitize_attr_value` returns the original string unchanged in the common
    // case, so escape it in place rather than always materializing a new owned
    // value.
    match sanitize_attr_value(key, value) {
        std::borrow::Cow::Borrowed(v) => write_escaped_attr(out, v),
        std::borrow::Cow::Owned(v) => write_escaped_attr(out, &v),
    }
    out.push('"');
}

fn write_attrs(out: &mut String, attrs: &Option<Attrs>) {
    let Some(attrs) = attrs else {
        return;
    };
    if attrs.order.is_empty() {
        if let Some(id) = &attrs.id {
            write_attr_id(out, id);
        }
        if !attrs.classes.is_empty() {
            write_attr_class(out, &attrs.classes);
        }
        for (key, value) in &attrs.key_values {
            if !is_dangerous_attr_name(key) && is_valid_attr_name(key) {
                write_attr_key_value(out, key, value);
            }
        }
        return;
    }
    // Track which slots the recorded `order` already emitted, so attrs an
    // extension added WITHOUT updating `order` (a stale order list) are still
    // appended below instead of silently dropped. For normally-parsed nodes
    // `order` covers everything, so the fallback emits nothing.
    let mut seen_id = false;
    let mut seen_class = false;
    let mut seen_keys: Vec<&str> = Vec::new();
    for slot in &attrs.order {
        match slot {
            AttrSlot::Id => {
                if let Some(id) = &attrs.id {
                    write_attr_id(out, id);
                }
                seen_id = true;
            }
            AttrSlot::Class => {
                if !attrs.classes.is_empty() {
                    write_attr_class(out, &attrs.classes);
                }
                seen_class = true;
            }
            AttrSlot::Key(key) => {
                if let Some(value) = attrs.key_values.get(key) {
                    if !is_dangerous_attr_name(key) && is_valid_attr_name(key) {
                        write_attr_key_value(out, key, value);
                    }
                }
                seen_keys.push(key.as_str());
            }
        }
    }
    if !seen_id {
        if let Some(id) = &attrs.id {
            write_attr_id(out, id);
        }
    }
    if !seen_class && !attrs.classes.is_empty() {
        write_attr_class(out, &attrs.classes);
    }
    for (key, value) in &attrs.key_values {
        if !seen_keys.contains(&key.as_str())
            && !is_dangerous_attr_name(key)
            && is_valid_attr_name(key)
        {
            write_attr_key_value(out, key, value);
        }
    }
}

pub(crate) fn render_attrs(attrs: &Option<Attrs>) -> String {
    let mut out = String::new();
    write_attrs(&mut out, attrs);
    out
}

pub(crate) fn render_attrs_without_keys(attrs: &Option<Attrs>, blocked: &[&str]) -> String {
    let Some(a) = attrs else {
        return String::new();
    };
    let is_blocked = |k: &str| blocked.contains(&k.to_ascii_lowercase().as_str());
    if !a.key_values.keys().any(|k| is_blocked(k)) {
        return render_attrs(attrs);
    }
    let mut filtered = a.clone();
    filtered.key_values.retain(|k, _| !is_blocked(k));
    filtered.order.retain(|slot| match slot {
        AttrSlot::Key(k) => !is_blocked(k),
        _ => true,
    });
    render_attrs(&Some(filtered))
}

/// Render an attribute block's id and key-values in source order, omitting
/// the class slot. Used by a node whose class is structural and merged
/// separately (the math span: `class="math inline {extra}"`).
/// Render an attribute block in SOURCE ORDER with a mandatory base class
/// merged into the author's class slot.
///
/// PART 10 SS1 emits authored attributes in the order they were written, and a
/// structural class belongs in the class slot rather than ahead of everything.
/// Writing `class="..."` unconditionally first REORDERED the author's
/// attributes: `:widget[x]{#copy .shortcut}` came back as
/// `<span class="ext-widget shortcut" id="copy">` where carve-js keeps
/// `<span id="copy" class="ext-widget shortcut">` (markup-carve/carve#1164).
///
/// With no class slot to merge into there is no authored position to respect,
/// so the base class leads - which is what carve-js does for `{#copy}` and
/// `{k=v}` alike.
pub(crate) fn render_attrs_with_base_class(attrs: &Option<Attrs>, base: &str) -> String {
    let Some(attrs) = attrs else {
        return format!(" class=\"{}\"", escape_attr(base));
    };
    let merged = |classes: &[String]| -> String {
        let joined = if classes.is_empty() {
            base.to_string()
        } else {
            format!("{} {}", base, classes.join(" "))
        };
        format!(" class=\"{}\"", escape_attr(&dedup_class_str(&joined)))
    };
    // No recorded order: the slots go in the canonical order, class first.
    if attrs.order.is_empty() {
        let mut out = merged(&attrs.classes);
        if let Some(id) = &attrs.id {
            write_attr_id(&mut out, id);
        }
        for (key, value) in &attrs.key_values {
            if !is_dangerous_attr_name(key) && is_valid_attr_name(key) {
                out.push_str(&format!(
                    " {}=\"{}\"",
                    escape_attr(key),
                    escape_attr(&sanitize_attr_value(key, value))
                ));
            }
        }

        return out;
    }

    let has_class_slot = attrs.order.iter().any(|s| matches!(s, AttrSlot::Class));
    let mut out = String::new();
    if !has_class_slot {
        out.push_str(&merged(&attrs.classes));
    }
    let mut class_written = false;
    for slot in &attrs.order {
        match slot {
            AttrSlot::Id => {
                if let Some(id) = &attrs.id {
                    write_attr_id(&mut out, id);
                }
            }
            AttrSlot::Class => {
                // The FIRST class slot carries the merge; a second one would be
                // a second `class` attribute, which is never valid.
                if !class_written {
                    out.push_str(&merged(&attrs.classes));
                    class_written = true;
                }
            }
            AttrSlot::Key(key) => {
                if let Some(value) = attrs.key_values.get(key) {
                    if !is_dangerous_attr_name(key) && is_valid_attr_name(key) {
                        out.push_str(&format!(
                            " {}=\"{}\"",
                            escape_attr(key),
                            escape_attr(&sanitize_attr_value(key, value))
                        ));
                    }
                }
            }
        }
    }

    out
}

pub(crate) fn render_attrs_after_class(attrs: &Attrs) -> String {
    let mut out = String::new();
    if attrs.order.is_empty() {
        if let Some(id) = &attrs.id {
            out.push_str(&format!(" id=\"{}\"", escape_attr(id)));
        }
        for (key, value) in &attrs.key_values {
            if !is_dangerous_attr_name(key) && is_valid_attr_name(key) {
                out.push_str(&format!(
                    " {}=\"{}\"",
                    escape_attr(key),
                    escape_attr(&sanitize_attr_value(key, value))
                ));
            }
        }
        return out;
    }
    for slot in &attrs.order {
        match slot {
            AttrSlot::Id => {
                if let Some(id) = &attrs.id {
                    out.push_str(&format!(" id=\"{}\"", escape_attr(id)));
                }
            }
            AttrSlot::Class => {}
            AttrSlot::Key(key) => {
                if let Some(value) = attrs.key_values.get(key) {
                    if !is_dangerous_attr_name(key) && is_valid_attr_name(key) {
                        out.push_str(&format!(
                            " {}=\"{}\"",
                            escape_attr(key),
                            escape_attr(&sanitize_attr_value(key, value))
                        ));
                    }
                }
            }
        }
    }
    out
}

fn render_attrs_after_class_without_keys(attrs: &Option<Attrs>, blocked: &[&str]) -> String {
    let Some(attrs) = attrs else {
        return String::new();
    };
    let is_blocked = |key: &str| blocked.contains(&key.to_ascii_lowercase().as_str());
    if !attrs.key_values.keys().any(|key| is_blocked(key)) {
        return render_attrs_after_class(attrs);
    }
    let mut filtered = attrs.clone();
    filtered.key_values.retain(|key, _| !is_blocked(key));
    filtered.order.retain(|slot| match slot {
        AttrSlot::Key(key) => !is_blocked(key),
        _ => true,
    });
    render_attrs_after_class(&filtered)
}

fn render_attrs_without_id(attrs: &Option<Attrs>) -> String {
    let mut attrs = attrs.clone();
    if let Some(attrs) = &mut attrs {
        attrs.id = None;
        attrs.order.retain(|slot| !matches!(slot, AttrSlot::Id));
    }
    render_attrs(&attrs)
}

/// Allocate a run of `n` hyphens (`n >= 2`) into em/en dashes, matching the
/// canonical carve-js/carve-php/oracle `allocateDashes` (djot allocation): all
/// em when divisible by 3, all en when divisible by 2, otherwise as many
/// em-dashes as fit with the remainder as en-dashes - where a remainder of 1
/// trades one em for two en, so no literal hyphen is ever left over. Examples:
/// 2->en, 3->em, 4->en+en, 5->em+en, 6->em+em, 7->em+en+en, 8->en*4, 9->em*3.
pub(crate) fn allocate_dashes(n: usize) -> String {
    const EM: &str = "\u{2014}";
    const EN: &str = "\u{2013}";
    if n % 3 == 0 {
        return EM.repeat(n / 3);
    }
    if n % 2 == 0 {
        return EN.repeat(n / 2);
    }
    // Odd and not divisible by 3: the smallest such n is 5, and for n % 3 == 1
    // the smallest is 7, so `n / 3 >= 1` (and `>= 2` in the trade case) - the
    // `em -= 1` below never underflows given the `n >= 2` contract.
    let (em, en) = if n % 3 == 1 {
        (n / 3 - 1, 2)
    } else {
        (n / 3, 1)
    };
    let mut out = String::with_capacity((em + en) * 3);
    out.push_str(&EM.repeat(em));
    out.push_str(&EN.repeat(en));
    out
}
