//! HeadingNumbers (#198, Tier-3). Auto-number sections and rewrite auto-filled
//! `</#id>` cross-references to "Section 1.2 - Title". Render-stage, opt-in, no
//! new syntax (reads headings + the `{.unnumbered}` class). Off by default,
//! never corpus-pinned. See docs/extensions.md §9.
//!
//! Port of the carve-js `heading-numbers.ts`, byte-identical in HTML output. A
//! pure `before_render` transform: it prepends a `<span class="section-number">`
//! to each numbered `<h*>` and rewrites the children of `</#id>`-origin links
//! (identified by the non-rendered [`crate::ast::Link::from_crossref`] flag set
//! during crossref resolution).

use std::collections::BTreeMap;

use crate::ast::{Attrs, BlockNode, Document, FigureTarget, Heading, InlineNode, Link, Span, Text};
use crate::extension::{BeforeRenderContext, CarveExtension};
use crate::parse::slugify_parse;
use crate::render::plain_inlines;

/// In-text cross-reference rendering for an auto-filled `</#id>` reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrossrefStyle {
    /// `Section 1.2`
    Number,
    /// `Section 1.2 - Title` (default).
    NumberTitle,
    /// Leave cross-references untouched; number only the headings.
    Title,
}

/// Options for [`HeadingNumbers`].
#[derive(Debug, Clone)]
pub struct HeadingNumbersOptions {
    /// Top numbered heading level (1-6). Default 1; set 2 when `#` is the title.
    pub min_level: u8,
    /// Cross-reference prefix word. Default `"Section"`.
    pub label: String,
    /// Auto-filled cross-reference text. Default [`CrossrefStyle::NumberTitle`].
    pub crossref: CrossrefStyle,
}

impl Default for HeadingNumbersOptions {
    fn default() -> Self {
        Self {
            min_level: 1,
            label: "Section".to_string(),
            crossref: CrossrefStyle::NumberTitle,
        }
    }
}

/// Auto-number sections and render numbered cross-references.
///
/// ```
/// use carve::{HeadingNumbers, Options};
/// let ext = HeadingNumbers::new();
/// let opts = Options::new().with_extension(&ext);
/// let html = carve::to_html_with_options("# Parsing\n\nSee </#Parsing>.", &opts);
/// assert!(html.contains("<span class=\"section-number\">1</span> Parsing"));
/// assert!(html.contains("<a href=\"#Parsing\">Section 1 - Parsing</a>"));
/// ```
#[derive(Debug)]
pub struct HeadingNumbers {
    opts: HeadingNumbersOptions,
}

impl Default for HeadingNumbers {
    fn default() -> Self {
        Self::new()
    }
}

impl HeadingNumbers {
    /// Create with default options (number from level 1, `Section`, number-title).
    pub fn new() -> Self {
        Self {
            opts: HeadingNumbersOptions::default(),
        }
    }

    /// Create with explicit options.
    pub fn with_options(opts: HeadingNumbersOptions) -> Self {
        Self { opts }
    }
}

struct Entry {
    number: String,
    title: String,
}

impl CarveExtension for HeadingNumbers {
    fn name(&self) -> &'static str {
        "headingNumbers"
    }

    fn before_render(&self, mut doc: Document, _ctx: &BeforeRenderContext<'_>) -> Document {
        // No idempotency guard is needed: `before_render` takes the Document by
        // value and returns it, so the pipeline owns it and runs this exactly
        // once per render - there is no parse-once / render-twice reuse of the
        // same instance (unlike carve-js, which passes the doc by reference and
        // needs a WeakSet). A content-based "already numbered" check would also
        // be unsafe: an author span `[x]{.section-number}` is valid source and
        // must not disable the whole pass.
        // Pass 1: number headings (gap-free stack), decorate each `<h*>` with a
        // section-number span, and remember number + original title per id.
        let mut state = NumberState {
            min_level: self.opts.min_level,
            lowercase: _ctx.options().lowercase_heading_ids,
            levels: Vec::new(),
            numbers: Vec::new(),
            heading_counts: BTreeMap::new(),
            by_id: BTreeMap::new(),
        };
        number_blocks(&mut doc.children, false, &mut state);

        // Pass 2: rewrite auto-filled cross-references. Only links tagged
        // `from_crossref` are touched, so ordinary `[text](#id)` links and
        // implicit `[label][]` references keep their text. Walk the body and
        // footnote definitions (both rendered).
        let rewrite = |blocks: &mut Vec<BlockNode>| {
            rewrite_links_blocks(blocks, &state.by_id, &self.opts);
        };
        rewrite(&mut doc.children);
        for blocks in doc.footnote_defs.values_mut() {
            rewrite(blocks);
        }

        doc
    }
}

struct NumberState {
    min_level: u8,
    lowercase: bool,
    levels: Vec<u8>,
    numbers: Vec<u32>,
    /// Per-base id dedup counter, mirroring the renderer's `next_heading_id`
    /// so the computed id matches the rendered `<section id>` / `</#id>` href.
    heading_counts: BTreeMap<String, u32>,
    by_id: BTreeMap<String, Entry>,
}

fn has_class(h: &Heading, cls: &str) -> bool {
    h.attrs
        .as_ref()
        .is_some_and(|a| a.classes.iter().any(|c| c == cls))
}

fn number_blocks(blocks: &mut [BlockNode], in_blockquote: bool, state: &mut NumberState) {
    for block in blocks.iter_mut() {
        match block {
            BlockNode::Heading(h) => number_heading(h, in_blockquote, state),
            BlockNode::BlockQuote(b) => number_blocks(&mut b.children, true, state),
            BlockNode::Div(d) => number_blocks(&mut d.children, in_blockquote, state),
            BlockNode::Admonition(a) => number_blocks(&mut a.children, in_blockquote, state),
            BlockNode::List(l) => {
                for item in &mut l.items {
                    number_blocks(&mut item.children, in_blockquote, state);
                }
            }
            BlockNode::DefinitionList(dl) => {
                for item in &mut dl.items {
                    for def in &mut item.definitions {
                        number_blocks(def, in_blockquote, state);
                    }
                }
            }
            BlockNode::Figure(f) => {
                // Only a blockquote target can hold a heading; the resolver
                // assigns its heading an id (as a quoted heading), so mirror
                // that descent for first-id-wins.
                if let FigureTarget::BlockQuote(b) = &mut f.target {
                    number_blocks(&mut b.children, true, state);
                }
            }
            // An extension carrier (Details/Spoiler/Glossary/… registered
            // before this one) wraps rendered block content; descend so its
            // headings are numbered, matching how carve-js descends the
            // admonition those map to.
            BlockNode::Extension(e) => number_blocks(&mut e.children, in_blockquote, state),
            _ => {}
        }
    }
}

fn number_heading(h: &mut Heading, in_blockquote: bool, state: &mut NumberState) {
    // Compute this heading's FINAL id exactly as the renderer's `next_heading_id`
    // does (explicit id or slug, then dedup), advancing the shared counter for
    // EVERY heading in document order so the ids stay in lock-step with the
    // rendered `<section id>` / resolved `</#id>` hrefs. Deduped ids are unique,
    // so first-id-wins falls out for free (no later heading reuses a key).
    let base = h
        .attrs
        .as_ref()
        .and_then(|a| a.id.clone())
        .unwrap_or_else(|| slugify_parse(&plain_inlines(&h.children), state.lowercase));
    let count = state.heading_counts.entry(base.clone()).or_insert(0);
    *count += 1;
    let id = if *count == 1 {
        base
    } else {
        format!("{base}-{count}")
    };

    if in_blockquote || has_class(h, "unnumbered") || h.level < state.min_level {
        return;
    }

    let lvl = h.level;
    while state.levels.last().is_some_and(|&top| top > lvl) {
        state.levels.pop();
        state.numbers.pop();
    }
    if state.levels.last() == Some(&lvl) {
        *state.numbers.last_mut().unwrap() += 1;
    } else {
        state.levels.push(lvl);
        state.numbers.push(1);
    }
    let number = state
        .numbers
        .iter()
        .map(|n| n.to_string())
        .collect::<Vec<_>>()
        .join(".");

    let title = plain_inlines(&h.children); // capture BEFORE injecting the span
    state.by_id.insert(
        id,
        Entry {
            number: number.clone(),
            title,
        },
    );

    let span = InlineNode::Span(Span {
        attrs: Some(Attrs {
            classes: vec!["section-number".to_string()],
            ..Attrs::default()
        }),
        children: vec![InlineNode::text(number)],
        pos: None,
    });
    let mut new_children = Vec::with_capacity(h.children.len() + 2);
    new_children.push(span);
    new_children.push(InlineNode::text(" ".to_string()));
    new_children.append(&mut h.children);
    h.children = new_children;
}

fn rewrite_links_blocks(
    blocks: &mut [BlockNode],
    by_id: &BTreeMap<String, Entry>,
    opts: &HeadingNumbersOptions,
) {
    for block in blocks.iter_mut() {
        match block {
            BlockNode::Heading(h) => rewrite_links_inlines(&mut h.children, by_id, opts, false),
            BlockNode::Paragraph(p) => rewrite_links_inlines(&mut p.children, by_id, opts, false),
            BlockNode::BlockQuote(b) => {
                rewrite_links_blocks(&mut b.children, by_id, opts);
                if let Some(attr) = &mut b.attribution {
                    rewrite_links_inlines(attr, by_id, opts, false);
                }
            }
            BlockNode::Div(d) => rewrite_links_blocks(&mut d.children, by_id, opts),
            BlockNode::Admonition(a) => {
                if let Some(t) = &mut a.title {
                    rewrite_links_inlines(t, by_id, opts, false);
                }
                rewrite_links_blocks(&mut a.children, by_id, opts);
            }
            BlockNode::List(l) => {
                for item in &mut l.items {
                    rewrite_links_blocks(&mut item.children, by_id, opts);
                }
            }
            BlockNode::DefinitionList(dl) => {
                for item in &mut dl.items {
                    for term in &mut item.terms {
                        rewrite_links_inlines(term, by_id, opts, false);
                    }
                    for def in &mut item.definitions {
                        rewrite_links_blocks(def, by_id, opts);
                    }
                }
            }
            BlockNode::Table(t) => {
                if let Some(caption) = &mut t.caption {
                    rewrite_links_inlines(caption, by_id, opts, false);
                }
                for row in &mut t.rows {
                    for cell in &mut row.cells {
                        rewrite_links_inlines(&mut cell.children, by_id, opts, false);
                    }
                }
            }
            BlockNode::Figure(f) => {
                rewrite_links_inlines(&mut f.caption, by_id, opts, false);
                if let FigureTarget::BlockQuote(b) = &mut f.target {
                    rewrite_links_blocks(&mut b.children, by_id, opts);
                }
            }
            BlockNode::Extension(e) => rewrite_links_blocks(&mut e.children, by_id, opts),
            _ => {}
        }
    }
}

fn rewrite_links_inlines(
    nodes: &mut [InlineNode],
    by_id: &BTreeMap<String, Entry>,
    opts: &HeadingNumbersOptions,
    inside_link: bool,
) {
    for node in nodes.iter_mut() {
        match node {
            InlineNode::Link(l) => {
                rewrite_links_inlines(&mut l.children, by_id, opts, true);
                maybe_rewrite_link(l, by_id, opts);
            }
            InlineNode::CrossRef(c) => {
                if let Some((id, entry)) = resolve_numbered_crossref(by_id, &c.target) {
                    let text = numbered_crossref_text(entry, opts);
                    if inside_link {
                        *node = InlineNode::Text(Text {
                            value: text,
                            pos: c.pos,
                        });
                    } else {
                        *node = InlineNode::Link(Link {
                            attrs: None,
                            href: format!("#{id}"),
                            title: None,
                            children: vec![InlineNode::text(text)],
                            ref_label: None,
                            raw_ref: None,
                            from_crossref: true,
                            from_heading_reference: false,
                            pos: c.pos,
                        });
                    }
                }
            }
            InlineNode::Emphasis(e) => {
                rewrite_links_inlines(&mut e.children, by_id, opts, inside_link)
            }
            InlineNode::Span(s) => rewrite_links_inlines(&mut s.children, by_id, opts, inside_link),
            InlineNode::Extension(e) => {
                rewrite_links_inlines(&mut e.children, by_id, opts, inside_link)
            }
            InlineNode::CriticInsert(c) => {
                rewrite_links_inlines(&mut c.children, by_id, opts, inside_link)
            }
            InlineNode::CriticDelete(c) => {
                rewrite_links_inlines(&mut c.children, by_id, opts, inside_link)
            }
            InlineNode::Footnote(f) => {
                if let Some(inl) = &mut f.inline {
                    rewrite_links_inlines(inl, by_id, opts, false);
                }
            }
            InlineNode::CitationGroup(g) => {
                // Citation prefixes/locators are rendered inline and the parser
                // resolves `</#id>` inside them; rewrite there too (carve-js's
                // generic link walk reaches these).
                for item in &mut g.items {
                    if let Some(p) = &mut item.prefix {
                        rewrite_links_inlines(p, by_id, opts, inside_link);
                    }
                    if let Some(loc) = &mut item.locator {
                        rewrite_links_inlines(loc, by_id, opts, inside_link);
                    }
                }
            }
            _ => {}
        }
    }
}

fn maybe_rewrite_link(l: &mut Link, by_id: &BTreeMap<String, Entry>, opts: &HeadingNumbersOptions) {
    if opts.crossref == CrossrefStyle::Title {
        return;
    }
    if !l.from_crossref {
        return;
    }
    let Some(id) = l.href.strip_prefix('#') else {
        return;
    };
    let Some(entry) = by_id.get(id) else {
        return; // crossref to an unnumbered heading: leave the title
    };
    let text = numbered_crossref_text(entry, opts);
    l.children = vec![InlineNode::text(text)];
}

fn resolve_numbered_crossref<'a>(
    by_id: &'a BTreeMap<String, Entry>,
    target: &str,
) -> Option<(String, &'a Entry)> {
    if let Some(entry) = by_id.get(target) {
        return Some((target.to_string(), entry));
    }
    let folded_target = case_fold(target);
    by_id
        .iter()
        .find(|(id, _)| case_fold(id) == folded_target)
        .map(|(id, entry)| (id.clone(), entry))
}

fn numbered_crossref_text(entry: &Entry, opts: &HeadingNumbersOptions) -> String {
    match opts.crossref {
        CrossrefStyle::Number => format!("{} {}", opts.label, entry.number),
        CrossrefStyle::NumberTitle => format!("{} {} - {}", opts.label, entry.number, entry.title),
        CrossrefStyle::Title => entry.title.clone(),
    }
}

fn case_fold(s: &str) -> String {
    let mut out = String::new();
    for ch in s.chars() {
        for lc in ch.to_lowercase() {
            out.push(lc);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extension::Mode;
    use crate::parse::parse;
    use crate::render::render_html_with_options;
    use crate::Options;

    #[test]
    fn author_section_number_span_does_not_disable_numbering() {
        // An author span `[v1]{.section-number}` is valid source and must NOT be
        // mistaken for a processed marker that turns the whole pass into a no-op.
        let ext = HeadingNumbers::new();
        let opts = Options::default();
        let ctx = BeforeRenderContext::new(&opts, Mode::Interactive, true);
        let doc = parse("# [v1]{.section-number} API\n\n## Next\n\nSee </#Next>.\n");
        let out = render_html_with_options(&ext.before_render(doc, &ctx), &opts);
        // Numbering still happened: the second heading is numbered and its
        // cross-reference is rewritten.
        assert!(out.contains("<span class=\"section-number\">1.1</span> Next"));
        assert!(out.contains("Section 1.1 - Next"));
    }
}
