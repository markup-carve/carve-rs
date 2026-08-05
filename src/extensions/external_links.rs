//! Add `target` and `rel` to external links (`http(s)://…`).
//!
//! Port of carve-js `external-links.ts` / carve-php's `ExternalLinksExtension`.
//! Runs as a `before_render` transform, so the attributes it sets are emitted
//! by the core link renderer. Walks the whole document (links can sit inside
//! table cells, list items, captions, definition lists, footnote defs).

use crate::ast::{AttrSlot, Attrs, BlockNode, Document, InlineNode};
use crate::extension::{BeforeRenderContext, CarveExtension};

/// Options for [`ExternalLinks`].
#[derive(Debug, Clone)]
pub struct ExternalLinksOptions {
    /// `target` attribute value. Default `"_blank"`.
    pub target: String,
    /// `rel` attribute value. Default `"noopener noreferrer"`.
    pub rel: String,
    /// Append `nofollow` to `rel`. Default false.
    pub nofollow: bool,
}

impl Default for ExternalLinksOptions {
    fn default() -> Self {
        Self {
            target: "_blank".into(),
            rel: "noopener noreferrer".into(),
            nofollow: false,
        }
    }
}

/// Add `target` and `rel` to external links.
///
/// ```
/// use carve::{ExternalLinks, Options};
/// let ext = ExternalLinks::new();
/// let opts = Options::new().with_extension(&ext);
/// let html = carve::to_html_with_options("[docs](https://example.com)", &opts);
/// assert_eq!(
///     html,
///     "<p><a href=\"https://example.com\" target=\"_blank\" rel=\"noopener noreferrer\">docs</a></p>"
/// );
/// ```
pub struct ExternalLinks {
    target: String,
    rel: String,
}

impl ExternalLinks {
    /// Create an external-links extension with default options.
    pub fn new() -> Self {
        Self::with_options(ExternalLinksOptions::default())
    }

    /// Create an external-links extension with explicit options.
    pub fn with_options(opts: ExternalLinksOptions) -> Self {
        let mut rel = opts.rel;
        if opts.nofollow && !rel.split_whitespace().any(|w| w == "nofollow") {
            rel = format!("{rel} nofollow").trim().to_string();
        }
        Self {
            target: opts.target,
            rel,
        }
    }

    fn mark(&self, attrs: &mut Attrs) {
        set_attr(attrs, "target", &self.target);
        set_attr(attrs, "rel", &self.rel);
    }
}

impl Default for ExternalLinks {
    fn default() -> Self {
        Self::new()
    }
}

impl CarveExtension for ExternalLinks {
    fn name(&self) -> &'static str {
        "external-links"
    }

    fn before_render(&self, mut doc: Document, _ctx: &BeforeRenderContext<'_>) -> Document {
        for block in &mut doc.children {
            self.visit_block(block);
        }
        for defs in doc.footnote_defs.values_mut() {
            for block in defs.iter_mut() {
                self.visit_block(block);
            }
        }
        doc
    }
}

impl ExternalLinks {
    fn visit_block(&self, block: &mut BlockNode) {
        match block {
            BlockNode::Heading(h) => self.visit_inlines(&mut h.children),
            BlockNode::Paragraph(p) => self.visit_inlines(&mut p.children),
            BlockNode::List(l) => {
                for item in &mut l.items {
                    for child in &mut item.children {
                        self.visit_block(child);
                    }
                }
            }
            BlockNode::BlockQuote(b) => {
                for child in &mut b.children {
                    self.visit_block(child);
                }
                if let Some(attr) = &mut b.attribution {
                    self.visit_inlines(attr);
                }
            }
            BlockNode::Table(t) => {
                if let Some(cap) = &mut t.caption {
                    self.visit_inlines(cap);
                }
                for row in &mut t.rows {
                    for cell in &mut row.cells {
                        self.visit_inlines(&mut cell.children);
                    }
                }
            }
            BlockNode::Admonition(a) => {
                if let Some(title) = &mut a.title {
                    self.visit_inlines(title);
                }
                for child in &mut a.children {
                    self.visit_block(child);
                }
            }
            BlockNode::LineBlock(lb) => {
                for child in &mut lb.children {
                    self.visit_block(child);
                }
            }
            BlockNode::Div(d) => {
                for child in &mut d.children {
                    self.visit_block(child);
                }
            }
            BlockNode::DefinitionList(dl) => {
                for item in &mut dl.items {
                    for term in &mut item.terms {
                        self.visit_inlines(term);
                    }
                    for def in &mut item.definitions {
                        for child in def.iter_mut() {
                            self.visit_block(child);
                        }
                    }
                }
            }
            BlockNode::Figure(f) => {
                self.visit_inlines(&mut f.caption);
                self.visit_figure_target(f);
            }
            BlockNode::Extension(e) => {
                for child in &mut e.children {
                    self.visit_block(child);
                }
            }
            BlockNode::CodeBlock(_)
            | BlockNode::LinkReferenceDefinition(_)
            | BlockNode::AbbreviationDef(_)
            | BlockNode::RawBlock(_)
            | BlockNode::Comment(_)
            | BlockNode::BlockImage(_)
            | BlockNode::ThematicBreak(_) => {}
        }
    }

    fn visit_figure_target(&self, f: &mut crate::ast::Figure) {
        use crate::ast::FigureTarget;
        match &mut f.target {
            FigureTarget::BlockQuote(b) => {
                for child in &mut b.children {
                    self.visit_block(child);
                }
            }
            FigureTarget::Table(t) => {
                if let Some(cap) = &mut t.caption {
                    self.visit_inlines(cap);
                }
                for row in &mut t.rows {
                    for cell in &mut row.cells {
                        self.visit_inlines(&mut cell.children);
                    }
                }
            }
            FigureTarget::Paragraph(p) => self.visit_inlines(&mut p.children),
            FigureTarget::Image(_) | FigureTarget::CodeBlock(_) => {}
        }
    }

    fn visit_inlines(&self, nodes: &mut [InlineNode]) {
        for node in nodes {
            match node {
                InlineNode::Link(l) => {
                    if is_external(&l.href) {
                        let attrs = l.attrs.get_or_insert_with(Attrs::default);
                        self.mark(attrs);
                    }
                    self.visit_inlines(&mut l.children);
                }
                InlineNode::AutoLink(a) if is_external(&a.href) => {
                    let attrs = a.attrs.get_or_insert_with(Attrs::default);
                    self.mark(attrs);
                }
                InlineNode::Emphasis(e) => self.visit_inlines(&mut e.children),
                InlineNode::Span(s) => self.visit_inlines(&mut s.children),
                InlineNode::Extension(e) => self.visit_inlines(&mut e.children),
                InlineNode::CriticInsert(c) => self.visit_inlines(&mut c.children),
                InlineNode::CriticDelete(c) => self.visit_inlines(&mut c.children),
                InlineNode::Footnote(f) => {
                    if let Some(inline) = &mut f.inline {
                        self.visit_inlines(inline);
                    }
                }
                _ => {}
            }
        }
    }
}

fn is_external(href: &str) -> bool {
    let lower = href.to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

/// Set a key-value attribute, replacing any existing key that matches
/// case-insensitively (HTML attribute names are case-insensitive; a stray
/// `{Target=_self}` would otherwise win). Appends a fresh `order` slot when the
/// key is new so the attribute actually renders.
fn set_attr(attrs: &mut Attrs, name: &str, value: &str) {
    let dup_keys: Vec<String> = attrs
        .key_values
        .keys()
        .filter(|k| k.as_str() != name && k.eq_ignore_ascii_case(name))
        .cloned()
        .collect();
    for key in dup_keys {
        attrs.key_values.remove(&key);
        attrs
            .order
            .retain(|slot| !matches!(slot, AttrSlot::Key(k) if *k == key));
    }
    let existed = attrs.key_values.contains_key(name);
    attrs.key_values.insert(name.to_string(), value.to_string());
    if !existed && !attrs.order.is_empty() {
        attrs.order.push(AttrSlot::Key(name.to_string()));
    } else if !existed && attrs.order.is_empty() {
        // No explicit order on this node yet. Build one so the appended keys
        // render in a deterministic, js-matching sequence rather than the
        // BTreeMap default. Seed with the existing structural slots.
        if attrs.id.is_some() {
            attrs.order.push(AttrSlot::Id);
        }
        if !attrs.classes.is_empty() {
            attrs.order.push(AttrSlot::Class);
        }
        for key in attrs.key_values.keys() {
            if key != name {
                attrs.order.push(AttrSlot::Key(key.clone()));
            }
        }
        attrs.order.push(AttrSlot::Key(name.to_string()));
    }
}
