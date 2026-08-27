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
    /// Where a footnote definition was WRITTEN, for the definitions whose body
    /// cannot say - keyed by the same label.
    pub footnote_def_pos: BTreeMap<String, Pos>,
    pub children: Vec<BlockNode>,
    /// Byte length of the (normalized) source this document was parsed from.
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

impl Document {
    /// The length a per-render expansion budget may be sized from.
    ///
    /// The expansion budgets - abbreviations, the table of contents, the index -
    /// are `max(floor, factor * this)`. A cap has to be enforced against
    /// something the attacker does not supply, and on the parse path this is
    /// exactly that: the parser measured the input, so a bigger budget costs a
    /// bigger document.
    ///
    /// On the ingest path `source_len` arrives INSIDE the payload. Left alone it
    /// let the payload choose the size of the guard meant to bound it: rewriting
    /// one number to `1000000000` took a 214 KB payload from 1.04 MB of HTML to
    /// 101 MB, 472x, for nine extra bytes. So an ingested document is bounded by
    /// what its payload actually cost as well as by what it claims, and the
    /// smaller wins.
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
    FigureGroup(FigureGroup),
    AbbreviationDef(AbbreviationDef),
    LinkReferenceDefinition(LinkReferenceDefinition),
    CitationDefinition(CitationDefinition),
    RawBlock(RawBlock),
    Comment(Comment),
    Extension(BlockExtension),
    BlockImage(Image),
    ThematicBreak(ThematicBreak),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ThematicBreak {
    /// The thematic-break character as authored. Absence defaults to `-`.
    pub marker: Option<char>,
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
    pub block_image: bool,
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
    /// True when the quote was authored as a colon-fence container rather than
    /// with line markers. The two spellings are one node; this records which,
    /// so the canonical writer writes back what it read. False on a prefixed
    /// quote, so a document that predates the fence serializes exactly as it
    /// did (markup-carve/carve#1718).
    pub fenced: bool,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Table {
    pub attrs: Option<Attrs>,
    pub caption: Option<Vec<InlineNode>>,
    /// Structured publishing/navigation label; Carve 0.1 source has no spelling.
    pub short_caption: Option<Vec<InlineNode>>,
    /// Per-column publishing metadata, resolved from positional table attrs.
    pub columns: Vec<TableColumn>,
    pub rows: Vec<TableRow>,
    /// An explicit head/body/foot partition of `rows`, imported from or destined
    /// for a format whose table model has one. Carve 0.1 source has no spelling
    /// for it, so a parse never sets it.
    pub row_groups: Option<TableRowGroups>,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

/// An explicit head/body/foot partition of a table's rows (PART 12 §15).
///
/// It holds COUNTS, never rows: they consume `rows` in order, head first, then
/// each body, then the foot, and they MUST account for every row exactly once.
/// Absent means the implicit structure every renderer already derives - the
/// leading run of header rows as the head, everything after it as one body, no
/// foot, no row-head columns - so a tree without it does not change shape. HTML,
/// plain and ANSI output ignore it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableRowGroups {
    /// Rows at the start of `rows` forming the table head.
    pub head_rows: usize,
    /// The body groups, in order, each consuming the next
    /// `head_rows + body_rows` rows.
    pub bodies: Vec<TableBodyGroup>,
    /// Rows at the end of `rows` forming the table foot.
    pub foot_rows: usize,
}

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
    pub attrs: Option<Attrs>,
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
    pub valign: Option<TableVerticalAlign>,
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TableColumn {
    pub align: Option<TableAlign>,
    pub valign: Option<TableVerticalAlign>,
    /// Fraction of the table width, in `(0, 1]`.
    pub width: Option<f64>,
}

// Widths are finite values validated at the AST boundary.
impl Eq for TableColumn {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableVerticalAlign {
    Top,
    Middle,
    Bottom,
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
pub struct Figure {
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
    pub caption: Vec<InlineNode>,
    /// Structured publishing/navigation label; ordinary renderers ignore it.
    pub short_caption: Option<Vec<InlineNode>>,
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
    pub attrs: Option<Attrs>,
    pub children: Vec<BlockNode>,
    /// The group caption (the `^ ` line after the closing fence). `None`
    /// means the group is uncaptioned - never an empty placeholder.
    pub caption: Option<Vec<InlineNode>>,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AbbreviationDef {
    pub abbr: String,
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
    /// Present and non-empty: a definition with an empty destination is not one.
    pub href: String,
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
pub struct RawBlock {
    pub format: String,
    pub content: String,
    /// Span in the original source, when the parser could determine it.
    pub pos: Option<Pos>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comment {
    pub block: bool,
    pub delimited: bool,
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
    /// "unpositioned" for a variant added later, and the one caller derives a
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
            Self::SoftBreak(n) | Self::HardBreak(n) => n.pos.as_ref(),
            Self::CriticInsert(n) => n.pos.as_ref(),
            Self::CriticDelete(n) => n.pos.as_ref(),
            Self::CriticSubstitute(n) => n.pos.as_ref(),
            Self::CriticComment(n) => n.pos.as_ref(),
            Self::Comment(n) => n.pos.as_ref(),
        }
    }

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
    /// Span of this semicolon-delimited citation item in the original source.
    pub pos: Option<Pos>,
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
