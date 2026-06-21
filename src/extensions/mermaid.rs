//! Render fenced code blocks tagged `mermaid` as `<pre class="mermaid">…</pre>`.
//!
//! Mermaid is a text-mode preset of [`super::fenced_render::FencedRender`]; this
//! type is kept as a named extension for back-compat. A non-mermaid code block
//! is left untouched and defers to the core renderer.

use crate::ast::Document;
use crate::extension::CarveExtension;
use crate::extensions::fenced_render::{transform_blocks, ContentMode, FencedRenderOptions};

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
    opts: FencedRenderOptions,
}

impl Mermaid {
    /// Create a mermaid extension with default options.
    pub fn new() -> Self {
        Self::with_options(MermaidOptions::default())
    }

    /// Create a mermaid extension with explicit options.
    pub fn with_options(opts: MermaidOptions) -> Self {
        Self {
            opts: FencedRenderOptions::new(
                vec![opts.language],
                Some(opts.css_class),
                Some("pre".into()),
                ContentMode::Text,
            ),
        }
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
        for blocks in doc.footnote_defs.values_mut() {
            transform_blocks(blocks, &self.opts);
        }
        doc
    }
}
