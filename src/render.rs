//! HTML renderer — emits the canonical output the spec corpus expects.
//!
//! Output style matches `carve-js/render-html.ts`: block elements on
//! their own line; inline content flows within the block tag. Lists
//! indent their `<li>` children two spaces.

use crate::ast::*;
use crate::escape::{escape_attr, escape_text};
use crate::extension::{Options, RenderContext};
use std::collections::BTreeMap;

pub fn render_html(doc: &Document) -> String {
    render_html_with_options(doc, &Options::default())
}

pub fn render_html_with_options(doc: &Document, options: &Options<'_>) -> String {
    let mut doc = doc.clone();
    let mut state = RenderState::default();
    let footnotes = collect_footnotes(&mut doc);
    let mut html = render_document_blocks(doc.children.as_slice(), options, &mut state);
    if !footnotes.is_empty() {
        html.push('\n');
        html.push_str(&render_footnotes_section(
            &doc, &footnotes, options, &mut state,
        ));
    }
    html
}

// Entry point for `RenderContext::render_blocks` (the extension render helper).
// KNOWN LIMITATION: this starts a fresh heading-id counter, so headings inside
// blocks rendered by a custom block extension are numbered independently of the
// surrounding document (a collision can repeat an id). The common path -- the
// fallback `<div>` rendering in render_block_extension -- already threads the
// document `state`, so only an extension that explicitly re-renders child
// headings via this helper is affected. Sharing the live counter would require
// threading mutable state through the extension RenderContext, which is a
// cross-impl extension-API change (carve-js resolves heading ids in a separate
// pass that likewise does not descend into extension nodes); deferred until that
// is coordinated across implementations.
pub(crate) fn render_blocks_with_options(nodes: &[BlockNode], options: &Options<'_>) -> String {
    let mut state = RenderState::default();
    render_blocks(nodes, options, &mut state)
}

fn render_blocks(nodes: &[BlockNode], options: &Options<'_>, state: &mut RenderState) -> String {
    let mut out = String::new();
    let mut first = true;
    for block in nodes {
        if !first {
            out.push('\n');
        }
        render_block(&mut out, block, 0, options, state);
        first = false;
    }
    out
}

#[derive(Default)]
struct RenderState {
    heading_counts: BTreeMap<String, usize>,
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
        if !first {
            out.push('\n');
        }
        if matches!(nodes[i], BlockNode::Heading(_)) {
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
    let def_labels: Vec<String> = doc.footnote_defs.keys().cloned().collect();

    for block in &mut doc.children {
        collect_footnotes_block(block, &def_labels, &mut seen, &mut order);
    }

    let mut idx = 0;
    while idx < order.len() {
        let Some(label) = order[idx].label.clone() else {
            idx += 1;
            continue;
        };
        if let Some(blocks) = doc.footnote_defs.get_mut(&label) {
            for block in blocks {
                collect_footnotes_block(block, &def_labels, &mut seen, &mut order);
            }
        }
        idx += 1;
    }

    order
}

fn collect_footnotes_block(
    block: &mut BlockNode,
    def_labels: &[String],
    seen: &mut BTreeMap<String, usize>,
    order: &mut Vec<FootnoteEntry>,
) {
    match block {
        BlockNode::Heading(h) => collect_footnotes_inline(&mut h.children, def_labels, seen, order),
        BlockNode::Paragraph(p) => {
            collect_footnotes_inline(&mut p.children, def_labels, seen, order);
        }
        BlockNode::List(l) => {
            for item in &mut l.items {
                for child in &mut item.children {
                    collect_footnotes_block(child, def_labels, seen, order);
                }
            }
        }
        BlockNode::BlockQuote(b) => {
            if let Some(attribution) = &mut b.attribution {
                collect_footnotes_inline(attribution, def_labels, seen, order);
            }
            for child in &mut b.children {
                collect_footnotes_block(child, def_labels, seen, order);
            }
        }
        BlockNode::Table(t) => {
            if let Some(caption) = &mut t.caption {
                collect_footnotes_inline(caption, def_labels, seen, order);
            }
            for row in &mut t.rows {
                for cell in &mut row.cells {
                    collect_footnotes_inline(&mut cell.children, def_labels, seen, order);
                }
            }
        }
        BlockNode::Admonition(a) => {
            if let Some(title) = &mut a.title {
                collect_footnotes_inline(title, def_labels, seen, order);
            }
            for child in &mut a.children {
                collect_footnotes_block(child, def_labels, seen, order);
            }
        }
        BlockNode::Div(d) => {
            for child in &mut d.children {
                collect_footnotes_block(child, def_labels, seen, order);
            }
        }
        BlockNode::DefinitionList(d) => {
            for item in &mut d.items {
                for term in &mut item.terms {
                    collect_footnotes_inline(term, def_labels, seen, order);
                }
                for definition in &mut item.definitions {
                    for child in definition {
                        collect_footnotes_block(child, def_labels, seen, order);
                    }
                }
            }
        }
        BlockNode::Figure(f) => {
            collect_footnotes_inline(&mut f.caption, def_labels, seen, order);
            match &mut f.target {
                FigureTarget::BlockQuote(b) => {
                    if let Some(attribution) = &mut b.attribution {
                        collect_footnotes_inline(attribution, def_labels, seen, order);
                    }
                    for child in &mut b.children {
                        collect_footnotes_block(child, def_labels, seen, order);
                    }
                }
                FigureTarget::Table(t) => {
                    if let Some(caption) = &mut t.caption {
                        collect_footnotes_inline(caption, def_labels, seen, order);
                    }
                    for row in &mut t.rows {
                        for cell in &mut row.cells {
                            collect_footnotes_inline(&mut cell.children, def_labels, seen, order);
                        }
                    }
                }
                FigureTarget::Paragraph(p) => {
                    collect_footnotes_inline(&mut p.children, def_labels, seen, order);
                }
                FigureTarget::Image(_) | FigureTarget::CodeBlock(_) => {}
            }
        }
        BlockNode::Extension(e) => {
            for child in &mut e.children {
                collect_footnotes_block(child, def_labels, seen, order);
            }
        }
        _ => {}
    }
}

fn collect_footnotes_inline(
    nodes: &mut [InlineNode],
    def_labels: &[String],
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
                let idx = order
                    .iter()
                    .position(|entry| entry.label.as_ref() == Some(id))
                    .unwrap_or_else(|| {
                        order.push(FootnoteEntry {
                            label: Some(id.clone()),
                            inline: None,
                            backrefs: Vec::new(),
                        });
                        order.len() - 1
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
                collect_footnotes_inline(&mut e.children, def_labels, seen, order)
            }
            InlineNode::Link(l) => {
                collect_footnotes_inline(&mut l.children, def_labels, seen, order)
            }
            InlineNode::Span(s) => {
                collect_footnotes_inline(&mut s.children, def_labels, seen, order)
            }
            InlineNode::Extension(e) => {
                collect_footnotes_inline(&mut e.children, def_labels, seen, order)
            }
            InlineNode::CriticInsert(c) => {
                collect_footnotes_inline(&mut c.children, def_labels, seen, order);
            }
            InlineNode::CriticDelete(c) => {
                collect_footnotes_inline(&mut c.children, def_labels, seen, order);
            }
            _ => {}
        }
    }
}

fn render_footnotes_section(
    doc: &Document,
    footnotes: &[FootnoteEntry],
    options: &Options<'_>,
    state: &mut RenderState,
) -> String {
    let mut out = String::new();
    out.push_str("<section role=\"doc-endnotes\">\n  <hr>\n  <ol>");
    for (idx, entry) in footnotes.iter().enumerate() {
        let num = idx + 1;
        out.push('\n');
        out.push_str(&format!("    <li id=\"fn{}\">", num));
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
    match node {
        BlockNode::Heading(h) => render_heading(out, h, level, options, state),
        BlockNode::Paragraph(p) => render_paragraph(out, p, level, options),
        BlockNode::CodeBlock(c) => render_code_block(out, c, level),
        BlockNode::List(l) => render_list(out, l, level, options, state),
        BlockNode::BlockQuote(b) => render_blockquote(out, b, level, options, state),
        BlockNode::Table(t) => render_table(out, t, level, options),
        BlockNode::Admonition(a) => render_admonition(out, a, level, options, state),
        BlockNode::Div(d) => render_div(out, d, level, options, state),
        BlockNode::DefinitionList(d) => render_definition_list(out, d, level, options, state),
        BlockNode::Figure(f) => render_figure(out, f, level, options, state),
        BlockNode::AbbreviationDef(_) => {}
        BlockNode::RawBlock(r) => {
            if r.format == "html" {
                indent(out, level);
                out.push_str(&r.content);
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
            out.push_str(&format!("<hr{}>", render_attrs(&n.attrs)));
        }
    }
}

fn render_heading(
    out: &mut String,
    h: &Heading,
    level: usize,
    options: &Options<'_>,
    state: &mut RenderState,
) {
    // A heading rendered here is nested inside another block (list item,
    // blockquote, div, ...). Unlike a top-level heading it carries no
    // `<section>` wrapper, so it must emit its slug id directly on the tag
    // (matching carve-php). The id is allocated from the same document-order
    // counter as top-level headings, so duplicate slugs are numbered
    // consistently across nesting levels.
    let id = next_heading_id(h, state);
    indent(out, level);
    out.push_str(&format!("<h{} id=\"{}\"", h.level, escape_attr(&id)));
    out.push_str(&render_attrs_without_id(&h.attrs));
    out.push('>');
    render_inlines(out, &h.children, options);
    out.push_str(&format!("</h{}>", h.level));
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
    out.push_str(&format!("<h{}{}>", h.level, render_attrs(&attrs)));
    render_inlines(out, &h.children, options);
    out.push_str(&format!("</h{}>", h.level));
}

fn next_heading_id(h: &Heading, state: &mut RenderState) -> String {
    let base = h
        .attrs
        .as_ref()
        .and_then(|attrs| attrs.id.clone())
        .unwrap_or_else(|| slugify(&plain_inlines(&h.children)));
    let count = state.heading_counts.entry(base.clone()).or_insert(0);
    *count += 1;
    if *count == 1 {
        base
    } else {
        format!("{base}-{count}")
    }
}

fn plain_inlines(nodes: &[InlineNode]) -> String {
    let mut out = String::new();
    for node in nodes {
        match node {
            InlineNode::Text(s) => out.push_str(s),
            InlineNode::Emphasis(e) => out.push_str(&plain_inlines(&e.children)),
            InlineNode::Code(s, _) => out.push_str(s),
            InlineNode::Link(l) => out.push_str(&plain_inlines(&l.children)),
            InlineNode::Image(i) => out.push_str(&i.alt),
            InlineNode::Extension(e) => out.push_str(&plain_inlines(&e.children)),
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
            InlineNode::SoftBreak | InlineNode::HardBreak => out.push(' '),
            _ => {}
        }
    }
    out
}

fn slugify(text: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for ch in text.chars() {
        if ch.is_alphanumeric() {
            // Unicode-preserving, lowercased (GitHub-style): `Café` -> `café`,
            // `Über` -> `über`. ASCII-folding is opt-in, not the default.
            for lc in ch.to_lowercase() {
                out.push(lc);
            }
            last_dash = false;
        } else if !last_dash && !out.is_empty() {
            out.push('-');
            last_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        out = format!("s-{out}");
    }
    if out.is_empty() {
        "section".to_string()
    } else {
        out
    }
}

fn render_paragraph(out: &mut String, p: &Paragraph, level: usize, options: &Options<'_>) {
    indent(out, level);
    out.push_str(&format!("<p{}>", render_attrs(&p.attrs)));
    render_inlines(out, &p.children, options);
    out.push_str("</p>");
}

fn render_code_block(out: &mut String, c: &CodeBlock, level: usize) {
    indent(out, level);
    out.push_str(&format!("<pre{}><code", render_attrs(&c.attrs)));
    if let Some(lang) = &c.lang {
        out.push_str(&format!(" class=\"language-{}\"", lang));
    }
    out.push('>');
    out.push_str(&escape_text(&c.content));
    out.push_str("\n</code></pre>");
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
    let mut extra = render_attrs(&l.attrs);
    if l.ordered {
        if let Some(ol_type) = l.ol_type {
            let value = match ol_type {
                OrderedListType::LowerAlpha => "a",
                OrderedListType::UpperAlpha => "A",
                OrderedListType::LowerRoman => "i",
                OrderedListType::UpperRoman => "I",
            };
            extra.push_str(&format!(" type=\"{}\"", value));
        }
        if let Some(start) = l.start {
            extra.push_str(&format!(" start=\"{}\"", start));
        }
    }
    out.push_str(&format!("<{}{}>\n", tag, extra));
    for (i, item) in l.items.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        render_list_item(out, item, level + 1, l.tight, options, state);
    }
    out.push('\n');
    indent(out, level);
    out.push_str(&format!("</{}>", tag));
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
    out.push_str(&format!("<li{}>", render_attrs(&item.attrs)));
    let checkbox = match item.checked {
        None => "",
        Some(false) => "<input type=\"checkbox\" disabled> ",
        Some(true) => "<input type=\"checkbox\" checked disabled> ",
    };
    if tight && item.children.len() == 1 {
        if let BlockNode::Paragraph(p) = &item.children[0] {
            out.push_str(checkbox);
            render_inlines(out, &p.children, options);
            out.push_str("</li>");
            return;
        }
    }
    if tight && item.children.len() > 1 {
        if let BlockNode::Paragraph(p) = &item.children[0] {
            out.push_str(checkbox);
            render_inlines(out, &p.children, options);
            for child in item.children.iter().skip(1) {
                out.push('\n');
                render_block(out, child, level + 1, options, state);
            }
            out.push('\n');
            indent(out, level);
            out.push_str("</li>");
            return;
        }
    }
    if !tight && item.children.len() == 1 {
        if let BlockNode::Paragraph(p) = &item.children[0] {
            out.push_str("<p>");
            out.push_str(checkbox);
            render_inlines(out, &p.children, options);
            out.push_str("</p></li>");
            return;
        }
    }
    if !tight && item.children.len() > 1 {
        if let BlockNode::Paragraph(p) = &item.children[0] {
            out.push_str("<p>");
            out.push_str(checkbox);
            render_inlines(out, &p.children, options);
            out.push_str("</p>");
            for child in item.children.iter().skip(1) {
                out.push('\n');
                render_block(out, child, level + 1, options, state);
            }
            out.push('\n');
            indent(out, level);
            out.push_str("</li>");
            return;
        }
    }
    out.push('\n');
    out.push_str(checkbox);
    let mut first = true;
    for child in &item.children {
        if !first {
            out.push('\n');
        }
        render_block(out, child, level + 1, options, state);
        first = false;
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
            out.push_str(&format!("<blockquote{}><p>", render_attrs(&b.attrs)));
            render_inlines(out, &p.children, options);
            out.push_str("</p></blockquote>");
            return;
        }
    }
    out.push_str(&format!("<blockquote{}>\n", render_attrs(&b.attrs)));
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
    out.push_str(&format!("<table{}>", render_attrs(&t.attrs)));
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
    if has_header {
        out.push('\n');
        indent(out, level + 1);
        out.push_str("<thead>");
        for header in &t.rows[..header_count] {
            render_table_row(out, header, true, options);
        }
        out.push_str("</thead>");
    }
    // A header-only table (e.g. a GFM `| x |` + `|---|` with no body rows) emits
    // no <tbody>, matching carve-php.
    if body_start < t.rows.len() {
        out.push('\n');
        indent(out, level + 1);
        out.push_str("<tbody>");
        let rowspan_cols = compute_rowspans(t);
        for (row_idx, row) in t.rows.iter().enumerate().skip(body_start) {
            out.push('\n');
            indent(out, level + 2);
            render_table_body_row(out, row, row_idx, &rowspan_cols, t, options);
        }
        out.push('\n');
        indent(out, level + 1);
        out.push_str("</tbody>");
    }
    out.push('\n');
    indent(out, level);
    out.push_str("</table>");
}

fn render_table_row(out: &mut String, row: &TableRow, header_row: bool, options: &Options<'_>) {
    out.push_str("<tr>");
    for (col, cell) in row.cells.iter().enumerate() {
        let tag = if header_row || cell.header {
            "th"
        } else {
            "td"
        };
        let align = render_align_attr(cell.align.or_else(|| row_align(row, col)));
        let emitted: &[&str] = if align.is_empty() { &[] } else { &["style"] };
        out.push_str(&format!(
            "<{}{}{}>",
            tag,
            render_cell_author_attrs(&cell.attrs, emitted),
            align
        ));
        render_inlines(out, &cell.children, options);
        out.push_str(&format!("</{}>", tag));
    }
    out.push_str("</tr>");
}

fn render_table_body_row(
    out: &mut String,
    row: &TableRow,
    source_row_idx: usize,
    rowspan_cols: &BTreeMap<(usize, usize), usize>,
    table: &Table,
    options: &Options<'_>,
) {
    out.push_str("<tr>");
    let mut col = 0usize;
    for cell in &row.cells {
        if cell.span == Some(TableCellSpan::Rowspan) {
            col += 1;
            continue;
        }
        let mut attrs = String::new();
        let mut emitted: Vec<&str> = Vec::new();
        if let Some(span) = rowspan_cols.get(&(source_row_idx, col)) {
            attrs.push_str(&format!(" rowspan=\"{}\"", span));
            emitted.push("rowspan");
        }
        if cell.span == Some(TableCellSpan::Colspan) {
            col += 1;
            continue;
        }
        let colspan = following_colspans(row, col);
        if colspan > 1 {
            attrs.push_str(&format!(" colspan=\"{}\"", colspan));
            emitted.push("colspan");
        }
        if let Some(align) = cell.align.or_else(|| table_column_align(table, col)) {
            attrs.push_str(&align_attr(align));
            emitted.push("style");
        }
        // A `|=` cell in a body row is a row header: <th> inside <tbody>.
        let tag = if cell.header { "th" } else { "td" };
        out.push_str(&format!(
            "<{}{}{}>",
            tag,
            render_cell_author_attrs(&cell.attrs, &emitted),
            attrs
        ));
        render_inlines(out, &cell.children, options);
        out.push_str(&format!("</{}>", tag));
        col += colspan;
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

fn following_colspans(row: &TableRow, start: usize) -> usize {
    let mut span = 1;
    for cell in row.cells.iter().skip(start + 1) {
        if cell.span == Some(TableCellSpan::Colspan) {
            span += 1;
        } else {
            break;
        }
    }
    span
}

/// Maps the origin cell `(row, col)` of each rowspan to its span count. Resolved
/// by carrying the current chain origin down per column, so an all-`^` table is
/// O(cells) rather than O(rows^2) (each `^` previously walked up every prior row
/// and the result list was scanned linearly per marker).
fn compute_rowspans(t: &Table) -> BTreeMap<(usize, usize), usize> {
    let mut spans: BTreeMap<(usize, usize), usize> = BTreeMap::new();
    // Per column: the origin row of the current rowspan chain (the most recent
    // non-`^` cell above). A `^` extends that origin; a real cell starts a new
    // chain.
    let mut base_for_col: BTreeMap<usize, usize> = BTreeMap::new();
    for (row_idx, row) in t.rows.iter().enumerate() {
        for (col, cell) in row.cells.iter().enumerate() {
            if cell.span == Some(TableCellSpan::Rowspan) {
                if let Some(&base) = base_for_col.get(&col) {
                    *spans.entry((base, col)).or_insert(1) += 1;
                }
            } else {
                base_for_col.insert(col, row_idx);
            }
        }
    }
    spans
}

fn render_admonition(
    out: &mut String,
    a: &Admonition,
    level: usize,
    options: &Options<'_>,
    state: &mut RenderState,
) {
    let canonical = matches!(
        a.kind.as_str(),
        "note" | "tip" | "warning" | "danger" | "info" | "success" | "example" | "quote"
    );
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
            format!("{} {}", base, at.classes.join(" ")),
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
    for child in &a.children {
        out.push('\n');
        render_block(out, child, level + 1, options, state);
    }
    out.push('\n');
    indent(out, level);
    out.push_str(if canonical { "</aside>" } else { "</div>" });
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
            out.push_str("<dt>");
            render_inlines(out, term, options);
            out.push_str("</dt>");
        }
        for def in &item.definitions {
            for block in def {
                if let BlockNode::Paragraph(p) = block {
                    out.push('\n');
                    indent(out, level + 1);
                    out.push_str("<dd>");
                    render_inlines(out, &p.children, options);
                    out.push_str("</dd>");
                }
            }
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
    let ctx = RenderContext::new(options);
    for ext in &options.extensions {
        if let Some(html) = ext.render_block_extension(node, &ctx) {
            indent(out, level);
            out.push_str(&html);
            return;
        }
    }
    indent(out, level);
    out.push_str(&format!("<div class=\"{}\">", escape_attr(&node.name)));
    if !node.children.is_empty() {
        out.push('\n');
        let mut first = true;
        for child in &node.children {
            if !first {
                out.push('\n');
            }
            render_block(out, child, level + 1, options, state);
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
        escape_attr(&img.src),
        escape_attr(&img.alt)
    ));
    if let Some(title) = &img.title {
        out.push_str(&format!(" title=\"{}\"", escape_attr(title)));
    }
    out.push_str(&render_attrs(&img.attrs));
    out.push('>');
}

// ---- Inline ----

pub(crate) fn render_inlines_with_options(nodes: &[InlineNode], options: &Options<'_>) -> String {
    let mut out = String::new();
    render_inlines(&mut out, nodes, options);
    out
}

/// Running smart-quote state for one block. `"`/`'` toggle open/closed across
/// the WHOLE inline flow in document order -- including across emphasis, links,
/// spans, etc. -- so `"a /b" c/ d` closes the quote inside the emphasis. Reset
/// per block (a quote does not carry from one paragraph to the next); verbatim
/// nodes (code, math, raw) do not touch it.
struct SmartState {
    open_double: bool,
    open_single: bool,
}

impl Default for SmartState {
    fn default() -> Self {
        SmartState {
            open_double: true,
            open_single: true,
        }
    }
}

// Block-level entry: each block starts with a fresh quote state.
fn render_inlines(out: &mut String, nodes: &[InlineNode], options: &Options<'_>) {
    let mut state = SmartState::default();
    render_inlines_stateful(out, nodes, options, &mut state);
}

// Recursive entry: threads the running quote state so nested inline content
// (emphasis/link/span children) shares the parent's open/closed quote context.
fn render_inlines_stateful(
    out: &mut String,
    nodes: &[InlineNode],
    options: &Options<'_>,
    state: &mut SmartState,
) {
    let mut prev: Option<&InlineNode> = None;
    for node in nodes {
        render_inline_after(out, node, options, prev, state);
        prev = Some(node);
    }
}

/// Does an inline node end in visible, non-whitespace content? Used as the
/// flanking context for a `'`/`"` at the START of the following text node
/// (`@john's` -- the apostrophe is preceded by the mention, so it is a RIGHT
/// quote, not an opener). Breaks count as whitespace.
fn ends_non_whitespace(node: &InlineNode) -> bool {
    match node {
        InlineNode::SoftBreak | InlineNode::HardBreak => false,
        InlineNode::Text(s) => s.chars().last().is_some_and(|c| !c.is_whitespace()),
        _ => true,
    }
}

fn render_inline_after(
    out: &mut String,
    node: &InlineNode,
    options: &Options<'_>,
    prev: Option<&InlineNode>,
    state: &mut SmartState,
) {
    match node {
        InlineNode::Text(s) => {
            let prev_non_ws = prev.is_some_and(ends_non_whitespace);
            out.push_str(
                &escape_text(&smart_text_after(s, prev_non_ws, state))
                    .replace('\u{00a0}', "&nbsp;"),
            )
        }
        InlineNode::Emphasis(e) => render_emphasis(out, e, options, state),
        InlineNode::Code(s, attrs) => {
            out.push_str(&format!("<code{}>", render_attrs(attrs)));
            out.push_str(&escape_text(s));
            out.push_str("</code>");
        }
        InlineNode::Link(l) => render_link(out, l, options, state),
        InlineNode::Image(img) => render_image(out, img),
        InlineNode::Span(s) => {
            out.push_str(&format!("<span{}>", render_attrs(&s.attrs)));
            render_inlines_stateful(out, &s.children, options, state);
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
                    format!("{} {}", base, a.classes.join(" ")),
                    render_attrs_after_class(a),
                ),
                Some(a) => (base.to_string(), render_attrs_after_class(a)),
                None => (base.to_string(), String::new()),
            };
            out.push_str(&format!(
                "<span class=\"{}\"{}>{}{}</span>",
                escape_attr(&class),
                rest,
                open,
                escape_text(&m.content) + close
            ));
        }
        InlineNode::RawInline(r) => {
            if r.format.trim() == "html" {
                out.push_str(&r.content);
            }
        }
        InlineNode::Emoji(e) => {
            if let Some(value) = options.emoji.get(&e.name) {
                out.push_str(&escape_text(value));
            } else {
                out.push_str(&format!(":{}:", escape_text(&e.name)));
            }
        }
        InlineNode::AutoLink(a) => {
            let display = if let Some(stripped) = a.href.strip_prefix("mailto:") {
                stripped
            } else {
                &a.href
            };
            out.push_str(&format!(
                "<a href=\"{}\"{}>{}</a>",
                escape_attr(&a.href),
                render_attrs(&a.attrs),
                escape_text(display)
            ));
        }
        InlineNode::CrossRef(c) => out.push_str(&format!(
            "<a href=\"#{}\">{}</a>",
            escape_attr(&c.target),
            escape_text(&c.target)
        )),
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
                out.push_str(&format!(
                    "<a class=\"mention\" href=\"{}\">@{}</a>",
                    escape_attr(&href),
                    escape_text(&m.user)
                ));
            } else {
                out.push_str(&format!(
                    "<span class=\"mention\"><strong>@{}</strong></span>",
                    escape_text(&m.user)
                ));
            }
        }
        InlineNode::Tag(t) => {
            if let Some(template) = &options.tag_url {
                let encoded = percent_encode(&t.name);
                let href = template.replace("{name}", &encoded);
                out.push_str(&format!(
                    "<a class=\"tag\" href=\"{}\">#{}</a>",
                    escape_attr(&href),
                    escape_text(&t.name)
                ));
            } else {
                out.push_str(&format!(
                    "<span class=\"tag\"><strong>#{}</strong></span>",
                    escape_text(&t.name)
                ));
            }
        }
        InlineNode::Extension(e) => render_inline_extension(out, e, options, state),
        InlineNode::Abbreviation(a) => out.push_str(&format!(
            "<abbr title=\"{}\">{}</abbr>",
            escape_attr(&a.expansion),
            escape_text(&a.abbr)
        )),
        InlineNode::Footnote(f) => {
            if let (Some(number), Some(ref_id)) = (f.number, &f.ref_id) {
                out.push_str(&format!(
                    "<a id=\"{}\" href=\"#fn{}\" role=\"doc-noteref\"{}><sup>{}</sup></a>",
                    escape_attr(ref_id),
                    number,
                    render_attrs_without_id(&f.attrs),
                    number
                ));
            } else if let Some(id) = &f.id {
                out.push_str(&escape_text(&format!("[^{id}]")));
            }
        }
        InlineNode::SoftBreak => out.push('\n'),
        InlineNode::HardBreak => out.push_str("<br>\n"),
        InlineNode::CriticInsert(c) => {
            out.push_str("<ins>");
            render_inlines_stateful(out, &c.children, options, state);
            out.push_str("</ins>");
        }
        InlineNode::CriticDelete(c) => {
            out.push_str("<del>");
            render_inlines_stateful(out, &c.children, options, state);
            out.push_str("</del>");
        }
        InlineNode::CriticSubstitute(c) => out.push_str(&format!(
            "<del>{}</del><ins>{}</ins>",
            escape_text(&c.old_text),
            escape_text(&c.new_text)
        )),
        InlineNode::CriticComment(c) => out.push_str(&format!(
            "<span class=\"critic-comment\">{}</span>",
            escape_text(&c.text)
        )),
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

fn render_emphasis(out: &mut String, e: &Emphasis, options: &Options<'_>, state: &mut SmartState) {
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

fn render_link(out: &mut String, l: &Link, options: &Options<'_>, state: &mut SmartState) {
    out.push_str(&format!(
        "<a href=\"{}\"{}",
        escape_attr(&l.href),
        render_attrs(&l.attrs)
    ));
    if let Some(title) = &l.title {
        out.push_str(&format!(" title=\"{}\"", escape_attr(title)));
    }
    out.push('>');
    render_inlines_stateful(out, &l.children, options, state);
    out.push_str("</a>");
}

fn render_inline_extension(
    out: &mut String,
    node: &InlineExtension,
    options: &Options<'_>,
    state: &mut SmartState,
) {
    let ctx = RenderContext::new(options);
    for ext in &options.extensions {
        if let Some(html) = ext.render_inline_extension(node, &ctx) {
            out.push_str(&html);
            return;
        }
    }
    if node.name == "kbd" {
        out.push_str(&format!("<kbd{}>", render_attrs(&node.attrs)));
        render_inlines_stateful(out, &node.children, options, state);
        out.push_str("</kbd>");
        return;
    }
    out.push_str(&format!(
        "<span class=\"ext-{}\"{}>",
        escape_attr(&node.name),
        render_attrs(&node.attrs)
    ));
    render_inlines_stateful(out, &node.children, options, state);
    out.push_str("</span>");
}

fn render_attrs(attrs: &Option<Attrs>) -> String {
    let Some(attrs) = attrs else {
        return String::new();
    };
    let mut out = String::new();
    if attrs.order.is_empty() {
        if let Some(id) = &attrs.id {
            out.push_str(&format!(" id=\"{}\"", escape_attr(id)));
        }
        if !attrs.classes.is_empty() {
            out.push_str(&format!(
                " class=\"{}\"",
                escape_attr(&attrs.classes.join(" "))
            ));
        }
        for (key, value) in &attrs.key_values {
            out.push_str(&format!(" {}=\"{}\"", key, escape_attr(value)));
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
            AttrSlot::Class => {
                if !attrs.classes.is_empty() {
                    out.push_str(&format!(
                        " class=\"{}\"",
                        escape_attr(&attrs.classes.join(" "))
                    ));
                }
            }
            AttrSlot::Key(key) => {
                if let Some(value) = attrs.key_values.get(key) {
                    out.push_str(&format!(" {}=\"{}\"", key, escape_attr(value)));
                }
            }
        }
    }
    out
}

/// Render an attribute block's id and key-values in source order, omitting
/// the class slot. Used by a node whose class is structural and merged
/// separately (the math span: `class="math inline {extra}"`).
fn render_attrs_after_class(attrs: &Attrs) -> String {
    let mut out = String::new();
    if attrs.order.is_empty() {
        if let Some(id) = &attrs.id {
            out.push_str(&format!(" id=\"{}\"", escape_attr(id)));
        }
        for (key, value) in &attrs.key_values {
            out.push_str(&format!(" {}=\"{}\"", key, escape_attr(value)));
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
                    out.push_str(&format!(" {}=\"{}\"", key, escape_attr(value)));
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

/// Apply substring replacements, but never across an escape-guarded char (the
/// U+E000 guard marks the following char as an escaped literal, so it must not
/// participate in a smart-typography operator like `<=` or `->`).
fn apply_smart_ops(s: &str, replacements: &[(&str, &str)]) -> String {
    let mut out = String::new();
    let mut seg = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\u{e000}' {
            let mut replaced = seg.clone();
            for (from, to) in replacements {
                replaced = replaced.replace(from, to);
            }
            out.push_str(&replaced);
            seg.clear();
            out.push(c);
            if let Some(n) = chars.next() {
                out.push(n);
            }
        } else {
            seg.push(c);
        }
    }
    for (from, to) in replacements {
        seg = seg.replace(from, to);
    }
    out.push_str(&seg);
    out
}

fn smart_text_after(input: &str, prev_non_ws: bool, state: &mut SmartState) -> String {
    let s = unescape_text(input);
    let mut s = s;
    let replacements = [
        ("<->", "↔"),
        ("->", "→"),
        ("<-", "←"),
        ("=>", "⇒"),
        ("!=", "≠"),
        ("<=", "≤"),
        (">=", "≥"),
        ("+-", "±"),
        ("(c)", "©"),
        ("(r)", "®"),
        ("(tm)", "™"),
        ("------", "——"),
        ("-----", "—–"),
        ("----", "––"),
        ("---", "—"),
        ("--", "–"),
        ("...", "…"),
    ];
    // Apply the operator replacements only OUTSIDE escape-guarded chars, so an
    // escaped special (`\<= 5`, `\-> x`) does not form a smart-typography
    // operator. The U+E000 guard precedes each escaped char.
    s = apply_smart_ops(&s, &replacements);
    s = s.replace("&#NO_SMART_ARROW;", "->");
    s = s.replace("§NO_SMART_DOTS§", "...");
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::new();
    for (idx, ch) in chars.iter().copied().enumerate() {
        if ch == '\u{e000}' {
            continue;
        }
        let escaped = idx > 0 && chars[idx - 1] == '\u{e000}';
        if ch == '"' {
            if escaped {
                out.push(ch);
            } else {
                out.push(if state.open_double { '“' } else { '”' });
                state.open_double = !state.open_double;
            }
        } else if ch == '\'' {
            if escaped {
                out.push(ch);
                continue;
            }
            let prev_ws = if idx == 0 {
                !prev_non_ws
            } else {
                chars[idx - 1].is_whitespace()
            };
            let next_alpha = chars.get(idx + 1).is_some_and(|c| c.is_alphabetic());
            if prev_ws && next_alpha {
                out.push('‘');
                state.open_single = false;
            } else if !state.open_single {
                out.push('’');
                state.open_single = true;
            } else {
                out.push('’');
            }
        } else {
            out.push(ch);
        }
    }
    out
}

fn unescape_text(input: &str) -> String {
    let mut out = String::new();
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\\' && i + 1 < chars.len() {
            if chars[i + 1] == ' ' {
                out.push('\u{00a0}');
                i += 2;
                continue;
            }
            if chars[i + 1] == '-' && chars.get(i + 2) == Some(&'>') {
                out.push_str("&#NO_SMART_ARROW;");
                i += 3;
                continue;
            }
            if chars[i + 1] == '.' {
                let mut j = i + 1;
                let mut dots = 0usize;
                while chars.get(j) == Some(&'.') {
                    dots += 1;
                    j += 1;
                }
                if dots >= 3 {
                    out.push_str("§NO_SMART_DOTS§");
                } else {
                    for _ in 0..dots {
                        out.push('\u{e000}');
                        out.push('.');
                    }
                }
                i = j;
                continue;
            }
            if !is_render_escapable(chars[i + 1]) {
                out.push('\\');
                i += 1;
                continue;
            }
            out.push('\u{e000}');
            out.push(chars[i + 1]);
            i += 2;
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

fn is_render_escapable(ch: char) -> bool {
    matches!(
        ch,
        '\\' | '`'
            | '*'
            | '_'
            | '{'
            | '}'
            | '['
            | ']'
            | '('
            | ')'
            | '"'
            | '\''
            | '#'
            | '+'
            | '-'
            | '.'
            | '!'
            | '~'
            | '^'
            | '/'
            | '<'
            | '>'
            | '@'
            | '%'
            | '|'
            | '='
            | ','
            | ':'
            | ';'
            | '$'
            | '&'
            | '?'
    )
}
