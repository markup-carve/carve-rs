use crate::ast::*;
use crate::extension::Options;
use crate::render_text::strip_controls as strip_control_chars;
use std::collections::HashSet;

const MAX_RENDER_DEPTH: usize = 100;

fn trim_block_output(s: &str) -> &str {
    s.trim_matches(|c| c == '\n' || c == ' ')
}

thread_local! {
    /// Mode for the current render. A thread-local keeps this off every
    /// signature in the render tree; it is set once per render entry point
    /// and read only by the smart-punctuation arms.
    static SMART_TYPOGRAPHY: std::cell::Cell<crate::extension::SmartTypographyMode> =
        const { std::cell::Cell::new(crate::extension::SmartTypographyMode::Glyph) };
}

fn smart_punctuation_text(node: &crate::ast::SmartPunctuation) -> &str {
    if SMART_TYPOGRAPHY.with(std::cell::Cell::get) == crate::extension::SmartTypographyMode::Source
    {
        return &node.value;
    }

    smart_punctuation_glyph(node)
}

/// Render a document to Markdown, honouring `Options::smart_typography`. The
/// profile transform is applied to `doc` upstream (see `crate::prepare_doc`).
pub fn render_markdown_with_options(doc: &Document, options: &Options<'_>) -> String {
    render_markdown_inner(doc, options.smart_typography)
}

/// Render a document to Markdown with the default settings, so smart
/// typography renders as its glyph.
pub fn render_markdown(doc: &Document) -> String {
    render_markdown_inner(doc, crate::extension::SmartTypographyMode::Glyph)
}

fn render_markdown_inner(
    doc: &Document,
    smart_typography: crate::extension::SmartTypographyMode,
) -> String {
    SMART_TYPOGRAPHY.with(|cell| cell.set(smart_typography));
    let _abbr_guard = crate::abbr_budget::AbbrBudgetGuard::new(doc.source_len);
    let mut heading_ids = HashSet::new();
    let mut referenced_heading_ids = HashSet::new();
    // Footnote definition bodies are rendered as block content too, so their
    // headings and crossref links must be part of the heading-id / referenced-id
    // prepass; otherwise a heading referenced only from a footnote loses the
    // `{#id}` suffix needed to keep the link valid on reparse.
    let mut heading_pass = |block: &BlockNode, _: Option<&[InlineNode]>| {
        if let BlockNode::Heading(heading) = block {
            heading_ids.insert(heading_id(heading));
        }
    };
    walk_blocks(&doc.children, 0, &mut heading_pass);
    for body in doc.footnote_defs.values() {
        walk_blocks(body, 0, &mut heading_pass);
    }
    let mut ref_pass = |_: &BlockNode, inlines: Option<&[InlineNode]>| {
        if let Some(inlines) = inlines {
            walk_inlines(inlines, 0, &mut |node| {
                if let InlineNode::Link(link) = node {
                    if let Some(id) = fragment_id(&link.href) {
                        if heading_ids.contains(id) {
                            referenced_heading_ids.insert(id.to_string());
                        }
                    }
                }
            });
        }
    };
    walk_blocks(&doc.children, 0, &mut ref_pass);
    for body in doc.footnote_defs.values() {
        walk_blocks(body, 0, &mut ref_pass);
    }

    let mut ctx = MarkdownContext {
        heading_ids,
        referenced_heading_ids,
        list_depth: 0,
    };
    let out = render_blocks(&doc.children, &mut ctx, 0);
    let footnotes = render_footnote_defs(doc, &mut ctx);
    normalize(&format!("{out}{footnotes}"))
}

struct MarkdownContext {
    heading_ids: HashSet<String>,
    referenced_heading_ids: HashSet<String>,
    list_depth: usize,
}

fn render_block_inlines(nodes: &[InlineNode], ctx: &mut MarkdownContext) -> String {
    render_inlines(nodes, ctx, 0)
}

fn render_title_inlines(nodes: &[InlineNode], ctx: &mut MarkdownContext) -> String {
    let nodes = inline_nodes_without_strong(nodes);
    render_block_inlines(&nodes, ctx)
}

fn render_blocks(blocks: &[BlockNode], ctx: &mut MarkdownContext, depth: usize) -> String {
    if depth > MAX_RENDER_DEPTH {
        return String::new();
    }
    blocks
        .iter()
        .map(|block| render_block(block, ctx, depth))
        .collect()
}

fn render_block(node: &BlockNode, ctx: &mut MarkdownContext, depth: usize) -> String {
    if depth > MAX_RENDER_DEPTH {
        return String::new();
    }
    match node {
        BlockNode::Heading(heading) => {
            let text = flatten_heading_text(&render_block_inlines(&heading.children, ctx));
            let mut suffix = String::new();
            let id = heading_id(heading);
            if ctx.referenced_heading_ids.contains(&id) {
                suffix = format!(" {{#{id}}}");
            }
            format!("{} {text}{suffix}\n\n", "#".repeat(heading.level as usize))
        }
        BlockNode::Paragraph(paragraph) => {
            format!("{}\n\n", render_block_inlines(&paragraph.children, ctx))
        }
        BlockNode::CodeBlock(code) => {
            let content = strip_controls(&code.content);
            let fence = safe_fence(&content, 3);
            let mut info = code
                .lang
                .as_deref()
                .map(sanitize_code_lang)
                .filter(|lang| !lang.is_empty())
                .unwrap_or_default();
            if let Some(title) = &code.title {
                info.push_str(&format!(" \"{}\"", escape_code_title(title)));
            }
            // A grouping `[label]` rides along after the language and title.
            // Dropping it was silent data loss: an info string is free-form
            // after the first word, so every consumer ignores what it does not
            // understand, and carve-php was already emitting it (carve#352).
            if let Some(label) = &code.label {
                if !label.is_empty() {
                    let cleaned: String = label
                        .chars()
                        .filter(|c| !matches!(c, '[' | ']' | '`'))
                        .collect();
                    info.push_str(&format!(" [{cleaned}]"));
                }
            }
            format!("{}{}\n{}\n{}\n\n", fence, info, content, fence)
        }
        BlockNode::BlockQuote(quote) => {
            let lines = render_blocks(&quote.children, ctx, depth + 1);
            let body = trim_block_output(&lines)
                .split('\n')
                .map(|line| format!("> {line}"))
                .collect::<Vec<_>>()
                .join("\n");
            format!("{body}\n\n")
        }
        BlockNode::List(list) => render_list(list, ctx, depth + 1),
        BlockNode::ThematicBreak(_) => "---\n\n".to_string(),
        BlockNode::Table(table) => render_table(table, ctx),
        BlockNode::Admonition(admonition) => {
            // Markdown has no admonition; preserve the title (otherwise lost)
            // as a leading bold line, then the body.
            let body = render_blocks(&admonition.children, ctx, depth + 1);
            let body = match &admonition.title {
                Some(title) => {
                    let t = render_title_inlines(title, ctx);
                    if t.is_empty() {
                        body
                    } else {
                        format!("**{t}**\n\n{body}")
                    }
                }
                None => body,
            };
            prepend_label(body, admonition.label.as_deref())
        }
        BlockNode::LineBlock(lb) => render_blocks(&lb.children, ctx, depth + 1),
        BlockNode::Div(div) => {
            let body = render_blocks(&div.children, ctx, depth + 1);
            prepend_label(body, div.label.as_deref())
        }
        BlockNode::DefinitionList(list) => {
            render_definition_list(&list.items, ctx, true, depth + 1)
        }
        BlockNode::Figure(figure) => render_figure(figure, ctx, depth + 1),
        // A standalone block image is its own block: terminate it so the next
        // block is not glued onto the image (render_image stays newline-free
        // because it is shared with inline image rendering).
        BlockNode::BlockImage(image) => format!("{}\n\n", render_image(image)),
        BlockNode::RawBlock(raw) => {
            if raw.format == "html" {
                // Escape, not emit: raw HTML in Markdown would be live again
                // when the Markdown is rendered to HTML downstream.
                format!("{}\n\n", escape_md_html(&strip_controls(&raw.content)))
            } else {
                String::new()
            }
        }
        BlockNode::Extension(extension) => render_blocks(&extension.children, ctx, depth + 1),
        BlockNode::AbbreviationDef(_) | BlockNode::Comment(_) => String::new(),
    }
}

fn render_list(node: &List, ctx: &mut MarkdownContext, depth: usize) -> String {
    if depth > MAX_RENDER_DEPTH {
        return String::new();
    }
    ctx.list_depth += 1;
    let mut out = String::new();
    let mut counter = node.start.unwrap_or(1);
    for item in &node.items {
        let indent = "  ".repeat(ctx.list_depth - 1);
        let prefix = if node.ordered {
            let prefix = format!("{counter}. ");
            counter += 1;
            prefix
        } else if let Some(checked) = item.checked {
            if checked {
                "- [x] ".to_string()
            } else {
                "- [ ] ".to_string()
            }
        } else {
            "- ".to_string()
        };
        let content = trim_block_output(&render_blocks(&item.children, ctx, depth + 1)).to_string();
        let mut lines = content.split('\n');
        out.push_str(&format!(
            "{indent}{prefix}{}\n",
            lines.next().unwrap_or_default()
        ));
        let continuation = " ".repeat(prefix.len());
        for line in lines {
            out.push_str(&format!("{indent}{continuation}{line}\n"));
        }
    }
    ctx.list_depth -= 1;
    if ctx.list_depth == 0 {
        out.push('\n');
    }
    out
}

fn render_definition_list(
    items: &[DefinitionItem],
    ctx: &mut MarkdownContext,
    trailing_blank: bool,
    depth: usize,
) -> String {
    if depth > MAX_RENDER_DEPTH {
        return String::new();
    }
    let mut out = String::new();
    for item in items {
        for term in &item.terms {
            out.push_str(&format!("**{}**\n", render_block_inlines(term, ctx)));
        }
        for definition in &item.definitions {
            out.push_str(&format!(
                ": {}\n",
                trim_block_output(&render_blocks(definition, ctx, depth + 1))
            ));
        }
    }
    if trailing_blank {
        out.push('\n');
    }
    out
}

fn render_table(node: &Table, ctx: &mut MarkdownContext) -> String {
    let mut header = None;
    let mut rows = Vec::new();
    let mut columns = 0usize;
    // Per-column alignment from the first non-header row (matching carve-php),
    // so the Markdown separator preserves `:---` / `:---:` / `---:`.
    let mut aligns: Vec<Option<TableAlign>> = Vec::new();
    for row in &node.rows {
        let cells = row
            .cells
            .iter()
            .map(|cell| render_block_inlines(&cell.children, ctx).trim().to_string())
            .collect::<Vec<_>>();
        columns = columns.max(cells.len());
        let rendered = format!("| {} |", cells.join(" | "));
        if row.cells.iter().all(|cell| cell.header) {
            header = Some(rendered);
        } else {
            rows.push(rendered);
            for (i, cell) in row.cells.iter().enumerate() {
                if aligns.len() <= i {
                    aligns.resize(i + 1, None);
                }
                if aligns[i].is_none() {
                    aligns[i] = cell.align;
                }
            }
        }
    }
    let mut out = String::new();
    if let Some(header) = header {
        out.push_str(&header);
        out.push('\n');
        let sep = (0..columns)
            .map(|i| match aligns.get(i).copied().flatten() {
                Some(TableAlign::Left) => ":---",
                Some(TableAlign::Center) => ":---:",
                Some(TableAlign::Right) => "---:",
                None => "---",
            })
            .collect::<Vec<_>>()
            .join(" | ");
        out.push_str(&format!("| {sep} |\n"));
    }
    out.push_str(&rows.join("\n"));
    out.push_str("\n\n");
    out
}

fn render_figure(node: &Figure, ctx: &mut MarkdownContext, depth: usize) -> String {
    if depth > MAX_RENDER_DEPTH {
        return String::new();
    }
    let target = match &node.target {
        FigureTarget::Image(image) => render_image(image),
        FigureTarget::Table(table) => render_table(table, ctx).trim().to_string(),
        FigureTarget::BlockQuote(quote) => {
            render_block(&BlockNode::BlockQuote(quote.clone()), ctx, depth + 1)
                .trim()
                .to_string()
        }
        FigureTarget::CodeBlock(cb) => {
            render_block(&BlockNode::CodeBlock(cb.clone()), ctx, depth + 1)
                .trim()
                .to_string()
        }
        FigureTarget::Paragraph(p) => {
            render_block(&BlockNode::Paragraph(p.clone()), ctx, depth + 1)
                .trim()
                .to_string()
        }
    };
    // The caption sits on its own line directly under the figure (`\n`) - an
    // image target used to glue it on (`![a](/u)cap`). A blockquote target keeps
    // the blank-line separation; a table drops the caption entirely.
    let sep = match &node.target {
        FigureTarget::BlockQuote(_) => "\n\n",
        FigureTarget::Table(_) => "",
        _ => "\n",
    };
    // End with the block separator so a following block is not glued to the
    // caption (matching every other block renderer and carve-php).
    format!(
        "{target}{sep}{}\n\n",
        render_block_inlines(&node.caption, ctx)
    )
}

fn render_footnote_defs(doc: &Document, ctx: &mut MarkdownContext) -> String {
    let mut out = String::new();
    for (label, blocks) in &doc.footnote_defs {
        out.push_str(&format!(
            "[^{}]: {}\n",
            strip_controls(label),
            render_blocks(blocks, ctx, 0).trim()
        ));
    }
    out
}

fn render_inlines(nodes: &[InlineNode], ctx: &mut MarkdownContext, depth: usize) -> String {
    if depth > MAX_RENDER_DEPTH {
        return String::new();
    }
    let mut out = String::new();
    for node in nodes {
        out.push_str(&render_inline(node, ctx, depth));
    }
    out
}

fn render_inline(node: &InlineNode, ctx: &mut MarkdownContext, depth: usize) -> String {
    if depth > MAX_RENDER_DEPTH {
        return String::new();
    }
    match node {
        // Reproduce the author's escape. `\-\-` was written precisely so a
        // downstream processor with smart punctuation on would not read an en
        // dash; emitting the character bare loses exactly that (carve issue
        // 350). The underscore still goes through the sentinel so the intraword
        // rule can drop the backslash where CommonMark ignores it anyway.
        InlineNode::EscapedText(text) => {
            let ch = text.replace(crate::ESCAPED_CARET_PLACEHOLDER, "^");
            if ch == "_" {
                UNDERSCORE_ESCAPE.to_string()
            } else {
                format!("\\{ch}")
            }
        }
        InlineNode::Text(text) => {
            if is_literal_crossref(text) {
                strip_controls(text)
            } else {
                // The generated-NBSP placeholder (escaped space `\ ` / verse
                // indent) round-trips to a literal non-breaking space in
                // Markdown, matching the other renderers' source projection.
                escape_text(
                    &strip_controls(text)
                        .replace(crate::NBSP_PLACEHOLDER, "\u{00a0}")
                        .replace(crate::ESCAPED_CARET_PLACEHOLDER, "^"),
                )
            }
        }
        // Not escaped: a smart-typography run is either a glyph (nothing to
        // escape) or the author's source run, which must survive verbatim so a
        // reader searching for what was typed finds it. carve-php and carve-js
        // emit it unescaped for the same reason; escaping here turned `->`
        // into `-&gt;` in source mode.
        InlineNode::SmartPunctuation(s) => strip_controls(smart_punctuation_text(s)),
        InlineNode::Emphasis(emphasis) => match emphasis.kind {
            EmphasisKind::Italic => {
                format!("*{}*", render_inlines(&emphasis.children, ctx, depth + 1))
            }
            EmphasisKind::Strong => {
                format!("**{}**", render_inlines(&emphasis.children, ctx, depth + 1))
            }
            EmphasisKind::Underline => {
                format!(
                    "<u>{}</u>",
                    render_inlines(&emphasis.children, ctx, depth + 1)
                )
            }
            EmphasisKind::Strike => {
                format!("~~{}~~", render_inlines(&emphasis.children, ctx, depth + 1))
            }
            EmphasisKind::Sub => {
                format!(
                    "<sub>{}</sub>",
                    render_inlines(&emphasis.children, ctx, depth + 1)
                )
            }
            EmphasisKind::Super => {
                format!(
                    "<sup>{}</sup>",
                    render_inlines(&emphasis.children, ctx, depth + 1)
                )
            }
            EmphasisKind::Highlight => {
                format!(
                    "<mark>{}</mark>",
                    render_inlines(&emphasis.children, ctx, depth + 1)
                )
            }
            EmphasisKind::BoldItalic => {
                format!(
                    "***{}***",
                    render_inlines(&emphasis.children, ctx, depth + 1)
                )
            }
        },
        InlineNode::Code(code, _) => render_code(&strip_controls(code)),
        InlineNode::Link(link) => render_link(link, ctx, depth + 1),
        InlineNode::Image(image) => render_image(image),
        InlineNode::Span(span) => render_inlines(&span.children, ctx, depth + 1),
        InlineNode::Math(math) => {
            let content = strip_controls(&math.content);
            if math.display {
                format!("$${content}$$")
            } else {
                format!("${content}$")
            }
        }
        InlineNode::RawInline(raw) => {
            if raw.format == "html" {
                escape_md_html(&strip_controls(&raw.content))
            } else {
                String::new()
            }
        }
        InlineNode::LiteralInline(lit) => {
            // §27: emitted by EVERY renderer, never dropped. It is prose, not
            // code, so no code fence -- the content becomes literal text with
            // Markdown metacharacters escaped so `*not bold*` stays visible.
            escape_text(&strip_controls(&lit.content))
        }
        InlineNode::Symbol(symbol) => format!(":{}:", symbol.name),
        InlineNode::AutoLink(link) => format!(
            "[{}]({})",
            strip_controls(&link.text),
            encode_markdown_destination(&link.href)
        ),
        InlineNode::Mention(mention) => format!("@{}", strip_controls(&mention.user)),
        InlineNode::Tag(tag) => escape_text(&format!("#{}", strip_controls(&tag.name))),
        InlineNode::Extension(extension) => render_inlines(&extension.children, ctx, depth + 1),
        InlineNode::Abbreviation(abbr) => {
            // Markdown has no abbreviation syntax; emit an HTML <abbr> so the
            // title survives (markdown allows inline HTML), matching carve-php.
            // Dropping it to plain text would lose the expansion.
            let abbr_text = strip_controls(&abbr.abbr);
            let text = abbr_text
                .as_str()
                .replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;");
            // Bound cumulative expansion bytes (memory-amplification DoS): once
            // the budget is exhausted, degrade to plain key text with no title.
            if crate::abbr_budget::try_spend(abbr.expansion.len()) {
                let expansion = strip_controls(&abbr.expansion);
                let title = expansion
                    .as_str()
                    .replace('&', "&amp;")
                    .replace('<', "&lt;")
                    .replace('>', "&gt;")
                    .replace('"', "&quot;");
                format!("<abbr title=\"{title}\">{text}</abbr>")
            } else {
                text
            }
        }
        InlineNode::Footnote(footnote) => {
            if let Some(inline) = &footnote.inline {
                let rendered = render_inlines(inline, ctx, depth + 1);
                format!("^[{rendered}]")
            } else {
                format!(
                    "[^{}]",
                    strip_controls(footnote.id.as_deref().unwrap_or(""))
                )
            }
        }
        InlineNode::SoftBreak => "\n".to_string(),
        InlineNode::HardBreak => "  \n".to_string(),
        InlineNode::CriticInsert(insert) => {
            format!(
                "<ins>{}</ins>",
                render_inlines(&insert.children, ctx, depth + 1)
            )
        }
        InlineNode::CriticDelete(delete) => {
            format!(
                "<del>{}</del>",
                render_inlines(&delete.children, ctx, depth + 1)
            )
        }
        InlineNode::CriticSubstitute(sub) => format!(
            "<del>{}</del><ins>{}</ins>",
            escape_text(&strip_controls(&sub.old_text)),
            escape_text(&strip_controls(&sub.new_text))
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

fn render_link(node: &Link, ctx: &mut MarkdownContext, depth: usize) -> String {
    let text = render_inlines(&node.children, ctx, depth);
    if let Some(id) = fragment_id(&node.href) {
        if !ctx.heading_ids.contains(id) {
            return text;
        }
        let destination = encode_markdown_destination(&format!("#{id}"));
        if let Some(title) = &node.title {
            format!(
                "[{text}]({destination} \"{}\")",
                escape_md_title(&strip_controls(title))
            )
        } else {
            format!("[{text}]({destination})")
        }
    } else {
        let href = encode_markdown_destination(&node.href);
        if let Some(title) = &node.title {
            format!(
                "[{text}]({href} \"{}\")",
                escape_md_title(&strip_controls(title))
            )
        } else {
            format!("[{text}]({href})")
        }
    }
}

fn render_image(node: &Image) -> String {
    let src = encode_markdown_destination(&node.src);
    let alt = escape_md_label(&strip_controls(&node.alt));
    if let Some(title) = &node.title {
        format!(
            "![{}]({} \"{}\")",
            alt,
            src,
            escape_md_title(&strip_controls(title))
        )
    } else {
        format!("![{}]({})", alt, src)
    }
}

fn escape_md_title(title: &str) -> String {
    title.replace('\\', "\\\\").replace('"', "\\\"")
}

fn escape_md_label(label: &str) -> String {
    label
        .replace('\\', "\\\\")
        .replace('[', "\\[")
        .replace(']', "\\]")
}

fn sanitize_code_lang(lang: &str) -> String {
    // Keep only the first whitespace-delimited token (the language word); drop
    // it if it still contains a backtick (would break the fence).
    let token = strip_controls(lang)
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_string();
    if token.contains('`') {
        String::new()
    } else {
        token
    }
}

fn escape_code_title(title: &str) -> String {
    strip_controls(title)
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
}

fn safe_fence(content: &str, min: usize) -> String {
    let mut longest = 0usize;
    let mut current = 0usize;
    for ch in content.chars() {
        if ch == '`' {
            current += 1;
            longest = longest.max(current);
        } else {
            current = 0;
        }
    }
    "`".repeat(min.max(longest + 1))
}

fn render_code(content: &str) -> String {
    let fence = safe_fence(content, 1);
    if content.starts_with('`') || content.ends_with('`') {
        format!("{fence} {content} {fence}")
    } else {
        format!("{fence}{content}{fence}")
    }
}

fn fragment_id(href: &str) -> Option<&str> {
    href.strip_prefix('#')
}

/// Graceful degradation: when no extension consumed the grouping `[label]`,
/// surface it as a leading bold line (mirroring how an admonition title
/// renders) so the authored label is never silently dropped in Markdown.
fn prepend_label(body: String, label: Option<&str>) -> String {
    match label {
        Some(label) if !label.is_empty() => {
            let l = escape_text(label);
            if body.is_empty() {
                format!("**{l}**\n\n")
            } else {
                format!("**{l}**\n\n{body}")
            }
        }
        _ => body,
    }
}

fn escape_text(text: &str) -> String {
    let mut out = String::new();
    for ch in text.chars() {
        match ch {
            // Neutralize embedded HTML so Markdown re-rendered to HTML cannot
            // execute it (carve's "HTML is text" guarantee for the Markdown
            // target too): a literal `<img onerror=…>` becomes inert.
            '&' => {
                out.push_str("&amp;");
                continue;
            }
            '<' => {
                out.push_str("&lt;");
                continue;
            }
            '>' => {
                out.push_str("&gt;");
                continue;
            }
            // The underscore escape is emitted as a sentinel rather than a
            // backslash: whether it survives depends on its neighbours in the
            // assembled document, which only resolve_underscore_escapes() can
            // see. See UNDERSCORE_ESCAPE.
            '_' => {
                out.push(UNDERSCORE_ESCAPE);
                continue;
            }
            // Markdown metacharacters.
            '\\' | '`' | '*' | '[' | ']' | '#' => out.push('\\'),
            _ => {}
        }
        out.push(ch);
    }
    out
}

/// Sentinel standing in for an underscore escape this renderer emitted, so the
/// final pass can tell those apart from a backslash the author wrote. U+E000 is
/// the NBSP sentinel and the Carve writer claims U+E001..U+E003; this extends
/// the scheme. Author content never carries it: strip_controls() drops it on
/// the way in, and every path to the output runs through strip_controls().
const UNDERSCORE_ESCAPE: char = '\u{E004}';

/// Drop control characters from author content, and the underscore-escape
/// sentinel with them: author content that carried it would otherwise be read
/// as an escape this renderer emitted. Every path to the output passes here.
fn strip_controls(input: &str) -> String {
    strip_control_chars(&input.replace(UNDERSCORE_ESCAPE, ""))
}

/// Escape `<>&` so embedded raw HTML cannot become live markup downstream.
fn escape_md_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Blank a URL whose (normalized) scheme is on the dangerous denylist, so a
/// `javascript:` link/image does not survive into Markdown output.
fn sanitize_md_url(url: &str) -> String {
    let probe: String = url.chars().filter(|c| (*c as u32) > 0x20).collect();
    if let Some(colon) = probe.find(':') {
        let prefix = &probe[..colon];
        let is_scheme = prefix
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic())
            && prefix
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.');
        if is_scheme
            && matches!(
                prefix.to_ascii_lowercase().as_str(),
                "javascript" | "vbscript" | "data" | "file"
            )
        {
            return String::new();
        }
    }
    url.to_string()
}

fn encode_markdown_destination(url: &str) -> String {
    let sanitized = sanitize_md_url(url);
    let mut out = String::new();
    for ch in sanitized.chars() {
        match ch {
            ' ' => out.push_str("%20"),
            '(' => out.push_str("%28"),
            ')' => out.push_str("%29"),
            '<' => out.push_str("%3C"),
            '>' => out.push_str("%3E"),
            _ => out.push(ch),
        }
    }
    strip_controls(&out)
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
    let collapsed = format!("{}\n", out.trim_matches(|c| c == '\n' || c == ' '));

    resolve_underscore_escapes(&collapsed)
}

/// Resolve the underscore escapes, dropping the backslash from an intraword one.
///
/// CommonMark does not honour an intraword underscore, so `company_id`
/// renders literally with or without the escape - the backslash only litters
/// identifiers in output meant to be read and searched. An asterisk is NOT
/// symmetric here (`a*b*c` does emphasise), so this applies to `_` alone.
///
/// Runs on the assembled output rather than in `escape_text` because whether
/// an underscore is intraword is a property of the rendered stream, not of one
/// node: the parser splits `company_id` into the text nodes `company` and
/// `_id`, so at escape time the underscore looks like it starts a word.
///
/// It decides on the sentinel rather than on `\_` because the assembled
/// document also contains regions this renderer must reproduce byte-exact -
/// code spans, code blocks, link destinations, titles, raw HTML - and a
/// backslash there is content, not an escape. Matching `\_` rewrote those too
/// (carve-js issue 400).
fn resolve_underscore_escapes(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0usize;

    while i < chars.len() {
        let has_word_before = i > 0 && chars[i - 1].is_alphanumeric();
        let has_word_after = chars.get(i + 1).is_some_and(|c| c.is_alphanumeric());

        if chars[i] == UNDERSCORE_ESCAPE {
            out.push_str(if has_word_before && has_word_after {
                "_"
            } else {
                "\\_"
            });
            i += 1;
            continue;
        }

        out.push(chars[i]);
        i += 1;
    }

    out
}

fn flatten_heading_text(text: &str) -> String {
    text.split('\n')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn heading_id(heading: &Heading) -> String {
    if let Some(attrs) = &heading.attrs {
        if let Some(id) = &attrs.id {
            return id.clone();
        }
    }
    slugify(&plain_inlines(&heading.children))
}

// Markdown-specific flattening. Node coverage is kept in lockstep with the
// core, including `CitationGroup` -> `raw`, so a citation heading's id is
// consistent here too.
fn plain_inlines(nodes: &[InlineNode]) -> String {
    let mut out = String::new();
    for node in nodes {
        match node {
            InlineNode::Text(text) => out.push_str(
                &text
                    .replace(crate::NBSP_PLACEHOLDER, " ")
                    .replace(crate::ESCAPED_CARET_PLACEHOLDER, "^"),
            ),
            InlineNode::SmartPunctuation(s) => out.push_str(smart_punctuation_text(s)),
            InlineNode::Emphasis(emphasis) => out.push_str(&plain_inlines(&emphasis.children)),
            InlineNode::Code(code, _) => out.push_str(code),
            // An inline literal renders as visible prose (§27), so it feeds a
            // Markdown heading slug like a code span does.
            InlineNode::LiteralInline(lit) => out.push_str(&lit.content),
            InlineNode::Link(link) => out.push_str(&plain_inlines(&link.children)),
            InlineNode::Image(image) => out.push_str(&image.alt),
            InlineNode::Extension(extension) => out.push_str(&plain_inlines(&extension.children)),
            InlineNode::CitationGroup(group) => out.push_str(&group.raw),
            InlineNode::Abbreviation(abbr) => out.push_str(&abbr.abbr),
            InlineNode::Mention(mention) => out.push_str(&mention.user),
            InlineNode::Tag(tag) => out.push_str(&tag.name),
            InlineNode::CaptionNumber(number) => {
                if let Some(number) = number.number {
                    out.push_str(&number.to_string());
                }
            }
            InlineNode::SoftBreak | InlineNode::HardBreak => out.push(' '),
            _ => {}
        }
    }
    out
}

fn slugify(text: &str) -> String {
    // Delegate to the single canonical implementation so HTML, Markdown, and
    // the parser's id index never drift apart (or from carve-js / carve-php).
    // The Markdown renderer has no Options, so it always uses the case-preserving
    // default (lowercase = false), matching the parser's default id index.
    crate::parse::slugify_parse(text, false)
}

fn is_literal_crossref(text: &str) -> bool {
    text.starts_with("</#") && text.ends_with('>') && !text[3..text.len() - 1].contains('>')
}

fn walk_blocks<F>(blocks: &[BlockNode], depth: usize, visit: &mut F)
where
    F: FnMut(&BlockNode, Option<&[InlineNode]>),
{
    if depth > MAX_RENDER_DEPTH {
        return;
    }
    for block in blocks {
        visit(block, None);
        match block {
            BlockNode::Heading(heading) => visit(block, Some(&heading.children)),
            BlockNode::Paragraph(paragraph) => visit(block, Some(&paragraph.children)),
            BlockNode::BlockQuote(quote) => walk_blocks(&quote.children, depth + 1, visit),
            BlockNode::Admonition(admonition) => {
                // The title is now rendered, so a crossref link in it must be
                // seen by the prepass that collects referenced heading ids.
                if let Some(title) = &admonition.title {
                    visit(block, Some(title));
                }
                walk_blocks(&admonition.children, depth + 1, visit);
            }
            BlockNode::Div(div) => walk_blocks(&div.children, depth + 1, visit),
            BlockNode::LineBlock(lb) => walk_blocks(&lb.children, depth + 1, visit),
            BlockNode::List(list) => {
                for item in &list.items {
                    walk_blocks(&item.children, depth + 1, visit);
                }
            }
            BlockNode::DefinitionList(list) => {
                for item in &list.items {
                    for term in &item.terms {
                        visit(block, Some(term));
                    }
                    for definition in &item.definitions {
                        walk_blocks(definition, depth + 1, visit);
                    }
                }
            }
            BlockNode::Table(table) => {
                if let Some(caption) = &table.caption {
                    visit(block, Some(caption));
                }
                for row in &table.rows {
                    for cell in &row.cells {
                        visit(block, Some(&cell.children));
                    }
                }
            }
            BlockNode::Figure(figure) => {
                visit(block, Some(&figure.caption));
                match &figure.target {
                    FigureTarget::BlockQuote(quote) => {
                        walk_blocks(&quote.children, depth + 1, visit)
                    }
                    FigureTarget::Table(table) => {
                        walk_blocks(&[BlockNode::Table(table.clone())], depth + 1, visit);
                    }
                    FigureTarget::Paragraph(p) => {
                        walk_blocks(&[BlockNode::Paragraph(p.clone())], depth + 1, visit);
                    }
                    FigureTarget::Image(_) | FigureTarget::CodeBlock(_) => {}
                }
            }
            BlockNode::Extension(extension) => walk_blocks(&extension.children, depth + 1, visit),
            _ => {}
        }
    }
}

fn walk_inlines<F>(nodes: &[InlineNode], depth: usize, visit: &mut F)
where
    F: FnMut(&InlineNode),
{
    if depth > MAX_RENDER_DEPTH {
        return;
    }
    for node in nodes {
        visit(node);
        match node {
            InlineNode::Emphasis(emphasis) => walk_inlines(&emphasis.children, depth + 1, visit),
            InlineNode::Link(link) => walk_inlines(&link.children, depth + 1, visit),
            InlineNode::Span(span) => walk_inlines(&span.children, depth + 1, visit),
            InlineNode::Extension(extension) => walk_inlines(&extension.children, depth + 1, visit),
            InlineNode::Footnote(footnote) => {
                if let Some(inline) = &footnote.inline {
                    walk_inlines(inline, depth + 1, visit);
                }
            }
            InlineNode::CriticInsert(insert) => walk_inlines(&insert.children, depth + 1, visit),
            InlineNode::CriticDelete(delete) => walk_inlines(&delete.children, depth + 1, visit),
            _ => {}
        }
    }
}
