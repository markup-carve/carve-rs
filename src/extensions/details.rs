//! Render `::: details` admonitions as the HTML5 `<details>/<summary>`
//! disclosure widget instead of the default `<div class="details">`.
//!
//! Port of carve-js `details.ts`. carve-js implements this with a block-node
//! renderer keyed on the `admonition` node type; carve-rs has no per-node
//! render hook for an existing node, so this runs as a `before_render`
//! transform that rewrites every `details` admonition into a
//! [`BlockNode::Extension`] carrier (stashing the flattened title in its
//! `summary` field), then renders that carrier via
//! [`CarveExtension::render_block_extension`]. The inner content is rendered
//! by the core renderer at the correct nesting level
//! ([`RenderContext::render_blocks_at`]), so a details block behaves
//! identically wherever it sits - top level, inside a list item, inside a
//! blockquote.

use crate::ast::{BlockExtension, BlockNode, Document, InlineNode};
use crate::extension::{CarveExtension, RenderContext};
use crate::render::render_attrs;

/// Sentinel extension name for the rewritten carrier node. A `details`
/// admonition is rewritten to a `BlockNode::Extension` with this name; the
/// profile filter still gates it as a `div` (its origin) via
/// [`crate::profile::canonical_block_type`], so a restrictive profile that
/// denies custom containers strips the disclosure exactly as it would the
/// underlying admonition.
pub(crate) const CARRIER: &str = "carve-details";

/// Render `::: details` admonitions as the native `<details>/<summary>`
/// disclosure widget.
///
/// `details` is an ordinary custom admonition type, so by default it renders
/// as a generic `<div class="details">`. This extension opts into the native
/// disclosure widget: the quoted title becomes the `<summary>` (a title-less
/// block falls back to `<summary>Details</summary>`), and block attributes on
/// the opener (`{#faq open}`) carry onto the `<details>` tag in source order
/// (the auto `details` class is dropped - the tag is already the styling hook).
///
/// ```
/// use carve::{Details, Options};
/// let ext = Details::new();
/// let opts = Options::new().with_extension(&ext);
/// let src = "::: details \"More info\"\nHidden _here_.\n:::";
/// assert_eq!(
///     carve::to_html_with_options(src, &opts),
///     "<details>\n  <summary>More info</summary>\n  <p>Hidden <u>here</u>.</p>\n</details>"
/// );
/// ```
#[derive(Debug, Default, Clone)]
pub struct Details;

impl Details {
    /// Create a details extension.
    pub fn new() -> Self {
        Self
    }
}

impl CarveExtension for Details {
    fn name(&self) -> &'static str {
        "details"
    }

    fn before_render(&self, mut doc: Document) -> Document {
        rewrite_blocks(&mut doc.children);
        // Footnote bodies live outside the tree but are still rendered, so a
        // details block inside a footnote def must be rewritten too (matches
        // the mermaid extension, which transforms footnote-def blocks).
        for blocks in doc.footnote_defs.values_mut() {
            rewrite_blocks(blocks);
        }
        doc
    }

    fn render_block_extension(
        &self,
        node: &BlockExtension,
        ctx: &RenderContext<'_>,
    ) -> Option<String> {
        if node.name != CARRIER {
            return None;
        }
        let level = ctx.level();
        let inner_pad = ctx.indent(level + 1);
        let pad = ctx.indent(level);
        let summary = match node.summary.as_deref() {
            Some(t) if !t.trim().is_empty() => t,
            _ => "Details",
        };
        let open = format!("<details{}>", render_attrs(&node.attrs));
        let body = ctx.render_blocks_at(&node.children, level + 1);
        Some(format!(
            "{open}\n{inner_pad}<summary>{}</summary>\n{body}\n{pad}</details>",
            ctx.escape_html(summary),
        ))
    }
}

/// Rewrite every `details` admonition in `blocks` (recursively) into a
/// `carve-details` extension carrier, preserving its attributes and children
/// and stashing the flattened title text in the carrier's `summary` field.
fn rewrite_blocks(blocks: &mut [BlockNode]) {
    for block in blocks.iter_mut() {
        match block {
            BlockNode::Admonition(a) if a.kind == "details" => {
                rewrite_blocks(&mut a.children);
                let summary = a.title.as_deref().map(inline_text);
                *block = BlockNode::Extension(BlockExtension {
                    attrs: a.attrs.take(),
                    name: CARRIER.to_string(),
                    children: std::mem::take(&mut a.children),
                    summary,
                });
            }
            BlockNode::List(l) => {
                for item in &mut l.items {
                    rewrite_blocks(&mut item.children);
                }
            }
            BlockNode::BlockQuote(b) => rewrite_blocks(&mut b.children),
            BlockNode::Admonition(a) => rewrite_blocks(&mut a.children),
            BlockNode::Div(d) => rewrite_blocks(&mut d.children),
            BlockNode::Extension(e) => rewrite_blocks(&mut e.children),
            BlockNode::DefinitionList(dl) => {
                for item in &mut dl.items {
                    for def in &mut item.definitions {
                        rewrite_blocks(def);
                    }
                }
            }
            _ => {}
        }
    }
}

/// Flatten an inline tree to its text content (used for the summary title).
/// Inline markup is dropped: `"see /here/"` -> `see here`.
///
/// Mirrors carve-js `details.ts` `inlineText`, which collects each node's
/// string `value` (text, inline code) and recurses into its inline-children
/// array (`children` / `content`: emphasis, link, span, the inline extension,
/// critic insert/delete). Every other inline node - image, abbreviation,
/// mention, tag, math, autolink, emoji, cross-ref, footnote, citation - carries
/// its visible text in a differently named field, so carve-js's generic walk
/// drops it; this match drops the same set, byte-for-byte, so a title using
/// those nodes flattens identically across implementations.
fn inline_text(nodes: &[InlineNode]) -> String {
    let mut out = String::new();
    for node in nodes {
        match node {
            // String `value` fields (carve-js: `n.value`).
            InlineNode::Text(s) => out.push_str(s),
            InlineNode::Code(s, _) => out.push_str(s),
            // Inline-children arrays (carve-js: `n.children ?? n.content`).
            InlineNode::Emphasis(e) => out.push_str(&inline_text(&e.children)),
            InlineNode::Link(l) => out.push_str(&inline_text(&l.children)),
            InlineNode::Span(s) => out.push_str(&inline_text(&s.children)),
            InlineNode::Extension(e) => out.push_str(&inline_text(&e.children)),
            InlineNode::CriticInsert(c) => out.push_str(&inline_text(&c.children)),
            InlineNode::CriticDelete(c) => out.push_str(&inline_text(&c.children)),
            // Everything else carries no `value`/children array carve-js would
            // pick up, so it is dropped (matches carve-js exactly).
            _ => {}
        }
    }
    out
}
