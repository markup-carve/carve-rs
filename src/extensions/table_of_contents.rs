//! Generate a table of contents from the document's top-level headings.
//!
//! Port of carve-js `table-of-contents.ts` / carve-php's
//! `TableOfContentsExtension`. A `before_render` transform that collects
//! top-level headings (with their resolved ids), builds a nested `<nav>` of
//! links, and injects it at the top or bottom of the document as a raw HTML
//! block.
//!
//! Heading id resolution mirrors the renderer (case-preserving by default; see
//! [`crate::Options::lowercase_heading_ids`]). To match a document rendered
//! with `with_lowercase_heading_ids(true)`, set
//! [`TableOfContentsOptions::lowercase_ids`] to the same value.

use std::cell::RefCell;
use std::collections::BTreeMap;

use crate::ast::{Attrs, BlockExtension, BlockNode, Document, Heading, InlineNode, RawBlock};
use crate::escape::escape_attr;
use crate::extension::{
    AsciiHeadingIds, BeforeRenderContext, CarveExtension, HeadingIdOptions, RenderContext,
    LABEL_TOC_NAV,
};
use crate::render::render_attrs_without_keys;

/// Carrier extension name a `::: toc` block is rewritten to in `before_render`,
/// then rendered by [`TocPlacement::render_block_extension`].
const TOC_CARRIER: &str = "toc-placement";

/// List element for the TOC entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListType {
    /// `<ul>`
    Ul,
    /// `<ol>`
    Ol,
}

impl ListType {
    fn tag(self) -> &'static str {
        match self {
            ListType::Ul => "ul",
            ListType::Ol => "ol",
        }
    }
}

/// Where to insert the generated TOC.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Position {
    /// Top of the document.
    Top,
    /// Bottom of the document.
    Bottom,
}

/// Options for [`TableOfContents`].
#[derive(Debug, Clone)]
pub struct TableOfContentsOptions {
    /// Lowest heading level to include (1-6). Default 1.
    pub min_level: u8,
    /// Highest heading level to include (1-6). Default 6.
    pub max_level: u8,
    /// List element for the entries. Default [`ListType::Ul`].
    pub list_type: ListType,
    /// CSS class on the `<nav>` container. Default `"toc"`.
    pub css_class: String,
    /// Insert the generated TOC at the top or bottom. Default [`Position::Top`].
    pub position: Position,
    /// Lowercase auto-generated heading ids when computing link targets. Must
    /// match the renderer's [`crate::Options::lowercase_heading_ids`]. Default
    /// false (case-preserving), matching the carve-rs default.
    pub lowercase_ids: bool,
    /// Wrap the entries in a `<details>` / `<summary>` disclosure so the reader
    /// can collapse them. Default false.
    ///
    /// THE DISCLOSURE SHAPE HAS NO `<nav>` AT ALL, so it takes no accessible
    /// name from the `tocNav` label: there is no landmark left to name. The
    /// `<summary>` is visible text in a widget instead, and that is why
    /// [`Self::summary`] is this extension's own option rather than a second
    /// `labels` key beside `tocNav` (Extensions §1.5,
    /// markup-carve/carve#1510). The two strings sit on mutually exclusive
    /// shapes, which is what keeps their near-identical defaults from being one
    /// string wearing two hats.
    pub collapsible: bool,
    /// The disclosure's label. Default `"Table of Contents"`. Only read when
    /// [`Self::collapsible`] is set.
    ///
    /// An OPTION, never a `labels` key. Extensions §1.5 gives a string with a
    /// fixed English default a `labels` key *unless* the extension already
    /// exposes it as an option, and never both - so `tocSummary` stays outside
    /// this engine's `labels` vocabulary, and `carve::label_default` answers
    /// nothing for it.
    pub summary: String,
    /// Render the disclosure expanded. Default false (closed). Only read when
    /// [`Self::collapsible`] is set.
    pub open: bool,
}

impl Default for TableOfContentsOptions {
    fn default() -> Self {
        Self {
            min_level: 1,
            max_level: 6,
            list_type: ListType::Ul,
            css_class: "toc".into(),
            position: Position::Top,
            lowercase_ids: false,
            collapsible: false,
            summary: "Table of Contents".into(),
            open: false,
        }
    }
}

#[derive(Clone)]
struct TocEntry {
    level: u8,
    /// The entry's display text as NODES (PART 9R R4, DERIVED DISPLAY TEXT
    /// CLONES THE SAME NODES, markup-carve/carve#957). Not a string: a node
    /// carries the source run, the emphasis, the code span and the escape, and
    /// flattening here destroys all four before any renderer is invoked.
    ///
    /// The clone is [`crate::parse::derive_display_nodes`] in a LINK context -
    /// an entry is placed inside the `<a>` this list emits.
    label: Vec<InlineNode>,
    id: String,
}

/// Generate a table of contents from the document's headings.
///
/// ```
/// use carve::{TableOfContents, TableOfContentsOptions, Options};
/// let opts = TableOfContentsOptions { lowercase_ids: true, ..Default::default() };
/// let ext = TableOfContents::with_options(opts);
/// let options = Options::new()
///     .with_extension(&ext)
///     .with_lowercase_heading_ids(true);
/// let html = carve::to_html_with_options("# Intro\n\n## Details", &options);
/// assert!(html.starts_with("<nav class=\"toc\" aria-label=\"Table of contents\">"));
/// ```
///
/// Set `collapsible` to wrap the entries in a `<details>` the reader can fold
/// away. The disclosure emits no `<nav>`, so it carries no `aria-label`; the
/// `<summary>` is the visible text instead, and `summary` sets it.
///
/// ```
/// use carve::{TableOfContents, TableOfContentsOptions, Options};
/// let opts = TableOfContentsOptions {
///     collapsible: true,
///     summary: "Contents".into(),
///     open: true,
///     ..Default::default()
/// };
/// let ext = TableOfContents::with_options(opts);
/// let html = carve::to_html_with_options("# Intro", &Options::new().with_extension(&ext));
/// assert!(html.starts_with("<details class=\"toc\" open>\n<summary>Contents</summary>"));
/// ```
pub struct TableOfContents {
    opts: TableOfContentsOptions,
}

impl TableOfContents {
    /// Create a TOC extension with default options.
    pub fn new() -> Self {
        Self::with_options(TableOfContentsOptions::default())
    }

    /// Create a TOC extension with explicit options.
    pub fn with_options(opts: TableOfContentsOptions) -> Self {
        Self { opts }
    }
}

impl Default for TableOfContents {
    fn default() -> Self {
        Self::new()
    }
}

impl CarveExtension for TableOfContents {
    fn name(&self) -> &'static str {
        "table-of-contents"
    }

    fn before_render(&self, mut doc: Document, ctx: &BeforeRenderContext<'_>) -> Document {
        // A TOC entry is the heading's NODES, so nothing here spells them: the
        // document-global smart-typography mode (PART 9 §19), the symbols map
        // and the raw-HTML policy are the RENDERER's, and the nodes are handed
        // to it below with the caller's own options. Deriving a string here used
        // to settle the typography switch in a pre-render pass.
        //
        // The id it links to still keeps the glyph (`next_id`), as heading ids
        // must - PART 9 §19 pins them byte-identical in both modes.
        //
        // The renderer allocates ids over ALL headings in document order
        // (including nested ones). Reproduce that counter so a top-level
        // heading's link target matches the `<section id>` the core emits.
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        let mut entries: Vec<TocEntry> = Vec::new();
        collect_entries(&doc.children, &mut counts, &self.opts, &mut entries, true);

        if entries.is_empty() {
            return doc;
        }

        // `<nav>` is a navigation landmark unconditionally, so an unnamed one is
        // an entry in a reader's landmark list reading only "navigation" - and a
        // page holds more than one the moment `TocPlacement` is registered
        // beside this extension, or a site template contributes its own
        // (Extensions §8b.1, markup-carve/carve#1509). Named from the SAME
        // `labels` key `TocPlacement` reads, so the nav fragment §8b.3 makes the
        // cross-impl contract stays byte-identical between the two.
        let list = build_list(&entries, self.opts.list_type, &|nodes| {
            crate::render::render_inlines_inside_anchor(nodes, ctx.options())
        });
        let html = if self.opts.collapsible {
            format!(
                "<details class=\"{}\"{}>\n<summary>{}</summary>\n{}</details>",
                escape_html(&self.opts.css_class),
                if self.opts.open { " open" } else { "" },
                escape_html(&self.opts.summary),
                list,
            )
        } else {
            format!(
                "<nav class=\"{}\"{}>\n{}</nav>",
                escape_html(&self.opts.css_class),
                nav_label_attr(ctx.options().label(LABEL_TOC_NAV)),
                list,
            )
        };
        let toc = BlockNode::RawBlock(RawBlock {
            format: "html".into(),
            content: html,
            // Synthesized by an extension: no source span to report (PART 12 §4).
            pos: None,
        });
        match self.opts.position {
            Position::Top => doc.children.insert(0, toc),
            Position::Bottom => doc.children.push(toc),
        }
        doc
    }
}

/// Walk every heading in document order to keep the id counter aligned with the
/// renderer, but only record TOC entries for TOP-LEVEL headings (matching
/// carve-js, which iterates `doc.children` only).
fn collect_entries(
    blocks: &[BlockNode],
    counts: &mut BTreeMap<String, usize>,
    opts: &TableOfContentsOptions,
    entries: &mut Vec<TocEntry>,
    top_level: bool,
) {
    for block in blocks {
        match block {
            BlockNode::Heading(h) => {
                let id = next_id(
                    h,
                    counts,
                    HeadingIdOptions {
                        lowercase: opts.lowercase_ids,
                        ascii: AsciiHeadingIds::Off,
                    },
                );
                if top_level && h.level >= opts.min_level && h.level <= opts.max_level {
                    entries.push(TocEntry {
                        level: h.level,
                        label: crate::parse::derive_display_nodes(&h.children, true),
                        id,
                    });
                }
            }
            BlockNode::List(l) => {
                for item in &l.items {
                    collect_entries(&item.children, counts, opts, entries, false);
                }
            }
            BlockNode::BlockQuote(b) => collect_entries(&b.children, counts, opts, entries, false),
            BlockNode::Admonition(a) => collect_entries(&a.children, counts, opts, entries, false),
            BlockNode::Div(d) => collect_entries(&d.children, counts, opts, entries, false),
            BlockNode::Extension(e) => collect_entries(&e.children, counts, opts, entries, false),
            BlockNode::DefinitionList(dl) => {
                for item in &dl.items {
                    for def in &item.definitions {
                        collect_entries(def, counts, opts, entries, false);
                    }
                }
            }
            _ => {}
        }
    }
}

/// Like [`collect_entries`] but records EVERY heading (not only top-level ones),
/// recursing into every container. Used by the `::: toc` placement directive so
/// headings nested in `::: note`, blockquotes, lists, etc. appear in the TOC.
/// The id counter matches the renderer's document-order allocation.
fn collect_all_entries(
    blocks: &[BlockNode],
    counts: &mut BTreeMap<String, usize>,
    id_opts: HeadingIdOptions,
    entries: &mut Vec<TocEntry>,
) {
    for block in blocks {
        match block {
            BlockNode::Heading(h) => {
                let id = next_id(h, counts, id_opts);
                entries.push(TocEntry {
                    level: h.level,
                    label: crate::parse::derive_display_nodes(&h.children, true),
                    id,
                });
            }
            BlockNode::List(l) => {
                for item in &l.items {
                    collect_all_entries(&item.children, counts, id_opts, entries);
                }
            }
            BlockNode::BlockQuote(b) => collect_all_entries(&b.children, counts, id_opts, entries),
            BlockNode::Admonition(a) => collect_all_entries(&a.children, counts, id_opts, entries),
            BlockNode::Div(d) => collect_all_entries(&d.children, counts, id_opts, entries),
            BlockNode::Extension(e) => collect_all_entries(&e.children, counts, id_opts, entries),
            BlockNode::DefinitionList(dl) => {
                for item in &dl.items {
                    for def in &item.definitions {
                        collect_all_entries(def, counts, id_opts, entries);
                    }
                }
            }
            _ => {}
        }
    }
}

fn next_id(h: &Heading, counts: &mut BTreeMap<String, usize>, id_opts: HeadingIdOptions) -> String {
    let base = h
        .attrs
        .as_ref()
        .and_then(|a| a.id.clone())
        .unwrap_or_else(|| {
            crate::parse::slugify_parse(&crate::render::plain_inlines(&h.children), id_opts)
        });
    let count = counts.entry(base.clone()).or_insert(0);
    *count += 1;
    if *count == 1 {
        base
    } else {
        format!("{base}-{count}")
    }
}

/// Build a nested list from a flat, document-order entry list. Byte-faithful
/// port of carve-php's `TableOfContentsExtension::renderTocList` (and the
/// matching carve-js `buildList`): one tag per line, and a heading deeper than
/// its predecessor's predecessor stays a sibling `<li>` in the same nested
/// `<ul>` rather than opening a fresh one. Returns the `<ul>…</ul>` list with
/// its trailing newline.
fn build_list(
    entries: &[TocEntry],
    list_type: ListType,
    render_label: &dyn Fn(&[InlineNode]) -> String,
) -> String {
    let tag = list_type.tag();
    if entries.is_empty() {
        return String::new();
    }
    let mut html = format!("<{tag}>\n");
    let mut level_stack: Vec<u8> = vec![entries[0].level];
    let mut has_open_item = false;
    for e in entries {
        if has_open_item {
            let mut depth = level_stack.len();
            let current = level_stack[depth - 1];
            if e.level > current {
                html.push_str(&format!("\n<{tag}>\n"));
                level_stack.push(e.level);
            } else {
                while depth > 1 && e.level <= level_stack[depth - 2] {
                    html.push_str(&format!("</li>\n</{tag}>\n"));
                    level_stack.pop();
                    depth -= 1;
                }
                html.push_str("</li>\n");
                // Record this entry's (shallower) level so a later deeper
                // heading nests under IT, not the stale reused-list level (else
                // `# A/### B/## C/### D` flattens D as a sibling of C).
                level_stack[depth - 1] = e.level;
            }
        }
        // The label arrives as RENDERED HTML from the caller's own renderer, so
        // it is escaped exactly once, by the renderer, under the caller's
        // raw-HTML policy. `escape_html` here would double-escape every tag the
        // clone is there to keep.
        //
        // §26 bidi controls are stripped from the rendered bytes so a TOC link
        // cannot visually spoof its target, matching the core heading-text
        // policy. They are bare codepoints, never part of a tag, so stripping
        // after the render reaches every one of them and touches nothing else.
        html.push_str(&format!(
            "<li><a href=\"#{}\">{}</a>",
            escape_html(&e.id),
            strip_bidi(render_label(&e.label).trim())
        ));
        has_open_item = true;
    }
    html.push_str("</li>\n");
    let mut depth = level_stack.len();
    while depth > 1 {
        html.push_str(&format!("</{tag}>\n</li>\n"));
        level_stack.pop();
        depth -= 1;
    }
    html.push_str(&format!("</{tag}>\n"));
    html
}

/// Strip Trojan-Source bidi-override / isolate controls (§26) so a TOC link
/// cannot visually spoof its target, matching the core heading-text policy.
fn strip_bidi(s: &str) -> String {
    s.chars()
        .filter(|c| !matches!(*c, '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}'))
        .collect()
}

/// Escape `&`, `<`, `>`, `"` (matching the carve-js TOC `escapeHtml`). Note
/// this does NOT escape `'`, unlike the core attribute escaper.
fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            other => out.push(other),
        }
    }
    out
}

// ===========================================================================
// In-document `::: toc` placement directive
// ===========================================================================

/// In-document table-of-contents placement directive (Tier-3). Unlike
/// [`TableOfContents`] (which injects one TOC at the document top or bottom),
/// this renders a named `<nav class="toc">` exactly where the author writes a
/// `::: toc` block. Off by default.
pub struct TocPlacement {
    entries: RefCell<Vec<TocEntry>>,
    /// Remaining `<nav>` output budget for the current render; bounds K blocks
    /// x N headings amplification. Seeded in `before_render`.
    budget: RefCell<usize>,
}

impl TocPlacement {
    /// Create the placement extension.
    pub fn new() -> Self {
        Self {
            entries: RefCell::new(Vec::new()),
            budget: RefCell::new(0),
        }
    }
}

impl Default for TocPlacement {
    fn default() -> Self {
        Self::new()
    }
}

impl CarveExtension for TocPlacement {
    fn name(&self) -> &'static str {
        "toc"
    }

    fn before_render(&self, mut doc: Document, ctx: &BeforeRenderContext<'_>) -> Document {
        // Collect EVERY heading (the per-directive window is applied at render
        // time), recursing into containers so headings nested in `::: note`,
        // blockquotes, lists, etc. are included - they render with id anchors.
        // The id counter stays aligned with the renderer; footnote definitions
        // live outside `doc.children`, so their (id-less) headings are excluded.
        let id_opts = ctx.options().heading_id_options();
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        let mut entries: Vec<TocEntry> = Vec::new();
        collect_all_entries(&doc.children, &mut counts, id_opts, &mut entries);
        *self.entries.borrow_mut() = entries;
        *self.budget.borrow_mut() =
            (8usize.saturating_mul(doc.expansion_budget_len())).max(1_000_000);

        rewrite_toc_containers(&mut doc.children);
        doc
    }

    fn render_block_extension(
        &self,
        node: &BlockExtension,
        ctx: &RenderContext<'_>,
    ) -> Option<String> {
        if node.name != TOC_CARRIER {
            return None;
        }
        Some(render_toc_nav(
            node,
            ctx,
            &self.entries.borrow(),
            &self.budget,
        ))
    }
}

/// Rewrite every `::: toc` admonition into a [`TOC_CARRIER`] block extension so
/// `render_block_extension` renders it in place. Recurses into containers.
fn rewrite_toc_containers(blocks: &mut [BlockNode]) {
    for block in blocks.iter_mut() {
        match block {
            BlockNode::Admonition(a) if a.kind == "toc" => {
                rewrite_toc_containers(&mut a.children);
                *block = BlockNode::Extension(BlockExtension {
                    attrs: a.attrs.take(),
                    name: TOC_CARRIER.to_string(),
                    children: std::mem::take(&mut a.children),
                    summary: None,
                    label: a.label.take(),
                    pos: None,
                });
            }
            BlockNode::List(l) => {
                for item in &mut l.items {
                    rewrite_toc_containers(&mut item.children);
                }
            }
            BlockNode::BlockQuote(b) => rewrite_toc_containers(&mut b.children),
            BlockNode::Admonition(a) => rewrite_toc_containers(&mut a.children),
            BlockNode::Div(d) => rewrite_toc_containers(&mut d.children),
            BlockNode::Extension(e) => rewrite_toc_containers(&mut e.children),
            BlockNode::DefinitionList(dl) => {
                for item in &mut dl.items {
                    for def in &mut item.definitions {
                        rewrite_toc_containers(def);
                    }
                }
            }
            _ => {}
        }
    }
}

fn render_toc_nav(
    node: &BlockExtension,
    ctx: &RenderContext<'_>,
    entries: &[TocEntry],
    budget: &RefCell<usize>,
) -> String {
    let (min, max) = toc_window(&node.attrs);
    let attrs = render_attrs_without_keys(
        &Some(named_nav_attrs(&node.attrs, &ctx.label(LABEL_TOC_NAV))),
        &["depth", "from", "to"],
    );
    let empty_nav = format!("<nav{attrs}></nav>");
    // Preserve any authored blocks written inside the placeholder before the nav.
    let wrap = |nav: String| -> String {
        if node.children.is_empty() {
            nav
        } else {
            format!(
                "{}\n{}",
                ctx.render_blocks_at(&node.children, ctx.level()),
                nav
            )
        }
    };

    let picked: Vec<TocEntry> = entries
        .iter()
        .filter(|e| e.level >= min && e.level <= max)
        .cloned()
        .collect();
    if picked.is_empty() {
        return wrap(empty_nav);
    }
    // Rendered through the RENDER IN PROGRESS, so an entry obeys the same
    // raw-HTML policy, symbols map and typography mode as the heading it was
    // derived from - the injected `tableOfContents()` nav uses the same options
    // by the same argument, one hook earlier.
    let nav = format!(
        "<nav{attrs}>\n{}</nav>",
        build_list(&picked, ListType::Ul, &|nodes| ctx
            .render_inlines_inside_anchor(nodes))
    );
    // Bound cumulative nav bytes across all `::: toc` blocks in one render: K
    // blocks x N headings would otherwise amplify output ~K*N. Once the budget
    // is exhausted, degrade to an empty nav. The borrow is scoped and released
    // BEFORE `wrap`, which may render a nested `::: toc` that re-borrows the
    // budget (holding it across `wrap` panics with "RefCell already borrowed").
    let within_budget = {
        let mut remaining = budget.borrow_mut();
        if nav.len() > *remaining {
            false
        } else {
            *remaining -= nav.len();
            true
        }
    };
    if within_budget {
        wrap(nav)
    } else {
        wrap(empty_nav)
    }
}

/// Resolve the heading-level window from a `::: toc` directive's attributes.
/// `{from=X to=Y}` is an explicit range (swapped if inverted); `{depth=N}` is
/// shorthand for levels 1..N. `from`/`to` win over `depth` when both appear.
fn toc_window(attrs: &Option<Attrs>) -> (u8, u8) {
    let get = |k: &str| {
        attrs
            .as_ref()
            .and_then(|a| a.key_values.get(k))
            .map(String::as_str)
    };
    let level = |value: Option<&str>, fallback: u8| -> u8 {
        value
            .and_then(|s| s.trim().parse::<i64>().ok())
            .map(|n| n.clamp(1, 6) as u8)
            .unwrap_or(fallback)
    };
    if get("from").is_some() || get("to").is_some() {
        let mut min = level(get("from"), 1);
        let mut max = level(get("to"), 6);
        if min > max {
            std::mem::swap(&mut min, &mut max);
        }
        (min, max)
    } else {
        (1, level(get("depth"), 6))
    }
}

/// The ` aria-label="..."` run for a nav this extension builds from scratch, or
/// nothing when the host set the key to the empty string to suppress the name.
fn nav_label_attr(label: &str) -> String {
    if label.is_empty() {
        String::new()
    } else {
        format!(" aria-label=\"{}\"", escape_attr(label))
    }
}

/// The `::: toc` nav's attributes: the `toc` base class, the author's own
/// `{#id .class}`, and the landmark's accessible name APPENDED after whatever
/// the author wrote.
///
/// A name the AUTHOR wrote outranks the label and nothing is added beside it -
/// Extensions §1.5's existing precedence, since §8b.1 already carries the
/// attribute line onto the nav. The match is on the attribute NAME,
/// ASCII-case-insensitively as §16a rules for the shapes carve#1468 closed,
/// because this engine echoes an authored `ARIA-LABEL` back in the author's own
/// spelling and a case-sensitive test would write a second name beside theirs.
fn named_nav_attrs(attrs: &Option<Attrs>, label: &str) -> Attrs {
    let mut a = with_base_class(attrs, "toc");
    let authored = |name: &str| a.key_values.keys().any(|k| k.eq_ignore_ascii_case(name));
    if label.is_empty() || authored("aria-label") || authored("aria-labelledby") {
        return a;
    }
    a.key_values
        .insert("aria-label".to_string(), label.to_string());
    crate::extension::record_attr_order(&mut a, "aria-label");
    a
}

/// Prepend `base` as the leading class of (a clone of) `attrs`.
fn with_base_class(attrs: &Option<Attrs>, base: &str) -> Attrs {
    let mut a = attrs.clone().unwrap_or_default();
    // Drop any author-supplied copy so `{.toc}` never doubles the base class.
    a.classes.retain(|c| c != base);
    a.classes.insert(0, base.to_string());
    a
}
