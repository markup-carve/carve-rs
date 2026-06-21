//! Generic client-rendered fenced-block factory (Tier-3).
//!
//! Port of carve#167 / carve-php `FencedRenderExtension` / carve-js
//! `fencedRender`. Claims fenced code blocks by language word and rewrites each
//! match into a `raw-block` of one hydration element; the block body is passed
//! through verbatim. Mermaid is one preset of this client-hydration shape,
//! generalized so D2, Graphviz, WaveDrom, ABC, Vega-Lite, Chart.js, etc. need no
//! new code.
//!
//! - Text mode (Mermaid/D2/Graphviz/WaveDrom/ABC): the body is HTML-escaped
//!   (`&` and `<`), but `>` is preserved so arrow syntax (`-->`) survives.
//! - JSON mode (Vega-Lite/Chart.js): the body is emitted verbatim inside a
//!   `<script type="application/json">` (default wrapper `<div>`), with `</`
//!   rewritten to `<\/` so it cannot close the script element early.
//!
//! Author attributes on the fence are copied onto the wrapper and hardened
//! exactly as the core renderer hardens every element (`is_dangerous_attr_name`
//! strips `on*` / `srcdoc` / `formaction`; `sanitize_attr_value` neutralizes
//! dangerous URL / `expression()` values), so a `{onclick=...}` fence cannot
//! inject.

use crate::ast::{AttrSlot, Attrs, BlockNode, Document, RawBlock};
use crate::escape::{escape_attr, is_dangerous_attr_name, sanitize_attr_value};
use crate::extension::CarveExtension;

/// How a [`FencedRender`] instance places the block body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentMode {
    /// HTML-escaped text inside the wrapper element.
    Text,
    /// Verbatim body inside a `<script type="application/json">`.
    Json,
}

/// Options for [`FencedRender`].
#[derive(Debug, Clone)]
pub struct FencedRenderOptions {
    /// Fence info word(s) this instance claims.
    pub languages: Vec<String>,
    /// Class on the output element.
    pub css_class: String,
    /// Wrapper element (`pre` or `div`).
    pub tag: String,
    /// How the body is placed.
    pub content_mode: ContentMode,
    /// Wrap output in `<figure class="{css_class}-figure">`.
    pub wrap_in_figure: bool,
    /// Figure class.
    pub figure_class: String,
}

impl FencedRenderOptions {
    /// Build options, defaulting `css_class` to the first language word, `tag`
    /// to `div` for json mode (else `pre`), and `figure_class` to
    /// `{css_class}-figure`.
    pub fn new(
        languages: Vec<String>,
        css_class: Option<String>,
        tag: Option<String>,
        content_mode: ContentMode,
    ) -> Self {
        let css_class = css_class.unwrap_or_else(|| languages.first().cloned().unwrap_or_default());
        let tag = tag.unwrap_or_else(|| {
            if content_mode == ContentMode::Json {
                "div".into()
            } else {
                "pre".into()
            }
        });
        let figure_class = format!("{css_class}-figure");
        Self {
            languages,
            css_class,
            tag,
            content_mode,
            wrap_in_figure: false,
            figure_class,
        }
    }
}

/// Generic client-rendered fenced-block factory.
///
/// ```
/// use carve::{FencedRender, Options};
/// let ext = FencedRender::d2();
/// let opts = Options::new().with_extension(&ext);
/// assert_eq!(
///     carve::to_html_with_options("``` d2\na -> b\n```\n", &opts),
///     "<pre class=\"d2\">a -> b</pre>"
/// );
/// ```
pub struct FencedRender {
    opts: FencedRenderOptions,
}

impl FencedRender {
    /// Text-mode instance claiming a single language (cssClass = language).
    pub fn new(language: impl Into<String>) -> Self {
        Self::with_options(FencedRenderOptions::new(
            vec![language.into()],
            None,
            None,
            ContentMode::Text,
        ))
    }

    /// Instance with explicit options.
    pub fn with_options(opts: FencedRenderOptions) -> Self {
        Self { opts }
    }

    /// Mermaid preset (text mode, `<pre class="mermaid">`).
    ///
    /// Mermaid is one preset of this factory; load Mermaid.js on the page to
    /// render the diagrams.
    pub fn mermaid() -> Self {
        Self::new("mermaid")
    }

    /// D2 preset (text mode, `<pre class="d2">`).
    pub fn d2() -> Self {
        Self::new("d2")
    }

    /// Graphviz preset (text mode); claims both `dot` and `graphviz`.
    pub fn graphviz() -> Self {
        Self::with_options(FencedRenderOptions::new(
            vec!["dot".into(), "graphviz".into()],
            Some("graphviz".into()),
            None,
            ContentMode::Text,
        ))
    }

    /// WaveDrom preset (text mode, `<pre class="wavedrom">`).
    pub fn wavedrom() -> Self {
        Self::new("wavedrom")
    }

    /// ABC music notation preset (text mode, `<pre class="abc">`).
    pub fn abc() -> Self {
        Self::new("abc")
    }

    /// Vega-Lite preset (json mode, `<div class="vega-lite"><script ...>`).
    pub fn vega_lite() -> Self {
        Self::with_options(FencedRenderOptions::new(
            vec!["vega-lite".into()],
            None,
            None,
            ContentMode::Json,
        ))
    }

    /// Chart.js preset (json mode, `<div class="chart"><script ...>`).
    pub fn chart() -> Self {
        Self::with_options(FencedRenderOptions::new(
            vec!["chart".into()],
            None,
            None,
            ContentMode::Json,
        ))
    }

    /// Every bundled diagram preset as ready-to-register instances.
    ///
    /// Claims every preset fence word (`mermaid`, `d2`, `dot`, `graphviz`,
    /// `wavedrom`, `abc`, `vega-lite`, `chart`), so a literal code sample in one
    /// of those languages becomes a hydration element; register only the presets
    /// whose client library you actually load if that matters.
    ///
    /// ```
    /// use carve::{FencedRender, Options};
    /// let presets = FencedRender::presets();
    /// let mut opts = Options::new();
    /// for ext in &presets {
    ///     opts = opts.with_extension(ext);
    /// }
    /// let html = carve::to_html_with_options("``` mermaid\ngraph TD; A-->B\n```\n", &opts);
    /// assert_eq!(html, "<pre class=\"mermaid\">graph TD; A-->B</pre>");
    /// ```
    pub fn presets() -> Vec<FencedRender> {
        vec![
            Self::mermaid(),
            Self::d2(),
            Self::graphviz(),
            Self::wavedrom(),
            Self::abc(),
            Self::vega_lite(),
            Self::chart(),
        ]
    }
}

impl CarveExtension for FencedRender {
    fn name(&self) -> &'static str {
        "fenced-render"
    }

    fn before_render(&self, mut doc: Document) -> Document {
        transform_blocks(&mut doc.children, &self.opts);
        // Footnote bodies render from footnote_defs (outside the tree), so a
        // claimed block inside a footnote must be transformed too.
        for blocks in doc.footnote_defs.values_mut() {
            transform_blocks(blocks, &self.opts);
        }
        doc
    }
}

/// Rewrite every claimed code block in `blocks` into its hydration `raw-block`.
/// Shared with the `Mermaid` preset.
pub(crate) fn transform_blocks(blocks: &mut [BlockNode], opts: &FencedRenderOptions) {
    for block in blocks.iter_mut() {
        match block {
            BlockNode::CodeBlock(code)
                if code
                    .lang
                    .as_deref()
                    .map(|l| opts.languages.iter().any(|w| w == l))
                    .unwrap_or(false) =>
            {
                // Merge the cssClass into the front of the class group, keeping
                // the block's own attributes and their source order.
                let mut attrs = code.attrs.clone().unwrap_or_default();
                let mut classes = vec![opts.css_class.clone()];
                classes.extend(attrs.classes.iter().cloned());
                attrs.classes = classes;
                ensure_class_slot(&mut attrs);

                let body = match opts.content_mode {
                    ContentMode::Text => escape_text_keep_gt(&code.content),
                    ContentMode::Json => format!(
                        "<script type=\"application/json\">{}</script>",
                        guard_script_close(&code.content)
                    ),
                };
                let element = format!("<{0}{1}>{2}</{0}>", opts.tag, render_attrs(&attrs), body);
                let html = if opts.wrap_in_figure {
                    format!(
                        "<figure class=\"{}\">\n{}\n</figure>",
                        escape_attr(&opts.figure_class),
                        element
                    )
                } else {
                    element
                };
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

/// Ensure the merged class group renders in source-order position: when a node
/// carries an explicit attribute `order` but no class slot, add one at the
/// front (mirroring carve-js, which spreads the merged classes ahead).
fn ensure_class_slot(attrs: &mut Attrs) {
    if attrs.classes.is_empty() {
        return;
    }
    if !attrs.order.is_empty() && !attrs.order.iter().any(|s| matches!(s, AttrSlot::Class)) {
        attrs.order.insert(0, AttrSlot::Class);
    }
}

/// Render attributes the same way the core renderer does, with the always-on
/// attribute hardening: drop dangerous names (`on*` / `srcdoc` / `formaction`),
/// neutralize dangerous values, and escape names + values.
fn render_attrs(attrs: &Attrs) -> String {
    let mut out = String::new();
    let push_kv = |out: &mut String, key: &str, value: &str| {
        if !is_dangerous_attr_name(key) {
            out.push_str(&format!(
                " {}=\"{}\"",
                escape_attr(key),
                escape_attr(&sanitize_attr_value(key, value))
            ));
        }
    };
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
            push_kv(&mut out, key, value);
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
                    push_kv(&mut out, key, value);
                }
            }
        }
    }
    out
}

/// Text mode: encode `&` and `<` but keep `>` so arrow syntax (`A-->B`) survives.
fn escape_text_keep_gt(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;")
}

/// JSON mode: rewrite `</` to `<\/` so the body cannot close the `<script>`
/// element early (byte-equivalent JSON, `\/` decodes to `/`).
fn guard_script_close(s: &str) -> String {
    s.replace("</", "<\\/")
}
