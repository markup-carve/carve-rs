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
use crate::extension::{BeforeRenderContext, CarveExtension, MathRendererRef};

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

    fn before_render(&self, mut doc: Document, ctx: &BeforeRenderContext<'_>) -> Document {
        // On the HTML static path a supplied `renderers.math` server-renders the
        // body (MathML / HTML) inside the `math display` div so the page needs no
        // client KaTeX / MathJax; absent it, the static output is the same
        // `\[…\]` source as interactive (never blank). The effective mode is
        // interactive for the non-HTML renderers (static rendering is HTML-only),
        // so Markdown / ANSI output is unchanged when one `Options` is reused
        // across formats. Mirrors carve-js `math-block.ts` `staticBlockRenderers`.
        let math_renderer: Option<MathRendererRef<'_>> = if ctx.is_static() {
            ctx.renderers().math.as_deref()
        } else {
            None
        };
        transform_blocks(&mut doc.children, &self.opts, math_renderer);
        // Footnote bodies are rendered from footnote_defs (outside the tree), so
        // a math block inside a footnote must be transformed too (matches
        // carve-js, which transforms footnote-def blocks).
        for blocks in doc.footnote_defs.values_mut() {
            transform_blocks(blocks, &self.opts, math_renderer);
        }
        doc
    }
}

fn transform_blocks(
    blocks: &mut [BlockNode],
    opts: &MathBlockOptions,
    math_renderer: Option<MathRendererRef<'_>>,
) {
    for block in blocks.iter_mut() {
        match block {
            BlockNode::CodeBlock(code) if code.lang.as_deref() == Some(opts.language.as_str()) => {
                // Merge the `math display` base class ahead of author classes
                // and copy the author attributes, mirroring core display `$$`
                // math (class first, then id / key-values in source order).
                // render_attrs_after_class drops dangerous names (`on*`,
                // `srcdoc`, `formaction`) and neutralizes dangerous values, so
                // a `{onclick=...}` on a ```math fence can never reach output.
                let base = "math display";
                let (class, rest) = match &code.attrs {
                    Some(a) if !a.classes.is_empty() => (
                        format!("{} {}", base, a.classes.join(" ")),
                        crate::render::render_attrs_after_class(a),
                    ),
                    Some(a) => (base.to_string(), crate::render::render_attrs_after_class(a)),
                    None => (base.to_string(), String::new()),
                };
                // Static-with-renderer: the build renderer's verbatim SSR output
                // (display = true). Else (interactive, or static with no math
                // renderer): the `\[…\]` source with the body HTML-escaped.
                let body = match math_renderer {
                    Some(build) => build(&code.content, true),
                    None => format!("\\[{}\\]", crate::escape::escape_text(&code.content)),
                };
                let html = format!(
                    "<div class=\"{}\"{}>{}</div>",
                    crate::escape::escape_attr(&class),
                    rest,
                    body,
                );
                *block = BlockNode::RawBlock(RawBlock {
                    format: "html".into(),
                    content: html,
                });
            }
            BlockNode::List(l) => {
                for item in &mut l.items {
                    transform_blocks(&mut item.children, opts, math_renderer);
                }
            }
            BlockNode::BlockQuote(b) => transform_blocks(&mut b.children, opts, math_renderer),
            BlockNode::Admonition(a) => transform_blocks(&mut a.children, opts, math_renderer),
            BlockNode::Div(d) => transform_blocks(&mut d.children, opts, math_renderer),
            BlockNode::Extension(e) => transform_blocks(&mut e.children, opts, math_renderer),
            BlockNode::DefinitionList(dl) => {
                for item in &mut dl.items {
                    for def in &mut item.definitions {
                        transform_blocks(def, opts, math_renderer);
                    }
                }
            }
            _ => {}
        }
    }
}
