use crate::ast::*;
use crate::extension::Options;
use crate::render_text::strip_controls;

const MAX_RENDER_DEPTH: usize = 100;

thread_local! {
    /// Labels that actually have a definition in the document being rendered.
    ///
    /// A footnote reference without one did not form a footnote, so it has to be
    /// reproduced as source text. The HTML renderer decides that on the node's
    /// `number`, which numbering assigns -- this target does no numbering, so the
    /// field is always None here and there was nothing to check. A thread-local
    /// keeps the answer off every signature in the render tree, matching how
    /// render_markdown carries its typography mode; it is set once per render
    /// entry point and read only by the footnote arm.
    static DEFINED_FOOTNOTES: std::cell::RefCell<std::collections::BTreeSet<String>> =
        const { std::cell::RefCell::new(std::collections::BTreeSet::new()) };
}

fn footnote_is_defined(id: &str) -> bool {
    DEFINED_FOOTNOTES.with(|set| set.borrow().contains(id))
}

fn trim_block_output(s: &str) -> &str {
    s.trim_matches(|c| c == '\n' || c == ' ')
}

/// Render a document to plain text. See `render_markdown_with_options` for why
/// the options-taking wrapper exists; the profile transform runs upstream.
pub fn render_plain_text_with_options(doc: &Document, _options: &Options<'_>) -> String {
    render_plain_text(doc)
}

pub fn render_plain_text(doc: &Document) -> String {
    DEFINED_FOOTNOTES.with(|set| {
        *set.borrow_mut() = doc.footnote_defs.keys().cloned().collect();
    });
    let out = render_blocks(&doc.children, 0);
    let footnotes = render_footnote_defs(doc);
    normalize(&format!("{out}{footnotes}"))
}

fn render_blocks(blocks: &[BlockNode], depth: usize) -> String {
    if depth > MAX_RENDER_DEPTH {
        return String::new();
    }
    blocks
        .iter()
        .map(|block| render_block(block, depth))
        .collect()
}

fn render_block(node: &BlockNode, depth: usize) -> String {
    if depth > MAX_RENDER_DEPTH {
        return String::new();
    }
    match node {
        BlockNode::Heading(heading) => format!("{}\n\n", render_inlines(&heading.children)),
        BlockNode::Paragraph(paragraph) => {
            format!("{}\n\n", render_inlines(&paragraph.children))
        }
        BlockNode::CodeBlock(code) => format!("{}\n\n", strip_controls(&code.content)),
        BlockNode::BlockQuote(quote) => {
            format!(
                "\"{}\"\n\n",
                trim_block_output(&render_blocks(&quote.children, depth + 1))
            )
        }
        BlockNode::List(list) => render_list(list, depth + 1),
        BlockNode::ThematicBreak(_) => "---\n\n".to_string(),
        BlockNode::Table(table) => render_table(table),
        BlockNode::Admonition(admonition) => {
            let body = render_blocks(&admonition.children, depth + 1);
            let body = match &admonition.title {
                Some(title) => {
                    let t = render_inlines(title);
                    if t.is_empty() {
                        body
                    } else {
                        format!("{t}\n\n{body}")
                    }
                }
                None => body,
            };
            prepend_label(body, admonition.label.as_deref())
        }
        BlockNode::LineBlock(lb) => render_blocks(&lb.children, depth + 1),
        BlockNode::Div(div) => {
            let body = render_blocks(&div.children, depth + 1);
            prepend_label(body, div.label.as_deref())
        }
        BlockNode::DefinitionList(list) => render_definition_list(&list.items, true, depth + 1),
        BlockNode::Figure(figure) => render_figure(figure, depth + 1),
        // Terminate the block image so the next block is not glued onto it.
        BlockNode::BlockImage(image) => format!("{}\n\n", strip_controls(&image.alt)),
        BlockNode::Extension(extension) => render_blocks(&extension.children, depth + 1),
        BlockNode::RawBlock(_) | BlockNode::AbbreviationDef(_) | BlockNode::Comment(_) => {
            String::new()
        }
    }
}

/// Graceful degradation: when no extension consumed the grouping `[label]`,
/// surface it as a leading line (mirroring how an admonition title renders) so
/// the authored label is never silently dropped in plain text.
fn prepend_label(body: String, label: Option<&str>) -> String {
    match label {
        Some(label) if !label.is_empty() => {
            let l = strip_controls(label);
            if body.is_empty() {
                format!("{l}\n\n")
            } else {
                format!("{l}\n\n{body}")
            }
        }
        _ => body,
    }
}

fn render_list(node: &List, depth: usize) -> String {
    if depth > MAX_RENDER_DEPTH {
        return String::new();
    }
    let mut out = String::new();
    let mut counter = node.start.unwrap_or(1);
    for item in &node.items {
        if node.ordered {
            out.push_str(&format!("{counter}. "));
            counter += 1;
        } else {
            out.push_str("- ");
        }
        out.push_str(trim_block_output(&render_blocks(&item.children, depth + 1)));
        out.push('\n');
    }
    out.push('\n');
    out
}

fn render_definition_list(items: &[DefinitionItem], trailing_blank: bool, depth: usize) -> String {
    if depth > MAX_RENDER_DEPTH {
        return String::new();
    }
    let mut out = String::new();
    for item in items {
        for term in &item.terms {
            out.push_str(&format!("{}\n", render_inlines(term)));
        }
        for definition in &item.definitions {
            out.push_str(&format!(
                "  {}\n",
                trim_block_output(&render_blocks(definition, depth + 1))
            ));
        }
    }
    if trailing_blank {
        out.push('\n');
    }
    out
}

fn render_table(node: &Table) -> String {
    let mut out = String::new();
    for row in &node.rows {
        out.push_str(
            &row.cells
                .iter()
                .map(|cell| render_inlines(&cell.children).trim().to_string())
                .collect::<Vec<_>>()
                .join(" | "),
        );
        out.push('\n');
    }
    if let Some(caption) = &node.caption {
        out = format!("{}\n{}\n", out.trim_end(), render_inlines(caption));
    }
    out.push('\n');
    out
}

fn render_figure(node: &Figure, depth: usize) -> String {
    if depth > MAX_RENDER_DEPTH {
        return String::new();
    }
    let target = match &node.target {
        FigureTarget::Image(image) => strip_controls(&image.alt),
        FigureTarget::Table(table) => render_table(table).trim().to_string(),
        FigureTarget::BlockQuote(quote) => {
            render_block(&BlockNode::BlockQuote(quote.clone()), depth + 1)
                .trim()
                .to_string()
        }
        FigureTarget::CodeBlock(cb) => render_block(&BlockNode::CodeBlock(cb.clone()), depth + 1)
            .trim()
            .to_string(),
        FigureTarget::Paragraph(p) => render_block(&BlockNode::Paragraph(p.clone()), depth + 1)
            .trim()
            .to_string(),
    };
    // The caption sits on its own line directly under the figure (`\n`) - an
    // image target used to glue it on. A blockquote target keeps the blank-line
    // separation; a table drops the caption. End with the block separator so a
    // following block is not glued (matching carve-php).
    let sep = match &node.target {
        FigureTarget::BlockQuote(_) => "\n\n",
        FigureTarget::Table(_) => "",
        _ => "\n",
    };
    format!("{target}{sep}{}\n\n", render_inlines(&node.caption))
}

fn render_footnote_defs(doc: &Document) -> String {
    let mut out = String::new();
    for (label, blocks) in &doc.footnote_defs {
        out.push_str(&format!(
            "[{}]: {}\n",
            strip_controls(label),
            render_blocks(blocks, 0).trim()
        ));
    }
    out
}

fn render_inlines(nodes: &[InlineNode]) -> String {
    render_inlines_stateful(nodes, 0)
}

fn render_inlines_stateful(nodes: &[InlineNode], depth: usize) -> String {
    if depth > MAX_RENDER_DEPTH {
        return String::new();
    }
    let mut out = String::new();
    for node in nodes {
        out.push_str(&render_inline(node, depth));
    }
    out
}

fn render_inline(node: &InlineNode, depth: usize) -> String {
    if depth > MAX_RENDER_DEPTH {
        return String::new();
    }
    match node {
        InlineNode::Text(text) | InlineNode::EscapedText(text) => strip_controls(text),
        InlineNode::SmartPunctuation(s) => strip_controls(smart_punctuation_glyph(s)),
        InlineNode::Emphasis(emphasis) => match emphasis.kind {
            EmphasisKind::Strike => render_inlines_stateful(&emphasis.children, depth + 1),
            _ => render_inlines_stateful(&emphasis.children, depth + 1),
        },
        InlineNode::Code(code, _) => strip_controls(code),
        InlineNode::Link(link) => render_inlines_stateful(&link.children, depth + 1),
        InlineNode::Image(image) => strip_controls(&image.alt),
        InlineNode::Span(span) => render_inlines_stateful(&span.children, depth + 1),
        InlineNode::Math(math) => strip_controls(&math.content),
        InlineNode::RawInline(_) => String::new(),
        // §27: always emitted (unlike raw passthrough above), as plain prose.
        InlineNode::LiteralInline(lit) => strip_controls(&lit.content),
        InlineNode::Symbol(symbol) => format!(":{}:", symbol.name),
        InlineNode::AutoLink(link) => {
            // Raw autolink content: a URI autolink keeps its scheme, an email
            // shows the address.
            strip_controls(&link.text)
        }
        InlineNode::Mention(mention) => format!("@{}", strip_controls(&mention.user)),
        InlineNode::Tag(tag) => format!("#{}", strip_controls(&tag.name)),
        InlineNode::Extension(extension) => render_inlines_stateful(&extension.children, depth + 1),
        InlineNode::Abbreviation(abbr) => strip_controls(&abbr.abbr),
        InlineNode::Footnote(footnote) => {
            if let Some(inline) = &footnote.inline {
                // Footnote content is its own context: render with a FRESH quote
                // state so it neither inherits nor mutates the surrounding
                // paragraph's open quotes. Matches carve-php.
                format!("({})", render_inlines_stateful(inline, depth + 1))
            } else {
                let id = strip_controls(footnote.id.as_deref().unwrap_or(""));
                // An UNRESOLVED reference stays literal, exactly as the HTML
                // target renders it: the construct did not form, so `[^a]` is
                // ordinary text and dropping the caret invented a reference the
                // document does not have. carve-php already did this
                // (carve#352, corpus 132/133/157/161).
                if footnote_is_defined(&id) {
                    format!("[{id}]")
                } else {
                    format!("[^{id}]")
                }
            }
        }
        InlineNode::SoftBreak => " ".to_string(),
        InlineNode::HardBreak => "\n".to_string(),
        InlineNode::CriticInsert(insert) => render_inlines_stateful(&insert.children, depth + 1),
        InlineNode::CriticDelete(delete) => {
            format!("~{}~", render_inlines_stateful(&delete.children, depth + 1))
        }
        InlineNode::CriticSubstitute(sub) => format!(
            "~{}~{}",
            strip_controls(&sub.old_text),
            strip_controls(&sub.new_text)
        ),
        InlineNode::CriticComment(_) => String::new(),
        InlineNode::CrossRef(crossref) => format!("</#{}>", strip_controls(&crossref.target)),
        // Tier-2 ext node; the core renderer has no numbering, so emit the source.
        InlineNode::CitationGroup(group) => strip_controls(&group.raw),
        InlineNode::CaptionNumber(number) => number
            .number
            .map(|n| n.to_string())
            .unwrap_or_else(|| "#".to_string()),
    }
}

fn normalize(text: &str) -> String {
    let mut out = String::new();
    let mut newlines = 0usize;
    for ch in text.chars() {
        if ch == '\n' {
            newlines += 1;
            if newlines <= 2 {
                out.push(ch);
            }
        } else {
            newlines = 0;
            out.push(ch);
        }
    }
    // Trim only document-edge newlines/ASCII spaces, NOT the generated-NBSP
    // placeholders that carry line-block / escaped-space indentation (a plain
    // `.trim()` would strip them too, dropping a first verse line's indent).
    let trimmed = out.trim_matches(|c| c == '\n' || c == ' ');
    // A generated-NBSP placeholder (escaped space / verse indent) becomes a
    // plain space in display output; a LITERAL U+00A0 typed in the source is
    // preserved as-is. Only the HTML renderer folds both to `&nbsp;`.
    format!("{trimmed}\n")
        .replace(crate::NBSP_PLACEHOLDER, " ")
        .replace(crate::ESCAPED_CARET_PLACEHOLDER, "^")
}
