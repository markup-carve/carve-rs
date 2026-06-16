//! Generate a table of contents from the document's top-level headings.
//!
//! Port of carve-js `table-of-contents.ts` / carve-php's
//! `TableOfContentsExtension`. A `before_render` transform that collects
//! top-level headings (with their resolved ids), builds a nested `<nav>` of
//! links, and injects it at the top or bottom of the document as a raw HTML
//! block.
//!
//! Heading id resolution mirrors the renderer (case-preserving by default; see
//! [`crate::Options::lowercase_heading_ids`]). To match a document rendered
//! with `with_lowercase_heading_ids(true)`, set
//! [`TableOfContentsOptions::lowercase_ids`] to the same value.

use std::collections::BTreeMap;

use crate::ast::{BlockNode, Document, Heading, InlineNode, RawBlock};
use crate::extension::CarveExtension;

/// List element for the TOC entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListType {
    /// `<ul>`
    Ul,
    /// `<ol>`
    Ol,
}

impl ListType {
    fn tag(self) -> &'static str {
        match self {
            ListType::Ul => "ul",
            ListType::Ol => "ol",
        }
    }
}

/// Where to insert the generated TOC.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Position {
    /// Top of the document.
    Top,
    /// Bottom of the document.
    Bottom,
}

/// Options for [`TableOfContents`].
#[derive(Debug, Clone)]
pub struct TableOfContentsOptions {
    /// Lowest heading level to include (1-6). Default 1.
    pub min_level: u8,
    /// Highest heading level to include (1-6). Default 6.
    pub max_level: u8,
    /// List element for the entries. Default [`ListType::Ul`].
    pub list_type: ListType,
    /// CSS class on the `<nav>` container. Default `"toc"`.
    pub css_class: String,
    /// Insert the generated TOC at the top or bottom. Default [`Position::Top`].
    pub position: Position,
    /// Lowercase auto-generated heading ids when computing link targets. Must
    /// match the renderer's [`crate::Options::lowercase_heading_ids`]. Default
    /// false (case-preserving), matching the carve-rs default.
    pub lowercase_ids: bool,
}

impl Default for TableOfContentsOptions {
    fn default() -> Self {
        Self {
            min_level: 1,
            max_level: 6,
            list_type: ListType::Ul,
            css_class: "toc".into(),
            position: Position::Top,
            lowercase_ids: false,
        }
    }
}

struct TocEntry {
    level: u8,
    text: String,
    id: String,
}

/// Generate a table of contents from the document's headings.
///
/// ```
/// use carve::{TableOfContents, TableOfContentsOptions, Options};
/// let opts = TableOfContentsOptions { lowercase_ids: true, ..Default::default() };
/// let ext = TableOfContents::with_options(opts);
/// let options = Options::new()
///     .with_extension(&ext)
///     .with_lowercase_heading_ids(true);
/// let html = carve::to_html_with_options("# Intro\n\n## Details", &options);
/// assert!(html.starts_with("<nav class=\"toc\">"));
/// ```
pub struct TableOfContents {
    opts: TableOfContentsOptions,
}

impl TableOfContents {
    /// Create a TOC extension with default options.
    pub fn new() -> Self {
        Self::with_options(TableOfContentsOptions::default())
    }

    /// Create a TOC extension with explicit options.
    pub fn with_options(opts: TableOfContentsOptions) -> Self {
        Self { opts }
    }
}

impl Default for TableOfContents {
    fn default() -> Self {
        Self::new()
    }
}

impl CarveExtension for TableOfContents {
    fn name(&self) -> &'static str {
        "table-of-contents"
    }

    fn before_render(&self, mut doc: Document) -> Document {
        // The renderer allocates ids over ALL headings in document order
        // (including nested ones). Reproduce that counter so a top-level
        // heading's link target matches the `<section id>` the core emits.
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        let mut entries: Vec<TocEntry> = Vec::new();
        collect_entries(&doc.children, &mut counts, &self.opts, &mut entries, true);

        if entries.is_empty() {
            return doc;
        }

        let html = format!(
            "<nav class=\"{}\">{}</nav>",
            escape_html(&self.opts.css_class),
            build_list(&entries, self.opts.list_type),
        );
        let toc = BlockNode::RawBlock(RawBlock {
            format: "html".into(),
            content: html,
        });
        match self.opts.position {
            Position::Top => doc.children.insert(0, toc),
            Position::Bottom => doc.children.push(toc),
        }
        doc
    }
}

/// Walk every heading in document order to keep the id counter aligned with the
/// renderer, but only record TOC entries for TOP-LEVEL headings (matching
/// carve-js, which iterates `doc.children` only).
fn collect_entries(
    blocks: &[BlockNode],
    counts: &mut BTreeMap<String, usize>,
    opts: &TableOfContentsOptions,
    entries: &mut Vec<TocEntry>,
    top_level: bool,
) {
    for block in blocks {
        match block {
            BlockNode::Heading(h) => {
                let id = next_id(h, counts, opts.lowercase_ids);
                if top_level && h.level >= opts.min_level && h.level <= opts.max_level {
                    entries.push(TocEntry {
                        level: h.level,
                        text: plain_text(&h.children),
                        id,
                    });
                }
            }
            BlockNode::List(l) => {
                for item in &l.items {
                    collect_entries(&item.children, counts, opts, entries, false);
                }
            }
            BlockNode::BlockQuote(b) => collect_entries(&b.children, counts, opts, entries, false),
            BlockNode::Admonition(a) => collect_entries(&a.children, counts, opts, entries, false),
            BlockNode::Div(d) => collect_entries(&d.children, counts, opts, entries, false),
            BlockNode::Extension(e) => collect_entries(&e.children, counts, opts, entries, false),
            BlockNode::DefinitionList(dl) => {
                for item in &dl.items {
                    for def in &item.definitions {
                        collect_entries(def, counts, opts, entries, false);
                    }
                }
            }
            _ => {}
        }
    }
}

fn next_id(h: &Heading, counts: &mut BTreeMap<String, usize>, lowercase: bool) -> String {
    let base = h
        .attrs
        .as_ref()
        .and_then(|a| a.id.clone())
        .unwrap_or_else(|| crate::parse::slugify_parse(&plain_text(&h.children), lowercase));
    let count = counts.entry(base.clone()).or_insert(0);
    *count += 1;
    if *count == 1 {
        base
    } else {
        format!("{base}-{count}")
    }
}

/// Build a nested list from a flat, document-order entry list. Mirrors the
/// carve-js stack walk exactly: never pop the root list, open one nested list
/// when going deeper, emit a sibling at the same level, and a heading shallower
/// than its predecessor but deeper than an ancestor stays nested.
fn build_list(entries: &[TocEntry], list_type: ListType) -> String {
    let tag = list_type.tag();
    let mut html = String::new();
    let mut open: Vec<u8> = Vec::new();
    for e in entries {
        if open.is_empty() {
            html.push('<');
            html.push_str(tag);
            html.push('>');
            open.push(e.level);
        } else {
            while open.len() > 1 && *open.last().expect("non-empty") > e.level {
                html.push_str(&format!("</li></{tag}>"));
                open.pop();
            }
            let last = *open.last().expect("non-empty");
            if last < e.level {
                html.push('<');
                html.push_str(tag);
                html.push('>');
                open.push(e.level);
            } else {
                html.push_str("</li>");
                if e.level < last {
                    let idx = open.len() - 1;
                    open[idx] = e.level;
                }
            }
        }
        html.push_str(&format!(
            "<li><a href=\"#{}\">{}</a>",
            escape_html(&e.id),
            escape_html(&e.text)
        ));
    }
    while !open.is_empty() {
        html.push_str(&format!("</li></{tag}>"));
        open.pop();
    }
    html
}

/// Escape `&`, `<`, `>`, `"` (matching the carve-js TOC `escapeHtml`). Note
/// this does NOT escape `'`, unlike the core attribute escaper.
fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            other => out.push(other),
        }
    }
    out
}

/// Plain-text projection matching the renderer's `plain_inlines`.
fn plain_text(nodes: &[InlineNode]) -> String {
    let mut out = String::new();
    for node in nodes {
        match node {
            InlineNode::Text(s) => out.push_str(s),
            InlineNode::Emphasis(e) => out.push_str(&plain_text(&e.children)),
            InlineNode::Code(s, _) => out.push_str(s),
            InlineNode::Link(l) => out.push_str(&plain_text(&l.children)),
            InlineNode::Image(i) => out.push_str(&i.alt),
            InlineNode::Extension(e) => out.push_str(&plain_text(&e.children)),
            InlineNode::Abbreviation(a) => out.push_str(&a.abbr),
            InlineNode::Mention(m) => out.push_str(&m.user),
            InlineNode::Tag(t) => out.push_str(&t.name),
            InlineNode::CaptionNumber(n) => {
                if let Some(number) = n.number {
                    out.push_str(&number.to_string());
                }
            }
            InlineNode::SoftBreak | InlineNode::HardBreak => out.push(' '),
            _ => {}
        }
    }
    out
}
