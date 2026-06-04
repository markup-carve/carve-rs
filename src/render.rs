//! HTML renderer — emits the canonical output the spec corpus expects.
//!
//! Output style matches `carve-js/render-html.ts`: block elements on
//! their own line; inline content flows within the block tag. Lists
//! indent their `<li>` children two spaces.

use crate::ast::*;
use crate::escape::{escape_attr, escape_text};
use crate::extension::{Options, RenderContext};
use std::cell::RefCell;
use std::collections::BTreeMap;

thread_local! {
    static FOOTNOTE_NUMBERS: RefCell<BTreeMap<String, usize>> = const { RefCell::new(BTreeMap::new()) };
}

pub fn render_html(doc: &Document) -> String {
    render_html_with_options(doc, &Options::default())
}

pub fn render_html_with_options(doc: &Document, options: &Options<'_>) -> String {
    let mut state = RenderState::default();
    let refs = collect_footnote_refs(&doc.children);
    let numbers: BTreeMap<String, usize> = refs
        .iter()
        .enumerate()
        .map(|(idx, label)| (label.clone(), idx + 1))
        .collect();
    FOOTNOTE_NUMBERS.with(|cell| *cell.borrow_mut() = numbers);
    let mut html = render_document_blocks(doc.children.as_slice(), options, &mut state);
    if !refs.is_empty() {
        html.push('\n');
        html.push_str(&render_footnotes_section(doc, &refs, options));
    }
    FOOTNOTE_NUMBERS.with(|cell| cell.borrow_mut().clear());
    html
}

pub(crate) fn render_blocks_with_options(nodes: &[BlockNode], options: &Options<'_>) -> String {
    render_blocks(nodes, options)
}

fn render_blocks(nodes: &[BlockNode], options: &Options<'_>) -> String {
    let mut out = String::new();
    let mut first = true;
    for block in nodes {
        if !first {
            out.push('\n');
        }
        render_block(&mut out, block, 0, options);
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
            render_block(&mut out, &nodes[i], 0, options);
            i += 1;
        }
        first = false;
    }
    out
}

fn collect_footnote_refs(nodes: &[BlockNode]) -> Vec<String> {
    let mut refs = Vec::new();
    for block in nodes {
        collect_footnote_refs_block(block, &mut refs);
    }
    refs
}

fn collect_footnote_refs_block(block: &BlockNode, refs: &mut Vec<String>) {
    match block {
        BlockNode::Heading(h) => collect_footnote_refs_inline(&h.children, refs),
        BlockNode::Paragraph(p) => collect_footnote_refs_inline(&p.children, refs),
        BlockNode::List(l) => {
            for item in &l.items {
                for child in &item.children {
                    collect_footnote_refs_block(child, refs);
                }
            }
        }
        BlockNode::BlockQuote(b) => {
            for child in &b.children {
                collect_footnote_refs_block(child, refs);
            }
        }
        BlockNode::Table(t) => {
            for row in &t.rows {
                for cell in &row.cells {
                    collect_footnote_refs_inline(&cell.children, refs);
                }
            }
        }
        BlockNode::Admonition(a) => {
            for child in &a.children {
                collect_footnote_refs_block(child, refs);
            }
        }
        _ => {}
    }
}

fn collect_footnote_refs_inline(nodes: &[InlineNode], refs: &mut Vec<String>) {
    for node in nodes {
        match node {
            InlineNode::Footnote(f) => {
                if let Some(id) = &f.id {
                    if !refs.contains(id) {
                        refs.push(id.clone());
                    }
                }
            }
            InlineNode::Emphasis(e) => collect_footnote_refs_inline(&e.children, refs),
            InlineNode::Link(l) => collect_footnote_refs_inline(&l.children, refs),
            InlineNode::Span(s) => collect_footnote_refs_inline(&s.children, refs),
            InlineNode::Extension(e) => collect_footnote_refs_inline(&e.children, refs),
            _ => {}
        }
    }
}

fn render_footnotes_section(doc: &Document, refs: &[String], options: &Options<'_>) -> String {
    let mut out = String::new();
    out.push_str("<section role=\"doc-endnotes\">\n  <hr>\n  <ol>");
    for (idx, label) in refs.iter().enumerate() {
        let num = idx + 1;
        out.push('\n');
        out.push_str(&format!("    <li id=\"fn{}\">", num));
        if let Some(blocks) = doc.footnote_defs.get(label) {
            for (block_idx, block) in blocks.iter().enumerate() {
                out.push('\n');
                let mut rendered = String::new();
                render_block(&mut rendered, block, 3, options);
                if block_idx + 1 == blocks.len() {
                    let backlink = format!("<a href=\"#fnref{}\" role=\"doc-backlink\">↩</a>", num);
                    if let Some(pos) = rendered.rfind("</p>") {
                        rendered.insert_str(pos, &backlink);
                    } else {
                        rendered.push_str(&backlink);
                    }
                }
                out.push_str(&rendered);
            }
        }
        out.push('\n');
        out.push_str("    </li>");
    }
    out.push_str("\n  </ol>\n</section>");
    out
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
        render_block(out, &nodes[i], level + 1, options);
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

fn render_block(out: &mut String, node: &BlockNode, level: usize, options: &Options<'_>) {
    match node {
        BlockNode::Heading(h) => render_heading(out, h, level, options),
        BlockNode::Paragraph(p) => render_paragraph(out, p, level, options),
        BlockNode::CodeBlock(c) => render_code_block(out, c, level),
        BlockNode::List(l) => render_list(out, l, level, options),
        BlockNode::BlockQuote(b) => render_blockquote(out, b, level, options),
        BlockNode::Table(t) => render_table(out, t, level, options),
        BlockNode::Admonition(a) => render_admonition(out, a, level, options),
        BlockNode::Div(d) => render_div(out, d, level, options),
        BlockNode::DefinitionList(d) => render_definition_list(out, d, level, options),
        BlockNode::Figure(f) => render_figure(out, f, level, options),
        BlockNode::AbbreviationDef(_) => {}
        BlockNode::RawBlock(r) => {
            if r.format == "html" {
                indent(out, level);
                out.push_str(&r.content);
            }
        }
        BlockNode::Comment(_) => {}
        BlockNode::Extension(e) => render_block_extension(out, e, level, options),
        BlockNode::BlockImage(img) => {
            indent(out, level);
            render_image(out, img);
        }
        BlockNode::ThematicBreak => {
            indent(out, level);
            out.push_str("<hr>");
        }
    }
}

fn render_heading(out: &mut String, h: &Heading, level: usize, options: &Options<'_>) {
    indent(out, level);
    out.push_str(&format!("<h{}{}>", h.level, render_attrs(&h.attrs)));
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
            InlineNode::Code(s) => out.push_str(s),
            InlineNode::Link(l) => out.push_str(&plain_inlines(&l.children)),
            InlineNode::Image(i) => out.push_str(&i.alt),
            InlineNode::Extension(e) => out.push_str(&plain_inlines(&e.children)),
            InlineNode::Abbreviation(a) => out.push_str(&a.abbr),
            InlineNode::Mention(m) => out.push_str(&m.user),
            InlineNode::Tag(t) => out.push_str(&t.name),
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
        let mapped = match ch {
            'À' | 'Á' | 'Â' | 'Ã' | 'Ä' | 'Å' | 'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' => {
                'a'
            }
            'Ç' | 'ç' => 'c',
            'È' | 'É' | 'Ê' | 'Ë' | 'è' | 'é' | 'ê' | 'ë' => 'e',
            'Ì' | 'Í' | 'Î' | 'Ï' | 'ì' | 'í' | 'î' | 'ï' => 'i',
            'Ñ' | 'ñ' => 'n',
            'Ò' | 'Ó' | 'Ô' | 'Õ' | 'Ö' | 'ò' | 'ó' | 'ô' | 'õ' | 'ö' => 'o',
            'Ù' | 'Ú' | 'Û' | 'Ü' | 'ù' | 'ú' | 'û' | 'ü' => 'u',
            'Ý' | 'Ÿ' | 'ý' | 'ÿ' => 'y',
            'ß' => 's',
            c if c.is_ascii_alphanumeric() => c.to_ascii_lowercase(),
            _ => '-',
        };
        if mapped == '-' {
            if !last_dash && !out.is_empty() {
                out.push('-');
                last_dash = true;
            }
        } else {
            out.push(mapped);
            last_dash = false;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        out = format!("section-{out}");
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

fn render_list(out: &mut String, l: &List, level: usize, options: &Options<'_>) {
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
        render_list_item(out, item, level + 1, l.tight, options);
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
) {
    indent(out, level);
    out.push_str("<li>");
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
                render_block(out, child, level + 1, options);
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
                render_block(out, child, level + 1, options);
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
        render_block(out, child, level + 1, options);
        first = false;
    }
    out.push('\n');
    indent(out, level);
    out.push_str("</li>");
}

fn render_blockquote(out: &mut String, b: &BlockQuote, level: usize, options: &Options<'_>) {
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
        render_block(out, child, level + 1, options);
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
    let has_header = t
        .rows
        .first()
        .is_some_and(|row| row.cells.iter().any(|cell| cell.header));
    let body_start = if has_header { 1 } else { 0 };
    if has_header {
        let header = &t.rows[0];
        out.push('\n');
        indent(out, level + 1);
        out.push_str("<thead>");
        render_table_row(out, header, true, options);
        out.push_str("</thead>");
    }
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
        out.push_str(&format!(
            "<{}{}>",
            tag,
            render_align_attr(cell.align.or_else(|| row_align(row, col)))
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
    rowspan_cols: &[(usize, usize, usize)],
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
        if let Some((_, _, span)) = rowspan_cols
            .iter()
            .find(|(r, c, _)| *r == source_row_idx && *c == col)
        {
            attrs.push_str(&format!(" rowspan=\"{}\"", span));
        }
        if cell.span == Some(TableCellSpan::Colspan) {
            col += 1;
            continue;
        }
        let colspan = following_colspans(row, col);
        if colspan > 1 {
            attrs.push_str(&format!(" colspan=\"{}\"", colspan));
        }
        if let Some(align) = cell.align.or_else(|| table_column_align(table, col)) {
            attrs.push_str(&align_attr(align));
        }
        out.push_str(&format!("<td{}>", attrs));
        render_inlines(out, &cell.children, options);
        out.push_str("</td>");
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

fn compute_rowspans(t: &Table) -> Vec<(usize, usize, usize)> {
    let mut out = Vec::new();
    for row_idx in 1..t.rows.len() {
        for col in 0..t.rows[row_idx].cells.len() {
            if t.rows[row_idx].cells[col].span == Some(TableCellSpan::Rowspan) {
                let mut base = row_idx;
                while base > 0
                    && t.rows[base].cells.get(col).and_then(|c| c.span)
                        == Some(TableCellSpan::Rowspan)
                {
                    base -= 1;
                }
                let span = out.iter_mut().find(|(r, c, _)| *r == base && *c == col);
                if let Some((_, _, existing)) = span {
                    *existing += 1;
                } else {
                    out.push((base, col, 2));
                }
            }
        }
    }
    out
}

fn render_admonition(out: &mut String, a: &Admonition, level: usize, options: &Options<'_>) {
    let canonical = matches!(
        a.kind.as_str(),
        "note" | "tip" | "warning" | "danger" | "info" | "success" | "example" | "quote"
    );
    indent(out, level);
    if canonical {
        out.push_str(&format!(
            "<aside class=\"admonition {}\">",
            escape_attr(&a.kind)
        ));
    } else {
        out.push_str(&format!("<div class=\"{}\">", escape_attr(&a.kind)));
    }
    if let Some(title) = &a.title {
        out.push('\n');
        indent(out, level + 1);
        out.push_str("<p class=\"admonition-title\">");
        render_inlines(out, title, options);
        out.push_str("</p>");
    }
    for child in &a.children {
        out.push('\n');
        render_block(out, child, level + 1, options);
    }
    out.push('\n');
    indent(out, level);
    out.push_str(if canonical { "</aside>" } else { "</div>" });
}

fn render_div(out: &mut String, d: &Div, level: usize, options: &Options<'_>) {
    indent(out, level);
    out.push_str(&format!("<div{}>", render_attrs(&d.attrs)));
    for child in &d.children {
        out.push('\n');
        render_block(out, child, level + 1, options);
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

fn render_figure(out: &mut String, f: &Figure, level: usize, options: &Options<'_>) {
    indent(out, level);
    out.push_str(&format!("<figure{}>", render_attrs(&f.attrs)));
    out.push('\n');
    match &f.target {
        FigureTarget::Image(img) => {
            indent(out, level + 1);
            render_image(out, img);
        }
        FigureTarget::BlockQuote(b) => render_blockquote(out, b, level + 1, options),
        FigureTarget::Table(t) => render_table(out, t, level + 1, options),
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
            render_block(out, child, level + 1, options);
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

fn render_inlines(out: &mut String, nodes: &[InlineNode], options: &Options<'_>) {
    for node in nodes {
        render_inline(out, node, options);
    }
}

fn render_inline(out: &mut String, node: &InlineNode, options: &Options<'_>) {
    match node {
        InlineNode::Text(s) => {
            out.push_str(&escape_text(&smart_text(s)).replace('\u{00a0}', "&nbsp;"))
        }
        InlineNode::Emphasis(e) => render_emphasis(out, e, options),
        InlineNode::Code(s) => {
            out.push_str("<code>");
            out.push_str(&escape_text(s));
            out.push_str("</code>");
        }
        InlineNode::Link(l) => render_link(out, l, options),
        InlineNode::Image(img) => render_image(out, img),
        InlineNode::Span(s) => {
            out.push_str(&format!("<span{}>", render_attrs(&s.attrs)));
            render_inlines(out, &s.children, options);
            out.push_str("</span>");
        }
        InlineNode::Math(m) => {
            let class = if m.display {
                "math display"
            } else {
                "math inline"
            };
            let open = if m.display { "\\[" } else { "\\(" };
            let close = if m.display { "\\]" } else { "\\)" };
            out.push_str(&format!(
                "<span class=\"{}\">{}{}</span>",
                class,
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
        InlineNode::Extension(e) => render_inline_extension(out, e, options),
        InlineNode::Abbreviation(a) => out.push_str(&format!(
            "<abbr title=\"{}\">{}</abbr>",
            escape_attr(&a.expansion),
            escape_text(&a.abbr)
        )),
        InlineNode::Footnote(f) => {
            if let Some(id) = &f.id {
                FOOTNOTE_NUMBERS.with(|cell| {
                    if let Some(num) = cell.borrow().get(id).copied() {
                        out.push_str(&format!(
                            "<a id=\"fnref{}\" href=\"#fn{}\" role=\"doc-noteref\"><sup>{}</sup></a>",
                            num, num, num
                        ));
                    } else {
                        out.push_str(&escape_text(&format!("[^{}]", id)));
                    }
                });
            }
        }
        InlineNode::SoftBreak => out.push('\n'),
        InlineNode::HardBreak => out.push_str("<br>\n"),
        InlineNode::CriticInsert(c) => {
            out.push_str("<ins>");
            render_inlines(out, &c.children, options);
            out.push_str("</ins>");
        }
        InlineNode::CriticDelete(c) => {
            out.push_str("<del>");
            render_inlines(out, &c.children, options);
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
        render_inlines(out, &e.children, options);
        out.push_str(close);
    } else {
        out.push_str(&format!("<{}{}>", open, render_attrs(&e.attrs)));
        render_inlines(out, &e.children, options);
        out.push_str(&format!("</{}>", close));
    }
}

fn render_link(out: &mut String, l: &Link, options: &Options<'_>) {
    out.push_str(&format!(
        "<a href=\"{}\"{}",
        escape_attr(&l.href),
        render_attrs(&l.attrs)
    ));
    if let Some(title) = &l.title {
        out.push_str(&format!(" title=\"{}\"", escape_attr(title)));
    }
    out.push('>');
    render_inlines(out, &l.children, options);
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
    if node.name == "kbd" {
        out.push_str(&format!("<kbd{}>", render_attrs(&node.attrs)));
        render_inlines(out, &node.children, options);
        out.push_str("</kbd>");
        return;
    }
    out.push_str(&format!(
        "<span class=\"ext-{}\"{}>",
        escape_attr(&node.name),
        render_attrs(&node.attrs)
    ));
    render_inlines(out, &node.children, options);
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

fn smart_text(input: &str) -> String {
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
    for (from, to) in replacements {
        s = s.replace(from, to);
    }
    s = s.replace("&#NO_SMART_ARROW;", "->");
    s = s.replace("§NO_SMART_DOTS§", "...");
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::new();
    let mut open_double = true;
    let mut open_single = true;
    for (idx, ch) in chars.iter().copied().enumerate() {
        if ch == '\u{e000}' {
            continue;
        }
        let escaped = idx > 0 && chars[idx - 1] == '\u{e000}';
        if ch == '"' {
            if escaped {
                out.push(ch);
            } else {
                out.push(if open_double { '“' } else { '”' });
                open_double = !open_double;
            }
        } else if ch == '\'' {
            if escaped {
                out.push(ch);
                continue;
            }
            let prev_ws = idx == 0 || chars[idx - 1].is_whitespace();
            let next_alpha = chars.get(idx + 1).is_some_and(|c| c.is_alphabetic());
            if prev_ws && next_alpha {
                out.push('‘');
                open_single = false;
            } else if !open_single {
                out.push('’');
                open_single = true;
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
