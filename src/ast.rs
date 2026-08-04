//! Carve AST node definitions for the MVP subset.
//!
//! Mirrors the shape of `markup-carve/carve-js`'s `ast.ts`, but only
//! covers constructs the MVP parser+renderer produces. Tables,
//! admonitions, abbreviations, mentions/tags, attributes, and
//! frontmatter are deferred to future PRs.

use std::collections::BTreeMap;
use std::ops::{Deref, DerefMut};

/// A node's span in the ORIGINAL source (spec PART 12 section 4).
///
/// Lines and columns are 1-based; offsets are 0-based byte offsets. `end_column`
/// and `end_offset` are exclusive.
///
/// Recording this is not free here: the parser works on lines whose container
/// prefixes have already been stripped - a blockquote marker, a list indent -
/// so a column in the text the parser sees is not a column in the document.
/// `MappedSource` therefore carries the stripped width per line alongside the
/// line map, and that is what makes the column recoverable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Pos {
    pub start_line: usize,
    pub end_line: usize,
    pub start_column: usize,
    pub end_column: usize,
    pub start_offset: usize,
    pub end_offset: usize,
}

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

/// A frontmatter block as the author wrote it: the format token from the
/// opening fence, and the raw text between the fences.
///
/// Kept alongside the parsed `frontmatter` map because the map cannot be
/// serialized back to the source - key order, comments, anchors and any
/// non-`key: value` structure are gone the moment it is built, and a typed
/// (`---json`, `---toml`) block is not parsed into it at all. Spec PART 12 §2
/// pins the raw form for exactly that reason.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Frontmatter {
    /// The token after the opening fence, or `yaml` when the fence is bare -
    /// matching what the reference publishes.
    pub format: String,
    /// The text between the fences, without a trailing newline.
    pub content: String,
    /// Span in the original source, when the caller asked for positions. Covers
    /// the whole block, fences included - matching the reference.
    pub pos: Option<Pos>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    pub frontmatter: BTreeMap<String, String>,
    /// The frontmatter block as written, when the document has one.
    pub frontmatter_raw: Option<Frontmatter>,
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
    LineBlock(LineBlock),
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
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heading {
    pub attrs: Option<Attrs>,
    pub level: u8,
    pub children: Vec<InlineNode>,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Paragraph {
    pub attrs: Option<Attrs>,
    pub children: Vec<InlineNode>,
    /// Whether the paragraph's first source line began at its container's content
    /// column (column 0 at the top level), i.e. was NOT indented above it. Used
    /// only by the post-parse image-figure promotion: an image + `^ caption`
    /// paragraph promotes to a `<figure>` only when the image sat at the content
    /// column (strict column-0 rule, docs/divergence-from-djot.md §11). An
    /// indented image + caption stays a literal paragraph, matching carve-php /
    /// carve-js. Non-parse construction sites leave this `false` (the default);
    /// none of them build an image + caption paragraph, so it never blocks a
    /// legitimate promotion.
    pub at_content_column: bool,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeBlock {
    pub attrs: Option<Attrs>,
    pub lang: Option<String>,
    pub title: Option<String>,
    pub label: Option<String>,
    pub content: String,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct List {
    pub attrs: Option<Attrs>,
    pub ordered: bool,
    pub start: Option<usize>,
    pub ol_type: Option<OrderedListType>,
    /// Runtime-only authoring flag for ordered lists opened with the bare-dot
    /// marker (`. item`). It stays out of PART 12 JSON until the schema names it.
    pub bare_marker: bool,
    /// Ordered-marker delimiter as authored: `.` or `)`. The marker is
    /// semantic (§11: a sibling with a different delimiter starts a new
    /// list), so the formatter preserves it (carve issue 286). `None`
    /// (programmatic ASTs, bullets) falls back to `.`.
    pub delim: Option<char>,
    /// Bullet character as authored: `-` or `*` (unordered lists only).
    /// Same §11 semantics as `delim`; `None` falls back to `-`.
    pub bullet_char: Option<char>,
    pub tight: bool,
    pub items: Vec<ListItem>,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
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
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockQuote {
    pub attrs: Option<Attrs>,
    pub children: Vec<BlockNode>,
    pub attribution: Option<Vec<InlineNode>>,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Table {
    pub attrs: Option<Attrs>,
    pub caption: Option<Vec<InlineNode>>,
    pub rows: Vec<TableRow>,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableRow {
    pub cells: Vec<TableCell>,
    /// Row-level attributes from a `{...}` block glued to the closing pipe.
    pub attrs: Option<Attrs>,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableCell {
    pub header: bool,
    pub span: Option<TableCellSpan>,
    pub align: Option<TableAlign>,
    /// Author attributes from a `{...}` glued to the cell's opening pipe.
    pub attrs: Option<Attrs>,
    pub children: Vec<InlineNode>,
    /// Where this cell sits in the source (spec PART 12 §4).
    pub pos: Option<Pos>,
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
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Div {
    pub attrs: Option<Attrs>,
    pub label: Option<String>,
    pub children: Vec<BlockNode>,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

/// Line block (section 4.4): a `::: |` fence whose every newline is a hard break.
///
/// Its own type rather than a [`Div`] carrying a `.line-block` class, because
/// the two are not the same document: a plain div with that class keeps soft
/// breaks, and a writer given only the class cannot tell which one to emit. The
/// block vocabulary in the spec's profiles.md lists `line_block` for the same
/// reason - a profile denying it must be able to name it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LineBlock {
    pub attrs: Option<Attrs>,
    pub children: Vec<BlockNode>,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefinitionItem {
    pub terms: Vec<DefinitionTerm>,
    pub definitions: Vec<DefinitionDef>,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefinitionTerm {
    pub attrs: Option<Attrs>,
    pub children: Vec<InlineNode>,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefinitionDef {
    pub attrs: Option<Attrs>,
    pub children: Vec<BlockNode>,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
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
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Figure {
    pub attrs: Option<Attrs>,
    pub target: FigureTarget,
    pub caption: Vec<InlineNode>,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
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
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawBlock {
    pub format: String,
    pub content: String,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comment {
    pub block: bool,
    pub content: String,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
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
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InlineNode {
    Text(Text),
    EscapedText(EscapedText),
    SmartPunctuation(SmartPunctuation),
    Emphasis(Emphasis),
    Code(Code),
    Link(Link),
    Image(Image),
    Span(Span),
    Math(Math),
    RawInline(RawInline),
    LiteralInline(LiteralInline),
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
    SoftBreak(Break),
    HardBreak(Break),
    CriticInsert(CriticInsert),
    CriticDelete(CriticDelete),
    CriticSubstitute(CriticSubstitute),
    CriticComment(CriticComment),
}

impl InlineNode {
    pub fn text(value: impl Into<String>) -> Self {
        Self::Text(Text {
            value: value.into(),
            pos: None,
        })
    }

    pub fn escaped_text(value: impl Into<String>) -> Self {
        Self::EscapedText(EscapedText {
            value: value.into(),
            pos: None,
        })
    }

    pub fn code(value: impl Into<String>, attrs: Option<Attrs>) -> Self {
        Self::Code(Code {
            value: value.into(),
            attrs,
            pos: None,
        })
    }

    pub fn soft_break() -> Self {
        Self::SoftBreak(Break { pos: None })
    }

    pub fn hard_break() -> Self {
        Self::HardBreak(Break { pos: None })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Text {
    pub value: String,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

impl From<String> for Text {
    fn from(value: String) -> Self {
        Self { value, pos: None }
    }
}

impl From<&str> for Text {
    fn from(value: &str) -> Self {
        Self {
            value: value.to_string(),
            pos: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EscapedText {
    /// A character the author escaped with a backslash (`\-`, `\"`).
    ///
    /// Its own variant rather than plain text, because the escape carries
    /// intent the literal character alone cannot: the author wrote `\-\-`
    /// precisely so a downstream processor would NOT turn it into an en dash.
    /// Flattening it into text lost that, and the Markdown target emitted the
    /// trigger bare where carve-php reproduced the escape (carve issue 350).
    /// The inline vocabulary in the spec's profiles.md lists `escaped_text`.
    ///
    /// The value is the literal character, without the backslash.
    pub value: String,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Code {
    pub value: String,
    pub attrs: Option<Attrs>,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Break {
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SmartPunctuation {
    pub kind: String,
    pub value: String,
    pub glyph: Option<String>,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

pub const GLYPHS: &[(&str, &str)] = &[
    ("ellipsis", "…"),
    ("em_dash", "—"),
    ("en_dash", "–"),
    ("left_right_arrow", "↔"),
    ("rightwards_arrow", "→"),
    ("leftwards_arrow", "←"),
    ("rightwards_double_arrow", "⇒"),
    ("less_than_or_equal", "≤"),
    ("greater_than_or_equal", "≥"),
    ("not_equal", "≠"),
    ("plus_minus", "±"),
    ("copyright", "©"),
    ("registered", "®"),
    ("trademark", "™"),
];

pub fn smart_punctuation_glyph(node: &SmartPunctuation) -> &str {
    node.glyph.as_deref().unwrap_or_else(|| {
        GLYPHS
            .iter()
            .find_map(|(kind, glyph)| (*kind == node.kind).then_some(*glyph))
            .unwrap_or(&node.value)
    })
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
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
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
    /// Set when this reference resolved against a HEADING rather than a
    /// `[label]: url` definition (PART 11 R1).
    ///
    /// The canonical writer needs it: a heading-derived reference has no
    /// definition line, so `[H][]` is the only record of the authored form and
    /// writing `[H](#H)` bakes a generated id into the source. An explicit
    /// definition normalizes instead, because its definition line is dropped
    /// either way.
    ///
    /// This used to be carried by "the node still has a ref" - the explicit
    /// branch cleared it, the heading branch did not. PART 12 §3a made both
    /// keep it (carve#597), so the distinction needs a field of its own.
    pub from_heading_reference: bool,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
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
    /// stays an Image and renders from the literal `raw_ref` source. Unlike a
    /// link ref it never matches heading text. `None` for a direct
    /// `![alt](src)` image.
    pub ref_label: Option<String>,
    pub raw_ref: Option<String>,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineExtension {
    pub attrs: Option<Attrs>,
    pub name: String,
    pub children: Vec<InlineNode>,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub attrs: Option<Attrs>,
    pub children: Vec<InlineNode>,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Math {
    pub attrs: Option<Attrs>,
    pub display: bool,
    pub content: String,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawInline {
    pub format: String,
    pub content: String,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

/// Inline literal (`` !`…` ``): a `!` prefix on a verbatim code span (grammar
/// PART 9 §27, `literal_inline = '!', code_span`), mirroring the `$`-math
/// prefix. `content` is captured verbatim by the backtick run exactly as for a
/// code span -- no inline construct is recognized inside it and smart
/// typography does not apply.
///
/// Unlike raw passthrough (§20) it is EMITTED BY EVERY RENDERER and never
/// dropped or target-routed, and its content is HTML-escaped on output. The
/// `<code>` wrapper is dropped: an inline literal is prose, not code. A
/// trailing attribute block is the ordinary inline attribute block and lands in
/// `attrs`, rendered on a `<span>`; with none, the content is emitted as bare
/// text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiteralInline {
    pub content: String,
    pub attrs: Option<Attrs>,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Symbol {
    pub name: String,
    pub attrs: Option<Attrs>,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutoLink {
    pub attrs: Option<Attrs>,
    pub href: String,
    /// Display text = the raw content between `<>`: a URI autolink keeps its
    /// scheme (`<mailto:a@b>` shows `mailto:a@b`), an email shows the address.
    pub text: String,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrossRef {
    /// The raw id between `</#` and `>`, as the author spelled it. Ids resolve
    /// case-insensitively, so this is not necessarily the id it resolved to -
    /// which is why PART 12 section 3a keeps it beside `href` rather than
    /// letting the resolution replace it.
    pub target: String,
    /// The resolved destination (`#` + the heading's id), set where the
    /// crossref resolved against a heading in this document. `None` says it
    /// resolved against nothing, which is what makes it render literally.
    pub href: Option<String>,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptionNumber {
    pub number: Option<usize>,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mention {
    pub attrs: Option<Attrs>,
    pub user: String,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tag {
    pub attrs: Option<Attrs>,
    pub name: String,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CitationGroup {
    pub items: Vec<Citation>,
    pub raw: String,
    pub mode: Option<CitationRenderMode>,
    pub integral: bool,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
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
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Footnote {
    pub attrs: Option<Attrs>,
    pub id: Option<String>,
    pub inline: Option<Vec<InlineNode>>,
    pub number: Option<usize>,
    pub ref_id: Option<String>,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CriticInsert {
    pub children: Vec<InlineNode>,
    pub attrs: Option<Attrs>,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CriticDelete {
    pub children: Vec<InlineNode>,
    pub attrs: Option<Attrs>,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CriticSubstitute {
    pub old_text: String,
    pub new_text: String,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CriticComment {
    pub text: String,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}
