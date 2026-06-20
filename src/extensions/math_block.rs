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

use crate::ast::{BlockNode, Document, RawBlock};
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
                // Emit only the fixed `math display` class. Author attributes
                // from the fence are intentionally NOT copied: rendering them
                // here would bypass safe-mode attribute filtering (an
                // `{onclick=...}` on a ```math fence would become an executable
                // handler on the <div>).
                let html = format!(
                    "<div class=\"math display\">\\[{}\\]</div>",
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
