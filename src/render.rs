//! HTML renderer — emits the canonical output the spec corpus expects.
//!
//! Output style matches `carve-js/render-html.ts`: block elements on
//! their own line; inline content flows within the block tag. Lists
//! indent their `<li>` children two spaces.

use crate::ast::*;
use crate::escape::{escape_attr, escape_text};
use crate::extension::{Options, RenderContext};

pub fn render_html(doc: &Document) -> String {
    render_html_with_options(doc, &Options::default())
}

pub fn render_html_with_options(doc: &Document, options: &Options<'_>) -> String {
    render_blocks(doc.children.as_slice(), options)
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
        BlockNode::DefinitionList(_) => {}
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
    out.push_str(&format!("<{}{}>\n", tag, render_attrs(&l.attrs)));
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
    let (thead, body) = t
        .rows
        .split_first()
        .map_or((&[][..], &[][..]), |(first, rest)| {
            (std::slice::from_ref(first), rest)
        });
    if let Some(header) = thead.first() {
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
    for (row_idx, row) in body.iter().enumerate() {
        out.push('\n');
        indent(out, level + 2);
        render_table_body_row(out, row, row_idx + 1, &rowspan_cols, options);
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
    for cell in &row.cells {
        let tag = if header_row || cell.header {
            "th"
        } else {
            "td"
        };
        out.push_str(&format!("<{}>", tag));
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
        out.push_str(&format!("<td{}>", attrs));
        render_inlines(out, &cell.children, options);
        out.push_str("</td>");
        col += colspan;
    }
    out.push_str("</tr>");
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
            if t.rows[row_idx].cells[col].span == Some(TableCellSpan::Rowspan) && row_idx > 1 {
                out.push((row_idx - 1, col, 2));
            }
        }
    }
    out
}

fn render_admonition(out: &mut String, a: &Admonition, level: usize, options: &Options<'_>) {
    indent(out, level);
    out.push_str(&format!(
        "<aside class=\"admonition {}\">",
        escape_attr(&a.kind)
    ));
    for child in &a.children {
        out.push('\n');
        render_block(out, child, level + 1, options);
    }
    out.push('\n');
    indent(out, level);
    out.push_str("</aside>");
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
        InlineNode::Text(s) => out.push_str(&escape_text(s)),
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
            if r.format == "html" {
                out.push_str(&r.content);
            }
        }
        InlineNode::Emoji(e) => out.push_str(&format!(":{}:", escape_text(&e.name))),
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
        InlineNode::Mention(m) => out.push_str(&format!(
            "<a class=\"mention\" href=\"/users/{}\">@{}</a>",
            escape_attr(&m.user),
            escape_text(&m.user)
        )),
        InlineNode::Tag(t) => out.push_str(&format!(
            "<a class=\"tag\" href=\"/tags/{}\">#{}</a>",
            escape_attr(&t.name),
            escape_text(&t.name)
        )),
        InlineNode::Extension(e) => render_inline_extension(out, e, options),
        InlineNode::Abbreviation(a) => out.push_str(&format!(
            "<abbr title=\"{}\">{}</abbr>",
            escape_attr(&a.expansion),
            escape_text(&a.abbr)
        )),
        InlineNode::Footnote(_) => {}
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
        InlineNode::CriticComment(_) => {}
    }
}

fn render_emphasis(out: &mut String, e: &Emphasis, options: &Options<'_>) {
    let (open, close) = match e.kind {
        EmphasisKind::Italic => ("<em>", "</em>"),
        EmphasisKind::Strong => ("<strong>", "</strong>"),
        EmphasisKind::Underline => ("<u>", "</u>"),
        EmphasisKind::Strike => ("<s>", "</s>"),
        EmphasisKind::Super => ("<sup>", "</sup>"),
        EmphasisKind::Sub => ("<sub>", "</sub>"),
        EmphasisKind::Highlight => ("<mark>", "</mark>"),
        EmphasisKind::BoldItalic => ("<strong><em>", "</em></strong>"),
    };
    out.push_str(open);
    render_inlines(out, &e.children, options);
    out.push_str(close);
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
        out.push_str("<kbd>");
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
    if !attrs.classes.is_empty() {
        out.push_str(&format!(
            " class=\"{}\"",
            escape_attr(&attrs.classes.join(" "))
        ));
    }
    if let Some(id) = &attrs.id {
        out.push_str(&format!(" id=\"{}\"", escape_attr(id)));
    }
    for (key, value) in &attrs.key_values {
        out.push_str(&format!(" {}=\"{}\"", key, escape_attr(value)));
    }
    out
}
