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

/// The title of a link or image with no destination, which has no slot left to
/// carry it, kept on a span the way the HTML importer keeps it (carve-rs#1738).
fn titled_span(title: String, children: Vec<InlineNode>) -> InlineNode {
    let mut attrs = Attrs::default();
    attrs.key_values.insert("title".to_string(), title);
    InlineNode::Span(Span {
        attrs: Some(attrs),
        children,
        injected: false,
        pos: None,
    })
}

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
fn is_empty_destination(destination: &str) -> bool {
    destination
        .trim_matches(|c: char| c.is_ascii_whitespace())
        .is_empty()
}

/// Convert Markdown source to Carve source, or the writer's refusal.
///
/// Markdown caps nesting nowhere, and [`markdown_to_ast`] builds the tree from
/// the parser's frames rather than by recursing, so this is the one importer that
/// can hand the writer a tree deeper than `parse::MAX_NESTING_DEPTH` - which is
/// what PART 9 §25's ceiling is for. Untrusted Markdown therefore needs the
/// fallible form: [`markdown_to_carve`] can only answer with an empty string
/// (carve-rs#1877).
pub fn try_markdown_to_carve(markdown: &str) -> Result<String, crate::RenderCarveError> {
    render_carve(&markdown_to_ast(markdown))
}

/// Convert Markdown source to Carve source.
///
/// Returns an empty string where the writer refuses; prefer
/// [`try_markdown_to_carve`] for input you did not write.
pub fn markdown_to_carve(markdown: &str) -> String {
    try_markdown_to_carve(markdown).unwrap_or_default()
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
    for (event, range) in Parser::new_ext(markdown, options).into_offset_iter() {
        if matches!(&event, Event::Html(_))
            && !builder
                .frames
                .iter()
                .any(|frame| matches!(frame, Frame::ListItem { .. } | Frame::BlockQuote(_)))
        {
            let line_start = markdown[..range.start]
                .rfind('\n')
                .map_or(0, |newline| newline + 1);
            let indent = &markdown[line_start..range.start];
            if !indent.is_empty() && indent.chars().all(|ch| matches!(ch, ' ' | '\t')) {
                builder.raw_html(indent);
            }
        }
        builder.push(event, &markdown[range]);
        if builder.over_depth {
            break;
        }
    }

    builder.finish()
}

/// The nesting the importer will BUILD, in AST levels (PART 9 §25).
///
/// ONE level above the renderers' ceiling, deliberately: a tree that reaches this
/// cap is already one level past what any renderer accepts, so the writer refuses
/// it with exactly the error it would have produced without the cap. A document
/// under the ceiling is built untouched.
///
/// The cap is here because this importer is the only one with nothing else to
/// bound it. The Carve parser caps its own nesting, the HTML importer answers
/// `HtmlImportError::DepthLimit`, ingest has its JSON depth budget - Markdown
/// nesting is limited only by the input, and this builder folds frames instead of
/// recursing, so it cheerfully built a tree that could not afterwards be walked,
/// cloned or even FREED without overflowing the stack and aborting the process
/// (carve-rs#1877). Derived `Clone` and `Drop` recurse over the tree and have no
/// ceiling to consult, so the only place to stop it is before it exists.
const MAX_IMPORT_LEVELS: usize = crate::render::MAX_RENDER_DEPTH + 1;

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
    /// A paired HTML tag with a native Carve inline representation.
    HtmlEmphasis {
        tag: String,
        kind: EmphasisKind,
        children: Vec<InlineNode>,
    },
    HtmlCode {
        tag: String,
        content: String,
    },
    HtmlInsert {
        tag: String,
        children: Vec<InlineNode>,
    },
    /// An inline HTML run with no native Carve node. `tag` is the outer tag
    /// whose closing fragment completes the run; standalone fragments have
    /// already been emitted and never need a frame.
    RawInline {
        tag: String,
        content: String,
    },
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

/// The AST levels a finished frame adds beneath itself: (block, inline).
///
/// EXHAUSTIVE on purpose. A container counted as zero here is a hole in
/// [`MAX_IMPORT_LEVELS`], and the hole would only show as an abort on input deep
/// enough to reach it.
fn levels_added(frame: &Frame) -> (usize, usize) {
    match frame {
        // An item's blocks sit one level under the LIST, so the list owns that
        // level and the item frame adds none of its own.
        Frame::BlockQuote(_)
        | Frame::List { .. }
        | Frame::Table { .. }
        | Frame::FootnoteDef { .. } => (1, 0),
        Frame::Emphasis(..)
        | Frame::Link { .. }
        | Frame::HtmlEmphasis { .. }
        | Frame::HtmlInsert { .. } => (0, 1),
        // A paragraph, heading or cell OPENS an inline sequence rather than
        // nesting inside one, and the rest hold text.
        Frame::Paragraph(_)
        | Frame::Heading(..)
        | Frame::ListItem { .. }
        | Frame::CodeBlock { .. }
        | Frame::RawHtml(_)
        | Frame::HtmlCode { .. }
        | Frame::RawInline { .. }
        | Frame::Image { .. }
        | Frame::TableRow { .. }
        | Frame::TableCell(_)
        | Frame::Metadata(_) => (0, 0),
    }
}

#[derive(Default)]
struct Builder {
    frames: Vec<Frame>,
    /// Block and inline nesting the open frames account for, held against
    /// [`MAX_IMPORT_LEVELS`]. Maintained by `push_frame` / `pop_frame` alone, so
    /// no call site can move the stack without moving these with it.
    block_levels: usize,
    inline_levels: usize,
    /// Set when a frame was refused at the cap. The event loop stops on it and
    /// `finish` closes what is open.
    over_depth: bool,
    /// Top-level blocks, once every frame above them has closed.
    blocks: Vec<BlockNode>,
    footnote_defs: BTreeMap<String, Vec<BlockNode>>,
    /// Footnote numbers in order of first reference, which is the order the
    /// Carve parser assigns and therefore the one a round trip must reproduce.
    footnote_numbers: BTreeMap<String, usize>,
    frontmatter: Option<Frontmatter>,
}

impl Builder {
    fn push(&mut self, event: Event<'_>, source: &str) {
        // Once a non-native HTML element opens, everything through its closing
        // tag is one verbatim raw-inline run. pulldown still tokenizes Markdown
        // inside that run, so reconstruct the few token shapes it can emit.
        if self.append_to_raw_inline(&event) {
            return;
        }

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
            Event::Html(html) => {
                // pulldown normalizes away up to three leading spaces on an
                // HTML-block line, while carve-js preserves those source
                // bytes. The offset iterator still exposes them.
                let value = if source.trim_start() == html.as_ref() {
                    source
                } else {
                    &html
                };
                self.raw_html(value)
            }
            Event::InlineHtml(html) => self.inline_html(&html),
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

    fn append_to_raw_inline(&mut self, event: &Event<'_>) -> bool {
        let Some(Frame::RawInline { tag, content }) = self.frames.last_mut() else {
            return false;
        };

        if matches!(event, Event::End(end) if is_block_end(end)) || matches!(event, Event::Rule) {
            self.close();
            return false;
        }

        match event {
            Event::InlineHtml(html) | Event::Html(html) => {
                content.push_str(html);
                if html_tag(html)
                    .is_some_and(|parsed| parsed.closing && parsed.name.eq_ignore_ascii_case(tag))
                {
                    self.close();
                }
            }
            Event::Text(text) => content.push_str(text),
            Event::Code(code) => {
                content.push('`');
                content.push_str(code);
                content.push('`');
            }
            Event::SoftBreak => content.push('\n'),
            Event::HardBreak => content.push_str("  \n"),
            _ => {}
        }
        true
    }

    fn inline_html(&mut self, value: &str) {
        let Some(tag) = html_tag(value) else {
            self.raw_inline(value.to_string());
            return;
        };

        if tag.closing {
            let closes_top = match self.frames.last() {
                Some(Frame::HtmlEmphasis { tag: open, .. })
                | Some(Frame::HtmlCode { tag: open, .. })
                | Some(Frame::HtmlInsert { tag: open, .. }) => open.eq_ignore_ascii_case(tag.name),
                _ => false,
            };
            if closes_top {
                self.close_matched_html();
            } else {
                self.raw_inline(value.to_string());
            }
            return;
        }

        if tag.self_closing || is_void_html_tag(tag.name) {
            self.raw_inline(value.to_string());
            return;
        }

        let name = tag.name.to_ascii_lowercase();
        // Only a BARE native tag converts to a Carve construct. An attributed
        // tag (`<b class="x">`) opens a raw-inline run instead, so its
        // attributes survive verbatim rather than being dropped.
        if !tag.bare {
            self.push_frame(Frame::RawInline {
                tag: name,
                content: value.to_string(),
            });
            return;
        }
        let frame = match name.as_str() {
            "b" | "strong" => Frame::HtmlEmphasis {
                tag: name,
                kind: EmphasisKind::Strong,
                children: Vec::new(),
            },
            "i" | "em" => Frame::HtmlEmphasis {
                tag: name,
                kind: EmphasisKind::Italic,
                children: Vec::new(),
            },
            "mark" => Frame::HtmlEmphasis {
                tag: name,
                kind: EmphasisKind::Highlight,
                children: Vec::new(),
            },
            "sup" => Frame::HtmlEmphasis {
                tag: name,
                kind: EmphasisKind::Super,
                children: Vec::new(),
            },
            "sub" => Frame::HtmlEmphasis {
                tag: name,
                kind: EmphasisKind::Sub,
                children: Vec::new(),
            },
            "del" | "s" => Frame::HtmlEmphasis {
                tag: name,
                kind: EmphasisKind::Strike,
                children: Vec::new(),
            },
            "ins" => Frame::HtmlInsert {
                tag: name,
                children: Vec::new(),
            },
            "code" => Frame::HtmlCode {
                tag: name,
                content: String::new(),
            },
            _ => Frame::RawInline {
                tag: name,
                content: value.to_string(),
            },
        };
        self.push_frame(frame);
    }

    fn raw_inline(&mut self, content: String) {
        self.inline(InlineNode::RawInline(RawInline {
            format: "html".to_string(),
            content,
            injected: false,
            pos: None,
        }));
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
            Some(Frame::HtmlCode { content, .. }) => content.push_str(value),
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

        self.push_frame(frame);
    }

    /// Push a frame, unless it would nest past [`MAX_IMPORT_LEVELS`].
    ///
    /// A refusal sets `over_depth` instead of pushing, which stops the event loop
    /// - so the matching end event never arrives and the stack stays balanced.
    fn push_frame(&mut self, frame: Frame) {
        let (block, inline) = levels_added(&frame);
        if self.block_levels + block > MAX_IMPORT_LEVELS
            || self.inline_levels + inline > MAX_IMPORT_LEVELS
        {
            self.over_depth = true;
            return;
        }
        self.block_levels += block;
        self.inline_levels += inline;
        self.frames.push(frame);
    }

    fn pop_frame(&mut self) -> Option<Frame> {
        let frame = self.frames.pop()?;
        let (block, inline) = levels_added(&frame);
        self.block_levels -= block;
        self.inline_levels -= inline;
        Some(frame)
    }

    fn end(&mut self, _tag: TagEnd) {
        while matches!(
            self.frames.last(),
            Some(
                Frame::HtmlEmphasis { .. }
                    | Frame::HtmlCode { .. }
                    | Frame::HtmlInsert { .. }
                    | Frame::RawInline { .. }
            )
        ) {
            self.close();
        }
        self.close();
    }

    fn close(&mut self) {
        let Some(frame) = self.pop_frame() else {
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
                    // GFM has `[ ]` and `[x]` only.
                    task_state: None,
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
            Frame::RawHtml(content) => {
                let content = content.trim_end_matches('\n').to_string();
                // carve-js renders a block-level HTML element inline only when it
                // is a tight item's SOLE content (right after the marker, nothing
                // before it); a block element on a continuation line, after the
                // item's own text, stays a block. Match that: the item must have
                // collected neither a block nor any pending inline run yet.
                let inline_in_tight_item = matches!(
                    self.frames.last(),
                    Some(Frame::ListItem { loose: false, children, pending, .. })
                        if children.is_empty() && pending.is_empty()
                );
                if inline_in_tight_item {
                    self.raw_inline(content);
                } else {
                    self.block(BlockNode::RawBlock(RawBlock {
                        format: "html".to_string(),
                        content,
                        pos: None,
                    }));
                }
            }
            // A native inline-HTML frame reaching close() here was NOT closed by
            // its own end tag - a paragraph or the document ended first. Its open
            // tag never paired, so it is not a Carve construct: emit the bare open
            // tag as a raw-inline span and let its collected content stand, the
            // way carve-js leaves an unclosed `<b>` as `` `<b>`{=html} `` text.
            Frame::HtmlEmphasis { tag, children, .. } | Frame::HtmlInsert { tag, children, .. } => {
                self.raw_inline(format!("<{tag}>"));
                for child in children {
                    self.inline(child);
                }
            }
            Frame::HtmlCode { tag, content, .. } => {
                self.raw_inline(format!("<{tag}>"));
                if !content.is_empty() {
                    self.text(&content);
                }
            }
            Frame::RawInline { content, .. } => self.raw_inline(content),
            // Markdown emphasis IS Carve emphasis; only the spelling differs,
            // and the spelling belongs to the writer.
            Frame::Emphasis(kind, children) => self.inline(InlineNode::Emphasis(Emphasis {
                attrs: None,
                kind,
                children,
                pos: None,
            })),
            // Carve has no spelling for an empty destination, so the link is its
            // content and the image its alt (docs/html-import-contract.md).
            Frame::Link {
                href,
                title,
                children,
            } if is_empty_destination(&href) => match title {
                Some(title) => self.inline(titled_span(title, children)),
                None => {
                    for child in children {
                        self.inline(child);
                    }
                }
            },
            Frame::Image { src, title, alt } if is_empty_destination(&src) => {
                let content = if alt.is_empty() {
                    Vec::new()
                } else {
                    vec![InlineNode::text(alt)]
                };
                match title {
                    Some(title) => self.inline(titled_span(title, content)),
                    None => {
                        for child in content {
                            self.inline(child);
                        }
                    }
                }
            }
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
                    colspan: None,
                    rowspan: None,
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

    /// Close a native inline-HTML frame that its OWN matching end tag closed,
    /// building the Carve construct. The caller has confirmed the top frame is
    /// the one the end tag pairs with; anything reaching the generic `close()`
    /// was left unpaired and falls back to raw there instead.
    fn close_matched_html(&mut self) {
        match self.pop_frame() {
            Some(Frame::HtmlEmphasis { kind, children, .. }) => {
                self.inline(InlineNode::Emphasis(Emphasis {
                    attrs: None,
                    kind,
                    children,
                    pos: None,
                }));
            }
            Some(Frame::HtmlCode { content, .. }) => {
                self.inline(InlineNode::code(content, None));
            }
            Some(Frame::HtmlInsert { children, .. }) => {
                self.inline(InlineNode::CriticInsert(CriticInsert {
                    children,
                    attrs: None,
                    pos: None,
                }));
            }
            Some(other) => self.push_frame(other),
            None => {}
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
            | Some(Frame::HtmlEmphasis { children, .. })
            | Some(Frame::HtmlInsert { children, .. })
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

fn is_block_end(end: &TagEnd) -> bool {
    matches!(
        end,
        TagEnd::Paragraph
            | TagEnd::Heading(_)
            | TagEnd::BlockQuote(_)
            | TagEnd::CodeBlock
            | TagEnd::HtmlBlock
            | TagEnd::List(_)
            | TagEnd::Item
            | TagEnd::FootnoteDefinition
            | TagEnd::DefinitionList
            | TagEnd::DefinitionListTitle
            | TagEnd::DefinitionListDefinition
            | TagEnd::Table
            | TagEnd::TableHead
            | TagEnd::TableRow
            | TagEnd::TableCell
            | TagEnd::MetadataBlock(_)
    )
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

#[derive(Clone, Copy)]
struct HtmlTag<'a> {
    name: &'a str,
    closing: bool,
    self_closing: bool,
    bare: bool,
}

/// Read just enough of an HTML fragment to pair the events pulldown-cmark
/// emits. Declarations, comments and processing instructions deliberately do
/// not look like tags here: each is a complete standalone raw fragment.
fn html_tag(fragment: &str) -> Option<HtmlTag<'_>> {
    let bytes = fragment.as_bytes();
    if bytes.first() != Some(&b'<') || bytes.last() != Some(&b'>') {
        return None;
    }

    let mut cursor = 1;
    let closing = bytes.get(cursor) == Some(&b'/');
    if closing {
        cursor += 1;
    }
    if matches!(bytes.get(cursor), Some(b'!') | Some(b'?')) {
        return None;
    }
    let start = cursor;
    while bytes
        .get(cursor)
        .is_some_and(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b':' | b'_'))
    {
        cursor += 1;
    }
    if cursor == start {
        return None;
    }

    Some(HtmlTag {
        name: &fragment[start..cursor],
        closing,
        self_closing: fragment[..fragment.len() - 1].trim_end().ends_with('/'),
        // A bare open tag is `<name>` with nothing between the name and `>` - no
        // attributes, no interior whitespace. Only a bare native tag converts to
        // a Carve construct; an attributed one is kept verbatim as raw HTML so
        // the attributes are not silently dropped, matching carve-js.
        bare: !closing && bytes.get(cursor) == Some(&b'>'),
    })
}

fn is_void_html_tag(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "area"
            | "base"
            | "br"
            | "col"
            | "embed"
            | "hr"
            | "img"
            | "input"
            | "link"
            | "meta"
            | "param"
            | "source"
            | "track"
            | "wbr"
    )
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
    fn inline_html_uses_native_nodes_or_raw_html() {
        assert_eq!(markdown_to_carve("a <b>c</b> d"), "a *c* d\n");
        assert_eq!(
            markdown_to_carve("a <span>c</span> d"),
            "a `<span>c</span>`{=html} d\n"
        );
        assert_eq!(markdown_to_carve("use <code>x=1</code>"), "use `x=1`\n");
        assert_eq!(
            markdown_to_carve("before <!-- c --> after"),
            "before `<!-- c -->`{=html} after\n"
        );
    }

    #[test]
    fn html_highlight_uses_contextual_bracing() {
        assert_eq!(markdown_to_carve("a <mark>x</mark> b"), "a =x= b\n");
        assert_eq!(markdown_to_carve("a<mark>x</mark>b"), "a{=x=}b\n");
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
