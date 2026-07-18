//! Carve AST node definitions for the MVP subset.
//!
//! Mirrors the shape of `markup-carve/carve-js`'s `ast.ts`, but only
//! covers constructs the MVP parser+renderer produces. Tables,
//! admonitions, abbreviations, mentions/tags, attributes, and
//! frontmatter are deferred to future PRs.

use std::collections::BTreeMap;
use std::ops::{Deref, DerefMut};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Attrs {
    pub id: Option<String>,
    pub classes: Vec<String>,
    pub key_values: BTreeMap<String, String>,
    pub order: Vec<AttrSlot>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttrSlot {
    Id,
    Class,
    Key(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    pub frontmatter: BTreeMap<String, String>,
    pub footnote_defs: BTreeMap<String, Vec<BlockNode>>,
    pub children: Vec<BlockNode>,
    /// Byte length of the (normalized) source this document was parsed from.
    /// Renderers use it to size the abbreviation-expansion budget that bounds
    /// memory-amplification DoS (see `ABBR_EXPANSION_BUDGET_BASE`). Documents
    /// built programmatically (not via `parse`) leave this at 0, which yields
    /// the budget floor — far above any realistic hand-built document.
    pub source_len: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockNode {
    Heading(Heading),
    Paragraph(Paragraph),
    CodeBlock(CodeBlock),
    List(List),
    BlockQuote(BlockQuote),
    Table(Table),
    Admonition(Admonition),
    Div(Div),
    DefinitionList(DefinitionList),
    Figure(Figure),
    AbbreviationDef(AbbreviationDef),
    RawBlock(RawBlock),
    Comment(Comment),
    Extension(BlockExtension),
    BlockImage(Image),
    ThematicBreak(ThematicBreak),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ThematicBreak {
    /// Attributes from a preceding block-attribute line (`{.x}` then `---`).
    pub attrs: Option<Attrs>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heading {
    pub attrs: Option<Attrs>,
    pub level: u8,
    pub children: Vec<InlineNode>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paragraph {
    pub attrs: Option<Attrs>,
    pub children: Vec<InlineNode>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeBlock {
    pub attrs: Option<Attrs>,
    pub lang: Option<String>,
    pub title: Option<String>,
    pub label: Option<String>,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct List {
    pub attrs: Option<Attrs>,
    pub ordered: bool,
    pub start: Option<usize>,
    pub ol_type: Option<OrderedListType>,
    pub tight: bool,
    pub items: Vec<ListItem>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderedListType {
    LowerAlpha,
    UpperAlpha,
    LowerRoman,
    UpperRoman,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListItem {
    pub attrs: Option<Attrs>,
    /// `None` for plain bullets; `Some(checked)` for task-list items.
    pub checked: Option<bool>,
    pub children: Vec<BlockNode>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockQuote {
    pub attrs: Option<Attrs>,
    pub children: Vec<BlockNode>,
    pub attribution: Option<Vec<InlineNode>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Table {
    pub attrs: Option<Attrs>,
    pub caption: Option<Vec<InlineNode>>,
    pub rows: Vec<TableRow>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableRow {
    pub cells: Vec<TableCell>,
    /// Row-level attributes from a `{...}` block glued to the closing pipe.
    pub attrs: Option<Attrs>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableCell {
    pub header: bool,
    pub span: Option<TableCellSpan>,
    pub align: Option<TableAlign>,
    /// Author attributes from a `{...}` glued to the cell's opening pipe.
    pub attrs: Option<Attrs>,
    pub children: Vec<InlineNode>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableCellSpan {
    Rowspan,
    Colspan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableAlign {
    Left,
    Right,
    Center,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Admonition {
    pub attrs: Option<Attrs>,
    pub kind: String,
    pub title: Option<Vec<InlineNode>>,
    pub label: Option<String>,
    pub children: Vec<BlockNode>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Div {
    pub attrs: Option<Attrs>,
    pub label: Option<String>,
    pub children: Vec<BlockNode>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefinitionItem {
    pub terms: Vec<DefinitionTerm>,
    pub definitions: Vec<DefinitionDef>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefinitionTerm {
    pub attrs: Option<Attrs>,
    pub children: Vec<InlineNode>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefinitionDef {
    pub attrs: Option<Attrs>,
    pub children: Vec<BlockNode>,
}

impl Deref for DefinitionTerm {
    type Target = Vec<InlineNode>;

    fn deref(&self) -> &Self::Target {
        &self.children
    }
}

impl DerefMut for DefinitionTerm {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.children
    }
}

impl Deref for DefinitionDef {
    type Target = Vec<BlockNode>;

    fn deref(&self) -> &Self::Target {
        &self.children
    }
}

impl DerefMut for DefinitionDef {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.children
    }
}

impl<'a> IntoIterator for &'a DefinitionDef {
    type Item = &'a BlockNode;
    type IntoIter = std::slice::Iter<'a, BlockNode>;

    fn into_iter(self) -> Self::IntoIter {
        self.children.iter()
    }
}

impl<'a> IntoIterator for &'a mut DefinitionDef {
    type Item = &'a mut BlockNode;
    type IntoIter = std::slice::IterMut<'a, BlockNode>;

    fn into_iter(self) -> Self::IntoIter {
        self.children.iter_mut()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefinitionList {
    pub attrs: Option<Attrs>,
    pub items: Vec<DefinitionItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Figure {
    pub attrs: Option<Attrs>,
    pub target: FigureTarget,
    pub caption: Vec<InlineNode>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FigureTarget {
    Image(Image),
    BlockQuote(BlockQuote),
    Table(Table),
    CodeBlock(CodeBlock),
    Paragraph(Paragraph),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AbbreviationDef {
    pub abbr: String,
    pub expansion: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawBlock {
    pub format: String,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comment {
    pub block: bool,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockExtension {
    pub attrs: Option<Attrs>,
    pub name: String,
    pub children: Vec<BlockNode>,
    /// Optional parsed title carried by a `before_render` rewrite (e.g. the
    /// `details` extension stashes the admonition title here so its renderer
    /// can emit a `<summary>`). `None` for ordinary extension carrier nodes.
    pub summary: Option<Vec<InlineNode>>,
    /// Optional grouping `[label]` carried over from the source container, so a
    /// static-mode renderer can surface it as the caption floor (mirroring the
    /// core caption floor for an unconsumed label). `None` when the source had
    /// no label.
    pub label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InlineNode {
    Text(String),
    Emphasis(Emphasis),
    Code(String, Option<Attrs>),
    Link(Link),
    Image(Image),
    Span(Span),
    Math(Math),
    RawInline(RawInline),
    Symbol(Symbol),
    AutoLink(AutoLink),
    CrossRef(CrossRef),
    CaptionNumber(CaptionNumber),
    Mention(Mention),
    Tag(Tag),
    CitationGroup(CitationGroup),
    Extension(InlineExtension),
    Abbreviation(Abbreviation),
    Footnote(Footnote),
    SoftBreak,
    HardBreak,
    CriticInsert(CriticInsert),
    CriticDelete(CriticDelete),
    CriticSubstitute(CriticSubstitute),
    CriticComment(CriticComment),
}

pub(crate) fn inline_nodes_without_strong(nodes: &[InlineNode]) -> Vec<InlineNode> {
    let mut out = Vec::new();
    for node in nodes {
        match node {
            InlineNode::Emphasis(e) if e.kind == EmphasisKind::Strong => {
                out.extend(inline_nodes_without_strong(&e.children));
            }
            InlineNode::Emphasis(e) => {
                let mut e = e.clone();
                e.children = inline_nodes_without_strong(&e.children);
                out.push(InlineNode::Emphasis(e));
            }
            InlineNode::Link(l) => {
                let mut l = l.clone();
                l.children = inline_nodes_without_strong(&l.children);
                out.push(InlineNode::Link(l));
            }
            InlineNode::Span(s) => {
                let mut s = s.clone();
                s.children = inline_nodes_without_strong(&s.children);
                out.push(InlineNode::Span(s));
            }
            InlineNode::Extension(e) => {
                let mut e = e.clone();
                e.children = inline_nodes_without_strong(&e.children);
                out.push(InlineNode::Extension(e));
            }
            InlineNode::CitationGroup(g) => {
                let mut g = g.clone();
                for item in &mut g.items {
                    if let Some(prefix) = &mut item.prefix {
                        *prefix = inline_nodes_without_strong(prefix);
                    }
                    if let Some(locator) = &mut item.locator {
                        *locator = inline_nodes_without_strong(locator);
                    }
                    if let Some(suffix) = &mut item.suffix {
                        *suffix = inline_nodes_without_strong(suffix);
                    }
                }
                out.push(InlineNode::CitationGroup(g));
            }
            InlineNode::Footnote(f) => {
                let mut f = f.clone();
                if let Some(inline) = &mut f.inline {
                    *inline = inline_nodes_without_strong(inline);
                }
                out.push(InlineNode::Footnote(f));
            }
            InlineNode::CriticInsert(c) => {
                let mut c = c.clone();
                c.children = inline_nodes_without_strong(&c.children);
                out.push(InlineNode::CriticInsert(c));
            }
            InlineNode::CriticDelete(c) => {
                let mut c = c.clone();
                c.children = inline_nodes_without_strong(&c.children);
                out.push(InlineNode::CriticDelete(c));
            }
            _ => out.push(node.clone()),
        }
    }
    out
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Emphasis {
    pub attrs: Option<Attrs>,
    pub kind: EmphasisKind,
    pub children: Vec<InlineNode>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmphasisKind {
    Italic,
    Strong,
    Underline,
    Strike,
    Super,
    Sub,
    Highlight,
    BoldItalic,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    pub attrs: Option<Attrs>,
    pub href: String,
    pub title: Option<String>,
    pub children: Vec<InlineNode>,
    pub ref_label: Option<String>,
    pub raw_ref: Option<String>,
    /// Set when this `Link` was produced from a `</#id>` cross-reference (not an
    /// ordinary `[text](url)` link or an implicit `[label][]` reference).
    /// Non-rendered metadata - every renderer ignores it; it lets a render-stage
    /// extension (HeadingNumbers, #198) rewrite only auto-filled
    /// cross-references without a fragile title-equality guess.
    pub from_crossref: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    pub attrs: Option<Attrs>,
    pub src: String,
    pub alt: String,
    pub title: Option<String>,
    /// Unresolved reference label for `![alt][ref]` / collapsed `![alt][]`,
    /// mirroring [`Link::ref_label`]. `resolve_reference_links` matches it
    /// against the document's explicit `[label]: url` defs (case-sensitively):
    /// on hit it fills `src`/`title` and clears these; an unresolved image ref
    /// becomes the literal `raw_ref` source. Unlike a link ref it never matches
    /// heading text. `None` for a direct `![alt](src)` image.
    pub ref_label: Option<String>,
    pub raw_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineExtension {
    pub attrs: Option<Attrs>,
    pub name: String,
    pub children: Vec<InlineNode>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub attrs: Option<Attrs>,
    pub children: Vec<InlineNode>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Math {
    pub attrs: Option<Attrs>,
    pub display: bool,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawInline {
    pub format: String,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Symbol {
    pub name: String,
    pub attrs: Option<Attrs>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutoLink {
    pub attrs: Option<Attrs>,
    pub href: String,
    /// Display text = the raw content between `<>`: a URI autolink keeps its
    /// scheme (`<mailto:a@b>` shows `mailto:a@b`), an email shows the address.
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrossRef {
    pub target: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptionNumber {
    pub number: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mention {
    pub user: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tag {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CitationGroup {
    pub items: Vec<Citation>,
    pub raw: String,
    pub mode: Option<CitationRenderMode>,
    pub integral: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CitationRenderMode {
    Numbered,
    AuthorDate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Citation {
    pub key: String,
    pub prefix: Option<Vec<InlineNode>>,
    pub locator: Option<Vec<InlineNode>>,
    pub locator_label: Option<String>,
    pub locator_value: Option<String>,
    pub suffix: Option<Vec<InlineNode>>,
    pub suppress_author: bool,
    pub number: Option<usize>,
    pub label: Option<String>,
    /// Per-key, document-wide use-site index (1-based), assigned only when a
    /// bibliography pool is supplied; drives back-link anchors (#199).
    pub use_index: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Abbreviation {
    pub abbr: String,
    pub expansion: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Footnote {
    pub attrs: Option<Attrs>,
    pub id: Option<String>,
    pub inline: Option<Vec<InlineNode>>,
    pub number: Option<usize>,
    pub ref_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CriticInsert {
    pub children: Vec<InlineNode>,
    pub attrs: Option<Attrs>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CriticDelete {
    pub children: Vec<InlineNode>,
    pub attrs: Option<Attrs>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CriticSubstitute {
    pub old_text: String,
    pub new_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CriticComment {
    pub text: String,
}
