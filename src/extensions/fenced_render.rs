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
//!   rewritten to `<\/` so it cannot close the script element early. Note:
//!   consumers that sanitize the HTML after conversion should whitelist that
//!   `<script>` tag or use text mode (the config then rides in a `<pre>` as
//!   escaped text).
//!
//! Author attributes on the fence are copied onto the wrapper and hardened
//! exactly as the core renderer hardens every element (`is_dangerous_attr_name`
//! strips `on*` / `srcdoc` / `formaction`; `sanitize_attr_value` neutralizes
//! dangerous URL / `expression()` values), so a `{onclick=...}` fence cannot
//! inject.

use crate::ast::{AttrSlot, Attrs, BlockNode, Document, RawBlock};
use crate::escape::{
    escape_attr, escape_text, is_dangerous_attr_name, is_valid_attr_name, sanitize_attr_value,
};
use crate::extension::{BeforeRenderContext, CarveExtension, DiagramRendererRef};

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

    /// Graphviz preset (text mode); claims both `dot` and `graphviz`. On the
    /// HTML static path a supplied `renderers.graphviz` pre-renders the source
    /// to an image; absent that, it degrades to the source as a `<pre><code>`.
    pub fn graphviz() -> Self {
        let opts = FencedRenderOptions::new(
            vec!["dot".into(), "graphviz".into()],
            Some("graphviz".into()),
            None,
            ContentMode::Text,
        );
        Self::with_options(opts)
    }

    /// WaveDrom preset (text mode, `<pre class="wavedrom">`).
    pub fn wavedrom() -> Self {
        Self::new("wavedrom")
    }

    /// ABC music notation preset (text mode, `<pre class="abc">`).
    pub fn abc() -> Self {
        Self::new("abc")
    }

    /// PlantUML preset (text mode); claims both `plantuml` and `puml`. Covers
    /// the UML shapes Mermaid does not (use case, component, deployment,
    /// timing). Load a client-side PlantUML build to render the diagrams.
    pub fn plantuml() -> Self {
        let opts = FencedRenderOptions::new(
            vec!["plantuml".into(), "puml".into()],
            Some("plantuml".into()),
            None,
            ContentMode::Text,
        );
        Self::with_options(opts)
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

    /// Chart.js preset (json mode, `<div class="chart"><script ...>`). On the
    /// HTML static path a supplied `renderers.chart` pre-renders the config to
    /// an image; absent that, it degrades to the JSON source as a `<pre><code>`.
    pub fn chart() -> Self {
        let opts = FencedRenderOptions::new(vec!["chart".into()], None, None, ContentMode::Json);
        Self::with_options(opts)
    }

    /// Every bundled diagram preset as ready-to-register instances.
    ///
    /// Claims every preset fence word (`mermaid`, `d2`, `dot`, `graphviz`,
    /// `wavedrom`, `abc`, `plantuml`, `puml`, `vega-lite`, `chart`), so a
    /// literal code sample in one
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
            Self::plantuml(),
            Self::vega_lite(),
            Self::chart(),
        ]
    }
}

impl CarveExtension for FencedRender {
    fn name(&self) -> &'static str {
        "fenced-render"
    }

    fn before_render(&self, mut doc: Document, ctx: &BeforeRenderContext<'_>) -> Document {
        // Only the HTML renderer emits the hydration / static element. For the
        // Markdown / plain / ANSI targets, leave the CodeBlock untouched so the
        // renderer emits it as its source fence (matching carve-php / carve-js),
        // rather than an escaped `<pre>` raw-HTML block (carve#305).
        if !ctx.target_is_html() {
            return doc;
        }
        // On the HTML static path the client-script diagram cannot be drawn by
        // the engine. Resolve this instance's build renderer (if it declares one
        // and the caller supplied it): a present renderer SSR-renders the source;
        // absent, the static path degrades to an escaped `<pre><code>` source
        // block. In interactive mode this is `None` and the live hydration
        // element is emitted as before. The effective mode is interactive for
        // the non-HTML renderers (static rendering is HTML-only), so reusing one
        // `Options` across formats leaves Markdown / ANSI output unchanged.
        // Mirrors carve-js `fenced-render.ts` `staticBlockRenderers`.
        let static_build: Option<DiagramRendererRef<'_>> = if ctx.is_static() {
            ctx.renderers().get_diagram(&self.opts.css_class)
        } else {
            None
        };
        let mode = StaticState {
            is_static: ctx.is_static(),
            build: static_build,
        };
        transform_blocks(&mut doc.children, &self.opts, &mode);
        // Footnote bodies render from footnote_defs (outside the tree), so a
        // claimed block inside a footnote must be transformed too.
        for blocks in doc.footnote_defs.values_mut() {
            transform_blocks(blocks, &self.opts, &mode);
        }
        doc
    }
}

/// Resolved static-render decision for a `FencedRender` instance, computed once
/// per `before_render`: whether HTML static mode is active and, if so, the
/// build renderer to SSR this instance's source (else `None` -> degrade to a
/// `<pre><code>` source block).
pub(crate) struct StaticState<'r> {
    is_static: bool,
    build: Option<DiagramRendererRef<'r>>,
}

/// Rewrite every claimed code block in `blocks` into its hydration `raw-block`
/// (interactive) or its static form (`mode.is_static`): the build renderer's
/// SSR output when supplied, else an escaped `<pre><code>` source block.
pub(crate) fn transform_blocks(
    blocks: &mut [BlockNode],
    opts: &FencedRenderOptions,
    mode: &StaticState<'_>,
) {
    for block in blocks.iter_mut() {
        match block {
            BlockNode::CodeBlock(code)
                if code
                    .lang
                    .as_deref()
                    .map(|l| opts.languages.iter().any(|w| w == l))
                    .unwrap_or(false) =>
            {
                let html = if mode.is_static {
                    static_html(code, opts, mode.build)
                } else {
                    interactive_html(code, opts)
                };
                *block = BlockNode::RawBlock(RawBlock {
                    format: "html".into(),
                    content: html,
                    // Synthesized by an extension: no source span to report (PART 12 §4).
                    pos: None,
                });
            }
            BlockNode::List(l) => {
                for item in &mut l.items {
                    transform_blocks(&mut item.children, opts, mode);
                }
            }
            BlockNode::BlockQuote(b) => transform_blocks(&mut b.children, opts, mode),
            BlockNode::Admonition(a) => transform_blocks(&mut a.children, opts, mode),
            BlockNode::Div(d) => transform_blocks(&mut d.children, opts, mode),
            BlockNode::Extension(e) => transform_blocks(&mut e.children, opts, mode),
            BlockNode::DefinitionList(dl) => {
                for item in &mut dl.items {
                    for def in &mut item.definitions {
                        transform_blocks(def, opts, mode);
                    }
                }
            }
            _ => {}
        }
    }
}

/// The interactive client-hydration element (the original behavior).
fn interactive_html(code: &crate::ast::CodeBlock, opts: &FencedRenderOptions) -> String {
    let attrs = merged_attrs(code, opts);
    let body = match opts.content_mode {
        ContentMode::Text => escape_text_keep_gt(&code.content),
        ContentMode::Json => format!(
            "<script type=\"application/json\">{}</script>",
            guard_script_close(&code.content)
        ),
    };
    let element = format!("<{0}{1}>{2}</{0}>", opts.tag, render_attrs(&attrs), body);
    wrap_figure(element, opts)
}

/// The HTML static-path output: the build renderer's verbatim SSR output when
/// supplied (an `<svg>` / `<img>`), else the source as a self-contained,
/// HTML-escaped `<pre><code class="language-LANG">…\n</code></pre>` block
/// (fence attributes preserved). Never blank. Mirrors carve-js
/// `fenced-render.ts` `staticBlockRenderers`.
fn static_html(
    code: &crate::ast::CodeBlock,
    opts: &FencedRenderOptions,
    build: Option<DiagramRendererRef<'_>>,
) -> String {
    if let Some(build) = build {
        // Wrap the renderer's output in a `<div>` carrying the fence's merged
        // attributes (cssClass + author `{#id .class}`), so the class/attrs
        // survive and the wrapper is identical across engines (carve#302).
        let attrs = merged_attrs(code, opts);
        let element = format!(
            "<div{}>{}</div>",
            render_attrs(&attrs),
            build(&code.content)
        );
        return wrap_figure(element, opts);
    }
    // Source fallback: merge the cssClass ahead of author classes and copy the
    // author attributes (same hardening as the interactive path), so an
    // `{#id .class data-x=y}` on the fence survives the degradation path.
    let attrs = merged_attrs(code, opts);
    let lang_attr = match &code.lang {
        Some(l) => format!(" class=\"language-{}\"", escape_attr(l)),
        None => String::new(),
    };
    format!(
        "<pre{}><code{}>{}\n</code></pre>",
        render_attrs(&attrs),
        lang_attr,
        escape_text(&code.content),
    )
}

/// Merge the cssClass into the front of the class group, keeping the block's
/// own attributes and their source order.
fn merged_attrs(code: &crate::ast::CodeBlock, opts: &FencedRenderOptions) -> Attrs {
    let mut attrs = code.attrs.clone().unwrap_or_default();
    let mut classes = vec![opts.css_class.clone()];
    classes.extend(attrs.classes.iter().cloned());
    attrs.classes = classes;
    ensure_class_slot(&mut attrs);
    attrs
}

/// Wrap an element in `<figure class="{figure_class}">` when configured.
fn wrap_figure(element: String, opts: &FencedRenderOptions) -> String {
    if opts.wrap_in_figure {
        format!(
            "<figure class=\"{}\">\n{}\n</figure>",
            escape_attr(&opts.figure_class),
            element
        )
    } else {
        element
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
        if !is_dangerous_attr_name(key) && is_valid_attr_name(key) {
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
