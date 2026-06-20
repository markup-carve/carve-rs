//! Render fenced code blocks tagged `math` as block-level display math.
//!
//! Port of carve-js `math-block.ts` / carve-php's `MathBlockExtension`.
//! carve-js implements this via a code-block renderer; carve-rs has no
//! per-node block render hook, so this runs as a `before_render` transform that
//! rewrites a matching code block into a `raw-block` of the exact `<div>` HTML.
//! A non-math code block is left untouched and defers to the core renderer.
//!
//! The LaTeX body is HTML-escaped the same way the core math renderer escapes
//! inline / display `$…$` math (via [`crate::escape::escape_text`], which
//! escapes `&`, `<`, and `>`). Note this escapes `>` too, unlike the Mermaid
//! extension which keeps `>` for arrow syntax.

use crate::ast::{Attrs, BlockNode, Document, RawBlock};
use crate::extension::CarveExtension;

/// Options for [`MathBlock`].
#[derive(Debug, Clone)]
pub struct MathBlockOptions {
    /// Language tag that marks a display-math block. Default `"math"`.
    pub language: String,
}

impl Default for MathBlockOptions {
    fn default() -> Self {
        Self {
            language: "math".into(),
        }
    }
}

/// Render `math` fenced code blocks as block-level display math.
///
/// ```
/// use carve::{MathBlock, Options};
/// let ext = MathBlock::new();
/// let opts = Options::new().with_extension(&ext);
/// let src = "``` math\n\\int_0^1 x^2 \\, dx\n```\n";
/// assert_eq!(
///     carve::to_html_with_options(src, &opts),
///     "<div class=\"math display\">\\[\\int_0^1 x^2 \\, dx\\]</div>"
/// );
/// ```
pub struct MathBlock {
    opts: MathBlockOptions,
}

impl MathBlock {
    /// Create a math-block extension with default options.
    pub fn new() -> Self {
        Self::with_options(MathBlockOptions::default())
    }

    /// Create a math-block extension with explicit options.
    pub fn with_options(opts: MathBlockOptions) -> Self {
        Self { opts }
    }
}

impl Default for MathBlock {
    fn default() -> Self {
        Self::new()
    }
}

impl CarveExtension for MathBlock {
    fn name(&self) -> &'static str {
        "math-block"
    }

    fn before_render(&self, mut doc: Document) -> Document {
        transform_blocks(&mut doc.children, &self.opts);
        // Footnote bodies are rendered from footnote_defs (outside the tree), so
        // a math block inside a footnote must be transformed too (matches
        // carve-js, which transforms footnote-def blocks).
        for blocks in doc.footnote_defs.values_mut() {
            transform_blocks(blocks, &self.opts);
        }
        doc
    }
}

fn transform_blocks(blocks: &mut [BlockNode], opts: &MathBlockOptions) {
    for block in blocks.iter_mut() {
        match block {
            BlockNode::CodeBlock(code) if code.lang.as_deref() == Some(opts.language.as_str()) => {
                // Preserve the block's own attributes (and source order) and
                // merge the mandatory math classes into the front of the class
                // group (matching inline math: base classes first).
                let mut attrs = code.attrs.clone().unwrap_or_default();
                let mut classes = vec!["math".to_string(), "display".to_string()];
                classes.extend(attrs.classes.iter().cloned());
                attrs.classes = classes;
                ensure_class_slot(&mut attrs);

                let html = format!(
                    "<div{}>\\[{}\\]</div>",
                    render_attrs(&attrs),
                    crate::escape::escape_text(&code.content),
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
/// `order` (author wrote `{...}`) but no class slot, add one so the merged math
/// classes are emitted in source-order position.
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
