use crate::ast::*;

const MAX_RENDER_DEPTH: usize = 200;

struct CarveContext {
    block_depth: usize,
    inline_depth: usize,
    list_depth: usize,
    smart_state: crate::render_text::SmartQuoteState,
}

pub fn render_carve(doc: &Document) -> String {
    let mut ctx = CarveContext {
        block_depth: 0,
        inline_depth: 0,
        list_depth: 0,
        smart_state: crate::render_text::SmartQuoteState::new(),
    };
    let mut parts = Vec::new();
    if !doc.frontmatter.is_empty() {
        parts.push(render_frontmatter(&doc.frontmatter));
    }
    let body = render_blocks(&doc.children, &mut ctx);
    if !body.is_empty() {
        parts.push(body);
    }
    let footnotes = render_footnote_defs(doc, &mut ctx);
    if !footnotes.is_empty() {
        parts.push(footnotes);
    }
    normalize(&parts.join("\n\n"))
}

fn render_blocks(blocks: &[BlockNode], ctx: &mut CarveContext) -> String {
    if ctx.block_depth >= MAX_RENDER_DEPTH {
        return String::new();
    }
    ctx.block_depth += 1;
    let out = blocks
        .iter()
        .map(|block| render_block(block, ctx))
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    ctx.block_depth -= 1;
    out
}

fn render_block(node: &BlockNode, ctx: &mut CarveContext) -> String {
    ctx.smart_state = crate::render_text::SmartQuoteState::new();
    match node {
        BlockNode::Heading(heading) => with_block_attrs(
            &heading.attrs,
            &format!(
                "{} {}",
                "#".repeat(heading.level as usize),
                trim_non_nbsp(&render_inlines(&heading.children, ctx))
            ),
        ),
        BlockNode::Paragraph(paragraph) => {
            with_block_attrs(&paragraph.attrs, &render_inlines(&paragraph.children, ctx))
        }
        BlockNode::CodeBlock(code) => {
            let fence = safe_fence(&code.content, 3);
            let info = code_fence_info(
                code.lang.as_deref(),
                code.title.as_deref(),
                code.label.as_deref(),
            );
            with_block_attrs(
                &code.attrs,
                &format!(
                    "{fence}{info}\n{}\n{fence}",
                    protect_verbatim(&code.content)
                ),
            )
        }
        BlockNode::BlockQuote(quote) => {
            let inner = render_blocks(&quote.children, ctx);
            let body = inner
                .split('\n')
                .map(|line| {
                    if line.is_empty() {
                        ">".to_string()
                    } else {
                        format!("> {line}")
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            with_block_attrs(&quote.attrs, &body)
        }
        BlockNode::List(list) => with_block_attrs(&list.attrs, &render_list(list, ctx)),
        BlockNode::ThematicBreak(rule) => with_block_attrs(&rule.attrs, "---"),
        BlockNode::Table(table) => with_block_attrs(&table.attrs, &render_table(table, ctx)),
        BlockNode::Admonition(admonition) => {
            let title = admonition
                .title
                .as_ref()
                .map(|title| format!(" \"{}\"", escape_quoted(&render_inlines(title, ctx))))
                .unwrap_or_default();
            let label = admonition
                .label
                .as_ref()
                .map(|label| format!(" [{}]", escape_bracket_text(label)))
                .unwrap_or_default();
            let body = render_blocks(&admonition.children, ctx);
            let fence = colon_fence_for(&admonition.children);
            with_block_attrs(
                &admonition.attrs,
                &format!("{fence} {}{title}{label}\n{body}\n{fence}", admonition.kind),
            )
        }
        BlockNode::Div(div) => {
            let label = div
                .label
                .as_ref()
                .map(|label| format!(" [{}]", escape_bracket_text(label)))
                .unwrap_or_default();
            let body = render_blocks(&div.children, ctx);
            let fence = colon_fence_for(&div.children);
            with_block_attrs(&div.attrs, &format!("{fence}{label}\n{body}\n{fence}"))
        }
        BlockNode::DefinitionList(list) => {
            with_block_attrs(&list.attrs, &render_definition_list(&list.items, ctx))
        }
        BlockNode::Figure(figure) => with_block_attrs(&figure.attrs, &render_figure(figure, ctx)),
        BlockNode::BlockImage(image) => render_image(image),
        BlockNode::RawBlock(raw) => {
            let fence = safe_fence(&raw.content, 3);
            format!(
                "{fence}={}\n{}\n{fence}",
                escape_format(&raw.format),
                protect_verbatim(&raw.content)
            )
        }
        BlockNode::AbbreviationDef(abbr) => {
            format!(
                "*[{}]: {}",
                escape_abbr(&abbr.abbr),
                escape_plain_line(&abbr.expansion)
            )
        }
        BlockNode::Comment(comment) => {
            if comment.block {
                render_block_comment(&comment.content)
            } else {
                format!("%% {}", comment.content)
            }
        }
        BlockNode::Extension(extension) => {
            with_block_attrs(&extension.attrs, &render_blocks(&extension.children, ctx))
        }
    }
}

fn with_block_attrs(attrs: &Option<Attrs>, body: &str) -> String {
    let rendered = render_attrs(attrs);
    if rendered.is_empty() {
        body.to_string()
    } else {
        format!("{rendered}\n{body}")
    }
}

fn render_list(node: &List, ctx: &mut CarveContext) -> String {
    ctx.list_depth += 1;
    let mut out = String::new();
    let mut counter = node.start.unwrap_or(1);
    // The marker is semantic (§11: a different bullet char / ordered delim
    // starts a new list), so emit it as authored - normalizing would merge
    // adjacent sibling lists on re-parse (carve issue 286).
    let delim = node.delim.unwrap_or('.');
    let bullet = node.bullet_char.unwrap_or('-');
    for (idx, item) in node.items.iter().enumerate() {
        let indent = "  ".repeat(ctx.list_depth - 1);
        let mut prefix = if node.ordered {
            let marker = ordered_marker(counter, node.ol_type);
            counter += 1;
            format!("{marker}{delim} ")
        } else if let Some(checked) = item.checked {
            format!("{bullet} [{}] ", if checked { "x" } else { " " })
        } else {
            format!("{bullet} ")
        };
        let item_attrs = render_attrs(&item.attrs);
        if !item_attrs.is_empty() {
            prefix = if node.ordered {
                format!("{}{item_attrs} ", prefix.trim_end())
            } else if let Some(checked) = item.checked {
                format!(
                    "{bullet}{item_attrs} [{}] ",
                    if checked { "x" } else { " " }
                )
            } else {
                format!("{bullet}{item_attrs} ")
            };
        }
        let mut content = render_blocks(&item.children, ctx);
        let trimmed_content = trim_non_nbsp(&content);
        if trimmed_content.is_empty()
            || (trimmed_content.starts_with("[^") && trimmed_content.contains(": "))
        {
            content = "+".to_string();
        }
        let mut content = trim_non_nbsp(&content).to_string();
        if item.children.len() == 1 && matches!(item.children.first(), Some(BlockNode::List(_))) {
            content = content
                .lines()
                .map(|line| line.strip_prefix("  ").unwrap_or(line))
                .collect::<Vec<_>>()
                .join("\n");
        }
        let mut lines = if content.is_empty() {
            vec!["".to_string()]
        } else {
            content.split('\n').map(str::to_string).collect()
        };
        let first = lines.remove(0);
        out.push_str(&format!("{indent}{prefix}{first}\n"));
        let continuation = " ".repeat(prefix.len());
        for line in lines {
            if is_rendered_list_marker(&line) {
                out.push_str(&format!("{indent}  {line}\n"));
            } else {
                out.push_str(&format!("{indent}{continuation}{line}\n"));
            }
        }
        let ends_with_nested_list = content.lines().last().is_some_and(|line| {
            line.starts_with(' ') && is_rendered_list_marker(line.trim_start())
        });
        if !node.tight && idx < node.items.len() - 1 && !ends_with_nested_list {
            out.push('\n');
        }
    }
    ctx.list_depth -= 1;
    trim_end_non_nbsp(&out).to_string()
}

fn ordered_marker(n: usize, ty: Option<OrderedListType>) -> String {
    match ty {
        Some(OrderedListType::LowerAlpha) => alpha_marker(n, false),
        Some(OrderedListType::UpperAlpha) => alpha_marker(n, true),
        Some(OrderedListType::LowerRoman) => roman_marker(n).to_ascii_lowercase(),
        Some(OrderedListType::UpperRoman) => roman_marker(n),
        None => n.to_string(),
    }
}

fn is_rendered_list_marker(line: &str) -> bool {
    line.starts_with("- ")
        || line.starts_with("* ")
        || line.starts_with("- [")
        || line.starts_with("* [")
        || [". ", ") "].iter().any(|sep| {
            line.split_once(sep).is_some_and(|(marker, _)| {
                !marker.is_empty() && marker.chars().all(|ch| ch.is_ascii_alphanumeric())
            })
        })
}

fn alpha_marker(n: usize, upper: bool) -> String {
    let base = ((n.saturating_sub(1) % 26) as u8) + if upper { b'A' } else { b'a' };
    (base as char).to_string()
}

fn roman_marker(mut n: usize) -> String {
    let values = [
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];
    let mut out = String::new();
    for (value, token) in values {
        while n >= value {
            out.push_str(token);
            n -= value;
        }
    }
    if out.is_empty() {
        "I".to_string()
    } else {
        out
    }
}

fn render_definition_list(items: &[DefinitionItem], ctx: &mut CarveContext) -> String {
    let mut out = Vec::new();
    for item in items {
        for term in &item.terms {
            out.push(format!(":: {}", render_inlines(term, ctx)));
        }
        for def in &item.definitions {
            let body = trim_non_nbsp(&render_blocks(def, ctx)).to_string();
            let mut lines = body.split('\n');
            out.push(format!(":  {}", lines.next().unwrap_or_default()));
            for line in lines {
                out.push(format!("   {line}"));
            }
        }
    }
    out.join("\n")
}

fn colon_fence_for(children: &[BlockNode]) -> &'static str {
    if children
        .iter()
        .any(|child| matches!(child, BlockNode::Admonition(_) | BlockNode::Div(_)))
    {
        "::::"
    } else {
        ":::"
    }
}

fn render_table(node: &Table, ctx: &mut CarveContext) -> String {
    let mut rows = Vec::new();
    let columns = node
        .rows
        .iter()
        .map(|row| row.cells.len())
        .max()
        .unwrap_or(0);
    let gfm_header = node
        .rows
        .first()
        .is_some_and(|row| !row.cells.is_empty() && row.cells.iter().all(|cell| cell.header));
    let header_aligns: Vec<Option<TableAlign>> = node
        .rows
        .first()
        .map(|row| row.cells.iter().map(|cell| cell.align).collect())
        .unwrap_or_default();
    for (row_index, row) in node.rows.iter().enumerate() {
        let mut cells = Vec::new();
        for i in 0..columns {
            if let Some(cell) = row.cells.get(i) {
                let suppress_header = gfm_header && row_index == 0;
                let suppress_align = gfm_header
                    && row_index > 0
                    && Some(cell.align) == header_aligns.get(i).copied();
                cells.push(render_table_cell(
                    cell,
                    ctx,
                    suppress_header,
                    suppress_align,
                ));
            } else {
                cells.push(RenderedCell {
                    text: String::new(),
                    tight: false,
                });
            }
        }
        rows.push(render_table_row(&cells, &render_attrs(&row.attrs)));
    }
    if gfm_header {
        let sep = (0..columns)
            .map(|i| table_separator(node.rows.first().and_then(|row| row.cells.get(i))))
            .collect::<Vec<_>>()
            .join("|");
        rows.insert(1, format!("|{sep}|"));
    }
    if let Some(caption) = &node.caption {
        rows.push(format!("^ {}", render_inlines(caption, ctx)));
    }
    rows.join("\n")
}

struct RenderedCell {
    text: String,
    tight: bool,
}

fn render_table_row(cells: &[RenderedCell], attrs: &str) -> String {
    format!(
        "|{}|{}",
        cells
            .iter()
            .map(|cell| {
                if cell.tight {
                    cell.text.clone()
                } else {
                    format!(" {} ", cell.text)
                }
            })
            .collect::<Vec<_>>()
            .join("|"),
        attrs
    )
}

fn render_table_cell(
    cell: &TableCell,
    ctx: &mut CarveContext,
    suppress_header: bool,
    suppress_align: bool,
) -> RenderedCell {
    let attrs = render_attrs(&cell.attrs);
    if cell.span == Some(TableCellSpan::Rowspan) {
        return RenderedCell {
            text: format!("{attrs}^"),
            tight: true,
        };
    }
    if cell.span == Some(TableCellSpan::Colspan) {
        return RenderedCell {
            text: format!("{attrs}<"),
            tight: true,
        };
    }
    let prefix = format!(
        "{}{}{}",
        attrs,
        if cell.header && !suppress_header {
            "="
        } else {
            ""
        },
        if suppress_align {
            ""
        } else {
            align_marker(cell.align)
        }
    );
    RenderedCell {
        text: format!("{prefix}{}", render_inlines(&cell.children, ctx)),
        tight: !prefix.is_empty(),
    }
}

fn table_separator(cell: Option<&TableCell>) -> &'static str {
    match cell.and_then(|cell| cell.align) {
        Some(TableAlign::Left) => ":---",
        Some(TableAlign::Right) => "---:",
        Some(TableAlign::Center) => ":---:",
        None => "---",
    }
}

fn render_figure(node: &Figure, ctx: &mut CarveContext) -> String {
    let target = match &node.target {
        FigureTarget::Image(image) => render_image(image),
        FigureTarget::Table(table) => render_table(table, ctx),
        FigureTarget::BlockQuote(quote) => render_block(&BlockNode::BlockQuote(quote.clone()), ctx),
        FigureTarget::CodeBlock(code) => render_block(&BlockNode::CodeBlock(code.clone()), ctx),
        FigureTarget::Paragraph(paragraph) => {
            render_block(&BlockNode::Paragraph(paragraph.clone()), ctx)
        }
    };
    format!("{target}\n^ {}", render_inlines(&node.caption, ctx))
}

fn render_footnote_defs(doc: &Document, ctx: &mut CarveContext) -> String {
    let mut out = Vec::new();
    for (label, blocks) in &doc.footnote_defs {
        out.push(render_footnote_def_source(label, blocks, ctx));
    }
    out.join("\n\n")
}

fn render_footnote_def_source(label: &str, blocks: &[BlockNode], ctx: &mut CarveContext) -> String {
    let raw_body = render_blocks(blocks, ctx);
    let single_body;
    let body = trim_non_nbsp(if blocks.len() == 1 {
        single_body = raw_body.replace("\n\n", "\n");
        &single_body
    } else {
        &raw_body
    })
    .to_string();
    let mut lines = body.split('\n');
    let mut def_lines = vec![format!(
        "[^{}]: {}",
        escape_footnote_label(label),
        lines.next().unwrap_or_default()
    )];
    for line in lines {
        def_lines.push(format!("   {line}"));
    }
    def_lines.join("\n")
}

fn render_inlines(nodes: &[InlineNode], ctx: &mut CarveContext) -> String {
    if ctx.inline_depth >= MAX_RENDER_DEPTH {
        return String::new();
    }
    ctx.inline_depth += 1;
    let mut out = String::new();
    for (idx, node) in nodes.iter().enumerate() {
        let prev = idx
            .checked_sub(1)
            .and_then(|i| last_boundary(&nodes[i]))
            .unwrap_or_default();
        let next = nodes
            .get(idx + 1)
            .and_then(first_boundary)
            .unwrap_or_default();
        let rendered = render_inline(node, ctx, prev, next);
        if !rendered.is_empty() {
            ctx.smart_state.mark_started();
        }
        out.push_str(&rendered);
    }
    ctx.inline_depth -= 1;
    out
}

fn render_inline(
    node: &InlineNode,
    ctx: &mut CarveContext,
    prev_char: char,
    next_char: char,
) -> String {
    match node {
        InlineNode::Text(text) => {
            let smart = crate::render_text::clean_smart_text_stateful(text, &mut ctx.smart_state)
                .replace(crate::NBSP_PLACEHOLDER, "\u{00a0}");
            escape_text(&smart)
        }
        InlineNode::Emphasis(emphasis) => {
            let content = render_inlines(&emphasis.children, ctx);
            let (delim, body) = match emphasis.kind {
                EmphasisKind::Italic => ("/", render_emphasis("/", &content, prev_char, next_char)),
                EmphasisKind::Strong => ("*", render_emphasis("*", &content, prev_char, next_char)),
                EmphasisKind::Underline => {
                    ("_", render_emphasis("_", &content, prev_char, next_char))
                }
                EmphasisKind::Strike => ("~", render_emphasis("~", &content, prev_char, next_char)),
                EmphasisKind::Super => ("^", render_forced_emphasis("^", &content)),
                EmphasisKind::Sub => (",", render_forced_emphasis(",", &content)),
                EmphasisKind::Highlight => {
                    ("=", render_emphasis("=", &content, prev_char, next_char))
                }
                EmphasisKind::BoldItalic => ("", format!("/*{content}*/")),
            };
            let _ = delim;
            format!("{body}{}", render_attrs(&emphasis.attrs))
        }
        InlineNode::Code(code, attrs) => format!("{}{}", render_code(code), render_attrs(attrs)),
        InlineNode::Link(link) => render_link(link, ctx),
        InlineNode::Image(image) => render_image(image),
        InlineNode::Span(span) => {
            let attrs = render_attrs(&span.attrs);
            format!(
                "[{}]{}",
                render_inlines(&span.children, ctx),
                if attrs.is_empty() { "{}" } else { &attrs }
            )
        }
        InlineNode::Math(math) => format!(
            "{}{}{}",
            if math.display { "$$" } else { "$" },
            render_code(&math.content),
            render_attrs(&math.attrs)
        ),
        InlineNode::RawInline(raw) => {
            format!(
                "{}{{={}}}",
                render_code(&raw.content),
                escape_format(&raw.format)
            )
        }
        InlineNode::Symbol(symbol) => format!(
            ":{}:{}",
            escape_symbol_name(&symbol.name),
            render_attrs(&symbol.attrs)
        ),
        InlineNode::AutoLink(link) => {
            // Emit the raw autolink content verbatim (keeps a URI scheme like
            // `mailto:`), so it re-parses to the same autolink.
            format!(
                "<{}>{}",
                escape_autolink_href(&link.text),
                render_attrs(&link.attrs)
            )
        }
        InlineNode::Mention(mention) => format!("@{}", escape_name(&mention.user)),
        InlineNode::Tag(tag) => format!("#{}", escape_name(&tag.name)),
        InlineNode::Extension(extension) => format!(
            ":{}[{}]{}",
            escape_identifier(&extension.name),
            render_inlines(&extension.children, ctx),
            render_attrs(&extension.attrs)
        ),
        InlineNode::Abbreviation(abbr) => escape_text(&abbr.abbr),
        InlineNode::Footnote(footnote) => {
            let body = if let Some(inline) = &footnote.inline {
                format!("^[{}]", render_inlines(inline, ctx))
            } else {
                format!(
                    "[^{}]",
                    escape_footnote_label(footnote.id.as_deref().unwrap_or_default())
                )
            };
            format!("{body}{}", render_attrs(&footnote.attrs))
        }
        InlineNode::SoftBreak => "\n".to_string(),
        InlineNode::HardBreak => "\\\n".to_string(),
        InlineNode::CriticInsert(insert) => {
            format!(
                "{{+{}+}}{}",
                render_inlines(&insert.children, ctx),
                render_attrs(&insert.attrs)
            )
        }
        InlineNode::CriticDelete(delete) => {
            format!(
                "{{-{}-}}{}",
                render_inlines(&delete.children, ctx),
                render_attrs(&delete.attrs)
            )
        }
        InlineNode::CriticSubstitute(sub) => {
            format!(
                "{{~{}~>{}~}}",
                escape_critic_text(&sub.old_text),
                escape_critic_text(&sub.new_text)
            )
        }
        InlineNode::CriticComment(comment) => {
            format!("{{#{}#}}", escape_critic_text(&comment.text))
        }
        InlineNode::CrossRef(crossref) => {
            format!("</#{}>", escape_crossref_target(&crossref.target))
        }
        InlineNode::CaptionNumber(_) => "#".to_string(),
        InlineNode::CitationGroup(group) => group.raw.clone(),
    }
}

fn render_link(node: &Link, ctx: &mut CarveContext) -> String {
    if node.ref_label.is_some() && node.raw_ref.is_some() {
        return node.raw_ref.clone().unwrap_or_default();
    }
    if node.from_crossref {
        if let Some(target) = node.href.strip_prefix('#') {
            return format!("</#{}>", escape_crossref_target(target));
        }
    }
    let text = render_inlines(&node.children, ctx);
    let title = node
        .title
        .as_ref()
        .map(|title| format!(" \"{}\"", escape_quoted(title)))
        .unwrap_or_default();
    format!(
        "[{text}]({}{title}){}",
        escape_destination(&node.href),
        render_attrs(&node.attrs)
    )
}

fn render_image(node: &Image) -> String {
    // An unresolved reference image round-trips via its verbatim source, exactly
    // like an unresolved reference link (render_link); `![alt]()` would change
    // the rendered text and break the to_html(fmt(x)) == to_html(x) invariant.
    if node.ref_label.is_some() && node.raw_ref.is_some() {
        return node.raw_ref.clone().unwrap_or_default();
    }
    let title = node
        .title
        .as_ref()
        .map(|title| format!(" \"{}\"", escape_quoted(title)))
        .unwrap_or_default();
    format!(
        "![{}]({}{title}){}",
        escape_image_alt(&node.alt),
        escape_destination(&node.src),
        render_attrs(&node.attrs)
    )
}

fn render_frontmatter(frontmatter: &std::collections::BTreeMap<String, String>) -> String {
    let mut out = String::from("---");
    for (key, value) in frontmatter {
        out.push('\n');
        out.push_str(key);
        out.push_str(": ");
        out.push_str(&protect_verbatim(value));
    }
    out.push_str("\n---");
    out
}

fn render_block_comment(content: &str) -> String {
    let mut longest = 0usize;
    let mut current = 0usize;
    for ch in content.chars() {
        if ch == '%' {
            current += 1;
            longest = longest.max(current);
        } else {
            current = 0;
        }
    }
    let fence = "%".repeat(3.max(longest + 1));
    format!("{fence}\n{}\n{fence}", protect_verbatim(content))
}

// Superscript and subscript have no bare delimiter form -- always emit the
// braced `{^x^}` / `{,x,}` form.
fn render_forced_emphasis(delim: &str, content: &str) -> String {
    format!("{{{delim}{content}{delim}}}")
}

fn render_emphasis(delim: &str, content: &str, prev_char: char, next_char: char) -> String {
    let needs_forced = is_word_boundary(prev_char)
        || is_word_boundary(next_char)
        || content.starts_with(delim)
        || content.ends_with(delim)
        || content.starts_with(' ')
        || content.ends_with(' ')
        || content.is_empty();
    if needs_forced {
        format!("{{{delim}{content}{delim}}}")
    } else {
        format!("{delim}{content}{delim}")
    }
}

fn is_word_boundary(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

fn render_code(content: &str) -> String {
    let fence = safe_fence(content, 1);
    if content.starts_with('`') || content.ends_with('`') {
        format!("{fence} {content} {fence}")
    } else {
        format!("{fence}{content}{fence}")
    }
}

fn code_fence_info(lang: Option<&str>, title: Option<&str>, label: Option<&str>) -> String {
    let mut parts = Vec::new();
    if let Some(lang) = lang.filter(|s| !s.is_empty()) {
        parts.push(escape_fence_token(lang));
    }
    if let Some(title) = title {
        parts.push(format!("\"{}\"", escape_quoted(title)));
    }
    if let Some(label) = label {
        parts.push(format!("[{}]", escape_bracket_text(label)));
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!(" {}", parts.join(" "))
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

fn render_attrs(attrs: &Option<Attrs>) -> String {
    let Some(attrs) = attrs else {
        return String::new();
    };
    let mut parts = Vec::new();
    let id_as_key = attrs.id.as_ref().is_some_and(|id| !is_attr_identifier(id));
    let mut seen_keys: Vec<&str> = Vec::new();
    let emit_id = |parts: &mut Vec<String>| {
        if let Some(id) = &attrs.id {
            if id_as_key {
                parts.push(format!("id={}", quote_attr_value(id)));
            } else {
                parts.push(format!("#{}", escape_attr_name_value(id)));
            }
        }
    };
    let emit_classes = |parts: &mut Vec<String>| {
        for cls in &attrs.classes {
            parts.push(format!(".{}", escape_attr_name_value(cls)));
        }
    };
    let emit_key = |parts: &mut Vec<String>, key: &str| {
        if let Some(value) = attrs.key_values.get(key) {
            parts.push(format!(
                "{}={}",
                escape_attr_key(key),
                quote_attr_value(value)
            ));
        }
    };
    if attrs.order.is_empty() {
        emit_id(&mut parts);
        emit_classes(&mut parts);
        for key in attrs.key_values.keys() {
            emit_key(&mut parts, key);
        }
    } else {
        for slot in &attrs.order {
            match slot {
                AttrSlot::Id => emit_id(&mut parts),
                AttrSlot::Class => emit_classes(&mut parts),
                AttrSlot::Key(key) => {
                    if !seen_keys.contains(&key.as_str()) {
                        emit_key(&mut parts, key);
                        seen_keys.push(key);
                    }
                }
            }
        }
        for key in attrs.key_values.keys() {
            if !seen_keys.contains(&key.as_str()) {
                emit_key(&mut parts, key);
            }
        }
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!("{{{}}}", parts.join(" "))
    }
}

fn quote_attr_value(value: &str) -> String {
    if !value.is_empty()
        && value
            .chars()
            .all(|ch| !ch.is_whitespace() && !matches!(ch, '"' | '\'' | '{' | '}'))
    {
        value.to_string()
    } else {
        format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
    }
}

fn align_marker(align: Option<TableAlign>) -> &'static str {
    match align {
        Some(TableAlign::Left) => "<",
        Some(TableAlign::Right) => ">",
        Some(TableAlign::Center) => "~",
        None => "",
    }
}

fn normalize(text: &str) -> String {
    let text = text.replace('\u{e000}', "\u{00a0}");
    let lines = trim_non_nbsp(&text)
        .split('\n')
        .map(|line| trim_end_non_nbsp(line).to_string())
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "{}\n",
        restore_verbatim(trim_non_nbsp(&collapse_blank_lines(&lines)))
    )
}

/// Whole-document normalization (trailing-whitespace strip, blank-line
/// collapsing) must not reach inside verbatim content - code blocks, raw
/// blocks, frontmatter, and block comments reproduce their content byte-exact
/// (carve-js issue 340). Sentinel-encode the vulnerable bytes before the
/// content joins the document string; `normalize` restores them at the end.
/// U+E000 is already the NBSP sentinel; U+E001..U+E003 extend the scheme.
fn protect_verbatim(content: &str) -> String {
    let mut lines = Vec::new();
    for line in content.split('\n') {
        if line.is_empty() {
            lines.push("\u{e003}".to_string());
            continue;
        }
        let stripped = line.trim_end_matches([' ', '\t']);
        let tail: String = line[stripped.len()..]
            .chars()
            .map(|ch| if ch == ' ' { '\u{e001}' } else { '\u{e002}' })
            .collect();
        lines.push(format!("{stripped}{tail}"));
    }
    lines.join("\n")
}

fn restore_verbatim(text: &str) -> String {
    text.replace('\u{e001}', " ")
        .replace('\u{e002}', "\t")
        .replace('\u{e003}', "")
}

fn collapse_blank_lines(text: &str) -> String {
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
    out
}

fn trim_non_nbsp(text: &str) -> &str {
    text.trim_matches(|ch: char| ch.is_whitespace() && ch != '\u{00a0}')
}

fn trim_end_non_nbsp(text: &str) -> &str {
    text.trim_end_matches(|ch: char| ch.is_whitespace() && ch != '\u{00a0}')
}

fn escape_text(text: &str) -> String {
    let mut out = String::new();
    for ch in text.chars() {
        if matches!(ch, '\u{0000}'..='\u{0008}' | '\u{000b}'..='\u{001f}' | '\u{007f}'..='\u{009f}')
        {
            continue;
        }
        if matches!(
            ch,
            '\\' | '`'
                | '*'
                | '_'
                | '{'
                | '}'
                | '['
                | ']'
                | '('
                | ')'
                | '#'
                | '+'
                | '-'
                | '.'
                | '!'
                | '~'
                | '^'
                | '/'
                | '<'
                | '>'
                | '@'
                | '%'
                | '|'
                | '='
                // no ',' here: there is no bare subscript delimiter, and the
                // braced `{,` opener is neutralized by the `{` escape
                | ':'
                | ';'
                | '"'
                | '\''
        ) {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

fn escape_plain_line(text: &str) -> String {
    text.replace('\n', " ")
}

fn escape_image_alt(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('[', "\\[")
        .replace(']', "\\]")
}

fn escape_destination(text: &str) -> String {
    let sanitize_blank = dangerous_destination_scheme(text);
    let mut out = String::new();
    for ch in text.chars() {
        match ch {
            // A backslash is a literal destination character (no destination
            // escapes), emitted verbatim -- escaping it would double on
            // re-parse. Whitespace is percent-encoded (it would end the
            // destination otherwise).
            ch if ch.is_whitespace() => {
                if ch == ' ' {
                    out.push_str("%20");
                } else {
                    out.push_str(&format!("%{:02X}", ch as u32));
                }
            }
            '(' if sanitize_blank => out.push_str("%28"),
            ')' if sanitize_blank => out.push_str("%29"),
            _ => out.push(ch),
        }
    }
    out
}

fn dangerous_destination_scheme(text: &str) -> bool {
    let trimmed = text.trim_start_matches(|ch: char| {
        ch <= '\u{0020}'
            || matches!(
                ch,
                '\u{00a0}' | '\u{1680}' | '\u{2000}'
                    ..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}'
            )
    });
    let Some(colon) = trimmed.find(':') else {
        return false;
    };
    let scheme = &trimmed[..colon];
    !scheme.is_empty()
        && scheme
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic())
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '-'))
        && matches!(
            scheme.to_ascii_lowercase().as_str(),
            "javascript" | "vbscript" | "data" | "file"
        )
}

fn escape_quoted(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

fn escape_bracket_text(text: &str) -> String {
    text.replace('\\', "\\\\").replace(']', "\\]")
}

fn escape_footnote_label(text: &str) -> String {
    escape_bracket_text(text)
}

fn escape_abbr(text: &str) -> String {
    escape_bracket_text(text)
}

fn escape_identifier(text: &str) -> String {
    text.chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '-')
        .collect()
}

// A symbol name may contain `+` and `-` (so `:+1:` / `:-1:` round-trip),
// unlike an extension identifier.
fn escape_symbol_name(text: &str) -> String {
    text.chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '+' || *ch == '-')
        .collect()
}

fn escape_name(text: &str) -> String {
    let trimmed = text.trim_matches('.');
    trimmed
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '.' || *ch == '-')
        .collect()
}

fn escape_format(text: &str) -> String {
    let safe: String = text
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '-')
        .collect();
    if safe.is_empty() {
        "text".to_string()
    } else {
        safe
    }
}

fn escape_fence_token(text: &str) -> String {
    text.split_whitespace()
        .next()
        .unwrap_or_default()
        .replace('`', "")
}

fn escape_attr_key(text: &str) -> String {
    let mut out = String::new();
    let mut started = false;
    for ch in text.chars() {
        if !started {
            if ch.is_ascii_alphabetic() || ch == '_' {
                out.push(ch);
                started = true;
            }
        } else if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
            out.push(ch);
        }
    }
    if out.is_empty() {
        "x".to_string()
    } else {
        out
    }
}

fn escape_attr_name_value(text: &str) -> String {
    text.chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
                ch
            } else {
                '-'
            }
        })
        .collect()
}

fn is_attr_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    chars
        .next()
        .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
}

fn escape_autolink_href(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('<', "\\<")
        .replace('>', "\\>")
}

fn escape_crossref_target(text: &str) -> String {
    text.replace('\\', "\\\\").replace('>', "\\>")
}

fn escape_critic_text(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('{', "\\{")
        .replace('}', "\\}")
}

fn first_boundary(node: &InlineNode) -> Option<char> {
    boundary_text(node).and_then(|s| {
        let mut chars = s.chars();
        match chars.next() {
            // In carve parse mode, text nodes preserve backslash escapes, so a
            // formatted `\_b\_` reaches us with a leading `\`. The escape marker
            // is not the adjacency-relevant character -- the escaped punctuation
            // char is. Skip a single leading backslash that escapes an ASCII
            // punctuation char so the emphasis bracing decision stays a function
            // of the semantic next character (e.g. `_`), matching `last_boundary`
            // (which already returns the escaped char) and keeping the formatter
            // idempotent and byte-identical to carve-js / carve-php.
            Some('\\') => match chars.next() {
                Some(next) if next.is_ascii_punctuation() => Some(next),
                _ => Some('\\'),
            },
            other => other,
        }
    })
}

fn last_boundary(node: &InlineNode) -> Option<char> {
    boundary_text(node).and_then(|s| s.chars().next_back())
}

fn boundary_text(node: &InlineNode) -> Option<&str> {
    match node {
        InlineNode::Text(text) => Some(text),
        InlineNode::Code(text, _) => Some(text),
        InlineNode::Abbreviation(abbr) => Some(&abbr.abbr),
        InlineNode::Mention(mention) => Some(&mention.user),
        InlineNode::Tag(tag) => Some(&tag.name),
        InlineNode::Symbol(symbol) => Some(&symbol.name),
        _ => None,
    }
}
