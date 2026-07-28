//! Glossary (#91, Tier-3). A `::: glossary` definition list declares terms;
//! `:term[word]` links a use to its `<dt id="gloss-{slug}">`. Reuses the
//! definition-list and `:name[…]` inline forms - no new syntax. Off by default,
//! never corpus-pinned. See docs/extensions.md §7.
//!
//! Port of the carve-js `glossary.ts`, byte-identical in HTML output. carve-rs
//! has no per-node block render hook, so (like `details` / `list-table`) this
//! runs as a `before_render` transform that rewrites each renderable
//! `::: glossary` admonition into a [`BlockNode::Extension`] carrier rendered by
//! [`CarveExtension::render_block_extension`]. `:term[word]` is an inline
//! extension rendered by [`CarveExtension::render_inline_extension`].

use std::cell::RefCell;
use std::collections::BTreeSet;

use crate::ast::{
    smart_punctuation_glyph, Attrs, BlockExtension, BlockNode, Document, InlineExtension,
    InlineNode,
};
use crate::extension::{BeforeRenderContext, CarveExtension, RenderContext};
use crate::parse::slugify_parse;
use crate::render::{render_attrs, render_attrs_without_keys};

/// Sentinel name for the rewritten carrier node.
pub(crate) const CARRIER: &str = "carve-glossary";

/// Render `::: glossary` definition lists with linkable term ids.
///
/// ```
/// use carve::{Glossary, Options};
/// let ext = Glossary::new();
/// let opts = Options::new().with_extension(&ext);
/// let src = "Use :term[HTTP].\n\n::: glossary\n:: HTTP\n:  HyperText Transfer Protocol.\n:::";
/// let html = carve::to_html_with_options(src, &opts);
/// assert!(html.contains("<a href=\"#gloss-http\" class=\"term\">HTTP</a>"));
/// assert!(html.contains("<dt id=\"gloss-http\">HTTP</dt>"));
/// ```
#[derive(Debug, Default)]
pub struct Glossary {
    /// Defined term slugs across every `::: glossary` block.
    defined: RefCell<BTreeSet<String>>,
    /// Per-render set giving the id to the first occurrence of a duplicated slug.
    id_seen: RefCell<BTreeSet<String>>,
}

impl Glossary {
    /// Create a glossary extension.
    pub fn new() -> Self {
        Self::default()
    }
}

impl CarveExtension for Glossary {
    fn name(&self) -> &'static str {
        "glossary"
    }

    fn before_render(&self, mut doc: Document, _ctx: &BeforeRenderContext<'_>) -> Document {
        let mut defined = BTreeSet::new();
        rewrite_blocks(&mut doc.children, &mut defined);
        // A `::: glossary` may live in a footnote definition (rendered later).
        for blocks in doc.footnote_defs.values_mut() {
            rewrite_blocks(blocks, &mut defined);
        }
        *self.defined.borrow_mut() = defined;
        self.id_seen.borrow_mut().clear();
        doc
    }

    fn render_inline_extension(
        &self,
        node: &InlineExtension,
        ctx: &RenderContext<'_>,
    ) -> Option<String> {
        if node.name != "term" {
            return None;
        }
        Some(render_term(node, ctx, &self.defined.borrow()))
    }

    fn render_block_extension(
        &self,
        node: &BlockExtension,
        ctx: &RenderContext<'_>,
    ) -> Option<String> {
        if node.name != CARRIER {
            return None;
        }
        Some(render_glossary(node, ctx, &mut self.id_seen.borrow_mut()))
    }
}

fn term_slug(term: &[InlineNode]) -> String {
    slugify_parse(&inline_text(term), true)
}

/// Prepend `base` as the leading class of (a clone of) `attrs`.
fn with_base_class(attrs: &Option<Attrs>, base: &str) -> Attrs {
    let mut a = attrs.clone().unwrap_or_default();
    a.classes.insert(0, base.to_string());
    a
}

/// Rewrite every `::: glossary` admonition (recursively) into a carrier and
/// collect its defined term slugs.
fn rewrite_blocks(blocks: &mut [BlockNode], defined: &mut BTreeSet<String>) {
    for block in blocks.iter_mut() {
        match block {
            BlockNode::Admonition(a) if a.kind == "glossary" => {
                rewrite_blocks(&mut a.children, defined);
                let has_list = a
                    .children
                    .iter()
                    .any(|c| matches!(c, BlockNode::DefinitionList(_)));
                if !has_list {
                    continue;
                }
                for child in &a.children {
                    if let BlockNode::DefinitionList(dl) = child {
                        for item in &dl.items {
                            for term in &item.terms {
                                defined.insert(term_slug(term));
                            }
                        }
                    }
                }
                *block = BlockNode::Extension(BlockExtension {
                    attrs: a.attrs.take(),
                    name: CARRIER.to_string(),
                    children: std::mem::take(&mut a.children),
                    summary: None,
                    label: a.label.take(),
                });
            }
            BlockNode::List(l) => {
                for item in &mut l.items {
                    rewrite_blocks(&mut item.children, defined);
                }
            }
            BlockNode::BlockQuote(b) => rewrite_blocks(&mut b.children, defined),
            BlockNode::Admonition(a) => rewrite_blocks(&mut a.children, defined),
            BlockNode::Div(d) => rewrite_blocks(&mut d.children, defined),
            BlockNode::Extension(e) => rewrite_blocks(&mut e.children, defined),
            BlockNode::DefinitionList(dl) => {
                for item in &mut dl.items {
                    for def in &mut item.definitions {
                        rewrite_blocks(def, defined);
                    }
                }
            }
            _ => {}
        }
    }
}

fn render_term(
    node: &InlineExtension,
    ctx: &RenderContext<'_>,
    defined: &BTreeSet<String>,
) -> String {
    let word = ctx.render_inlines(&node.children);
    let slug = term_slug(&node.children);
    let attrs = Some(with_base_class(&node.attrs, "term"));
    if defined.contains(&slug) {
        // The structural glossary target wins; drop any author `href`
        // (case-insensitively) so the <a> never has two.
        let attr_str = render_attrs_without_keys(&attrs, &["href"]);
        format!(
            "<a href=\"#gloss-{}\"{}>{}</a>",
            ctx.escape_attr(&slug),
            attr_str,
            word
        )
    } else {
        // Resolved, but no matching entry: degrade to a plain span.
        format!("<span{}>{}</span>", render_attrs(&attrs), word)
    }
}

fn render_glossary(
    node: &BlockExtension,
    ctx: &RenderContext<'_>,
    id_seen: &mut BTreeSet<String>,
) -> String {
    let level = ctx.level();
    let pad = ctx.indent(level);
    let inner = ctx.indent(level + 1);
    // Render children in source order: each definition list becomes a
    // `<dl class="glossary">` in place, any other block is preserved verbatim,
    // so notes before/between/after the lists keep their position. The block's
    // authored `{#id .class}` rides on the first <dl>.
    let mut first_dl = true;
    let mut parts: Vec<String> = Vec::new();
    for child in &node.children {
        let BlockNode::DefinitionList(dl) = child else {
            parts.push(ctx.render_blocks_at(std::slice::from_ref(child), level));
            continue;
        };
        let mut rows: Vec<String> = Vec::new();
        for item in &dl.items {
            for term in &item.terms {
                let slug = term_slug(term);
                let id_attr = if id_seen.contains(&slug) {
                    String::new()
                } else {
                    id_seen.insert(slug.clone());
                    format!(" id=\"gloss-{}\"", ctx.escape_attr(&slug))
                };
                rows.push(format!(
                    "{}<dt{}>{}</dt>",
                    inner,
                    id_attr,
                    ctx.render_inlines(term)
                ));
            }
            for def in &item.definitions {
                rows.push(format!("{}<dd>{}</dd>", inner, render_def(def, ctx)));
            }
        }
        let attr_str = if first_dl {
            render_attrs(&Some(with_base_class(&node.attrs, "glossary")))
        } else {
            " class=\"glossary\"".to_string()
        };
        first_dl = false;
        parts.push(format!(
            "{}<dl{}>\n{}\n{}</dl>",
            pad,
            attr_str,
            rows.join("\n"),
            pad
        ));
    }
    parts.join("\n")
}

/// A single-paragraph definition collapses to inline content; a multi-block
/// definition keeps its block wrappers (rendered at a deeper level).
fn render_def(def: &[BlockNode], ctx: &RenderContext<'_>) -> String {
    if def.len() == 1 {
        if let BlockNode::Paragraph(p) = &def[0] {
            return ctx.render_inlines(&p.children);
        }
    }
    format!(
        "\n{}\n{}",
        ctx.render_blocks_at(def, ctx.level() + 2),
        ctx.indent(ctx.level() + 1)
    )
}

/// Flatten an inline tree to its text content (for slug derivation), matching
/// carve-js `inlineText` byte-for-byte.
fn inline_text(nodes: &[InlineNode]) -> String {
    let mut out = String::new();
    for node in nodes {
        match node {
            InlineNode::Text(s) => out.push_str(&s.replace(crate::ESCAPED_CARET_PLACEHOLDER, "^")),
            InlineNode::SmartPunctuation(s) => out.push_str(smart_punctuation_glyph(s)),
            InlineNode::Code(s, _) => out.push_str(s),
            // An inline literal renders as visible prose (§27), matching carve-js
            // `inlineText` which folds its content into the flattened term text.
            InlineNode::LiteralInline(l) => out.push_str(&l.content),
            InlineNode::Emphasis(e) => out.push_str(&inline_text(&e.children)),
            InlineNode::Link(l) => out.push_str(&inline_text(&l.children)),
            InlineNode::Span(s) => out.push_str(&inline_text(&s.children)),
            InlineNode::Extension(e) => out.push_str(&inline_text(&e.children)),
            InlineNode::CriticInsert(c) => out.push_str(&inline_text(&c.children)),
            InlineNode::CriticDelete(c) => out.push_str(&inline_text(&c.children)),
            _ => {}
        }
    }
    out
}
