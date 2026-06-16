//! Render fenced code blocks tagged `mermaid` as `<pre class="mermaid">…</pre>`.
//!
//! Port of carve-js `mermaid.ts` / carve-php's `MermaidExtension`. carve-js
//! implements this via a code-block renderer; carve-rs has no per-node block
//! render hook, so this runs as a `before_render` transform that rewrites a
//! matching code block into a `raw-block` of the exact `<pre>` HTML. A
//! non-mermaid code block is left untouched and defers to the core renderer.

use crate::ast::{Attrs, BlockNode, Document, RawBlock};
use crate::extension::CarveExtension;

/// Options for [`Mermaid`].
#[derive(Debug, Clone)]
pub struct MermaidOptions {
    /// CSS class Mermaid.js detects. Default `"mermaid"`.
    pub css_class: String,
    /// Language tag that marks a diagram block. Default `"mermaid"`.
    pub language: String,
}

impl Default for MermaidOptions {
    fn default() -> Self {
        Self {
            css_class: "mermaid".into(),
            language: "mermaid".into(),
        }
    }
}

/// Render `mermaid` fenced code blocks for client-side Mermaid.js.
///
/// ```
/// use carve::{Mermaid, Options};
/// let ext = Mermaid::new();
/// let opts = Options::new().with_extension(&ext);
/// let src = "``` mermaid\ngraph TD; A-->B\n```\n";
/// assert_eq!(
///     carve::to_html_with_options(src, &opts),
///     "<pre class=\"mermaid\">graph TD; A-->B</pre>"
/// );
/// ```
pub struct Mermaid {
    opts: MermaidOptions,
}

impl Mermaid {
    /// Create a mermaid extension with default options.
    pub fn new() -> Self {
        Self::with_options(MermaidOptions::default())
    }

    /// Create a mermaid extension with explicit options.
    pub fn with_options(opts: MermaidOptions) -> Self {
        Self { opts }
    }
}

impl Default for Mermaid {
    fn default() -> Self {
        Self::new()
    }
}

impl CarveExtension for Mermaid {
    fn name(&self) -> &'static str {
        "mermaid"
    }

    fn before_render(&self, mut doc: Document) -> Document {
        transform_blocks(&mut doc.children, &self.opts);
        // Footnote bodies are rendered from footnote_defs (outside the tree), so
        // a mermaid block inside a footnote must be transformed too (matches
        // carve-js, which transforms footnote-def diagrams).
        for blocks in doc.footnote_defs.values_mut() {
            transform_blocks(blocks, &self.opts);
        }
        doc
    }
}

fn transform_blocks(blocks: &mut [BlockNode], opts: &MermaidOptions) {
    for block in blocks.iter_mut() {
        match block {
            BlockNode::CodeBlock(code) if code.lang.as_deref() == Some(opts.language.as_str()) => {
                // Preserve the block's own attributes (and source order) and
                // merge the mermaid class into the front of the class group.
                let mut attrs = code.attrs.clone().unwrap_or_default();
                let mut classes = vec![opts.css_class.clone()];
                classes.extend(attrs.classes.iter().cloned());
                attrs.classes = classes;
                ensure_class_slot(&mut attrs);

                let html = format!(
                    "<pre{}>{}</pre>",
                    render_attrs(&attrs),
                    escape_mermaid(&code.content),
                );
                *block = BlockNode::RawBlock(RawBlock {
                    format: "html".into(),
                    content: html,
                });
            }
            BlockNode::List(l) => {
                for item in &mut l.items {
                    transform_blocks(&mut item.children, opts);
                }
            }
            BlockNode::BlockQuote(b) => transform_blocks(&mut b.children, opts),
            BlockNode::Admonition(a) => transform_blocks(&mut a.children, opts),
            BlockNode::Div(d) => transform_blocks(&mut d.children, opts),
            BlockNode::Extension(e) => transform_blocks(&mut e.children, opts),
            BlockNode::DefinitionList(dl) => {
                for item in &mut dl.items {
                    for def in &mut item.definitions {
                        transform_blocks(def, opts);
                    }
                }
            }
            _ => {}
        }
    }
}

/// Ensure the class group renders. When a node carries an explicit attribute
/// `order` (author wrote `{...}`) but no class slot, add one so the merged
/// mermaid class is emitted in source-order position.
fn ensure_class_slot(attrs: &mut Attrs) {
    use crate::ast::AttrSlot;
    if attrs.classes.is_empty() {
        return;
    }
    if !attrs.order.is_empty() && !attrs.order.iter().any(|s| matches!(s, AttrSlot::Class)) {
        // Place the class slot first to mirror carve-js, which spreads the
        // merged classes ahead of the rest.
        attrs.order.insert(0, AttrSlot::Class);
    }
}

/// Render attributes the same way the core HTML renderer does: when `order` is
/// empty, emit id, class, then key-values; otherwise follow `order`.
fn render_attrs(attrs: &Attrs) -> String {
    use crate::ast::AttrSlot;
    use crate::escape::escape_attr;
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

/// Escape for Mermaid content: encode `&` and `<` but keep `>` so arrow syntax
/// (`A-->B`) survives, matching carve-php / carve-js.
fn escape_mermaid(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;")
}
