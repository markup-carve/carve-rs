//! HTML renderer — emits the canonical output the spec corpus expects.
//!
//! Output style matches `carve-js/render-html.ts`: block elements on
//! their own line; inline content flows within the block tag. Lists
//! indent their `<li>` children two spaces.

use crate::abbr_budget::AbbrBudgetGuard;
use crate::ast::*;
use crate::escape::{
    escape_attr, escape_text, has_denied_url_scheme, is_dangerous_attr_name, is_valid_attr_name,
    sanitize_attr_value, sanitize_destination, write_escaped_attr, write_escaped_text,
    write_escaped_text_inner,
};
use crate::extension::{
    HeadingIdOptions, Options, RenderContext, SocialLinkKind, SocialLinkResolverInput,
};
use crate::parse::{label_key, unwrap_nested_anchors};
use crate::table_spans::TableSpanPlan;
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Write as _;

/// The recursion bound every renderer shares, and it MUST sit ABOVE the
/// parser's (§25).
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
    // BEFORE the clone below, which is itself a recursion over the tree with no
    // ceiling to consult (`crate::render_depth::refuse_if_too_deep`).
    if crate::render_depth::html_preparation_needed(doc)? {
        render_html_owned_unchecked(doc.clone(), options)
    } else {
        let watch = crate::render_depth::RenderDepthWatch::new();
        let index = crate::parse::crossref_index_for_document(doc, options.heading_id_options());
        watch.into_result(render_prepared_html(doc, &[], options, index, true))
    }
}

/// Render an owned document without cloning its tree.
///
/// Returns an error if the tree exceeds [`MAX_RENDER_DEPTH`].
///
/// ```
/// let doc = carve::parse("Hello.");
/// assert_eq!(carve::render_html_owned(doc).unwrap(), "<p>Hello.</p>");
/// ```
pub fn render_html_owned(doc: Document) -> Result<String, crate::RenderDepthError> {
    render_html_owned_with_options(doc, &Options::default())
}

/// Render an owned document with options, without cloning its tree.
///
/// Returns an error before preprocessing if the tree exceeds [`MAX_RENDER_DEPTH`].
pub fn render_html_owned_with_options(
    doc: Document,
    options: &Options<'_>,
) -> Result<String, crate::RenderDepthError> {
    crate::render_depth::refuse_if_too_deep(&doc, "html")?;
    render_html_owned_unchecked(doc, options)
}

fn render_html_owned_unchecked(
    doc: Document,
    options: &Options<'_>,
) -> Result<String, crate::RenderDepthError> {
    let watch = crate::render_depth::RenderDepthWatch::new();
    watch.into_result(render_html_inner(doc, options, None, true, true))
}

pub(crate) fn render_html_owned_with_index(
    doc: Document,
    options: &Options<'_>,
    crossref_index: crate::parse::CrossrefIndex,
    needs_document_ids: bool,
    needs_footnotes: bool,
) -> Result<String, crate::RenderDepthError> {
    let watch = crate::render_depth::RenderDepthWatch::new();
    watch.into_result(render_html_inner(
        doc,
        options,
        Some(crossref_index),
        needs_document_ids,
        needs_footnotes,
    ))
}

fn render_html_inner(
    mut doc: Document,
    options: &Options<'_>,
    crossref_index: Option<crate::parse::CrossrefIndex>,
    needs_document_ids: bool,
    needs_footnotes: bool,
) -> String {
    // A lone image paragraph renders as a bare block image at EVERY column, and
    // the parse tree deliberately does not (markup-carve/carve#1660). This is
    // the one place that difference is resolved, ahead of every layout decision
    // below, so nothing downstream has to know about the second shape.
    crate::parse::collapse_lone_image_paragraphs(&mut doc);
    if options.html_carriers() {
        crate::roundtrip::report_reserved_attributes(&mut doc);
    }
    let crossref_index = crossref_index.unwrap_or_else(|| {
        crate::parse::crossref_index_for_document(&doc, options.heading_id_options())
    });
    // Numbering and reference ids are assigned ONCE, ahead of the pass below,
    // because that pass can run a second time: collecting again per pass would
    // be a second chance for the two runs to disagree about a number that is
    // already written into the body they both render.
    let footnotes = if needs_footnotes {
        collect_footnotes(&mut doc, true)
    } else {
        Vec::new()
    };

    render_prepared_html(
        &doc,
        &footnotes,
        options,
        crossref_index,
        needs_document_ids,
    )
}

fn render_prepared_html(
    doc: &Document,
    footnotes: &[FootnoteEntry],
    options: &Options<'_>,
    crossref_index: crate::parse::CrossrefIndex,
    needs_document_ids: bool,
) -> String {
    let losses = crate::render_loss::checkpoint();
    let mut default_buffer = [0; 4];
    let default_marker = FOOTNOTES_PLACEMENT_DEFAULT.encode_utf8(&mut default_buffer);
    let (first, crossref_index) = render_html_pass(
        doc,
        footnotes,
        options,
        crossref_index,
        needs_document_ids,
        default_marker,
    );
    if first.seen == first.emitted {
        return first.html;
    }
    // The count is a LOWER BOUND on the markers this pass put in the document
    // (see `without_placement_accounting`), so `seen` can only differ upward,
    // and it differs when the DOCUMENT carries a marker of its own - the case a
    // fixed marker answers by rewriting the author's text into a footnotes
    // section. Re-pick against every private-use code point the assembled
    // document holds (the author's and this pass's own, a superset, so the new
    // marker is free of both) and render again.
    //
    // A THIRD PASS CANNOT BE NEEDED: the retry runs the same code over the same
    // tree, and the only thing that differs is a marker the assembled
    // document does not contain. Same shape, and the same reasoning, as the
    // Markdown target's carrier retry in `render_markdown_inner`.
    let picked = crate::sentinel_run::pick_sentinel_run::<1>(
        first
            .occupied
            .as_ref()
            .expect("collected when seen differs from emitted"),
        u32::from(FOOTNOTES_PLACEMENT_DEFAULT),
    )[0];
    let picked = if first
        .occupied
        .as_ref()
        .is_some_and(|set| set.contains(&u32::from(picked)))
    {
        let nonces = first
            .nonce_cores
            .as_ref()
            .expect("collected before the splice");
        let nonce = (0..=nonces.len())
            .find(|nonce| !nonces.contains(nonce))
            .unwrap();
        format!("\u{e009}m{nonce}:html\u{e00a}")
    } else {
        picked.to_string()
    };
    losses.restore();
    let (second, _) = render_html_pass(
        doc,
        footnotes,
        options,
        crossref_index,
        needs_document_ids,
        &picked,
    );
    second.html
}

/// One whole-document render, with ONE placement marker installed for it.
struct HtmlPass {
    html: String,
    /// Markers this pass wrote ON A PATH THAT REACHES THE DOCUMENT - a lower
    /// bound, not a total (see `without_placement_accounting`).
    emitted: usize,
    /// Markers that STOOD in the rendered body and endnotes section, counted
    /// before any of them was consumed. More than were written means the
    /// document carries one of its own, or an extension embedded a fragment
    /// whose markers `emitted` deliberately leaves out.
    seen: usize,
    /// Which private-use code points the rendered body and endnotes section
    /// held, collected only when `seen` differed from `emitted` and only from
    /// the text as it stood BEFORE the splice consumed a marker.
    occupied: Option<std::collections::BTreeSet<u32>>,
    nonce_cores: Option<std::collections::BTreeSet<usize>>,
}

/// Render `doc` with `marker` standing for a `::: footnotes` placement block,
/// handing the cross-reference index back for a possible second pass.
fn render_html_pass(
    doc: &Document,
    footnotes: &[FootnoteEntry],
    options: &Options<'_>,
    crossref_index: crate::parse::CrossrefIndex,
    needs_document_ids: bool,
    marker: &str,
) -> (HtmlPass, crate::parse::CrossrefIndex) {
    let _abbr_guard = AbbrBudgetGuard::for_document(doc);
    let _index_guard = crate::index_budget::IndexBudgetGuard::new(doc.expansion_budget_len());
    // Document id namespace (extensions contract §2.6): seeded with every
    // explicit `{#id}` attribute and every heading id this render will assign,
    // so extension-generated ids (citation anchors / reference entries) take
    // the next free suffix instead of emitting a duplicate DOM id.
    let _document_ids_guard = needs_document_ids
        .then(|| crate::document_ids::DocumentIdsGuard::new(doc, options.heading_id_options()));
    // EVERY guard here is installed PER PASS, and that is what makes a retry
    // free of consequences: a second pass rendering into a half-consumed id
    // namespace would take the next free suffix and move ids the first pass had
    // already placed, and one rendering into a half-spent expansion budget would
    // clip abbreviations the first pass emitted whole.
    let _placement_guard = FootnotesPlacementGuard::install(marker);
    let _reserved_guard = ReservedNameGuard::install(options.html_carriers());

    let mut state = RenderState {
        heading_id_options: options.heading_id_options(),
        crossref_index,
        has_footnotes: !footnotes.is_empty(),
        ..RenderState::default()
    };
    let mut html =
        render_document_blocks(doc.children.as_slice(), options, &mut state, doc.source_len);
    // PART 10 §13a: the document's abbreviation definitions, one per line, in a
    // `<template>` the browser never renders. An expansion reaches HTML as an
    // `<abbr title>` on each USE, so a document whose definitions were never
    // used, or whose `<abbr>` the author overrode, has nowhere else to keep
    // them. Appended before the endnotes section, matching carve-php.
    if options.html_carriers() {
        if let Some(template) = abbreviation_template(doc) {
            if !html.is_empty() {
                html.push('\n');
            }
            html.push_str(&template);
        }
    }
    let mut seen = html.matches(marker).count();
    let section = (!footnotes.is_empty())
        .then(|| render_footnotes_section(doc, footnotes, options, &mut state));
    if let Some(section) = &section {
        seen += section.matches(marker).count();
    }
    let emitted = PLACEMENTS_EMITTED.with(Cell::get);
    // OCCUPANCY IS READ BEFORE THE SPLICE, and that is the whole reason this
    // pass carries a set rather than the caller scanning the finished document.
    // The splice CONSUMES a marker - including the author's own, which is the
    // collision being answered - so a set collected from the finished document
    // reports the very code point that collided as free, and the retry picks it
    // again.
    let occupied = (seen != emitted).then(|| {
        let mut set = crate::sentinel_run::occupied_private_use(&html);
        if let Some(section) = &section {
            crate::sentinel_run::collect_private_use(section, &mut set);
        }
        set
    });
    let nonce_cores = occupied
        .as_ref()
        .filter(|set| {
            set.len()
                == (crate::sentinel_run::POOL_LAST - crate::sentinel_run::POOL_FIRST + 1) as usize
        })
        .map(|_| {
            let mut nonces = std::collections::BTreeSet::new();
            crate::sentinel_run::collect_marker_nonce_cores(&html, &mut nonces);
            if let Some(section) = &section {
                crate::sentinel_run::collect_marker_nonce_cores(section, &mut nonces);
            }
            nonces
        });
    if let Some(section) = section {
        // `::: footnotes` placement: every footnote is numbered by now, so flush
        // the endnotes at the marker instead of appending at the end. A
        // document without the marker is byte-identical to before.
        if html.contains(marker) {
            html = place_footnotes_section(html, &section, marker);
        } else {
            html.push('\n');
            html.push_str(&section);
        }
    }
    // Sweep any marker that still remains and degrade it to an empty
    // placeholder: a `::: footnotes` nested INSIDE a footnote definition emits a
    // marker while the endnotes section renders (after the body check above),
    // and a marker in a document with no footnotes never hit the branch above.
    // The raw marker must never leak into output.
    if html.contains(marker) {
        html = html.replace(marker, "<div class=\"footnotes\"></div>");
    }
    let pass = HtmlPass {
        html,
        emitted,
        seen,
        occupied,
        nonce_cores,
    };
    (pass, state.crossref_index)
}

/// The `<template>` holding every top-level `*[abbr]: expansion` definition, or
/// `None` where the document has none.
///
/// TOP LEVEL ONLY, deliberately: a definition is a document-level construct and
/// the expansion pass reads it as one, so walking containers would collect
/// something that never contributed an expansion.
fn abbreviation_template(doc: &Document) -> Option<String> {
    let mut lines = Vec::new();
    for block in &doc.children {
        if let BlockNode::AbbreviationDef(def) = block {
            lines.push(format!("*[{}]: {}", def.abbr, def.expansion));
        }
    }
    if lines.is_empty() {
        return None;
    }
    let mut out = String::from("<template");
    write_bare_carrier(
        &mut out,
        crate::roundtrip::ABBREVIATIONS,
        &None,
        None,
        crate::render_loss::RawNodeType::Block,
    );
    out.push('>');
    write_escaped_text_nbsp(&mut out, &lines.join("\n"));
    out.push_str("</template>");
    Some(out)
}

/// The preferred marker for a `::: footnotes` placement block: what a document
/// that writes no private-use character of its own gets.
const FOOTNOTES_PLACEMENT_DEFAULT: char = '\u{e008}';

thread_local! {
    /// The placement marker installed for the render running on this thread.
    ///
    /// A thread-local rather than a [`RenderState`] field for the reason the
    /// parser's `PLACEHOLDERS` is one: a nested render - a block extension
    /// rendering sub-blocks, an extension calling back into the renderer -
    /// starts a FRESH `RenderState`, and a marker carried on that struct would
    /// be the default again inside it while the outer render looked for the
    /// picked one.
    #[allow(clippy::missing_const_for_thread_local)]
    static FOOTNOTES_PLACEMENT: RefCell<String> = RefCell::new(FOOTNOTES_PLACEMENT_DEFAULT.to_string());

    /// How many placement markers the render running on this thread has WRITTEN,
    /// which is what a count of markers standing in the output is compared
    /// against. Nested renders share it for the same reason as above: their
    /// markers reach the same assembled document.
    static PLACEMENTS_EMITTED: Cell<usize> = const { Cell::new(0) };
}

thread_local! {
    /// Whether the render on this thread is a roundtrip export, which drops an
    /// authored `data-carve-src` (PART 10 §13a). A thread-local for the reason
    /// `FOOTNOTES_PLACEMENT` is one: the attribute writers take no options, and
    /// a nested render has to answer the same way.
    static DROPS_RESERVED_NAME: Cell<bool> = const { Cell::new(false) };

}

struct ReservedNameGuard(bool);

impl ReservedNameGuard {
    fn install(active: bool) -> ReservedNameGuard {
        ReservedNameGuard(DROPS_RESERVED_NAME.with(|slot| slot.replace(active)))
    }
}

impl Drop for ReservedNameGuard {
    fn drop(&mut self) {
        DROPS_RESERVED_NAME.with(|slot| slot.set(self.0));
    }
}

/// Whether an authored attribute reaches the element: its name is safe and
/// spellable, and roundtrip export does not reserve it.
///
/// Reserved names are reported separately before the render pass.
pub(crate) fn writes_authored_key(key: &str) -> bool {
    if is_dangerous_attr_name(key) || !is_valid_attr_name(key) {
        return false;
    }
    if crate::roundtrip::canonical(key).is_none() || !DROPS_RESERVED_NAME.with(Cell::get) {
        return true;
    }

    false
}

/// Install `marker` for the duration of one pass, restoring the previous marker
/// and emission count on drop (panic unwind included), so a nested render leaves
/// the outer one exactly as it found it.
struct FootnotesPlacementGuard(String, usize);

impl FootnotesPlacementGuard {
    fn install(marker: &str) -> FootnotesPlacementGuard {
        FootnotesPlacementGuard(
            FOOTNOTES_PLACEMENT.with(|slot| slot.replace(marker.to_string())),
            PLACEMENTS_EMITTED.with(|slot| slot.replace(0)),
        )
    }
}

impl Drop for FootnotesPlacementGuard {
    fn drop(&mut self) {
        FOOTNOTES_PLACEMENT.with(|slot| {
            *slot.borrow_mut() = std::mem::take(&mut self.0);
        });
        PLACEMENTS_EMITTED.with(|slot| slot.set(self.1));
    }
}

/// Render a fragment that is HANDED TO AN EXTENSION, and do not count the
/// placement markers it writes.
///
/// The count is compared against the markers standing in the assembled
/// document, and that comparison is only sound while every counted marker is
/// certain to BE in it. A core render writes into its caller's buffer, which
/// always flows up; a fragment handed to an extension does not - the extension
/// may embed it, embed part of it, or drop it. Counting those markers lets a
/// dropped one pay for an authored one, and the two cancel into a comparison
/// that reports no collision.
///
/// Not counting them keeps the count a LOWER BOUND on the markers this render
/// put in the document, so `seen == emitted` still means the document
/// contributed none. An embedded fragment marker instead makes `seen` exceed
/// `emitted` and costs a second pass, which renders the same document to the
/// same bytes - the rare price for a check that cannot be cancelled out.
pub(crate) fn without_placement_accounting<R>(render: impl FnOnce() -> R) -> R {
    let before = PLACEMENTS_EMITTED.with(Cell::get);
    let rendered = render();
    PLACEMENTS_EMITTED.with(|slot| slot.set(before));
    rendered
}

/// Write one placement marker, and record that this render wrote it.
fn push_footnotes_placement_marker(out: &mut String) {
    FOOTNOTES_PLACEMENT.with(|slot| out.push_str(&slot.borrow()));
    PLACEMENTS_EMITTED.with(|slot| slot.set(slot.get() + 1));
}

/// Relocate the endnotes section to the first `::: footnotes` marker; any
/// additional markers degrade to an empty placeholder so a second block never
/// duplicates the section.
fn place_footnotes_section(html: String, section: &str, marker: &str) -> String {
    let Some(pos) = html.find(marker) else {
        return html;
    };
    let mut out = String::with_capacity(html.len() + section.len());
    out.push_str(&html[..pos]);
    out.push_str(section);
    out.push_str(&html[pos + marker.len()..]);
    out.replace(marker, "<div class=\"footnotes\"></div>")
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
    without_placement_accounting(|| {
        let mut state = RenderState {
            heading_id_options: options.heading_id_options(),
            ..RenderState::default()
        };
        render_blocks(nodes, level, options, &mut state)
    })
}

// Render block nodes at `level`, continuing an existing `RenderState` so the
// shared heading-id counter keeps numbering across an extension boundary.
pub(crate) fn render_blocks_at_with_state(
    nodes: &[BlockNode],
    options: &Options<'_>,
    level: usize,
    state: &mut RenderState,
) -> String {
    without_placement_accounting(|| render_blocks(nodes, level, options, state))
}

pub(crate) fn render_blocks_with_state_from_depth(
    nodes: &[BlockNode],
    options: &Options<'_>,
    depth: usize,
    state: &mut RenderState,
) -> String {
    without_placement_accounting(|| {
        let previous = state.block_depth_bias;
        state.block_depth_bias = depth;
        let output = render_blocks(nodes, 0, options, state);
        state.block_depth_bias = previous;
        output
    })
}

fn render_blocks(
    nodes: &[BlockNode],
    level: usize,
    options: &Options<'_>,
    state: &mut RenderState,
) -> String {
    let mut out = String::new();
    let mut first = true;
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
        if !first {
            out.push('\n');
        }
        render_block(&mut out, block, level, options, state);
        first = false;
    }
    out
}

/// Render a raw block whose format the HTML target drops, reporting whether
/// `block` was one.
///
/// CARVE-P2-022 ties the payload's newline to the format matching the output
/// format: an `=html` payload "contributes its newline when the raw format
/// matches the output format", and anything else is DROPPED. Nothing reaches
/// the output, so no separating line is owed either - the same standing the
/// document's two root loops already give a comment or a hoisted definition.
/// The drop still has to be observable (CARVE-P2-024), so the block goes
/// through `render_block` into a sink rather than being skipped outright.
fn render_dropped_raw_block(
    block: &BlockNode,
    level: usize,
    options: &Options<'_>,
    state: &mut RenderState,
) -> bool {
    if !matches!(block, BlockNode::RawBlock(raw) if raw.format != "html" && raw.format != "html-document")
    {
        return false;
    }
    let mut sink = String::new();
    render_block(&mut sink, block, level, options, state);
    debug_assert!(sink.is_empty(), "a dropped raw block renders nothing");
    true
}

/// Append visible children, rolling back the separator when a child emits nothing.
fn render_children(
    out: &mut String,
    nodes: &[BlockNode],
    level: usize,
    options: &Options<'_>,
    state: &mut RenderState,
) -> bool {
    #[cfg(test)]
    CONTAINER_OUTPUT_BUFFERS.with(|buffers| {
        buffers.borrow_mut().insert(out as *const String as usize);
    });
    let mut visible = false;
    for child in nodes {
        let start = out.len();
        out.push('\n');
        let body_start = out.len();
        render_block(out, child, level, options, state);
        if out.len() == body_start {
            out.truncate(start);
        } else {
            visible = true;
        }
    }
    visible
}

#[derive(Default)]
pub(crate) struct RenderState {
    heading_counts: BTreeMap<String, usize>,
    crossref_index: crate::parse::CrossrefIndex,
    link_depth: usize,
    inline_depth: usize,
    block_depth_bias: usize,
    /// Mirrors the heading-id options so the `<section id>` derived
    /// here matches the parse-time id index (and the resolved cross-ref hrefs).
    heading_id_options: HeadingIdOptions,
    /// True while rendering the endnotes section's footnote bodies. A
    /// `::: footnotes` nested inside a footnote definition must NOT emit a
    /// placement marker (it renders as an ordinary div, matching carve-js).
    rendering_footnotes: bool,
    /// True only while the next block to render stands in the document's OWN
    /// block list. `CARVE-P9-073`: a placement marker places only there, so
    /// anything reached any other way sits inside a container. Heading
    /// `<section>` wrapping is a rendering artifact and keeps the flag, which
    /// is why this is not derived from the indent `level`.
    at_document_top_level: bool,
    admonition_count: usize,
    footnotes_heading: Option<FootnotesHeading>,
    has_footnotes: bool,
    suppress_automatic_abbreviation: bool,
}

#[derive(Clone)]
struct FootnotesHeading {
    title: Option<Vec<InlineNode>>,
    label: Option<String>,
    id: Option<String>,
}

impl RenderState {
    pub(crate) fn mint_admonition_id(&mut self) -> String {
        self.admonition_count += 1;
        crate::document_ids::unique_id(&format!("adm-{}", self.admonition_count))
    }
}

fn render_document_blocks(
    nodes: &[BlockNode],
    options: &Options<'_>,
    state: &mut RenderState,
    source_len: usize,
) -> String {
    // Markup normally expands the source. One up-front reservation avoids the
    // geometric realloc/copy ladder while keeping pathological expansion
    // bounded by String's ordinary growth behavior.
    let mut out = String::with_capacity(source_len.saturating_add(source_len / 2));
    let mut i = 0;
    let mut first = true;
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
        if render_dropped_raw_block(&nodes[i], 0, options, state) {
            i += 1;
            continue;
        }
        if !first {
            out.push('\n');
        }
        if matches!(nodes[i], BlockNode::Heading(_)) && options.sections {
            i = render_section(&mut out, nodes, i, 0, options, state);
        } else {
            state.at_document_top_level = true;
            render_block(&mut out, &nodes[i], 0, options, state);
            i += 1;
        }
        first = false;
    }
    out
}

mod footnotes;
use footnotes::*;
pub(crate) use footnotes::{collect_footnotes, used_footnote_labels, FootnoteEntry};
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
    write!(out, "<section id=\"{}\"", escape_attr(&section_id)).unwrap();
    // PART 10 §13a: presence says the id was AUTHORED rather than generated, so
    // an importer can put `{#id}` back instead of baking in a slug this render
    // derived. It rides the `<section>` where PART 9 §13 wraps one.
    if options.html_carriers() {
        let block = crate::render_loss::RawNodeType::Block;
        let pos = heading.pos.as_ref();
        if heading_id_is_authored(heading) {
            write_carrier(out, crate::roundtrip::EXPLICIT_ID, "1", &None, pos, block);
            // The id's slot among the heading's attributes, which the hoist
            // onto the wrapper loses. 0 is where a reader puts it anyway.
            let slot = heading
                .attrs
                .as_ref()
                .and_then(|attrs| attrs.order.iter().filter(|slot| !matches!(slot, AttrSlot::Key(name) if crate::roundtrip::canonical(name).is_some())).position(|s| matches!(s, AttrSlot::Id)))
                .unwrap_or(0);
            if slot > 0 {
                write_carrier(
                    out,
                    crate::roundtrip::ID_SLOT,
                    &slot.to_string(),
                    &None,
                    pos,
                    block,
                );
            }
        } else {
            // The id is generated, so an importer drops it without guessing.
            write_carrier(out, crate::roundtrip::GENERATED_ID, "1", &None, pos, block);
        }
    }
    out.push_str(">\n");
    render_heading_without_section_id(out, heading, level + 1, options, state);
    let mut i = start + 1;
    while i < nodes.len() {
        if matches!(&nodes[i], BlockNode::RawBlock(raw) if raw.format == "html-document") {
            break;
        }
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
        if render_dropped_raw_block(&nodes[i], level + 1, options, state) {
            i += 1;
            continue;
        }
        out.push('\n');
        // A section wrapper does not contain its blocks the way a container
        // does: they are still the document's own (`CARVE-P9-073`).
        state.at_document_top_level = true;
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
    if crate::abbr_budget::try_spend_label(label.len(), target) {
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
    // Taken here, so every nested render below reads false without each
    // container having to clear it.
    let at_document_top_level = std::mem::take(&mut state.at_document_top_level);
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
        BlockNode::CodeBlock(c) => render_code_block(out, c, level, options),
        BlockNode::List(l) => render_list(out, l, level, options, state),
        BlockNode::BlockQuote(b) => render_blockquote(out, b, level, options, state),
        BlockNode::Table(t) => render_table(out, t, level, options, state),
        BlockNode::Admonition(a) => render_admonition(out, a, level, options, state),
        BlockNode::Directive(d) => {
            render_directive(out, d, level, at_document_top_level, options, state)
        }
        BlockNode::Div(d) => render_div(out, d, level, options, state),
        BlockNode::Section(d) => {
            indent(out, level);
            out.push_str(&format!("<section{}>", render_attrs(&d.attrs)));
            let visible = render_children(out, &d.children, level + 1, options, state);
            close_block_container(out, level, "section", visible);
        }
        BlockNode::LineBlock(lb) => render_line_block(out, lb, level, options, state),
        BlockNode::DefinitionList(d) => render_definition_list(out, d, level, options, state),
        BlockNode::Figure(f) => render_figure(out, f, level, options, state),
        BlockNode::FigureGroup(g) => render_figure_group(out, g, level, options, state),
        BlockNode::AbbreviationDef(_) => {}
        BlockNode::RawBlock(r) => {
            if r.format == "html" || r.format == "html-document" {
                if options.allow_raw_html {
                    indent(out, level);
                    out.push_str(r.target_content());
                } else {
                    render_escaped_raw_block(out, r, level);
                }
            } else {
                crate::render_loss::record_raw_drop(
                    &r.format,
                    crate::RawNodeType::Block,
                    r.pos.clone(),
                );
            }
        }
        BlockNode::Comment(_) => {}
        // CARVE-P12-055: a core target renders the FALLBACK. The payload is not
        // Carve content and the extension's own renderer is not this engine's.
        BlockNode::BlockExtension(n) => render_block(out, &n.fallback, level, options, state),
        BlockNode::ExtensionCarrier(e) => render_extension_carrier(out, e, level, options, state),
        BlockNode::BlockImage(img) => {
            indent(out, level);
            render_image(out, img, options, crate::render_loss::RawNodeType::Block);
        }
        BlockNode::ThematicBreak(n) => {
            indent(out, level);
            out.push_str("<hr");
            // PART 10 §13a: the authored break character, under roundtrip
            // export only and only where it is not the default `-`.
            let carried = options
                .html_carriers()
                .then(|| n.marker.filter(|c| *c != '-'))
                .flatten();
            match carried {
                Some(marker) => {
                    write_carrier(
                        out,
                        crate::roundtrip::CHAR,
                        &marker.to_string(),
                        &n.attrs,
                        n.pos.as_ref(),
                        crate::render_loss::RawNodeType::Block,
                    );
                    out.push_str(&render_attrs_without_keys(
                        &n.attrs,
                        &[crate::roundtrip::CHAR],
                    ));
                }
                None => write_attrs(out, &n.attrs),
            }
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
    let id = next_heading_id(h, state);
    // AUTHORED means the id took a SLOT in an attribute block, not merely that
    // the node carries one. Since carve#750 the parse stamps a heading's
    // GENERATED id onto the node, so presence no longer distinguishes them - and
    // testing for it put the generated id in the authored run, ahead of
    // `data-source-line`, which section_wrapping.rs pins the other way round.
    let authored_id = heading_id_is_authored(h);
    indent(out, level);
    write!(out, "<h{}", h.level).unwrap();
    // PART 10 §13a: with no `<section>` to ride, the carrier rides the heading.
    // Structural, so it leads the authored run (§1) - the authored id keeps its
    // own slot inside that run and there is no position "after the id" to take.
    let carried_explicit_id = options.html_carriers() && authored_id;
    if carried_explicit_id {
        write_carrier(
            out,
            crate::roundtrip::EXPLICIT_ID,
            "1",
            &h.attrs,
            h.pos.as_ref(),
            crate::render_loss::RawNodeType::Block,
        );
    }
    if options.html_carriers() && !authored_id {
        write_carrier(
            out,
            crate::roundtrip::GENERATED_ID,
            "1",
            &None,
            h.pos.as_ref(),
            crate::render_loss::RawNodeType::Block,
        );
    }
    // The carrier owns the name, so an authored copy does not reach the element.
    let own_attrs = if carried_explicit_id {
        strip_keys(&h.attrs, &[crate::roundtrip::EXPLICIT_ID])
    } else {
        h.attrs.clone()
    };
    if authored_id {
        // Render through the normal attribute walk so the id lands in its
        // authored slot. The resolved id is the authored one (an explicit
        // heading id wins verbatim), but write it back rather than assume so.
        let mut attrs = own_attrs;
        if let Some(attrs) = &mut attrs {
            attrs.id = Some(id.clone());
        }
        write_attrs_for(out, &attrs, &heading_tag(h.level));
    } else {
        // `data-source-line` is a RENDER annotation, not something the author
        // wrote, and it is emitted last - so the generated id goes before it,
        // not after. carve-rs stamps it as an ordinary key-value at parse time,
        // which would otherwise carry it along in the authored run and put the
        // id behind it (`tests/source_lines.rs` catches exactly that).
        let mut authored = own_attrs;
        let stamp = authored.as_mut().and_then(take_source_line_attr);
        out.push_str(&render_attrs_without_id_for(
            &authored,
            &heading_tag(h.level),
        ));
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
    write_attrs_for(out, &attrs, &heading_tag(h.level));
    out.push('>');
    render_inlines(out, &h.children, options, state);
    write!(out, "</h{}>", h.level).unwrap();
}

/// Whether the heading's id took a SLOT in an attribute block.
///
/// AUTHORED means exactly that, not merely that the node carries an id: since
/// carve#750 the parse stamps a heading's GENERATED id onto the node, so
/// presence no longer distinguishes them.
fn heading_id_is_authored(h: &Heading) -> bool {
    h.attrs.as_ref().is_some_and(|attrs| {
        attrs.id.is_some() && attrs.order.iter().any(|slot| matches!(slot, AttrSlot::Id))
    })
}

fn next_heading_id(h: &Heading, state: &mut RenderState) -> String {
    let explicit = h.attrs.as_ref().and_then(|attrs| attrs.id.clone());
    let has_explicit = explicit.is_some();
    let base =
        explicit.unwrap_or_else(|| slugify(&plain_inlines(&h.children), state.heading_id_options));
    crate::document_ids::allocate_heading_id(
        base,
        has_explicit,
        &mut state.heading_counts,
        crate::document_ids::is_explicit_id,
    )
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
    for node in nodes {
        match node {
            InlineNode::NonBreakingSpace(_) => out.push('\u{00a0}'),
            InlineNode::Text(s) => out.push_str(&s.value),
            // Visible prose, so it feeds derived text (carve-rs#800). This has to
            // move together with `plain_inlines_parse`'s arm: the parse-time
            // index and the render-time id are two spellings of one derivation,
            // and a heading whose id disagreed between them would publish an
            // anchor no cross-reference could resolve.
            InlineNode::EscapedText(e) => out.push_str(&e.value),
            InlineNode::SmartPunctuation(s) => {
                if smart.uses_source(&s.kind) {
                    out.push_str(&s.value);
                } else {
                    out.push_str(smart_punctuation_glyph(s));
                }
            }
            InlineNode::Emphasis(e) => {
                out.push_str(&plain_inlines_typography_at(&e.children, smart, depth + 1))
            }
            // PART 9R R4: a derived text is taken before any render-stage
            // injection, and this engine builds it after the hooks have run, so
            // the `section-number` span has to be skipped here.
            InlineNode::Span(s) if !s.injected => {
                out.push_str(&plain_inlines_typography_at(&s.children, smart, depth + 1))
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
            InlineNode::Ruby(r) => {
                for pair in &r.pairs {
                    out.push_str(&plain_inlines_typography_at(&pair.base, smart, depth + 1));
                    out.push('(');
                    out.push_str(&plain_inlines_typography_at(
                        &pair.annotation,
                        smart,
                        depth + 1,
                    ));
                    out.push(')');
                }
            }
            InlineNode::AutoLink(a) => out.push_str(&a.text),
            InlineNode::Image(i) => out.push_str(&i.alt),
            InlineNode::Extension(e) => {
                out.push_str(&plain_inlines_typography_at(&e.children, smart, depth + 1))
            }
            // CARVE-P2-004: editorial text is carried text. Mirrors `plain_inlines_parse`.
            InlineNode::CriticInsert(c) => {
                out.push_str(&plain_inlines_typography_at(&c.children, smart, depth + 1))
            }
            InlineNode::CriticDelete(c) => {
                out.push_str(&plain_inlines_typography_at(&c.children, smart, depth + 1))
            }
            InlineNode::CriticSubstitute(c) => {
                out.push_str(&plain_inlines_typography_at(&c.old, smart, depth + 1));
                out.push_str(&plain_inlines_typography_at(&c.new, smart, depth + 1));
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
            InlineNode::SoftBreak(_) | InlineNode::HardBreak(_) | InlineNode::CellWrap(_) => {
                out.push(' ')
            }
            _ => {}
        }
    }
    out
}

fn slugify(text: &str, id_opts: HeadingIdOptions) -> String {
    // Delegate to the single canonical implementation so HTML, Markdown, and
    // the parser's id index never drift apart (or from carve-js / carve-php).
    crate::parse::slugify_parse(text, id_opts)
}

fn render_paragraph(
    out: &mut String,
    p: &Paragraph,
    level: usize,
    options: &Options<'_>,
    state: &mut RenderState,
) {
    if p.block_image {
        if let Some(index) = crate::parse::lone_rendered_image(&p.children) {
            if let InlineNode::Image(image) = &p.children[index] {
                let mut image = image.clone();
                if let Some(attrs) = &p.attrs {
                    crate::parse::merge_leading_attrs(&mut image.attrs, attrs.clone());
                }
                indent(out, level);
                render_image(out, &image, options, crate::render_loss::RawNodeType::Block);
                return;
            }
        }
    }
    indent(out, level);
    out.push_str("<p");
    write_attrs_for(out, &p.attrs, "p");
    out.push('>');
    render_inlines(out, &p.children, options, state);
    out.push_str("</p>");
}

fn render_code_block(out: &mut String, c: &CodeBlock, level: usize, options: &Options) {
    indent(out, level);
    out.push_str("<pre");
    let label = c.label.as_ref().filter(|_| options.html_carriers());
    let structural_title = label.and_then(|_| {
        c.attrs
            .as_ref()
            .and_then(|attrs| {
                attrs
                    .key_values
                    .iter()
                    .find(|(key, _)| key.eq_ignore_ascii_case("title"))
                    .map(|(_, value)| value)
            })
            .or(c.title.as_ref())
    });
    if let Some(title) = structural_title {
        write_attr_key_value(out, "title", title);
    } else if let Some(title) = &c.title {
        if !attrs_has_key(&c.attrs, "title") {
            write_attr_key_value(out, "title", title);
        }
    }
    if let Some(label) = label {
        write_carrier(
            out,
            crate::roundtrip::LABEL,
            label,
            &c.attrs,
            c.pos.as_ref(),
            crate::render_loss::RawNodeType::Block,
        );
    }
    let attrs = if label.is_some() {
        strip_keys(
            &c.attrs,
            if structural_title.is_some() {
                &[crate::roundtrip::LABEL, "title"]
            } else {
                &[crate::roundtrip::LABEL]
            },
        )
    } else {
        c.attrs.clone()
    };
    write_attrs(out, &attrs);
    out.push_str("><code");
    if let Some(lang) = &c.lang {
        out.push_str(" class=\"language-");
        out.push_str(lang);
        out.push('"');
    }
    out.push('>');
    write_escaped_text_nbsp(out, &c.content);
    out.push_str("</code></pre>");
}

/// A raw block a safe policy escapes is shown as source: the PART 10 §6 fenced
/// code block with the raw format as its language (markup-carve/carve#2795).
fn render_escaped_raw_block(out: &mut String, r: &RawBlock, level: usize) {
    let content = r.content.clone();
    let code = CodeBlock {
        attrs: r.attrs.clone(),
        lang: Some(if r.format == "html-document" {
            "html".into()
        } else {
            r.format.clone()
        }),
        title: None,
        label: None,
        content,
        pos: None,
    };
    render_code_block(out, &code, level, &Options::default());
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
        // 1 IS THE DEFAULT, SO IT IS NOT AN ATTRIBUTE. The parser never sets
        // `start` to 1 -- `resources/ast-schema.json` documents the field as
        // "the first number of an ordered list, WHEN IT IS NOT 1" -- so the
        // only way to get here with 1 is an ingested AST that spells the
        // default out. Writing `start="1"` then made one JSON payload two
        // different documents depending on the output asked for: the HTML
        // carried the attribute while `--carve` wrote a plain `1.` marker,
        // which reads back without it.
        if let Some(start) = l.start.filter(|start| *start != 1) {
            write!(out, " start=\"{start}\"").unwrap();
        }
        // PART 10 §12: the authored delimiter, which HTML has no attribute of
        // its own for, trailing `type` and `start` and only where it is `)`.
        if let Some(delim) = l.delim.filter(|delim| *delim == ')') {
            write!(out, " data-delim=\"{}\"", escape_attr(&delim.to_string())).unwrap();
        }
    }
    // PART 10 §13a: the authored bullet, under roundtrip export only and only
    // where it is not the default `-`. An ORDERED list's authored marker is its
    // DELIMITER, which §12's `data-delim` already writes on every render, and
    // §13a says this option "neither gates nor duplicates" a clause of its own -
    // so `data-marker` rides the `<ul>` here and the `<ol>` keeps `data-delim`.
    // carve-php writes `data-marker=")"` on an `<ol>` and no `data-delim` at
    // all, which is its §12 gap rather than a second spelling to match.
    let bullet_carrier = options
        .html_carriers()
        .then(|| l.bullet_char.filter(|c| *c != '-'))
        .flatten();
    if let Some(bullet) = bullet_carrier {
        write_carrier(
            out,
            crate::roundtrip::MARKER,
            &bullet.to_string(),
            &l.attrs,
            l.pos.as_ref(),
            crate::render_loss::RawNodeType::Block,
        );
    }
    // PART 10 §13a: a tight list whose item paragraph keeps its `<p>` for its
    // own attributes reads back as loose without this.
    let tight_carrier = options.html_carriers()
        && l.tight
        && l.items.iter().any(|item| {
            item.children.iter().any(
                |child| matches!(child, BlockNode::Paragraph(p) if has_authored_attrs(&p.attrs)),
            )
        });
    if tight_carrier {
        write_carrier(
            out,
            crate::roundtrip::TIGHT,
            "1",
            &l.attrs,
            l.pos.as_ref(),
            crate::render_loss::RawNodeType::Block,
        );
    }
    // PART 19 §1: `task-list` is a mandatory base class. Every item has to be a
    // task, so an ingested mixed list never hides a plain item's bullet.
    let task_list =
        !l.ordered && !l.items.is_empty() && l.items.iter().all(|item| item.checked.is_some());
    // A carrier owns its name, so an authored copy does not reach the element.
    let mut owned: Vec<&str> = Vec::new();
    // A derived `type`, `start` or `data-delim` wins over an authored copy (PART 10 R1).
    if l.ordered {
        if l.ol_type.is_some() {
            owned.push("type");
        }
        if l.start.is_some_and(|start| start != 1) {
            owned.push("start");
        }
        if l.delim == Some(')') {
            owned.push("data-delim");
        }
    }
    if bullet_carrier.is_some() {
        owned.push(crate::roundtrip::MARKER);
    }
    if tight_carrier {
        owned.push(crate::roundtrip::TIGHT);
    }
    let authored = if owned.is_empty() {
        l.attrs.clone()
    } else {
        strip_keys(&l.attrs, &owned)
    };
    if task_list {
        out.push_str(&render_attrs_with_base_class(&authored, "task-list"));
    } else {
        write_attrs(out, &authored);
    }
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
    #[cfg(test)]
    LIST_OUTPUT_BUFFERS.with(|buffers| {
        buffers.borrow_mut().insert(out as *const String as usize);
    });
    indent(out, level);
    out.push_str("<li");
    // PART 10 §11. Structural, so it leads the authored attributes (§1). A done
    // item writes `x` whichever case it was authored in (carve#2887).
    let task_state = match item.checked {
        Some(true) => Some('x'),
        _ => item.task_state,
    };
    if let Some(task_state) = task_state {
        out.push_str(&format!(
            " data-task-state=\"{}\"",
            escape_attr(&task_state.to_string())
        ));
        // An authored copy would be a second attribute of the same name.
        out.push_str(&render_attrs_without_keys(
            &item.attrs,
            &["data-task-state"],
        ));
    } else {
        write_attrs(out, &item.attrs);
    }
    out.push('>');
    let task_name = if item.checked.is_some() {
        match item
            .children
            .iter()
            .find(|child| !matches!(child, BlockNode::Comment(_)))
        {
            Some(BlockNode::Paragraph(p)) => {
                plain_inlines_typography(&p.children, options.smart_typography)
                    .split_ascii_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
            }
            _ => String::new(),
        }
    } else {
        String::new()
    };
    let name_attr = if task_name.is_empty() {
        String::new()
    } else {
        format!(" aria-label=\"{}\"", escape_attr(&task_name))
    };
    let checkbox = match item.checked {
        None => String::new(),
        Some(false) => format!("<input type=\"checkbox\" disabled{name_attr}> "),
        Some(true) => format!("<input type=\"checkbox\" checked disabled{name_attr}> "),
    };
    if item.children.is_empty() {
        out.push_str(&checkbox);
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
    // The checkbox belongs to the item, including a block-first item.
    out.push_str(&checkbox);
    let mut visible = 0;
    let mut first_inline = false;
    for child in &item.children {
        if let BlockNode::Paragraph(p) = child {
            let mut html = String::new();
            render_inlines(&mut html, &p.children, options, state);
            // Authored paragraph attributes require their own wrapper even in a tight item.
            if !tight || has_authored_attrs(&p.attrs) {
                html = format!("<p{}>{html}</p>", render_attrs_for(&p.attrs, "p"));
            }
            // Whitespace is content; only an empty rendering removes the slot.
            if html.is_empty() {
                continue;
            }
            if visible == 0 {
                first_inline = true;
            } else {
                out.push('\n');
                indent(out, level + 1);
            }
            out.push_str(&html);
        } else {
            // Append nested blocks directly so ancestors do not copy their HTML.
            // Roll back the separator when a block emits nothing.
            let start = out.len();
            out.push('\n');
            let body_start = out.len();
            render_block(out, child, level + 1, options, state);
            if out.len() == body_start {
                out.truncate(start);
                continue;
            }
        }
        visible += 1;
    }
    if visible == 0 || (visible == 1 && first_inline) {
        out.push_str("</li>");
        return;
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
    #[cfg(test)]
    QUOTE_OUTPUT_BUFFERS.with(|buffers| {
        buffers.borrow_mut().insert(out as *const String as usize);
    });
    indent(out, level);
    // CARVE-P10-012: a comment or a raw block for another target contributes no
    // slot, so it does not push a lone paragraph out of the compact form.
    let mut visible = b.children.iter().filter(|child| !renders_nothing(child));
    if let (Some(BlockNode::Paragraph(_)), None) = (visible.next(), visible.next()) {
        out.push_str("<blockquote");
        write_attrs(out, &b.attrs);
        for child in &b.children {
            let BlockNode::Paragraph(p) = child else {
                // Rendered anyway for its side effects, such as a render-loss row.
                render_block(out, child, level + 1, options, state);
                continue;
            };
            out.push_str("><p");
            write_attrs(out, &p.attrs);
            out.push('>');
            render_inlines(out, &p.children, options, state);
            out.push_str("</p></blockquote>");
        }
        return;
    }
    out.push_str("<blockquote");
    write_attrs(out, &b.attrs);
    out.push_str(">\n");
    let mut first = true;
    for child in &b.children {
        let start = out.len();
        if !first {
            out.push('\n');
        }
        let body_start = out.len();
        render_block(out, child, level + 1, options, state);
        if out.len() == body_start {
            out.truncate(start);
            continue;
        }
        first = false;
    }
    out.push('\n');
    indent(out, level);
    out.push_str("</blockquote>");
}

/// The block kinds `render_block` writes no HTML for. Keep in step with its arms.
fn renders_nothing(node: &BlockNode) -> bool {
    match node {
        BlockNode::Comment(_)
        | BlockNode::LinkReferenceDefinition(_)
        | BlockNode::CitationDefinition(_)
        | BlockNode::AbbreviationDef(_) => true,
        BlockNode::RawBlock(r) => r.format != "html" && r.format != "html-document",
        BlockNode::BlockExtension(n) => renders_nothing(&n.fallback),
        _ => false,
    }
}

fn render_table(
    out: &mut String,
    t: &Table,
    level: usize,
    options: &Options<'_>,
    state: &mut RenderState,
) {
    let mut resolved = std::borrow::Cow::Borrowed(t);
    if t.columns.is_empty() {
        let columns = columns_from_attrs(t.attrs.as_ref());
        if !columns.is_empty() {
            resolved.to_mut().columns = columns;
        }
    }
    if let Some(groups) = &t.row_groups {
        if groups.head_attrs.is_some()
            || groups.foot_attrs.is_some()
            || groups.bodies.iter().any(|b| b.attrs.is_some())
        {
            let mut end = groups.head_rows;
            let mut boundaries = vec![end];
            for body in &groups.bodies {
                end += body.head_rows + body.body_rows;
                boundaries.push(end);
            }
            for boundary in boundaries {
                if t.rows.get(boundary).is_some_and(|row| {
                    row.cells
                        .iter()
                        .any(|cell| cell.span == Some(TableCellSpan::Rowspan))
                }) {
                    for cell in &mut resolved.to_mut().rows[boundary].cells {
                        if cell.span == Some(TableCellSpan::Rowspan) {
                            cell.span = None;
                            cell.children.clear();
                            cell.blocks = None;
                        }
                    }
                }
            }
        }
    }
    let t = resolved.as_ref();
    indent(out, level);
    out.push_str("<table");
    // PART 10 §13a: the authored separator row's dash runs, under roundtrip
    // export only. PRESENCE is the carried half - a `|=` header and a separator
    // row give the same `<table>` otherwise.
    // A carrier a conforming reader will refuse is a round-trip promise the
    // writer cannot keep, so an entry past the §13a bound writes no carrier.
    let widths_carrier = options
        .html_carriers()
        .then_some(t.separator_widths.as_ref())
        .flatten()
        .filter(|widths| {
            !widths.is_empty()
                && widths
                    .iter()
                    .all(|w| (1..=crate::roundtrip::SEPARATOR_WIDTH_LIMIT).contains(w))
        });
    if let Some(widths) = widths_carrier {
        let joined = widths
            .iter()
            .map(|w| w.to_string())
            .collect::<Vec<_>>()
            .join(",");
        write_carrier(
            out,
            crate::roundtrip::COL_WIDTHS,
            &joined,
            &t.attrs,
            t.pos.as_ref(),
            crate::render_loss::RawNodeType::Block,
        );
    }
    let mut table_attrs = t.attrs.clone();
    if widths_carrier.is_some() {
        // The carrier owns the name, so an authored copy does not reach it.
        table_attrs = strip_keys(&table_attrs, &[crate::roundtrip::COL_WIDTHS]);
    }
    if let Some(attrs) = &mut table_attrs {
        let has_body_metadata = ["body-rows", "body-header-rows", "body-header-cols"]
            .iter()
            .any(|key| attrs.key_values.contains_key(*key));
        let valid_groups = crate::table_source_metadata::row_groups(attrs, t.rows.len()).is_some();
        if valid_groups {
            attrs.key_values.retain(|key, _| {
                !matches!(
                    key.as_str(),
                    "body-rows" | "body-header-rows" | "body-header-cols"
                )
            });
            attrs.order.retain(|slot| !matches!(slot, AttrSlot::Key(key) if matches!(key.as_str(), "body-rows" | "body-header-rows" | "body-header-cols")));
        }
        attrs
            .key_values
            .retain(|k, _| !matches!(k.as_str(), "aligns" | "valigns" | "widths"));
        attrs.order.retain(|s| !matches!(s, AttrSlot::Key(k) if matches!(k.as_str(), "aligns" | "valigns" | "widths")));
        if !has_body_metadata || valid_groups {
            attrs
                .key_values
                .retain(|key, _| !matches!(key.as_str(), "header-rows" | "footer-rows"));
            attrs.order.retain(|slot| !matches!(slot, AttrSlot::Key(key) if matches!(key.as_str(), "header-rows" | "footer-rows")));
        }
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
                write!(
                    out,
                    " style=\"width: {}%;\"",
                    crate::table_width::percentage(width)
                )
                .unwrap();
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
    let span_plan = TableSpanPlan::new(t);
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
            let resolved = |i: usize| span_plan.entry(*r, i).skip;
            (0..row.cells.len()).any(|i| !resolved(i))
                && row
                    .cells
                    .iter()
                    .enumerate()
                    .all(|(i, cell)| cell.header || resolved(i))
        })
        .count();
    let invalid_source_partition = t.row_groups.is_none()
        && t.attrs.as_ref().is_some_and(|attrs| {
            [
                "header-rows",
                "footer-rows",
                "body-rows",
                "body-header-rows",
                "body-header-cols",
            ]
            .iter()
            .any(|key| attrs.key_values.contains_key(*key))
                && crate::table_source_metadata::row_groups(attrs, t.rows.len()).is_none()
        });
    let header_count = if invalid_source_partition {
        0
    } else {
        t.row_groups
            .as_ref()
            .map_or(derived_header_count, |groups| groups.head_rows)
    };
    let footer_count = t.row_groups.as_ref().map_or(0, |groups| groups.foot_rows);
    let footer_start = t.rows.len() - footer_count;
    let mut section_ends = vec![false; t.rows.len() + 1];
    for end in [header_count, footer_start] {
        if let Some(boundary) = section_ends.get_mut(end) {
            *boundary = true;
        }
    }
    let mut section_end = header_count;
    if let Some(groups) = &t.row_groups {
        for body in &groups.bodies {
            section_end += body.head_rows + body.body_rows;
            if let Some(boundary) = section_ends.get_mut(section_end) {
                *boundary = true;
            }
        }
    }
    let mut next_boundary = usize::MAX;
    let crosses_section = (0..t.rows.len()).rev().any(|row| {
        if section_ends[row + 1] {
            next_boundary = row + 1;
        }
        span_plan
            .row(row)
            .iter()
            .any(|entry| !entry.skip && entry.rowspan > 1 && row + entry.rowspan > next_boundary)
    });
    // Computed once per table: every row and every cell reads the same answer.
    let column_defaults = table_column_defaults(t, header_count);
    let mut row_contexts: Vec<_> = (0..t.rows.len())
        .map(|row| (row < header_count, 0))
        .collect();
    if let Some(groups) = &t.row_groups {
        let mut start = groups.head_rows;
        for body in &groups.bodies {
            let end = start + body.head_rows + body.body_rows;
            for (row, context) in row_contexts.iter_mut().enumerate().take(end).skip(start) {
                *context = (
                    row < start + body.head_rows,
                    body.row_head_columns.unwrap_or(0),
                );
            }
            start = end;
        }
    }
    if crosses_section {
        out.push('\n');
        indent(out, level + 1);
        out.push_str("<tbody>");
        let mut body_ctx = TableBodyRenderContext {
            row_head_columns: 0,
            spans: &span_plan,
            defaults: &column_defaults,
            options,
            state,
        };
        for (row_idx, row) in t.rows.iter().enumerate() {
            out.push('\n');
            indent(out, level + 2);
            body_ctx.row_head_columns = row_contexts[row_idx].1;
            if row_contexts[row_idx].0 {
                render_table_row(
                    out,
                    row,
                    true,
                    body_ctx.options,
                    row_idx,
                    &span_plan,
                    body_ctx.state,
                    &column_defaults,
                );
            } else {
                render_table_body_row(out, row, row_idx, &mut body_ctx);
            }
        }
        out.push('\n');
        indent(out, level + 1);
        out.push_str("</tbody>");
        out.push('\n');
        indent(out, level);
        out.push_str("</table>");
        return;
    }
    let has_header = header_count > 0;
    let body_start = header_count;
    // A ROW IS A ROW, IN EVERY SECTION (PART 10 §7, markup-carve/carve#1459).
    // `thead` and `tfoot` used to put their rows on the section's own line while
    // `tbody` gave each row a line, and nothing said why one element had two
    // layouts - which is how two corpus fixtures came to demand different
    // `tfoot` shapes with no rule to measure either against.
    if has_header
        || t.row_groups
            .as_ref()
            .is_some_and(|g| g.head_attrs.is_some())
    {
        out.push('\n');
        indent(out, level + 1);
        out.push_str("<thead");
        if let Some(g) = &t.row_groups {
            write_attrs(out, &g.head_attrs);
        }
        out.push('>');
        for (row_idx, header) in t.rows[..header_count].iter().enumerate() {
            out.push('\n');
            indent(out, level + 2);
            render_table_row(
                out,
                header,
                true,
                options,
                row_idx,
                &span_plan,
                state,
                &column_defaults,
            );
        }
        out.push('\n');
        indent(out, level + 1);
        out.push_str("</thead>");
    }
    // A header-only table (e.g. a GFM `| x |` + `|---|` with no body rows) emits
    // no <tbody>, matching carve-php.
    let fallback_bodies = if body_start < footer_start {
        vec![TableBodyGroup {
            head_rows: 0,
            body_rows: footer_start - body_start,
            row_head_columns: None,
            attrs: None,
        }]
    } else {
        vec![]
    };
    let bodies = t
        .row_groups
        .as_ref()
        .map_or(fallback_bodies.as_slice(), |g| g.bodies.as_slice());
    let mut body_start = body_start;
    for body in bodies {
        let body_end = body_start + body.head_rows + body.body_rows;
        out.push('\n');
        indent(out, level + 1);
        out.push_str("<tbody");
        write_attrs(out, &body.attrs);
        out.push('>');
        let mut body_ctx = TableBodyRenderContext {
            row_head_columns: body.row_head_columns.unwrap_or(0),
            spans: &span_plan,
            defaults: &column_defaults,
            options,
            state,
        };
        for (row_idx, row) in t.rows.iter().enumerate().take(body_end).skip(body_start) {
            out.push('\n');
            indent(out, level + 2);
            if row_idx < body_start + body.head_rows {
                render_table_row(
                    out,
                    row,
                    true,
                    body_ctx.options,
                    row_idx,
                    &span_plan,
                    body_ctx.state,
                    &column_defaults,
                );
            } else {
                render_table_body_row(out, row, row_idx, &mut body_ctx);
            }
        }
        out.push('\n');
        indent(out, level + 1);
        out.push_str("</tbody>");
        body_start = body_end;
    }
    if footer_start < t.rows.len()
        || t.row_groups
            .as_ref()
            .is_some_and(|g| g.foot_attrs.is_some())
    {
        out.push('\n');
        indent(out, level + 1);
        out.push_str("<tfoot");
        if let Some(g) = &t.row_groups {
            write_attrs(out, &g.foot_attrs);
        }
        out.push('>');
        let mut foot_ctx = TableBodyRenderContext {
            row_head_columns: 0,
            spans: &span_plan,
            defaults: &column_defaults,
            options,
            state,
        };
        for (row_idx, row) in t.rows.iter().enumerate().skip(footer_start) {
            out.push('\n');
            indent(out, level + 2);
            render_table_body_row(out, row, row_idx, &mut foot_ctx);
        }
        out.push('\n');
        indent(out, level + 1);
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
            width: widths.get(i).and_then(|v| crate::table_width::fraction(v)),
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
    spans: &TableSpanPlan,
    state: &mut RenderState,
    defaults: &ColumnDefaults,
) {
    out.push_str("<tr");
    write_attrs(out, &row.attrs);
    out.push('>');
    // A head row resolves its continuations the same way a body row does. It
    // used to resolve neither: a `^` rendered an empty `<th>` BESIDE the
    // `rowspan` its origin already carried, and a `<` rendered an empty `<th>`
    // instead of widening the cell to its left - so a header cell spanning
    // columns lost the span and gained a column the table does not have.
    for (col, cell) in row.cells.iter().enumerate() {
        let entry = spans.entry(row_idx, col);
        if entry.skip {
            continue;
        }
        let tag = if in_head || cell.header { "th" } else { "td" };
        let mut extra = String::new();
        let mut emitted: Vec<&str> = Vec::new();
        // A header cell can carry a rowspan that extends down into the body
        // (a `^` below it), so the header row emits it too -- not just bodies.
        if entry.rowspan > 1 {
            extra.push_str(&format!(" rowspan=\"{}\"", entry.rowspan));
            emitted.push("rowspan");
        }
        if entry.colspan > 1 {
            extra.push_str(&format!(" colspan=\"{}\"", entry.colspan));
            emitted.push("colspan");
        }
        let (cell_attrs, style) = merge_cell_style(
            &cell.attrs,
            table_cell_style(cell, col, row_align(row, col), defaults),
        );
        if !style.is_empty() {
            emitted.push("style");
        }
        out.push('<');
        out.push_str(tag);
        out.push_str(&cell_scope_attr(cell, tag == "th", in_head));
        out.push_str(&extra);
        out.push_str(&render_cell_author_attrs(&cell_attrs, &emitted));
        out.push_str(&style);
        out.push('>');
        if let Some(blocks) = &cell.blocks {
            out.push_str(&render_blocks(blocks, 0, options, state));
        } else {
            render_inlines(
                out,
                crate::ast::visible_table_cell_children(&cell.children),
                options,
                state,
            );
        }
        out.push_str("</");
        out.push_str(tag);
        out.push('>');
    }
    out.push_str("</tr>");
}

struct TableBodyRenderContext<'a, 'b> {
    spans: &'a TableSpanPlan,
    /// The column defaults declared by the head, computed once per table.
    defaults: &'a ColumnDefaults,
    options: &'a Options<'a>,
    state: &'b mut RenderState,
    row_head_columns: usize,
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
    for (cell_index, cell) in row.cells.iter().enumerate() {
        let is_header = cell.header || cell_index < ctx.row_head_columns;
        // A marker absorbed into another cell renders nothing; one with no
        // origin, or blocked, renders as an EMPTY cell of its own (T5).
        let entry = ctx.spans.entry(source_row_idx, cell_index);
        if entry.skip {
            continue;
        }
        let mut attrs = String::new();
        let mut emitted: Vec<&str> = Vec::new();
        if entry.rowspan > 1 {
            attrs.push_str(&format!(" rowspan=\"{}\"", entry.rowspan));
            emitted.push("rowspan");
        }
        if entry.colspan > 1 {
            attrs.push_str(&format!(" colspan=\"{}\"", entry.colspan));
            emitted.push("colspan");
        }
        let (cell_attrs, style) = merge_cell_style(
            &cell.attrs,
            table_cell_style(
                cell,
                cell_index,
                table_column_align(ctx.defaults, cell_index),
                ctx.defaults,
            ),
        );
        if !style.is_empty() {
            emitted.push("style");
        }
        // A `|=` cell in a body row is a row header: <th> inside <tbody>.
        let tag = if is_header { "th" } else { "td" };
        out.push('<');
        out.push_str(tag);
        // Below the header run, so a header cell here heads its ROW.
        out.push_str(&cell_scope_attr(cell, is_header, false));
        out.push_str(&attrs);
        out.push_str(&render_cell_author_attrs(&cell_attrs, &emitted));
        out.push_str(&style);
        out.push('>');
        if let Some(blocks) = &cell.blocks {
            out.push_str(&render_blocks(blocks, 0, ctx.options, ctx.state));
        } else {
            render_inlines(
                out,
                crate::ast::visible_table_cell_children(&cell.children),
                ctx.options,
                ctx.state,
            );
        }
        out.push_str("</");
        out.push_str(tag);
        out.push('>');
    }
    out.push_str("</tr>");
}

fn row_align(row: &TableRow, col: usize) -> Option<TableAlign> {
    row.cells.get(col).and_then(|c| c.align)
}

/// A column's declared defaults, read off the HEADER SECTION.
///
/// Column defaults come from the header rows: with several of them the last row
/// that declares an axis wins, and omission does not reset it. A HEADERLESS
/// table has no column default at all - a `|=` cell below the header run is a
/// ROW header (§5 T9), so it heads its row rather than its column, and its
/// alignment is its own.
///
/// This used to read row 0 unconditionally, which is the same answer whenever
/// row 0 is a header row and a different one whenever it is not. Nothing could
/// see the difference until §5 T11 made a rejected marker run demote a row that
/// used to be all-header: corpus 373 then aligned a BODY cell from a row header
/// above it, where carve-js and the spec's own oracle align nothing.
/// ONE SCAN PER TABLE, not one per cell. Asked per cell this walks the whole
/// header section for every cell in the table, which is quadratic in the header
/// row count on an all-header table and re-reads the head once per body cell.
/// The defaults do not depend on the cell, so they are computed once and
/// carried.
type ColumnDefaults = Vec<(Option<TableAlign>, Option<TableVerticalAlign>)>;

fn table_column_defaults(table: &Table, header_count: usize) -> ColumnDefaults {
    let width = table
        .rows
        .iter()
        .map(|row| row.cells.len())
        .max()
        .unwrap_or(0)
        .max(table.columns.len());
    let mut defaults: ColumnDefaults = vec![(None, None); width];
    for row in table.rows.iter().take(header_count) {
        for (col, cell) in row.cells.iter().enumerate() {
            // HTML row-group promotion does not widen a plain cell marker
            // into a column default. Native and GFM header cells do.
            if !cell.header {
                continue;
            }
            let Some(slot) = defaults.get_mut(col) else {
                continue;
            };
            if cell.align.is_some() {
                slot.0 = cell.align;
            }
            if cell.valign.is_some() {
                slot.1 = cell.valign;
            }
        }
    }
    for (col, slot) in defaults.iter_mut().enumerate() {
        let column = table.columns.get(col);
        slot.0 = slot.0.or_else(|| column.and_then(|c| c.align));
        slot.1 = slot.1.or_else(|| column.and_then(|c| c.valign));
    }
    defaults
}

fn table_column_align(defaults: &ColumnDefaults, col: usize) -> Option<TableAlign> {
    defaults.get(col).and_then(|slot| slot.0)
}

fn table_cell_style(
    cell: &TableCell,
    col: usize,
    inherited_align: Option<TableAlign>,
    defaults: &ColumnDefaults,
) -> String {
    let slot = defaults.get(col).copied().unwrap_or((None, None));
    let align = cell.align.or(inherited_align).or(slot.0);
    let valign = cell.valign.or(slot.1);
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

/// An authored `style` keeps its slot and takes the cell's alignment
/// declarations, as on a text block (PART 10 R1).
fn merge_cell_style(attrs: &Option<Attrs>, style: String) -> (Option<Attrs>, String) {
    let Some(key) = attrs.as_ref().and_then(|a| {
        a.key_values
            .keys()
            .find(|key| key.eq_ignore_ascii_case("style"))
            .cloned()
    }) else {
        return (attrs.clone(), style);
    };
    let Some(declarations) = style
        .strip_prefix(" style=\"")
        .and_then(|rest| rest.strip_suffix('"'))
    else {
        return (attrs.clone(), style);
    };
    let mut merged = attrs.clone();
    if let Some(a) = &mut merged {
        let value = append_declaration(&a.key_values[&key], declarations);
        a.key_values.insert(key, value);
    }
    (merged, String::new())
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

/// A `::: footnotes` placement directive, or any other generated-content
/// container (CARVE-P12-057).
fn render_directive(
    out: &mut String,
    d: &Directive,
    level: usize,
    at_document_top_level: bool,
    options: &Options<'_>,
    state: &mut RenderState,
) {
    // Emit the marker that the top-level render replaces with the endnotes
    // section, relocating it from the document end. A document without this
    // block is byte-identical to before.
    // Only a marker in a document that HAS a note places the section, and only
    // one standing at the document's own top level (`CARVE-P9-073`). Any other
    // one falls through below and renders as the ordinary `<div class="{kind}">`
    // holding its own title, label and blocks, which is where an unconsumed
    // token belongs (CARVE-P9-072) and what carve-js and carve-php emit.
    if d.kind == "footnotes"
        && at_document_top_level
        && !state.rendering_footnotes
        && state.has_footnotes
        && state.footnotes_heading.is_none()
    {
        // The title takes its id HERE, before the marker's children render, so
        // the `adm-{n}` sequence follows document order.
        let id = d.title.as_ref().map(|_| state.mint_admonition_id());
        state.footnotes_heading = Some(FootnotesHeading {
            title: d.title.clone(),
            label: d.label.clone(),
            id,
        });
        // Preserve any blocks authored inside the placeholder before the
        // relocated endnotes (matching carve-js), then the marker.
        let body = render_blocks(&d.children, level, options, state);
        let body = body.trim_end_matches('\n');
        if !body.is_empty() {
            out.push_str(body);
            out.push('\n');
        }
        push_footnotes_placement_marker(out);
        return;
    }
    render_named_container(
        out,
        &d.attrs,
        &d.kind,
        d.title.as_deref(),
        &d.label,
        &d.children,
        level,
        options,
        state,
    );
}

fn render_admonition(
    out: &mut String,
    a: &Admonition,
    level: usize,
    options: &Options<'_>,
    state: &mut RenderState,
) {
    render_named_container(
        out,
        &a.attrs,
        &a.kind,
        a.title.as_deref(),
        &a.label,
        &a.children,
        level,
        options,
        state,
    );
}

/// One renderer for both named containers.
#[allow(clippy::too_many_arguments)]
fn render_named_container(
    out: &mut String,
    attrs: &Option<Attrs>,
    kind: &str,
    title: Option<&[InlineNode]>,
    label: &Option<String>,
    children: &[BlockNode],
    level: usize,
    options: &Options<'_>,
    state: &mut RenderState,
) {
    let canonical = crate::profile::ADMONITION_TIER1_KINDS.contains(&kind);
    indent(out, level);
    // The type class is structural (`admonition {kind}` for Tier 1, the bare
    // `{kind}` for a Tier-2 div) and emitted first; the opener's own
    // attribute block merges its classes into it and contributes id /
    // key-values after (never a second class).
    let base = if canonical {
        format!("admonition {kind}")
    } else {
        kind.to_string()
    };
    let tag = if canonical { "aside" } else { "div" };
    let (class, rest) = match attrs {
        Some(at) if !at.classes.is_empty() => (
            merge_base_class(&base, &at.classes),
            render_attrs_after_class_for(at, tag),
        ),
        Some(at) => (base, render_attrs_after_class_for(at, tag)),
        None => (base, String::new()),
    };
    let authored_name = attrs.as_ref().is_some_and(|attrs| {
        attrs.key_values.keys().any(|key| {
            key.eq_ignore_ascii_case("aria-label") || key.eq_ignore_ascii_case("aria-labelledby")
        })
    });
    let mut title_id = None;
    let accessible_name = if canonical && !authored_name {
        if title.is_some() {
            let id = state.mint_admonition_id();
            let attr = format!(" aria-labelledby=\"{}\"", escape_attr(&id));
            title_id = Some(id);
            attr
        } else {
            let mut chars = kind.chars();
            let key = format!(
                "admonition{}{}",
                chars.next().unwrap_or_default().to_ascii_uppercase(),
                chars.as_str()
            );
            let value = options.label(&key);
            if value.is_empty() {
                String::new()
            } else {
                format!(" aria-label=\"{}\"", escape_attr(value))
            }
        }
    } else {
        String::new()
    };
    out.push_str(&format!(
        "<{} class=\"{}\"{}{}>",
        tag,
        escape_attr(&class),
        rest,
        accessible_name
    ));
    if let Some(title) = title {
        out.push('\n');
        indent(out, level + 1);
        out.push_str("<p class=\"admonition-title\"");
        if let Some(id) = &title_id {
            write!(out, " id=\"{}\"", escape_attr(id)).unwrap();
        }
        out.push('>');
        render_inlines(out, title, options, state);
        out.push_str("</p>");
    }
    // Graceful degradation: when no extension consumed the grouping `[label]`,
    // surface it as a visible caption so the authored label is never silently
    // dropped in static output. Title (when present) renders first.
    if let Some(label) = label {
        out.push('\n');
        indent(out, level + 1);
        out.push_str("<p class=\"div-label\">");
        out.push_str(&render_container_label(label, options));
        out.push_str("</p>");
    }
    let visible = render_children(out, children, level + 1, options, state);
    close_block_container(
        out,
        level,
        tag,
        title.is_some() || label.is_some() || visible,
    );
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
    let mut authored = render_attrs_for(&lb.attrs, "div");
    append_class_token(&mut authored, "line-block");

    indent(out, level);
    // PART 10 §13a: the class alone also names a `line-block` admonition.
    let carrier = if options.html_carriers() {
        format!(" {}", crate::roundtrip::LINE_BLOCK)
    } else {
        String::new()
    };
    out.push_str(&format!("<div{carrier}{authored}>"));
    let visible = render_children(out, &lb.children, level + 1, options, state);
    close_block_container(out, level, "div", visible);
}

/// Append a structural class to the folded attribute run, after every
/// authored class whatever its spelling, or as a trailing `class` attribute.
fn append_class_token(attrs: &mut String, token: &str) {
    let mut at = 0;
    while let Some(tail) = attrs[at..].strip_prefix(' ') {
        let name_end = tail.find(['=', ' ']).unwrap_or(tail.len());
        let value_start = at + 1 + name_end + 2;
        let Some(close) = tail[name_end..]
            .strip_prefix("=\"")
            .and_then(|value| value.find('"'))
        else {
            at += 1 + name_end;
            continue;
        };
        if tail[..name_end].eq_ignore_ascii_case("class") {
            let end = value_start + close;
            let joiner = if close == 0 { "" } else { " " };
            attrs.insert_str(end, &format!("{joiner}{}", escape_attr(token)));
            return;
        }
        at = value_start + close + 1;
    }
    attrs.push_str(&format!(" class=\"{}\"", escape_attr(token)));
}

fn close_block_container(out: &mut String, level: usize, tag: &str, has_visible_body: bool) {
    out.push('\n');
    if !has_visible_body {
        out.push('\n');
    }
    indent(out, level);
    write!(out, "</{tag}>").unwrap();
}

fn render_div(
    out: &mut String,
    d: &Div,
    level: usize,
    options: &Options<'_>,
    state: &mut RenderState,
) {
    indent(out, level);
    let attrs = render_attrs_for(&d.attrs, "div");
    // PART 10 §13a: a `<div>` with no attribute is any foreign div.
    if attrs.is_empty() && options.html_carriers() {
        out.push_str(&format!("<div {}>", crate::roundtrip::DIV));
    } else {
        out.push_str(&format!("<div{attrs}>"));
    }
    // Graceful degradation: surface an unconsumed grouping `[label]` as a
    // visible caption (see render_admonition for rationale).
    if let Some(label) = &d.label {
        out.push('\n');
        indent(out, level + 1);
        out.push_str("<p class=\"div-label\">");
        out.push_str(&render_container_label(label, options));
        out.push_str("</p>");
    }
    let visible = render_children(out, &d.children, level + 1, options, state);
    close_block_container(out, level, "div", d.label.is_some() || visible);
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
                // §17 L7: a SPELLED looseness WRAPS the one paragraph, which is
                // the whole point of the key - `<dd><p>x</p></dd>` has no
                // blank-line spelling at any entry count, because a blank line
                // between two ENTRIES does not loosen a `<dl>` at all.
                //
                // It stays on the ONE LINE the derived form uses, the way a
                // loose list item's single paragraph does (`<li><p>x</p></li>`).
                // The multi-block path below is what indents, and a description
                // holding one block is not that shape however it was spelled.
                // An attributed paragraph keeps its `<p>` too (CARVE-P9-081).
                if d.loose || has_authored_attrs(&p.attrs) {
                    out.push_str(&format!("<p{}>", render_attrs_for(&p.attrs, "p")));
                    render_inlines(out, &p.children, options, state);
                    out.push_str("</p>");
                } else {
                    render_inlines(out, &p.children, options, state);
                }
                out.push_str("</dd>");
                continue;
            }
            out.push_str(&format!("<dd{}>", render_attrs(&def.attrs)));
            let visible = render_children(out, def, level + 2, options, state);
            // A definition whose whole body renders to nothing closes on its
            // own line, like the single-paragraph form above.
            if !visible {
                out.push_str("</dd>");
                continue;
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
            merge_base_class(base, &at.classes),
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
    // An extension claimed the captioned block and rendered it - a `chart` or
    // `mermaid` fence, an `img` fence. The replacement is an ordinary
    // `RawBlock`, so it takes the ordinary raw-block path and
    // `allow_raw_html: false` escapes it here exactly as it escapes a claimed
    // fence that stood on its own (markup-carve/carve-rs#1151).
    if let Some(raw) = &f.rendered_target {
        render_block(
            out,
            &BlockNode::RawBlock((**raw).clone()),
            level + 1,
            options,
            state,
        );
    } else {
        match &*f.target {
            FigureTarget::Image(img) => {
                indent(out, level + 1);
                render_image(out, img, options, crate::render_loss::RawNodeType::Block);
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
    let mut visible_children = 0;
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
            visible_children += 1;
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
    close_block_container(
        out,
        level,
        "figure",
        visible_children > 0 || g.caption.is_some(),
    );
}

fn render_extension_carrier(
    out: &mut String,
    node: &ExtensionCarrier,
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
            if let Some(html) = ext.render_extension_carrier(node, &ctx) {
                // A COLUMN-0 FRAGMENT TAKES NO INDENT HERE. It writes its own
                // closer at 0, so indenting the first line would move the opener
                // alone and leave the element at two columns - which
                // CARVE-P10-010 refuses under either of the choices it allows
                // (carve-rs#1969).
                if !ext.carrier_anchored_at_column_zero(node) {
                    indent(out, level);
                }
                out.push_str(&html);
                return;
            }
        }
    }
    let mut state = shared.borrow_mut();
    indent(out, level);
    out.push_str(&format!("<div class=\"{}\">", escape_attr(&node.name)));
    if render_children(out, &node.children, level + 1, options, &mut state) {
        out.push('\n');
        indent(out, level);
    }
    out.push_str("</div>");
}

fn render_image(
    out: &mut String,
    img: &Image,
    options: &Options<'_>,
    node_type: crate::render_loss::RawNodeType,
) {
    if crate::parse::is_unresolved_image(img) {
        // Same PART 10 SS2 nbsp fold as the link literal above.
        write_escaped_text_nbsp(out, img.raw_ref.as_deref().unwrap_or_default());
        return;
    }
    out.push_str(&format!(
        "<img src=\"{}\" alt=\"{}\"",
        escape_attr(&sanitize_destination(
            &img.src,
            img.pos.as_ref(),
            crate::render_loss::DeniedSink::ImageSource,
        )),
        escape_attr(&img.alt)
    ));
    if let Some(title) = &img.title {
        out.push_str(&format!(" title=\"{}\"", escape_attr(title)));
    }
    // PART 10 §13a: the reference label the image resolved by. HTML spells only
    // the resolved `src`, so without this a `![alt][lbl]` comes back inlined.
    let mut blocked: Vec<&str> = if img.title.is_some() {
        vec!["src", "alt", "title"]
    } else {
        vec!["src", "alt"]
    };
    if options.html_carriers() {
        if let Some(label) = &img.ref_label {
            write_carrier(
                out,
                crate::roundtrip::REF,
                label,
                &img.attrs,
                img.pos.as_ref(),
                node_type,
            );
            blocked.push(crate::roundtrip::REF);
        }
    }
    out.push_str(&render_attrs_without_keys(&img.attrs, &blocked));
    out.push('>');
}

mod inlines;
use inlines::*;
pub(crate) use inlines::{
    render_container_label, render_inlines_inside_anchor, render_inlines_with_options,
    semantic_span_order, semantic_value_target, EXTENDED_SEMANTIC_SPAN_ORDER,
};
/// Write the ` id="..."` slot. A generated slug holds no colon, so only an
/// authored value can meet the CARVE-P9-054 hardening here.
#[inline]
fn write_attr_id(out: &mut String, id: &str) {
    out.push_str(" id=\"");
    write_escaped_attr(out, &sanitize_attr_value("id", id));
    out.push('"');
}

/// The class slot as an HTML sink may write it: every AUTHORED entry through the
/// value sanitizer, the empty ones dropped, deduped in first-occurrence order
/// (§15, matching carve-php / carve-js).
pub(crate) fn sanitized_classes(classes: &[String]) -> Vec<std::borrow::Cow<'_, str>> {
    let mut sanitized: Vec<std::borrow::Cow<'_, str>> = Vec::new();
    let mut seen = (classes.len() > 8).then(|| HashSet::with_capacity(classes.len()));
    for entry in classes {
        // The parser split `class=V` into tokens (CARVE-P4-007); an AST entry
        // that still holds a space is probed whole.
        let entry = crate::escape::sanitize_attr_value("class", entry);
        if !entry.is_empty() {
            let unique = if let Some(seen) = &mut seen {
                seen.insert(entry.clone())
            } else {
                !sanitized.contains(&entry)
            };
            if unique {
                sanitized.push(entry);
            }
        }
    }
    sanitized
}

/// Write the ` class="..."` slot. The quotes go out even when every entry drops,
/// because a bare `class` claims the slot as `class=""` (PART 4).
#[inline]
fn write_attr_class(out: &mut String, classes: &[String]) {
    out.push_str(" class=\"");
    write_escaped_attr(out, &sanitized_classes(classes).join(" "));
    out.push('"');
}

/// A structural base class list, then the author's sanitized classes it does not
/// already hold: one class list, each value once (CARVE-P4-002).
fn merge_base_class(base: &str, classes: &[String]) -> String {
    let mut out = String::from(base);
    let tokens: Vec<&str> = base.split(' ').collect();
    for entry in sanitized_classes(classes) {
        if tokens.contains(&entry.as_ref()) {
            continue;
        }
        out.push(' ');
        out.push_str(&entry);
    }
    out
}

fn structural_attrs(base: &str, attrs: &Option<Attrs>) -> (String, String) {
    match attrs {
        Some(a) if !a.classes.is_empty() => (
            merge_base_class(base, &a.classes),
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
    let start = out.len();
    write_attrs_unfolded(out, attrs);
    fold_attr_names_from(out, start);
}

/// HTML attribute names compare ASCII case-insensitively in every merge: one
/// attribute per name, at the first occurrence's position and spelling, with
/// the last value; `class` values join (PART 10 §1).
pub(crate) fn fold_attr_names_from(out: &mut String, start: usize) {
    let segment = &out[start..];
    let mut attrs: Vec<(&str, Option<&str>)> = Vec::new();
    let mut rest = segment;
    while let Some(tail) = rest.strip_prefix(' ') {
        let name_end = tail.find(['=', ' ']).unwrap_or(tail.len());
        let name = &tail[..name_end];
        let after = &tail[name_end..];
        match after.strip_prefix("=\"") {
            Some(value_tail) => {
                let Some(close) = value_tail.find('"') else {
                    return;
                };
                attrs.push((name, Some(&value_tail[..close])));
                rest = &value_tail[close + 1..];
            }
            None => {
                attrs.push((name, None));
                rest = after;
            }
        }
    }
    if !rest.is_empty() || attrs.is_empty() {
        return;
    }
    let duplicated = attrs.iter().enumerate().any(|(i, (name, _))| {
        attrs[..i]
            .iter()
            .any(|(earlier, _)| earlier.eq_ignore_ascii_case(name))
    });
    if !duplicated {
        return;
    }
    let mut merged: Vec<(&str, Option<String>)> = Vec::new();
    for (name, value) in attrs {
        let Some(slot) = merged
            .iter_mut()
            .find(|(earlier, _)| earlier.eq_ignore_ascii_case(name))
        else {
            merged.push((name, value.map(str::to_string)));
            continue;
        };
        if name.eq_ignore_ascii_case("class") {
            let mut tokens: Vec<String> = slot
                .1
                .as_deref()
                .unwrap_or_default()
                .split(' ')
                .filter(|token| !token.is_empty())
                .map(str::to_string)
                .collect();
            for token in value.unwrap_or_default().split(' ') {
                if !token.is_empty() && !tokens.iter().any(|t| t == token) {
                    tokens.push(token.to_string());
                }
            }
            slot.1 = Some(tokens.join(" "));
        } else {
            slot.1 = value.map(str::to_string);
        }
    }
    let mut folded = String::with_capacity(segment.len());
    for (name, value) in merged {
        folded.push(' ');
        folded.push_str(name);
        if let Some(value) = value {
            folded.push_str("=\"");
            folded.push_str(&value);
            folded.push('"');
        }
    }
    out.truncate(start);
    out.push_str(&folded);
}

fn write_attrs_unfolded(out: &mut String, attrs: &Option<Attrs>) {
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
            if writes_authored_key(key) {
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
    let mut seen_keys: HashSet<&str> = HashSet::new();
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
                    if writes_authored_key(key) {
                        write_attr_key_value(out, key, value);
                    }
                }
                seen_keys.insert(key.as_str());
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
        if !seen_keys.contains(key.as_str()) && writes_authored_key(key) {
            write_attr_key_value(out, key, value);
        }
    }
}

/// The elements on which HTML's legacy `align` attribute means TEXT ALIGNMENT,
/// so `{align=...}` on them renders the CSS declaration instead of the
/// deprecated attribute (PART 10, markup-carve/carve#1755).
///
/// `table` IS DELIBERATELY ABSENT AND MUST NOT BE ADDED. On a table `align` is
/// PLACEMENT - the table floats left or right, or centres as a block - which is
/// a different property that does not map to `text-align` at all. Rewriting it
/// would silently turn a floated table into one whose CELL TEXT is
/// right-aligned, in every existing document that spells it, which is a worse
/// defect than the deprecated attribute. `{align=...}` on a table keeps
/// rendering `align=` until somebody rules what a floated table should spell.
///
/// The same reasoning keeps `img`, `hr`, `iframe`, `object`, `embed`, `input`
/// and `caption` out: HTML maps their `align` to a float, a margin, a
/// `vertical-align` or a caption side, never to `text-align`.
const TEXT_ALIGN_ELEMENTS: &[&str] = &["p", "div", "h1", "h2", "h3", "h4", "h5", "h6"];

/// The `align` values HTML gives a `text-align` meaning on those elements.
const TEXT_ALIGN_VALUES: &[&str] = &["left", "right", "center"];

/// Rewrite a text-alignment `align` key-value into a `text-align` declaration,
/// or `None` when there is nothing to rewrite (the common path allocates
/// nothing).
///
/// `align` is one of the KNOWN keys the attribute mechanism acts on, alongside
/// `loose` (consumed, emits nothing), `#id` and `.class`. Every other key stays
/// a raw pass-through: `{banana=yellow}` still renders `banana="yellow"`, and
/// `{valign=...}` is untouched here (markup-carve/carve#1756 ruled it working
/// as designed).
///
/// The declaration takes the `align` slot so source order is preserved. When
/// the author also wrote `style`, it is appended to that value instead - two
/// `style` attributes would be invalid HTML and the second one ignored.
pub(crate) fn rewrite_text_align(attrs: &Attrs, tag: &str) -> Option<Attrs> {
    if !TEXT_ALIGN_ELEMENTS.contains(&tag) {
        return None;
    }
    // HTML attribute names are case-insensitive, so `{ALIGN=right}` is the same
    // key and must take the same path.
    let align_key = attrs
        .key_values
        .keys()
        .find(|key| key.eq_ignore_ascii_case("align"))?
        .clone();
    let value = attrs.key_values[&align_key].trim().to_ascii_lowercase();
    if !TEXT_ALIGN_VALUES.contains(&value.as_str()) {
        return None;
    }
    let declaration = format!("text-align: {value};");
    let style_key = attrs
        .key_values
        .keys()
        .find(|key| key.eq_ignore_ascii_case("style"))
        .cloned();

    let mut rewritten = attrs.clone();
    rewritten.key_values.remove(&align_key);
    match &style_key {
        Some(key) => {
            let merged = append_declaration(&attrs.key_values[key], &declaration);
            rewritten.key_values.insert(key.clone(), merged);
            rewritten
                .order
                .retain(|slot| !matches!(slot, AttrSlot::Key(k) if *k == align_key));
        }
        None => {
            rewritten
                .key_values
                .insert("style".to_string(), declaration);
            for slot in &mut rewritten.order {
                if matches!(slot, AttrSlot::Key(k) if *k == align_key) {
                    *slot = AttrSlot::Key("style".to_string());
                }
            }
        }
    }
    Some(rewritten)
}

/// Append a declaration to an author `style` value, keeping one `style` attribute.
fn append_declaration(style: &str, declaration: &str) -> String {
    let trimmed = style.trim();
    if trimmed.is_empty() {
        return declaration.to_string();
    }
    if trimmed.ends_with(';') {
        format!("{trimmed} {declaration}")
    } else {
        format!("{trimmed}; {declaration}")
    }
}

/// `rewrite_text_align` for an optional attribute block.
fn rewrite_text_align_opt(attrs: &Option<Attrs>, tag: &str) -> Option<Attrs> {
    rewrite_text_align(attrs.as_ref()?, tag)
}

/// `write_attrs` for an element whose `align` may be a text alignment.
pub(crate) fn write_attrs_for(out: &mut String, attrs: &Option<Attrs>, tag: &str) {
    match rewrite_text_align_opt(attrs, tag) {
        Some(rewritten) => write_attrs(out, &Some(rewritten)),
        None => write_attrs(out, attrs),
    }
}

/// `render_attrs` for an element whose `align` may be a text alignment.
pub(crate) fn render_attrs_for(attrs: &Option<Attrs>, tag: &str) -> String {
    match rewrite_text_align_opt(attrs, tag) {
        Some(rewritten) => render_attrs(&Some(rewritten)),
        None => render_attrs(attrs),
    }
}

/// `render_attrs_after_class` for an element whose `align` may be a text alignment.
pub(crate) fn render_attrs_after_class_for(attrs: &Attrs, tag: &str) -> String {
    match rewrite_text_align(attrs, tag) {
        Some(rewritten) => render_attrs_after_class(&rewritten),
        None => render_attrs_after_class(attrs),
    }
}

pub(crate) fn render_attrs(attrs: &Option<Attrs>) -> String {
    let mut out = String::new();
    write_attrs(&mut out, attrs);
    out
}

/// Write one HTML roundtrip carrier. Reserved authored names are reported
/// before rendering and excluded from the authored attribute walk.
pub(crate) fn write_carrier(
    out: &mut String,
    name: &'static str,
    value: &str,
    _attrs: &Option<Attrs>,
    _pos: Option<&Pos>,
    _node_type: crate::render_loss::RawNodeType,
) {
    out.push(' ');
    out.push_str(name);
    out.push_str("=\"");
    write_escaped_attr(out, value);
    out.push('"');
}

/// Write a PRESENCE carrier bare, with no value at all.
///
/// §13a: "A writer writes the empty value or `1`". Measured against carve-php
/// under roundtrip mode, two of the four presence carriers are written BARE
/// (`<span data-djot-escaped>`, `<template data-djot-abbreviations>`) and the
/// other three carry `="1"`, so the inventory's two spellings are both in use
/// and byte-parity needs both here.
pub(crate) fn write_bare_carrier(
    out: &mut String,
    name: &'static str,
    _attrs: &Option<Attrs>,
    _pos: Option<&Pos>,
    _node_type: crate::render_loss::RawNodeType,
) {
    out.push(' ');
    out.push_str(name);
}

/// The attribute set without the named keys, for a caller that has to hand the
/// filtered set to a writer of its own rather than take a rendered string.
pub(crate) fn strip_keys(attrs: &Option<Attrs>, blocked: &[&str]) -> Option<Attrs> {
    let a = attrs.as_ref()?;
    let is_blocked = |k: &str| blocked.contains(&k.to_ascii_lowercase().as_str());
    if !a.key_values.keys().any(|k| is_blocked(k)) {
        return attrs.clone();
    }
    let mut filtered = a.clone();
    filtered.key_values.retain(|k, _| !is_blocked(k));
    filtered.order.retain(|slot| match slot {
        AttrSlot::Key(k) => !is_blocked(k),
        _ => true,
    });
    Some(filtered)
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
    let mut out = render_attrs_with_base_class_unfolded(attrs, base);
    fold_attr_names_from(&mut out, 0);
    out
}

fn render_attrs_with_base_class_unfolded(attrs: &Option<Attrs>, base: &str) -> String {
    let Some(attrs) = attrs else {
        return format!(" class=\"{}\"", escape_attr(base));
    };
    let merged = |classes: &[String]| -> String {
        format!(
            " class=\"{}\"",
            escape_attr(&merge_base_class(base, classes))
        )
    };
    // No recorded order: the slots go in the canonical order, class first.
    if attrs.order.is_empty() {
        let mut out = merged(&attrs.classes);
        if let Some(id) = &attrs.id {
            write_attr_id(&mut out, id);
        }
        for (key, value) in &attrs.key_values {
            if writes_authored_key(key) {
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
                    if writes_authored_key(key) {
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
    let mut out = render_attrs_after_class_unfolded(attrs);
    fold_attr_names_from(&mut out, 0);
    out
}

fn render_attrs_after_class_unfolded(attrs: &Attrs) -> String {
    let mut out = String::new();
    if attrs.order.is_empty() {
        if let Some(id) = &attrs.id {
            write_attr_id(&mut out, id);
        }
        for (key, value) in &attrs.key_values {
            if writes_authored_key(key) {
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
                    write_attr_id(&mut out, id);
                }
            }
            AttrSlot::Class => {}
            AttrSlot::Key(key) => {
                if let Some(value) = attrs.key_values.get(key) {
                    if writes_authored_key(key) {
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

/// `render_attrs_without_id`, additionally dropping the named keys a §13a
/// carrier has taken over on the same element.
fn render_attrs_without_id_and_keys(attrs: &Option<Attrs>, blocked: &[&str]) -> String {
    let mut attrs = attrs.clone();
    if let Some(attrs) = &mut attrs {
        attrs.id = None;
        attrs.order.retain(|slot| !matches!(slot, AttrSlot::Id));
    }
    render_attrs_without_keys(&attrs, blocked)
}

/// `render_attrs_without_id` for an element whose `align` may be a text alignment.
fn render_attrs_without_id_for(attrs: &Option<Attrs>, tag: &str) -> String {
    let mut attrs = attrs.clone();
    if let Some(attrs) = &mut attrs {
        attrs.id = None;
        attrs.order.retain(|slot| !matches!(slot, AttrSlot::Id));
    }
    render_attrs_for(&attrs, tag)
}

/// The HTML tag a heading of this level writes, for the text-alignment set.
fn heading_tag(level: u8) -> String {
    format!("h{level}")
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

#[cfg(test)]
thread_local! {
    static CONTAINER_OUTPUT_BUFFERS: std::cell::RefCell<std::collections::HashSet<usize>> =
        std::cell::RefCell::new(std::collections::HashSet::new());
    static QUOTE_OUTPUT_BUFFERS: std::cell::RefCell<std::collections::HashSet<usize>> =
        std::cell::RefCell::new(std::collections::HashSet::new());
    static LIST_OUTPUT_BUFFERS: std::cell::RefCell<std::collections::HashSet<usize>> =
        std::cell::RefCell::new(std::collections::HashSet::new());
}

#[cfg(test)]
mod nested_container_output_buffers {
    #[test]
    fn nested_divs_append_to_one_output_buffer() {
        std::thread::Builder::new()
            .stack_size(64 * 1024 * 1024)
            .spawn(|| {
                let mut source = String::new();
                for width in (3..=130).rev() {
                    source.push_str(&":".repeat(width));
                    source.push('\n');
                }
                source.push_str("body\n");
                for width in 3..=130 {
                    source.push_str(&":".repeat(width));
                    source.push('\n');
                }
                let doc = crate::parse(&source);
                super::CONTAINER_OUTPUT_BUFFERS.with(|buffers| buffers.borrow_mut().clear());
                let html = crate::render_html(&doc).unwrap();
                assert_eq!(html.matches("<div>").count(), 128);
                assert_eq!(
                    super::CONTAINER_OUTPUT_BUFFERS.with(|buffers| buffers.borrow().len()),
                    1
                );
            })
            .unwrap()
            .join()
            .unwrap();
    }

    #[test]
    fn nested_quotes_append_to_one_output_buffer() {
        std::thread::Builder::new()
            .stack_size(64 * 1024 * 1024)
            .spawn(|| {
                let source = format!("{}end\n", "> ".repeat(192));
                let doc = crate::parse(&source);
                super::QUOTE_OUTPUT_BUFFERS.with(|buffers| buffers.borrow_mut().clear());
                let html = crate::render_html(&doc).unwrap();
                assert_eq!(html.matches("<blockquote>").count(), 192);
                assert_eq!(
                    super::QUOTE_OUTPUT_BUFFERS.with(|buffers| buffers.borrow().len()),
                    1
                );
            })
            .unwrap()
            .join()
            .unwrap();
    }

    #[test]
    fn nested_lists_append_to_one_output_buffer() {
        std::thread::Builder::new()
            .stack_size(64 * 1024 * 1024)
            .spawn(|| {
                let source = format!("{}end\n", "- ".repeat(192));
                let doc = crate::parse(&source);
                super::LIST_OUTPUT_BUFFERS.with(|buffers| buffers.borrow_mut().clear());
                let html = crate::render_html(&doc).unwrap();
                assert_eq!(html.matches("<ul>").count(), 192);
                assert_eq!(
                    super::LIST_OUTPUT_BUFFERS.with(|buffers| buffers.borrow().len()),
                    1
                );
            })
            .unwrap()
            .join()
            .unwrap();
    }
}

#[cfg(test)]
mod owned_html_tests {
    use super::*;

    #[test]
    fn owned_and_borrowed_renderers_agree() {
        for source in [
            "# Same\n\n# Same\n",
            "![alt](/image)\n",
            "[text][r]\n\n[r]: /target \"title\"\n",
            "A[^n]\n\n[^n]: note\n",
            "# Target\n\n</#Target>\n",
        ] {
            for sections in [false, true] {
                let options = Options {
                    sections,
                    ..Options::default()
                };
                let doc = crate::parse_with_options(source, &options);
                let before = doc.clone();
                let borrowed = render_html_with_options(&doc, &options).unwrap();
                assert_eq!(doc, before);
                assert_eq!(
                    render_html_owned_with_options(doc, &options).unwrap(),
                    borrowed
                );
            }
        }
        assert_eq!(
            render_html_owned(crate::parse("Hello.")).unwrap(),
            "<p>Hello.</p>"
        );
    }

    #[test]
    fn owned_renderer_refuses_before_preprocessing() {
        std::thread::Builder::new()
            .stack_size(256 * 1024)
            .spawn(|| {
                let mut doc = crate::parse("leaf");
                let mut block = doc.children.pop().unwrap();
                for _ in 0..100_000 {
                    block = BlockNode::BlockQuote(BlockQuote {
                        children: vec![block],
                        attrs: None,
                        pos: None,
                        fenced: false,
                    });
                }
                doc.children.push(block);
                let error = render_html_owned(doc).unwrap_err();
                assert_eq!(error.renderer(), "html");
                assert_eq!(error.limit(), MAX_RENDER_DEPTH);
            })
            .unwrap()
            .join()
            .unwrap();
    }
}

#[cfg(test)]
mod owned_html_metadata_depth_tests {
    use super::*;

    fn deep_inline() -> Vec<InlineNode> {
        let mut node = InlineNode::text("leaf".to_string());
        for _ in 0..MAX_RENDER_DEPTH + 16 {
            node = InlineNode::Emphasis(Emphasis {
                attrs: None,
                kind: EmphasisKind::Strong,
                children: vec![node],
                pos: None,
            });
        }
        vec![node]
    }

    #[test]
    fn metadata_inline_trees_obey_the_ceiling() {
        for slot in 0..6 {
            let mut doc = crate::parse("");
            let table = |caption, short_caption| Table {
                source_format: None,
                attrs: None,
                caption,
                short_caption,
                columns: vec![],
                rows: vec![],
                row_groups: None,
                separator_widths: None,
                pos: None,
            };
            let node = match slot {
                0 => BlockNode::CitationDefinition(CitationDefinition {
                    key: "key".to_string(),
                    children: deep_inline(),
                    attrs: None,
                    pos: None,
                }),
                1 => BlockNode::ExtensionCarrier(ExtensionCarrier {
                    name: "name".to_string(),
                    attrs: None,
                    children: vec![],
                    summary: Some(deep_inline()),
                    label: None,
                    pos: None,
                }),
                2 => BlockNode::Table(table(None, Some(deep_inline()))),
                _ => BlockNode::Figure(Figure {
                    attrs: None,
                    caption: vec![],
                    rendered_target: None,
                    pos: None,
                    short_caption: if slot == 3 { Some(deep_inline()) } else { None },
                    target: Box::new(FigureTarget::Table(table(
                        if slot == 4 { Some(deep_inline()) } else { None },
                        if slot == 5 { Some(deep_inline()) } else { None },
                    ))),
                }),
            };
            doc.children.push(node);
            assert!(render_html(&doc).is_err(), "slot {slot}");
            assert!(render_html_owned(doc).is_err(), "slot {slot}");
        }
    }
}

#[cfg(test)]
mod borrowed_preparation_tests {
    use super::*;

    #[test]
    fn borrowed_and_owned_paths_agree_and_leave_the_input_unchanged() {
        for source in [
            "# Head\n\nplain *emphasis* and [link](https://example.org).\n",
            "::: note \"Title\"\n> paragraph\n:::\n",
            "| a | b |\n| ^ | c |\n",
            "![map](map.svg)\n",
            "  ![map](map.svg)\n  ^ caption\n",
            "note[^a] and ^[inline]\n\n[^a]: body\n",
            "![unresolved][missing]\n",
            "| ![map](map.svg) | text |\n",
            "[![map](map.svg)](https://example.org)\n",
            "| cell |\n^ note[^a]\n\n[^a]: body\n",
            "[^unused]: ![map](map.svg)\n\nplain\n",
            "::: box\n[^unused]: body\n\nplain\n:::\n",
            "::: box\n- ![map](map.svg)\n:::\n",
        ] {
            let doc = crate::parse(source);
            let before = crate::ast_json::to_json(&doc);
            let borrowed = render_html(&doc).unwrap();
            let owned = render_html_owned(doc.clone()).unwrap();
            assert_eq!(borrowed, owned, "{source}");
            assert_eq!(before, crate::ast_json::to_json(&doc), "{source}");
        }
    }
}

#[cfg(test)]
mod escaped_raw_block {
    use super::render_escaped_raw_block;
    use crate::ast::RawBlock;

    #[test]
    fn the_class_is_the_recorded_format_verbatim() {
        let raw = RawBlock {
            attrs: None,
            format: "latex".to_string(),
            content: "\\emph{x}".to_string(),
            pos: None,
        };
        let mut out = String::new();
        render_escaped_raw_block(&mut out, &raw, 0);
        assert_eq!(
            out,
            "<pre><code class=\"language-latex\">\\emph{x}</code></pre>"
        );
    }
}

fn authored_ref_id(f: &crate::ast::Footnote) -> Option<String> {
    f.attrs.as_ref()?.id.clone()
}
