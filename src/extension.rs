//! Extension contracts for opt-in Carve behavior.
//!
//! The model mirrors the language-level extension lifecycle:
//! parse matchers, `after_parse`, `before_render`, then renderer hooks.
//! Implementations register extensions through [`Options`].

use std::collections::BTreeMap;
use std::collections::HashMap;

use crate::ast::{BlockExtension, BlockNode, Document, InlineExtension, InlineNode};
use crate::escape::{escape_attr, escape_text};
use crate::parse::{parse_blocks_with_options, parse_inline_with_options};
use crate::profile::Profile;

/// Render mode - a render OPTION, not document syntax. See the
/// [extensions contract](https://markup-carve.github.io/carve/extensions)
/// §2.5 "Static rendering mode".
///
/// - [`Mode::Interactive`] (default): online HTML - extensions render their
///   interactive form (live disclosures, mermaid via a client script, KaTeX).
/// - [`Mode::Static`]: HTML for a medium that cannot interact or run client
///   scripts (print, PDF source, archival HTML). Extensions render through
///   their static path (disclosures expand, diagrams/math become build-rendered
///   output or source). The Markdown, plain-text and ANSI renderers are
///   inherently static and reach the same end by flattening containers (they do
///   not consult this mode).
///
/// `"print"` / `"email"` and similar names are RESERVED for future named
/// presets; this enum admits only the two valid values, so an unknown mode is
/// rejected by construction (the spec requires rejecting, not guessing).
/// Omitting the mode means [`Mode::Interactive`], so existing callers are
/// unaffected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    /// Live HTML (the default).
    #[default]
    Interactive,
    /// Self-contained HTML for print / PDF / archival - no client scripts.
    Static,
}

/// A build-time renderer for a diagram extension (mermaid / chart): source
/// string in, self-contained HTML out (an `<svg>` / `<img>`).
pub type DiagramRenderer = Box<dyn Fn(&str) -> String + 'static>;

/// A build-time renderer for math: the TeX source plus a `display` flag in,
/// MathML / HTML out.
pub type MathRenderer = Box<dyn Fn(&str, bool) -> String + 'static>;

/// A borrowed diagram renderer, as threaded into an extension's static path.
pub(crate) type DiagramRendererRef<'r> = &'r (dyn Fn(&str) -> String + 'static);

/// A borrowed math renderer, as threaded into an extension's static path.
pub(crate) type MathRendererRef<'r> = &'r (dyn Fn(&str, bool) -> String + 'static);

/// Build-time renderers for client-script extensions, supplied for a
/// [`Mode::Static`] HTML render. Each maps the construct's source to a
/// self-contained string the engine emits directly (an `<svg>` / `<img>` for a
/// diagram, MathML / HTML for math). When the renderer a node needs is absent,
/// the extension's static path falls back to source - never blank.
///
/// The map is **open**: a diagram renderer is keyed by the fence's css class
/// (`mermaid`, `chart`, `graphviz`, `plantuml`, or any custom fence word), so a
/// custom [`FencedRender`] instance is static-capable without any change to this
/// type. `math` is the one distinct entry (its closure also takes a display
/// flag). A caller injects renderers with the builder:
///
/// ```
/// use carve::{Mode, Options, StaticRenderers};
/// let renderers = StaticRenderers::new()
///     .diagram("mermaid", |src: &str| format!("<svg data-len=\"{}\"></svg>", src.len()))
///     .diagram("myuml", |src: &str| format!("<img data-len=\"{}\">", src.len()))
///     .math(|tex: &str, display: bool| format!("<math data-display=\"{display}\">{tex}</math>"));
/// let opts = Options::new().with_mode(Mode::Static).with_renderers(renderers);
/// ```
#[derive(Default)]
pub struct StaticRenderers {
    /// Diagram renderers keyed by fence css class. source -> SVG / HTML string.
    pub diagrams: HashMap<String, DiagramRenderer>,
    /// Math TeX source -> MathML / HTML string. The `bool` flags display math.
    pub math: Option<MathRenderer>,
}

impl StaticRenderers {
    /// An empty renderer set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a diagram renderer under a css-class key (chainable). Any key is
    /// accepted, including a custom fence word's class.
    pub fn diagram<F>(mut self, key: impl Into<String>, f: F) -> Self
    where
        F: Fn(&str) -> String + 'static,
    {
        self.diagrams.insert(key.into(), Box::new(f));
        self
    }

    /// Register the math renderer (chainable).
    pub fn math<F>(mut self, f: F) -> Self
    where
        F: Fn(&str, bool) -> String + 'static,
    {
        self.math = Some(Box::new(f));
        self
    }

    /// Borrow the diagram renderer registered under `key`, if any.
    pub fn get_diagram(&self, key: &str) -> Option<DiagramRendererRef<'_>> {
        self.diagrams.get(key).map(|b| b.as_ref())
    }
}

impl std::fmt::Debug for StaticRenderers {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut keys: Vec<&str> = self.diagrams.keys().map(String::as_str).collect();
        keys.sort_unstable();
        f.debug_struct("StaticRenderers")
            .field("diagrams", &keys)
            .field("math", &self.math.as_ref().map(|_| "<fn>"))
            .finish()
    }
}

/// Whether smart typography renders as its glyph or as the source run the
/// author typed.
///
/// Presentation output wants the glyph. Output written for a machine to read
/// is usually better off with the characters that were actually typed: the
/// glyph is a presentation choice the consumer did not ask for and cannot
/// undo, and a search for the source spelling misses it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SmartTypographyMode {
    /// Render the resolved glyph, so `...` becomes an ellipsis. The default.
    #[default]
    Glyph,
    /// Render the author's source run, so `...` stays three periods.
    Source,
}

/// The `labels` key for the endnote backlink's accessible name (PART 9 §16a).
pub const LABEL_FOOTNOTE_BACKLINK: &str = "footnoteBacklink";

/// The English default for a `labels` key, and the whole set: one string.
pub fn label_default(key: &str) -> &'static str {
    match key {
        LABEL_FOOTNOTE_BACKLINK => "Back to reference",
        _ => "",
    }
}

pub struct Options<'a> {
    pub extensions: Vec<&'a dyn CarveExtension>,
    pub mention_url: Option<String>,
    pub tag_url: Option<String>,
    pub symbols: BTreeMap<String, String>,
    /// The strings the ENGINE writes rather than the author (PART 9 §16a).
    ///
    /// A key left out keeps its English default; [`label_default`] is the whole
    /// set, one key today. NOT the `symbols` map's twin: a symbol is emitted
    /// RAW because processor configuration is trusted, a label is TEXT and is
    /// escaped where it lands, so a host feeding these from a translation
    /// catalog is not handing the renderer an injection vector.
    ///
    /// A caption's label is the author's word (`^ Figure #: …`), and
    /// extension-written strings stay options on the extension that writes them
    /// (`HeadingPermalinks`' `aria_label`, ...), so this map stays small on
    /// purpose.
    pub labels: BTreeMap<String, String>,
    /// Allow raw HTML passthrough (`` `…`{=html} `` inline and ` ```=html `
    /// block) to emit verbatim. Default `true` (matches the corpus). Set
    /// `false` for UNTRUSTED input: raw-HTML content is then escaped to text
    /// instead of emitted, closing the one author-controlled raw-HTML vector.
    pub allow_raw_html: bool,
    /// Whether the Markdown renderer emits smart-typography glyphs or the
    /// author's source run. Default `Glyph`; only the Markdown target reads
    /// this.
    pub smart_typography: SmartTypographyMode,
    /// When `true`, lowercase the kept characters of an auto-generated heading
    /// id per code point (`char::to_lowercase`). Default `false`: heading ids
    /// are CASE-PRESERVING (`# Getting Started` -> `Getting-Started`), matching
    /// carve-js / carve-php. carve-rs has no ASCII transliterator, so
    /// ascii-folding is intentionally unsupported here; only `lowercase` is.
    pub lowercase_heading_ids: bool,
    /// When `true`, stamp each top-level block element with a `data-source-line`
    /// attribute holding the 1-based source line where the block starts, so an
    /// editor live-preview can sync scroll to the source. Opt-in (default
    /// `false`) so normal rendering output is unchanged.
    pub source_lines: bool,
    /// Record a source span on every node that supports one (spec PART 12
    /// section 4).
    ///
    /// Off by default, and the spec allows that: tracking may be gated behind a
    /// parse option, and must be enabled when serializing. An ordinary parse
    /// does not pay for spans it never reads.
    pub positions: bool,
    /// Optional feature-restriction profile. When set, disallowed nodes are
    /// converted to text / stripped / error'd per the profile's action,
    /// link/image URLs are gated by its link policy, and `max_nesting` /
    /// `max_length` are enforced. The transform runs on the parsed document
    /// before rendering, so it holds for every renderer. See [`Profile`].
    pub profile: Option<Profile>,
    /// Current document host, used by the profile's link policy to tell
    /// internal from external links.
    pub profile_base_host: Option<String>,
    /// Render mode. [`Mode::Interactive`] (default) emits live HTML; the HTML
    /// renderer's [`Mode::Static`] path flattens interactive constructs and
    /// degrades client-script visuals (mermaid / chart / math) to a supplied
    /// build renderer's output, else source. Ignored by the Markdown,
    /// plain-text and ANSI renderers (they are inherently static). See [`Mode`].
    pub mode: Mode,
    /// Build-time renderers for client-script extensions, consulted only on the
    /// HTML [`Mode::Static`] path. See [`StaticRenderers`].
    pub renderers: StaticRenderers,
    /// Wrap each top-level heading, and the content following it up to the next
    /// same-or-shallower heading, in a `<section id="{slug}">` (spec PART 9
    /// §13). Default `true`, which is what the conformance corpus pins.
    ///
    /// Set `false` to render headings flat: the id goes back on the `<h*>`
    /// alongside its other attributes, and the blocks that would have been the
    /// section's children stay as siblings - so they also lose the two-space
    /// indentation they carried as container children.
    ///
    /// ```text
    /// # A          sections: true      sections: false
    /// p            <section id="A">    <h1 id="A">A</h1>
    ///                <h1>A</h1>        <p>p</p>
    ///                <p>p</p>
    ///              </section>
    /// ```
    ///
    /// For a site whose CSS or JS assumes rendered blocks are direct children
    /// of the content container (the `.stack > * + *` spacing idiom,
    /// `:first-child`, `nth-child()` counting, DOM child walks), the wrapper is
    /// the one output change a clean source migration still breaks.
    ///
    /// Nothing else is affected: ids, collision dedup, `</#id>` crossrefs,
    /// implicit `[Heading][]` references and heading numbering all resolve
    /// against the slug rather than the element carrying it. The endnotes
    /// `<section role="doc-endnotes">` is a separate construct and is emitted
    /// either way. A heading inside a container was never wrapped, so it
    /// renders identically under both settings.
    pub sections: bool,
}

impl Default for Options<'_> {
    fn default() -> Self {
        Self {
            extensions: Vec::new(),
            mention_url: None,
            tag_url: None,
            symbols: BTreeMap::new(),
            labels: BTreeMap::new(),
            allow_raw_html: true,
            smart_typography: SmartTypographyMode::Glyph,
            lowercase_heading_ids: false,
            source_lines: false,
            positions: false,
            profile: None,
            profile_base_host: None,
            mode: Mode::Interactive,
            renderers: StaticRenderers::default(),
            sections: true,
        }
    }
}

impl<'a> Options<'a> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Allow or suppress raw HTML passthrough. Pass `false` for untrusted
    /// input to escape `=html` raw inline/block content instead of emitting it.
    pub fn with_raw_html(mut self, allow: bool) -> Self {
        self.allow_raw_html = allow;
        self
    }

    /// Stamp top-level block elements with a `data-source-line` attribute
    /// (1-based). Opt-in; for editor preview scroll-sync.
    pub fn with_source_lines(mut self, enabled: bool) -> Self {
        self.source_lines = enabled;
        self
    }

    /// Wrap top-level headings in `<section>`. Default `true`; pass `false` to
    /// render headings flat with the id on the `<h*>`. See [`Options::sections`].
    pub fn with_sections(mut self, enabled: bool) -> Self {
        self.sections = enabled;
        self
    }

    /// Record source spans on AST nodes that support them.
    pub fn with_positions(mut self, enabled: bool) -> Self {
        self.positions = enabled;
        self
    }

    pub fn with_extension(mut self, extension: &'a dyn CarveExtension) -> Self {
        self.extensions.push(extension);
        self
    }

    pub fn with_mention_url(mut self, template: impl Into<String>) -> Self {
        self.mention_url = Some(template.into());
        self
    }

    pub fn with_tag_url(mut self, template: impl Into<String>) -> Self {
        self.tag_url = Some(template.into());
        self
    }

    pub fn with_symbol(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.symbols.insert(name.into(), value.into());
        self
    }

    /// The engine-written string for `key`, or its English default.
    pub fn label(&self, key: &str) -> &str {
        self.labels
            .get(key)
            .map(String::as_str)
            .unwrap_or_else(|| label_default(key))
    }

    /// Override one of the strings the engine writes itself (PART 9 §16a).
    ///
    /// ```
    /// # use carve::Options;
    /// let opts = Options::new().with_label("footnoteBacklink", "Zurück zur Fußnote");
    /// ```
    pub fn with_label(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.labels.insert(key.into(), value.into());
        self
    }

    /// Opt in to lowercasing auto-generated heading ids (default is
    /// case-preserving). See [`Options::lowercase_heading_ids`].
    pub fn with_lowercase_heading_ids(mut self, lowercase: bool) -> Self {
        self.lowercase_heading_ids = lowercase;
        self
    }

    /// Apply a feature-restriction [`Profile`] before rendering.
    pub fn with_profile(mut self, profile: Profile) -> Self {
        self.profile = Some(profile);
        self
    }

    /// Set the base host for the profile's link policy (internal/external
    /// link detection).
    pub fn with_profile_base_host(mut self, host: impl Into<String>) -> Self {
        self.profile_base_host = Some(host.into());
        self
    }

    /// Set the render [`Mode`]. Omitting it leaves [`Mode::Interactive`]
    /// (the default). [`Mode::Static`] only affects the HTML renderer; the
    /// Markdown / plain-text / ANSI renderers are inherently static.
    pub fn with_mode(mut self, mode: Mode) -> Self {
        self.mode = mode;
        self
    }

    /// Supply the [`StaticRenderers`] map for an HTML [`Mode::Static`] render
    /// (build-time mermaid / chart / math renderers). Without a renderer the
    /// matching static path degrades to source, never blank.
    pub fn with_renderers(mut self, renderers: StaticRenderers) -> Self {
        self.renderers = renderers;
        self
    }

    /// True when this render is in HTML static mode.
    pub(crate) fn is_static(&self) -> bool {
        self.mode == Mode::Static
    }
}

pub trait CarveExtension {
    fn name(&self) -> &'static str;

    /// Semantic span attribute names this extension adds (spec PART 9 §10).
    ///
    /// DECLARATIVE ON PURPOSE. The nesting order, the value mapping and §9's
    /// riding rule live in ONE renderer; an extension that re-implemented them
    /// for its own names would be a second copy of the feature, drifting from
    /// core the first time either side changed. So an extension names what it
    /// claims and core renders it.
    fn semantic_span_names(&self) -> &'static [&'static str] {
        &[]
    }

    fn match_inline(
        &self,
        _text: &str,
        _pos: usize,
        _ctx: &MatcherContext<'_>,
    ) -> Option<InlineMatch> {
        None
    }

    fn match_block(
        &self,
        _lines: &[&str],
        _start: usize,
        _ctx: &MatcherContext<'_>,
    ) -> Option<BlockMatch> {
        None
    }

    fn after_parse(&self, doc: Document) -> Document {
        doc
    }

    /// `beforeRender` transform. Receives a [`BeforeRenderContext`] carrying the
    /// render [`Options`] AND the *effective* [`Mode`] for the actual target
    /// format. An extension that emits its final HTML here (the carve-rs
    /// transform model for `FencedRender` / `MathBlock`) branches on
    /// `ctx.mode()` / `ctx.is_static()` and consults `ctx.options().renderers`.
    ///
    /// The effective mode is `Interactive` for the Markdown / plain-text / ANSI
    /// renderers regardless of `Options::mode` - static rendering is an
    /// HTML-only concern (those renderers reach the same end by flattening), so
    /// a caller reusing one `Options` across formats gets unchanged non-HTML
    /// output. Most extensions ignore this hook entirely.
    fn before_render(&self, doc: Document, _ctx: &BeforeRenderContext<'_>) -> Document {
        doc
    }

    fn render_inline_extension(
        &self,
        _node: &InlineExtension,
        _ctx: &RenderContext<'_>,
    ) -> Option<String> {
        None
    }

    fn render_block_extension(
        &self,
        _node: &BlockExtension,
        _ctx: &RenderContext<'_>,
    ) -> Option<String> {
        None
    }
}

/// Context handed to [`CarveExtension::before_render`]. Carries the render
/// [`Options`] and the *effective* [`Mode`] for the target format - which is
/// always [`Mode::Interactive`] for the non-HTML renderers, so static rendering
/// stays an HTML-only concern even when a single `Options` is reused across
/// formats.
pub struct BeforeRenderContext<'a> {
    options: &'a Options<'a>,
    effective_mode: Mode,
    target_is_html: bool,
}

impl<'a> BeforeRenderContext<'a> {
    pub(crate) fn new(
        options: &'a Options<'a>,
        effective_mode: Mode,
        target_is_html: bool,
    ) -> Self {
        Self {
            options,
            effective_mode,
            target_is_html,
        }
    }

    /// The render options.
    pub fn options(&self) -> &Options<'a> {
        self.options
    }

    /// The effective render [`Mode`] for the target format.
    pub fn mode(&self) -> Mode {
        self.effective_mode
    }

    /// True when the effective mode is [`Mode::Static`] (HTML static path).
    pub fn is_static(&self) -> bool {
        self.effective_mode == Mode::Static
    }

    /// True when the final render target is HTML. The Markdown / plain / ANSI
    /// renderers set this `false`, so an extension that emits HTML in
    /// `before_render` (e.g. a client-script hydration element) can skip its
    /// transform and leave the source node for the non-HTML renderer to emit as
    /// source.
    pub fn target_is_html(&self) -> bool {
        self.target_is_html
    }

    /// The build-time [`StaticRenderers`] map (shorthand for
    /// `self.options().renderers`).
    pub fn renderers(&self) -> &StaticRenderers {
        &self.options.renderers
    }
}

pub struct InlineMatch {
    pub node: InlineNode,
    pub end: usize,
}

pub struct BlockMatch {
    pub node: BlockNode,
    pub lines_consumed: usize,
}

pub struct MatcherContext<'a> {
    options: &'a Options<'a>,
}

impl<'a> MatcherContext<'a> {
    pub(crate) fn new(options: &'a Options<'a>) -> Self {
        Self { options }
    }

    pub fn parse_inlines(&self, text: &str) -> Vec<InlineNode> {
        parse_inline_with_options(text, self.options)
    }

    pub fn parse_blocks(&self, source: &str) -> Vec<BlockNode> {
        parse_blocks_with_options(source, self.options)
    }
}

pub struct RenderContext<'a> {
    pub(crate) options: &'a Options<'a>,
    /// Indentation level of the node currently being rendered. Zero on the
    /// inline path (inline extensions never indent); set to the block node's
    /// level when a block extension is rendered, so a block-extension renderer
    /// can emit nesting-aware indentation (mirrors carve-js
    /// `BlockExtensionRenderContext.level`).
    level: usize,
    /// The live document render state (heading-id counter) when rendering a
    /// block extension, so nested rendering continues the document's heading
    /// numbering and depth budget across the extension boundary. `None` on the
    /// inline path.
    state: Option<&'a std::cell::RefCell<&'a mut crate::render::RenderState>>,
}

impl<'a> RenderContext<'a> {
    pub(crate) fn new(options: &'a Options<'a>) -> Self {
        Self {
            options,
            level: 0,
            state: None,
        }
    }

    pub(crate) fn with_level_and_state(
        options: &'a Options<'a>,
        level: usize,
        state: &'a std::cell::RefCell<&'a mut crate::render::RenderState>,
    ) -> Self {
        Self {
            options,
            level,
            state: Some(state),
        }
    }

    pub fn render_inlines(&self, nodes: &[InlineNode]) -> String {
        crate::render::render_inlines_with_options(nodes, self.options)
    }

    /// [`RenderContext::render_inlines`], for nodes the caller is about to place
    /// inside an anchor of its own. A construct that would open its own anchor
    /// renders its non-anchor form instead (PART 12 section 3a, LINKS NEVER
    /// NEST) - what a table-of-contents entry needs, since the entry is written
    /// into an `<a href="#id">` the extension emits.
    pub fn render_inlines_inside_anchor(&self, nodes: &[InlineNode]) -> String {
        crate::render::render_inlines_inside_anchor(nodes, self.options)
    }

    /// Render block nodes at level 0 (no leading indentation), while preserving
    /// the current recursion depth when called by a block extension. Use
    /// [`RenderContext::render_blocks_at`] to render at a specific indentation
    /// level.
    pub fn render_blocks(&self, nodes: &[BlockNode]) -> String {
        match self.state {
            Some(cell) => {
                let mut state = cell.borrow_mut();
                crate::render::render_blocks_with_state_from_depth(
                    nodes,
                    self.options,
                    self.level.saturating_add(1),
                    &mut state,
                )
            }
            None => crate::render::render_blocks_with_options(nodes, self.options),
        }
    }

    /// Render block nodes indented to `level`, matching the core renderer's
    /// two-space-per-level layout. A block-extension renderer uses this to
    /// place its children at `ctx.level() + 1` so a details/disclosure block
    /// nests identically wherever it sits (top level, list item, blockquote).
    /// When this context carries the live document state, the heading-id
    /// counter continues across the extension boundary (a duplicate slug gets
    /// its numeric suffix); otherwise it falls back to a fresh counter.
    pub fn render_blocks_at(&self, nodes: &[BlockNode], level: usize) -> String {
        match self.state {
            Some(cell) => {
                let mut state = cell.borrow_mut();
                crate::render::render_blocks_at_with_state(nodes, self.options, level, &mut state)
            }
            None => crate::render::render_blocks_at_with_options(nodes, self.options, level),
        }
    }

    /// The indentation level of the block node being rendered. Zero on the
    /// inline path.
    pub fn level(&self) -> usize {
        self.level
    }

    /// The two-space-per-level indent string for `level`.
    pub fn indent(&self, level: usize) -> String {
        "  ".repeat(level)
    }

    /// The active render [`Mode`]. A block-extension renderer branches on this
    /// to emit its static (flattened, no-interaction) form.
    pub fn mode(&self) -> Mode {
        self.options.mode
    }

    /// True when this render is in HTML static mode.
    pub fn is_static(&self) -> bool {
        self.options.is_static()
    }

    /// The build-time [`StaticRenderers`] map for this render. Used by a
    /// client-script extension's static path to server-render mermaid / chart /
    /// math, falling back to source when the needed renderer is absent.
    pub fn renderers(&self) -> &StaticRenderers {
        &self.options.renderers
    }

    /// Reserve a DOM id in the shared per-render document id namespace (see
    /// the [extensions contract](https://markup-carve.github.io/carve/extensions)
    /// §2.6 "Generated ids"): returns `base_id` when the name is free, else
    /// the next free numeric suffix (`base_id-2`, `-3`, ...), never colliding
    /// with an explicit `{#id}` attribute, a generated heading id, or a
    /// previously generated id. The namespace is seeded from the whole
    /// document before rendering starts, so first-in-source wins. Outside an
    /// active HTML render the base id is returned unchanged.
    pub fn unique_id(&self, base_id: &str) -> String {
        crate::document_ids::unique_id(base_id)
    }

    pub fn escape_html(&self, input: &str) -> String {
        escape_text(input)
    }

    pub fn escape_attr(&self, input: &str) -> String {
        escape_attr(input)
    }
}
