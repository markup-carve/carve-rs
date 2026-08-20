//! Append (or prepend) a clickable permalink anchor to each heading.
//!
//! Port of carve-js `heading-permalinks.ts` / carve-php's
//! `HeadingPermalinksExtension`. carve-js implements this through a heading
//! block renderer; carve-rs has no per-node block render hook, so this runs as
//! a `before_render` transform that appends the anchor as an inline child of
//! each heading. The `<section id>` wrapper stays core; the `<h*>` gains the
//! anchor.
//!
//! Heading id resolution: carve-rs computes heading ids at render time
//! (case-preserving by default; see [`crate::Options::lowercase_heading_ids`]).
//! This transform reproduces the same document-order numbered-slug logic. To
//! match a rendered document that used `with_lowercase_heading_ids(true)`,
//! construct the extension with [`HeadingPermalinksOptions::lowercase_ids`] set
//! to the same value.

use std::collections::BTreeMap;

use crate::ast::{BlockNode, Document, Heading, InlineNode, RawInline};
use crate::extension::{AsciiHeadingIds, BeforeRenderContext, CarveExtension, HeadingIdOptions};

/// Options for [`HeadingPermalinks`].
#[derive(Debug, Clone)]
pub struct HeadingPermalinksOptions {
    /// Anchor glyph. Default `"¶"`.
    pub symbol: String,
    /// CSS class on the anchor. Default `"permalink"`.
    pub css_class: String,
    /// `aria-label` on the anchor. Default `"Permalink"`.
    pub aria_label: String,
    /// Heading levels (1-6) to add a permalink to. Default all.
    pub levels: Vec<u8>,
    /// Place the anchor before the heading text instead of after. Default false.
    pub prepend: bool,
    /// Only reveal the anchor on heading hover: wrap it in a
    /// `<span class="permalink-wrapper permalink-hover">` the host stylesheet
    /// targets via `h*:hover > .permalink-hover`. Default false (bare anchor).
    pub show_on_hover: bool,
    /// Add a `data-permalink-copy` hook the host JS can use to copy the URL.
    /// Default false.
    pub copy_to_clipboard: bool,
    /// Lowercase auto-generated heading ids when computing the link target.
    /// Must match the renderer's [`crate::Options::lowercase_heading_ids`].
    /// Default false (case-preserving), matching the carve-rs default.
    pub lowercase_ids: bool,
}

impl Default for HeadingPermalinksOptions {
    fn default() -> Self {
        Self {
            symbol: "¶".into(),
            css_class: "permalink".into(),
            aria_label: "Permalink".into(),
            levels: vec![1, 2, 3, 4, 5, 6],
            prepend: false,
            show_on_hover: false,
            copy_to_clipboard: false,
            lowercase_ids: false,
        }
    }
}

/// Append (or prepend) a clickable permalink anchor to each heading.
///
/// ```
/// use carve::{HeadingPermalinks, HeadingPermalinksOptions, Options};
/// let opts = HeadingPermalinksOptions { lowercase_ids: true, ..Default::default() };
/// let ext = HeadingPermalinks::with_options(opts);
/// let options = Options::new()
///     .with_extension(&ext)
///     .with_lowercase_heading_ids(true);
/// let html = carve::to_html_with_options("# My Heading", &options);
/// assert!(html.contains(
///     "<h1>My Heading <a href=\"#my-heading\" class=\"permalink\" aria-label=\"Permalink\">¶</a></h1>"
/// ));
/// ```
pub struct HeadingPermalinks {
    opts: HeadingPermalinksOptions,
}

impl HeadingPermalinks {
    /// Create a heading-permalinks extension with default options.
    pub fn new() -> Self {
        Self::with_options(HeadingPermalinksOptions::default())
    }

    /// Create a heading-permalinks extension with explicit options.
    pub fn with_options(opts: HeadingPermalinksOptions) -> Self {
        Self { opts }
    }
}

impl Default for HeadingPermalinks {
    fn default() -> Self {
        Self::new()
    }
}

impl CarveExtension for HeadingPermalinks {
    fn name(&self) -> &'static str {
        "heading-permalinks"
    }

    fn before_render(&self, mut doc: Document, _ctx: &BeforeRenderContext<'_>) -> Document {
        // Reproduce the renderer's document-order id counter so the anchor
        // href matches the `<section id>` / `<h* id>` the core emits.
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        walk_blocks(&mut doc.children, &mut |h| {
            let id = next_id(
                h,
                &mut counts,
                HeadingIdOptions {
                    lowercase: self.opts.lowercase_ids,
                    ascii: AsciiHeadingIds::Off,
                },
            );
            if !self.opts.levels.contains(&h.level) {
                return;
            }
            self.append_anchor(h, &id);
        });
        doc
    }
}

impl HeadingPermalinks {
    fn append_anchor(&self, h: &mut Heading, id: &str) {
        let copy_attr = if self.opts.copy_to_clipboard {
            " data-permalink-copy=\"\""
        } else {
            ""
        };
        let anchor_html = format!(
            "<a href=\"#{}\" class=\"{}\" aria-label=\"{}\"{}>{}</a>",
            crate::escape::escape_attr(id),
            crate::escape::escape_attr(&self.opts.css_class),
            crate::escape::escape_attr(&self.opts.aria_label),
            copy_attr,
            crate::escape::escape_text(&self.opts.symbol),
        );
        // showOnHover wraps the anchor so the hover CSS (`h*:hover >
        // .permalink-hover`) has a child to target; default is the bare anchor.
        let marker_html = if self.opts.show_on_hover {
            format!("<span class=\"permalink-wrapper permalink-hover\">{anchor_html}</span>")
        } else {
            anchor_html
        };
        // Emit the marker as raw inline HTML so the core inline renderer passes
        // it through verbatim, with a literal space separating it from the text.
        //
        // ONE node, separator included, and MARKED as injected. Both halves are
        // PART 9R R4's THE LABEL IS TAKEN BEFORE ANY RENDER-STAGE INJECTION: a
        // permalink anchor is not part of a heading's derived display text, and
        // this engine derives that text at render time - after this hook - so
        // the anchor has to be recognizable there. A separate `text(" ")` node
        // could not be marked and would survive the strip as a stray space
        // inside every derived label. The emitted bytes are unchanged either
        // way: the space sits at the same place in the same run.
        let content = if self.opts.prepend {
            format!("{marker_html} ")
        } else {
            format!(" {marker_html}")
        };
        let anchor = InlineNode::RawInline(RawInline {
            format: "html".into(),
            content,
            injected: true,
            pos: None,
        });
        if self.opts.prepend {
            h.children.insert(0, anchor);
        } else {
            h.children.push(anchor);
        }
    }
}

/// Walk every heading in the document in render order, including headings
/// nested inside container blocks (the renderer allocates ids for those too).
fn walk_blocks(blocks: &mut [BlockNode], f: &mut impl FnMut(&mut Heading)) {
    for block in blocks {
        match block {
            BlockNode::Heading(h) => f(h),
            BlockNode::List(l) => {
                for item in &mut l.items {
                    walk_blocks(&mut item.children, f);
                }
            }
            BlockNode::BlockQuote(b) => walk_blocks(&mut b.children, f),
            BlockNode::Admonition(a) => walk_blocks(&mut a.children, f),
            BlockNode::Div(d) => walk_blocks(&mut d.children, f),
            BlockNode::Extension(e) => walk_blocks(&mut e.children, f),
            BlockNode::DefinitionList(dl) => {
                for item in &mut dl.items {
                    for def in &mut item.definitions {
                        walk_blocks(def, f);
                    }
                }
            }
            _ => {}
        }
    }
}

/// Mirror the renderer's `next_heading_id`: author id if present, else a slug
/// of the plain text, numbered per document-order duplicate.
///
/// The plain-text projection is taken from the core renderer's
/// [`crate::render::plain_inlines`] (not a private copy), so the anchor `href`
/// this extension computes is byte-identical to the `<section id>` / `<h* id>`
/// the core emits for the same heading - for every inline node type, including
/// citations. See the regression tests for the invariant `href == id`.
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
