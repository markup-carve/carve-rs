//! Hidden / blurred "spoiler" content, revealed on interaction (Tier-3).
//!
//! Implements the standard `spoiler` extension from the spec's Extension
//! Registry - no new syntax, it claims the reserved `spoiler` role:
//!
//! - **Inline** `:spoiler[text]` → `<span class="spoiler">text</span>` via
//!   [`CarveExtension::render_inline_extension`]. Without the extension this
//!   stays the generic `<span class="ext-spoiler">text</span>`.
//! - **Block** `::: spoiler "Title"` → an HTML5 `<details class="spoiler">`
//!   disclosure (native, keyboard- and screen-reader-accessible). Like the
//!   `details` extension, carve-rs has no per-node block hook, so this runs as
//!   a `before_render` transform that rewrites a `spoiler` admonition into a
//!   [`BlockNode::ExtensionCarrier`] carrier rendered via
//!   [`CarveExtension::render_extension_carrier`]. A title-less block falls back
//!   to `<summary>Spoiler</summary>`. Without the extension it stays a plain
//!   `<div class="spoiler">`.
//!
//! Carve only emits the marker; the blur + reveal is the host's CSS (like the
//! Mermaid extension). Author attributes merge onto the output element - the
//! `spoiler` base class merges at the authored class slot, or follows the
//! authored attributes when no class slot exists - with
//! the always-on attribute hardening (`render_attrs_after_class` drops
//! `on*` / `srcdoc` / `formaction` and neutralizes dangerous values), so a
//! `{onclick=...}` can never reach the output.

use crate::ast::{Attrs, BlockNode, Document, ExtensionCarrier, InlineExtension};
use crate::escape::escape_attr;
use crate::extension::{BeforeRenderContext, CarveExtension, RenderContext};
use crate::render::{render_attrs_after_class, render_attrs_with_base_class};

/// The extra class a static (revealed) spoiler carries (after the base
/// `spoiler` class): the blur is dropped, the content shown plainly.
const REVEALED: &str = "spoiler spoiler-revealed";

/// Sentinel extension name for the rewritten block carrier.
pub(crate) const CARRIER: &str = "carve-spoiler";

/// The inline extension role / admonition kind this extension claims.
const ROLE: &str = "spoiler";

/// Default summary label for a title-less spoiler block.
const DEFAULT_SUMMARY: &str = "Spoiler";

/// Render `spoiler` inline roles and `::: spoiler` blocks as hidden content.
///
/// ```
/// use carve::{Spoiler, Options};
/// let ext = Spoiler::new();
/// let opts = Options::new().with_extension(&ext);
/// assert_eq!(
///     carve::to_html_with_options("Plot: :spoiler[the butler did it].", &opts),
///     "<p>Plot: <span class=\"spoiler\">the butler did it</span>.</p>"
/// );
/// ```
#[derive(Debug, Default, Clone)]
pub struct Spoiler;

impl Spoiler {
    /// Create a spoiler extension.
    pub fn new() -> Self {
        Self
    }
}

impl CarveExtension for Spoiler {
    fn name(&self) -> &'static str {
        "spoiler"
    }

    fn before_render(&self, mut doc: Document, _ctx: &BeforeRenderContext<'_>) -> Document {
        rewrite_blocks(&mut doc.children);
        for blocks in doc.footnote_defs.values_mut() {
            rewrite_blocks(blocks);
        }
        doc
    }

    fn render_inline_extension(
        &self,
        node: &InlineExtension,
        ctx: &RenderContext<'_>,
    ) -> Option<String> {
        if node.name != ROLE {
            return None;
        }
        // Static mode: hiding is meaningless offline, so the content is revealed
        // (the `spoiler-revealed` class drops the blur). Mirrors carve-js
        // `spoiler.ts` `staticInlineRenderers`.
        let base = if ctx.is_static() { REVEALED } else { ROLE };
        Some(format!(
            "<span{}>{}</span>",
            open_attrs_with_base(node.attrs.as_ref(), base),
            ctx.render_inlines(&node.children),
        ))
    }

    fn render_extension_carrier(
        &self,
        node: &ExtensionCarrier,
        ctx: &RenderContext<'_>,
    ) -> Option<String> {
        if node.name != CARRIER {
            return None;
        }
        let level = ctx.level();
        let inner_pad = ctx.indent(level + 1);
        let pad = ctx.indent(level);
        let summary = match &node.summary {
            Some(nodes) => {
                let rendered = ctx.render_inlines(nodes);
                if rendered.trim().is_empty() {
                    DEFAULT_SUMMARY.to_string()
                } else {
                    rendered
                }
            }
            None => DEFAULT_SUMMARY.to_string(),
        };
        let body = ctx.render_blocks_at(&node.children, level + 1);
        // Static mode: the disclosure is revealed and expanded into a flat,
        // inert `<section class="spoiler spoiler-revealed">` - the title becomes
        // an `<h3 class="spoiler-title">` heading and a grouping `[label]` is
        // surfaced as the caption floor after it. Mirrors carve-js
        // `spoiler.ts` `staticBlockRenderers`.
        if ctx.is_static() {
            let open = format!(
                "<section{}>",
                open_attrs_with_base(node.attrs.as_ref(), REVEALED)
            );
            let label_line = match &node.label {
                Some(l) => format!(
                    "{inner_pad}<p class=\"div-label\">{}</p>\n",
                    ctx.escape_html(l)
                ),
                None => String::new(),
            };
            return Some(format!(
                "{open}\n{inner_pad}<h3 class=\"spoiler-title\">{}</h3>\n{label_line}{body}\n{pad}</section>",
                summary,
            ));
        }
        let open = format!(
            "<details{}>",
            open_attrs_with_base(node.attrs.as_ref(), ROLE)
        );
        Some(format!(
            "{open}\n{inner_pad}<summary>{}</summary>\n{body}\n{pad}</details>",
            summary,
        ))
    }
}

/// Merge the structural class at the authored class slot, or append it after
/// authored attributes when that slot is absent. Without source order, use the
/// canonical attribute order shared by other AST renderers.
fn open_attrs_with_base(attrs: Option<&Attrs>, base: &str) -> String {
    match attrs {
        Some(a) if a.classes.is_empty() && !a.order.is_empty() => {
            format!(
                "{} class=\"{}\"",
                render_attrs_after_class(a),
                escape_attr(base)
            )
        }
        _ => render_attrs_with_base_class(&attrs.cloned(), base),
    }
}

/// Rewrite every `spoiler` admonition (recursively) into a `carve-spoiler`
/// extension carrier, preserving its attributes and children and stashing the
/// flattened title text in the carrier's `summary` field.
fn rewrite_blocks(blocks: &mut [BlockNode]) {
    for block in blocks.iter_mut() {
        match block {
            BlockNode::Admonition(a) if a.kind == ROLE => {
                rewrite_blocks(&mut a.children);
                *block = BlockNode::ExtensionCarrier(ExtensionCarrier {
                    attrs: a.attrs.take(),
                    name: CARRIER.to_string(),
                    children: std::mem::take(&mut a.children),
                    summary: a.title.take(),
                    label: a.label.take(),
                    pos: None,
                });
            }
            BlockNode::List(l) => {
                for item in &mut l.items {
                    rewrite_blocks(&mut item.children);
                }
            }
            BlockNode::BlockQuote(b) => rewrite_blocks(&mut b.children),
            BlockNode::Admonition(a) => rewrite_blocks(&mut a.children),
            BlockNode::Directive(d) => rewrite_blocks(&mut d.children),
            BlockNode::Div(d) => rewrite_blocks(&mut d.children),
            BlockNode::ExtensionCarrier(e) => rewrite_blocks(&mut e.children),
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
