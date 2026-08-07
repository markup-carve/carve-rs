//! Index terms (#91, Tier-3). Invisible `:index[term]` markers are collected
//! into a `::: index` block - a sorted `<ul class="index">` with one back-link
//! per occurrence. Reuses the `:name[…]` inline form; no new syntax. Off by
//! default, never corpus-pinned. See docs/extensions.md §8.
//!
//! Port of the carve-js `index-terms.ts`, byte-identical in HTML output. Like
//! `details` / `list-table`, this is a `before_render` transform: body
//! `:index[term]` markers are rewritten into a carrier inline extension that
//! carries their per-slug occurrence index, and a `::: index` admonition (when
//! any marker exists) is rewritten into a block carrier that renders the list.
//! A marker outside the body (e.g. inside a footnote definition) is left as the
//! plain `index` extension and renders inert, so the index never dangles.

use std::cell::RefCell;
use std::collections::BTreeMap;

use crate::ast::{
    smart_punctuation_glyph, Attrs, BlockExtension, BlockNode, Document, FigureTarget,
    InlineExtension, InlineNode,
};
use crate::extension::{BeforeRenderContext, CarveExtension, RenderContext, SmartTypographyMode};
use crate::parse::slugify_parse;
use crate::render::render_attrs;

/// Sentinel name for a counted body marker (carries `slug` + occurrence `n`).
const MARKER_CARRIER: &str = "carve-index-marker";
/// Sentinel name for the rewritten `::: index` list carrier.
pub(crate) const LIST_CARRIER: &str = "carve-index-list";

/// Collect `:index[term]` markers into a generated `::: index` list.
///
/// ```
/// use carve::{Index, Options};
/// let ext = Index::new();
/// let opts = Options::new().with_extension(&ext);
/// let src = "A :index[parser] here.\n\n::: index\n:::";
/// let html = carve::to_html_with_options(src, &opts);
/// assert!(html.contains("<span id=\"idx-parser-1\" class=\"index-term\"></span>"));
/// assert!(html.contains("<a href=\"#idx-parser-1\" class=\"index-backref\">"));
/// ```
#[derive(Debug, Default)]
pub struct Index {
    /// slug -> total occurrences (BTreeMap keeps codepoint/byte-ascending order).
    counts: RefCell<BTreeMap<String, usize>>,
    /// slug -> first occurrence's display text.
    display: RefCell<BTreeMap<String, String>>,
}

impl Index {
    /// Create an index extension.
    pub fn new() -> Self {
        Self::default()
    }
}

impl CarveExtension for Index {
    fn name(&self) -> &'static str {
        "index"
    }

    fn before_render(&self, mut doc: Document, ctx: &BeforeRenderContext<'_>) -> Document {
        // The DISPLAY text below is prose the reader sees, so it follows the
        // document-global smart-typography mode (PART 9 §19). The slug does not.
        let smart = ctx.options().smart_typography;
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        let mut display: BTreeMap<String, String> = BTreeMap::new();
        // Body-only collection: a marker in deferred content (footnote defs the
        // renderer may drop or reorder) is left as the plain `index` extension
        // and renders inert, so the index never points at a dropped anchor.
        for block in &mut doc.children {
            rewrite_markers_block(block, &mut counts, &mut display, smart);
        }
        let any = !counts.is_empty();
        *self.counts.borrow_mut() = counts;
        *self.display.borrow_mut() = display;
        // Rewrite `::: index` placeholders into list carriers only when there is
        // something to list; otherwise leave the plain `<div class="index">`.
        if any {
            rewrite_containers(&mut doc.children);
            for blocks in doc.footnote_defs.values_mut() {
                rewrite_containers(blocks);
            }
        }
        doc
    }

    fn render_inline_extension(
        &self,
        node: &InlineExtension,
        ctx: &RenderContext<'_>,
    ) -> Option<String> {
        match node.name.as_str() {
            MARKER_CARRIER => {
                let slug = attr(node, "idx-slug");
                let n = attr(node, "idx-n");
                Some(format!(
                    "<span id=\"idx-{}-{}\" class=\"index-term\"></span>",
                    ctx.escape_attr(&slug),
                    ctx.escape_attr(&n)
                ))
            }
            // An uncounted marker (deferred content) renders inert: no id.
            "index" => Some("<span class=\"index-term\"></span>".to_string()),
            _ => None,
        }
    }

    fn render_block_extension(
        &self,
        node: &BlockExtension,
        ctx: &RenderContext<'_>,
    ) -> Option<String> {
        if node.name != LIST_CARRIER {
            return None;
        }
        Some(render_index_list(
            node,
            ctx,
            &self.counts.borrow(),
            &self.display.borrow(),
        ))
    }
}

fn term_slug(term: &[InlineNode]) -> String {
    // GLYPH, always. An id must not depend on presentational typography, so the
    // slug is byte-identical in both smart-typography modes -- the same rule the
    // core applies to heading ids (PART 9 §19: "heading ids are BYTE-IDENTICAL
    // either way"). Only the DISPLAY text below follows the mode.
    slugify_parse(&inline_text(term, SmartTypographyMode::Glyph), true)
}

fn attr(node: &InlineExtension, key: &str) -> String {
    node.attrs
        .as_ref()
        .and_then(|a| a.key_values.get(key))
        .cloned()
        .unwrap_or_default()
}

/// Prepend `base` as the leading class of (a clone of) `attrs`.
fn with_base_class(attrs: &Option<Attrs>, base: &str) -> Attrs {
    let mut a = attrs.clone().unwrap_or_default();
    a.classes.insert(0, base.to_string());
    a
}

// ----- before_render: rewrite body markers ---------------------------------

fn rewrite_markers_block(
    block: &mut BlockNode,
    counts: &mut BTreeMap<String, usize>,
    display: &mut BTreeMap<String, String>,
    smart: SmartTypographyMode,
) {
    match block {
        BlockNode::Heading(h) => rewrite_markers_inline(&mut h.children, counts, display, smart),
        BlockNode::Paragraph(p) => rewrite_markers_inline(&mut p.children, counts, display, smart),
        BlockNode::List(l) => {
            for item in &mut l.items {
                for child in &mut item.children {
                    rewrite_markers_block(child, counts, display, smart);
                }
            }
        }
        BlockNode::BlockQuote(b) => {
            for child in &mut b.children {
                rewrite_markers_block(child, counts, display, smart);
            }
        }
        BlockNode::Admonition(a) => {
            if let Some(title) = &mut a.title {
                rewrite_markers_inline(title, counts, display, smart);
            }
            for child in &mut a.children {
                rewrite_markers_block(child, counts, display, smart);
            }
        }
        BlockNode::Div(d) => {
            for child in &mut d.children {
                rewrite_markers_block(child, counts, display, smart);
            }
        }
        BlockNode::Extension(e) => {
            for child in &mut e.children {
                rewrite_markers_block(child, counts, display, smart);
            }
        }
        BlockNode::Table(t) => {
            for row in &mut t.rows {
                for cell in &mut row.cells {
                    rewrite_markers_inline(&mut cell.children, counts, display, smart);
                }
            }
        }
        BlockNode::DefinitionList(dl) => {
            for item in &mut dl.items {
                for term in &mut item.terms {
                    rewrite_markers_inline(term, counts, display, smart);
                }
                for def in &mut item.definitions {
                    for child in def {
                        rewrite_markers_block(child, counts, display, smart);
                    }
                }
            }
        }
        BlockNode::Figure(f) => {
            rewrite_markers_inline(&mut f.caption, counts, display, smart);
            match &mut f.target {
                FigureTarget::BlockQuote(b) => {
                    for child in &mut b.children {
                        rewrite_markers_block(child, counts, display, smart);
                    }
                }
                FigureTarget::Table(t) => {
                    for row in &mut t.rows {
                        for cell in &mut row.cells {
                            rewrite_markers_inline(&mut cell.children, counts, display, smart);
                        }
                    }
                }
                FigureTarget::Paragraph(p) => {
                    rewrite_markers_inline(&mut p.children, counts, display, smart);
                }
                FigureTarget::Image(_) | FigureTarget::CodeBlock(_) => {}
            }
        }
        _ => {}
    }
}

fn rewrite_markers_inline(
    nodes: &mut [InlineNode],
    counts: &mut BTreeMap<String, usize>,
    display: &mut BTreeMap<String, String>,
    smart: SmartTypographyMode,
) {
    for node in nodes.iter_mut() {
        match node {
            InlineNode::Extension(e) if e.name == "index" => {
                let slug = term_slug(&e.children);
                let n = counts.entry(slug.clone()).or_insert(0);
                *n += 1;
                let occurrence = *n;
                display
                    .entry(slug.clone())
                    .or_insert_with(|| inline_text(&e.children, smart));
                let mut attrs = Attrs::default();
                attrs.key_values.insert("idx-slug".to_string(), slug);
                attrs
                    .key_values
                    .insert("idx-n".to_string(), occurrence.to_string());
                *node = InlineNode::Extension(InlineExtension {
                    attrs: Some(attrs),
                    name: MARKER_CARRIER.to_string(),
                    children: Vec::new(),
                    pos: None,
                });
            }
            InlineNode::Emphasis(e) => {
                rewrite_markers_inline(&mut e.children, counts, display, smart)
            }
            InlineNode::Link(l) => rewrite_markers_inline(&mut l.children, counts, display, smart),
            InlineNode::Span(s) => rewrite_markers_inline(&mut s.children, counts, display, smart),
            InlineNode::Extension(e) => {
                rewrite_markers_inline(&mut e.children, counts, display, smart)
            }
            InlineNode::CriticInsert(c) => {
                rewrite_markers_inline(&mut c.children, counts, display, smart)
            }
            InlineNode::CriticDelete(c) => {
                rewrite_markers_inline(&mut c.children, counts, display, smart)
            }
            _ => {}
        }
    }
}

// ----- before_render: rewrite `::: index` containers -----------------------

fn rewrite_containers(blocks: &mut [BlockNode]) {
    for block in blocks.iter_mut() {
        match block {
            BlockNode::Admonition(a) if a.kind == "index" => {
                rewrite_containers(&mut a.children);
                *block = BlockNode::Extension(BlockExtension {
                    attrs: a.attrs.take(),
                    name: LIST_CARRIER.to_string(),
                    children: std::mem::take(&mut a.children),
                    summary: None,
                    label: a.label.take(),
                    pos: None,
                });
            }
            BlockNode::List(l) => {
                for item in &mut l.items {
                    rewrite_containers(&mut item.children);
                }
            }
            BlockNode::BlockQuote(b) => rewrite_containers(&mut b.children),
            BlockNode::Admonition(a) => rewrite_containers(&mut a.children),
            BlockNode::Div(d) => rewrite_containers(&mut d.children),
            BlockNode::Extension(e) => rewrite_containers(&mut e.children),
            BlockNode::DefinitionList(dl) => {
                for item in &mut dl.items {
                    for def in &mut item.definitions {
                        rewrite_containers(def);
                    }
                }
            }
            _ => {}
        }
    }
}

// ----- render --------------------------------------------------------------

fn render_index_list(
    node: &BlockExtension,
    ctx: &RenderContext<'_>,
    counts: &BTreeMap<String, usize>,
    display: &BTreeMap<String, String>,
) -> String {
    let level = ctx.level();
    let pad = ctx.indent(level);
    let inner = ctx.indent(level + 1);
    // BTreeMap iterates keys in ascending byte order == Unicode-codepoint order,
    // the same locale-independent sort every implementation uses.
    //
    // Bound cumulative emitted bytes against the per-render index budget: the
    // complete backlink list is re-emitted in EVERY `::: index` block, so with
    // many markers and many blocks the output amplifies far beyond the input.
    // Once charging the next entry or backlink would exceed the budget, stop
    // emitting further index content (no huge string is ever allocated). The
    // budget sits far above any legitimate document, so the corpus is unaffected.
    let mut items: Vec<String> = Vec::new();
    'entries: for (slug, &n) in counts.iter() {
        // Bail before the (possibly large) escape once the budget is spent, so a
        // huge first term repeated across many `::: index` blocks cannot become a
        // CPU/allocation amplification path even though no content is emitted.
        if crate::index_budget::is_exhausted() {
            break;
        }
        let text = display.get(slug).map(String::as_str).unwrap_or_default();
        let escaped_text = ctx.escape_html(text);
        let mut entry = format!("{}<li>{} ", inner, escaped_text);
        if !crate::index_budget::try_spend(entry.len()) {
            break;
        }
        for m in 1..=n {
            let link = format!(
                "<a href=\"#idx-{}-{}\" class=\"index-backref\">\u{21a9}</a>",
                ctx.escape_attr(slug),
                m
            );
            // Each backlink after the first is separated by a space.
            let cost = if m == 1 { link.len() } else { link.len() + 1 };
            if !crate::index_budget::try_spend(cost) {
                break 'entries;
            }
            if m > 1 {
                entry.push(' ');
            }
            entry.push_str(&link);
        }
        entry.push_str("</li>");
        items.push(entry);
    }
    // The framework indents the FIRST line of the returned HTML by `level`
    // (see render_block_extension), so the opening `<ul>` must NOT carry its own
    // leading pad or it double-indents inside a container (`    <ul>` instead of
    // `  <ul>`, diverging from carve-js / carve-php). Interior lines still
    // self-indent: `<li>` at level+1, `</ul>` at level.
    let ul = format!(
        "<ul{}>\n{}\n{}</ul>",
        render_attrs(&Some(with_base_class(&node.attrs, "index"))),
        items.join("\n"),
        pad
    );
    // Preserve any authored content inside the placeholder before the list. That
    // content becomes the framework-indented first line, so the `<ul>` is no
    // longer first and must supply its own `pad`.
    if node.children.is_empty() {
        ul
    } else {
        format!(
            "{}\n{}{}",
            ctx.render_blocks_at(&node.children, level),
            pad,
            ul
        )
    }
}

/// Flatten an inline tree to its text content, matching carve-js `inlineText`.
fn inline_text(nodes: &[InlineNode], smart: SmartTypographyMode) -> String {
    let mut out = String::new();
    for node in nodes {
        match node {
            // Borrow the term text unless the placeholder is actually present.
            // A 2MB index term flattens through here (see the large-term
            // performance test), and an unconditional `replace` would copy the
            // whole thing on every pass to unescape a caret that is almost
            // never there.
            InlineNode::Text(s) => out.push_str(&s.value),
            InlineNode::SmartPunctuation(s) => {
                if smart == SmartTypographyMode::Source {
                    out.push_str(&s.value);
                } else {
                    out.push_str(smart_punctuation_glyph(s));
                }
            }
            InlineNode::Code(s) => out.push_str(&s.value),
            // An inline literal renders as visible prose (§27), matching carve-js
            // `inlineText` which folds its content into the flattened term text.
            InlineNode::LiteralInline(l) => out.push_str(&l.content),
            InlineNode::Emphasis(e) => out.push_str(&inline_text(&e.children, smart)),
            InlineNode::Link(l) => out.push_str(&inline_text(&l.children, smart)),
            InlineNode::Span(s) => out.push_str(&inline_text(&s.children, smart)),
            InlineNode::Extension(e) => out.push_str(&inline_text(&e.children, smart)),
            InlineNode::CriticInsert(c) => out.push_str(&inline_text(&c.children, smart)),
            InlineNode::CriticDelete(c) => out.push_str(&inline_text(&c.children, smart)),
            _ => {}
        }
    }
    out
}
