use crate::ast::*;
use crate::extension::Options;
use crate::render_text::{clean_smart_text_stateful, strip_controls, SmartQuoteState};

/// Render a document to plain text. See `render_markdown_with_options` for why
/// the options-taking wrapper exists; the profile transform runs upstream.
pub fn render_plain_text_with_options(doc: &Document, _options: &Options<'_>) -> String {
    render_plain_text(doc)
}

pub fn render_plain_text(doc: &Document) -> String {
    let out = render_blocks(&doc.children);
    let footnotes = render_footnote_defs(doc);
    normalize(&format!("{out}{footnotes}"))
}

fn render_blocks(blocks: &[BlockNode]) -> String {
    blocks.iter().map(render_block).collect()
}

fn render_block(node: &BlockNode) -> String {
    match node {
        BlockNode::Heading(heading) => format!("{}\n\n", render_inlines(&heading.children)),
        BlockNode::Paragraph(paragraph) => {
            if let Some((term, def)) = legacy_definition_parts(&paragraph.children) {
                return format!("{term}\n  {def}\n\n");
            }
            format!("{}\n\n", render_inlines(&paragraph.children))
        }
        BlockNode::CodeBlock(code) => format!("{}\n\n", strip_controls(&code.content)),
        BlockNode::BlockQuote(quote) => {
            format!("\"{}\"\n\n", render_blocks(&quote.children).trim())
        }
        BlockNode::List(list) => render_list(list),
        BlockNode::ThematicBreak(_) => "---\n\n".to_string(),
        BlockNode::Table(table) => render_table(table),
        BlockNode::Admonition(admonition) => {
            let body = render_blocks(&admonition.children);
            match &admonition.title {
                Some(title) => {
                    let t = render_inlines(title);
                    if t.is_empty() {
                        body
                    } else {
                        format!("{t}\n\n{body}")
                    }
                }
                None => body,
            }
        }
        BlockNode::Div(div) => render_blocks(&div.children),
        BlockNode::DefinitionList(list) => render_definition_list(&list.items, true),
        BlockNode::Figure(figure) => render_figure(figure),
        // Terminate the block image so the next block is not glued onto it.
        BlockNode::BlockImage(image) => format!("{}\n\n", image.alt),
        BlockNode::Extension(extension) => render_blocks(&extension.children),
        BlockNode::RawBlock(_) | BlockNode::AbbreviationDef(_) | BlockNode::Comment(_) => {
            String::new()
        }
    }
}

fn render_list(node: &List) -> String {
    let mut out = String::new();
    let mut counter = node.start.unwrap_or(1);
    for item in &node.items {
        if node.ordered {
            out.push_str(&format!("{counter}. "));
            counter += 1;
        } else {
            out.push_str("- ");
        }
        out.push_str(render_blocks(&item.children).trim());
        out.push('\n');
    }
    out.push('\n');
    out
}

fn render_definition_list(items: &[DefinitionItem], trailing_blank: bool) -> String {
    let mut out = String::new();
    for item in items {
        for term in &item.terms {
            out.push_str(&format!("{}\n", render_inlines(term)));
        }
        for definition in &item.definitions {
            out.push_str(&format!("  {}\n", render_blocks(definition).trim()));
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

fn render_figure(node: &Figure) -> String {
    let target = match &node.target {
        FigureTarget::Image(image) => image.alt.clone(),
        FigureTarget::Table(table) => render_table(table).trim().to_string(),
        FigureTarget::BlockQuote(quote) => render_block(&BlockNode::BlockQuote(quote.clone()))
            .trim()
            .to_string(),
        FigureTarget::CodeBlock(cb) => render_block(&BlockNode::CodeBlock(cb.clone()))
            .trim()
            .to_string(),
        FigureTarget::Paragraph(p) => render_block(&BlockNode::Paragraph(p.clone()))
            .trim()
            .to_string(),
    };
    // A block-level target keeps the caption on its own line; an inline image
    // stays adjacent.
    let sep = match &node.target {
        FigureTarget::CodeBlock(_) | FigureTarget::Paragraph(_) => "\n",
        _ => "",
    };
    format!("{target}{sep}{}", render_inlines(&node.caption))
}

fn render_footnote_defs(doc: &Document) -> String {
    let mut out = String::new();
    for (label, blocks) in &doc.footnote_defs {
        out.push_str(&format!("[{label}]: {}\n", render_blocks(blocks).trim()));
    }
    out
}

fn render_inlines(nodes: &[InlineNode]) -> String {
    // Block-level entry: each block starts with a fresh smart-quote state.
    let mut state = SmartQuoteState::new();
    render_inlines_stateful(nodes, &mut state)
}

fn render_inlines_stateful(nodes: &[InlineNode], state: &mut SmartQuoteState) -> String {
    let mut out = String::new();
    for node in nodes {
        out.push_str(&render_inline(node, state));
    }
    out
}

fn render_inline(node: &InlineNode, state: &mut SmartQuoteState) -> String {
    match node {
        InlineNode::Text(text) => strip_controls(&clean_smart_text_stateful(text, state)),
        InlineNode::Emphasis(emphasis) => match emphasis.kind {
            EmphasisKind::Strike => render_inlines_stateful(&emphasis.children, state),
            _ => render_inlines_stateful(&emphasis.children, state),
        },
        InlineNode::Code(code, _) => strip_controls(code),
        InlineNode::Link(link) => {
            if link.href.starts_with('#') {
                render_inlines_stateful(&link.children, state)
            } else {
                strip_controls(&link.href)
            }
        }
        InlineNode::Image(image) => image.alt.clone(),
        InlineNode::Span(span) => render_inlines_stateful(&span.children, state),
        InlineNode::Math(math) => strip_controls(&math.content),
        InlineNode::RawInline(_) => String::new(),
        InlineNode::Emoji(emoji) => format!(":{}:", emoji.name),
        InlineNode::AutoLink(link) => {
            let href = strip_controls(&link.href);
            href.strip_prefix("mailto:").unwrap_or(&href).to_string()
        }
        InlineNode::Mention(mention) => format!("@{}", mention.user),
        InlineNode::Tag(tag) => format!("#{}", tag.name),
        InlineNode::Extension(extension) => render_inlines_stateful(&extension.children, state),
        InlineNode::Abbreviation(abbr) => abbr.abbr.clone(),
        InlineNode::Footnote(footnote) => {
            if let Some(inline) = &footnote.inline {
                // Footnote content is its own context: render with a FRESH quote
                // state (via render_inlines) so it neither inherits nor mutates
                // the surrounding paragraph's open quotes. Matches carve-php.
                format!("({})", render_inlines(inline))
            } else {
                format!("[{}]", footnote.id.as_deref().unwrap_or(""))
            }
        }
        InlineNode::SoftBreak => " ".to_string(),
        InlineNode::HardBreak => "\n".to_string(),
        InlineNode::CriticInsert(insert) => render_inlines_stateful(&insert.children, state),
        InlineNode::CriticDelete(delete) => {
            format!("~{}~", render_inlines_stateful(&delete.children, state))
        }
        InlineNode::CriticSubstitute(sub) => format!("~{}~{}", sub.old_text, sub.new_text),
        InlineNode::CriticComment(_) => String::new(),
        InlineNode::CrossRef(crossref) => format!("</#{}>", crossref.target),
        // Tier-2 ext node; the core renderer has no numbering, so emit the source.
        InlineNode::CitationGroup(group) => group.raw.clone(),
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
    // Trim only document-edge newlines/ASCII spaces, NOT the non-breaking
    // spaces that carry line-block / escaped-space indentation (a plain `.trim()`
    // would strip leading U+00A0 too, dropping a first verse line's indent).
    // Then a non-breaking space becomes a plain space in display output; only
    // the HTML renderer emits `&nbsp;`.
    let trimmed = out.trim_matches(|c| c == '\n' || c == ' ');
    format!("{trimmed}\n").replace('\u{00a0}', " ")
}

fn legacy_definition_parts(nodes: &[InlineNode]) -> Option<(String, String)> {
    if nodes.len() != 3 {
        return None;
    }
    if !matches!(nodes[1], InlineNode::SoftBreak) {
        return None;
    }
    if let InlineNode::Text(term) = &nodes[0] {
        if let InlineNode::Text(def) = &nodes[2] {
            if let Some(stripped) = term.strip_prefix(": ") {
                return Some((stripped.to_string(), def.clone()));
            }
        }
    }
    None
}
