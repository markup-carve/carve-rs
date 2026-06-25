//! Carve AST node definitions for the MVP subset.
//!
//! Mirrors the shape of `markup-carve/carve-js`'s `ast.ts`, but only
//! covers constructs the MVP parser+renderer produces. Tables,
//! admonitions, abbreviations, mentions/tags, attributes, and
//! frontmatter are deferred to future PRs.

use std::collections::BTreeMap;

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
    pub terms: Vec<Vec<InlineNode>>,
    pub definitions: Vec<Vec<BlockNode>>,
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
    /// Optional pre-flattened title text carried by a `before_render`
    /// rewrite (e.g. the `details` extension stashes the admonition title
    /// here so its renderer can emit a `<summary>`). `None` for ordinary
    /// extension carrier nodes.
    pub summary: Option<String>,
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
    Emoji(Emoji),
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    pub attrs: Option<Attrs>,
    pub src: String,
    pub alt: String,
    pub title: Option<String>,
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
pub struct Emoji {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutoLink {
    pub attrs: Option<Attrs>,
    pub href: String,
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
    pub suppress_author: bool,
    pub number: Option<usize>,
    pub label: Option<String>,
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CriticDelete {
    pub children: Vec<InlineNode>,
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
