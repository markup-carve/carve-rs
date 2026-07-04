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

use std::cell::RefCell;
use std::collections::BTreeMap;

use crate::ast::{Attrs, BlockExtension, BlockNode, Document, Heading, RawBlock};
use crate::extension::{BeforeRenderContext, CarveExtension, RenderContext};
use crate::render::render_attrs_without_keys;

/// Carrier extension name a `::: toc` block is rewritten to in `before_render`,
/// then rendered by [`TocPlacement::render_block_extension`].
const TOC_CARRIER: &str = "toc-placement";

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

#[derive(Clone)]
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

    fn before_render(&self, mut doc: Document, _ctx: &BeforeRenderContext<'_>) -> Document {
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
            "<nav class=\"{}\">\n{}</nav>",
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
                        text: crate::render::plain_inlines(&h.children),
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

/// Like [`collect_entries`] but records EVERY heading (not only top-level ones),
/// recursing into every container. Used by the `::: toc` placement directive so
/// headings nested in `::: note`, blockquotes, lists, etc. appear in the TOC.
/// The id counter matches the renderer's document-order allocation.
fn collect_all_entries(
    blocks: &[BlockNode],
    counts: &mut BTreeMap<String, usize>,
    lowercase: bool,
    entries: &mut Vec<TocEntry>,
) {
    for block in blocks {
        match block {
            BlockNode::Heading(h) => {
                let id = next_id(h, counts, lowercase);
                entries.push(TocEntry {
                    level: h.level,
                    text: crate::render::plain_inlines(&h.children),
                    id,
                });
            }
            BlockNode::List(l) => {
                for item in &l.items {
                    collect_all_entries(&item.children, counts, lowercase, entries);
                }
            }
            BlockNode::BlockQuote(b) => {
                collect_all_entries(&b.children, counts, lowercase, entries)
            }
            BlockNode::Admonition(a) => {
                collect_all_entries(&a.children, counts, lowercase, entries)
            }
            BlockNode::Div(d) => collect_all_entries(&d.children, counts, lowercase, entries),
            BlockNode::Extension(e) => collect_all_entries(&e.children, counts, lowercase, entries),
            BlockNode::DefinitionList(dl) => {
                for item in &dl.items {
                    for def in &item.definitions {
                        collect_all_entries(def, counts, lowercase, entries);
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
        .unwrap_or_else(|| {
            crate::parse::slugify_parse(&crate::render::plain_inlines(&h.children), lowercase)
        });
    let count = counts.entry(base.clone()).or_insert(0);
    *count += 1;
    if *count == 1 {
        base
    } else {
        format!("{base}-{count}")
    }
}

/// Build a nested list from a flat, document-order entry list. Byte-faithful
/// port of carve-php's `TableOfContentsExtension::renderTocList` (and the
/// matching carve-js `buildList`): one tag per line, and a heading deeper than
/// its predecessor's predecessor stays a sibling `<li>` in the same nested
/// `<ul>` rather than opening a fresh one. Returns the `<ul>…</ul>` list with
/// its trailing newline.
fn build_list(entries: &[TocEntry], list_type: ListType) -> String {
    let tag = list_type.tag();
    if entries.is_empty() {
        return String::new();
    }
    let mut html = format!("<{tag}>\n");
    let mut level_stack: Vec<u8> = vec![entries[0].level];
    let mut has_open_item = false;
    for e in entries {
        if has_open_item {
            let mut depth = level_stack.len();
            let current = level_stack[depth - 1];
            if e.level > current {
                html.push_str(&format!("\n<{tag}>\n"));
                level_stack.push(e.level);
            } else {
                while depth > 1 && e.level <= level_stack[depth - 2] {
                    html.push_str(&format!("</li>\n</{tag}>\n"));
                    level_stack.pop();
                    depth -= 1;
                }
                html.push_str("</li>\n");
            }
        }
        html.push_str(&format!(
            "<li><a href=\"#{}\">{}</a>",
            escape_html(&e.id),
            escape_html(&e.text)
        ));
        has_open_item = true;
    }
    html.push_str("</li>\n");
    let mut depth = level_stack.len();
    while depth > 1 {
        html.push_str(&format!("</{tag}>\n</li>\n"));
        level_stack.pop();
        depth -= 1;
    }
    html.push_str(&format!("</{tag}>\n"));
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

// ===========================================================================
// In-document `::: toc` placement directive
// ===========================================================================

/// In-document table-of-contents placement directive (Tier-3). Unlike
/// [`TableOfContents`] (which injects one TOC at the document top or bottom),
/// this renders a `<nav class="toc">` exactly where the author writes a
/// `::: toc` block. Off by default.
///
/// The level window is set with attributes on the line *before* the opener
/// (Carve attaches `:::`-block attributes on a preceding attribute line):
///
/// ```text
/// ::: toc              (all levels, 1-6)
/// :::
///
/// {depth=2}            (levels 1-2)
/// ::: toc
/// :::
///
/// {from=2 to=4}        (levels 2-4)
/// ::: toc
/// :::
/// ```
///
/// The nested `<ul>` is byte-identical to carve-js / carve-php. Heading ids are
/// derived with the renderer's `lowercase_heading_ids` option so link targets
/// match the emitted `<section id>` anchors.
pub struct TocPlacement {
    entries: RefCell<Vec<TocEntry>>,
}

impl TocPlacement {
    /// Create the placement extension.
    pub fn new() -> Self {
        Self {
            entries: RefCell::new(Vec::new()),
        }
    }
}

impl Default for TocPlacement {
    fn default() -> Self {
        Self::new()
    }
}

impl CarveExtension for TocPlacement {
    fn name(&self) -> &'static str {
        "toc"
    }

    fn before_render(&self, mut doc: Document, ctx: &BeforeRenderContext<'_>) -> Document {
        // Collect EVERY heading (the per-directive window is applied at render
        // time), recursing into containers so headings nested in `::: note`,
        // blockquotes, lists, etc. are included - they render with id anchors.
        // The id counter stays aligned with the renderer; footnote definitions
        // live outside `doc.children`, so their (id-less) headings are excluded.
        let lowercase = ctx.options().lowercase_heading_ids;
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        let mut entries: Vec<TocEntry> = Vec::new();
        collect_all_entries(&doc.children, &mut counts, lowercase, &mut entries);
        *self.entries.borrow_mut() = entries;

        rewrite_toc_containers(&mut doc.children);
        doc
    }

    fn render_block_extension(
        &self,
        node: &BlockExtension,
        ctx: &RenderContext<'_>,
    ) -> Option<String> {
        if node.name != TOC_CARRIER {
            return None;
        }
        Some(render_toc_nav(node, ctx, &self.entries.borrow()))
    }
}

/// Rewrite every `::: toc` admonition into a [`TOC_CARRIER`] block extension so
/// `render_block_extension` renders it in place. Recurses into containers.
fn rewrite_toc_containers(blocks: &mut [BlockNode]) {
    for block in blocks.iter_mut() {
        match block {
            BlockNode::Admonition(a) if a.kind == "toc" => {
                rewrite_toc_containers(&mut a.children);
                *block = BlockNode::Extension(BlockExtension {
                    attrs: a.attrs.take(),
                    name: TOC_CARRIER.to_string(),
                    children: std::mem::take(&mut a.children),
                    summary: None,
                    label: a.label.take(),
                });
            }
            BlockNode::List(l) => {
                for item in &mut l.items {
                    rewrite_toc_containers(&mut item.children);
                }
            }
            BlockNode::BlockQuote(b) => rewrite_toc_containers(&mut b.children),
            BlockNode::Admonition(a) => rewrite_toc_containers(&mut a.children),
            BlockNode::Div(d) => rewrite_toc_containers(&mut d.children),
            BlockNode::Extension(e) => rewrite_toc_containers(&mut e.children),
            BlockNode::DefinitionList(dl) => {
                for item in &mut dl.items {
                    for def in &mut item.definitions {
                        rewrite_toc_containers(def);
                    }
                }
            }
            _ => {}
        }
    }
}

fn render_toc_nav(node: &BlockExtension, ctx: &RenderContext<'_>, entries: &[TocEntry]) -> String {
    let (min, max) = toc_window(&node.attrs);
    let picked: Vec<TocEntry> = entries
        .iter()
        .filter(|e| e.level >= min && e.level <= max)
        .cloned()
        .collect();
    let attrs = render_attrs_without_keys(
        &Some(with_base_class(&node.attrs, "toc")),
        &["depth", "from", "to"],
    );
    let nav = if picked.is_empty() {
        format!("<nav{attrs}></nav>")
    } else {
        format!("<nav{attrs}>\n{}</nav>", build_list(&picked, ListType::Ul))
    };
    // Preserve any authored blocks written inside the placeholder before the nav.
    if node.children.is_empty() {
        nav
    } else {
        format!(
            "{}\n{}",
            ctx.render_blocks_at(&node.children, ctx.level()),
            nav
        )
    }
}

/// Resolve the heading-level window from a `::: toc` directive's attributes.
/// `{from=X to=Y}` is an explicit range (swapped if inverted); `{depth=N}` is
/// shorthand for levels 1..N. `from`/`to` win over `depth` when both appear.
fn toc_window(attrs: &Option<Attrs>) -> (u8, u8) {
    let get = |k: &str| {
        attrs
            .as_ref()
            .and_then(|a| a.key_values.get(k))
            .map(String::as_str)
    };
    let level = |value: Option<&str>, fallback: u8| -> u8 {
        value
            .and_then(|s| s.trim().parse::<i64>().ok())
            .map(|n| n.clamp(1, 6) as u8)
            .unwrap_or(fallback)
    };
    if get("from").is_some() || get("to").is_some() {
        let mut min = level(get("from"), 1);
        let mut max = level(get("to"), 6);
        if min > max {
            std::mem::swap(&mut min, &mut max);
        }
        (min, max)
    } else {
        (1, level(get("depth"), 6))
    }
}

/// Prepend `base` as the leading class of (a clone of) `attrs`.
fn with_base_class(attrs: &Option<Attrs>, base: &str) -> Attrs {
    let mut a = attrs.clone().unwrap_or_default();
    a.classes.insert(0, base.to_string());
    a
}
