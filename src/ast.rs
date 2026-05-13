//! Carve AST node definitions for the MVP subset.
//!
//! Mirrors the shape of `markup-carve/carve-js`'s `ast.ts`, but only
//! covers constructs the MVP parser+renderer produces. Tables,
//! admonitions, abbreviations, mentions/tags, extensions, attributes,
//! and frontmatter are deferred to future PRs.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    pub children: Vec<BlockNode>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockNode {
    Heading(Heading),
    Paragraph(Paragraph),
    CodeBlock(CodeBlock),
    List(List),
    BlockQuote(BlockQuote),
    BlockImage(Image),
    ThematicBreak,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heading {
    pub level: u8,
    pub children: Vec<InlineNode>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paragraph {
    pub children: Vec<InlineNode>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeBlock {
    pub lang: Option<String>,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct List {
    pub ordered: bool,
    pub items: Vec<ListItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListItem {
    /// `None` for plain bullets; `Some(checked)` for task-list items.
    pub checked: Option<bool>,
    pub children: Vec<BlockNode>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockQuote {
    pub children: Vec<BlockNode>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InlineNode {
    Text(String),
    Emphasis(Emphasis),
    Code(String),
    Link(Link),
    Image(Image),
    SoftBreak,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Emphasis {
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
    pub href: String,
    pub title: Option<String>,
    pub children: Vec<InlineNode>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    pub src: String,
    pub alt: String,
    pub title: Option<String>,
}
