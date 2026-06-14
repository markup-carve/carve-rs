use crate::ast::*;
use crate::render_text::clean_smart_text;
use std::collections::HashSet;

pub fn render_markdown(doc: &Document) -> String {
    let mut heading_ids = HashSet::new();
    let mut referenced_heading_ids = HashSet::new();
    walk_blocks(&doc.children, &mut |block, _| {
        if let BlockNode::Heading(heading) = block {
            heading_ids.insert(heading_id(heading));
        }
    });
    walk_blocks(&doc.children, &mut |_, inlines| {
        if let Some(inlines) = inlines {
            walk_inlines(inlines, &mut |node| {
                if let InlineNode::Link(link) = node {
                    if let Some(id) = fragment_id(&link.href) {
                        if heading_ids.contains(id) {
                            referenced_heading_ids.insert(id.to_string());
                        }
                    }
                }
            });
        }
    });

    let mut ctx = MarkdownContext {
        heading_ids,
        referenced_heading_ids,
        list_depth: 0,
    };
    let out = render_blocks(&doc.children, &mut ctx);
    let footnotes = render_footnote_defs(doc, &mut ctx);
    normalize(&format!("{out}{footnotes}"))
}

struct MarkdownContext {
    heading_ids: HashSet<String>,
    referenced_heading_ids: HashSet<String>,
    list_depth: usize,
}

fn render_blocks(blocks: &[BlockNode], ctx: &mut MarkdownContext) -> String {
    blocks
        .iter()
        .map(|block| render_block(block, ctx))
        .collect()
}

fn render_block(node: &BlockNode, ctx: &mut MarkdownContext) -> String {
    match node {
        BlockNode::Heading(heading) => {
            let text = flatten_heading_text(&render_inlines(&heading.children, ctx));
            let mut suffix = String::new();
            let id = heading_id(heading);
            if ctx.referenced_heading_ids.contains(&id) {
                suffix = format!(" {{#{id}}}");
            }
            format!("{} {text}{suffix}\n\n", "#".repeat(heading.level as usize))
        }
        BlockNode::Paragraph(paragraph) => {
            if let Some((term, def)) = legacy_definition_parts(&paragraph.children) {
                return format!("**{}**\n: {}\n\n", escape_text(&term), escape_text(&def));
            }
            format!("{}\n\n", render_inlines(&paragraph.children, ctx))
        }
        BlockNode::CodeBlock(code) => {
            let fence = safe_fence(&code.content, 3);
            format!(
                "{}{}\n{}\n{}\n\n",
                fence,
                code.lang.as_deref().unwrap_or(""),
                code.content,
                fence
            )
        }
        BlockNode::BlockQuote(quote) => {
            let lines = render_blocks(&quote.children, ctx);
            let body = lines
                .trim()
                .split('\n')
                .map(|line| format!("> {line}"))
                .collect::<Vec<_>>()
                .join("\n");
            format!("{body}\n\n")
        }
        BlockNode::List(list) => render_list(list, ctx),
        BlockNode::ThematicBreak(_) => "---\n\n".to_string(),
        BlockNode::Table(table) => render_table(table, ctx),
        BlockNode::Admonition(admonition) => {
            // Markdown has no admonition; preserve the title (otherwise lost)
            // as a leading bold line, then the body.
            let body = render_blocks(&admonition.children, ctx);
            match &admonition.title {
                Some(title) => {
                    let t = render_inlines(title, ctx);
                    if t.is_empty() {
                        body
                    } else {
                        format!("**{t}**\n\n{body}")
                    }
                }
                None => body,
            }
        }
        BlockNode::Div(div) => render_blocks(&div.children, ctx),
        BlockNode::DefinitionList(list) => render_definition_list(&list.items, ctx, true),
        BlockNode::Figure(figure) => render_figure(figure, ctx),
        // A standalone block image is its own block: terminate it so the next
        // block is not glued onto the image (render_image stays newline-free
        // because it is shared with inline image rendering).
        BlockNode::BlockImage(image) => format!("{}\n\n", render_image(image)),
        BlockNode::RawBlock(raw) => {
            if raw.format == "html" {
                format!("{}\n\n", raw.content)
            } else {
                String::new()
            }
        }
        BlockNode::Extension(extension) => render_blocks(&extension.children, ctx),
        BlockNode::AbbreviationDef(_) | BlockNode::Comment(_) => String::new(),
    }
}

fn render_list(node: &List, ctx: &mut MarkdownContext) -> String {
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
        let content = render_blocks(&item.children, ctx).trim().to_string();
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
) -> String {
    let mut out = String::new();
    for item in items {
        for term in &item.terms {
            out.push_str(&format!("**{}**\n", render_inlines(term, ctx)));
        }
        for definition in &item.definitions {
            out.push_str(&format!(": {}\n", render_blocks(definition, ctx).trim()));
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
    for row in &node.rows {
        let cells = row
            .cells
            .iter()
            .map(|cell| render_inlines(&cell.children, ctx).trim().to_string())
            .collect::<Vec<_>>();
        columns = columns.max(cells.len());
        let rendered = format!("| {} |", cells.join(" | "));
        if row.cells.iter().all(|cell| cell.header) {
            header = Some(rendered);
        } else {
            rows.push(rendered);
        }
    }
    let mut out = String::new();
    if let Some(header) = header {
        out.push_str(&header);
        out.push('\n');
        out.push_str(&format!("| {} |\n", vec!["---"; columns].join(" | ")));
    }
    out.push_str(&rows.join("\n"));
    out.push_str("\n\n");
    out
}

fn render_figure(node: &Figure, ctx: &mut MarkdownContext) -> String {
    let target = match &node.target {
        FigureTarget::Image(image) => render_image(image),
        FigureTarget::Table(table) => render_table(table, ctx).trim().to_string(),
        FigureTarget::BlockQuote(quote) => render_block(&BlockNode::BlockQuote(quote.clone()), ctx)
            .trim()
            .to_string(),
        FigureTarget::CodeBlock(cb) => render_block(&BlockNode::CodeBlock(cb.clone()), ctx)
            .trim()
            .to_string(),
        FigureTarget::Paragraph(p) => render_block(&BlockNode::Paragraph(p.clone()), ctx)
            .trim()
            .to_string(),
    };
    // A block-level target (a code-block listing or a display-math equation)
    // keeps the caption on its own line; an inline image stays adjacent.
    let sep = match &node.target {
        FigureTarget::CodeBlock(_) | FigureTarget::Paragraph(_) => "\n",
        _ => "",
    };
    format!("{target}{sep}{}", render_inlines(&node.caption, ctx))
}

fn render_footnote_defs(doc: &Document, ctx: &mut MarkdownContext) -> String {
    let mut out = String::new();
    for (label, blocks) in &doc.footnote_defs {
        out.push_str(&format!(
            "[^{label}]: {}\n",
            render_blocks(blocks, ctx).trim()
        ));
    }
    out
}

fn render_inlines(nodes: &[InlineNode], ctx: &mut MarkdownContext) -> String {
    nodes.iter().map(|node| render_inline(node, ctx)).collect()
}

fn render_inline(node: &InlineNode, ctx: &mut MarkdownContext) -> String {
    match node {
        InlineNode::Text(text) => {
            if is_literal_crossref(text) {
                text.clone()
            } else {
                escape_text(&clean_smart_text(text))
            }
        }
        InlineNode::Emphasis(emphasis) => match emphasis.kind {
            EmphasisKind::Italic => format!("*{}*", render_inlines(&emphasis.children, ctx)),
            EmphasisKind::Strong => format!("**{}**", render_inlines(&emphasis.children, ctx)),
            EmphasisKind::Underline => {
                format!("<u>{}</u>", render_inlines(&emphasis.children, ctx))
            }
            EmphasisKind::Strike | EmphasisKind::Sub => {
                format!("~~{}~~", render_inlines(&emphasis.children, ctx))
            }
            EmphasisKind::Super => {
                format!("<sup>{}</sup>", render_inlines(&emphasis.children, ctx))
            }
            EmphasisKind::Highlight => {
                format!("<mark>{}</mark>", render_inlines(&emphasis.children, ctx))
            }
            EmphasisKind::BoldItalic => {
                format!("***{}***", render_inlines(&emphasis.children, ctx))
            }
        },
        InlineNode::Code(code, _) => render_code(code),
        InlineNode::Link(link) => render_link(link, ctx),
        InlineNode::Image(image) => render_image(image),
        InlineNode::Span(span) => render_inlines(&span.children, ctx),
        InlineNode::Math(math) => {
            if math.display {
                format!("$${}$$", math.content)
            } else {
                format!("${}$", math.content)
            }
        }
        InlineNode::RawInline(raw) => {
            if raw.format == "html" {
                raw.content.clone()
            } else {
                String::new()
            }
        }
        InlineNode::Emoji(emoji) => format!(":{}:", emoji.name),
        InlineNode::AutoLink(link) => format!("[{}]({})", link.href, link.href),
        InlineNode::Mention(mention) => format!("@{}", mention.user),
        InlineNode::Tag(tag) => escape_text(&format!("#{}", tag.name)),
        InlineNode::Extension(extension) => render_inlines(&extension.children, ctx),
        InlineNode::Abbreviation(abbr) => {
            // Markdown has no abbreviation syntax; emit an HTML <abbr> so the
            // title survives (markdown allows inline HTML), matching carve-php.
            // Dropping it to plain text would lose the expansion.
            let title = abbr
                .expansion
                .replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;")
                .replace('"', "&quot;");
            let text = abbr
                .abbr
                .replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;");
            format!("<abbr title=\"{title}\">{text}</abbr>")
        }
        InlineNode::Footnote(footnote) => {
            if let Some(inline) = &footnote.inline {
                format!("^[{}]", render_inlines(inline, ctx))
            } else {
                format!("[^{}]", footnote.id.as_deref().unwrap_or(""))
            }
        }
        InlineNode::SoftBreak => "\n".to_string(),
        InlineNode::HardBreak => "  \n".to_string(),
        InlineNode::CriticInsert(insert) => {
            format!("<ins>{}</ins>", render_inlines(&insert.children, ctx))
        }
        InlineNode::CriticDelete(delete) => {
            format!("~~{}~~", render_inlines(&delete.children, ctx))
        }
        InlineNode::CriticSubstitute(sub) => format!(
            "<del>{}</del><ins>{}</ins>",
            escape_text(&sub.old_text),
            escape_text(&sub.new_text)
        ),
        InlineNode::CriticComment(_) => String::new(),
        InlineNode::CrossRef(crossref) => format!("</#{}>", crossref.target),
        InlineNode::CaptionNumber(number) => number
            .number
            .map(|n| n.to_string())
            .unwrap_or_else(|| "#".to_string()),
    }
}

fn render_link(node: &Link, ctx: &mut MarkdownContext) -> String {
    let text = render_inlines(&node.children, ctx);
    if let Some(id) = fragment_id(&node.href) {
        if !ctx.heading_ids.contains(id) {
            return text;
        }
        let destination = markdown_fragment_destination(id);
        if let Some(title) = &node.title {
            format!("[{text}]({destination} \"{title}\")")
        } else {
            format!("[{text}]({destination})")
        }
    } else if let Some(title) = &node.title {
        format!("[{text}]({} \"{}\")", node.href, title)
    } else {
        format!("[{text}]({})", node.href)
    }
}

fn render_image(node: &Image) -> String {
    if let Some(title) = &node.title {
        format!("![{}]({} \"{}\")", node.alt, node.src, title)
    } else {
        format!("![{}]({})", node.alt, node.src)
    }
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

fn markdown_fragment_destination(id: &str) -> String {
    if !id
        .chars()
        .any(|ch| matches!(ch, ' ' | '(' | ')' | '<' | '>'))
    {
        return format!("#{id}");
    }
    let escaped = id
        .replace('\\', "\\\\")
        .replace('<', "\\<")
        .replace('>', "\\>");
    format!("<#{escaped}>")
}

fn fragment_id(href: &str) -> Option<&str> {
    href.strip_prefix('#')
}

fn escape_text(text: &str) -> String {
    let mut out = String::new();
    for ch in text.chars() {
        if matches!(ch, '\\' | '`' | '*' | '_' | '[' | ']' | '#') {
            out.push('\\');
        }
        out.push(ch);
    }
    out
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
    format!("{}\n", out.trim())
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

fn plain_inlines(nodes: &[InlineNode]) -> String {
    let mut out = String::new();
    for node in nodes {
        match node {
            InlineNode::Text(text) => out.push_str(&clean_smart_text(text)),
            InlineNode::Emphasis(emphasis) => out.push_str(&plain_inlines(&emphasis.children)),
            InlineNode::Code(code, _) => out.push_str(code),
            InlineNode::Link(link) => out.push_str(&plain_inlines(&link.children)),
            InlineNode::Image(image) => out.push_str(&image.alt),
            InlineNode::Extension(extension) => out.push_str(&plain_inlines(&extension.children)),
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
    let mut out = String::new();
    let mut last_dash = false;
    for ch in text.chars() {
        if ch.is_alphanumeric() {
            for lc in ch.to_lowercase() {
                out.push(lc);
            }
            last_dash = false;
        } else if !last_dash && !out.is_empty() {
            out.push('-');
            last_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.chars().next().is_some_and(|ch| ch.is_ascii_digit()) {
        out = format!("s-{out}");
    }
    if out.is_empty() {
        "section".to_string()
    } else {
        out
    }
}

fn is_literal_crossref(text: &str) -> bool {
    text.starts_with("</#") && text.ends_with('>') && !text[3..text.len() - 1].contains('>')
}

fn walk_blocks<F>(blocks: &[BlockNode], visit: &mut F)
where
    F: FnMut(&BlockNode, Option<&[InlineNode]>),
{
    for block in blocks {
        visit(block, None);
        match block {
            BlockNode::Heading(heading) => visit(block, Some(&heading.children)),
            BlockNode::Paragraph(paragraph) => visit(block, Some(&paragraph.children)),
            BlockNode::BlockQuote(quote) => walk_blocks(&quote.children, visit),
            BlockNode::Admonition(admonition) => {
                // The title is now rendered, so a crossref link in it must be
                // seen by the prepass that collects referenced heading ids.
                if let Some(title) = &admonition.title {
                    visit(block, Some(title));
                }
                walk_blocks(&admonition.children, visit);
            }
            BlockNode::Div(div) => walk_blocks(&div.children, visit),
            BlockNode::List(list) => {
                for item in &list.items {
                    walk_blocks(&item.children, visit);
                }
            }
            BlockNode::DefinitionList(list) => {
                for item in &list.items {
                    for term in &item.terms {
                        visit(block, Some(term));
                    }
                    for definition in &item.definitions {
                        walk_blocks(definition, visit);
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
                    FigureTarget::BlockQuote(quote) => walk_blocks(&quote.children, visit),
                    FigureTarget::Table(table) => {
                        walk_blocks(&[BlockNode::Table(table.clone())], visit);
                    }
                    FigureTarget::Paragraph(p) => {
                        walk_blocks(&[BlockNode::Paragraph(p.clone())], visit);
                    }
                    FigureTarget::Image(_) | FigureTarget::CodeBlock(_) => {}
                }
            }
            BlockNode::Extension(extension) => walk_blocks(&extension.children, visit),
            _ => {}
        }
    }
}

fn walk_inlines<F>(nodes: &[InlineNode], visit: &mut F)
where
    F: FnMut(&InlineNode),
{
    for node in nodes {
        visit(node);
        match node {
            InlineNode::Emphasis(emphasis) => walk_inlines(&emphasis.children, visit),
            InlineNode::Link(link) => walk_inlines(&link.children, visit),
            InlineNode::Span(span) => walk_inlines(&span.children, visit),
            InlineNode::Extension(extension) => walk_inlines(&extension.children, visit),
            InlineNode::Footnote(footnote) => {
                if let Some(inline) = &footnote.inline {
                    walk_inlines(inline, visit);
                }
            }
            InlineNode::CriticInsert(insert) => walk_inlines(&insert.children, visit),
            InlineNode::CriticDelete(delete) => walk_inlines(&delete.children, visit),
            _ => {}
        }
    }
}
