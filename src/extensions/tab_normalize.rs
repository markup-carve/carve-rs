//! Normalize tabs to spaces in code content on output.
//!
//! Port of carve-js `tab-normalize.ts`. Carve preserves literal tabs in code
//! blocks and inline code by default (djot/CommonMark-aligned). This `before_render`
//! transform expands each tab to a fixed number of spaces before rendering,
//! useful for fixed-width output without CSS (email, RSS, plain HTML).
//!
//! Flat replacement: every tab becomes exactly `width` spaces (no elastic tab
//! stops). Only code CONTENT is touched (fenced code blocks and inline code
//! spans), never prose, attributes, or structure. Default width is 2 (matching
//! djot's 2-space convention).
//!
//! Note: carve-js documents this as a source/line preprocessor in spirit, but
//! its implementation (which this mirrors) is an AST transform over code
//! content. carve-rs has no pre-parse line seam, so the AST transform is the
//! exact equivalent.

use crate::ast::{BlockNode, Document, InlineNode};
use crate::extension::{BeforeRenderContext, CarveExtension};

/// Normalize tabs to spaces in code content.
///
/// ```
/// use carve::{TabNormalize, Options};
/// let ext = TabNormalize::new();
/// let opts = Options::new().with_extension(&ext);
/// let html = carve::to_html_with_options("```\n\tindented\n```\n", &opts);
/// assert!(html.contains("  indented"));
/// ```
pub struct TabNormalize {
    spaces: String,
}

impl TabNormalize {
    /// Create a tab-normalize extension with the default width of 2 spaces.
    pub fn new() -> Self {
        Self::with_width(2)
    }

    /// Create a tab-normalize extension with an explicit tab width.
    pub fn with_width(width: usize) -> Self {
        Self {
            spaces: " ".repeat(width),
        }
    }

    fn expand(&self, s: &str) -> String {
        s.replace('\t', &self.spaces)
    }
}

impl Default for TabNormalize {
    fn default() -> Self {
        Self::new()
    }
}

impl CarveExtension for TabNormalize {
    fn name(&self) -> &'static str {
        "tab-normalize"
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

impl TabNormalize {
    fn visit_block(&self, block: &mut BlockNode) {
        match block {
            BlockNode::CodeBlock(c) => c.content = self.expand(&c.content),
            BlockNode::Heading(h) => self.visit_inlines(&mut h.children),
            BlockNode::Paragraph(p) => self.visit_inlines(&mut p.children),
            BlockNode::CitationDefinition(d) => self.visit_inlines(&mut d.children),
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
            BlockNode::FigureGroup(g) => {
                if let Some(caption) = &mut g.caption {
                    self.visit_inlines(caption);
                }
                for child in &mut g.children {
                    self.visit_block(child);
                }
            }
            BlockNode::Extension(e) => {
                for child in &mut e.children {
                    self.visit_block(child);
                }
            }
            BlockNode::LinkReferenceDefinition(_)
            | BlockNode::AbbreviationDef(_)
            | BlockNode::RawBlock(_)
            | BlockNode::Comment(_)
            | BlockNode::BlockImage(_)
            | BlockNode::ThematicBreak(_) => {}
        }
    }

    fn visit_figure_target(&self, f: &mut crate::ast::Figure) {
        use crate::ast::FigureTarget;
        match &mut *f.target {
            FigureTarget::CodeBlock(c) => c.content = self.expand(&c.content),
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
            FigureTarget::Image(_) => {}
        }
    }

    fn visit_inlines(&self, nodes: &mut [InlineNode]) {
        for node in nodes {
            match node {
                InlineNode::Code(code) => code.value = self.expand(&code.value),
                InlineNode::Emphasis(e) => self.visit_inlines(&mut e.children),
                InlineNode::Link(l) => self.visit_inlines(&mut l.children),
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
