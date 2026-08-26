//! Markdown-to-Carve migration boundary.
//!
//! AST-first, the way [`crate::html_import`] is: the source is parsed to a real
//! syntax tree, walked into a Carve [`Document`], and written by the canonical
//! writer. carve-js and carve-php instead rewrite Markdown source line by line,
//! which is why their output keeps the author's spelling and this one does not
//! - both produce the same document, spelled canonically here.
//!
//! Parsing to a tree is what makes the hard parts free. A line rewriter has to
//! carry fence state, a stack of list content columns and CommonMark's lazy
//! continuation rules by hand, and an off-by-one there silently re-bases a
//! fence into the wrong container. Here the parser owns all of it.
//!
//! ```
//! assert_eq!(carve::markdown_to_carve("*em* and **strong**"), "/em/ and *strong*\n");
//! ```

use std::collections::BTreeMap;

use pulldown_cmark::{Alignment, CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};

use crate::ast::*;
use crate::render_carve;

/// Convert Markdown source to Carve source.
///
/// GFM tables, strikethrough and task lists are enabled: they are what real
/// Markdown documents carry, and Carve has a spelling for each.
///
/// Footnotes and YAML frontmatter are enabled for a second reason - leaving
/// them OFF is not neutral, it corrupts. Without footnotes, `[^1]: Note.` is a
/// link-reference definition and `Text[^1]` a shortcut link to it, so the note
/// became the destination: `Text[^1](Note.)`. Without metadata blocks, a
/// `---` fence is a thematic break and the key line beneath it a setext
/// heading, so `title: T` became an `<h2>`. Both were caught by the
/// differential against carve-js, not by reasoning.
pub fn markdown_to_carve(markdown: &str) -> String {
    let document = markdown_to_ast(markdown);

    render_carve(&document).unwrap_or_default()
}

/// Convert Markdown source to a Carve [`Document`].
pub fn markdown_to_ast(markdown: &str) -> Document {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_FOOTNOTES);
    options.insert(Options::ENABLE_YAML_STYLE_METADATA_BLOCKS);

    let mut builder = Builder::default();
    for event in Parser::new_ext(markdown, options) {
        builder.push(event);
    }

    builder.finish()
}

/// A container under construction.
///
/// A start event pushes a frame and its end pops one, folding the finished node
/// into the frame beneath - so nesting comes from the parser rather than from
/// tracking indentation. Each frame owns the children it collects, which is why
/// a block frame and an inline frame are different variants rather than one
/// frame plus two side stacks that could drift out of step.
enum Frame {
    Paragraph(Vec<InlineNode>),
    Heading(u8, Vec<InlineNode>),
    BlockQuote(Vec<BlockNode>),
    List {
        ordered: bool,
        start: Option<usize>,
        items: Vec<ListItem>,
        /// A list is loose when any item holds more than one block, which
        /// CommonMark decides by blank lines between items; the parser has
        /// already applied that rule, so this reads the result off the tree.
        tight: bool,
    },
    ListItem {
        checked: Option<bool>,
        children: Vec<BlockNode>,
        /// Set when a paragraph opens DIRECTLY inside this item, which is how
        /// the parser spells looseness: a tight item emits its text with no
        /// paragraph around it. It cannot be inferred from the finished item,
        /// because this builder wraps that bare text in a paragraph of its own
        /// - so by the time the item closes, both shapes look alike.
        loose: bool,
        /// A TIGHT item's inline run, held until the item's content column is
        /// closed by a block or by the item itself.
        ///
        /// Tightness IS the absence of `Start(Paragraph)`, so these nodes
        /// arrive with no inline frame open and there is nothing to collect
        /// them into. The run is one paragraph, so it is buffered whole rather
        /// than folded a node at a time - it lives in the FRAME because a
        /// nested list opens a second item while the first one's run is still
        /// unflushed, and one buffer on the builder would mix the two.
        pending: Vec<InlineNode>,
    },
    CodeBlock {
        lang: Option<String>,
        content: String,
    },
    /// A block-level HTML element, which the parser opens once and then fills a
    /// line at a time. It is a frame rather than a buffer on the builder so the
    /// finished raw block folds into whatever container the element sits in,
    /// the same way every other block does.
    RawHtml(String),
    Emphasis(EmphasisKind, Vec<InlineNode>),
    Link {
        href: String,
        title: Option<String>,
        children: Vec<InlineNode>,
    },
    Image {
        src: String,
        title: Option<String>,
        alt: String,
    },
    Table {
        alignments: Vec<Alignment>,
        rows: Vec<TableRow>,
    },
    TableRow {
        header: bool,
        cells: Vec<TableCell>,
    },
    TableCell(Vec<InlineNode>),
    FootnoteDef {
        label: String,
        children: Vec<BlockNode>,
    },
    Metadata(String),
}

#[derive(Default)]
struct Builder {
    frames: Vec<Frame>,
    /// Top-level blocks, once every frame above them has closed.
    blocks: Vec<BlockNode>,
    footnote_defs: BTreeMap<String, Vec<BlockNode>>,
    /// Footnote numbers in order of first reference, which is the order the
    /// Carve parser assigns and therefore the one a round trip must reproduce.
    footnote_numbers: BTreeMap<String, usize>,
    frontmatter: Option<Frontmatter>,
}

impl Builder {
    fn push(&mut self, event: Event<'_>) {
        match event {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(text) => self.text(&text),
            Event::Code(code) => self.inline(InlineNode::code(code.to_string(), None)),
            // A soft break is a newline the author wrote inside a paragraph;
            // Carve keeps it, so the writer can re-wrap at the same place.
            Event::SoftBreak => self.inline(InlineNode::soft_break()),
            Event::HardBreak => self.inline(InlineNode::hard_break()),
            Event::Rule => self.block(BlockNode::ThematicBreak(ThematicBreak::default())),
            // An HTML BLOCK becomes a raw block, matching carve-js and
            // carve-php. Markdown's own contract is that block HTML is HTML,
            // and dropping it to text silently loses a `<div>` wrapper the
            // author wrote on purpose. Whether that raw block may render is a
            // PROFILE decision (`apply_profile` strips it), which is where a
            // policy about untrusted input belongs - not silently inside one
            // engine's importer while the other two keep it.
            Event::Html(html) => self.raw_html(&html),
            // Inline HTML stays text: it has no block to become, and Carve has
            // no inline raw spelling in the 0.1 core.
            Event::InlineHtml(html) => self.text(&html),
            Event::FootnoteReference(label) => {
                let next = self.footnote_numbers.len() + 1;
                let number = *self
                    .footnote_numbers
                    .entry(label.to_string())
                    .or_insert(next);
                self.inline(InlineNode::Footnote(Footnote {
                    attrs: None,
                    id: Some(label.to_string()),
                    inline: None,
                    number: Some(number),
                    ref_id: None,
                    pos: None,
                }));
            }
            Event::TaskListMarker(checked) => {
                if let Some(Frame::ListItem { checked: slot, .. }) = self.frames.last_mut() {
                    *slot = Some(checked);
                }
            }
            _ => {}
        }
    }

    /// Collect one line of a block-level HTML element into the frame the
    /// element opened.
    fn raw_html(&mut self, value: &str) {
        match self.frames.last_mut() {
            Some(Frame::RawHtml(content)) => content.push_str(value),
            // The parser wraps every block-HTML line in an `HtmlBlock`, so a
            // line with no frame open is not a shape this reaches today. It
            // keeps the source as text rather than dropping it, which is what
            // inline HTML does with the same content.
            _ => self.text(value),
        }
    }

    /// Text lands in whatever is open: a code block collects it verbatim, an
    /// image's alt is a plain string on the node, everything else takes a node.
    fn text(&mut self, value: &str) {
        match self.frames.last_mut() {
            Some(Frame::CodeBlock { content, .. }) | Some(Frame::Metadata(content)) => {
                content.push_str(value)
            }
            Some(Frame::Image { alt, .. }) => alt.push_str(value),
            _ => self.inline(InlineNode::text(value)),
        }
    }

    fn start(&mut self, tag: Tag<'_>) {
        if matches!(tag, Tag::Paragraph) {
            if let Some(Frame::ListItem { loose, .. }) = self.frames.last_mut() {
                *loose = true;
            }
        }

        let frame = match tag {
            Tag::Paragraph => Frame::Paragraph(Vec::new()),
            Tag::Heading { level, .. } => Frame::Heading(heading_level(level), Vec::new()),
            Tag::BlockQuote(_) => Frame::BlockQuote(Vec::new()),
            Tag::List(start) => Frame::List {
                ordered: start.is_some(),
                start: start.map(|start| start as usize),
                items: Vec::new(),
                tight: true,
            },
            Tag::Item => Frame::ListItem {
                checked: None,
                children: Vec::new(),
                loose: false,
                pending: Vec::new(),
            },
            Tag::CodeBlock(kind) => Frame::CodeBlock {
                lang: match kind {
                    // Only the first word of the info string is the language;
                    // the rest is the author's metadata, which Carve's fence
                    // has no slot for.
                    CodeBlockKind::Fenced(info) => info
                        .split_whitespace()
                        .next()
                        .filter(|word| !word.is_empty())
                        .map(str::to_string),
                    CodeBlockKind::Indented => None,
                },
                content: String::new(),
            },
            Tag::HtmlBlock => Frame::RawHtml(String::new()),
            Tag::Emphasis => Frame::Emphasis(EmphasisKind::Italic, Vec::new()),
            Tag::Strong => Frame::Emphasis(EmphasisKind::Strong, Vec::new()),
            Tag::Strikethrough => Frame::Emphasis(EmphasisKind::Strike, Vec::new()),
            Tag::Link {
                dest_url, title, ..
            } => Frame::Link {
                href: dest_url.to_string(),
                title: optional(&title),
                children: Vec::new(),
            },
            Tag::Image {
                dest_url, title, ..
            } => Frame::Image {
                src: dest_url.to_string(),
                title: optional(&title),
                alt: String::new(),
            },
            Tag::Table(alignments) => Frame::Table {
                alignments,
                rows: Vec::new(),
            },
            Tag::TableHead => Frame::TableRow {
                header: true,
                cells: Vec::new(),
            },
            Tag::TableRow => Frame::TableRow {
                header: false,
                cells: Vec::new(),
            },
            Tag::TableCell => Frame::TableCell(Vec::new()),
            Tag::FootnoteDefinition(label) => Frame::FootnoteDef {
                label: label.to_string(),
                children: Vec::new(),
            },
            Tag::MetadataBlock(_) => Frame::Metadata(String::new()),
            // Nothing else is enabled, so nothing reaches here; an unopened
            // frame would desync the stack on the matching end event, hence a
            // frame rather than a skip.
            _ => Frame::Paragraph(Vec::new()),
        };

        self.frames.push(frame);
    }

    fn end(&mut self, _tag: TagEnd) {
        self.close();
    }

    fn close(&mut self) {
        let Some(frame) = self.frames.pop() else {
            return;
        };

        match frame {
            Frame::Paragraph(children) => self.block(BlockNode::Paragraph(Paragraph {
                attrs: None,
                children,
                at_content_column: true,
                block_image: false,
                pos: None,
            })),
            Frame::Heading(level, children) => self.block(BlockNode::Heading(Heading {
                attrs: None,
                level,
                children,
                pos: None,
            })),
            Frame::BlockQuote(children) => self.block(BlockNode::BlockQuote(BlockQuote {
                attrs: None,
                children,
                // Markdown has one spelling, so an import carries none.
                fenced: false,
                pos: None,
            })),
            Frame::List {
                ordered,
                start,
                items,
                tight,
            } => self.block(BlockNode::List(List {
                attrs: None,
                ordered,
                // A list that starts at 1 is the default, and recording it
                // makes the writer spell out a start the author did not.
                start: start.filter(|start| *start != 1),
                ol_type: None,
                bare_marker: false,
                delim: None,
                bullet_char: None,
                tight,
                items,
                pos: None,
            })),
            Frame::ListItem {
                checked,
                mut children,
                loose,
                mut pending,
            } => {
                flush_inline_run(&mut pending, &mut children);
                let item = ListItem {
                    attrs: None,
                    checked,
                    children,
                    pos: None,
                };
                if let Some(Frame::List { items, tight, .. }) = self.frames.last_mut() {
                    *tight = *tight && !loose;
                    items.push(item);
                }
            }
            Frame::CodeBlock { lang, content } => self.block(BlockNode::CodeBlock(CodeBlock {
                attrs: None,
                lang,
                title: None,
                label: None,
                // The parser hands the body with its closing newline; the node
                // holds the body, and the writer supplies the fence lines.
                content: content.strip_suffix('\n').unwrap_or(&content).to_string(),
                pos: None,
            })),
            // The frame has already been popped, so the raw block folds into
            // the container the element sits in - a quote, a list item or a
            // footnote definition - instead of landing at the top of the
            // document ahead of the container it was written inside.
            Frame::RawHtml(content) => self.block(BlockNode::RawBlock(RawBlock {
                format: "html".to_string(),
                content: content.trim_end_matches('\n').to_string(),
                pos: None,
            })),
            // Markdown emphasis IS Carve emphasis; only the spelling differs,
            // and the spelling belongs to the writer.
            Frame::Emphasis(kind, children) => self.inline(InlineNode::Emphasis(Emphasis {
                attrs: None,
                kind,
                children,
                pos: None,
            })),
            Frame::Link {
                href,
                title,
                children,
            } => self.inline(InlineNode::Link(Link {
                attrs: None,
                href,
                title,
                children,
                ref_label: None,
                raw_ref: None,
                from_crossref: false,
                from_heading_reference: false,
                pos: None,
            })),
            Frame::Image { src, title, alt } => self.inline(InlineNode::Image(Image {
                attrs: None,
                src,
                alt,
                title,
                ref_label: None,
                raw_ref: None,
                pos: None,
            })),
            Frame::Table { rows, .. } => self.block(BlockNode::Table(Table {
                attrs: None,
                caption: None,
                short_caption: None,
                columns: Vec::new(),
                rows,
                row_groups: None,
                pos: None,
            })),
            Frame::TableRow { cells, .. } => {
                let row = TableRow {
                    cells,
                    attrs: None,
                    pos: None,
                };
                if let Some(Frame::Table { rows, .. }) = self.frames.last_mut() {
                    rows.push(row);
                }
            }
            Frame::TableCell(children) => {
                // Alignment is a property of the COLUMN in Markdown and of the
                // CELL in Carve, so it is read at the moment the cell closes,
                // when its column index is the row's current cell count.
                let (header, column) = match self.frames.last() {
                    Some(Frame::TableRow { header, cells }) => (*header, cells.len()),
                    _ => (false, 0),
                };
                let align = self
                    .frames
                    .iter()
                    .rev()
                    .find_map(|frame| match frame {
                        Frame::Table { alignments, .. } => Some(alignments),
                        _ => None,
                    })
                    .and_then(|alignments| match alignments.get(column) {
                        Some(Alignment::Left) => Some(TableAlign::Left),
                        Some(Alignment::Center) => Some(TableAlign::Center),
                        Some(Alignment::Right) => Some(TableAlign::Right),
                        _ => None,
                    });
                let cell = TableCell {
                    header,
                    span: None,
                    align,
                    valign: None,
                    attrs: None,
                    children,
                    pos: None,
                };
                if let Some(Frame::TableRow { cells, .. }) = self.frames.last_mut() {
                    cells.push(cell);
                }
            }
            // A definition is not a block in the document: Carve holds it in a
            // map keyed by label, so a note may be written anywhere and still
            // render at the end.
            Frame::FootnoteDef { label, children } => {
                self.footnote_defs.insert(label, children);
            }
            Frame::Metadata(content) => {
                self.frontmatter = Some(Frontmatter {
                    format: "yaml".to_string(),
                    content: content.trim_end_matches('\n').to_string(),
                    pos: None,
                });
            }
        }
    }

    fn block(&mut self, node: BlockNode) {
        match self.frames.last_mut() {
            // A block ends the item's content column, so whatever inline run
            // was still open closes as its own paragraph FIRST. Without this a
            // nested list would sort ahead of the text the item opened with.
            Some(Frame::ListItem {
                children, pending, ..
            }) => {
                flush_inline_run(pending, children);
                children.push(node);
            }
            Some(Frame::BlockQuote(children)) | Some(Frame::FootnoteDef { children, .. }) => {
                children.push(node)
            }
            _ => self.blocks.push(node),
        }
    }

    fn inline(&mut self, node: InlineNode) {
        match self.frames.last_mut() {
            Some(Frame::Paragraph(children))
            | Some(Frame::Heading(_, children))
            | Some(Frame::Emphasis(_, children))
            | Some(Frame::Link { children, .. })
            | Some(Frame::TableCell(children)) => children.push(node),
            // A TIGHT item spells its content with no paragraph around it, so
            // the run arrives here. It is buffered and closed whole - one
            // paragraph for the item, not one per node.
            Some(Frame::ListItem { pending, .. }) => pending.push(node),
            // An image's alt is a plain string on the node, so a construct
            // inside it contributes its TEXT. Turning it into a node of its own
            // would put it outside the image entirely, which is where it used
            // to go.
            Some(Frame::Image { alt, .. }) => alt.push_str(&inline_text(&node)),
            // Inline content with no inline container open is content the
            // author wrote outside any block; it becomes a paragraph rather
            // than being dropped.
            _ => self.block(BlockNode::Paragraph(Paragraph {
                attrs: None,
                children: vec![node],
                at_content_column: true,
                block_image: false,
                pos: None,
            })),
        }
    }

    fn finish(mut self) -> Document {
        // Truncated input can leave frames open; closing them keeps the content
        // rather than discarding a half-built tree.
        while !self.frames.is_empty() {
            self.close();
        }

        let frontmatter = self
            .frontmatter
            .as_ref()
            .map(|frontmatter| parse_frontmatter(&frontmatter.content))
            .unwrap_or_default();

        Document {
            frontmatter,
            frontmatter_raw: self.frontmatter,
            footnote_defs: self.footnote_defs,
            footnote_def_pos: BTreeMap::new(),
            children: self.blocks,
            source_len: 0,
            ingest_payload_len: 0,
        }
    }
}

/// Read the flat `key: value` pairs a Carve document exposes alongside the raw
/// block. Deliberately not a YAML parser - the raw block keeps the source, so
/// anything structured survives there and only the scalars are lifted, which is
/// what the Carve parser's own frontmatter handling does.
/// Close a tight list item's collected inline run as the ONE paragraph it is.
///
/// `pulldown-cmark` spells a tight item by emitting its inlines with no
/// `Start(Paragraph)` around them - that absence IS the tightness - so the run
/// arrives with nothing to collect it into. The content still needs a
/// paragraph; what it does not need is one per node, which is what wrapping
/// each arriving node separately produced (markup-carve/carve-rs#969).
fn flush_inline_run(pending: &mut Vec<InlineNode>, children: &mut Vec<BlockNode>) {
    if pending.is_empty() {
        return;
    }
    children.push(BlockNode::Paragraph(Paragraph {
        attrs: None,
        children: std::mem::take(pending),
        at_content_column: true,
        block_image: false,
        pos: None,
    }));
}

/// The text a construct inside an image's alt contributes to it.
///
/// `alt` is a plain string on the node, so an emphasis, a code span or a link
/// written inside `![...]` has no node to become. CommonMark flattens it the
/// same way - `![a *b* c](i.png)` carries `alt="a b c"` - so the text is what
/// is kept. A break contributes nothing, which is what this importer already
/// did with one and is deliberately not changed here: a newline inside `alt`
/// would have to be written back into a single-line image spelling.
fn inline_text(node: &InlineNode) -> String {
    match node {
        InlineNode::Text(text) => text.value.clone(),
        InlineNode::Code(code) => code.value.clone(),
        InlineNode::Emphasis(emphasis) => inline_run_text(&emphasis.children),
        InlineNode::Link(link) => inline_run_text(&link.children),
        InlineNode::Image(image) => image.alt.clone(),
        _ => String::new(),
    }
}

fn inline_run_text(nodes: &[InlineNode]) -> String {
    nodes.iter().map(inline_text).collect()
}

fn parse_frontmatter(content: &str) -> BTreeMap<String, String> {
    content
        .lines()
        .filter_map(|line| {
            let (key, value) = line.split_once(':')?;
            let key = key.trim();
            (!key.is_empty() && !key.starts_with('#'))
                .then(|| (key.to_string(), value.trim().to_string()))
        })
        .collect()
}

fn optional(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.to_string())
}

fn heading_level(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emphasis_takes_the_carve_spelling() {
        assert_eq!(markdown_to_carve("*em*"), "/em/\n");
        assert_eq!(markdown_to_carve("**strong**"), "*strong*\n");
        assert_eq!(markdown_to_carve("_em_"), "/em/\n");
    }

    #[test]
    fn nested_emphasis_keeps_both_families() {
        assert_eq!(markdown_to_carve("*a **b** c*"), "/a *b* c/\n");
    }

    #[test]
    fn headings_keep_their_level() {
        assert_eq!(markdown_to_carve("# One"), "# One\n");
        assert_eq!(markdown_to_carve("### Three"), "### Three\n");
    }

    #[test]
    fn a_setext_heading_becomes_an_atx_one() {
        // The tree records a level-1 heading; the spelling is the writer's.
        assert_eq!(markdown_to_carve("Title\n====="), "# Title\n");
    }

    #[test]
    fn a_fence_keeps_its_language_and_body() {
        // The Markdown source is the lenient spelling and the output is the
        // canonical one: `fenced_code_block` names the no-space form canonical
        // and says it is what the X->Carve converters emit, and this importer
        // ends at the canonical writer.
        assert_eq!(
            markdown_to_carve("``` js\nlet x = 1\n```"),
            "```js\nlet x = 1\n```\n"
        );
        assert_eq!(
            markdown_to_carve("```js\nlet x = 1\n```"),
            "```js\nlet x = 1\n```\n"
        );
    }

    #[test]
    fn an_info_string_contributes_only_its_first_word() {
        assert!(markdown_to_carve("``` js title=x\ny\n```").starts_with("```js\n"));
    }

    #[test]
    fn a_code_span_is_verbatim() {
        assert_eq!(markdown_to_carve("`*not em*`"), "`*not em*`\n");
    }

    #[test]
    fn lists_survive_with_their_nesting() {
        assert_eq!(markdown_to_carve("- a\n- b"), "- a\n- b\n");
        assert!(markdown_to_carve("- a\n  - b").contains("- b"));
        assert_eq!(markdown_to_carve("1. a\n2. b"), "1. a\n2. b\n");
    }

    #[test]
    fn a_task_list_keeps_its_checkbox() {
        let out = markdown_to_carve("- [x] done\n- [ ] todo");
        assert!(out.contains("[x]"), "{out}");
        assert!(out.contains("[ ]"), "{out}");
    }

    #[test]
    fn a_blockquote_survives() {
        assert_eq!(markdown_to_carve("> quoted"), "> quoted\n");
    }

    #[test]
    fn a_link_and_an_image_survive() {
        assert_eq!(
            markdown_to_carve("[text](https://e.com)"),
            "[text](https://e.com)\n"
        );
        assert_eq!(markdown_to_carve("![alt](a.png)"), "![alt](a.png)\n");
    }

    #[test]
    fn a_gfm_table_becomes_a_carve_table() {
        let out = markdown_to_carve("| A | B |\n|---|---|\n| 1 | 2 |");
        assert!(out.contains('A') && out.contains('B'), "{out}");
        assert!(out.contains("| 1 | 2 |"), "{out}");
    }

    #[test]
    fn strikethrough_takes_the_single_delimiter() {
        assert_eq!(markdown_to_carve("~~gone~~"), "~gone~\n");
    }

    #[test]
    fn a_thematic_break_survives() {
        assert_eq!(markdown_to_carve("---"), "---\n");
    }

    #[test]
    fn an_html_block_becomes_a_raw_block() {
        assert_eq!(
            markdown_to_carve("<div>\nraw\n</div>"),
            "```=html\n<div>\nraw\n</div>\n```\n"
        );
    }

    #[test]
    fn consecutive_html_lines_are_one_raw_block() {
        // The parser hands block HTML a chunk at a time; two raw blocks here
        // would be two `=html` fences where the author wrote one element.
        assert_eq!(
            markdown_to_carve("<div>\nraw\n</div>")
                .matches("```")
                .count(),
            2
        );
    }

    #[test]
    fn inline_html_stays_text() {
        // No block to become, and the 0.1 core has no inline raw spelling.
        let out = markdown_to_carve("a <b>c</b> d");
        assert!(!out.contains("=html"), "{out}");
    }

    #[test]
    fn frontmatter_survives_instead_of_becoming_a_heading() {
        let document = markdown_to_ast("---\ntitle: T\n---\n\nBody.");
        assert_eq!(
            document.frontmatter.get("title").map(String::as_str),
            Some("T")
        );
        assert_eq!(document.children.len(), 1);
    }

    #[test]
    fn a_footnote_keeps_its_definition_instead_of_becoming_a_link() {
        let document = markdown_to_ast("Text[^1]\n\n[^1]: Note.");
        assert!(document.footnote_defs.contains_key("1"));
        assert_eq!(
            markdown_to_carve("Text[^1]\n\n[^1]: Note."),
            "Text[^1]\n\n[^1]: Note.\n"
        );
    }

    #[test]
    fn a_loose_list_stays_loose() {
        // Looseness is what puts a `<p>` inside each `<li>`, so collapsing it
        // changes the rendered document, not only the source.
        assert_eq!(markdown_to_carve("- a\n\n- b"), "- a\n\n- b\n");
        assert_eq!(markdown_to_carve("- a\n- b"), "- a\n- b\n");
    }

    #[test]
    fn an_empty_document_stays_empty() {
        assert_eq!(markdown_to_carve("").trim(), "");
    }
}
