//! Carve AST nodes for parsing, transformation, and rendering.
//!
//! The public JSON representation is encoded separately in `ast_json`.

mod checked;
pub use checked::{AstDepthError, MAX_CHECKED_AST_DEPTH};

use std::collections::BTreeMap;
use std::ops::{Deref, DerefMut};

/// A node's span in the ORIGINAL source (spec PART 12 section 4).
///
/// Lines are 1-based; columns and offsets count UNICODE CODEPOINTS, columns
/// 1-based and offsets 0-based, with `end_column` and `end_offset` exclusive.
/// This said "byte offsets", which is not what the parser records and not what
/// PART 12 §4 pins - `docs/ast-json.md`: "columns and offsets count Unicode
/// codepoints - not bytes, not UTF-16 code units". A consumer that slices a
/// Rust `&str` with one of these PANICS on the first non-ASCII character before
/// it, so the unit is worth stating correctly; `lint::LintWarning` converts to
/// bytes for exactly that reason.
///
/// Recording this is not free here: the parser works on lines whose container
/// prefixes have already been stripped - a blockquote marker, a list indent -
/// so a column in the text the parser sees is not a column in the document.
/// `MappedSource` therefore carries the stripped width per line alongside the
/// line map, and that is what makes the column recoverable.
// NOT `Copy`: the file identity is owned, and a span is cloned explicitly
// rather than duplicated implicitly - which is what a reader wants anyway, since
// copying one used to hide that a position was being reused across nodes.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Pos {
    /// First source line covered by the node, starting at 1.
    pub start_line: usize,
    /// Last source line covered by the node, starting at 1.
    pub end_line: usize,
    /// First column on `start_line`, counted in Unicode codepoints from 1.
    pub start_column: usize,
    /// Exclusive column after the node on `end_line`, counted in Unicode codepoints.
    pub end_column: usize,
    /// First codepoint offset in the original source, starting at 0.
    pub start_offset: usize,
    /// Exclusive codepoint offset after the node in the original source.
    pub end_offset: usize,
    /// Identity of the file these coordinates are measured in, when that is NOT
    /// the document being parsed: the canonical id of the file an include
    /// pulled the node in from (PART 9 §19).
    ///
    /// `None` means the top-level document, so a tree with no includes is
    /// unchanged. Without it an included span is ambiguous - a child's first
    /// paragraph and the parent's first paragraph both report line 1, and a
    /// source-mapped host has no way to tell them apart.
    pub file: Option<SourceFile>,
}

/// The file a span is measured in, shared by every node one include
/// contributed.
///
/// SHARED on purpose, and behind a THIN pointer. `Pos` is embedded in every
/// node, so this field's width multiplies across the whole tree: an owned
/// `String` here put `BlockNode` over the size ceiling that
/// `no_single_variant_sets_the_size_of_every_block_node` exists to hold, which
/// PART 9 §25's nesting cap then multiplies again. One `Arc` per FILE rather
/// than one string per node costs 8 bytes in the node and a refcount bump per
/// clone.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourceFile(std::sync::Arc<String>);

impl SourceFile {
    /// Wrap the identifier used for a file included in a document.
    pub fn new(id: impl Into<String>) -> Self {
        Self(std::sync::Arc::new(id.into()))
    }

    /// Borrow the file identifier without cloning it.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl std::fmt::Display for SourceFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl From<String> for SourceFile {
    fn from(id: String) -> Self {
        Self::new(id)
    }
}

impl From<&str> for SourceFile {
    fn from(id: &str) -> Self {
        Self::new(id)
    }
}

/// Authored attributes and their source order.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Attrs {
    /// Explicit node identifier, when the author supplied one.
    pub id: Option<String>,
    /// Class names in authored order.
    pub classes: Vec<String>,
    /// Named attribute values, indexed by key.
    pub key_values: BTreeMap<String, String>,
    /// Order of the identifier, classes, and named attributes in source.
    pub order: Vec<AttrSlot>,
}

/// An attribute position in [`Attrs::order`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AttrSlot {
    /// The explicit identifier.
    Id,
    /// A class from [`Attrs::classes`].
    Class,
    /// The named value stored under this key in [`Attrs::key_values`].
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

/// A Carve document.
///
/// Teardown drains block and inline trees iteratively while they are owned by
/// this document. Use [`dispose_blocks`] or [`dispose_inlines`] for detached
/// trees that may be deeply nested.
/// Renderers refuse trees past [`crate::MAX_RENDER_DEPTH`].
///
/// Because `Document` implements [`Drop`], its fields cannot be moved out
/// directly. This includes destructuring and struct update syntax. Borrow a
/// field to inspect it, or use [`std::mem::take`] on a mutable document to
/// transfer ownership. Pass a detached deep tree to the corresponding disposal
/// function when it is no longer needed.
///
/// `Clone`, `Debug`, and equality still recurse through child nodes. A test of
/// 16 nested block quotes and emphasis nodes passes on a 256 KiB thread stack
/// in a debug build. The result depends on the shape of the tree, build profile,
/// compiler, and caller's stack. No depth is guaranteed for arbitrary trees.
/// These traits have no depth error and may overflow the stack on deeper trees.
/// Use [`Document::summary`] for bounded diagnostic output and
/// [`Document::try_clone`] / [`Document::try_eq`] for checked operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    /// Parsed frontmatter keys and values when the frontmatter can be represented as a map.
    pub frontmatter: BTreeMap<String, String>,
    /// The frontmatter block as written, when the document has one.
    pub frontmatter_raw: Option<Frontmatter>,
    /// Footnote bodies indexed by their definition labels.
    pub footnote_defs: BTreeMap<String, Vec<BlockNode>>,
    /// Where a footnote definition was WRITTEN, for the definitions whose body
    /// cannot say - keyed by the same label.
    pub footnote_def_pos: BTreeMap<String, Pos>,
    /// Top-level blocks in document order.
    pub children: Vec<BlockNode>,
    /// UTF-8 byte length of the input before source normalization.
    ///
    /// What the DOCUMENT says about itself. On the parse path this crate
    /// measured it; on the ingest path it is `srcByteLength`, read off the wire
    /// exactly as written, because PART 12 §7 makes it a field of the payload
    /// and a reader that rewrites it has silently repaired the record.
    /// Documents built programmatically (not via `parse`) leave this at 0.
    ///
    /// NOT what a cap may be sized from when the document was ingested. See
    /// [`Document::expansion_budget_len`] and [`Document::untrusted_input_len`].
    pub source_len: usize,
    /// Byte length of the JSON payload this document was decoded from, or 0
    /// when it was not decoded from one.
    ///
    /// Set by `from_json` and by nothing else. It never reaches the wire: it is
    /// a fact about how this document ARRIVED rather than about the document,
    /// and re-publishing it would put one reader's measurement where the next
    /// reader would read it back as a claim.
    pub ingest_payload_len: usize,
}

/// A fixed-size view of a document for logging without recursive AST traversal.
///
/// Counts describe root collections, not descendants. Source bytes are the
/// document's recorded value, which may be supplied by an ingested payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct DocumentSummary {
    /// Blocks directly under the document, excluding descendants and footnotes.
    pub root_blocks: usize,
    /// Defined footnote labels, regardless of the size of their bodies.
    pub footnote_definitions: usize,
    /// Parsed frontmatter entries. Raw JSON and TOML frontmatter is not counted.
    pub frontmatter_entries: usize,
    /// Recorded [`Document::source_len`], normally zero for caller-built trees.
    pub source_bytes: usize,
}

impl Document {
    /// Summarize root metadata without reading node content or walking children.
    ///
    /// Formatting this value uses bounded stack and output even when the AST
    /// has arbitrary depth. Use it instead of the recursive `Document` debug
    /// representation when logging caller-built trees.
    pub fn summary(&self) -> DocumentSummary {
        DocumentSummary {
            root_blocks: self.children.len(),
            footnote_definitions: self.footnote_defs.len(),
            frontmatter_entries: self.frontmatter.len(),
            source_bytes: self.source_len,
        }
    }

    /// The length a per-render expansion budget may be sized from.
    ///
    /// The expansion budgets - abbreviations, the table of contents, the index -
    /// are `max(floor, factor * this)`. A cap has to be enforced against
    /// something the attacker does not supply, and on the parse path this is
    /// exactly that: the parser measured the input, so a bigger budget costs a
    /// bigger document.
    ///
    /// On the ingest path `source_len` arrives INSIDE the payload. Left alone it
    /// let the payload choose the size of the guard meant to bound it. An
    /// ingested document is therefore bounded by its payload size as well as by
    /// its claimed source length, and the smaller wins.
    ///
    /// The claim is still honored where it is smaller, because a document that
    /// says it came from a short source is not made suspect by its AST being
    /// verbose - and an encoded tree is larger than the source it came from, so
    /// on an honest round trip this does not bind.
    pub fn expansion_budget_len(&self) -> usize {
        if self.ingest_payload_len == 0 {
            return self.source_len;
        }
        self.source_len.min(self.ingest_payload_len)
    }

    /// The number of untrusted input bytes this document actually cost.
    ///
    /// What a profile's `max_length` bounds. The CLI already measures this
    /// correctly for `--from-json` and says why in `main.rs`: a profile's
    /// `max_length` bounds untrusted input, and on the ingest path the untrusted
    /// input is the payload - it is what gets parsed, held and walked. Unlike
    /// [`Document::expansion_budget_len`] this does not take the smaller of the
    /// two, because a payload that claims to have come from nothing still cost
    /// its own bytes to send.
    pub fn untrusted_input_len(&self) -> usize {
        if self.ingest_payload_len == 0 {
            return self.source_len;
        }
        self.ingest_payload_len
    }
}

// A document owns both block and inline trees. Empty each recursive field before
// its parent is dropped, keeping the pending work on the heap.
impl Drop for Document {
    fn drop(&mut self) {
        let mut pending = Vec::new();
        queue_blocks(&mut pending, std::mem::take(&mut self.children));
        for blocks in self.footnote_defs.values_mut() {
            queue_blocks(&mut pending, std::mem::take(blocks));
        }

        dispose_pending(pending);
    }
}

/// Drop detached block trees without recursion through their child nodes.
///
/// This accepts a single node with `vec![node]` as well as a vector moved out
/// of [`Document::children`] with [`std::mem::take`]. A regular Rust `drop` on
/// a detached tree still recurses through its child nodes.
pub fn dispose_blocks(blocks: Vec<BlockNode>) {
    let mut pending = Vec::new();
    queue_blocks(&mut pending, blocks);
    dispose_pending(pending);
}

/// Drop detached inline trees without recursion through their child nodes.
///
/// Pass a single node as `vec![node]`. A regular Rust `drop` on a detached
/// tree still recurses through its child nodes.
pub fn dispose_inlines(inlines: Vec<InlineNode>) {
    let mut pending = Vec::new();
    queue_inlines(&mut pending, inlines);
    dispose_pending(pending);
}

// Each queued vector is emptied of child vectors in place and then dropped
// shallow, so no node is moved through the work list.
fn dispose_pending(mut pending: Vec<DropWork>) {
    while let Some(work) = pending.pop() {
        match work {
            DropWork::Blocks(mut blocks) => {
                for block in &mut blocks {
                    drain_block(&mut pending, block);
                }
            }
            DropWork::Inlines(mut inlines) => {
                for inline in &mut inlines {
                    drain_inline(&mut pending, inline);
                }
            }
        }
    }
}

enum DropWork {
    Blocks(Vec<BlockNode>),
    Inlines(Vec<InlineNode>),
}

fn queue_blocks(pending: &mut Vec<DropWork>, blocks: Vec<BlockNode>) {
    if !blocks.is_empty() {
        pending.push(DropWork::Blocks(blocks));
    }
}

fn queue_inlines(pending: &mut Vec<DropWork>, inlines: Vec<InlineNode>) {
    if !inlines.is_empty() {
        pending.push(DropWork::Inlines(inlines));
    }
}

fn queue_table(pending: &mut Vec<DropWork>, table: &mut Table) {
    if let Some(caption) = table.caption.take() {
        queue_inlines(pending, caption);
    }
    if let Some(caption) = table.short_caption.take() {
        queue_inlines(pending, caption);
    }
    for row in &mut table.rows {
        for cell in &mut row.cells {
            queue_inlines(pending, std::mem::take(&mut cell.children));
            if let Some(blocks) = cell.blocks.take() {
                queue_blocks(pending, blocks);
            }
        }
    }
}

fn drain_block(pending: &mut Vec<DropWork>, block: &mut BlockNode) {
    match block {
        BlockNode::Heading(node) => queue_inlines(pending, std::mem::take(&mut node.children)),
        BlockNode::Paragraph(node) => queue_inlines(pending, std::mem::take(&mut node.children)),
        BlockNode::List(node) => {
            for item in &mut node.items {
                queue_blocks(pending, std::mem::take(&mut item.children));
            }
        }
        BlockNode::BlockQuote(node) => queue_blocks(pending, std::mem::take(&mut node.children)),
        BlockNode::Table(node) => queue_table(pending, node),
        BlockNode::Admonition(node) => {
            if let Some(title) = node.title.take() {
                queue_inlines(pending, title);
            }
            queue_blocks(pending, std::mem::take(&mut node.children));
        }
        BlockNode::Directive(node) => {
            if let Some(title) = node.title.take() {
                queue_inlines(pending, title);
            }
            queue_blocks(pending, std::mem::take(&mut node.children));
        }
        BlockNode::Div(node) => queue_blocks(pending, std::mem::take(&mut node.children)),
        BlockNode::Section(node) => queue_blocks(pending, std::mem::take(&mut node.children)),
        BlockNode::LineBlock(node) => queue_blocks(pending, std::mem::take(&mut node.children)),
        BlockNode::DefinitionList(node) => {
            for item in &mut node.items {
                for term in &mut item.terms {
                    queue_inlines(pending, std::mem::take(&mut term.children));
                }
                for definition in &mut item.definitions {
                    queue_blocks(pending, std::mem::take(&mut definition.children));
                }
            }
        }
        BlockNode::Figure(node) => {
            queue_inlines(pending, std::mem::take(&mut node.caption));
            if let Some(caption) = node.short_caption.take() {
                queue_inlines(pending, caption);
            }
            match node.target.as_mut() {
                FigureTarget::BlockQuote(target) => {
                    queue_blocks(pending, std::mem::take(&mut target.children));
                }
                FigureTarget::Table(target) => queue_table(pending, target),
                FigureTarget::Paragraph(target) => {
                    queue_inlines(pending, std::mem::take(&mut target.children));
                }
                FigureTarget::Image(_) | FigureTarget::CodeBlock(_) => {}
            }
        }
        BlockNode::FigureGroup(node) => {
            queue_blocks(pending, std::mem::take(&mut node.children));
            if let Some(caption) = node.caption.take() {
                queue_inlines(pending, caption);
            }
        }
        BlockNode::CitationDefinition(node) => {
            queue_inlines(pending, std::mem::take(&mut node.children));
        }
        BlockNode::BlockExtension(node) => {
            let fallback = std::mem::replace(
                node.fallback.as_mut(),
                BlockNode::ThematicBreak(ThematicBreak::default()),
            );
            pending.push(DropWork::Blocks(vec![fallback]));
        }
        BlockNode::ExtensionCarrier(node) => {
            queue_blocks(pending, std::mem::take(&mut node.children));
            if let Some(summary) = node.summary.take() {
                queue_inlines(pending, summary);
            }
        }
        BlockNode::CodeBlock(_)
        | BlockNode::AbbreviationDef(_)
        | BlockNode::LinkReferenceDefinition(_)
        | BlockNode::RawBlock(_)
        | BlockNode::Comment(_)
        | BlockNode::BlockImage(_)
        | BlockNode::ThematicBreak(_) => {}
    }
}

fn drain_inline(pending: &mut Vec<DropWork>, inline: &mut InlineNode) {
    match inline {
        InlineNode::Emphasis(node) => queue_inlines(pending, std::mem::take(&mut node.children)),
        InlineNode::Link(node) => queue_inlines(pending, std::mem::take(&mut node.children)),
        InlineNode::Span(node) => queue_inlines(pending, std::mem::take(&mut node.children)),
        InlineNode::Extension(node) => queue_inlines(pending, std::mem::take(&mut node.children)),
        InlineNode::Ruby(node) => {
            for pair in &mut node.pairs {
                queue_inlines(pending, std::mem::take(&mut pair.base));
                queue_inlines(pending, std::mem::take(&mut pair.annotation));
            }
        }
        InlineNode::CitationGroup(node) => {
            for item in &mut node.items {
                for field in [&mut item.prefix, &mut item.locator, &mut item.suffix] {
                    if let Some(inlines) = field.take() {
                        queue_inlines(pending, inlines);
                    }
                }
            }
        }
        InlineNode::Footnote(node) => {
            if let Some(inlines) = node.inline.take() {
                queue_inlines(pending, inlines);
            }
        }
        InlineNode::CriticInsert(node) => {
            queue_inlines(pending, std::mem::take(&mut node.children))
        }
        InlineNode::CriticDelete(node) => {
            queue_inlines(pending, std::mem::take(&mut node.children))
        }
        InlineNode::CriticSubstitute(node) => {
            queue_inlines(pending, std::mem::take(&mut node.old));
            queue_inlines(pending, std::mem::take(&mut node.new));
        }
        InlineNode::Text(_)
        | InlineNode::EscapedText(_)
        | InlineNode::SmartPunctuation(_)
        | InlineNode::Code(_)
        | InlineNode::Image(_)
        | InlineNode::Math(_)
        | InlineNode::RawInline(_)
        | InlineNode::LiteralInline(_)
        | InlineNode::Symbol(_)
        | InlineNode::AutoLink(_)
        | InlineNode::CrossRef(_)
        | InlineNode::CaptionNumber(_)
        | InlineNode::Mention(_)
        | InlineNode::Tag(_)
        | InlineNode::Abbreviation(_)
        | InlineNode::NonBreakingSpace(_)
        | InlineNode::SoftBreak(_)
        | InlineNode::CellWrap(_)
        | InlineNode::HardBreak(_)
        | InlineNode::CriticComment(_)
        | InlineNode::Comment(_) => {}
    }
}

/// A block in the document tree.
///
/// Use [`dispose_blocks`] for a detached tree with unbounded nesting. Derived
/// `Clone`, `Debug`, and equality recurse through child nodes; see [`Document`]
/// for the measured small-stack behavior.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockNode {
    /// A heading with a level and inline content.
    Heading(Heading),
    /// A paragraph of inline content.
    Paragraph(Paragraph),
    /// A fenced code block.
    CodeBlock(CodeBlock),
    /// An ordered, unordered, or task list.
    List(List),
    /// Blocks inside a quoted container.
    BlockQuote(BlockQuote),
    /// A table and its rows, cells, and caption metadata.
    Table(Table),
    /// A typed admonition container.
    Admonition(Admonition),
    /// A named container whose kind names GENERATED CONTENT (CARVE-P12-057).
    /// Everything else named is an [`BlockNode::Admonition`]; anonymous or
    /// attribute-only is a [`BlockNode::Div`].
    Directive(Directive),
    /// An anonymous or attribute-only block container.
    Div(Div),
    /// An explicit section from an interchange document. Source sections are implicit.
    Section(Section),
    /// Lines grouped as a line block.
    LineBlock(LineBlock),
    /// Terms and descriptions in a definition list.
    DefinitionList(DefinitionList),
    /// A figure with a target and optional caption.
    Figure(Figure),
    /// Figures grouped under one container.
    FigureGroup(FigureGroup),
    /// An abbreviation definition collected from source.
    AbbreviationDef(AbbreviationDef),
    /// A link reference definition collected from source.
    LinkReferenceDefinition(LinkReferenceDefinition),
    /// A citation definition collected from source.
    CitationDefinition(CitationDefinition),
    /// A raw block with an output format.
    RawBlock(RawBlock),
    /// A block comment preserved in the AST.
    Comment(Comment),
    /// A block construct an extension owns, carried with a core FALLBACK
    /// (CARVE-P12-055). Interchange-only: Carve 0.1 source spells none, so a
    /// parse never produces one.
    BlockExtension(BlockExtension),
    /// The render-stage carrier a `before_render` rewrite leaves behind. NOT the
    /// schema's `block_extension`, which is [`BlockNode::BlockExtension`].
    ExtensionCarrier(ExtensionCarrier),
    /// An image placed at block level.
    BlockImage(Image),
    /// A thematic separator.
    ThematicBreak(ThematicBreak),
}

/// A horizontal separator authored with a run of marker characters.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ThematicBreak {
    /// The thematic-break character as authored. Absence defaults to `-`.
    pub marker: Option<char>,
    /// Attributes from a preceding block-attribute line (`{.x}` then `---`).
    pub attrs: Option<Attrs>,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

/// A heading and its inline content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heading {
    /// Attributes attached to the heading.
    pub attrs: Option<Attrs>,
    /// Heading depth, from 1 through 6.
    pub level: u8,
    /// Inline content of the heading.
    pub children: Vec<InlineNode>,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

/// A paragraph of inline content.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Paragraph {
    /// Attributes attached to the paragraph.
    pub attrs: Option<Attrs>,
    /// Inline content of the paragraph.
    pub children: Vec<InlineNode>,
    /// Whether the paragraph's first source line began at its container's content
    /// column (column 0 at the top level), i.e. was NOT indented above it. Used
    /// only by the post-parse image-figure promotion: an image + `^ caption`
    /// paragraph promotes to a `<figure>` only when the image sat at the content
    /// column (strict column-0 rule, docs/divergence-from-djot.md §11). An
    /// indented image + caption stays a literal paragraph, matching carve-php /
    /// carve-js.
    ///
    /// EVERY construction site must set this deliberately. The claim that used to
    /// stand here - that the sites leaving it `false` never build an image +
    /// caption paragraph - was untrue: the list item's LEAD paragraph is built by
    /// hand in `parse_list`, left it at the default, and so blocked the promotion
    /// for every list item in every document (carve-rs#610). Prefer naming all
    /// four fields over `..Default::default()` at a parse site, so a paragraph
    /// that begins at its content column cannot silently claim otherwise.
    pub at_content_column: bool,
    /// Set after reference resolution when the paragraph contains one resolved image.
    /// On the wire it appears only as `true`; absence means the producer skipped
    /// the phase, not that the paragraph is ordinary.
    pub block_image: bool,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

/// A fenced code block with an optional information string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeBlock {
    /// Attributes attached to the block.
    pub attrs: Option<Attrs>,
    /// Language name from the fence opener.
    pub lang: Option<String>,
    /// Optional title from the fence opener.
    pub title: Option<String>,
    /// Optional label from the fence opener.
    pub label: Option<String>,
    /// Literal payload text, including each authored line ending.
    /// Empty fences contain no characters; EOF can leave the last line unterminated.
    pub content: String,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

/// Whether a nonempty payload needs a line ending before a closing fence.
pub fn fenced_payload_needs_ending(content: &str) -> bool {
    !content.is_empty() && !content.ends_with('\n')
}

/// An ordered, unordered, or task list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct List {
    /// Attributes attached to the list.
    pub attrs: Option<Attrs>,
    /// Whether the list uses ordered markers.
    pub ordered: bool,
    /// First ordinal of an ordered list, when specified.
    pub start: Option<usize>,
    /// Letter or Roman numeral style of an ordered list.
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
    /// Whether item paragraphs render without `<p>` wrappers.
    pub tight: bool,
    /// Items in source order.
    pub items: Vec<ListItem>,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

/// Letter or Roman numeral style for an ordered list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderedListType {
    /// Lowercase letters.
    LowerAlpha,
    /// Uppercase letters.
    UpperAlpha,
    /// Lowercase Roman numerals.
    LowerRoman,
    /// Uppercase Roman numerals.
    UpperRoman,
}

/// One list marker and the blocks that belong to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListItem {
    /// Attributes attached to the item.
    pub attrs: Option<Attrs>,
    /// `None` for plain bullets; `Some(checked)` for task-list items.
    pub checked: Option<bool>,
    /// Marker character as authored, when it is not the default for `checked`.
    /// Same author-choice role as `List::bullet_char`; `X` folds to `x`.
    pub task_state: Option<char>,
    /// Blocks inside the item.
    pub children: Vec<BlockNode>,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

/// Quoted blocks, either line-prefixed or fenced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockQuote {
    /// Attributes attached to the quote.
    pub attrs: Option<Attrs>,
    /// Blocks inside the quote.
    pub children: Vec<BlockNode>,
    /// True when the quote was authored as a colon-fence container rather than
    /// with line markers. The two spellings are one node; this records which,
    /// so the canonical writer writes back what it read. False on a prefixed
    /// quote, so a document that predates the fence serializes exactly as it
    /// did (markup-carve/carve#1718).
    pub fenced: bool,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

/// A table, its rows, and optional publishing metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Table {
    /// Attributes attached to the table.
    pub attrs: Option<Attrs>,
    /// Source format when the table came from a delimited-data fence.
    pub source_format: Option<TableSourceFormat>,
    /// Inline caption shown with the table.
    pub caption: Option<Vec<InlineNode>>,
    /// Structured publishing/navigation label; Carve 0.1 source has no spelling.
    pub short_caption: Option<Vec<InlineNode>>,
    /// Per-column publishing metadata, resolved from positional table attrs.
    pub columns: Vec<TableColumn>,
    /// Rows in source order.
    pub rows: Vec<TableRow>,
    /// An explicit head/body/foot partition of `rows`, imported from or destined
    /// for a format whose table model has one. Carve 0.1 source has no spelling
    /// for it, so a parse never sets it.
    pub row_groups: Option<Box<TableRowGroups>>,
    /// The dash runs the author's separator row spelled, one per separator
    /// cell, alignment colons excluded (PART 12 §3). `None` means the author
    /// wrote NO separator row, which is the half of the distinction the rows
    /// cannot carry: `|= a |` and a plain row over `|---|` parse alike.
    /// Author choice on the same terms as `List::bullet_char`.
    pub separator_widths: Option<Vec<u64>>,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

/// Delimited-data source format for a table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum TableSourceFormat {
    /// Comma-separated values.
    Csv,
    /// Tab-separated values.
    Tsv,
}

impl TableSourceFormat {
    /// Returns the name used in fences and AST JSON.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Csv => "csv",
            Self::Tsv => "tsv",
        }
    }

    /// Parses a CSV or TSV source format name.
    pub fn from_name(value: &str) -> Option<Self> {
        match value {
            "csv" => Some(Self::Csv),
            "tsv" => Some(Self::Tsv),
            _ => None,
        }
    }

    pub(crate) const fn separator(self) -> char {
        match self {
            Self::Csv => ',',
            Self::Tsv => '\t',
        }
    }
}

/// An explicit head/body/foot partition of a table's rows (PART 12 §15).
///
/// It holds COUNTS, never rows: they consume `rows` in order, head first, then
/// each body, then the foot, and they MUST account for every row exactly once.
/// Absent means the implicit structure every renderer already derives - the
/// leading run of header rows as the head, everything after it as one body, no
/// foot, no row-head columns. HTML renders explicit sections; text targets
/// retain row order and report discarded section attributes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableRowGroups {
    /// Rows at the start of `rows` forming the table head.
    pub head_rows: usize,
    /// The body groups, in order, each consuming the next
    /// `head_rows + body_rows` rows.
    pub bodies: Vec<TableBodyGroup>,
    /// Rows at the end of `rows` forming the table foot.
    pub foot_rows: usize,
    /// Attributes attached to the head section.
    pub head_attrs: Option<Attrs>,
    /// Attributes attached to the foot section.
    pub foot_attrs: Option<Attrs>,
}

/// One body section in an explicit table partition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableBodyGroup {
    /// Rows at the start of this group forming its intermediate header.
    pub head_rows: usize,
    /// Rows in this group after its intermediate header.
    pub body_rows: usize,
    /// Leading columns of this group's rows that are row headers. `None` means
    /// zero. It sits on the group rather than on the table because that is where
    /// the exchanged model puts it.
    pub row_head_columns: Option<usize>,
    /// Attributes attached to this body section.
    pub attrs: Option<Attrs>,
}

/// A table row and its cells.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableRow {
    /// Cells in source order.
    pub cells: Vec<TableCell>,
    /// Row-level attributes from a `{...}` block glued to the closing pipe.
    pub attrs: Option<Attrs>,
    /// The number of `+` continuation lines the row has, absent when none.
    pub continuation_lines: Option<usize>,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

/// A table cell with inline or interchange-only block content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableCell {
    /// Whether the cell is a column header.
    pub header: bool,
    /// Source marker for a row or column span.
    pub span: Option<TableCellSpan>,
    /// Resolved number of columns this cell occupies (spec PART 12 §26).
    /// `None` means 1. Published beside `span`, never instead of it.
    pub colspan: Option<usize>,
    /// Resolved number of rows this cell occupies (spec PART 12 §26).
    /// `None` means 1.
    pub rowspan: Option<usize>,
    /// Horizontal alignment, if specified.
    pub align: Option<TableAlign>,
    /// Vertical alignment, if specified.
    pub valign: Option<TableVerticalAlign>,
    /// Author attributes from a `{...}` glued to the cell's opening pipe.
    pub attrs: Option<Attrs>,
    /// Inline cell content.
    pub children: Vec<InlineNode>,
    /// Interchange-only block content, in place of inline children.
    pub blocks: Option<Vec<BlockNode>>,
    /// Where this cell sits in the source (spec PART 12 §4).
    pub pos: Option<Pos>,
}

/// Direction of a table cell span marker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableCellSpan {
    /// Continue a cell from the row above.
    Rowspan,
    /// Continue a cell from the preceding column.
    Colspan,
}

/// Horizontal alignment of a table column or cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableAlign {
    /// Left aligned.
    Left,
    /// Right aligned.
    Right,
    /// Centered.
    Center,
}

/// Resolved publishing metadata for a table column.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TableColumn {
    /// Horizontal alignment, if specified.
    pub align: Option<TableAlign>,
    /// Vertical alignment, if specified.
    pub valign: Option<TableVerticalAlign>,
    /// Fraction of the table width, in `(0, 1]`.
    pub width: Option<f64>,
}

// Widths are finite values validated at the AST boundary.
impl Eq for TableColumn {}

/// Vertical alignment of a table column or cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableVerticalAlign {
    /// Top aligned.
    Top,
    /// Centered vertically.
    Middle,
    /// Bottom aligned.
    Bottom,
}

/// A named callout container.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Admonition {
    /// Attributes attached to the container.
    pub attrs: Option<Attrs>,
    /// Name on the container opener.
    pub kind: String,
    /// Inline title from the opener.
    pub title: Option<Vec<InlineNode>>,
    /// Optional label from the opener.
    pub label: Option<String>,
    /// Blocks inside the container.
    pub children: Vec<BlockNode>,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

/// An unnamed block container.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Div {
    /// Attributes attached to the container.
    pub attrs: Option<Attrs>,
    /// Optional label from the opener.
    pub label: Option<String>,
    /// Blocks inside the container.
    pub children: Vec<BlockNode>,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

/// A section wrapper with an optional heading level.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    /// Attributes attached to the section.
    pub attrs: Option<Attrs>,
    /// Heading level associated with the section, if known.
    pub level: Option<u8>,
    /// Blocks inside the section.
    pub children: Vec<BlockNode>,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

/// The six kinds a `:::` container names to PLACE generated content
/// (CARVE-P12-057). A named container carrying one of these is a [`Directive`];
/// every other named container is an [`Admonition`].
///
/// The list is CLOSED. The clause names six kinds and rules that "every other
/// named container is an `admonition`", so a seventh generated-looking word -
/// `endnotes`, `contents`, `bibliographies` - is an admonition. The schema's
/// `directive.kind` enum carries the same six.
pub const GENERATED_CONTENT_KINDS: &[&str] = &[
    "bibliography",
    "footnotes",
    "glossary",
    "index",
    "references",
    "toc",
];

/// Whether `kind` names generated content, and so opens a [`Directive`] rather
/// than an [`Admonition`].
pub fn is_generated_content_kind(kind: &str) -> bool {
    GENERATED_CONTENT_KINDS.contains(&kind)
}

/// Generated-content placement (CARVE-P12-057): a named `:::` container whose
/// kind asks the renderer to place something the document already holds, rather
/// than to present the blocks written inside it.
///
/// Its own type rather than an [`Admonition`] carrying one of six kinds, because
/// a consumer dispatching on the node type to decide whether to draw a callout
/// otherwise has to carry the not-a-callout list itself.
///
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Directive {
    /// Attributes attached to the directive.
    pub attrs: Option<Attrs>,
    /// One of [`GENERATED_CONTENT_KINDS`].
    pub kind: String,
    /// Inline content of the opener's quoted title, like [`Admonition::title`].
    pub title: Option<Vec<InlineNode>>,
    /// Opener `[label]` grouping id. Inert in core, like [`Admonition::label`].
    pub label: Option<String>,
    /// Blocks written inside the opener. Usually empty: the content is
    /// generated, not authored.
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
    /// Attributes attached to the line block.
    pub attrs: Option<Attrs>,
    /// Blocks containing the line content.
    pub children: Vec<BlockNode>,
    /// Optional line-end pointers for each stanza, relative to its child block.
    pub lines: Option<Vec<Vec<String>>>,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

/// A definition-list entry with one or more terms and descriptions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefinitionItem {
    /// Terms for this entry.
    pub terms: Vec<DefinitionTerm>,
    /// Descriptions for this entry.
    pub definitions: Vec<DefinitionDef>,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

/// A term in a definition-list entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefinitionTerm {
    /// Attributes attached to the term.
    pub attrs: Option<Attrs>,
    /// Inline term content.
    pub children: Vec<InlineNode>,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

/// A description in a definition-list entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefinitionDef {
    /// Attributes attached to the description.
    pub attrs: Option<Attrs>,
    /// Blocks in the description.
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
/// A definition list containing term and description entries.
pub struct DefinitionList {
    /// Attributes attached to this node.
    pub attrs: Option<Attrs>,
    /// Term and description entries in source order.
    pub items: Vec<DefinitionItem>,
    /// PART 9 §17 L7: the looseness was SPELLED, with the consumed `loose`
    /// boolean on the preceding block-attribute line, so every description
    /// renders its children as BLOCKS rather than as an inline run.
    ///
    /// It reaches the one shape a blank line cannot say. A blank line between
    /// two ENTRIES does not loosen a `<dl>` at all - only a second block inside
    /// the description wraps it - so `<dd><p>x</p></dd>` is unspellable at every
    /// entry count.
    ///
    /// UNLIKE `List::tight` THIS FIELD IS NOT TOTAL, and PART 12 §8 publishes it
    /// only when true for that reason: absent means each description derives its
    /// own wrapper from its block count, which is what every other definition
    /// list does. Only the spelled fact is underivable, so only it is published
    /// (markup-carve/carve#1624).
    pub loose: bool,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// A captioned block target.
pub struct Figure {
    /// Attributes attached to this node.
    pub attrs: Option<Attrs>,
    /// BOXED to keep `BlockNode` small. `FigureTarget` embeds a whole `Table`
    /// or `CodeBlock`, which made `Figure` the largest variant by far and so
    /// set the size of EVERY `BlockNode`: 472 bytes, against 272 for the next
    /// largest. Every recursive walk over the tree moves `BlockNode` values by
    /// value, so that number is what a nesting level costs in stack, and PART 9
    /// §25's cap of 200 levels multiplied it into most of a 1 MiB wasm stack
    /// (markup-carve/carve-wasm#44). One indirection on a node kind that is
    /// rare in real documents buys the cap back.
    pub target: Box<FigureTarget>,
    /// HTML an extension rendered FOR `target`, replacing it on the HTML path
    /// only (`None` in every parsed document).
    ///
    /// A `before_render` transform claims a fence by swapping the `CodeBlock`
    /// for a `RawBlock` - and a CAPTIONED fence is not a block in a list, it is
    /// this node's `target`, which PART 12 pins to five types with no raw-HTML
    /// spelling among them. Carrying the replacement beside the target rather
    /// than inside it keeps the wire shape exactly as the schema pins it (the
    /// encoder does not write this field) while letting `chart`, `mermaid` and
    /// the other presets reach a captioned diagram, which is the shape a figure
    /// most often IS in a technical document (markup-carve/carve-rs#1151).
    ///
    /// BOXED for the reason `target` is: a `RawBlock` inline here adds its two
    /// `String`s to every `Figure`, and `Figure` is a `BlockNode` variant, so it
    /// would set the size of EVERY node and multiply through §25's 200-level
    /// cap. Unboxed this took `BlockNode` from 272 to 312 bytes, over the guard
    /// in tests/no_single_variant_sets_the_size_of_every_block_node.rs.
    pub rendered_target: Option<Box<RawBlock>>,
    /// Inline caption shown with the target.
    pub caption: Vec<InlineNode>,
    /// Structured publishing/navigation label; ordinary renderers ignore it.
    pub short_caption: Option<Vec<InlineNode>>,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Block content that a figure captions.
pub enum FigureTarget {
    /// An image target.
    Image(Image),
    /// A quoted block target.
    BlockQuote(BlockQuote),
    /// A table target.
    Table(Table),
    /// A fenced code target.
    CodeBlock(CodeBlock),
    /// A paragraph target.
    Paragraph(Paragraph),
}

/// A composite figure: one figure-numbering unit holding ordered panels
/// (PART 9 §4c, a bare `::: figure` container).
///
/// `children` are the body's blocks in source order; the PANELS are the
/// `Figure` and `Table` nodes among them, derived by type rather than stored
/// in a second list, and stray non-panel content is preserved in place.
/// Discriminated from `Figure` by the node TYPE: every `Figure` carries a
/// `target`, this node deliberately does not, and it has no title, label or
/// short-caption slot - its one authored metadata channel is the group
/// caption on the closing fence (carve#1118/carve#1121 own the rest).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FigureGroup {
    /// Attributes attached to this node.
    pub attrs: Option<Attrs>,
    /// Blocks inside the group, including its panels.
    pub children: Vec<BlockNode>,
    /// The group caption (the `^ ` line after the closing fence). `None`
    /// means the group is uncaptioned - never an empty placeholder.
    pub caption: Option<Vec<InlineNode>>,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// An abbreviation definition hoisted to document scope.
pub struct AbbreviationDef {
    /// Abbreviated spelling.
    pub abbr: String,
    /// Expanded text for the abbreviation.
    pub expansion: String,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

/// The `[label]: /url "title" {attrs}` definition line.
///
/// PART 12 §10 (NORMATIVE): a link reference definition is a NODE, so a writer can
/// reproduce it. Before it had one, the canonical writer had nowhere to write the
/// definition back from and INLINED every resolved reference instead - which lost
/// `ref`/`raw_ref` on the reparse and duplicated one destination into N
/// (carve-rs#631, carve#642).
///
/// It HOISTS to the document exactly as §7 requires of the other definition kinds:
/// a definition authored inside a block quote or list item is a child of the
/// document, and `pos` still says where it was written.
///
/// Renders nothing itself; it feeds every link or image that resolves the label
/// (PART 9R R1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkReferenceDefinition {
    /// The label as the AUTHOR wrote it, before any folding.
    pub label: String,
    /// Present even when the authored definition has an empty destination.
    pub href: String,
    /// Optional title from the definition.
    pub title: Option<String>,
    /// A trailing attribute block on the definition line (§15 A2b).
    pub attrs: Option<Attrs>,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

/// A `[@key]: {author= year=} entry` bibliography line (PART 12 §18).
///
/// Shaped after [`LinkReferenceDefinition`] rather than after the footnote,
/// which is the closer of the two analogues: a footnote body holds BLOCKS,
/// while a citation definition holds a metadata run plus one line of rendered
/// text. So the entry is `children` of INLINE nodes and the metadata lands in
/// `attrs`, with `key` where the link kind has `label` - `citation.key`
/// already names the same string at the use site.
///
/// Renders nothing where it sits; the entry's text renders in the references
/// list the Citations extension builds. Tier-2: with the extension off,
/// `[@key]: entry` is ordinary paragraph text, so a default-profile parse
/// never produces this type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CitationDefinition {
    /// The citation key as the author wrote it, without the `@`.
    pub key: String,
    /// The entry's inline content: what follows the `]: ` separator and the
    /// optional metadata block. May be empty.
    pub children: Vec<InlineNode>,
    /// The leading `{author= year=}` metadata block, when the definition
    /// carries one.
    pub attrs: Option<Attrs>,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Raw block content routed to a named output format.
pub struct RawBlock {
    /// Attributes attached to this node.
    pub attrs: Option<Attrs>,
    /// Target format for the raw content.
    pub format: String,
    /// Literal payload text, including each authored line ending.
    pub content: String,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

impl RawBlock {
    // Target text omits the payload terminator but retains all-blank lines.
    pub(crate) fn target_content(&self) -> &str {
        if self.content.bytes().all(|byte| byte == b'\n') {
            &self.content
        } else {
            self.content.strip_suffix('\n').unwrap_or(&self.content)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// A block or inline comment preserved in the AST.
pub struct Comment {
    /// Whether the comment uses a fenced, multiline form.
    pub block: bool,
    /// Whether the comment uses the inline `{% ... %}` form.
    pub delimited: bool,
    /// Literal content.
    pub content: String,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

/// The schema's `block_extension` (CARVE-P12-055): a block an extension owns,
/// carried with a core `fallback` so a reader that does not implement the
/// extension still has something real to render, write and count.
///
/// Interchange-only. Carve 0.1 source spells no block extension, so a parse
/// never produces one; a tree reaches this shape from a format bridge, an
/// importer or an editing API. Distinct from [`ExtensionCarrier`], which is this
/// crate's own render-stage rewrite artifact and is NOT this wire type - the two
/// shared the name `block_extension` and the engine could read back neither
/// (carve-rs#1865).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockExtension {
    /// Globally qualified, so two extensions cannot collide:
    /// `org.example.diagram`, not `diagram`.
    pub name: String,
    /// The extension's own version, opaque here.
    pub version: Option<String>,
    /// REQUIRED. The core block a reader that does not implement this extension
    /// renders, writes and counts instead. Every walk that reaches children
    /// reaches this, so the renderers need no per-target special case.
    pub fallback: Box<BlockNode>,
    /// Opaque extension data, if supplied.
    pub payload: Option<ExtensionPayload>,
    /// Attributes attached to this node.
    pub attrs: Option<Attrs>,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

/// A [`BlockExtension`]'s own data. NOT Carve content, and never rendered by a
/// core target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtensionPayload {
    /// A media type naming how `value` is encoded, so a reader can tell whether
    /// it can parse it at all. REQUIRED: a payload without one is refused.
    pub format: String,
    /// The extension's own data as JSON text, held verbatim and opaque. A
    /// `serde_json::Value` here would cost [`BlockNode`]'s `Eq`, and nothing in
    /// this crate reads inside a payload: the schema calls it opaque and no core
    /// renderer touches it.
    pub value: Option<String>,
}

impl BlockExtension {
    /// The fallback as a one-element slice, for the walks that take a block
    /// list. The fallback IS this node's children, so a walk that reaches it
    /// needs no knowledge that this type exists.
    pub fn fallback_slice(&self) -> &[BlockNode] {
        std::slice::from_ref(&*self.fallback)
    }

    /// [`BlockExtension::fallback_slice`], mutably.
    pub fn fallback_slice_mut(&mut self) -> &mut [BlockNode] {
        std::slice::from_mut(&mut *self.fallback)
    }
}

/// The render-stage carrier a `before_render` rewrite leaves behind, so an
/// extension's own renderer gets its content back.
///
/// NOT the schema's `block_extension` - that is [`BlockExtension`]. This node
/// never reaches the wire under its own identity: the canonical writer degrades
/// it to its children the way every non-HTML renderer already does, and the AST
/// writer publishes it as a `div` (carve-rs#1865).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtensionCarrier {
    /// Attributes attached to this node.
    pub attrs: Option<Attrs>,
    /// Name of the extension that owns this carrier.
    pub name: String,
    /// Blocks inside this node.
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

/// An inline node in the document tree.
///
/// Use [`dispose_inlines`] for a detached tree with unbounded nesting. Derived
/// `Clone`, `Debug`, and equality recurse through child nodes; see [`Document`]
/// for the measured small-stack behavior.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InlineNode {
    /// Ordinary authored text.
    Text(Text),
    /// Text whose source used an escape.
    EscapedText(EscapedText),
    /// A punctuation substitution with its source form.
    SmartPunctuation(SmartPunctuation),
    /// Emphasis, strong text, or another emphasis kind.
    Emphasis(Emphasis),
    /// Inline code.
    Code(Code),
    /// A link with inline label content.
    Link(Link),
    /// An image with alternative text.
    Image(Image),
    /// An attributed inline container.
    Span(Span),
    /// Ruby annotations for base text.
    Ruby(Ruby),
    /// Inline mathematics.
    Math(Math),
    /// Raw inline content routed to an output format.
    RawInline(RawInline),
    /// Verbatim prose written with the `!`-prefixed code-span syntax.
    LiteralInline(LiteralInline),
    /// A named symbol.
    Symbol(Symbol),
    /// An automatically recognized link.
    AutoLink(AutoLink),
    /// A reference to a heading, figure, or other target.
    CrossRef(CrossRef),
    /// A generated caption number.
    CaptionNumber(CaptionNumber),
    /// A mention node.
    Mention(Mention),
    /// A tag node.
    Tag(Tag),
    /// One or more citations.
    CitationGroup(CitationGroup),
    /// Content owned by an inline extension.
    Extension(InlineExtension),
    /// An abbreviation use.
    Abbreviation(Abbreviation),
    /// A footnote reference or inline footnote.
    Footnote(Footnote),
    /// A space that renderers must keep unbroken.
    NonBreakingSpace(NonBreakingSpace),
    /// A source line ending represented as a soft break.
    SoftBreak(Break),
    /// Where a `+` continuation row joins content onto a table cell
    /// (CARVE-P12-069). Renders as one space.
    CellWrap(CellWrap),
    /// A source line ending represented as a hard break.
    HardBreak(Break),
    /// Inserted text in editorial markup.
    CriticInsert(CriticInsert),
    /// Deleted text in editorial markup.
    CriticDelete(CriticDelete),
    /// Replacement text in editorial markup.
    CriticSubstitute(CriticSubstitute),
    /// An editorial comment.
    CriticComment(CriticComment),
    /// A trailing line comment inside a paragraph (`text %% note`). It renders
    /// to nothing on every target but the canonical Carve writer, and PART 12
    /// publishes it: a tree that records what the author wrote cannot drop it
    /// (carve-rs#513). Shares `Comment` with the block form, which carries
    /// `block: false` here.
    Comment(Comment),
}

impl InlineNode {
    /// The node's span in the original source, when the parser could determine
    /// one. EXHAUSTIVE on purpose: a `_ => None` arm here would silently answer
    /// "unpositioned" for a variant added later, and callers derive a
    /// block's `pos` from the first and last positioned node on a line.
    pub(crate) fn pos(&self) -> Option<&Pos> {
        match self {
            Self::Text(n) => n.pos.as_ref(),
            Self::EscapedText(n) => n.pos.as_ref(),
            Self::SmartPunctuation(n) => n.pos.as_ref(),
            Self::Emphasis(n) => n.pos.as_ref(),
            Self::Code(n) => n.pos.as_ref(),
            Self::Link(n) => n.pos.as_ref(),
            Self::Image(n) => n.pos.as_ref(),
            Self::Span(n) => n.pos.as_ref(),
            Self::Ruby(n) => n.pos.as_ref(),
            Self::Math(n) => n.pos.as_ref(),
            Self::RawInline(n) => n.pos.as_ref(),
            Self::LiteralInline(n) => n.pos.as_ref(),
            Self::Symbol(n) => n.pos.as_ref(),
            Self::AutoLink(n) => n.pos.as_ref(),
            Self::CrossRef(n) => n.pos.as_ref(),
            Self::CaptionNumber(n) => n.pos.as_ref(),
            Self::Mention(n) => n.pos.as_ref(),
            Self::Tag(n) => n.pos.as_ref(),
            Self::CitationGroup(n) => n.pos.as_ref(),
            Self::Extension(n) => n.pos.as_ref(),
            Self::Abbreviation(n) => n.pos.as_ref(),
            Self::Footnote(n) => n.pos.as_ref(),
            Self::NonBreakingSpace(n) => n.pos.as_ref(),
            Self::SoftBreak(n) | Self::HardBreak(n) => n.pos.as_ref(),
            Self::CellWrap(n) => n.pos.as_ref(),
            Self::CriticInsert(n) => n.pos.as_ref(),
            Self::CriticDelete(n) => n.pos.as_ref(),
            Self::CriticSubstitute(n) => n.pos.as_ref(),
            Self::CriticComment(n) => n.pos.as_ref(),
            Self::Comment(n) => n.pos.as_ref(),
        }
    }

    /// Create a plain text node.
    pub fn text(value: impl Into<String>) -> Self {
        Self::Text(Text {
            value: value.into(),
            pos: None,
        })
    }

    /// Create an escaped text node.
    pub fn escaped_text(value: impl Into<String>) -> Self {
        Self::EscapedText(EscapedText {
            value: value.into(),
            pos: None,
        })
    }

    /// Create an inline code node.
    pub fn code(value: impl Into<String>, attrs: Option<Attrs>) -> Self {
        Self::Code(Code {
            value: value.into(),
            attrs,
            cell_wraps: None,
            pos: None,
        })
    }

    /// Create a soft line break.
    pub fn soft_break() -> Self {
        Self::SoftBreak(Break { pos: None })
    }

    /// Create a hard line break.
    pub fn hard_break() -> Self {
        Self::HardBreak(Break { pos: None })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Plain inline text.
pub struct Text {
    /// Text content.
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
/// A source character protected by a backslash.
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
/// An inline code span.
pub struct Code {
    /// Verbatim code content.
    pub value: String,
    /// Attributes attached to this node.
    pub attrs: Option<Attrs>,
    /// The `+` continuation joins inside `value` (CARVE-P12-069).
    pub cell_wraps: Option<Vec<VerbatimCellWrap>>,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// An authored nonbreaking space.
pub struct NonBreakingSpace {
    /// Attributes attached to this node.
    pub attrs: Option<Attrs>,
    /// Span in the original source, when available.
    pub pos: Option<Pos>,
}

/// A `+` continuation join inside a verbatim payload (CARVE-P12-069):
/// `offset` counts codepoints up to the joining space.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerbatimCellWrap {
    /// Codepoint offset of the joining space.
    pub offset: usize,
    /// Source continuation line, counted from 1.
    pub line: usize,
    /// Table row offset, when recorded.
    pub row: Option<usize>,
    /// Table column offset, when recorded.
    pub col: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// A continuation join inside a table cell.
pub struct CellWrap {
    /// Column of the continuation join, when recorded.
    pub col: Option<usize>,
    /// The row's continuation line this part came from, counted from 1.
    pub line: usize,
    /// Rows down from the cell to the row holding that line, when the cell
    /// spans down to it.
    pub row: Option<usize>,
    /// Span in the original source, when available.
    pub pos: Option<Pos>,
}

/// The first wrap in a table cell records its source line but separates no
/// visible content. Nested inline lists can start with a wrap that does
/// separate content outside the nested node, so callers use this only for a
/// table cell's own child list.
pub(crate) fn visible_table_cell_children(children: &[InlineNode]) -> &[InlineNode] {
    if matches!(children.first(), Some(InlineNode::CellWrap(_))) {
        &children[1..]
    } else {
        children
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// A soft or hard inline line break.
pub struct Break {
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// A named punctuation replacement.
pub struct SmartPunctuation {
    /// Name of the punctuation substitution.
    pub kind: String,
    /// Source spelling of the punctuation.
    pub value: String,
    /// Resolved output glyph, when provided.
    pub glyph: Option<String>,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

/// Built-in glyphs for named smart punctuation.
pub const GLYPHS: &[(&str, &str)] = &[
    ("ellipsis", "…"),
    ("em_dash", "—"),
    ("en_dash", "–"),
    ("left_right_arrow", "↔"),
    ("rightwards_arrow", "→"),
    ("leftwards_arrow", "←"),
    ("rightwards_double_arrow", "⇒"),
    ("leftwards_double_arrow", "⇐"),
    ("left_right_double_arrow", "⇔"),
    ("less_than_or_equal", "≤"),
    ("greater_than_or_equal", "≥"),
    ("not_equal", "≠"),
    ("plus_minus", "±"),
    ("copyright", "©"),
    ("registered", "®"),
    ("trademark", "™"),
];

/// Return the node's glyph, else the built-in glyph for its kind, else the source spelling.
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
            InlineNode::Ruby(r) => {
                let mut r = r.clone();
                for pair in &mut r.pairs {
                    pair.base = inline_nodes_without_strong(&pair.base);
                    pair.annotation = inline_nodes_without_strong(&pair.annotation);
                }
                out.push(InlineNode::Ruby(r));
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
/// Formatted inline content.
pub struct Emphasis {
    /// Attributes attached to this node.
    pub attrs: Option<Attrs>,
    /// Formatting applied to the children.
    pub kind: EmphasisKind,
    /// Inline content of this node.
    pub children: Vec<InlineNode>,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Formatting applied to inline content.
pub enum EmphasisKind {
    /// Italic emphasis.
    Italic,
    /// Strong emphasis.
    Strong,
    /// Underlined text.
    Underline,
    /// Struck text.
    Strike,
    /// Superscript text.
    Super,
    /// Subscript text.
    Sub,
    /// Highlighted text.
    Highlight,
    /// Combined bold and italic text.
    BoldItalic,
    /// Interchange-only (CARVE-P12-050). Carve 0.1 source has no spelling for
    /// the wrapper, so the parser never produces one and the canonical writer
    /// writes the children without it. It shares this enum with the kinds that
    /// DO have a spelling because it is the same node shape - children plus
    /// attributes - and every walker already reaches it here.
    SmallCaps,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// A link with inline label content.
pub struct Link {
    /// Attributes attached to this node.
    pub attrs: Option<Attrs>,
    /// Resolved or authored destination.
    pub href: String,
    /// Optional title text.
    pub title: Option<String>,
    /// Inline content of this node.
    pub children: Vec<InlineNode>,
    /// Unresolved reference label, when the source used one.
    pub ref_label: Option<String>,
    /// Original reference spelling, when retained.
    pub raw_ref: Option<String>,
    /// True when a reference resolves to a definition with an empty destination.
    /// Rebuilt from definitions after AST JSON decode; never written to the wire.
    pub resolved_empty_reference: bool,
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
/// An image with alternative text.
pub struct Image {
    /// Attributes attached to this node.
    pub attrs: Option<Attrs>,
    /// Resolved or authored image source.
    pub src: String,
    /// Alternative text for the image.
    pub alt: String,
    /// Optional title text.
    pub title: Option<String>,
    /// Unresolved reference label for `![alt][ref]` / collapsed `![alt][]`,
    /// mirroring [`Link::ref_label`]. `resolve_reference_links` matches it
    /// against the document's explicit `[label]: url` defs (case-sensitively):
    /// on hit it fills `src`/`title` and clears these; an unresolved image ref
    /// stays an Image and renders from the literal `raw_ref` source. Unlike a
    /// link ref it never matches heading text. `None` for a direct
    /// `![alt](src)` image.
    pub ref_label: Option<String>,
    /// Original reference spelling, when retained.
    pub raw_ref: Option<String>,
    /// True when a reference resolves to a definition with an empty destination.
    /// Rebuilt from definitions after AST JSON decode; never written to the wire.
    pub resolved_empty_reference: bool,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Inline content owned by an extension.
pub struct InlineExtension {
    /// Attributes attached to this node.
    pub attrs: Option<Attrs>,
    /// Name of this construct.
    pub name: String,
    /// Inline content of this node.
    pub children: Vec<InlineNode>,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// An inline container with attributes.
pub struct Span {
    /// Attributes attached to this node.
    pub attrs: Option<Attrs>,
    /// Inline content of this node.
    pub children: Vec<InlineNode>,
    /// True when a render-stage transform injected this node rather than the
    /// author writing it - today, the `section-number` span `headingNumbers`
    /// prepends to a heading. See [`RawInline::injected`]; the same render-time
    /// fact, and never written to the wire for the same reason.
    ///
    /// A CLASS is not a substitute: `[v1]{.section-number}` is valid source, and
    /// keying the strip on the class would delete an author's own span out of
    /// every derived label.
    pub injected: bool,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Ruby annotations paired with base text.
pub struct Ruby {
    /// Attributes attached to this node.
    pub attrs: Option<Attrs>,
    /// Base text and annotation pairs in order.
    pub pairs: Vec<RubyPair>,
    /// Span in the original source, when available.
    pub pos: Option<Pos>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Base text and its ruby annotation.
pub struct RubyPair {
    /// Base text being annotated.
    pub base: Vec<InlineNode>,
    /// Pronunciation or reading shown as the annotation.
    pub annotation: Vec<InlineNode>,
}

impl Ruby {
    /// Flatten ruby pairs to inline text with parenthesized annotations.
    pub fn flattened(&self) -> Vec<InlineNode> {
        let mut out = Vec::new();
        for pair in &self.pairs {
            out.extend(pair.base.iter().cloned());
            out.push(InlineNode::text("("));
            out.extend(pair.annotation.iter().cloned());
            out.push(InlineNode::text(")"));
        }
        out
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Inline or display mathematics.
pub struct Math {
    /// Attributes attached to this node.
    pub attrs: Option<Attrs>,
    /// Whether the math is displayed as a block.
    pub display: bool,
    /// Verbatim math content.
    pub content: String,
    /// The `+` continuation joins inside `content` (CARVE-P12-069).
    pub cell_wraps: Option<Vec<VerbatimCellWrap>>,
    /// Authored equation-number label: the visible numbering prefix and the
    /// counter bucket (CARVE-P12-051). Carve 0.1 source has no spelling for
    /// one, so the parser never sets it; it arrives on ingest.
    pub label: Option<String>,
    /// Equation number assigned by resolution (PART 9R R5a). Only a display
    /// node carrying a `label` may carry one.
    pub number: Option<usize>,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Raw inline content routed to a named format.
pub struct RawInline {
    /// Target format for the raw content.
    pub format: String,
    /// Literal raw content.
    pub content: String,
    /// The `+` continuation joins inside `content` (CARVE-P12-069).
    pub cell_wraps: Option<Vec<VerbatimCellWrap>>,
    /// True when a render-stage transform injected this node rather than the
    /// author writing it - today, the anchor `headingPermalinks` appends to a
    /// heading.
    ///
    /// A render-time fact about how the node got here, so it is NOT written to
    /// the wire (like [`Link::from_crossref`]): re-publishing it would put one
    /// pass's bookkeeping where the next reader reads it back as a claim. It
    /// exists because PART 9R R4's THE LABEL IS TAKEN BEFORE ANY RENDER-STAGE
    /// INJECTION has to be answerable AFTER the injection has happened - this
    /// engine derives display text at render time - and content-sniffing an
    /// anchor out of a raw HTML string is not an answer.
    pub injected: bool,
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
    /// Verbatim prose content.
    pub content: String,
    /// Attributes attached to this node.
    pub attrs: Option<Attrs>,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// A named shortcode for renderer substitution.
pub struct Symbol {
    /// Shortcode name without its delimiters.
    pub name: String,
    /// Attributes attached to this node.
    pub attrs: Option<Attrs>,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// A link recognized from angle-bracket source.
pub struct AutoLink {
    /// Attributes attached to this node.
    pub attrs: Option<Attrs>,
    /// Link destination.
    pub href: String,
    /// Display text = the raw content between `<>`: a URI autolink keeps its
    /// scheme (`<mailto:a@b>` shows `mailto:a@b`), an email shows the address.
    pub text: String,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// A reference to a heading id.
pub struct CrossRef {
    /// The raw id between `</#` and `>`, as the author spelled it. PART 12
    /// section 3a keeps it beside `href` rather than letting the resolution
    /// replace it.
    pub target: String,
    /// The resolved destination (`#` + the heading's id), set where the
    /// crossref resolved against a heading in this document. `None` says it
    /// resolved against nothing, which is what makes it render literally.
    pub href: Option<String>,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// A resolved figure or table caption number.
pub struct CaptionNumber {
    /// Resolved caption number, when assigned.
    pub number: Option<usize>,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// A named social mention.
pub struct Mention {
    /// Attributes attached to this node.
    pub attrs: Option<Attrs>,
    /// Mentioned user name.
    pub user: String,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// A named social tag.
pub struct Tag {
    /// Attributes attached to this node.
    pub attrs: Option<Attrs>,
    /// Tag name.
    pub name: String,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Citations written in one source group.
pub struct CitationGroup {
    /// Citations in source order.
    pub items: Vec<Citation>,
    /// Original source spelling of the group.
    pub raw: String,
    /// Which built-in formatter the Citations extension picked for this group.
    /// Annotation only: the wire `mode` field is [`Citation::mode`], which is a
    /// different fact entirely.
    pub render_mode: Option<CitationRenderMode>,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

impl CitationGroup {
    /// The group's `mode` on the wire: the summary of a per-item fact, not a
    /// fact of its own (carve-rs#1858). A source-spelled `[+@a; @b]` marks
    /// every item, so this is true exactly when the `+` was there.
    pub fn integral(&self) -> bool {
        !self.items.is_empty()
            && self
                .items
                .iter()
                .all(|item| item.mode == Some(CitationItemMode::Integral))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Built-in style chosen for rendering a citation group.
pub enum CitationRenderMode {
    /// Numeric citation labels.
    Numbered,
    /// Author and date citation labels.
    AuthorDate,
}

/// `citation.mode`. The schema admits one value, so the absent case is
/// parenthetical; it is an enum rather than a bool because the field names the
/// CSL citation-mode vocabulary and is expected to grow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CitationItemMode {
    /// Citation integrated into the surrounding sentence.
    Integral,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// One cited key and its surrounding annotation.
pub struct Citation {
    /// Cited reference key.
    pub key: String,
    /// This item's citation mode. Absent means parenthetical.
    pub mode: Option<CitationItemMode>,
    /// Inline text before the citation.
    pub prefix: Option<Vec<InlineNode>>,
    /// Parsed locator text, when supplied.
    pub locator: Option<Vec<InlineNode>>,
    /// Locator kind, such as a page label.
    pub locator_label: Option<String>,
    /// Locator value following its label.
    pub locator_value: Option<String>,
    /// Inline text after the citation.
    pub suffix: Option<Vec<InlineNode>>,
    /// Whether author text is suppressed.
    pub suppress_author: bool,
    /// Resolved numeric citation number.
    pub number: Option<usize>,
    /// Resolved display label.
    pub label: Option<String>,
    /// Per-key, document-wide use-site index (1-based), assigned only when a
    /// bibliography pool is supplied; drives back-link anchors (#199).
    pub use_index: Option<usize>,
    /// Span of this semicolon-delimited citation item in the original source.
    pub pos: Option<Pos>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// An inline abbreviation and its expansion.
pub struct Abbreviation {
    /// Abbreviated spelling.
    pub abbr: String,
    /// Expanded text for the abbreviation.
    pub expansion: String,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// An inline or referenced footnote.
pub struct Footnote {
    /// Attributes attached to this node.
    pub attrs: Option<Attrs>,
    /// Reference identifier, when present.
    pub id: Option<String>,
    /// Inline footnote content, when authored inline.
    pub inline: Option<Vec<InlineNode>>,
    /// Resolved footnote number.
    pub number: Option<usize>,
    /// Resolved backlink identifier.
    pub ref_id: Option<String>,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Inline text marked as an insertion.
pub struct CriticInsert {
    /// Inline content of this node.
    pub children: Vec<InlineNode>,
    /// Attributes attached to this node.
    pub attrs: Option<Attrs>,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Inline text marked as a deletion.
pub struct CriticDelete {
    /// Inline content of this node.
    pub children: Vec<InlineNode>,
    /// Attributes attached to this node.
    pub attrs: Option<Attrs>,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Old and new inline content in a substitution.
pub struct CriticSubstitute {
    /// The deleted half, as inline content (PART 12, ruling
    /// markup-carve/carve-js#1827).
    pub old: Vec<InlineNode>,
    /// The inserted half.
    pub new: Vec<InlineNode>,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// An inline editorial comment.
pub struct CriticComment {
    /// Editorial comment text.
    pub text: String,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

/// Whether a block PUBLISHES NOTHING, in every target.
///
/// §17 L1a: an invisible construct has no visible effect. Wherever the SHAPE of
/// the output depends on how many blocks a container holds, it is these that
/// must not be counted - a comment, an abbreviation definition and a link
/// reference definition all leave a node in the tree and nothing on the page.
///
/// Named here because the same three arms are spelled by hand at two sites in
/// the parser, and the one place they were NOT spelled is where a `dd` counted a
/// trailing comment as a second block and rendered loose while its LIST twin,
/// which filters the same set, stayed tight. The two existing sites are left as
/// they are: one of them counts a different set (no link reference definition),
/// so folding them together is a change of behavior rather than a rename.
pub(crate) fn publishes_nothing(block: &BlockNode) -> bool {
    matches!(
        block,
        BlockNode::Comment(_)
            | BlockNode::AbbreviationDef(_)
            | BlockNode::LinkReferenceDefinition(_)
    )
}
