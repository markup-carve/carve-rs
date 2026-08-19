//! Per-render document id namespace (extensions contract §2.6).
//!
//! Extension-generated DOM ids — here the citation `cite-{key}-{n}` back-link
//! anchors and `ref-{key}` reference entries — must join the same id namespace
//! as explicit `{#id}` attributes and generated heading ids, deduplicated with
//! the heading mechanism: the first use of a name keeps it, every later
//! collision takes the next free 1-based numeric suffix (`base-2`, `-3`, ...),
//! skipping candidates that are already reserved. Ids without collisions stay
//! byte-identical to before. Mirrors carve-php's
//! `HeadingIdTracker::uniqueId()` and carve-js's `DocumentIdRegistry`.
//!
//! The registry lives in a thread-local installed for the duration of one HTML
//! render by [`DocumentIdsGuard`] (RAII, the same save/restore discipline as
//! `AbbrBudgetGuard`). It is seeded up front from the resolved document —
//! every explicit `{#id}` attribute plus every heading id the renderer will
//! assign — so ids reserved during the render can never collide with them.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

use crate::ast::*;

#[derive(Default)]
pub(crate) struct DocumentIdRegistry {
    /// id -> next 1-based suffix candidate for that base (mirrors carve-php's
    /// `HeadingIdTracker::$usedIds` / carve-js's `DocumentIdRegistry.usedIds`).
    used_ids: BTreeMap<String, usize>,
    /// `(key, use-site count)` in first-citation order, pending reservation.
    pending_citations: Vec<(String, usize)>,
    citations_reserved: bool,
    /// `{key}-{n}` -> deduplicated `cite-{key}-{n}` use-site anchor id.
    cite_ids: BTreeMap<String, String>,
    /// key -> deduplicated `ref-{key}` references-list entry id.
    ref_ids: BTreeMap<String, String>,
    /// Every EXPLICIT `{#id}` in the document (on any node). A heading's auto
    /// slug must skip these so it never collides with an explicit id elsewhere
    /// (two elements sharing a DOM id is invalid HTML). Reserved up front, so
    /// the seeder's simulated heading ids and the renderer's assigned ones agree.
    explicit_ids: BTreeSet<String>,
}

impl DocumentIdRegistry {
    /// Reserve an id verbatim (an explicit attribute or a heading id the
    /// renderer will assign). First reservation wins; repeats are no-ops.
    fn reserve(&mut self, id: &str) {
        if !id.is_empty() && !self.used_ids.contains_key(id) {
            self.used_ids.insert(id.to_string(), 1);
        }
    }

    /// Reserve `base_id` in the namespace, or the next free numeric suffix
    /// (`base_id-2`, `-3`, ...) when taken — skipping candidates already
    /// reserved by explicit attributes or previously generated ids. The
    /// per-base counter is remembered, so repeated calls for the same base
    /// continue from the last suffix.
    fn unique_id(&mut self, base_id: &str) -> String {
        let Some(&count) = self.used_ids.get(base_id) else {
            self.used_ids.insert(base_id.to_string(), 1);
            return base_id.to_string();
        };
        let mut n = count;
        let candidate = loop {
            n += 1;
            let candidate = format!("{base_id}-{n}");
            if !self.used_ids.contains_key(&candidate) {
                break candidate;
            }
        };
        self.used_ids.insert(base_id.to_string(), n);
        self.used_ids.insert(candidate.clone(), 1);
        candidate
    }

    /// Reserve the citation ids on first lookup — matching carve-php /
    /// carve-js, which reserve when the first citation group or references
    /// list renders: `cite-{key}-{n}` per use site in document order, then
    /// `ref-{key}` per key in first-citation order. Anchors, hrefs,
    /// back-links, and `<li id>`s all read from the resulting maps, so they
    /// stay consistent regardless of render order.
    fn ensure_citation_ids(&mut self) {
        if self.citations_reserved {
            return;
        }
        self.citations_reserved = true;
        let pending = std::mem::take(&mut self.pending_citations);
        for (key, count) in &pending {
            for n in 1..=*count {
                let id = self.unique_id(&format!("cite-{key}-{n}"));
                self.cite_ids.insert(format!("{key}-{n}"), id);
            }
        }
        for (key, _) in &pending {
            let id = self.unique_id(&format!("ref-{key}"));
            self.ref_ids.insert(key.clone(), id);
        }
    }
}

thread_local! {
    /// The registry for the render currently running on this thread. `None`
    /// means no render is active (lookups then fall back to the base ids).
    static ACTIVE: RefCell<Option<DocumentIdRegistry>> = const { RefCell::new(None) };
}

/// RAII guard that installs the document id registry for one render and
/// restores the previous one on drop, so nested renders (a block extension
/// rendering a sub-document) correctly stack and unwind.
pub(crate) struct DocumentIdsGuard {
    previous: Option<DocumentIdRegistry>,
}

impl DocumentIdsGuard {
    /// Seed a registry from `doc` and install it for the current render.
    pub(crate) fn new(doc: &Document, lowercase_heading_ids: bool) -> Self {
        let registry = seed_registry(doc, lowercase_heading_ids);
        let previous = ACTIVE.with(|cell| cell.borrow_mut().replace(registry));
        DocumentIdsGuard { previous }
    }
}

impl Drop for DocumentIdsGuard {
    fn drop(&mut self) {
        ACTIVE.with(|cell| *cell.borrow_mut() = self.previous.take());
    }
}

/// Reserve a unique id in the active render's document id namespace: returns
/// `base_id` when free, else the next free numeric suffix. Outside an active
/// HTML render (no guard installed) the base id is returned unchanged.
pub(crate) fn unique_id(base_id: &str) -> String {
    ACTIVE.with(|cell| match cell.borrow_mut().as_mut() {
        Some(registry) => registry.unique_id(base_id),
        None => base_id.to_string(),
    })
}

/// True when `id` is an explicit `{#id}` in the document. A heading's auto slug
/// skips these so it never emits a duplicate DOM id. Outside an active render
/// (no guard) nothing is reserved, so this is false.
pub(crate) fn is_explicit_id(id: &str) -> bool {
    ACTIVE.with(|cell| {
        cell.borrow()
            .as_ref()
            .is_some_and(|registry| registry.explicit_ids.contains(id))
    })
}

/// The deduplicated id of the `n`-th use-site anchor of citation `key`
/// (base form `cite-{key}-{n}`).
pub(crate) fn cite_id(key: &str, n: usize) -> String {
    ACTIVE
        .with(|cell| {
            cell.borrow_mut().as_mut().and_then(|registry| {
                registry.ensure_citation_ids();
                registry.cite_ids.get(&format!("{key}-{n}")).cloned()
            })
        })
        .unwrap_or_else(|| format!("cite-{key}-{n}"))
}

/// The deduplicated id of citation `key`'s references-list entry
/// (base form `ref-{key}`).
pub(crate) fn ref_id(key: &str) -> String {
    ACTIVE
        .with(|cell| {
            cell.borrow_mut().as_mut().and_then(|registry| {
                registry.ensure_citation_ids();
                registry.ref_ids.get(key).cloned()
            })
        })
        .unwrap_or_else(|| format!("ref-{key}"))
}

/// Walk the resolved document and seed a registry with every id already
/// claimed: explicit `{#id}` attributes anywhere, plus the heading ids the
/// renderer will assign (simulated with the same base + document-order counter
/// as `render::next_heading_id`, since carve-rs resolves heading ids at render
/// time rather than storing them in the AST). Citation use sites are collected
/// along the way so their ids can be reserved lazily on first lookup.
/// The id the renderer will assign to each heading, in document order.
///
/// Runs the same two passes `seed_registry` does - explicit ids first, then
/// numbering - so the answer cannot differ from the one the HTML render uses.
/// The AST encoder publishes these as `attrs.id` where the source wrote none
/// (PART 12 §5: a generated heading id is a resolution result, because dedup
/// makes it a function of the whole document rather than of the heading).
pub(crate) fn assigned_heading_ids(doc: &Document, lowercase_heading_ids: bool) -> Vec<String> {
    let mut seeder = Seeder {
        registry: DocumentIdRegistry::default(),
        heading_counts: BTreeMap::new(),
        citation_index: BTreeMap::new(),
        lowercase_heading_ids,
        assigned: Vec::new(),
        collect_explicit_only: true,
    };
    seeder.walk_blocks(&doc.children);
    for blocks in doc.footnote_defs.values() {
        seeder.walk_blocks(blocks);
    }
    seeder.collect_explicit_only = false;
    seeder.walk_blocks(&doc.children);
    for blocks in doc.footnote_defs.values() {
        seeder.walk_blocks(blocks);
    }

    seeder.assigned
}

fn seed_registry(doc: &Document, lowercase_heading_ids: bool) -> DocumentIdRegistry {
    let mut seeder = Seeder {
        registry: DocumentIdRegistry::default(),
        heading_counts: BTreeMap::new(),
        citation_index: BTreeMap::new(),
        lowercase_heading_ids,
        assigned: Vec::new(),
        collect_explicit_only: true,
    };
    // Pass A: reserve every explicit id across the whole document (body then
    // footnote defs), so heading auto-slugs in pass B can skip them regardless
    // of document order.
    seeder.walk_blocks(&doc.children);
    for blocks in doc.footnote_defs.values() {
        seeder.walk_blocks(blocks);
    }
    // Pass B: number headings (skipping the explicit ids) and reserve citation
    // ids. Footnote definitions render after the body, so they join last.
    seeder.collect_explicit_only = false;
    seeder.walk_blocks(&doc.children);
    for blocks in doc.footnote_defs.values() {
        seeder.walk_blocks(blocks);
    }
    seeder.registry
}

struct Seeder {
    registry: DocumentIdRegistry,
    /// Simulates `RenderState::heading_counts` so the reserved heading ids
    /// match the ones `next_heading_id` assigns during the actual render.
    heading_counts: BTreeMap<String, usize>,
    /// Citation key -> index into `registry.pending_citations`.
    citation_index: BTreeMap<String, usize>,
    lowercase_heading_ids: bool,
    /// The id assigned to each heading, in document order. Written in pass B
    /// and read by [`assigned_heading_ids`], which is how the AST encoder
    /// publishes a generated id (PART 12 §5, carve#750) without a second
    /// implementation of slug + dedup to drift from this one.
    assigned: Vec<String>,
    /// Pass A only reserves EXPLICIT ids (so the whole explicit-id set is known
    /// before any heading is numbered); heading + citation reservation run in
    /// pass B.
    collect_explicit_only: bool,
}

impl Seeder {
    fn reserve_attrs(&mut self, attrs: &Option<Attrs>) {
        if let Some(id) = attrs.as_ref().and_then(|attrs| attrs.id.as_deref()) {
            self.registry.reserve(id);
            self.registry.explicit_ids.insert(id.to_string());
        }
    }

    /// Reserve the id the renderer will assign to this heading: the explicit
    /// attribute id or the text slug, numbered by the shared document-order
    /// counter (mirrors `render::next_heading_id`).
    fn reserve_heading_id(&mut self, h: &Heading) {
        let explicit = h.attrs.as_ref().and_then(|attrs| attrs.id.clone());
        let has_explicit = explicit.is_some();
        let base = explicit.unwrap_or_else(|| {
            crate::parse::slugify_parse(
                &crate::render::plain_inlines(&h.children),
                self.lowercase_heading_ids,
            )
        });
        let mut count = self.heading_counts.get(&base).copied().unwrap_or(0);
        let id = loop {
            count += 1;
            let id = if count == 1 {
                base.clone()
            } else {
                format!("{base}-{count}")
            };
            // An explicit heading id wins verbatim; an auto slug skips any id an
            // explicit `{#id}` elsewhere already claimed (avoids a duplicate id).
            if has_explicit || !self.registry.explicit_ids.contains(&id) {
                break id;
            }
        };
        self.heading_counts.insert(base, count);
        self.registry.reserve(&id);
        self.assigned.push(id);
    }

    fn walk_blocks(&mut self, blocks: &[BlockNode]) {
        for block in blocks {
            self.walk_block(block);
        }
    }

    fn walk_block(&mut self, block: &BlockNode) {
        match block {
            BlockNode::Heading(h) => {
                if self.collect_explicit_only {
                    // Pass A: a heading's own `{#id}` is an explicit id like any
                    // other block's, and has to be RECORDED as one. It was only
                    // reserved, in pass B - so the guard in `reserve_heading_id`
                    // and in `render::next_heading_id`, which both ask
                    // "has an explicit id claimed this?", could never see a
                    // heading's. `{#API-2}` on one heading plus a later
                    // `# API` then emitted `id="API-2"` twice (#335).
                    self.reserve_attrs(&h.attrs);
                } else {
                    self.reserve_heading_id(h);
                }
                self.walk_inlines(&h.children);
            }
            BlockNode::Paragraph(p) => {
                self.reserve_attrs(&p.attrs);
                self.walk_inlines(&p.children);
            }
            BlockNode::CitationDefinition(d) => {
                // The entry is inline content that renders in the references
                // list, so an id authored inside it is claimed like any other.
                self.reserve_attrs(&d.attrs);
                self.walk_inlines(&d.children);
            }
            BlockNode::CodeBlock(c) => self.reserve_attrs(&c.attrs),
            BlockNode::List(l) => {
                self.reserve_attrs(&l.attrs);
                for item in &l.items {
                    self.reserve_attrs(&item.attrs);
                    self.walk_blocks(&item.children);
                }
            }
            BlockNode::BlockQuote(b) => self.walk_blockquote(b),
            BlockNode::Table(t) => self.walk_table(t),
            BlockNode::Admonition(a) => {
                self.reserve_attrs(&a.attrs);
                if let Some(title) = &a.title {
                    self.walk_inlines(title);
                }
                self.walk_blocks(&a.children);
            }
            BlockNode::Div(d) => {
                self.reserve_attrs(&d.attrs);
                self.walk_blocks(&d.children);
            }
            BlockNode::LineBlock(lb) => {
                self.reserve_attrs(&lb.attrs);
                self.walk_blocks(&lb.children);
            }
            BlockNode::DefinitionList(d) => {
                self.reserve_attrs(&d.attrs);
                for item in &d.items {
                    for term in &item.terms {
                        self.walk_inlines(term);
                    }
                    for definition in &item.definitions {
                        self.walk_blocks(definition);
                    }
                }
            }
            BlockNode::Figure(f) => {
                self.reserve_attrs(&f.attrs);
                match &*f.target {
                    FigureTarget::Image(i) => self.reserve_attrs(&i.attrs),
                    FigureTarget::BlockQuote(b) => self.walk_blockquote(b),
                    FigureTarget::Table(t) => self.walk_table(t),
                    FigureTarget::CodeBlock(c) => self.reserve_attrs(&c.attrs),
                    FigureTarget::Paragraph(p) => {
                        self.reserve_attrs(&p.attrs);
                        self.walk_inlines(&p.children);
                    }
                }
                self.walk_inlines(&f.caption);
            }
            BlockNode::FigureGroup(g) => {
                self.reserve_attrs(&g.attrs);
                self.walk_blocks(&g.children);
                if let Some(caption) = &g.caption {
                    self.walk_inlines(caption);
                }
            }
            BlockNode::Extension(e) => {
                self.reserve_attrs(&e.attrs);
                self.walk_blocks(&e.children);
            }
            BlockNode::BlockImage(i) => self.reserve_attrs(&i.attrs),
            BlockNode::ThematicBreak(t) => self.reserve_attrs(&t.attrs),
            BlockNode::LinkReferenceDefinition(_)
            | BlockNode::AbbreviationDef(_)
            | BlockNode::RawBlock(_)
            | BlockNode::Comment(_) => {}
        }
    }

    fn walk_blockquote(&mut self, b: &BlockQuote) {
        self.reserve_attrs(&b.attrs);
        self.walk_blocks(&b.children);
    }

    fn walk_table(&mut self, t: &Table) {
        self.reserve_attrs(&t.attrs);
        if let Some(caption) = &t.caption {
            self.walk_inlines(caption);
        }
        for row in &t.rows {
            self.reserve_attrs(&row.attrs);
            for cell in &row.cells {
                self.reserve_attrs(&cell.attrs);
                self.walk_inlines(&cell.children);
            }
        }
    }

    fn walk_inlines(&mut self, nodes: &[InlineNode]) {
        for node in nodes {
            match node {
                InlineNode::Emphasis(e) => {
                    self.reserve_attrs(&e.attrs);
                    self.walk_inlines(&e.children);
                }
                InlineNode::Code(code) => self.reserve_attrs(&code.attrs),
                InlineNode::LiteralInline(lit) => self.reserve_attrs(&lit.attrs),
                InlineNode::Link(l) => {
                    self.reserve_attrs(&l.attrs);
                    self.walk_inlines(&l.children);
                }
                InlineNode::Image(i) => self.reserve_attrs(&i.attrs),
                InlineNode::Span(s) => {
                    self.reserve_attrs(&s.attrs);
                    self.walk_inlines(&s.children);
                }
                InlineNode::Math(m) => self.reserve_attrs(&m.attrs),
                InlineNode::AutoLink(a) => self.reserve_attrs(&a.attrs),
                InlineNode::CitationGroup(g) if !self.collect_explicit_only => {
                    self.collect_citation_group(g)
                }
                InlineNode::Extension(e) => {
                    self.reserve_attrs(&e.attrs);
                    self.walk_inlines(&e.children);
                }
                InlineNode::Footnote(f) => {
                    self.reserve_attrs(&f.attrs);
                    if let Some(inline) = &f.inline {
                        self.walk_inlines(inline);
                    }
                }
                InlineNode::CriticInsert(c) => self.walk_inlines(&c.children),
                InlineNode::CriticDelete(c) => self.walk_inlines(&c.children),
                _ => {}
            }
        }
    }

    /// Collect the citation use sites the render will mint ids for. Mirrors
    /// the resolution rule in `citations`: a group with any unresolved item
    /// renders verbatim (its items carry no `label`) and mints no ids; a
    /// resolved item contributes a `ref-{key}` entry (first-citation order)
    /// and — when a bibliography pool assigned it a `use_index` — one
    /// `cite-{key}-{n}` use-site anchor.
    fn collect_citation_group(&mut self, group: &CitationGroup) {
        if group.items.iter().all(|item| item.label.is_some()) {
            for item in &group.items {
                let index = match self.citation_index.get(&item.key) {
                    Some(&index) => index,
                    None => {
                        let index = self.registry.pending_citations.len();
                        self.registry.pending_citations.push((item.key.clone(), 0));
                        self.citation_index.insert(item.key.clone(), index);
                        index
                    }
                };
                if item.use_index.is_some() {
                    self.registry.pending_citations[index].1 += 1;
                }
            }
        }
        for item in &group.items {
            if let Some(prefix) = &item.prefix {
                self.walk_inlines(prefix);
            }
            if let Some(locator) = &item.locator {
                self.walk_inlines(locator);
            }
            if let Some(suffix) = &item.suffix {
                self.walk_inlines(suffix);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::DocumentIdRegistry;

    #[test]
    fn unique_id_returns_base_when_free_and_suffixes_on_collision() {
        let mut registry = DocumentIdRegistry::default();
        assert_eq!(registry.unique_id("tabset-1"), "tabset-1");
        assert_eq!(registry.unique_id("tabset-1"), "tabset-1-2");
        assert_eq!(registry.unique_id("tabset-1"), "tabset-1-3");
    }

    #[test]
    fn unique_id_skips_reserved_suffix_candidates() {
        let mut registry = DocumentIdRegistry::default();
        registry.reserve("x-2");
        registry.reserve("x-3");
        assert_eq!(registry.unique_id("x"), "x");
        assert_eq!(registry.unique_id("x"), "x-4");
    }
}
