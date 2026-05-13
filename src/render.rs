//! HTML renderer — emits the canonical output the spec corpus expects.
//!
//! Output style matches `carve-js/render-html.ts`: block elements on
//! their own line; inline content flows within the block tag. Lists
//! indent their `<li>` children two spaces.

use crate::ast::{
    BlockNode, BlockQuote, CodeBlock, Document, Emphasis, EmphasisKind, Heading, Image, InlineNode,
    Link, List, ListItem, Paragraph,
};
use crate::escape::{escape_attr, escape_text};

pub fn render_html(doc: &Document) -> String {
    let mut out = String::new();
    let mut first = true;
    for block in &doc.children {
        if !first {
            out.push('\n');
        }
        render_block(&mut out, block, 0);
        first = false;
    }
    out
}

fn indent(out: &mut String, level: usize) {
    for _ in 0..level {
        out.push_str("  ");
    }
}

fn render_block(out: &mut String, node: &BlockNode, level: usize) {
    match node {
        BlockNode::Heading(h) => render_heading(out, h, level),
        BlockNode::Paragraph(p) => render_paragraph(out, p, level),
        BlockNode::CodeBlock(c) => render_code_block(out, c, level),
        BlockNode::List(l) => render_list(out, l, level),
        BlockNode::BlockQuote(b) => render_blockquote(out, b, level),
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

fn render_heading(out: &mut String, h: &Heading, level: usize) {
    indent(out, level);
    out.push_str(&format!("<h{}>", h.level));
    render_inlines(out, &h.children);
    out.push_str(&format!("</h{}>", h.level));
}

fn render_paragraph(out: &mut String, p: &Paragraph, level: usize) {
    indent(out, level);
    out.push_str("<p>");
    render_inlines(out, &p.children);
    out.push_str("</p>");
}

fn render_code_block(out: &mut String, c: &CodeBlock, level: usize) {
    indent(out, level);
    out.push_str("<pre><code");
    if let Some(lang) = &c.lang {
        out.push_str(&format!(" class=\"language-{}\"", lang));
    }
    out.push('>');
    out.push_str(&escape_text(&c.content));
    out.push_str("\n</code></pre>");
}

fn render_list(out: &mut String, l: &List, level: usize) {
    indent(out, level);
    let tag = if l.ordered { "ol" } else { "ul" };
    out.push_str(&format!("<{}>\n", tag));
    for (i, item) in l.items.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        render_list_item(out, item, level + 1);
    }
    out.push('\n');
    indent(out, level);
    out.push_str(&format!("</{}>", tag));
}

fn render_list_item(out: &mut String, item: &ListItem, level: usize) {
    indent(out, level);
    out.push_str("<li>");
    let checkbox = match item.checked {
        None => "",
        Some(false) => "<input type=\"checkbox\" disabled> ",
        Some(true) => "<input type=\"checkbox\" checked disabled> ",
    };
    if item.children.len() == 1 {
        if let BlockNode::Paragraph(p) = &item.children[0] {
            out.push_str(checkbox);
            render_inlines(out, &p.children);
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
        render_block(out, child, level + 1);
        first = false;
    }
    out.push('\n');
    indent(out, level);
    out.push_str("</li>");
}

fn render_blockquote(out: &mut String, b: &BlockQuote, level: usize) {
    indent(out, level);
    if b.children.len() == 1 {
        if let BlockNode::Paragraph(p) = &b.children[0] {
            out.push_str("<blockquote><p>");
            render_inlines(out, &p.children);
            out.push_str("</p></blockquote>");
            return;
        }
    }
    out.push_str("<blockquote>\n");
    let mut first = true;
    for child in &b.children {
        if !first {
            out.push('\n');
        }
        render_block(out, child, level + 1);
        first = false;
    }
    out.push('\n');
    indent(out, level);
    out.push_str("</blockquote>");
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

fn render_inlines(out: &mut String, nodes: &[InlineNode]) {
    for node in nodes {
        render_inline(out, node);
    }
}

fn render_inline(out: &mut String, node: &InlineNode) {
    match node {
        InlineNode::Text(s) => out.push_str(&escape_text(s)),
        InlineNode::Emphasis(e) => render_emphasis(out, e),
        InlineNode::Code(s) => {
            out.push_str("<code>");
            out.push_str(&escape_text(s));
            out.push_str("</code>");
        }
        InlineNode::Link(l) => render_link(out, l),
        InlineNode::Image(img) => render_image(out, img),
        InlineNode::SoftBreak => out.push('\n'),
    }
}

fn render_emphasis(out: &mut String, e: &Emphasis) {
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
    render_inlines(out, &e.children);
    out.push_str(close);
}

fn render_link(out: &mut String, l: &Link) {
    out.push_str(&format!("<a href=\"{}\"", escape_attr(&l.href)));
    if let Some(title) = &l.title {
        out.push_str(&format!(" title=\"{}\"", escape_attr(title)));
    }
    out.push('>');
    render_inlines(out, &l.children);
    out.push_str("</a>");
}
