use crate::ast::*;
use crate::extension::Options;
use crate::render_text::strip_controls;

use crate::render::MAX_RENDER_DEPTH;

thread_local! {
    /// Mode for the current render, carried the way `render_markdown` carries
    /// its own: set once per render entry point, read only by the
    /// smart-punctuation arm, off every signature in between.
    ///
    /// This target keeps its OWN cell rather than sharing the Markdown one. No
    /// entry point restores the previous value, so a shared cell would let a
    /// nested render of another target leave its mode behind in this one.
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

fn trim_block_output(s: &str) -> &str {
    s.trim_matches(|c| c == '\n' || c == ' ')
}

/// Render a document to ANSI-styled text, honouring
/// `Options::smart_typography`. See `render_markdown_with_options` for why the
/// options-taking wrapper exists; the profile transform runs upstream.
/// Render a tree that did NOT come from the parser, refusing at the ceiling.
///
/// `Err` when the tree nests deeper than [`crate::MAX_RENDER_DEPTH`], naming the
/// renderer and the bound (PART 9 §25). A parser-produced tree cannot reach it -
/// the parse cap sits below the ceiling - so this fails only for a tree built
/// through the API or read by `from_json`, which is the caller who can act on it.
pub fn render_ansi_with_options(
    doc: &Document,
    options: &Options<'_>,
) -> Result<String, crate::RenderDepthError> {
    let watch = crate::render_depth::RenderDepthWatch::new();
    watch.into_result(render_ansi_inner(
        doc,
        options.smart_typography,
        options.lowercase_heading_ids,
    ))
}

const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const ITALIC: &str = "\x1b[3m";
const UNDERLINE: &str = "\x1b[4m";
const STRIKE: &str = "\x1b[9m";
const FG_BLUE: &str = "\x1b[34m";
const FG_MAGENTA: &str = "\x1b[35m";
const FG_CYAN: &str = "\x1b[36m";
const FG_YELLOW: &str = "\x1b[33m";
const FG_GREEN: &str = "\x1b[32m";
const FG_BRIGHT_BLACK: &str = "\x1b[90m";
const FG_BRIGHT_YELLOW: &str = "\x1b[93m";
const FG_BRIGHT_MAGENTA: &str = "\x1b[95m";
const FG_BRIGHT_CYAN: &str = "\x1b[96m";
const FG_BRIGHT_BLUE: &str = "\x1b[94m";
const FG_BRIGHT_GREEN: &str = "\x1b[92m";
const FG_BRIGHT_WHITE: &str = "\x1b[97m";

/// Render a tree that did NOT come from the parser, refusing at the ceiling.
///
/// `Err` when the tree nests deeper than [`crate::MAX_RENDER_DEPTH`], naming the
/// renderer and the bound (PART 9 §25). A parser-produced tree cannot reach it -
/// the parse cap sits below the ceiling - so this fails only for a tree built
/// through the API or read by `from_json`, which is the caller who can act on it.
pub fn render_ansi(doc: &Document) -> Result<String, crate::RenderDepthError> {
    let watch = crate::render_depth::RenderDepthWatch::new();
    watch.into_result(render_ansi_inner(
        doc,
        crate::extension::SmartTypographyMode::Glyph,
        Options::default().lowercase_heading_ids,
    ))
}

fn render_ansi_inner(
    doc: &Document,
    smart_typography: crate::extension::SmartTypographyMode,
    lowercase_heading_ids: bool,
) -> String {
    SMART_TYPOGRAPHY.with(|cell| cell.set(smart_typography));
    let _abbr_guard = crate::abbr_budget::AbbrBudgetGuard::new(doc.source_len);
    let mut ctx = AnsiContext {
        list_depth: 0,
        block_quote_depth: 0,
        ordered: Vec::new(),
        defined_footnotes: doc.footnote_defs.keys().cloned().collect(),
        crossref_index: crate::parse::crossref_index_for_document(doc, lowercase_heading_ids),
        link_depth: 0,
    };
    let out = render_blocks(&doc.children, &mut ctx, 0);
    let footnotes = render_footnote_defs(doc, &mut ctx);
    normalize(&format!("{out}{footnotes}"))
}

struct AnsiContext {
    list_depth: usize,
    block_quote_depth: usize,
    ordered: Vec<usize>,
    /// Labels that actually have a definition. A reference without one did not
    /// form a footnote, so it is not a footnote marker. The HTML renderer decides
    /// this on the node's `number`, which numbering assigns -- this target does no
    /// numbering, so that field is always None here and there was nothing to
    /// check (carve#352).
    defined_footnotes: std::collections::BTreeSet<String>,
    crossref_index: crate::parse::CrossrefIndex,
    /// Nonzero while rendering a link's label. Links never nest, and the parser
    /// enforces that -- but a cross-reference becomes a link only HERE, after
    /// the parser has run, so this target has to apply the rule itself. A
    /// nested link sequence ends with its own reset, which closes the OUTER
    /// link's styling early and leaves the rest of the label unstyled
    /// (carve-rs#436).
    link_depth: usize,
}

fn render_block_inlines(nodes: &[InlineNode], ctx: &mut AnsiContext) -> String {
    render_inlines(nodes, ctx, 0)
}

fn render_title_inlines(nodes: &[InlineNode], ctx: &mut AnsiContext) -> String {
    let nodes = inline_nodes_without_strong(nodes);
    render_block_inlines(&nodes, ctx)
}

fn style(text: &str, codes: &str) -> String {
    format!("{codes}{text}{RESET}")
}

fn render_blocks(blocks: &[BlockNode], ctx: &mut AnsiContext, depth: usize) -> String {
    if depth > MAX_RENDER_DEPTH {
        crate::render_depth::record("ansi");
        return String::new();
    }
    blocks
        .iter()
        .map(|block| render_block(block, ctx, depth))
        .collect()
}

fn render_block(node: &BlockNode, ctx: &mut AnsiContext, depth: usize) -> String {
    if depth > MAX_RENDER_DEPTH {
        crate::render_depth::record("ansi");
        return String::new();
    }
    match node {
        // Renders nothing: a definition line is not prose.
        BlockNode::LinkReferenceDefinition(_) => String::new(),
        BlockNode::Heading(heading) => {
            render_heading(heading.level, &render_block_inlines(&heading.children, ctx))
        }
        BlockNode::Paragraph(paragraph) => {
            let mut content = render_block_inlines(&paragraph.children, ctx);
            let prefix = block_quote_prefix(ctx);
            if !prefix.is_empty() {
                content = prefix_lines(&content, &prefix);
            }
            format!("{content}\n\n")
        }
        BlockNode::CodeBlock(code) => {
            let lang = code.lang.as_deref().map(strip_controls);
            render_code_block(&strip_controls(&code.content), lang.as_deref())
        }
        BlockNode::BlockQuote(quote) => {
            ctx.block_quote_depth += 1;
            let out = render_blocks(&quote.children, ctx, depth + 1);
            ctx.block_quote_depth -= 1;
            out
        }
        BlockNode::List(list) => render_list(list, ctx, depth + 1),
        BlockNode::ThematicBreak(_) => format!("{}\n\n", style(&"─".repeat(40), DIM)),
        BlockNode::Table(table) => render_table(table, ctx),
        BlockNode::Admonition(admonition) => {
            let body = render_blocks(&admonition.children, ctx, depth + 1);
            // The LABEL goes on first so the TITLE ends up above it, which is the
            // order the source writes them (`::: tip "Pro Tip" [Build]`) and the
            // order the HTML renderer emits (carve#352, corpus 42-admonitions-4).
            let body = prepend_label(body, admonition.label.as_deref(), ctx);
            match &admonition.title {
                Some(title) => {
                    let t = render_title_inlines(title, ctx);
                    if t.is_empty() {
                        body
                    } else {
                        // Carry the blockquote `│` prefix onto the title line
                        // too, matching how the body content is prefixed.
                        let prefix = block_quote_prefix(ctx);
                        let title_line = if prefix.is_empty() {
                            style(&t, BOLD)
                        } else {
                            prefix_lines(&style(&t, BOLD), &prefix)
                        };
                        format!("{title_line}\n\n{body}")
                    }
                }
                None => body,
            }
        }
        BlockNode::LineBlock(lb) => render_blocks(&lb.children, ctx, depth + 1),
        BlockNode::Div(div) => {
            let body = render_blocks(&div.children, ctx, depth + 1);
            prepend_label(body, div.label.as_deref(), ctx)
        }
        BlockNode::DefinitionList(list) => {
            render_definition_list(&list.items, ctx, true, depth + 1)
        }
        BlockNode::Figure(figure) => render_figure(figure, ctx, depth + 1),
        // Terminate the block image so the next block is not glued onto it.
        BlockNode::BlockImage(image) => format!("{}\n\n", render_image(image)),
        BlockNode::RawBlock(raw) => format!(
            "{}\n\n",
            style(
                &format!(
                    "[raw:{}] {}",
                    strip_controls(&raw.format),
                    strip_controls(&raw.content)
                ),
                DIM
            )
        ),
        BlockNode::Extension(extension) => render_blocks(&extension.children, ctx, depth + 1),
        // PART 10 §10a - see the note in render_markdown.
        BlockNode::AbbreviationDef(def) => format!(
            "{}\n\n",
            style(
                &format!(
                    "*[{}]: {}",
                    strip_controls(&def.abbr),
                    strip_controls(&def.expansion)
                ),
                DIM
            )
        ),
        BlockNode::Comment(_) => String::new(),
    }
}

fn render_heading(level: u8, content: &str) -> String {
    let color = match level {
        1 => FG_BRIGHT_MAGENTA,
        2 => FG_BRIGHT_CYAN,
        3 => FG_BRIGHT_BLUE,
        4 => FG_BRIGHT_GREEN,
        5 => FG_BRIGHT_YELLOW,
        _ => FG_BRIGHT_WHITE,
    };
    let mut out = style(content, &(BOLD.to_string() + color));
    if level <= 2 {
        let ch = if level == 1 { '═' } else { '─' };
        out.push('\n');
        out.push_str(&style(&ch.to_string().repeat(width(content)), color));
    }
    format!("{out}\n\n")
}

fn render_code_block(content: &str, lang: Option<&str>) -> String {
    let mut out = String::new();
    if let Some(lang) = lang {
        out.push_str(&format!("{}\n", style(&format!("┌── {lang} "), DIM)));
    }
    for line in content.strip_suffix('\n').unwrap_or(content).split('\n') {
        out.push_str(&format!(
            "{}\n",
            style(&format!("  {line}"), FG_BRIGHT_WHITE)
        ));
    }
    out.push('\n');
    out
}

/// Graceful degradation: when no extension consumed the grouping `[label]`,
/// surface it as a leading bold line (mirroring how an admonition title
/// renders, including the blockquote prefix) so the authored label is never
/// silently dropped in ANSI output.
fn prepend_label(body: String, label: Option<&str>, ctx: &AnsiContext) -> String {
    match label {
        Some(label) if !label.is_empty() => {
            let l = strip_controls(label);
            let prefix = block_quote_prefix(ctx);
            let label_line = if prefix.is_empty() {
                style(&l, BOLD)
            } else {
                prefix_lines(&style(&l, BOLD), &prefix)
            };
            if body.is_empty() {
                format!("{label_line}\n\n")
            } else {
                format!("{label_line}\n\n{body}")
            }
        }
        _ => body,
    }
}

fn block_quote_prefix(ctx: &AnsiContext) -> String {
    if ctx.block_quote_depth == 0 {
        String::new()
    } else {
        format!("{} ", style("│", &(FG_CYAN.to_string() + DIM))).repeat(ctx.block_quote_depth)
    }
}

fn prefix_lines(content: &str, prefix: &str) -> String {
    content
        .split('\n')
        .map(|line| format!("{prefix}{line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_list(node: &List, ctx: &mut AnsiContext, depth: usize) -> String {
    if depth > MAX_RENDER_DEPTH {
        crate::render_depth::record("ansi");
        return String::new();
    }
    ctx.list_depth += 1;
    if node.ordered {
        if ctx.ordered.len() <= ctx.list_depth {
            ctx.ordered.resize(ctx.list_depth + 1, 1);
        }
        ctx.ordered[ctx.list_depth] = node.start.unwrap_or(1);
    }
    let mut out = String::new();
    for item in &node.items {
        let indent = "  ".repeat(ctx.list_depth - 1);
        let marker = if node.ordered {
            let n = ctx.ordered[ctx.list_depth];
            ctx.ordered[ctx.list_depth] = n + 1;
            style(&format!("{n}."), FG_YELLOW)
        } else if let Some(checked) = item.checked {
            if checked {
                style("☑", FG_GREEN)
            } else {
                style("☐", FG_BRIGHT_BLACK)
            }
        } else {
            style("•", FG_CYAN)
        };
        out.push_str(&format!(
            "{indent}{marker} {}\n",
            trim_block_output(&render_blocks(&item.children, ctx, depth + 1))
        ));
    }
    ctx.list_depth -= 1;
    if ctx.list_depth == 0 {
        out.push('\n');
    }
    out
}

fn render_definition_list(
    items: &[DefinitionItem],
    ctx: &mut AnsiContext,
    trailing_blank: bool,
    depth: usize,
) -> String {
    if depth > MAX_RENDER_DEPTH {
        crate::render_depth::record("ansi");
        return String::new();
    }
    let mut out = String::new();
    for item in items {
        for term in &item.terms {
            out.push_str(&format!(
                "{}\n",
                style(
                    &render_block_inlines(term, ctx),
                    &(BOLD.to_string() + FG_YELLOW)
                )
            ));
        }
        for definition in &item.definitions {
            out.push_str(&format!(
                "  {}\n",
                trim_block_output(&render_blocks(definition, ctx, depth + 1))
            ));
        }
    }
    if trailing_blank {
        out.push('\n');
    }
    out
}

struct RenderedCell {
    content: String,
    plain: String,
    is_header: bool,
}

fn render_table(node: &Table, ctx: &mut AnsiContext) -> String {
    let rows = node
        .rows
        .iter()
        .map(|row| {
            let row_is_header = row.cells.iter().all(|cell| cell.header);
            row.cells
                .iter()
                .map(|cell| {
                    let content = render_block_inlines(&cell.children, ctx).trim().to_string();
                    let plain = strip_ansi(&content);
                    RenderedCell {
                        content,
                        plain,
                        is_header: row_is_header,
                    }
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let mut widths = Vec::<usize>::new();
    for row in &rows {
        for (i, cell) in row.iter().enumerate() {
            if widths.len() <= i {
                widths.resize(i + 1, 0);
            }
            widths[i] = widths[i].max(width(&cell.plain));
        }
    }
    let mut out = String::new();
    let mut header_rendered = false;
    if !rows.is_empty() {
        out.push_str(&table_border(&widths, "top"));
    }
    for row in &rows {
        out.push_str(&table_row(row, &widths));
        if row.first().is_some_and(|cell| cell.is_header) && !header_rendered {
            out.push_str(&table_border(&widths, "middle"));
            header_rendered = true;
        }
    }
    if !rows.is_empty() {
        out.push_str(&table_border(&widths, "bottom"));
    }
    if let Some(caption) = &node.caption {
        out.push_str(&render_caption(caption, ctx));
    }
    out.push('\n');
    out
}

fn table_border(widths: &[usize], pos: &str) -> String {
    let (left, right, cross) = match pos {
        "top" => ("┌", "┐", "┬"),
        "middle" => ("├", "┤", "┼"),
        _ => ("└", "┘", "┴"),
    };
    let body = widths
        .iter()
        .map(|w| "─".repeat(w + 2))
        .collect::<Vec<_>>()
        .join(cross);
    format!("{}\n", style(&format!("{left}{body}{right}"), DIM))
}

fn table_row(cells: &[RenderedCell], widths: &[usize]) -> String {
    let sep = style("│", DIM);
    let parts = cells
        .iter()
        .enumerate()
        .map(|(i, cell)| {
            let padding = widths
                .get(i)
                .copied()
                .unwrap_or(0)
                .saturating_sub(width(&cell.plain));
            let content = if cell.is_header {
                style(&(cell.content.clone() + &" ".repeat(padding)), BOLD)
            } else {
                cell.content.clone() + &" ".repeat(padding)
            };
            format!(" {content} ")
        })
        .collect::<Vec<_>>();
    format!("{sep}{}{sep}\n", parts.join(&sep))
}

fn render_figure(node: &Figure, ctx: &mut AnsiContext, depth: usize) -> String {
    if depth > MAX_RENDER_DEPTH {
        crate::render_depth::record("ansi");
        return String::new();
    }
    let target = match &node.target {
        FigureTarget::Image(image) => render_image(image),
        FigureTarget::Table(table) => render_table(table, ctx).trim_end().to_string(),
        FigureTarget::BlockQuote(quote) => {
            render_block(&BlockNode::BlockQuote(quote.clone()), ctx, depth + 1)
                .trim_end()
                .to_string()
        }
        FigureTarget::CodeBlock(cb) => {
            render_block(&BlockNode::CodeBlock(cb.clone()), ctx, depth + 1)
                .trim_end()
                .to_string()
        }
        FigureTarget::Paragraph(p) => {
            render_block(&BlockNode::Paragraph(p.clone()), ctx, depth + 1)
                .trim_end()
                .to_string()
        }
    };
    let sep = match &node.target {
        FigureTarget::BlockQuote(_) => "\n\n",
        _ => "\n",
    };
    format!("{target}{sep}{}", render_caption(&node.caption, ctx))
}

fn render_caption(nodes: &[InlineNode], ctx: &mut AnsiContext) -> String {
    format!(
        "{}\n\n",
        style(
            render_block_inlines(nodes, ctx).trim(),
            &(ITALIC.to_string() + DIM)
        )
    )
}

fn render_footnote_defs(doc: &Document, ctx: &mut AnsiContext) -> String {
    let mut out = String::new();
    // SOURCE ORDER, not label order (§7; carve-rs#686). The map is a BTreeMap.
    for (label, blocks) in crate::ast_json::footnote_defs_in_source_order(doc) {
        out.push_str(&format!(
            "{} {}\n",
            // The marker as written (PART 10 §10a): the caret is the construct.
            style(
                &format!("[^{}]", strip_controls(label)),
                &(FG_CYAN.to_string() + DIM)
            ),
            render_blocks(blocks, ctx, 0).trim()
        ));
    }
    out
}

fn render_inlines(nodes: &[InlineNode], ctx: &mut AnsiContext, depth: usize) -> String {
    if depth > MAX_RENDER_DEPTH {
        crate::render_depth::record("ansi");
        return String::new();
    }
    let mut out = String::new();
    for node in nodes {
        out.push_str(&render_inline(node, ctx, depth));
    }
    out
}

fn render_inline(node: &InlineNode, ctx: &mut AnsiContext, depth: usize) -> String {
    if depth > MAX_RENDER_DEPTH {
        crate::render_depth::record("ansi");
        return String::new();
    }
    match node {
        InlineNode::Text(text) => strip_controls(&text.value),
        InlineNode::EscapedText(text) => strip_controls(&text.value),
        InlineNode::SmartPunctuation(s) => strip_controls(smart_punctuation_text(s)),
        InlineNode::Emphasis(emphasis) => match emphasis.kind {
            EmphasisKind::Italic => {
                style(&render_inlines(&emphasis.children, ctx, depth + 1), ITALIC)
            }
            EmphasisKind::Strong => {
                style(&render_inlines(&emphasis.children, ctx, depth + 1), BOLD)
            }
            EmphasisKind::Underline => style(
                &render_inlines(&emphasis.children, ctx, depth + 1),
                UNDERLINE,
            ),
            EmphasisKind::Strike => {
                style(&render_inlines(&emphasis.children, ctx, depth + 1), STRIKE)
            }
            EmphasisKind::Sub => to_subscript(&render_inlines(&emphasis.children, ctx, depth + 1)),
            EmphasisKind::Super => {
                to_superscript(&render_inlines(&emphasis.children, ctx, depth + 1))
            }
            EmphasisKind::Highlight => style(
                &render_inlines(&emphasis.children, ctx, depth + 1),
                &("\x1b[7m".to_string() + FG_YELLOW),
            ),
            EmphasisKind::BoldItalic => style(
                &render_inlines(&emphasis.children, ctx, depth + 1),
                &(BOLD.to_string() + ITALIC),
            ),
        },
        InlineNode::Code(code) => style(&strip_controls(&code.value), FG_BRIGHT_YELLOW),
        InlineNode::Link(link) => {
            if link.ref_label.is_some() && link.href.is_empty() {
                return strip_controls(link.raw_ref.as_deref().unwrap_or_default());
            }
            ctx.link_depth += 1;
            let text = render_inlines(&link.children, ctx, depth + 1);
            ctx.link_depth -= 1;
            if ctx.link_depth > 0 {
                // Already inside a link: the label is styled by the outer one,
                // and a second sequence here would reset it early.
                return text;
            }
            // PART 9 §25 binds every target that emits a resolvable URL, and this
            // parenthetical IS the destination: a terminal autolinks it and hands
            // the scheme to the OS handler on click, which is the same "deferred
            // by one step" the clause describes for Markdown. It printed
            // `javascript:` and the OS protocol-handler schemes verbatim, in all
            // three engines, where Markdown already blanked them (carve-rs#651).
            //
            // WHETHER to show the parenthetical is decided from the AUTHORED
            // destination, WHAT goes in it from the sanitized one. Deciding both
            // from the sanitized value turns a denied autolink - whose text IS the
            // URL, so no parenthetical was ever shown - into
            // `javascript:alert(1) ()`.
            let authored = strip_controls(&link.href);
            let mut out = style(&text, &(UNDERLINE.to_string() + FG_BLUE));
            if !authored.starts_with('#') && authored != strip_ansi(&text) {
                let shown = crate::escape::sanitize_url(&authored);
                out.push_str(&style(&format!(" ({shown})"), DIM));
            }
            out
        }
        InlineNode::Image(image) => render_image(image),
        InlineNode::Span(span) => render_inlines(&span.children, ctx, depth + 1),
        InlineNode::Math(math) => style(&strip_controls(&math.content), FG_BRIGHT_MAGENTA),
        InlineNode::RawInline(_) => String::new(),
        // §27: always emitted (unlike raw passthrough above). It is prose, not
        // code, so it carries no code styling.
        InlineNode::LiteralInline(lit) => strip_controls(&lit.content),
        InlineNode::Symbol(symbol) => format!(":{}:", symbol.name),
        InlineNode::AutoLink(link) => {
            // Raw autolink content (URI keeps its scheme; email shows address).
            style(
                &strip_controls(&link.text),
                &(UNDERLINE.to_string() + FG_BLUE),
            )
        }
        InlineNode::Mention(mention) => format!("@{}", strip_controls(&mention.user)),
        InlineNode::Tag(tag) => format!("#{}", strip_controls(&tag.name)),
        InlineNode::Extension(extension) => render_inlines(&extension.children, ctx, depth + 1),
        InlineNode::Abbreviation(abbr) => {
            // Bound cumulative expansion bytes (memory-amplification DoS): once
            // the budget is exhausted, drop the `(EXPANSION)` suffix and emit
            // the plain key only.
            let key = strip_controls(&abbr.abbr);
            if crate::abbr_budget::try_spend(abbr.expansion.len()) {
                format!(
                    "{}{}",
                    key,
                    style(&format!(" ({})", strip_controls(&abbr.expansion)), DIM)
                )
            } else {
                key
            }
        }
        InlineNode::Footnote(footnote) => {
            if let Some(inline) = &footnote.inline {
                // A footnote body is an aside, not part of the label it sits in
                // -- the HTML target renders it outside the anchor entirely. So
                // a reference inside one is not nested and still links.
                let outer = std::mem::replace(&mut ctx.link_depth, 0);
                let rendered = render_inlines(inline, ctx, depth + 1);
                ctx.link_depth = outer;
                format!("({rendered})")
            } else {
                let id = strip_controls(footnote.id.as_deref().unwrap_or(""));
                if ctx.defined_footnotes.contains(&id) {
                    style(&format!("[{id}]"), &(FG_CYAN.to_string() + BOLD))
                } else {
                    // UNRESOLVED: literal and UNSTYLED, as the HTML target
                    // renders it. Styling it announced a footnote the document
                    // does not have.
                    format!("[^{id}]")
                }
            }
        }
        InlineNode::SoftBreak(_) => " ".to_string(),
        InlineNode::HardBreak(_) => "\n".to_string(),
        InlineNode::CriticInsert(insert) => style(
            &render_inlines(&insert.children, ctx, depth + 1),
            &(FG_GREEN.to_string() + UNDERLINE),
        ),
        InlineNode::CriticDelete(delete) => style(
            &render_inlines(&delete.children, ctx, depth + 1),
            &(STRIKE.to_string() + "\x1b[31m"),
        ),
        InlineNode::CriticSubstitute(sub) => format!(
            "{}{}",
            style(
                &strip_controls(&sub.old_text),
                &(STRIKE.to_string() + "\x1b[31m")
            ),
            style(
                &strip_controls(&sub.new_text),
                &(FG_GREEN.to_string() + UNDERLINE)
            ),
        ),
        // A critic comment is VISIBLE content: the HTML target renders it as
        // `<span class="critic-comment"> note </span>`, so dropping it here made
        // two targets of one engine disagree about whether the document says it.
        // carve-php kept it (carve#352, corpus 33-editorial-markup).
        InlineNode::Comment(_) => String::new(),
        InlineNode::CriticComment(c) => strip_controls(&c.text),
        // A RESOLVED cross-reference renders exactly like a link to the same
        // heading, because that is what it is - the href is a fragment, so the
        // `(href)` suffix a link would add is suppressed there too. Only an
        // UNRESOLVED one degrades to its literal source.
        // The label is the target's cloned inline NODES (PART 9R R4), so the
        // source run reaches this renderer and its own typography mode applies
        // to it. A caption target has no nodes - its label is LABEL + NUMBER -
        // so that one is still a string.
        InlineNode::CrossRef(crossref) => {
            let resolved = ctx
                .crossref_index
                .resolve(&crossref.target)
                .map(|(id, title)| (id.to_string(), title.to_string()));
            match resolved {
                None => format!("</#{}>", strip_controls(&crossref.target)),
                Some((id, title)) => {
                    let label = ctx.crossref_index.label(&id);
                    let text = match &label {
                        Some(nodes) => render_inlines(nodes, ctx, depth + 1),
                        None => strip_controls(&title),
                    };
                    if ctx.link_depth > 0 {
                        text
                    } else {
                        style(&text, &(UNDERLINE.to_string() + FG_BLUE))
                    }
                }
            }
        }
        // Tier-2 ext node; the core renderer has no numbering, so emit the source.
        InlineNode::CitationGroup(group) => strip_controls(&group.raw),
        InlineNode::CaptionNumber(number) => number
            .number
            .map(|n| n.to_string())
            .unwrap_or_else(|| "#".to_string()),
    }
}

fn render_image(node: &Image) -> String {
    if node.ref_label.is_some() && node.src.is_empty() {
        return strip_controls(node.raw_ref.as_deref().unwrap_or_default());
    }
    format!(
        "{}{}{}",
        style("[img:", FG_MAGENTA),
        if node.alt.is_empty() {
            String::new()
        } else {
            format!(" {}", strip_controls(&node.alt))
        },
        style("]", FG_MAGENTA)
    )
}

fn strip_ansi(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if let ('\x1b', Some('[')) = (ch, chars.peek().copied()) {
            chars.next();
            for c in chars.by_ref() {
                if c == 'm' {
                    break;
                }
            }
            continue;
        }
        out.push(ch);
    }
    out
}

// East-Asian Wide / Fullwidth code points occupy two terminal columns; every
// other occupies one. Mirrors PHP's `mb_strwidth` for real content (CJK, Kana,
// Hangul, fullwidth forms, most emoji) so an ANSI table with CJK cells aligns
// with its box borders.
fn is_wide_char(c: char) -> bool {
    let cp = c as u32;
    matches!(cp,
        0x1100..=0x115f
        | 0x2329 | 0x232a
        | 0x2e80..=0x303e
        | 0x3041..=0x33ff
        | 0x3400..=0x4dbf
        | 0x4e00..=0x9fff
        | 0xa000..=0xa4cf
        | 0xac00..=0xd7a3
        | 0xf900..=0xfaff
        | 0xfe10..=0xfe19
        | 0xfe30..=0xfe6f
        | 0xff00..=0xff60
        | 0xffe0..=0xffe6
        | 0x1f300..=0x1faff
        | 0x20000..=0x3fffd
    )
}

fn width(text: &str) -> usize {
    strip_ansi(text)
        .chars()
        .map(|c| if is_wide_char(c) { 2 } else { 1 })
        .sum()
}

fn to_subscript(text: &str) -> String {
    text.chars()
        .map(|ch| match ch {
            '0' => '₀',
            '1' => '₁',
            '2' => '₂',
            '3' => '₃',
            '4' => '₄',
            '5' => '₅',
            '6' => '₆',
            '7' => '₇',
            '8' => '₈',
            '9' => '₉',
            '+' => '₊',
            '-' => '₋',
            '=' => '₌',
            '(' => '₍',
            ')' => '₎',
            other => other,
        })
        .collect()
}

fn to_superscript(text: &str) -> String {
    text.chars()
        .map(|ch| match ch {
            '0' => '⁰',
            '1' => '¹',
            '2' => '²',
            '3' => '³',
            '4' => '⁴',
            '5' => '⁵',
            '6' => '⁶',
            '7' => '⁷',
            '8' => '⁸',
            '9' => '⁹',
            '+' => '⁺',
            '-' => '⁻',
            '=' => '⁼',
            '(' => '⁽',
            ')' => '⁾',
            'n' => 'ⁿ',
            'i' => 'ⁱ',
            other => other,
        })
        .collect()
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
    format!("{trimmed}\n").replace(crate::NBSP_PLACEHOLDER, " ")
}
