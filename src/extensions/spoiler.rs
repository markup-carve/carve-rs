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
//!   [`BlockNode::Extension`] carrier rendered via
//!   [`CarveExtension::render_block_extension`]. A title-less block falls back
//!   to `<summary>Spoiler</summary>`. Without the extension it stays a plain
//!   `<div class="spoiler">`.
//!
//! Carve only emits the marker; the blur + reveal is the host's CSS (like the
//! Mermaid extension). Author attributes merge onto the output element - the
//! `spoiler` base class ahead of author classes, then id / key-values - with
//! the always-on attribute hardening (`render_attrs_after_class` drops
//! `on*` / `srcdoc` / `formaction` and neutralizes dangerous values), so a
//! `{onclick=...}` can never reach the output.

use crate::ast::{Attrs, BlockExtension, BlockNode, Document, InlineExtension, InlineNode};
use crate::escape::escape_attr;
use crate::extension::{CarveExtension, RenderContext};
use crate::render::render_attrs_after_class;

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

    fn before_render(&self, mut doc: Document) -> Document {
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
        Some(format!(
            "<span{}>{}</span>",
            open_attrs(node.attrs.as_ref()),
            ctx.render_inlines(&node.children),
        ))
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
            _ => DEFAULT_SUMMARY,
        };
        let open = format!("<details{}>", open_attrs(node.attrs.as_ref()));
        let body = ctx.render_blocks_at(&node.children, level + 1);
        Some(format!(
            "{open}\n{inner_pad}<summary>{}</summary>\n{body}\n{pad}</details>",
            ctx.escape_html(summary),
        ))
    }
}

/// Build the output element's attribute string: the `spoiler` base class ahead
/// of any author classes, then id / key-values via the shared
/// `render_attrs_after_class` (which applies the always-on attribute hardening
/// and value escaping). Class-first, matching carve-php and core math.
fn open_attrs(attrs: Option<&Attrs>) -> String {
    match attrs {
        Some(a) => {
            let mut classes = vec![ROLE.to_string()];
            for class in &a.classes {
                if !classes.contains(class) {
                    classes.push(class.clone());
                }
            }
            format!(
                " class=\"{}\"{}",
                escape_attr(&classes.join(" ")),
                render_attrs_after_class(a),
            )
        }
        None => format!(" class=\"{ROLE}\""),
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

/// Flatten an inline tree to its text content for the summary title (mirrors the
/// `details` extension's `inline_text`).
fn inline_text(nodes: &[InlineNode]) -> String {
    let mut out = String::new();
    for node in nodes {
        match node {
            InlineNode::Text(s) => out.push_str(s),
            InlineNode::Code(s, _) => out.push_str(s),
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
