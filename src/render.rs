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
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt::Write as _;

const MAX_RENDER_DEPTH: usize = 80;

pub fn render_html(doc: &Document) -> String {
    render_html_with_options(doc, &Options::default())
}

pub fn render_html_with_options(doc: &Document, options: &Options<'_>) -> String {
    let mut doc = doc.clone();
    let _abbr_guard = AbbrBudgetGuard::new(doc.source_len);
    let _index_guard = crate::index_budget::IndexBudgetGuard::new(doc.source_len);
    // Document id namespace (extensions contract §2.6): seeded with every
    // explicit `{#id}` attribute and every heading id this render will assign,
    // so extension-generated ids (citation anchors / reference entries) take
    // the next free suffix instead of emitting a duplicate DOM id.
    let _document_ids_guard =
        crate::document_ids::DocumentIdsGuard::new(&doc, options.lowercase_heading_ids);
    let mut state = RenderState {
        lowercase_heading_ids: options.lowercase_heading_ids,
        ..RenderState::default()
    };
    let footnotes = collect_footnotes(&mut doc);
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

fn render_blocks(
    nodes: &[BlockNode],
    level: usize,
    options: &Options<'_>,
    state: &mut RenderState,
) -> String {
    let mut out = String::new();
    let mut first = true;
    for block in nodes {
        if matches!(block, BlockNode::Comment(_)) {
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

#[derive(Default)]
pub(crate) struct RenderState {
    heading_counts: BTreeMap<String, usize>,
    /// Mirrors `Options::lowercase_heading_ids` so the `<section id>` derived
    /// here matches the parse-time id index (and the resolved cross-ref hrefs).
    lowercase_heading_ids: bool,
    /// True while rendering the endnotes section's footnote bodies. A
    /// `::: footnotes` nested inside a footnote definition must NOT emit a
    /// placement sentinel (it renders as an ordinary div, matching carve-js).
    rendering_footnotes: bool,
}

fn render_document_blocks(
    nodes: &[BlockNode],
    options: &Options<'_>,
    state: &mut RenderState,
) -> String {
    let mut out = String::new();
    let mut i = 0;
    let mut first = true;
    while i < nodes.len() {
        if matches!(nodes[i], BlockNode::Comment(_)) {
            i += 1;
            continue;
        }
        if !first {
            out.push('\n');
        }
        if matches!(nodes[i], BlockNode::Heading(_)) && options.sections {
            i = render_section(&mut out, nodes, i, 0, options, state);
        } else {
            render_block(&mut out, &nodes[i], 0, options, state);
            i += 1;
        }
        first = false;
    }
    out
}

#[derive(Clone)]
struct FootnoteEntry {
    label: Option<String>,
    inline: Option<Vec<InlineNode>>,
    backrefs: Vec<String>,
}

fn collect_footnotes(doc: &mut Document) -> Vec<FootnoteEntry> {
    let mut order = Vec::new();
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    let def_labels: HashSet<String> = doc.footnote_defs.keys().cloned().collect();
    let mut label_indices: HashMap<String, usize> = HashMap::new();

    for block in &mut doc.children {
        collect_footnotes_block(
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
    block: &mut BlockNode,
    def_labels: &HashSet<String>,
    label_indices: &mut HashMap<String, usize>,
    seen: &mut BTreeMap<String, usize>,
    order: &mut Vec<FootnoteEntry>,
) {
    match block {
        BlockNode::Heading(h) => {
            collect_footnotes_inline(&mut h.children, def_labels, label_indices, seen, order)
        }
        BlockNode::Paragraph(p) => {
            collect_footnotes_inline(&mut p.children, def_labels, label_indices, seen, order);
        }
        BlockNode::List(l) => {
            for item in &mut l.items {
                for child in &mut item.children {
                    collect_footnotes_block(child, def_labels, label_indices, seen, order);
                }
            }
        }
        BlockNode::BlockQuote(b) => {
            if let Some(attribution) = &mut b.attribution {
                collect_footnotes_inline(attribution, def_labels, label_indices, seen, order);
            }
            for child in &mut b.children {
                collect_footnotes_block(child, def_labels, label_indices, seen, order);
            }
        }
        BlockNode::Table(t) => {
            if let Some(caption) = &mut t.caption {
                collect_footnotes_inline(caption, def_labels, label_indices, seen, order);
            }
            for row in &mut t.rows {
                for cell in &mut row.cells {
                    collect_footnotes_inline(
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
                collect_footnotes_inline(title, def_labels, label_indices, seen, order);
            }
            for child in &mut a.children {
                collect_footnotes_block(child, def_labels, label_indices, seen, order);
            }
        }
        BlockNode::LineBlock(lb) => {
            for child in &mut lb.children {
                collect_footnotes_block(child, def_labels, label_indices, seen, order);
            }
        }
        BlockNode::Div(d) => {
            for child in &mut d.children {
                collect_footnotes_block(child, def_labels, label_indices, seen, order);
            }
        }
        BlockNode::DefinitionList(d) => {
            for item in &mut d.items {
                for term in &mut item.terms {
                    collect_footnotes_inline(term, def_labels, label_indices, seen, order);
                }
                for definition in &mut item.definitions {
                    for child in definition {
                        collect_footnotes_block(child, def_labels, label_indices, seen, order);
                    }
                }
            }
        }
        BlockNode::Figure(f) => {
            collect_footnotes_inline(&mut f.caption, def_labels, label_indices, seen, order);
            match &mut f.target {
                FigureTarget::BlockQuote(b) => {
                    if let Some(attribution) = &mut b.attribution {
                        collect_footnotes_inline(
                            attribution,
                            def_labels,
                            label_indices,
                            seen,
                            order,
                        );
                    }
                    for child in &mut b.children {
                        collect_footnotes_block(child, def_labels, label_indices, seen, order);
                    }
                }
                FigureTarget::Table(t) => {
                    if let Some(caption) = &mut t.caption {
                        collect_footnotes_inline(caption, def_labels, label_indices, seen, order);
                    }
                    for row in &mut t.rows {
                        for cell in &mut row.cells {
                            collect_footnotes_inline(
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
                collect_footnotes_block(child, def_labels, label_indices, seen, order);
            }
        }
        _ => {}
    }
}

fn collect_footnotes_inline(
    nodes: &mut [InlineNode],
    def_labels: &HashSet<String>,
    label_indices: &mut HashMap<String, usize>,
    seen: &mut BTreeMap<String, usize>,
    order: &mut Vec<FootnoteEntry>,
) {
    for node in nodes {
        match node {
            InlineNode::Footnote(f) => {
                if let Some(inline) = &f.inline {
                    let number = order.len() + 1;
                    let ref_id = format!("fnref{number}");
                    f.number = Some(number);
                    f.ref_id = Some(ref_id.clone());
                    order.push(FootnoteEntry {
                        label: None,
                        inline: Some(inline.clone()),
                        backrefs: vec![ref_id],
                    });
                    continue;
                }

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
                f.ref_id = Some(ref_id.clone());
                order[idx].backrefs.push(ref_id);
            }
            InlineNode::Emphasis(e) => {
                collect_footnotes_inline(&mut e.children, def_labels, label_indices, seen, order)
            }
            InlineNode::Link(l) => {
                collect_footnotes_inline(&mut l.children, def_labels, label_indices, seen, order)
            }
            InlineNode::Span(s) => {
                collect_footnotes_inline(&mut s.children, def_labels, label_indices, seen, order)
            }
            InlineNode::Extension(e) => {
                collect_footnotes_inline(&mut e.children, def_labels, label_indices, seen, order)
            }
            InlineNode::CriticInsert(c) => {
                collect_footnotes_inline(&mut c.children, def_labels, label_indices, seen, order);
            }
            InlineNode::CriticDelete(c) => {
                collect_footnotes_inline(&mut c.children, def_labels, label_indices, seen, order);
            }
            InlineNode::CitationGroup(g) => {
                for item in &mut g.items {
                    if let Some(prefix) = &mut item.prefix {
                        collect_footnotes_inline(prefix, def_labels, label_indices, seen, order);
                    }
                    if let Some(locator) = &mut item.locator {
                        collect_footnotes_inline(locator, def_labels, label_indices, seen, order);
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
            render_inlines(&mut out, inline, options);
            out.push_str(&render_backlinks(&entry.backrefs));
            out.push_str("</p>");
        } else if let Some(label) = &entry.label {
            if let Some(blocks) = doc.footnote_defs.get(label) {
                for (block_idx, block) in blocks.iter().enumerate() {
                    out.push('\n');
                    let mut rendered = String::new();
                    render_block(&mut rendered, block, 3, options, state);
                    if block_idx + 1 == blocks.len() {
                        let backlink = render_backlinks(&entry.backrefs);
                        if let Some(pos) = rendered.rfind("</p>") {
                            rendered.insert_str(pos, &backlink);
                        } else {
                            rendered.push_str(&backlink);
                        }
                    }
                    out.push_str(&rendered);
                }
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
    render_heading_without_section_id(out, heading, level + 1, options);
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

fn render_block(
    out: &mut String,
    node: &BlockNode,
    level: usize,
    options: &Options<'_>,
    state: &mut RenderState,
) {
    if level > MAX_RENDER_DEPTH {
        return;
    }
    match node {
        BlockNode::Heading(h) => render_heading(out, h, level, options, state),
        BlockNode::Paragraph(p) => render_paragraph(out, p, level, options),
        BlockNode::CodeBlock(c) => render_code_block(out, c, level),
        BlockNode::List(l) => render_list(out, l, level, options, state),
        BlockNode::BlockQuote(b) => render_blockquote(out, b, level, options, state),
        BlockNode::Table(t) => render_table(out, t, level, options),
        BlockNode::Admonition(a) => render_admonition(out, a, level, options, state),
        BlockNode::Div(d) => render_div(out, d, level, options, state),
        BlockNode::LineBlock(lb) => render_line_block(out, lb, level, options, state),
        BlockNode::DefinitionList(d) => render_definition_list(out, d, level, options, state),
        BlockNode::Figure(f) => render_figure(out, f, level, options, state),
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
    let authored_id = h.attrs.as_ref().is_some_and(|attrs| attrs.id.is_some());
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
    render_inlines(out, &h.children, options);
    write!(out, "</h{}>", h.level).unwrap();
}

fn render_heading_without_section_id(
    out: &mut String,
    h: &Heading,
    level: usize,
    options: &Options<'_>,
) {
    let mut attrs = h.attrs.clone();
    if let Some(attrs) = &mut attrs {
        attrs.id = None;
    }
    indent(out, level);
    write!(out, "<h{}", h.level).unwrap();
    write_attrs(out, &attrs);
    out.push('>');
    render_inlines(out, &h.children, options);
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
    let mut out = String::new();
    for node in nodes {
        match node {
            InlineNode::Text(s) => {
                out.push_str(&s.value.replace(crate::ESCAPED_CARET_PLACEHOLDER, "^"))
            }
            InlineNode::SmartPunctuation(s) => out.push_str(smart_punctuation_glyph(s)),
            InlineNode::Emphasis(e) => out.push_str(&plain_inlines(&e.children)),
            InlineNode::Code(s) => out.push_str(&s.value),
            // An inline literal renders as visible prose (§27), so it contributes
            // its content to a heading slug -- otherwise `` # !`Cat` `` would
            // slug to the empty fallback and `</#cat>` could never resolve.
            InlineNode::LiteralInline(lit) => out.push_str(&lit.content),
            // A `</#id>` cross-reference contributes nothing to a heading id: the
            // id is derived from the heading text as authored, before cross-ref
            // resolution turns the reference into a Link. Skipping it here keeps
            // the render-time id byte-identical to the parse-time id used to
            // build the cross-reference index (so `# A </#a>` keeps id `A`, not
            // `A-A`). Mirrors `plain_inlines_parse`, which never saw the Link.
            InlineNode::Link(l) if l.from_crossref => {}
            InlineNode::Link(l) => out.push_str(&plain_inlines(&l.children)),
            InlineNode::Image(i) => out.push_str(&i.alt),
            InlineNode::Extension(e) => out.push_str(&plain_inlines(&e.children)),
            InlineNode::CitationGroup(g) => out.push_str(&g.raw),
            InlineNode::Abbreviation(a) => out.push_str(&a.abbr),
            InlineNode::Mention(m) => out.push_str(&m.user),
            InlineNode::Tag(t) => out.push_str(&t.name),
            InlineNode::CaptionNumber(n) => {
                if let Some(number) = n.number {
                    out.push_str(&number.to_string());
                }
            }
            // A soft/hard break (e.g. a multi-line heading) is a word
            // separator for slug/plain-text purposes, not a join.
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

fn render_paragraph(out: &mut String, p: &Paragraph, level: usize, options: &Options<'_>) {
    indent(out, level);
    out.push_str("<p");
    write_attrs(out, &p.attrs);
    out.push('>');
    render_inlines(out, &p.children, options);
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
    write_escaped_text(out, &c.content);
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
    write_attrs(out, &l.attrs);
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
                render_inlines(&mut html, &p.children, options);
                if tight {
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
        .collect();
    match &parts[0] {
        Part::Inline(html) => {
            out.push_str(checkbox);
            out.push_str(html);
        }
        Part::Block(html) => {
            // A task item whose first child is a block still shows its checkbox
            // (defensive: no current input yields a block-first task item -- an
            // empty task marker renders `[ ]` as literal text -- but if one ever
            // did, dropping the marker would silently lose it).
            if !checkbox.is_empty() {
                out.push('\n');
                out.push_str(checkbox);
            } else {
                out.push('\n');
            }
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
            render_inlines(out, &p.children, options);
            out.push_str("</p></blockquote>");
            return;
        }
    }
    out.push_str("<blockquote");
    write_attrs(out, &b.attrs);
    out.push_str(">\n");
    let mut first = true;
    for child in &b.children {
        if !first {
            out.push('\n');
        }
        render_block(out, child, level + 1, options, state);
        first = false;
    }
    out.push('\n');
    indent(out, level);
    out.push_str("</blockquote>");
}

fn render_table(out: &mut String, t: &Table, level: usize, options: &Options<'_>) {
    indent(out, level);
    out.push_str("<table");
    write_attrs(out, &t.attrs);
    out.push('>');
    if let Some(caption) = &t.caption {
        out.push('\n');
        indent(out, level + 1);
        out.push_str("<caption>");
        render_inlines(out, caption, options);
        out.push_str("</caption>");
    }
    // The leading run of rows whose cells are ALL header cells forms <thead>.
    // A row that merely contains a header cell (a row header) stays in the body.
    let header_count = t
        .rows
        .iter()
        .take_while(|row| !row.cells.is_empty() && row.cells.iter().all(|cell| cell.header))
        .count();
    let has_header = header_count > 0;
    let body_start = header_count;
    // Computed once over ALL rows: a `^` in a body row extends the cell above
    // it even when that cell is in a header row, so a header cell can carry a
    // rowspan that crosses the thead/tbody boundary (matches carve-js).
    let (rowspan_cols, orphan_carets) = compute_rowspans(t);
    if has_header {
        out.push('\n');
        indent(out, level + 1);
        out.push_str("<thead>");
        for (row_idx, header) in t.rows[..header_count].iter().enumerate() {
            render_table_row(out, header, true, options, row_idx, &rowspan_cols);
        }
        out.push_str("</thead>");
    }
    // A header-only table (e.g. a GFM `| x |` + `|---|` with no body rows) emits
    // no <tbody>, matching carve-php.
    if body_start < t.rows.len() {
        out.push('\n');
        indent(out, level + 1);
        out.push_str("<tbody>");
        for (row_idx, row) in t.rows.iter().enumerate().skip(body_start) {
            out.push('\n');
            indent(out, level + 2);
            render_table_body_row(out, row, row_idx, &rowspan_cols, &orphan_carets, t, options);
        }
        out.push('\n');
        indent(out, level + 1);
        out.push_str("</tbody>");
    }
    out.push('\n');
    indent(out, level);
    out.push_str("</table>");
}

fn render_table_row(
    out: &mut String,
    row: &TableRow,
    header_row: bool,
    options: &Options<'_>,
    row_idx: usize,
    rowspan_cols: &BTreeMap<(usize, usize), usize>,
) {
    out.push_str("<tr");
    write_attrs(out, &row.attrs);
    out.push('>');
    for (col, cell) in row.cells.iter().enumerate() {
        let tag = if header_row || cell.header {
            "th"
        } else {
            "td"
        };
        let mut extra = String::new();
        let mut emitted: Vec<&str> = Vec::new();
        // A header cell can carry a rowspan that extends down into the body
        // (a `^` below it), so the header row emits it too -- not just bodies.
        if let Some(span) = rowspan_cols.get(&(row_idx, col)) {
            extra.push_str(&format!(" rowspan=\"{}\"", span));
            emitted.push("rowspan");
        }
        let align = render_align_attr(cell.align.or_else(|| row_align(row, col)));
        if !align.is_empty() {
            emitted.push("style");
        }
        out.push('<');
        out.push_str(tag);
        out.push_str(&render_cell_author_attrs(&cell.attrs, &emitted));
        out.push_str(&extra);
        out.push_str(&align);
        out.push('>');
        render_inlines(out, &cell.children, options);
        out.push_str("</");
        out.push_str(tag);
        out.push('>');
    }
    out.push_str("</tr>");
}

fn render_table_body_row(
    out: &mut String,
    row: &TableRow,
    source_row_idx: usize,
    rowspan_cols: &BTreeMap<(usize, usize), usize>,
    orphan_carets: &BTreeSet<(usize, usize)>,
    table: &Table,
    options: &Options<'_>,
) {
    out.push_str("<tr");
    write_attrs(out, &row.attrs);
    out.push('>');
    let consumed_cols = consumed_rowspan_cols(source_row_idx, rowspan_cols);
    let colspan_counts = compute_colspans(row, &consumed_cols);
    for (cell_index, cell) in row.cells.iter().enumerate() {
        if cell.span == Some(TableCellSpan::Rowspan) {
            // A `^` that merged into a cell above renders nothing; one with
            // nothing to extend (no cell above) renders an EMPTY cell (§5).
            if orphan_carets.contains(&(source_row_idx, cell_index)) {
                let tag = if cell.header { "th" } else { "td" };
                write!(out, "<{tag}></{tag}>").unwrap();
            }
            continue;
        }
        let mut attrs = String::new();
        let mut emitted: Vec<&str> = Vec::new();
        if let Some(span) = rowspan_cols.get(&(source_row_idx, cell_index)) {
            attrs.push_str(&format!(" rowspan=\"{}\"", span));
            emitted.push("rowspan");
        }
        if cell.span == Some(TableCellSpan::Colspan) {
            // A `<` that merged into a cell to its left renders nothing; one
            // with nothing to merge (first column / no real left cell) renders
            // an EMPTY cell (§5).
            if colspan_target(row, cell_index, &consumed_cols).is_none() {
                let tag = if cell.header { "th" } else { "td" };
                write!(out, "<{tag}></{tag}>").unwrap();
            }
            continue;
        }
        let colspan = colspan_counts.get(&cell_index).copied().unwrap_or(1);
        if colspan > 1 {
            attrs.push_str(&format!(" colspan=\"{}\"", colspan));
            emitted.push("colspan");
        }
        if let Some(align) = cell.align.or_else(|| table_column_align(table, cell_index)) {
            attrs.push_str(&align_attr(align));
            emitted.push("style");
        }
        // A `|=` cell in a body row is a row header: <th> inside <tbody>.
        let tag = if cell.header { "th" } else { "td" };
        out.push('<');
        out.push_str(tag);
        out.push_str(&render_cell_author_attrs(&cell.attrs, &emitted));
        out.push_str(&attrs);
        out.push('>');
        render_inlines(out, &cell.children, options);
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
    table.rows.first()?.cells.get(col)?.align
}

fn render_align_attr(align: Option<TableAlign>) -> String {
    align.map(align_attr).unwrap_or_default()
}

/// Render a cell's author attributes, dropping any key that collides (case
/// -insensitively) with a structural attribute this renderer actually emits
/// for the cell (`rowspan` / `colspan` / `style`) -- the computed value is
/// authoritative. When no such structural attribute is emitted, the author's
/// value (e.g. a custom `style`) is preserved.
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

fn align_attr(align: TableAlign) -> String {
    let value = match align {
        TableAlign::Left => "left",
        TableAlign::Right => "right",
        TableAlign::Center => "center",
    };
    format!(" style=\"text-align: {value};\"")
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
        render_inlines(out, title, options);
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
    if a.title.is_none() && a.label.is_none() && a.children.is_empty() {
        out.push('\n');
    }
    for child in &a.children {
        out.push('\n');
        render_block(out, child, level + 1, options, state);
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
    for child in &lb.children {
        out.push('\n');
        render_block(out, child, level + 1, options, state);
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
    for child in &d.children {
        out.push('\n');
        render_block(out, child, level + 1, options, state);
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
    _state: &mut RenderState,
) {
    indent(out, level);
    out.push_str(&format!("<dl{}>", render_attrs(&d.attrs)));
    for item in &d.items {
        for term in &item.terms {
            out.push('\n');
            indent(out, level + 1);
            out.push_str(&format!("<dt{}>", render_attrs(&term.attrs)));
            render_inlines(out, term, options);
            out.push_str("</dt>");
        }
        for def in &item.definitions {
            out.push('\n');
            indent(out, level + 1);
            if def.len() == 1 {
                if let BlockNode::Paragraph(p) = &def[0] {
                    out.push_str(&format!("<dd{}>", render_attrs(&def.attrs)));
                    render_inlines(out, &p.children, options);
                    out.push_str("</dd>");
                    continue;
                }
            }
            out.push_str(&format!("<dd{}>", render_attrs(&def.attrs)));
            for block in def {
                out.push('\n');
                render_block(out, block, level + 2, options, _state);
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
    out.push('\n');
    match &f.target {
        FigureTarget::Image(img) => {
            indent(out, level + 1);
            render_image(out, img);
        }
        FigureTarget::BlockQuote(b) => render_blockquote(out, b, level + 1, options, state),
        FigureTarget::Table(t) => render_table(out, t, level + 1, options),
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
    render_inlines(out, &f.caption, options);
    out.push_str("</figcaption>");
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
    if !node.children.is_empty() {
        out.push('\n');
        let mut first = true;
        for child in &node.children {
            if !first {
                out.push('\n');
            }
            render_block(out, child, level + 1, options, &mut state);
            first = false;
        }
        out.push('\n');
        indent(out, level);
    }
    out.push_str("</div>");
}

fn render_image(out: &mut String, img: &Image) {
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
    let mut out = String::new();
    render_inlines(&mut out, nodes, options);
    out
}

fn render_inlines(out: &mut String, nodes: &[InlineNode], options: &Options<'_>) {
    render_inlines_stateful(out, nodes, options);
}

fn render_inlines_stateful(out: &mut String, nodes: &[InlineNode], options: &Options<'_>) {
    for node in nodes {
        render_inline_after(out, node, options);
    }
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
            crate::ESCAPED_CARET_PLACEHOLDER => "^",
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

fn render_inline_after(out: &mut String, node: &InlineNode, options: &Options<'_>) {
    match node {
        InlineNode::Text(s) => {
            // Escape `& < >` AND fold U+00A0 to `&nbsp;` in a single pass over
            // `out`. None of the escaped chars is U+00A0, so the combined pass
            // is byte-identical to `escape_text(..).replace('\u{00a0}', ..)`.
            write_escaped_text_nbsp(out, &s.value);
        }
        InlineNode::EscapedText(s) => {
            // The backslash is authoring syntax; the reader sees the character.
            write_escaped_text_nbsp(out, &s.value.replace(crate::ESCAPED_CARET_PLACEHOLDER, "^"));
        }
        InlineNode::SmartPunctuation(s) => write_escaped_text_nbsp(out, smart_punctuation_glyph(s)),
        InlineNode::Emphasis(e) => render_emphasis(out, e, options),
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
        InlineNode::Link(l) => render_link(out, l, options),
        InlineNode::Image(img) => render_image(out, img),
        InlineNode::Span(s) => {
            out.push_str("<span");
            write_attrs(out, &s.attrs);
            out.push('>');
            render_inlines_stateful(out, &s.children, options);
            out.push_str("</span>");
        }
        InlineNode::Math(m) => {
            let base = if m.display {
                "math display"
            } else {
                "math inline"
            };
            let open = if m.display { "\\[" } else { "\\(" };
            let close = if m.display { "\\]" } else { "\\)" };
            // The `math {inline,display}` class is structural and emitted
            // first; a trailing attribute block merges its classes into it
            // and contributes id / key-values after (never a second class).
            let (class, rest) = match &m.attrs {
                Some(a) if !a.classes.is_empty() => (
                    dedup_class_str(&format!("{} {}", base, a.classes.join(" "))),
                    render_attrs_after_class(a),
                ),
                Some(a) => (base.to_string(), render_attrs_after_class(a)),
                None => (base.to_string(), String::new()),
            };
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
            out.push_str(&format!(
                "<span class=\"{}\"{}>{}</span>",
                escape_attr(&class),
                rest,
                body,
            ));
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
            out.push_str("<a href=\"#");
            write_escaped_attr(out, &c.target);
            out.push_str("\">");
            write_escaped_text(out, &c.target);
            out.push_str("</a>");
        }
        InlineNode::CaptionNumber(n) => {
            if let Some(number) = n.number {
                out.push_str(&number.to_string());
            }
        }
        InlineNode::Mention(m) => {
            if let Some(template) = &options.mention_url {
                let encoded = percent_encode(&m.user);
                let href = template
                    .replace("{name}", &encoded)
                    .replace("{user}", &encoded);
                out.push_str("<a class=\"mention\" href=\"");
                write_escaped_attr(out, &sanitize_url(&href));
                out.push_str("\">@");
                write_escaped_text(out, &m.user);
                out.push_str("</a>");
            } else {
                out.push_str("<span class=\"mention\"><strong>@");
                write_escaped_text(out, &m.user);
                out.push_str("</strong></span>");
            }
        }
        InlineNode::Tag(t) => {
            if let Some(template) = &options.tag_url {
                let encoded = percent_encode(&t.name);
                let href = template.replace("{name}", &encoded);
                out.push_str("<a class=\"tag\" href=\"");
                write_escaped_attr(out, &sanitize_url(&href));
                out.push_str("\">#");
                write_escaped_text(out, &t.name);
                out.push_str("</a>");
            } else {
                out.push_str("<span class=\"tag\"><strong>#");
                write_escaped_text(out, &t.name);
                out.push_str("</strong></span>");
            }
        }
        InlineNode::CitationGroup(g) => render_citation_group(out, g, options),
        InlineNode::Extension(e) => render_inline_extension(out, e, options),
        InlineNode::Abbreviation(a) => {
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
            render_inlines_stateful(out, &c.children, options);
            out.push_str("</ins>");
        }
        InlineNode::CriticDelete(c) => {
            out.push_str("<del");
            write_attrs(out, &c.attrs);
            out.push('>');
            render_inlines_stateful(out, &c.children, options);
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

fn render_citation_group(out: &mut String, g: &CitationGroup, options: &Options<'_>) {
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
                render_citation_item(out, item, options);
            }
            out.push(']');
        }
        CitationRenderMode::AuthorDate => {
            out.push('(');
            for (idx, item) in g.items.iter().enumerate() {
                if idx > 0 {
                    out.push_str("; ");
                }
                render_citation_item(out, item, options);
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
    let mut out = String::new();
    for node in nodes {
        match node {
            InlineNode::Text(t) => {
                out.push_str(&t.value.replace(crate::ESCAPED_CARET_PLACEHOLDER, "^"))
            }
            InlineNode::SmartPunctuation(s) => out.push_str(smart_punctuation_glyph(s)),
            InlineNode::Emphasis(e) => out.push_str(&flatten_text(&e.children)),
            InlineNode::Link(l) => out.push_str(&flatten_text(&l.children)),
            InlineNode::Span(s) => out.push_str(&flatten_text(&s.children)),
            InlineNode::Extension(e) => out.push_str(&flatten_text(&e.children)),
            _ => {}
        }
    }
    out
}

fn render_citation_item(out: &mut String, item: &Citation, options: &Options<'_>) {
    if let Some(prefix) = &item.prefix {
        render_inlines(out, prefix, options);
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
        render_inlines(out, locator, options);
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

fn render_emphasis(out: &mut String, e: &Emphasis, options: &Options<'_>) {
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
        render_inlines_stateful(out, &e.children, options);
        out.push_str(close);
    } else {
        out.push_str(&format!("<{}{}>", open, render_attrs(&e.attrs)));
        render_inlines_stateful(out, &e.children, options);
        out.push_str(&format!("</{}>", close));
    }
}

fn render_link(out: &mut String, l: &Link, options: &Options<'_>) {
    out.push_str(&format!(
        "<a href=\"{}\"{}",
        escape_attr(&sanitize_url(&l.href)),
        render_attrs_without_keys(&l.attrs, &["href"])
    ));
    if let Some(title) = &l.title {
        out.push_str(&format!(" title=\"{}\"", escape_attr(title)));
    }
    out.push('>');
    render_inlines_stateful(out, &l.children, options);
    out.push_str("</a>");
}

fn render_inline_extension(out: &mut String, node: &InlineExtension, options: &Options<'_>) {
    let ctx = RenderContext::new(options);
    for ext in &options.extensions {
        if let Some(html) = ext.render_inline_extension(node, &ctx) {
            out.push_str(&html);
            return;
        }
    }
    // Semantic shorthands: `:tag[content]` renders as the matching HTML element
    // (matches carve-js / carve-php). Any other name falls back to a generic
    // `<span class="ext-NAME">`.
    const SEMANTIC_TAGS: [&str; 9] = [
        "kbd", "dfn", "abbr", "cite", "samp", "var", "code", "mark", "time",
    ];
    if SEMANTIC_TAGS.contains(&node.name.as_str()) {
        out.push_str(&format!("<{}{}>", node.name, render_attrs(&node.attrs)));
        render_inlines_stateful(out, &node.children, options);
        out.push_str(&format!("</{}>", node.name));
        return;
    }
    // The `ext-NAME` class is structural and emitted first; a trailing
    // attribute block merges its classes into the SAME `class` attribute
    // (`:foo[a]{.cls}` -> `class="ext-foo cls"`, never two `class` attrs) and
    // contributes id / key-values after. Matches the math-span merge and
    // carve-js / carve-php.
    let base = format!("ext-{}", node.name);
    let (class, rest) = match &node.attrs {
        Some(a) if !a.classes.is_empty() => (
            dedup_class_str(&format!("{} {}", base, a.classes.join(" "))),
            render_attrs_after_class(a),
        ),
        Some(a) => (base, render_attrs_after_class(a)),
        None => (base, String::new()),
    };
    out.push_str(&format!("<span class=\"{}\"{}>", escape_attr(&class), rest));
    render_inlines_stateful(out, &node.children, options);
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
