//! JSON encoding/decoding for the public Carve AST exchange shape.
//!
//! This module intentionally has no serde dependency. It contains the small
//! JSON writer and parser needed for the schema-backed AST interchange format.

use std::collections::BTreeMap;
use std::fmt;

use crate::ast::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AstJsonError {
    message: String,
}

impl AstJsonError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for AstJsonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for AstJsonError {}

#[derive(Debug, Clone, PartialEq)]
enum Json {
    Null,
    Bool(bool),
    Number(i64),
    String(String),
    Array(Vec<Json>),
    Object(BTreeMap<String, Json>),
}

pub fn to_json(doc: &Document) -> String {
    let mut out = String::new();
    write_document(&mut out, doc);
    out
}

pub fn from_json(input: &str) -> Result<Document, AstJsonError> {
    let json = Parser::new(input).parse()?;
    let root = json.as_object("document root")?;
    let root_type = required_string(root, "document", "type")?;
    if root_type != "document" {
        return Err(AstJsonError::new(format!(
            "document.type must be \"document\", got {root_type:?}"
        )));
    }
    let src_byte_length = required_usize(root, "document", "srcByteLength")?;
    let mut children = Vec::new();
    let mut frontmatter_raw = None;
    let mut footnote_defs = BTreeMap::new();
    for child in required_array(root, "document", "children")? {
        let obj = child.as_object("document.children[]")?;
        match required_string(obj, "block node", "type")? {
            "frontmatter" => {
                if frontmatter_raw.is_some() {
                    return Err(AstJsonError::new("frontmatter appears more than once"));
                }
                frontmatter_raw = Some(Frontmatter {
                    format: required_string(obj, "frontmatter", "format")?.to_string(),
                    content: required_string(obj, "frontmatter", "content")?.to_string(),
                    pos: optional_pos(obj, "frontmatter")?,
                });
            }
            "footnote" => {
                let label = obj
                    .get("label")
                    .or_else(|| obj.get("id"))
                    .ok_or_else(|| AstJsonError::new("footnote.label is required"))?
                    .as_string("footnote.label")?
                    .to_string();
                let blocks = decode_blocks(required_array(obj, "footnote", "children")?)?;
                footnote_defs.insert(label, blocks);
            }
            _ => children.push(decode_block(child)?),
        }
    }
    Ok(Document {
        // Rebuilt from the raw block, with the same function the parser uses.
        // The wire carries the raw text only, so leaving this empty would make
        // a decoded document differ from the parsed one in a field neither the
        // format nor the consumer asked about.
        frontmatter: frontmatter_raw
            .as_ref()
            .map(|raw: &Frontmatter| crate::parse::frontmatter_map(&raw.format, &raw.content))
            .unwrap_or_default(),
        frontmatter_raw,
        footnote_defs,
        children,
        source_len: src_byte_length,
    })
}

struct Writer<'a> {
    out: &'a mut String,
    first: bool,
}

impl<'a> Writer<'a> {
    fn new(out: &'a mut String) -> Self {
        out.push('{');
        Self { out, first: true }
    }

    fn field(&mut self, name: &str, write: impl FnOnce(&mut String)) {
        if !self.first {
            self.out.push(',');
        }
        self.first = false;
        write_string(self.out, name);
        self.out.push(':');
        write(self.out);
    }

    fn finish(self) {
        self.out.push('}');
    }
}

fn write_document(out: &mut String, doc: &Document) {
    let mut w = Writer::new(out);
    w.field("type", |out| write_string(out, "document"));
    w.field("children", |out| {
        out.push('[');
        let mut first = true;
        if let Some(raw) = &doc.frontmatter_raw {
            write_comma(out, &mut first);
            write_frontmatter(out, raw);
        }
        for child in &doc.children {
            write_comma(out, &mut first);
            write_block(out, child);
        }
        // Definitions come AFTER the content and in source order (PART 12 §7).
        // The map is keyed by label, so sort by the first placed body block;
        // an unplaced body falls back to label order rather than inventing a
        // source position.
        let mut footnote_defs: Vec<_> = doc.footnote_defs.iter().collect();
        footnote_defs.sort_by_key(|(label, children)| {
            (
                first_block_pos(children)
                    .map(|pos| pos.start_offset)
                    .unwrap_or(usize::MAX),
                label.as_str(),
            )
        });
        for (label, children) in footnote_defs {
            write_comma(out, &mut first);
            write_footnote_def(out, label, children);
        }
        out.push(']');
    });
    w.field("srcByteLength", |out| write_usize(out, doc.source_len));
    w.finish();
}

fn first_block_pos(children: &[BlockNode]) -> Option<&Pos> {
    children.iter().find_map(block_pos)
}

fn block_pos(node: &BlockNode) -> Option<&Pos> {
    match node {
        BlockNode::Heading(n) => n.pos.as_ref(),
        BlockNode::Paragraph(n) => n.pos.as_ref(),
        BlockNode::CodeBlock(n) => n.pos.as_ref(),
        BlockNode::List(n) => n.pos.as_ref(),
        BlockNode::BlockQuote(n) => n.pos.as_ref(),
        BlockNode::Table(n) => n.pos.as_ref(),
        BlockNode::Admonition(n) => n.pos.as_ref(),
        BlockNode::Div(n) => n.pos.as_ref(),
        BlockNode::LineBlock(n) => n.pos.as_ref(),
        BlockNode::DefinitionList(n) => n.pos.as_ref(),
        BlockNode::Figure(n) => n.pos.as_ref(),
        BlockNode::AbbreviationDef(n) => n.pos.as_ref(),
        BlockNode::RawBlock(n) => n.pos.as_ref(),
        BlockNode::Comment(n) => n.pos.as_ref(),
        BlockNode::Extension(n) => n.pos.as_ref(),
        BlockNode::BlockImage(n) => n.pos.as_ref(),
        BlockNode::ThematicBreak(n) => n.pos.as_ref(),
    }
}

fn write_frontmatter(out: &mut String, raw: &Frontmatter) {
    let mut w = Writer::new(out);
    w.field("type", |out| write_string(out, "frontmatter"));
    w.field("format", |out| write_string(out, &raw.format));
    w.field("content", |out| write_string(out, &raw.content));
    write_pos_field(&mut w, &raw.pos);
    w.finish();
}

fn write_footnote_def(out: &mut String, label: &str, children: &[BlockNode]) {
    let mut w = Writer::new(out);
    w.field("type", |out| write_string(out, "footnote"));
    w.field("label", |out| write_string(out, label));
    w.field("children", |out| write_blocks(out, children));
    let pos = first_block_pos(children).copied();
    write_pos_field(&mut w, &pos);
    w.finish();
}

fn write_block(out: &mut String, node: &BlockNode) {
    match node {
        BlockNode::Heading(n) => {
            let mut w = typed(out, "heading");
            w.field("level", |out| write_usize(out, n.level as usize));
            w.field("children", |out| write_inlines(out, &n.children));
            write_attrs_field(&mut w, &n.attrs);
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        BlockNode::Paragraph(n) => write_paragraph(out, n),
        BlockNode::CodeBlock(n) => write_code_block(out, n),
        BlockNode::List(n) => {
            let mut w = typed(out, "list");
            w.field("ordered", |out| write_bool(out, n.ordered));
            w.field("tight", |out| write_bool(out, n.tight));
            w.field("items", |out| write_array(out, &n.items, write_list_item));
            if let Some(start) = n.start {
                w.field("start", |out| write_usize(out, start));
            }
            if let Some(ol_type) = n.ol_type {
                w.field("olType", |out| write_string(out, ol_type_json(ol_type)));
            }
            if let Some(delim) = n.delim {
                w.field("delim", |out| write_string(out, &delim.to_string()));
            }
            if let Some(bullet) = n.bullet_char {
                w.field("bulletChar", |out| write_string(out, &bullet.to_string()));
            }
            write_attrs_field(&mut w, &n.attrs);
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        BlockNode::BlockQuote(n) => write_block_quote(out, n),
        BlockNode::Table(n) => write_table(out, n),
        BlockNode::Admonition(n) => {
            let mut w = typed(out, "admonition");
            w.field("kind", |out| write_string(out, &n.kind));
            if let Some(title) = &n.title {
                w.field("title", |out| write_inlines(out, title));
            }
            if let Some(label) = &n.label {
                w.field("label", |out| write_string(out, label));
            }
            w.field("children", |out| write_blocks(out, &n.children));
            write_attrs_field(&mut w, &n.attrs);
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        BlockNode::Div(n) => {
            let mut w = typed(out, "div");
            w.field("children", |out| write_blocks(out, &n.children));
            if let Some(label) = &n.label {
                w.field("label", |out| write_string(out, label));
            }
            write_attrs_field(&mut w, &n.attrs);
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        BlockNode::LineBlock(n) => {
            let mut w = typed(out, "line_block");
            w.field("children", |out| write_blocks(out, &n.children));
            write_attrs_field(&mut w, &n.attrs);
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        BlockNode::DefinitionList(n) => {
            let mut w = typed(out, "definition_list");
            w.field("items", |out| write_definition_entries(out, &n.items));
            write_attrs_field(&mut w, &n.attrs);
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        BlockNode::Figure(n) => {
            let mut w = typed(out, "figure");
            w.field("target", |out| write_figure_target(out, &n.target));
            w.field("caption", |out| write_inlines(out, &n.caption));
            write_attrs_field(&mut w, &n.attrs);
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        BlockNode::AbbreviationDef(n) => {
            let mut w = typed(out, "abbreviation_def");
            w.field("abbr", |out| write_string(out, &n.abbr));
            w.field("expansion", |out| write_string(out, &n.expansion));
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        BlockNode::RawBlock(n) => {
            let mut w = typed(out, "raw_block");
            w.field("format", |out| write_string(out, &n.format));
            w.field("content", |out| write_string(out, &n.content));
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        BlockNode::Comment(n) => {
            let mut w = typed(out, "comment");
            w.field("block", |out| write_bool(out, n.block));
            w.field("content", |out| write_string(out, &n.content));
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        BlockNode::Extension(n) => {
            let mut w = typed(out, "block_extension");
            w.field("name", |out| write_string(out, &n.name));
            w.field("children", |out| write_blocks(out, &n.children));
            if let Some(summary) = &n.summary {
                w.field("summary", |out| write_inlines(out, summary));
            }
            if let Some(label) = &n.label {
                w.field("label", |out| write_string(out, label));
            }
            write_attrs_field(&mut w, &n.attrs);
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        BlockNode::BlockImage(n) => write_image(out, n),
        BlockNode::ThematicBreak(n) => {
            let mut w = typed(out, "thematic_break");
            write_attrs_field(&mut w, &n.attrs);
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
    }
}

fn write_paragraph(out: &mut String, n: &Paragraph) {
    let mut w = typed(out, "paragraph");
    w.field("children", |out| write_inlines(out, &n.children));
    write_attrs_field(&mut w, &n.attrs);
    write_pos_field(&mut w, &n.pos);
    w.finish();
}

fn write_code_block(out: &mut String, n: &CodeBlock) {
    let mut w = typed(out, "code_block");
    w.field("content", |out| write_string(out, &n.content));
    if let Some(lang) = &n.lang {
        w.field("lang", |out| write_string(out, lang));
    }
    if let Some(title) = &n.title {
        w.field("header", |out| write_string(out, title));
    }
    if let Some(label) = &n.label {
        w.field("label", |out| write_string(out, label));
    }
    write_attrs_field(&mut w, &n.attrs);
    write_pos_field(&mut w, &n.pos);
    w.finish();
}

fn write_block_quote(out: &mut String, n: &BlockQuote) {
    let mut w = typed(out, "block_quote");
    w.field("children", |out| write_blocks(out, &n.children));
    if let Some(attribution) = &n.attribution {
        w.field("attribution", |out| write_inlines(out, attribution));
    }
    write_attrs_field(&mut w, &n.attrs);
    write_pos_field(&mut w, &n.pos);
    w.finish();
}

fn write_list_item(out: &mut String, n: &ListItem) {
    let mut w = typed(out, "list_item");
    w.field("children", |out| write_blocks(out, &n.children));
    if let Some(checked) = n.checked {
        w.field("checked", |out| write_bool(out, checked));
    }
    write_attrs_field(&mut w, &n.attrs);
    write_pos_field(&mut w, &n.pos);
    w.finish();
}

fn write_table(out: &mut String, n: &Table) {
    let mut w = typed(out, "table");
    w.field("rows", |out| write_array(out, &n.rows, write_table_row));
    if let Some(caption) = &n.caption {
        w.field("caption", |out| write_inlines(out, caption));
    }
    write_attrs_field(&mut w, &n.attrs);
    write_pos_field(&mut w, &n.pos);
    w.finish();
}

fn write_table_row(out: &mut String, n: &TableRow) {
    let mut w = typed(out, "table_row");
    w.field("cells", |out| write_array(out, &n.cells, write_table_cell));
    write_attrs_field(&mut w, &n.attrs);
    write_pos_field(&mut w, &n.pos);
    w.finish();
}

fn write_table_cell(out: &mut String, n: &TableCell) {
    let mut w = typed(out, "table_cell");
    w.field("header", |out| write_bool(out, n.header));
    w.field("children", |out| write_inlines(out, &n.children));
    if let Some(span) = n.span {
        w.field("span", |out| {
            write_string(
                out,
                match span {
                    TableCellSpan::Rowspan => "rowspan",
                    TableCellSpan::Colspan => "colspan",
                },
            )
        });
    }
    if let Some(align) = n.align {
        w.field("align", |out| write_string(out, align_json(align)));
    }
    write_attrs_field(&mut w, &n.attrs);
    write_pos_field(&mut w, &n.pos);
    w.finish();
}

/// A definition list's entries, FLATTENED into the `<dt>` / `<dd>` sequence the
/// wire carries (PART 12).
///
/// This engine groups terms with their definitions, which is convenient in
/// memory and not something to publish: `definition_term` and
/// `definition_description` are in the normative block vocabulary, so a profile
/// can name them, and a plain `{terms, definitions}` object can carry no `pos` -
/// leaving a term the only content in a serialized document an editor cannot
/// navigate to.
///
/// The grouping was also not AGREED. Given `:: a` / `:: b` / `:  x` / `:  y`
/// this engine produced three entries and carve-js produced one, while both
/// rendered the same `<dl>`. A structure two producers disagree about, which no
/// output depends on, is an internal.
fn write_definition_entries(out: &mut String, items: &[DefinitionItem]) {
    out.push('[');
    let mut first = true;
    for item in items {
        for term in &item.terms {
            write_comma(out, &mut first);
            let mut w = typed(out, "definition_term");
            w.field("children", |out| write_inlines(out, &term.children));
            write_attrs_field(&mut w, &term.attrs);
            write_pos_field(&mut w, &term.pos);
            w.finish();
        }
        for definition in &item.definitions {
            write_comma(out, &mut first);
            let mut w = typed(out, "definition_description");
            w.field("children", |out| write_blocks(out, &definition.children));
            write_attrs_field(&mut w, &definition.attrs);
            write_pos_field(&mut w, &definition.pos);
            w.finish();
        }
    }
    out.push(']');
}

fn write_figure_target(out: &mut String, target: &FigureTarget) {
    match target {
        FigureTarget::Image(n) => write_image(out, n),
        FigureTarget::BlockQuote(n) => write_block_quote(out, n),
        FigureTarget::Table(n) => write_table(out, n),
        FigureTarget::CodeBlock(n) => write_code_block(out, n),
        FigureTarget::Paragraph(n) => write_paragraph(out, n),
    }
}

fn write_inline(out: &mut String, node: &InlineNode) {
    match node {
        InlineNode::Text(n) => {
            let mut w = typed(out, "text");
            w.field("value", |out| write_string(out, &n.value));
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        InlineNode::EscapedText(n) => {
            let mut w = typed(out, "escaped_text");
            w.field("value", |out| write_string(out, &n.value));
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        InlineNode::SmartPunctuation(n) => {
            let mut w = typed(out, "smart_punctuation");
            w.field("kind", |out| write_string(out, &n.kind));
            w.field("value", |out| write_string(out, &n.value));
            if let Some(glyph) = &n.glyph {
                w.field("glyph", |out| write_string(out, glyph));
            }
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        InlineNode::Emphasis(n) => {
            let mut w = typed(out, emphasis_type(n.kind));
            w.field("children", |out| write_inlines(out, &n.children));
            if n.kind == EmphasisKind::BoldItalic {
                w.field("boldItalic", |out| write_bool(out, true));
            }
            write_attrs_field(&mut w, &n.attrs);
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        InlineNode::Code(n) => {
            let mut w = typed(out, "code");
            w.field("value", |out| write_string(out, &n.value));
            write_attrs_field(&mut w, &n.attrs);
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        InlineNode::Link(n) => write_link(out, n),
        InlineNode::Image(n) => write_image(out, n),
        InlineNode::Span(n) => {
            let mut w = typed(out, "span");
            w.field("children", |out| write_inlines(out, &n.children));
            write_attrs_field(&mut w, &n.attrs);
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        InlineNode::Math(n) => {
            let mut w = typed(out, "math");
            w.field("display", |out| write_bool(out, n.display));
            w.field("content", |out| write_string(out, &n.content));
            write_attrs_field(&mut w, &n.attrs);
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        InlineNode::RawInline(n) => {
            let mut w = typed(out, "raw_inline");
            w.field("format", |out| write_string(out, &n.format));
            w.field("content", |out| write_string(out, &n.content));
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        InlineNode::LiteralInline(n) => {
            let mut w = typed(out, "literal_inline");
            w.field("content", |out| write_string(out, &n.content));
            write_attrs_field(&mut w, &n.attrs);
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        InlineNode::Symbol(n) => {
            let mut w = typed(out, "symbol");
            w.field("name", |out| write_string(out, &n.name));
            write_attrs_field(&mut w, &n.attrs);
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        InlineNode::AutoLink(n) => {
            let mut w = typed(out, "autolink");
            w.field("href", |out| write_string(out, &n.href));
            w.field("text", |out| write_string(out, &n.text));
            write_attrs_field(&mut w, &n.attrs);
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        InlineNode::CrossRef(n) => {
            let mut w = typed(out, "heading_ref");
            w.field("target", |out| write_string(out, &n.target));
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        InlineNode::CaptionNumber(n) => {
            let mut w = typed(out, "caption_number");
            if let Some(number) = n.number {
                w.field("n", |out| write_usize(out, number));
            }
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        InlineNode::Mention(n) => {
            let mut w = typed(out, "mention");
            w.field("user", |out| write_string(out, &n.user));
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        InlineNode::Tag(n) => {
            let mut w = typed(out, "tag");
            w.field("name", |out| write_string(out, &n.name));
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        InlineNode::CitationGroup(n) => {
            let mut w = typed(out, "citation_group");
            w.field("items", |out| write_array(out, &n.items, write_citation));
            w.field("raw", |out| write_string(out, &n.raw));
            if n.integral {
                w.field("mode", |out| write_string(out, "integral"));
            }
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        InlineNode::Extension(n) => {
            let mut w = typed(out, "inline_extension");
            w.field("name", |out| write_string(out, &n.name));
            w.field("content", |out| write_inlines(out, &n.children));
            write_attrs_field(&mut w, &n.attrs);
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        InlineNode::Abbreviation(n) => {
            let mut w = typed(out, "abbreviation");
            w.field("abbr", |out| write_string(out, &n.abbr));
            w.field("expansion", |out| write_string(out, &n.expansion));
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        InlineNode::Footnote(n) => {
            if let Some(inline) = &n.inline {
                let mut w = typed(out, "inline_footnote");
                w.field("inline", |out| write_inlines(out, inline));
                if let Some(number) = n.number {
                    w.field("number", |out| write_usize(out, number));
                }
                if let Some(ref_id) = &n.ref_id {
                    w.field("refId", |out| write_string(out, ref_id));
                }
                write_attrs_field(&mut w, &n.attrs);
                write_pos_field(&mut w, &n.pos);
                w.finish();
            } else {
                let mut w = typed(out, "footnote_ref");
                if let Some(id) = &n.id {
                    w.field("id", |out| write_string(out, id));
                }
                if let Some(number) = n.number {
                    w.field("number", |out| write_usize(out, number));
                }
                if let Some(ref_id) = &n.ref_id {
                    w.field("refId", |out| write_string(out, ref_id));
                }
                write_attrs_field(&mut w, &n.attrs);
                write_pos_field(&mut w, &n.pos);
                w.finish();
            }
        }
        InlineNode::SoftBreak(n) => {
            let mut w = typed(out, "soft_break");
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        InlineNode::HardBreak(n) => {
            let mut w = typed(out, "hard_break");
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        InlineNode::CriticInsert(n) => {
            let mut w = typed(out, "insert");
            w.field("children", |out| write_inlines(out, &n.children));
            write_attrs_field(&mut w, &n.attrs);
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        InlineNode::CriticDelete(n) => {
            let mut w = typed(out, "delete");
            w.field("children", |out| write_inlines(out, &n.children));
            write_attrs_field(&mut w, &n.attrs);
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        InlineNode::CriticSubstitute(n) => {
            let mut w = typed(out, "substitution");
            w.field("oldText", |out| write_string(out, &n.old_text));
            w.field("newText", |out| write_string(out, &n.new_text));
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        InlineNode::CriticComment(n) => {
            let mut w = typed(out, "critic_comment");
            w.field("text", |out| write_string(out, &n.text));
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
    }
}

fn write_link(out: &mut String, n: &Link) {
    let mut w = typed(out, "link");
    w.field("href", |out| write_string(out, &n.href));
    w.field("children", |out| write_inlines(out, &n.children));
    if let Some(title) = &n.title {
        w.field("title", |out| write_string(out, title));
    }
    if let Some(ref_label) = &n.ref_label {
        w.field("ref", |out| write_string(out, ref_label));
    }
    if let Some(raw_ref) = &n.raw_ref {
        w.field("rawRef", |out| write_string(out, raw_ref));
    }
    if n.from_crossref {
        w.field("fromCrossref", |out| write_bool(out, true));
    }
    write_attrs_field(&mut w, &n.attrs);
    write_pos_field(&mut w, &n.pos);
    w.finish();
}

fn write_image(out: &mut String, n: &Image) {
    let mut w = typed(out, "image");
    w.field("src", |out| write_string(out, &n.src));
    w.field("alt", |out| write_string(out, &n.alt));
    if let Some(title) = &n.title {
        w.field("title", |out| write_string(out, title));
    }
    if let Some(ref_label) = &n.ref_label {
        w.field("ref", |out| write_string(out, ref_label));
    }
    if let Some(raw_ref) = &n.raw_ref {
        w.field("rawRef", |out| write_string(out, raw_ref));
    }
    write_attrs_field(&mut w, &n.attrs);
    write_pos_field(&mut w, &n.pos);
    w.finish();
}

fn write_citation(out: &mut String, n: &Citation) {
    let mut w = Writer::new(out);
    w.field("key", |out| write_string(out, &n.key));
    if let Some(prefix) = &n.prefix {
        w.field("prefix", |out| write_inlines(out, prefix));
    }
    if let Some(locator) = &n.locator {
        w.field("locator", |out| write_inlines(out, locator));
    }
    if let Some(locator_label) = &n.locator_label {
        w.field("locatorLabel", |out| write_string(out, locator_label));
    }
    if let Some(locator_value) = &n.locator_value {
        w.field("locatorValue", |out| write_string(out, locator_value));
    }
    if let Some(suffix) = &n.suffix {
        w.field("suffix", |out| write_inlines(out, suffix));
    }
    w.field("suppressAuthor", |out| write_bool(out, n.suppress_author));
    if let Some(number) = n.number {
        w.field("number", |out| write_usize(out, number));
    }
    if let Some(use_index) = n.use_index {
        w.field("useIndex", |out| write_usize(out, use_index));
    }
    w.finish();
}

fn typed<'a>(out: &'a mut String, ty: &str) -> Writer<'a> {
    let mut w = Writer::new(out);
    w.field("type", |out| write_string(out, ty));
    w
}

fn write_attrs_field(w: &mut Writer<'_>, attrs: &Option<Attrs>) {
    if let Some(attrs) = attrs {
        w.field("attrs", |out| write_attrs(out, attrs));
    }
}

fn write_attrs(out: &mut String, attrs: &Attrs) {
    let mut w = Writer::new(out);
    if let Some(id) = &attrs.id {
        w.field("id", |out| write_string(out, id));
    }
    if !attrs.classes.is_empty() {
        w.field("classes", |out| write_string_array(out, &attrs.classes));
    }
    if !attrs.key_values.is_empty() {
        w.field("keyValues", |out| {
            let mut w = Writer::new(out);
            for (key, value) in &attrs.key_values {
                w.field(key, |out| write_string(out, value));
            }
            w.finish();
        });
    }
    if !attrs.order.is_empty() {
        w.field("order", |out| {
            out.push('[');
            let mut first = true;
            for slot in &attrs.order {
                write_comma(out, &mut first);
                match slot {
                    AttrSlot::Id => write_string(out, "#id"),
                    AttrSlot::Class => write_string(out, ".class"),
                    AttrSlot::Key(key) => write_string(out, key),
                }
            }
            out.push(']');
        });
    }
    w.finish();
}

fn write_pos_field(w: &mut Writer<'_>, pos: &Option<Pos>) {
    if let Some(pos) = pos {
        w.field("pos", |out| write_pos(out, pos));
    }
}

fn write_pos(out: &mut String, pos: &Pos) {
    let mut w = Writer::new(out);
    w.field("startLine", |out| write_usize(out, pos.start_line));
    w.field("endLine", |out| write_usize(out, pos.end_line));
    w.field("startColumn", |out| write_usize(out, pos.start_column));
    w.field("endColumn", |out| write_usize(out, pos.end_column));
    w.field("startOffset", |out| write_usize(out, pos.start_offset));
    w.field("endOffset", |out| write_usize(out, pos.end_offset));
    w.finish();
}

fn write_blocks(out: &mut String, blocks: &[BlockNode]) {
    write_array(out, blocks, write_block);
}

fn write_inlines(out: &mut String, inlines: &[InlineNode]) {
    write_array(out, inlines, write_inline);
}

fn write_string_array(out: &mut String, values: &[String]) {
    write_array(out, values, |out, s| write_string(out, s));
}

fn write_array<T>(out: &mut String, values: &[T], mut f: impl FnMut(&mut String, &T)) {
    out.push('[');
    let mut first = true;
    for value in values {
        write_comma(out, &mut first);
        f(out, value);
    }
    out.push(']');
}

fn write_comma(out: &mut String, first: &mut bool) {
    if *first {
        *first = false;
    } else {
        out.push(',');
    }
}

fn write_string(out: &mut String, value: &str) {
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            c if c <= '\u{1f}' => {
                out.push_str("\\u");
                out.push(hex((c as u32 >> 12) & 0xf));
                out.push(hex((c as u32 >> 8) & 0xf));
                out.push(hex((c as u32 >> 4) & 0xf));
                out.push(hex(c as u32 & 0xf));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

fn hex(n: u32) -> char {
    char::from_digit(n, 16).unwrap()
}

fn write_bool(out: &mut String, value: bool) {
    out.push_str(if value { "true" } else { "false" });
}

fn write_usize(out: &mut String, value: usize) {
    out.push_str(&value.to_string());
}

fn ol_type_json(t: OrderedListType) -> &'static str {
    match t {
        OrderedListType::LowerAlpha => "a",
        OrderedListType::UpperAlpha => "A",
        OrderedListType::LowerRoman => "i",
        OrderedListType::UpperRoman => "I",
    }
}

fn align_json(t: TableAlign) -> &'static str {
    match t {
        TableAlign::Left => "left",
        TableAlign::Right => "right",
        TableAlign::Center => "center",
    }
}

fn emphasis_type(t: EmphasisKind) -> &'static str {
    match t {
        EmphasisKind::Italic => "emphasis",
        EmphasisKind::Strong | EmphasisKind::BoldItalic => "strong",
        EmphasisKind::Underline => "underline",
        EmphasisKind::Strike => "strike",
        EmphasisKind::Super => "superscript",
        EmphasisKind::Sub => "subscript",
        EmphasisKind::Highlight => "highlight",
    }
}

fn decode_blocks(values: &[Json]) -> Result<Vec<BlockNode>, AstJsonError> {
    values.iter().map(decode_block).collect()
}

fn decode_inlines(values: &[Json]) -> Result<Vec<InlineNode>, AstJsonError> {
    values.iter().map(decode_inline).collect()
}

fn decode_block(value: &Json) -> Result<BlockNode, AstJsonError> {
    let obj = value.as_object("block node")?;
    let ty = required_string(obj, "block node", "type")?;
    match ty {
        "paragraph" => Ok(BlockNode::Paragraph(Paragraph {
            attrs: optional_attrs(obj)?,
            children: decode_inlines(required_array(obj, "paragraph", "children")?)?,
            // Parse-internal and NOT on the wire: it records whether the
            // paragraph's first line sat at its container's content column, and
            // the only reader is the image-figure promotion, which runs during
            // parsing and has already run by the time anything is serialized.
            // The default is the conservative answer - `true` would be a claim
            // about the source this tree no longer has, and the one thing it
            // could still do is promote a figure the author did not write.
            at_content_column: false,
            pos: optional_pos(obj, "paragraph")?,
        })),
        "heading" => Ok(BlockNode::Heading(Heading {
            attrs: optional_attrs(obj)?,
            level: required_usize(obj, "heading", "level")? as u8,
            children: decode_inlines(required_array(obj, "heading", "children")?)?,
            pos: optional_pos(obj, "heading")?,
        })),
        "block_quote" => Ok(BlockNode::BlockQuote(BlockQuote {
            attrs: optional_attrs(obj)?,
            children: decode_blocks(required_array(obj, "block_quote", "children")?)?,
            attribution: optional_inlines(obj, "attribution")?,
            pos: optional_pos(obj, "block_quote")?,
        })),
        "list" => Ok(BlockNode::List(List {
            attrs: optional_attrs(obj)?,
            ordered: required_bool(obj, "list", "ordered")?,
            tight: required_bool(obj, "list", "tight")?,
            items: required_array(obj, "list", "items")?
                .iter()
                .map(decode_list_item)
                .collect::<Result<_, _>>()?,
            start: optional_usize(obj, "start")?,
            ol_type: optional_string(obj, "olType")?
                .map(decode_ol_type)
                .transpose()?,
            delim: optional_marker_char(obj, "delim")?,
            bullet_char: optional_marker_char(obj, "bulletChar")?,
            pos: optional_pos(obj, "list")?,
        })),
        "code_block" => Ok(BlockNode::CodeBlock(CodeBlock {
            attrs: optional_attrs(obj)?,
            lang: optional_string(obj, "lang")?.map(str::to_string),
            title: optional_string(obj, "header")?
                .or(optional_string(obj, "title")?)
                .map(str::to_string),
            label: optional_string(obj, "label")?.map(str::to_string),
            content: required_string(obj, "code_block", "content")?.to_string(),
            pos: optional_pos(obj, "code_block")?,
        })),
        "thematic_break" => Ok(BlockNode::ThematicBreak(ThematicBreak {
            attrs: optional_attrs(obj)?,
            pos: optional_pos(obj, "thematic_break")?,
        })),
        "table" => Ok(BlockNode::Table(decode_table(obj)?)),
        "table_row" => Err(AstJsonError::new(
            "table_row is only valid inside table.rows",
        )),
        "table_cell" => Err(AstJsonError::new(
            "table_cell is only valid inside table_row.cells",
        )),
        "admonition" => Ok(BlockNode::Admonition(Admonition {
            attrs: optional_attrs(obj)?,
            kind: required_string(obj, "admonition", "kind")?.to_string(),
            title: optional_inlines(obj, "title")?,
            label: optional_string(obj, "label")?.map(str::to_string),
            children: decode_blocks(required_array(obj, "admonition", "children")?)?,
            pos: optional_pos(obj, "admonition")?,
        })),
        "div" => Ok(BlockNode::Div(Div {
            attrs: optional_attrs(obj)?,
            label: optional_string(obj, "label")?.map(str::to_string),
            children: decode_blocks(required_array(obj, "div", "children")?)?,
            pos: optional_pos(obj, "div")?,
        })),
        "line_block" => Ok(BlockNode::LineBlock(LineBlock {
            attrs: optional_attrs(obj)?,
            children: decode_blocks(required_array(obj, "line_block", "children")?)?,
            pos: optional_pos(obj, "line_block")?,
        })),
        "definition_list" => Ok(BlockNode::DefinitionList(DefinitionList {
            attrs: optional_attrs(obj)?,
            items: decode_definition_entries(required_array(obj, "definition_list", "items")?)?,
            pos: optional_pos(obj, "definition_list")?,
        })),
        "figure" => Ok(BlockNode::Figure(Figure {
            attrs: optional_attrs(obj)?,
            target: decode_figure_target(required_value(obj, "figure", "target")?)?,
            caption: decode_inlines(required_array(obj, "figure", "caption")?)?,
            pos: optional_pos(obj, "figure")?,
        })),
        "abbreviation_def" => Ok(BlockNode::AbbreviationDef(AbbreviationDef {
            abbr: required_string(obj, "abbreviation_def", "abbr")?.to_string(),
            expansion: required_string(obj, "abbreviation_def", "expansion")?.to_string(),
            pos: optional_pos(obj, "abbreviation_def")?,
        })),
        "raw_block" => Ok(BlockNode::RawBlock(RawBlock {
            format: required_string(obj, "raw_block", "format")?.to_string(),
            content: required_string(obj, "raw_block", "content")?.to_string(),
            pos: optional_pos(obj, "raw_block")?,
        })),
        "comment" => Ok(BlockNode::Comment(Comment {
            block: required_bool(obj, "comment", "block")?,
            content: required_string(obj, "comment", "content")?.to_string(),
            pos: optional_pos(obj, "comment")?,
        })),
        "block_extension" => Ok(BlockNode::Extension(BlockExtension {
            attrs: optional_attrs(obj)?,
            name: required_string(obj, "block_extension", "name")?.to_string(),
            children: decode_blocks(required_array(obj, "block_extension", "children")?)?,
            summary: optional_inlines(obj, "summary")?,
            label: optional_string(obj, "label")?.map(str::to_string),
            pos: optional_pos(obj, "block_extension")?,
        })),
        "image" => Ok(BlockNode::BlockImage(decode_image(obj)?)),
        "frontmatter" => Err(AstJsonError::new(
            "frontmatter is only valid as a document child",
        )),
        "footnote" => Err(AstJsonError::new(
            "footnote is only valid as a document child",
        )),
        other => Err(AstJsonError::new(format!(
            "unknown block node type {other:?}"
        ))),
    }
}

fn decode_list_item(value: &Json) -> Result<ListItem, AstJsonError> {
    let obj = value.as_object("list_item")?;
    expect_type(obj, "list_item")?;
    Ok(ListItem {
        attrs: optional_attrs(obj)?,
        checked: optional_bool(obj, "checked")?,
        children: decode_blocks(required_array(obj, "list_item", "children")?)?,
        pos: optional_pos(obj, "list_item")?,
    })
}

fn decode_table(obj: &BTreeMap<String, Json>) -> Result<Table, AstJsonError> {
    Ok(Table {
        attrs: optional_attrs(obj)?,
        caption: optional_inlines(obj, "caption")?,
        rows: required_array(obj, "table", "rows")?
            .iter()
            .map(decode_table_row)
            .collect::<Result<_, _>>()?,
        pos: optional_pos(obj, "table")?,
    })
}

fn decode_table_row(value: &Json) -> Result<TableRow, AstJsonError> {
    let obj = value.as_object("table_row")?;
    expect_type(obj, "table_row")?;
    Ok(TableRow {
        cells: required_array(obj, "table_row", "cells")?
            .iter()
            .map(decode_table_cell)
            .collect::<Result<_, _>>()?,
        attrs: optional_attrs(obj)?,
        pos: optional_pos(obj, "table_row")?,
    })
}

fn decode_table_cell(value: &Json) -> Result<TableCell, AstJsonError> {
    let obj = value.as_object("table_cell")?;
    expect_type(obj, "table_cell")?;
    Ok(TableCell {
        header: required_bool(obj, "table_cell", "header")?,
        span: optional_string(obj, "span")?
            .map(decode_cell_span)
            .transpose()?,
        align: optional_string(obj, "align")?
            .map(decode_table_align)
            .transpose()?,
        attrs: optional_attrs(obj)?,
        children: decode_inlines(required_array(obj, "table_cell", "children")?)?,
        pos: optional_pos(obj, "table_cell")?,
    })
}

/// The flat `<dt>` / `<dd>` sequence back to this engine's grouped entries.
///
/// The grouping rule is the renderer's, which is the only one all three engines
/// agree on: a run of terms opens an entry, the descriptions after it belong to
/// it, and the next term after a description starts the next entry.
///
/// A payload in the OLD `{terms, definitions}` form still decodes - trees in
/// that shape are stored, and this engine wrote them.
fn decode_definition_entries(values: &[Json]) -> Result<Vec<DefinitionItem>, AstJsonError> {
    let mut items: Vec<DefinitionItem> = Vec::new();

    for value in values {
        let obj = value.as_object("definition_list.items[]")?;
        if obj.contains_key("terms") || obj.contains_key("definitions") {
            items.push(decode_definition_item(value)?);
            continue;
        }

        let ty = required_string(obj, "definition_list.items[]", "type")?;
        match ty {
            "definition_term" => {
                let term = DefinitionTerm {
                    attrs: optional_attrs(obj)?,
                    children: decode_inlines(required_array(obj, "definition_term", "children")?)?,
                    pos: optional_pos(obj, "definition_term")?,
                };
                let start_new = items
                    .last()
                    .map(|item| !item.definitions.is_empty())
                    .unwrap_or(true);
                if start_new {
                    items.push(DefinitionItem {
                        terms: Vec::new(),
                        definitions: Vec::new(),
                        pos: None,
                    });
                }
                items
                    .last_mut()
                    .expect("an entry was just pushed")
                    .terms
                    .push(term);
            }
            "definition_description" => {
                let definition = DefinitionDef {
                    attrs: optional_attrs(obj)?,
                    children: decode_blocks(required_array(
                        obj,
                        "definition_description",
                        "children",
                    )?)?,
                    pos: optional_pos(obj, "definition_description")?,
                };
                if items.is_empty() {
                    // A description with no term before it: the parser cannot
                    // produce one, a hand-built payload can, and dropping it
                    // would lose content the caller handed us.
                    items.push(DefinitionItem {
                        terms: Vec::new(),
                        definitions: Vec::new(),
                        pos: None,
                    });
                }
                items
                    .last_mut()
                    .expect("an entry exists")
                    .definitions
                    .push(definition);
            }
            other => {
                return Err(AstJsonError::new(format!(
                "a definition list holds definition_term and definition_description, not {other}"
            )))
            }
        }
    }

    Ok(items)
}

fn decode_definition_item(value: &Json) -> Result<DefinitionItem, AstJsonError> {
    let obj = value.as_object("definition_item")?;
    let terms = required_array(obj, "definition_item", "terms")?
        .iter()
        .map(|value| {
            Ok(DefinitionTerm {
                attrs: None,
                children: decode_inlines(value.as_array("definition_item.terms[]")?)?,
                pos: None,
            })
        })
        .collect::<Result<_, AstJsonError>>()?;
    let definitions = required_array(obj, "definition_item", "definitions")?
        .iter()
        .map(|value| {
            Ok(DefinitionDef {
                attrs: None,
                children: decode_blocks(value.as_array("definition_item.definitions[]")?)?,
                pos: None,
            })
        })
        .collect::<Result<_, AstJsonError>>()?;
    Ok(DefinitionItem {
        terms,
        definitions,
        pos: None,
    })
}

fn decode_figure_target(value: &Json) -> Result<FigureTarget, AstJsonError> {
    let obj = value.as_object("figure.target")?;
    match required_string(obj, "figure.target", "type")? {
        "image" => Ok(FigureTarget::Image(decode_image(obj)?)),
        "block_quote" => match decode_block(value)? {
            BlockNode::BlockQuote(n) => Ok(FigureTarget::BlockQuote(n)),
            _ => unreachable!(),
        },
        "table" => Ok(FigureTarget::Table(decode_table(obj)?)),
        "code_block" => match decode_block(value)? {
            BlockNode::CodeBlock(n) => Ok(FigureTarget::CodeBlock(n)),
            _ => unreachable!(),
        },
        "paragraph" => match decode_block(value)? {
            BlockNode::Paragraph(n) => Ok(FigureTarget::Paragraph(n)),
            _ => unreachable!(),
        },
        other => Err(AstJsonError::new(format!(
            "unknown figure.target node type {other:?}"
        ))),
    }
}

fn decode_inline(value: &Json) -> Result<InlineNode, AstJsonError> {
    let obj = value.as_object("inline node")?;
    let ty = required_string(obj, "inline node", "type")?;
    match ty {
        "text" => Ok(InlineNode::Text(Text {
            value: required_string(obj, "text", "value")?.to_string(),
            pos: optional_pos(obj, "text")?,
        })),
        "escaped_text" => Ok(InlineNode::EscapedText(EscapedText {
            value: required_string(obj, "escaped_text", "value")?.to_string(),
            pos: optional_pos(obj, "escaped_text")?,
        })),
        "smart_punctuation" => Ok(InlineNode::SmartPunctuation(SmartPunctuation {
            kind: required_string(obj, "smart_punctuation", "kind")?.to_string(),
            value: required_string(obj, "smart_punctuation", "value")?.to_string(),
            glyph: optional_string(obj, "glyph")?.map(str::to_string),
            pos: optional_pos(obj, "smart_punctuation")?,
        })),
        "emphasis" | "strong" | "underline" | "strike" | "superscript" | "subscript"
        | "highlight" => Ok(InlineNode::Emphasis(Emphasis {
            attrs: optional_attrs(obj)?,
            kind: decode_emphasis_kind(ty, obj)?,
            children: decode_inlines(required_array(obj, ty, "children")?)?,
            pos: optional_pos(obj, ty)?,
        })),
        "code" => Ok(InlineNode::Code(Code {
            value: required_string(obj, "code", "value")?.to_string(),
            attrs: optional_attrs(obj)?,
            pos: optional_pos(obj, "code")?,
        })),
        "link" => Ok(InlineNode::Link(Link {
            attrs: optional_attrs(obj)?,
            href: required_string(obj, "link", "href")?.to_string(),
            title: optional_string(obj, "title")?.map(str::to_string),
            children: decode_inlines(required_array(obj, "link", "children")?)?,
            ref_label: optional_string(obj, "ref")?.map(str::to_string),
            raw_ref: optional_string(obj, "rawRef")?.map(str::to_string),
            from_crossref: optional_bool(obj, "fromCrossref")?.unwrap_or(false),
            pos: optional_pos(obj, "link")?,
        })),
        "image" => Ok(InlineNode::Image(decode_image(obj)?)),
        "span" => Ok(InlineNode::Span(Span {
            attrs: optional_attrs(obj)?,
            children: decode_inlines(required_array(obj, "span", "children")?)?,
            pos: optional_pos(obj, "span")?,
        })),
        "math" => Ok(InlineNode::Math(Math {
            attrs: optional_attrs(obj)?,
            display: required_bool(obj, "math", "display")?,
            content: required_string(obj, "math", "content")?.to_string(),
            pos: optional_pos(obj, "math")?,
        })),
        "raw_inline" => Ok(InlineNode::RawInline(RawInline {
            format: required_string(obj, "raw_inline", "format")?.to_string(),
            content: required_string(obj, "raw_inline", "content")?.to_string(),
            pos: optional_pos(obj, "raw_inline")?,
        })),
        "literal_inline" => Ok(InlineNode::LiteralInline(LiteralInline {
            content: required_string(obj, "literal_inline", "content")?.to_string(),
            attrs: optional_attrs(obj)?,
            pos: optional_pos(obj, "literal_inline")?,
        })),
        "symbol" => Ok(InlineNode::Symbol(Symbol {
            name: required_string(obj, "symbol", "name")?.to_string(),
            attrs: optional_attrs(obj)?,
            pos: optional_pos(obj, "symbol")?,
        })),
        "autolink" => Ok(InlineNode::AutoLink(AutoLink {
            attrs: optional_attrs(obj)?,
            href: required_string(obj, "autolink", "href")?.to_string(),
            text: optional_string(obj, "text")?.unwrap_or("").to_string(),
            pos: optional_pos(obj, "autolink")?,
        })),
        "heading_ref" => Ok(InlineNode::CrossRef(CrossRef {
            target: required_string(obj, "heading_ref", "target")?.to_string(),
            pos: optional_pos(obj, "heading_ref")?,
        })),
        "caption_number" => Ok(InlineNode::CaptionNumber(CaptionNumber {
            number: optional_usize(obj, "n")?,
            pos: optional_pos(obj, "caption_number")?,
        })),
        "mention" => Ok(InlineNode::Mention(Mention {
            user: required_string(obj, "mention", "user")?.to_string(),
            pos: optional_pos(obj, "mention")?,
        })),
        "tag" => Ok(InlineNode::Tag(Tag {
            name: required_string(obj, "tag", "name")?.to_string(),
            pos: optional_pos(obj, "tag")?,
        })),
        "citation_group" => Ok(InlineNode::CitationGroup(CitationGroup {
            items: required_array(obj, "citation_group", "items")?
                .iter()
                .map(decode_citation)
                .collect::<Result<_, _>>()?,
            raw: required_string(obj, "citation_group", "raw")?.to_string(),
            mode: None,
            integral: optional_string(obj, "mode")? == Some("integral"),
            pos: optional_pos(obj, "citation_group")?,
        })),
        "inline_extension" => Ok(InlineNode::Extension(InlineExtension {
            attrs: optional_attrs(obj)?,
            name: required_string(obj, "inline_extension", "name")?.to_string(),
            children: decode_inlines(
                obj.get("content")
                    .or_else(|| obj.get("children"))
                    .ok_or_else(|| AstJsonError::new("inline_extension.content is required"))?
                    .as_array("inline_extension.content")?,
            )?,
            pos: optional_pos(obj, "inline_extension")?,
        })),
        "abbreviation" => Ok(InlineNode::Abbreviation(Abbreviation {
            abbr: required_string(obj, "abbreviation", "abbr")?.to_string(),
            expansion: required_string(obj, "abbreviation", "expansion")?.to_string(),
            pos: optional_pos(obj, "abbreviation")?,
        })),
        "footnote_ref" => Ok(InlineNode::Footnote(Footnote {
            attrs: optional_attrs(obj)?,
            id: optional_string(obj, "id")?.map(str::to_string),
            inline: None,
            number: optional_usize(obj, "number")?,
            ref_id: optional_string(obj, "refId")?.map(str::to_string),
            pos: optional_pos(obj, "footnote_ref")?,
        })),
        "inline_footnote" => Ok(InlineNode::Footnote(Footnote {
            attrs: optional_attrs(obj)?,
            id: None,
            inline: Some(decode_inlines(required_array(
                obj,
                "inline_footnote",
                "inline",
            )?)?),
            number: optional_usize(obj, "number")?,
            ref_id: optional_string(obj, "refId")?.map(str::to_string),
            pos: optional_pos(obj, "inline_footnote")?,
        })),
        "soft_break" => Ok(InlineNode::SoftBreak(Break {
            pos: optional_pos(obj, "soft_break")?,
        })),
        "hard_break" => Ok(InlineNode::HardBreak(Break {
            pos: optional_pos(obj, "hard_break")?,
        })),
        "insert" => Ok(InlineNode::CriticInsert(CriticInsert {
            attrs: optional_attrs(obj)?,
            children: decode_inlines(required_array(obj, "insert", "children")?)?,
            pos: optional_pos(obj, "insert")?,
        })),
        "delete" => Ok(InlineNode::CriticDelete(CriticDelete {
            attrs: optional_attrs(obj)?,
            children: decode_inlines(required_array(obj, "delete", "children")?)?,
            pos: optional_pos(obj, "delete")?,
        })),
        "substitution" => Ok(InlineNode::CriticSubstitute(CriticSubstitute {
            old_text: required_string(obj, "substitution", "oldText")?.to_string(),
            new_text: required_string(obj, "substitution", "newText")?.to_string(),
            pos: optional_pos(obj, "substitution")?,
        })),
        "critic_comment" => Ok(InlineNode::CriticComment(CriticComment {
            text: required_string(obj, "critic_comment", "text")?.to_string(),
            pos: optional_pos(obj, "critic_comment")?,
        })),
        other => Err(AstJsonError::new(format!(
            "unknown inline node type {other:?}"
        ))),
    }
}

fn decode_image(obj: &BTreeMap<String, Json>) -> Result<Image, AstJsonError> {
    Ok(Image {
        attrs: optional_attrs(obj)?,
        src: required_string(obj, "image", "src")?.to_string(),
        alt: required_string(obj, "image", "alt")?.to_string(),
        title: optional_string(obj, "title")?.map(str::to_string),
        ref_label: optional_string(obj, "ref")?.map(str::to_string),
        raw_ref: optional_string(obj, "rawRef")?.map(str::to_string),
        pos: optional_pos(obj, "image")?,
    })
}

fn decode_citation(value: &Json) -> Result<Citation, AstJsonError> {
    let obj = value.as_object("citation")?;
    Ok(Citation {
        key: required_string(obj, "citation", "key")?.to_string(),
        prefix: optional_inlines(obj, "prefix")?,
        locator: optional_inlines(obj, "locator")?,
        locator_label: optional_string(obj, "locatorLabel")?.map(str::to_string),
        locator_value: optional_string(obj, "locatorValue")?.map(str::to_string),
        suffix: optional_inlines(obj, "suffix")?,
        suppress_author: required_bool(obj, "citation", "suppressAuthor")?,
        number: optional_usize(obj, "number")?,
        label: None,
        use_index: optional_usize(obj, "useIndex")?,
    })
}

fn decode_emphasis_kind(
    ty: &str,
    obj: &BTreeMap<String, Json>,
) -> Result<EmphasisKind, AstJsonError> {
    Ok(match ty {
        "emphasis" => EmphasisKind::Italic,
        "strong" if optional_bool(obj, "boldItalic")?.unwrap_or(false) => EmphasisKind::BoldItalic,
        "strong" => EmphasisKind::Strong,
        "underline" => EmphasisKind::Underline,
        "strike" => EmphasisKind::Strike,
        "superscript" => EmphasisKind::Super,
        "subscript" => EmphasisKind::Sub,
        "highlight" => EmphasisKind::Highlight,
        _ => return Err(AstJsonError::new(format!("unknown emphasis type {ty:?}"))),
    })
}

fn decode_ol_type(value: &str) -> Result<OrderedListType, AstJsonError> {
    match value {
        "a" => Ok(OrderedListType::LowerAlpha),
        "A" => Ok(OrderedListType::UpperAlpha),
        "i" => Ok(OrderedListType::LowerRoman),
        "I" => Ok(OrderedListType::UpperRoman),
        other => Err(AstJsonError::new(format!(
            "list.olType has invalid value {other:?}"
        ))),
    }
}

fn decode_cell_span(value: &str) -> Result<TableCellSpan, AstJsonError> {
    match value {
        "rowspan" => Ok(TableCellSpan::Rowspan),
        "colspan" => Ok(TableCellSpan::Colspan),
        other => Err(AstJsonError::new(format!(
            "table_cell.span has invalid value {other:?}"
        ))),
    }
}

fn decode_table_align(value: &str) -> Result<TableAlign, AstJsonError> {
    match value {
        "left" => Ok(TableAlign::Left),
        "right" => Ok(TableAlign::Right),
        "center" => Ok(TableAlign::Center),
        other => Err(AstJsonError::new(format!(
            "table_cell.align has invalid value {other:?}"
        ))),
    }
}

fn optional_marker_char(
    obj: &BTreeMap<String, Json>,
    field: &str,
) -> Result<Option<char>, AstJsonError> {
    optional_string(obj, field)?
        .map(|s| {
            let mut chars = s.chars();
            let Some(ch) = chars.next() else {
                return Err(AstJsonError::new(format!("list.{field} cannot be empty")));
            };
            if chars.next().is_some() {
                return Err(AstJsonError::new(format!(
                    "list.{field} must be one character"
                )));
            }
            Ok(ch)
        })
        .transpose()
}

fn optional_attrs(obj: &BTreeMap<String, Json>) -> Result<Option<Attrs>, AstJsonError> {
    let Some(value) = obj.get("attrs") else {
        return Ok(None);
    };
    let attrs_obj = value.as_object("attrs")?;
    let mut key_values = BTreeMap::new();
    if let Some(kv) = attrs_obj.get("keyValues") {
        for (key, value) in kv.as_object("attrs.keyValues")? {
            key_values.insert(
                key.clone(),
                value.as_string("attrs.keyValues value")?.to_string(),
            );
        }
    }
    let classes = match attrs_obj.get("classes") {
        Some(value) => value
            .as_array("attrs.classes")?
            .iter()
            .map(|value| value.as_string("attrs.classes[]").map(str::to_string))
            .collect::<Result<_, _>>()?,
        None => Vec::new(),
    };
    let order = match attrs_obj.get("order") {
        Some(value) => value
            .as_array("attrs.order")?
            .iter()
            .map(|value| {
                let slot = value.as_string("attrs.order[]")?;
                Ok(match slot {
                    "#id" => AttrSlot::Id,
                    ".class" => AttrSlot::Class,
                    key => AttrSlot::Key(key.to_string()),
                })
            })
            .collect::<Result<_, AstJsonError>>()?,
        None => Vec::new(),
    };
    Ok(Some(Attrs {
        id: optional_string(attrs_obj, "id")?.map(str::to_string),
        classes,
        key_values,
        order,
    }))
}

fn optional_pos(
    obj: &BTreeMap<String, Json>,
    node_type: &str,
) -> Result<Option<Pos>, AstJsonError> {
    let Some(value) = obj.get("pos") else {
        return Ok(None);
    };
    let pos = value.as_object("pos")?;
    Ok(Some(Pos {
        start_line: required_usize(pos, node_type, "startLine")?,
        end_line: required_usize(pos, node_type, "endLine")?,
        start_column: required_usize(pos, node_type, "startColumn")?,
        end_column: required_usize(pos, node_type, "endColumn")?,
        start_offset: required_usize(pos, node_type, "startOffset")?,
        end_offset: required_usize(pos, node_type, "endOffset")?,
    }))
}

fn optional_inlines(
    obj: &BTreeMap<String, Json>,
    field: &str,
) -> Result<Option<Vec<InlineNode>>, AstJsonError> {
    obj.get(field)
        .map(|value| decode_inlines(value.as_array(field)?))
        .transpose()
}

fn expect_type(obj: &BTreeMap<String, Json>, expected: &str) -> Result<(), AstJsonError> {
    let actual = required_string(obj, expected, "type")?;
    if actual == expected {
        Ok(())
    } else {
        Err(AstJsonError::new(format!(
            "{expected}.type must be {expected:?}, got {actual:?}"
        )))
    }
}

fn required_value<'a>(
    obj: &'a BTreeMap<String, Json>,
    node_type: &str,
    field: &str,
) -> Result<&'a Json, AstJsonError> {
    obj.get(field)
        .ok_or_else(|| AstJsonError::new(format!("{node_type}.{field} is required")))
}

fn required_array<'a>(
    obj: &'a BTreeMap<String, Json>,
    node_type: &str,
    field: &str,
) -> Result<&'a [Json], AstJsonError> {
    required_value(obj, node_type, field)?.as_array(&format!("{node_type}.{field}"))
}

fn required_string<'a>(
    obj: &'a BTreeMap<String, Json>,
    node_type: &str,
    field: &str,
) -> Result<&'a str, AstJsonError> {
    required_value(obj, node_type, field)?.as_string(&format!("{node_type}.{field}"))
}

fn required_bool(
    obj: &BTreeMap<String, Json>,
    node_type: &str,
    field: &str,
) -> Result<bool, AstJsonError> {
    required_value(obj, node_type, field)?.as_bool(&format!("{node_type}.{field}"))
}

fn required_usize(
    obj: &BTreeMap<String, Json>,
    node_type: &str,
    field: &str,
) -> Result<usize, AstJsonError> {
    required_value(obj, node_type, field)?.as_usize(&format!("{node_type}.{field}"))
}

fn optional_string<'a>(
    obj: &'a BTreeMap<String, Json>,
    field: &str,
) -> Result<Option<&'a str>, AstJsonError> {
    obj.get(field)
        .map(|value| value.as_string(field))
        .transpose()
}

fn optional_bool(obj: &BTreeMap<String, Json>, field: &str) -> Result<Option<bool>, AstJsonError> {
    obj.get(field).map(|value| value.as_bool(field)).transpose()
}

fn optional_usize(
    obj: &BTreeMap<String, Json>,
    field: &str,
) -> Result<Option<usize>, AstJsonError> {
    obj.get(field)
        .map(|value| value.as_usize(field))
        .transpose()
}

impl Json {
    fn as_object(&self, context: &str) -> Result<&BTreeMap<String, Json>, AstJsonError> {
        match self {
            Json::Object(obj) => Ok(obj),
            _ => Err(AstJsonError::new(format!("{context} must be an object"))),
        }
    }

    fn as_array(&self, context: &str) -> Result<&[Json], AstJsonError> {
        match self {
            Json::Array(values) => Ok(values),
            _ => Err(AstJsonError::new(format!("{context} must be an array"))),
        }
    }

    fn as_string(&self, context: &str) -> Result<&str, AstJsonError> {
        match self {
            Json::String(value) => Ok(value),
            _ => Err(AstJsonError::new(format!("{context} must be a string"))),
        }
    }

    fn as_bool(&self, context: &str) -> Result<bool, AstJsonError> {
        match self {
            Json::Bool(value) => Ok(*value),
            _ => Err(AstJsonError::new(format!("{context} must be a boolean"))),
        }
    }

    fn as_usize(&self, context: &str) -> Result<usize, AstJsonError> {
        match self {
            Json::Number(value) if *value >= 0 => Ok(*value as usize),
            _ => Err(AstJsonError::new(format!(
                "{context} must be a non-negative integer"
            ))),
        }
    }
}

/// Deepest container nesting the reader will follow.
///
/// The reader is recursive-descent, so nesting depth is stack depth, and a
/// document is untrusted input: `[[[[…]]]]` 200000 deep overflowed the stack and
/// ABORTED the process rather than returning an error. The markup parser bounds
/// itself the same way (`MAX_NESTING_DEPTH` in parse.rs, 200, matching carve-js
/// and carve-php).
///
/// The two caps count DIFFERENT THINGS, which is the bug this constant used to
/// carry (carve-rs#389). The parser's 200 is a NODE depth. This one is a raw
/// JSON structural depth, and a node costs two of those levels - the object,
/// then its `children` array - so a budget of 200 admitted only about 99 nested
/// containers and `from_json` REJECTED ASTs this crate's own `to_json` had just
/// produced. Measured: 200 containers serialize to a JSON depth of 405, the
/// ratio converging on 2.02 as the fixed overhead amortizes.
///
/// So it is derived rather than written down twice. The slack absorbs the
/// non-container nesting a node carries (`attrs`, `pos`) and keeps this from
/// sitting exactly on the boundary. Raising the parser's cap raises this one
/// with it, which is the point - the reader must accept whatever the parser can
/// emit, whatever that limit becomes.
const MAX_JSON_DEPTH: usize = crate::parse::MAX_NESTING_DEPTH * 2 + 16;

struct Parser<'a> {
    input: &'a str,
    pos: usize,
    depth: usize,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            input,
            pos: 0,
            depth: 0,
        }
    }

    /// Run `f` one level deeper, refusing past the cap.
    fn nested<T>(
        &mut self,
        f: impl FnOnce(&mut Self) -> Result<T, AstJsonError>,
    ) -> Result<T, AstJsonError> {
        if self.depth >= MAX_JSON_DEPTH {
            return Err(self.err("JSON nests deeper than the reader's depth budget"));
        }
        self.depth += 1;
        let out = f(self);
        self.depth -= 1;
        out
    }

    fn parse(mut self) -> Result<Json, AstJsonError> {
        let value = self.parse_value()?;
        self.skip_ws();
        if self.pos != self.input.len() {
            return Err(self.err("trailing characters after JSON value"));
        }
        Ok(value)
    }

    fn parse_value(&mut self) -> Result<Json, AstJsonError> {
        self.skip_ws();
        match self.peek() {
            Some(b'n') => self.parse_literal(b"null", Json::Null),
            Some(b't') => self.parse_literal(b"true", Json::Bool(true)),
            Some(b'f') => self.parse_literal(b"false", Json::Bool(false)),
            Some(b'"') => self.parse_string().map(Json::String),
            Some(b'[') => self.nested(Self::parse_array),
            Some(b'{') => self.nested(Self::parse_object),
            Some(b'-' | b'0'..=b'9') => self.parse_number().map(Json::Number),
            Some(_) => Err(self.err("unexpected character in JSON value")),
            None => Err(self.err("unexpected end of input")),
        }
    }

    fn parse_literal(&mut self, literal: &[u8], value: Json) -> Result<Json, AstJsonError> {
        if self.input.as_bytes()[self.pos..].starts_with(literal) {
            self.pos += literal.len();
            Ok(value)
        } else {
            Err(self.err("invalid JSON literal"))
        }
    }

    fn parse_array(&mut self) -> Result<Json, AstJsonError> {
        self.expect(b'[')?;
        let mut values = Vec::new();
        loop {
            self.skip_ws();
            if self.consume(b']') {
                break;
            }
            values.push(self.parse_value()?);
            self.skip_ws();
            if self.consume(b']') {
                break;
            }
            self.expect(b',')?;
        }
        Ok(Json::Array(values))
    }

    fn parse_object(&mut self) -> Result<Json, AstJsonError> {
        self.expect(b'{')?;
        let mut values = BTreeMap::new();
        loop {
            self.skip_ws();
            if self.consume(b'}') {
                break;
            }
            let key = self.parse_string()?;
            self.skip_ws();
            self.expect(b':')?;
            let value = self.parse_value()?;
            values.insert(key, value);
            self.skip_ws();
            if self.consume(b'}') {
                break;
            }
            self.expect(b',')?;
        }
        Ok(Json::Object(values))
    }

    fn parse_string(&mut self) -> Result<String, AstJsonError> {
        self.expect(b'"')?;
        let mut out = String::new();
        while let Some(byte) = self.next() {
            match byte {
                b'"' => return Ok(out),
                b'\\' => self.parse_escape(&mut out)?,
                0x00..=0x1f => return Err(self.err("control character in JSON string")),
                _ => {
                    let start = self.pos - 1;
                    let ch = self.input[start..]
                        .chars()
                        .next()
                        .ok_or_else(|| self.err("invalid UTF-8 in JSON string"))?;
                    self.pos = start + ch.len_utf8();
                    out.push(ch);
                }
            }
        }
        Err(self.err("unterminated JSON string"))
    }

    fn parse_escape(&mut self, out: &mut String) -> Result<(), AstJsonError> {
        match self.next() {
            Some(b'"') => out.push('"'),
            Some(b'\\') => out.push('\\'),
            Some(b'/') => out.push('/'),
            Some(b'b') => out.push('\u{08}'),
            Some(b'f') => out.push('\u{0c}'),
            Some(b'n') => out.push('\n'),
            Some(b'r') => out.push('\r'),
            Some(b't') => out.push('\t'),
            Some(b'u') => {
                let code = self.parse_hex4()?;
                if (0xd800..=0xdbff).contains(&code) {
                    self.expect(b'\\')?;
                    self.expect(b'u')?;
                    let low = self.parse_hex4()?;
                    if !(0xdc00..=0xdfff).contains(&low) {
                        return Err(self.err("invalid JSON unicode surrogate pair"));
                    }
                    let scalar = 0x10000 + (((code - 0xd800) << 10) | (low - 0xdc00));
                    out.push(
                        char::from_u32(scalar)
                            .ok_or_else(|| self.err("invalid JSON unicode escape"))?,
                    );
                } else if (0xdc00..=0xdfff).contains(&code) {
                    return Err(self.err("unpaired JSON unicode surrogate"));
                } else {
                    out.push(
                        char::from_u32(code)
                            .ok_or_else(|| self.err("invalid JSON unicode escape"))?,
                    );
                }
            }
            Some(_) => return Err(self.err("invalid JSON string escape")),
            None => return Err(self.err("unterminated JSON string escape")),
        }
        Ok(())
    }

    fn parse_hex4(&mut self) -> Result<u32, AstJsonError> {
        let mut value = 0;
        for _ in 0..4 {
            let Some(byte) = self.next() else {
                return Err(self.err("unterminated JSON unicode escape"));
            };
            value = (value << 4)
                | match byte {
                    b'0'..=b'9' => (byte - b'0') as u32,
                    b'a'..=b'f' => (byte - b'a' + 10) as u32,
                    b'A'..=b'F' => (byte - b'A' + 10) as u32,
                    _ => return Err(self.err("invalid JSON unicode escape")),
                };
        }
        Ok(value)
    }

    fn parse_number(&mut self) -> Result<i64, AstJsonError> {
        let start = self.pos;
        self.consume(b'-');
        match self.peek() {
            Some(b'0') => {
                self.pos += 1;
            }
            Some(b'1'..=b'9') => {
                self.pos += 1;
                while matches!(self.peek(), Some(b'0'..=b'9')) {
                    self.pos += 1;
                }
            }
            _ => return Err(self.err("invalid JSON number")),
        }
        if matches!(self.peek(), Some(b'.' | b'e' | b'E')) {
            return Err(self.err("JSON AST numbers must be integers"));
        }
        self.input[start..self.pos]
            .parse::<i64>()
            .map_err(|_| self.err("JSON number is out of range"))
    }

    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\n' | b'\r' | b'\t')) {
            self.pos += 1;
        }
    }

    fn expect(&mut self, byte: u8) -> Result<(), AstJsonError> {
        if self.consume(byte) {
            Ok(())
        } else {
            Err(self.err(format!("expected '{}'", byte as char)))
        }
    }

    fn consume(&mut self, byte: u8) -> bool {
        if self.peek() == Some(byte) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn peek(&self) -> Option<u8> {
        self.input.as_bytes().get(self.pos).copied()
    }

    fn next(&mut self) -> Option<u8> {
        let byte = self.peek()?;
        self.pos += 1;
        Some(byte)
    }

    fn err(&self, message: impl Into<String>) -> AstJsonError {
        AstJsonError::new(format!("{} at byte {}", message.into(), self.pos))
    }
}
