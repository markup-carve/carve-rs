use crate::ast::*;
use crate::render::MAX_RENDER_DEPTH;

struct CarveContext {
    block_depth: usize,
    inline_depth: usize,
    list_depth: usize,
    /// Depth of line-block nesting, so the inline writer drops the explicit
    /// backslash: inside a `::: |` fence every newline already IS a hard break.
    line_block_depth: usize,
    colon_fence_depth: usize,
    /// Inside a table cell, where a leading `^` cannot open a caption: a
    /// caption marker is a BLOCK line, and a cell's content is not one.
    table_cell_depth: usize,
    escape_mode: EscapeMode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EscapeMode {
    Minimal,
    /// Minimal, plus the escape for a caption marker - `^` followed by a space
    /// at the start of a block line. The middle step in `render_carve`.
    MinimalWithCaptionMarkers,
    Conservative,
}

/// Render a tree that did NOT come from the parser, refusing at the ceiling.
///
/// `Err` when the tree nests deeper than [`crate::MAX_RENDER_DEPTH`], naming the
/// renderer and the bound (PART 9 §25). A parser-produced tree cannot reach it -
/// the parse cap sits below the ceiling - so this fails only for a tree built
/// through the API or read by `from_json`, which is the caller who can act on it.
pub fn render_carve(doc: &Document) -> Result<String, crate::RenderDepthError> {
    let watch = crate::render_depth::RenderDepthWatch::new();
    watch.into_result(render_carve_unguarded(doc))
}

fn render_carve_unguarded(doc: &Document) -> String {
    let minimal = render_with_escapes(doc, EscapeMode::Minimal);
    let conservative = render_with_escapes(doc, EscapeMode::Conservative);
    if minimal == conservative || escaping_is_redundant(&minimal, &conservative) {
        return minimal;
    }
    // The minimal form changed the parse, and the blunt answer is to escape
    // EVERY candidate in the document. One dangerous character then pulls
    // escapes onto characters that were never at risk: that is how
    // `\^ Figure 1: moon` became `\^ Figure 1\: moon`, a colon escape that
    // changes no parse in any engine.
    //
    // So try one step in between - minimal, plus the escape for the construct
    // a line-initial `^ ` forms. If that reproduces the conservative parse it
    // is the least escaping that preserves meaning, which is what PART 11 §4
    // asks for.
    //
    // This replaces an UNCONDITIONAL caption escape (carve-rs#558, mine). That
    // could not tell corpus 158, where the caret is load-bearing because the
    // writer dedents the image to column 0, from `![a][nope]` + `^ cap`, where
    // the image resolves to nothing, the caption promotes nothing, and the bare
    // caret changes no parse. Asking the parser tells them apart; a rule about
    // position cannot (carve-rs#559).
    let with_caption_escapes = render_with_escapes(doc, EscapeMode::MinimalWithCaptionMarkers);
    if with_caption_escapes != minimal
        && escaping_is_redundant(&with_caption_escapes, &conservative)
    {
        return with_caption_escapes;
    }
    conservative
}

fn render_with_escapes(doc: &Document, escape_mode: EscapeMode) -> String {
    let mut ctx = CarveContext {
        block_depth: 0,
        inline_depth: 0,
        list_depth: 0,
        line_block_depth: 0,
        colon_fence_depth: 0,
        table_cell_depth: 0,
        escape_mode,
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

fn escaping_is_redundant(minimal: &str, conservative: &str) -> bool {
    let parsed = std::panic::catch_unwind(|| {
        (
            comparable_document(crate::parse::parse_for_carve(minimal)),
            comparable_document(crate::parse::parse_for_carve(conservative)),
        )
    });
    parsed.is_ok_and(|(minimal_doc, conservative_doc)| minimal_doc == conservative_doc)
}

fn comparable_document(mut doc: Document) -> Document {
    doc.source_len = 0;
    for block in &mut doc.children {
        normalize_escapes_block(block);
    }
    // Footnote definitions are NOT in `children` -- they hang off the document in
    // their own map. Leaving them un-normalized meant any escape inside one made
    // the two renders differ, so W4 escalated the WHOLE document to conservative:
    // `a.` alone formatted as `a.`, but the same paragraph beside a `[^f]: b.`
    // definition came back `a\.` (carve#352, corpus 22-footnotes).
    for blocks in doc.footnote_defs.values_mut() {
        for block in blocks.iter_mut() {
            normalize_escapes_block(block);
        }
    }
    doc
}

/// Collapse adjacent text and escaped-text nodes into one text node.
///
/// An escape is exactly what this comparison is deciding, so the two renders
/// must not be told apart BY it. Escaping a character both retypes the node and
/// SPLITS the run it sat in - `blue.` is one text node, `blue\.` is a text node
/// plus an escaped-text node - so without this every candidate character would
/// report a difference and escalate the whole document to conservative
/// escaping.
///
/// What survives the merge is the question worth asking: same characters, same
/// order, same surrounding structure - does dropping the escapes change
/// anything ELSE? PART 11 section 1 states this as the invariant's own
/// definition of equality.
fn normalize_escapes_inlines(nodes: &mut Vec<InlineNode>) {
    let mut merged: Vec<InlineNode> = Vec::with_capacity(nodes.len());
    for node in nodes.drain(..) {
        let text = match node {
            InlineNode::Text(t) => Some(t.value),
            InlineNode::EscapedText(t) => Some(t.value),
            other => {
                let mut other = other;
                normalize_escapes_nested(&mut other);
                merged.push(other);
                None
            }
        };
        if let Some(t) = text {
            if let Some(InlineNode::Text(previous)) = merged.last_mut() {
                previous.value.push_str(&t);
            } else {
                merged.push(InlineNode::text(t));
            }
        }
    }
    *nodes = merged;
}

/// Recurse into an inline node that carries inline children of its own.
fn normalize_escapes_nested(node: &mut InlineNode) {
    match node {
        InlineNode::Comment(_) => {}
        InlineNode::Emphasis(e) => normalize_escapes_inlines(&mut e.children),
        InlineNode::Link(l) => normalize_escapes_inlines(&mut l.children),
        InlineNode::Span(s) => normalize_escapes_inlines(&mut s.children),
        // An inline extension carries inline children too, and omitting it meant
        // an escape inside one made the two renders differ and escalated the
        // WHOLE document: `Press :kbd[Ctrl+C] to copy.` came back
        // `Press :kbd[Ctrl\+C] to copy\.` (carve#352, corpus 45-inline-extensions).
        InlineNode::Extension(e) => normalize_escapes_inlines(&mut e.children),
        // Editorial insert and delete carry inline children too. Omitting them
        // escalated any document containing an escape inside one: `{++a++}{.a}`
        // came back `{+\+a\++}{.a}`, over-escaping content the HTML target shows
        // as a literal `+a+` (carve#352, corpus 126).
        InlineNode::CriticInsert(i) => normalize_escapes_inlines(&mut i.children),
        InlineNode::CriticDelete(d) => normalize_escapes_inlines(&mut d.children),
        InlineNode::Footnote(f) => {
            if let Some(inline) = &mut f.inline {
                normalize_escapes_inlines(inline);
            }
        }
        // Listed rather than caught by `_`, so a new inline node that carries
        // children fails to compile here instead of being silently skipped. That
        // catch-all is how the extension gap (carve-rs#310) and the editorial gap
        // above both survived: adding a node type with children was enough to
        // introduce an over-escaping bug, with nothing to notice it.
        InlineNode::Text(_)
        | InlineNode::EscapedText(_)
        | InlineNode::SmartPunctuation(_)
        | InlineNode::Code(_)
        | InlineNode::Image(_)
        | InlineNode::Math(_)
        | InlineNode::RawInline(_)
        | InlineNode::LiteralInline(_)
        | InlineNode::Symbol(_)
        | InlineNode::AutoLink(_)
        | InlineNode::CrossRef(_)
        | InlineNode::CaptionNumber(_)
        | InlineNode::Mention(_)
        | InlineNode::Tag(_)
        | InlineNode::CitationGroup(_)
        | InlineNode::Abbreviation(_)
        | InlineNode::SoftBreak(_)
        | InlineNode::HardBreak(_)
        | InlineNode::CriticSubstitute(_)
        | InlineNode::CriticComment(_) => {}
    }
}

fn normalize_escapes_block(block: &mut BlockNode) {
    match block {
        BlockNode::Heading(h) => normalize_escapes_inlines(&mut h.children),
        BlockNode::Paragraph(p) => normalize_escapes_inlines(&mut p.children),
        BlockNode::List(l) => {
            for item in &mut l.items {
                for child in &mut item.children {
                    normalize_escapes_block(child);
                }
            }
        }
        BlockNode::BlockQuote(b) => {
            for child in &mut b.children {
                normalize_escapes_block(child);
            }
            if let Some(attr) = &mut b.attribution {
                normalize_escapes_inlines(attr);
            }
        }
        BlockNode::Table(t) => {
            if let Some(cap) = &mut t.caption {
                normalize_escapes_inlines(cap);
            }
            for row in &mut t.rows {
                for cell in &mut row.cells {
                    normalize_escapes_inlines(&mut cell.children);
                }
            }
        }
        BlockNode::Admonition(a) => {
            if let Some(title) = &mut a.title {
                normalize_escapes_inlines(title);
            }
            for child in &mut a.children {
                normalize_escapes_block(child);
            }
        }
        BlockNode::LineBlock(lb) => {
            for child in &mut lb.children {
                normalize_escapes_block(child);
            }
        }
        BlockNode::Div(d) => {
            for child in &mut d.children {
                normalize_escapes_block(child);
            }
        }
        BlockNode::DefinitionList(dl) => {
            for item in &mut dl.items {
                for term in &mut item.terms {
                    normalize_escapes_inlines(term);
                }
                for def in &mut item.definitions {
                    for child in def.iter_mut() {
                        normalize_escapes_block(child);
                    }
                }
            }
        }
        BlockNode::Figure(f) => {
            normalize_escapes_inlines(&mut f.caption);
            normalize_escapes_figure_target(f);
        }
        BlockNode::Extension(e) => {
            for child in &mut e.children {
                normalize_escapes_block(child);
            }
        }
        BlockNode::CodeBlock(_)
        | BlockNode::AbbreviationDef(_)
        | BlockNode::RawBlock(_)
        | BlockNode::Comment(_)
        | BlockNode::BlockImage(_)
        | BlockNode::ThematicBreak(_) => {}
    }
}

fn normalize_escapes_figure_target(f: &mut crate::ast::Figure) {
    match &mut f.target {
        FigureTarget::BlockQuote(b) => {
            for child in &mut b.children {
                normalize_escapes_block(child);
            }
        }
        FigureTarget::Table(t) => {
            if let Some(cap) = &mut t.caption {
                normalize_escapes_inlines(cap);
            }
            for row in &mut t.rows {
                for cell in &mut row.cells {
                    normalize_escapes_inlines(&mut cell.children);
                }
            }
        }
        FigureTarget::Paragraph(p) => normalize_escapes_inlines(&mut p.children),
        FigureTarget::Image(_) | FigureTarget::CodeBlock(_) => {}
    }
}

fn render_blocks(blocks: &[BlockNode], ctx: &mut CarveContext) -> String {
    if ctx.block_depth >= MAX_RENDER_DEPTH {
        crate::render_depth::record("carve");
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

fn with_reset_colon_fence_depth<T>(
    ctx: &mut CarveContext,
    f: impl FnOnce(&mut CarveContext) -> T,
) -> T {
    let saved = ctx.colon_fence_depth;
    ctx.colon_fence_depth = 0;
    let out = f(ctx);
    ctx.colon_fence_depth = saved;
    out
}

fn render_inside_colon_container(blocks: &[BlockNode], ctx: &mut CarveContext) -> String {
    ctx.colon_fence_depth += 1;
    let body = render_blocks(blocks, ctx);
    ctx.colon_fence_depth -= 1;
    body
}

/// Render a list item's children. A loose item separates every block with a
/// blank line. A tight item joins its blocks with a single newline so the
/// re-parse stays tight - EXCEPT it keeps the blank line adjacent to a nested
/// list child, whose own loose/tight rendering (and the continuation-indent
/// logic below) needs it. Without the tight join, a tight item with more than
/// one child (e.g. text after a fenced block, corpus 162) would be loosened by
/// the blank lines, breaking to_html(fmt(x)) == to_html(x); without the
/// nested-list exception, a tight item whose child is a nested list (corpus
/// 142) would stop being idempotent.
fn render_item_blocks(blocks: &[BlockNode], tight: bool, ctx: &mut CarveContext) -> String {
    if !tight {
        return render_blocks(blocks, ctx);
    }
    if ctx.block_depth >= MAX_RENDER_DEPTH {
        crate::render_depth::record("carve");
        return String::new();
    }
    ctx.block_depth += 1;
    let mut out = String::new();
    let mut prev: Option<&BlockNode> = None;
    for block in blocks {
        let rendered = render_block(block, ctx);
        if rendered.is_empty() {
            continue;
        }
        if let Some(prev_block) = prev {
            // A tight item joins every child with a single newline, including a
            // nested list. The blank line that used to be kept here existed to
            // work around nested looseness propagating to the outer item; with
            // that fixed in line_starts_paragraph, keeping it would insert a
            // blank the author never wrote and diverge from carve-js/carve-php.
            let _ = prev_block;
            out.push('\n');
        }
        out.push_str(&rendered);
        prev = Some(block);
    }
    ctx.block_depth -= 1;
    out
}

fn render_block(node: &BlockNode, ctx: &mut CarveContext) -> String {
    match node {
        BlockNode::Heading(heading) => {
            // A heading is SINGLE-LINE (PART 2), so its text must not contain a
            // newline: emitting one would end the heading and silently re-parse
            // the remainder as a following block. No parse builds such a
            // heading, but an ingested AST can - PART 12 lets any inline sit in
            // a heading, break nodes included - so a break collapses to a
            // single space here rather than corrupting the document it is
            // written back to. Matches carve-js.
            let rendered = render_inlines(&heading.children, ctx);
            let text = collapse_breaks(trim_non_nbsp(&rendered));
            with_block_attrs(
                &heading.attrs,
                &format!("{} {}", "#".repeat(heading.level as usize), text),
            )
        }
        BlockNode::Paragraph(paragraph) => {
            let body = guard_thematic_break_lines(&render_inlines(&paragraph.children, ctx));
            with_block_attrs(&paragraph.attrs, &body)
        }
        BlockNode::CodeBlock(code) => {
            let fence = safe_fence(&code.content, 3);
            let info = code_fence_info(
                code.lang.as_deref(),
                code.title.as_deref(),
                code.label.as_deref(),
            );
            // The opener's quoted title is resolved onto `attrs.title` at parse
            // time so it reaches every consumer, but the fence carries it too -
            // emitting both says it twice and re-parses with an attribute ORDER
            // slot the source never had (carve#369). The fence is the authored
            // spelling, so it wins.
            let attrs = match (&code.title, &code.attrs) {
                (Some(title), Some(a)) if a.key_values.get("title") == Some(title) => {
                    without_key(a, "title")
                }
                _ => code.attrs.clone(),
            };
            with_block_attrs(
                &attrs,
                &format!(
                    "{fence}{info}\n{}\n{fence}",
                    protect_verbatim(&code.content)
                ),
            )
        }
        BlockNode::BlockQuote(quote) => {
            let inner =
                with_reset_colon_fence_depth(ctx, |ctx| render_blocks(&quote.children, ctx));
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
        BlockNode::List(list) => with_block_attrs(
            &list.attrs,
            &with_reset_colon_fence_depth(ctx, |ctx| render_list(list, ctx)),
        ),
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
            let fence = colon_fence_for(ctx);
            let body = render_inside_colon_container(&admonition.children, ctx);
            with_block_attrs(
                &admonition.attrs,
                &format!("{fence} {}{title}{label}\n{body}\n{fence}", admonition.kind),
            )
        }
        BlockNode::LineBlock(lb) => {
            // `::: |` is the line-block opener (PART 3, line_block_open).
            // Emitting a bare `:::` and tagging the node with a `.line-block`
            // class instead re-parsed as an ordinary div, so the node type
            // changed across a format round trip and
            // `parse(fmt(x)) == parse(x)` did not hold (carve issue 359).
            //
            // Inside the fence every newline IS a hard break (PART 3,
            // line_block_body), so the explicit backslash the inline writer
            // emits for a HardBreak would double it on re-parse.
            ctx.line_block_depth += 1;
            let fence = colon_fence_for(ctx);
            let body = render_inside_colon_container(&lb.children, ctx);
            ctx.line_block_depth -= 1;
            with_block_attrs(&lb.attrs, &format!("{fence} |\n{body}\n{fence}"))
        }
        BlockNode::Div(div) => {
            let label = div
                .label
                .as_ref()
                .map(|label| format!(" [{}]", escape_bracket_text(label)))
                .unwrap_or_default();
            let fence = colon_fence_for(ctx);
            let body = render_inside_colon_container(&div.children, ctx);
            with_block_attrs(&div.attrs, &format!("{fence}{label}\n{body}\n{fence}"))
        }
        BlockNode::DefinitionList(list) => with_block_attrs(
            &list.attrs,
            &with_reset_colon_fence_depth(ctx, |ctx| render_definition_list(&list.items, ctx)),
        ),
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

/// A copy of `attrs` without one key-value, dropping the slot from `order`.
/// Returns `None` when the removal leaves nothing to render.
fn without_key(attrs: &Attrs, key: &str) -> Option<Attrs> {
    let mut next = attrs.clone();
    next.key_values.remove(key);
    next.order
        .retain(|slot| !matches!(slot, AttrSlot::Key(k) if k == key));
    if next.id.is_none() && next.classes.is_empty() && next.key_values.is_empty() {
        return None;
    }
    Some(next)
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
        // NO absolute depth term. The parent item's continuation prefix is
        // already the child list's indentation, so adding `"  " * (depth - 1)`
        // on top indented every level twice - and the two-space strip below was
        // compensating for it. Output grew as O(depth^3) where the source is
        // O(depth^2), and `05-lists-5` came back with four spaces where it was
        // written with two (carve-rs#594, the same defect carve-js fixed in its
        // #653).
        let mut prefix = if node.ordered {
            let marker = if node.bare_marker {
                String::new()
            } else {
                ordered_marker(counter, node.ol_type)
            };
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
        let mut content = render_item_blocks(&item.children, node.tight, ctx);
        let trimmed_content = trim_non_nbsp(&content);
        if trimmed_content.is_empty()
            || (trimmed_content.starts_with("[^") && trimmed_content.contains(": "))
        {
            content = "+".to_string();
        }
        let content = trim_non_nbsp(&content).to_string();
        let mut lines = if content.is_empty() {
            vec!["".to_string()]
        } else {
            content.split('\n').map(str::to_string).collect()
        };
        let first = lines.remove(0);
        out.push_str(&format!("{prefix}{first}\n"));
        let continuation = " ".repeat(prefix.len());
        for line in lines {
            if line.is_empty() || line.chars().eq([VERBATIM_BLANK]) {
                // A blank continuation line is emitted EMPTY, never indented to
                // the content column: PART 11 section 7 forbids a whitespace-only
                // line, because editors and CI that strip trailing whitespace
                // rewrite one, and `fmt` would then report a diff on a file
                // nobody edited (carve#375).
                //
                // A blank line INSIDE verbatim content arrives as the sentinel
                // rather than as "", because `protect_verbatim` encodes it to
                // keep whole-document normalization off it. That made it look
                // like content here, so it was indented, and the indent stayed
                // behind when the sentinel was restored to nothing - a
                // whitespace-only line, from a code block in a list item
                // (carve-rs#440). The sentinel is written through UNindented so
                // it keeps protecting the line it stands for.
                out.push_str(&line);
                out.push('\n');
            } else {
                out.push_str(&format!("{continuation}{line}\n"));
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

fn colon_fence_for(ctx: &CarveContext) -> String {
    ":".repeat(3 + ctx.colon_fence_depth)
}

/// Tables prefer the NATIVE header form: an `=` on each header cell, plus the
/// per-cell `<`/`>`/`~` alignment markers.
///
/// The GFM delimiter row is an accepted alias on input, but it says something
/// the AST does not: its alignment applies to the WHOLE column, header and body
/// alike (PART 9 T7), while alignment on the AST belongs to each cell. Writing a
/// delimiter row for the ordinary shape - an aligned header over unaligned body
/// cells - brought every body cell back aligned, so `parse(fmt(x)) == parse(x)`
/// did not hold (carve issue 359).
///
/// Two header shapes have no native spelling, because `header_cell` in the
/// grammar is `'=' [alignment_marker] content` and admits neither an attribute
/// block nor a span marker:
///
/// ```text
/// | < | b |     a span marker promoted to a header cell
/// |{.x} a | b | a header cell carrying attributes
/// ```
///
/// Those still need a delimiter row to promote the first row. It is emitted BARE
/// (`|---|---|`), never with colons: the cells keep their own alignment markers,
/// so the delimiter contributes structure only and cannot spill alignment down
/// the column.
fn render_table(node: &Table, ctx: &mut CarveContext) -> String {
    let mut rows = Vec::new();
    let columns = node
        .rows
        .iter()
        .map(|row| row.cells.len())
        .max()
        .unwrap_or(0);
    let header_row = node
        .rows
        .first()
        .is_some_and(|row| !row.cells.is_empty() && row.cells.iter().all(|cell| cell.header));
    let needs_delimiter = header_row
        && node.rows.first().is_some_and(|row| {
            row.cells
                .iter()
                .any(|cell| cell.span.is_some() || cell.attrs.is_some())
        });

    for (row_index, row) in node.rows.iter().enumerate() {
        let mut cells = Vec::new();
        for i in 0..columns {
            if let Some(cell) = row.cells.get(i) {
                // In the delimiter form the promoted row is written as ordinary
                // data cells - the row after it is what makes them headers.
                let mark_header = !(needs_delimiter && row_index == 0);
                cells.push(render_table_cell(cell, ctx, mark_header));
            } else {
                cells.push(RenderedCell {
                    text: String::new(),
                    tight: false,
                });
            }
        }
        rows.push(render_table_row(&cells, &render_attrs(&row.attrs)));
    }
    if needs_delimiter {
        let sep = vec!["---"; columns].join("|");
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

fn render_table_cell(cell: &TableCell, ctx: &mut CarveContext, mark_header: bool) -> RenderedCell {
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
        if cell.header && mark_header { "=" } else { "" },
        align_marker(cell.align)
    );
    ctx.table_cell_depth += 1;
    let content = render_inlines(&cell.children, ctx);
    ctx.table_cell_depth -= 1;
    RenderedCell {
        text: format!("{prefix}{content}"),
        tight: !prefix.is_empty(),
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
        // TWO spaces, the body's own column (PART 9 §16). A wider indent is legal
        // continuation but leaves the body's blocks at a relative column above
        // zero, and an indented block opener does not open a block - so a table
        // or a quote written at three came back as a paragraph.
        def_lines.push(format!("  {line}"));
    }
    def_lines.join("\n")
}

fn render_inlines(nodes: &[InlineNode], ctx: &mut CarveContext) -> String {
    if ctx.inline_depth >= MAX_RENDER_DEPTH {
        crate::render_depth::record("carve");
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
        // The one target that publishes it: the author wrote `%% note`, and
        // the canonical form writes it back verbatim. The parser drops the
        // whitespace before the marker (it is not part of the text), so put a
        // single space back unless the comment opens the line - `%%` only
        // starts a comment at the start of a line or after whitespace, so
        // gluing it to the word before would re-parse as literal text.
        InlineNode::Comment(c) => {
            let lead = if prev_char == '\0' || prev_char == '\n' {
                ""
            } else {
                " "
            };
            format!("{lead}%% {}", c.content)
        }
        InlineNode::Text(text) => escape_text(
            &resolve_nbsp_placeholder(&text.value, ctx.line_block_depth > 0),
            ctx.escape_mode,
            // Does this node's first character sit at the start of a block
            // line? Only there can a `^` be read back as a caption marker.
            (prev_char == '\0' || prev_char == '\n') && ctx.table_cell_depth == 0,
        ),
        InlineNode::EscapedText(text) => format!("\\{}", text.value),
        InlineNode::SmartPunctuation(s) => s.value.clone(),
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
        InlineNode::Code(code) => {
            format!("{}{}", render_code(&code.value), render_attrs(&code.attrs))
        }
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
        InlineNode::LiteralInline(lit) => {
            // §27: `!` prefix on a verbatim span. A trailing attribute block is
            // the ordinary inline attribute block (same as a code span carries).
            // `render_code` widens the backtick fence when the content holds
            // backticks, so the round-trip re-parses identically.
            format!("!{}{}", render_code(&lit.content), render_attrs(&lit.attrs))
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
        InlineNode::Abbreviation(abbr) => escape_text(&abbr.abbr, ctx.escape_mode, false),
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
        InlineNode::SoftBreak(_) => "\n".to_string(),
        InlineNode::HardBreak(_) => {
            if ctx.line_block_depth > 0 {
                "\n".to_string()
            } else {
                "\\\n".to_string()
            }
        }
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
    // UNRESOLVED means no destination, not "carries a label": PART 12 §3a keeps
    // `ref` and `raw_ref` on a RESOLVED reference too, so the label alone no
    // longer answers this and a working reference round-tripped as its own
    // source instead of normalizing to the inline form (carve#597).
    // The AUTHORED source, in two cases. UNRESOLVED: there is no destination to
    // write instead. HEADING-DERIVED (PART 11 R1, carve#478): there is no
    // definition line, so the reference is the only record of what the author
    // wrote, and resolving it bakes a generated id into the source on every fmt
    // pass. An explicit definition normalizes to the inline form - its
    // definition line is dropped either way.
    if node.ref_label.is_some()
        && node.raw_ref.is_some()
        && (node.href.is_empty() || node.from_heading_reference)
    {
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
    if node.ref_label.is_some() && node.raw_ref.is_some() && node.src.is_empty() {
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
    // Pad exactly where the parser strips, so the strip is reversible and fmt
    // stays idempotent; the padding sits inside the fence, so a trailing
    // attribute block still attaches to the closing run. The parser strips one
    // leading and one trailing space when the content BOTH begins and ends with
    // a space but is NOT entirely spaces (see strip_verbatim_padding in
    // parse.rs), and needs a space around backtick-adjacent content. All-space
    // content must therefore NOT be padded: it is emitted verbatim and read back
    // unchanged. Padding it instead grew the span by two spaces on every fmt
    // pass. One-sided space is left as-is (the parser only strips when both
    // sides are spaces).
    let needs_pad = content.starts_with('`')
        || content.ends_with('`')
        || (content.starts_with(' ')
            && content.ends_with(' ')
            && !content.chars().all(|c| c == ' '));
    if needs_pad {
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

/// Resolve the NBSP placeholder, which stands for two different things.
///
/// An escaped space (`\\ `) is written back as an ESCAPED SPACE. Resolving it to
/// a real non-breaking space instead lost the distinction the parser draws:
/// `10\\ kg` came back carrying U+00A0, which re-parses as a literal nbsp rather
/// than as an escape, so the text node differed even though the HTML did not.
/// carve-js fixed that in carve#369; this engine had not (carve#352, corpus
/// 29-non-breaking-space).
///
/// A line block's leading indentation resolves to ORDINARY spaces: that is the
/// source form the parser reads back as indentation, whereas a real nbsp
/// re-parses as literal text and the text node comes back different (carve issue
/// 359).
///
/// Only a run at the start of a line is indentation, so a mid-line escaped
/// space inside a line block still resolves to a real nbsp. The leading run is
/// handed to the verbatim scheme, which restores plain spaces after
/// `normalize` has run.
/// Stands in for an escaped space until `normalize` expands it, so the backslash
/// is not seen by the escaper (which would double it).
const ESCAPED_SPACE: &str = "\u{e010}";

/// Writer-internal staging for a space and a tab that must survive escaping.
///
/// These live in U+E010.. deliberately. U+E000 is a PUBLISHED value - it is the
/// no-break-space placeholder a parsed document carries - so a writer marker
/// sharing that code point would be indistinguishable from document content.
/// They used to sit at U+E001 and U+E002, which is why the published placeholder
/// could not move there (carve-rs#404).
const STAGED_SPACE: char = '\u{e011}';
const STAGED_TAB: char = '\u{e012}';
/// A blank line inside verbatim content, encoded so whole-document
/// normalization leaves it alone. `restore_verbatim` turns it back into
/// nothing.
const VERBATIM_BLANK: char = '\u{e003}';

fn resolve_nbsp_placeholder(text: &str, in_line_block: bool) -> String {
    if !in_line_block {
        return text.replace(crate::NBSP_PLACEHOLDER, ESCAPED_SPACE);
    }
    text.split('\n')
        .map(stage_line_block_layout)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Write a line block's preserved whitespace back as plain spaces.
///
/// The runs staged here are exactly the ones the parser reproduces from plain
/// spaces: a LEADING run of any width, and a medial or trailing run of two or
/// more (grammar §23). A lone medial placeholder can then only have come from
/// an escaped space, so `a\ b` still round-trips as written. Two ADJACENT
/// escaped spaces are the one form that changes - `a\ \ b` is written back as
/// `a  b` - because inside a line block those are the same document: both parse
/// to the same pair of placeholders.
fn stage_line_block_layout(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut seen_content = false;
    let mut chars = line.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch != crate::NBSP_PLACEHOLDER {
            out.push(ch);
            seen_content = true;
            continue;
        }

        let mut run = 1usize;
        while chars.peek() == Some(&crate::NBSP_PLACEHOLDER) {
            chars.next();
            run += 1;
        }

        if !seen_content || run >= 2 {
            for _ in 0..run {
                out.push(STAGED_SPACE);
            }
        } else {
            // A single placeholder mid-line is an escaped space, not layout.
            out.push_str(ESCAPED_SPACE);
        }
    }

    out
}

fn normalize(text: &str) -> String {
    // U+E000 marks an escaped space, and it resolves HERE rather than during
    // rendering because the backslash it expands to is itself an unconditional
    // escape: expanding earlier let escapeText double it, giving `10\\ kg`.
    let text = text.replace(ESCAPED_SPACE, "\\ ");
    // Strip a line's trailing whitespace only where it cannot be content. At the
    // end of a paragraph the parser drops it too, so the writer must; before a
    // SOFT BREAK the parser keeps it, and stripping it there changed the
    // rendered output (carve#359). A line whose successor is blank ends its
    // block; one followed by more text is mid-paragraph.
    let trimmed = trim_non_nbsp(&text);
    let raw: Vec<&str> = trimmed.split('\n').collect();
    let lines = raw
        .iter()
        .enumerate()
        .map(|(i, line)| {
            // A line whose only content is ASCII space or tab is emitted EMPTY,
            // wherever it sits (PART 11 section 7). Editors and CI that strip
            // trailing whitespace rewrite such a line, so `fmt` would report a
            // diff on a file nobody edited (carve#375). This is separate from
            // the block-final rule below, which is about a line WITH content:
            // that whitespace can be document content, and stripping it before
            // a soft break changed rendered output (carve#359).
            if !line.is_empty() && line.trim_matches([' ', '\t']).is_empty() {
                return String::new();
            }
            let ends_block = raw.get(i + 1).map_or(true, |next| next.trim().is_empty());
            if ends_block {
                trim_end_non_nbsp(line).to_string()
            } else {
                (*line).to_string()
            }
        })
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
            lines.push(VERBATIM_BLANK.to_string());
            continue;
        }
        let stripped = line.trim_end_matches([' ', '\t']);
        let tail: String = line[stripped.len()..]
            .chars()
            .map(|ch| if ch == ' ' { STAGED_SPACE } else { STAGED_TAB })
            .collect();
        lines.push(format!("{stripped}{tail}"));
    }
    lines.join("\n")
}

/// Protect a paragraph line that would re-parse as a thematic break.
///
/// Source indentation is not in the AST, so an indented `---` - a paragraph
/// holding an em dash - is emitted at column 0, where it stops being a
/// paragraph and becomes a thematic break.
///
/// Text nodes are already covered: the conservative form escapes the hyphens,
/// so the round-trip check sees the difference and picks that form. A
/// smart-punctuation run is not, because its source run is emitted verbatim in
/// BOTH forms - that is the point of the node - so the check never has a
/// difference to act on. Escaping the run in the conservative form does not
/// work either: it would make that form change the document, after which the
/// check could never prefer the minimal one.
///
/// It marks rather than escapes: escaping would split the run (a leading
/// escaped hyphen plus an en dash) and change the document just as surely,
/// while a leading space keeps the line a paragraph and keeps the em dash -
/// which is what the source said. The marker is a sentinel because normalize()
/// trims the document's leading whitespace, which would silently undo the guard
/// whenever the paragraph is the first block.
fn guard_thematic_break_lines(body: &str) -> String {
    if !body.contains('-') {
        return body.to_string();
    }
    body.split('\n')
        .map(|line| {
            let trimmed = line.trim_end_matches([' ', '\t']);
            if trimmed.len() >= 3 && trimmed.chars().all(|c| c == '-') {
                format!("\u{e004}{line}")
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Undo `protect_verbatim` and the thematic-break guard, POSITIONALLY.
///
/// This used to be four global `replace` calls, which cannot tell a sentinel the
/// writer inserted from one the AUTHOR wrote. So an authored U+E003 was deleted
/// and an authored U+E004 became a space - in 16 of 17 constructs measured, not
/// just in a code block (carve-rs#607).
///
/// Each sentinel is only ever inserted in ONE position, so each is only undone
/// there:
///
///   VERBATIM_BLANK  a line consisting of nothing else (protect_verbatim emits it
///                   for an empty line, and never inside one)
///   U+E004          a line PREFIX (guard_thematic_break_lines prepends it)
///   STAGED_SPACE    within the TRAILING whitespace run of a line, which is the
///   STAGED_TAB      only place protect_verbatim stages them
///
/// That leaves a much smaller residue than the global form: an authored sentinel
/// still collides if it sits in the exact position the writer uses one. Closing
/// that needs the insertion COUNTS, which is the design sketched on carve-rs#607
/// - this is the part that needs no bookkeeping and no AST traversal.
fn restore_verbatim(text: &str) -> String {
    text.split('\n')
        .map(|line| {
            // The marker may arrive INDENTED: inside a container the host adds
            // its columns before this runs, so the line is `  ` + marker rather
            // than the marker alone. Testing for the marker by itself missed
            // those and left a raw U+E003 in the output - caught by
            // `verbatim_content_stable_inside_containers` and by the corpus
            // formatter's semantic check on
            // `69-opaque-spans-inside-a-container-6`.
            //
            // Drop the marker and KEEP the indentation, which is what the global
            // replace did; a later trim removes a whitespace-only line. A marker
            // sitting next to real text is left alone, which is the point.
            let prefix = line.trim_end_matches(VERBATIM_BLANK);
            if prefix.len() != line.len()
                && prefix.chars().all(|c| c == ' ' || c == '\t' || c == '>')
            {
                // `>` belongs in the set: inside a block quote the line reaching
                // here is `> ` + marker, not the marker alone, and requiring pure
                // whitespace left a raw U+E003 in the output - which
                // `verbatim_content_stable_inside_containers` and the corpus
                // formatter's semantic check both caught. A line that is nothing
                // but container prefix plus the marker is the blank the marker
                // stands for, at any nesting.
                return prefix.to_string();
            }
            let line = match line.strip_prefix('\u{e004}') {
                Some(rest) => format!(" {rest}"),
                None => line.to_string(),
            };
            // The staged pair IS positional, once you read both insertion sites
            // together rather than looking for one position:
            //
            //   protect_verbatim stages a line's TRAILING run (any length)
            //   the line-block layout path stages a LEADING run (any length) or
            //     any run of TWO OR MORE - `!seen_content || run >= 2`
            //
            // So a run the writer inserted is always leading, trailing, or at
            // least two long. A SINGLE staged character sitting mid-line is
            // therefore never the writer's, and is left alone - which is the case
            // an author hits by typing one U+E011 or U+E012 in a code block.
            //
            // My earlier attempt at this restored only the trailing run, dropped
            // the medial case and broke `line_block_medial_gaps`; the note left
            // behind said separate sentinels were needed. They are not - the
            // run-length half of the layout condition is what was missing.
            //
            // RESIDUE, stated rather than implied: an authored run of two or more,
            // or a single one at the start or end of a line, still collides. That
            // needs the insertion counts.
            restore_staged_runs(&line)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Undo the staged whitespace pair only where the writer inserts it: a LEADING
/// run, a TRAILING run, or any run of two or more (see `restore_verbatim`).
///
/// A leading run is measured past the container prefix the host may have added
/// before this runs (spaces, tabs, `>`), the same allowance the blank-line marker
/// makes a few lines above.
fn restore_staged_runs(line: &str) -> String {
    let chars: Vec<char> = line.chars().collect();
    let staged = |c: char| c == STAGED_SPACE || c == STAGED_TAB;
    let prefix_end = chars
        .iter()
        .position(|&c| !(c == ' ' || c == '\t' || c == '>'))
        .unwrap_or(chars.len());
    let mut out = String::with_capacity(line.len());
    let mut i = 0usize;
    while i < chars.len() {
        if !staged(chars[i]) {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        let start = i;
        while i < chars.len() && staged(chars[i]) {
            i += 1;
        }
        let run = i - start;
        let writer_inserted = start == prefix_end || i == chars.len() || run >= 2;
        for &ch in &chars[start..i] {
            if writer_inserted {
                out.push(if ch == STAGED_SPACE { ' ' } else { '\t' });
            } else {
                out.push(ch);
            }
        }
    }
    out
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

/// Fold every line break in `text` (a hard break's `\` included) to one space,
/// then trim. Used where the target construct occupies exactly one line, so a
/// break in the tree would otherwise be written out as a real newline and
/// change the block structure on re-parse.
fn collapse_breaks(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut slashes = 0usize;
    while let Some(c) = chars.next() {
        if c == '\\' {
            slashes += 1;
            out.push(c);
            continue;
        }
        if c != '\n' {
            slashes = 0;
            out.push(c);
            continue;
        }
        // Only an ODD run of backslashes before the newline is a hard break's
        // marker; an even run is literal backslashes that happen to end the
        // line. Dropping one unconditionally turned `a\` plus a soft break into
        // `a\ b`, where the escape swallows the space and the backslash is lost.
        if slashes % 2 == 1 {
            out.pop();
        }
        slashes = 0;
        // Emit one space for the break and swallow the next line's indentation.
        out.push(' ');
        while chars.peek().is_some_and(|c| *c == ' ' || *c == '\t') {
            chars.next();
        }
    }
    trim_non_nbsp(&out).to_string()
}

fn trim_non_nbsp(text: &str) -> &str {
    text.trim_matches(|ch: char| ch.is_whitespace() && ch != '\u{00a0}')
}

fn trim_end_non_nbsp(text: &str) -> &str {
    text.trim_end_matches(|ch: char| ch.is_whitespace() && ch != '\u{00a0}')
}

fn escape_text(text: &str, mode: EscapeMode, opens_block_line: bool) -> String {
    let mut out = String::new();
    // A `^` is only dangerous where a caption marker could be read: at the
    // start of a line. Anywhere else it is literal text - superscript is
    // braced-only (`{^x^}`), so `10^6^` carries no markup - and forcing the
    // escape there put `10\^6\^` in the output where the other two engines
    // write `10^6^`. PART 11 §4 asks for the minimal form when dropping the
    // escape changes nothing, and this one changed nothing (carve-rs#555).
    //
    // Line-initial stays forced rather than left to the minimal/conservative
    // vote, because that vote is per DOCUMENT: letting `^ Figure 1` render
    // unescaped in the minimal pass makes it a caption, the two passes differ,
    // and the whole document escalates to conservative - which then escapes
    // every candidate in it, including the `:` that needs nothing. The corpus
    // pins that exact shape at 158-indented-image-and-caption-stay-literal.
    let mut at_line_start = opens_block_line;
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if matches!(ch, '\u{0000}'..='\u{0008}' | '\u{000b}'..='\u{001f}' | '\u{007f}'..='\u{009f}')
        {
            continue;
        }
        // The caption marker is `^` followed by a SPACE. `^sup^` at the start
        // of a line is not one - superscript is braced-only, so it is literal
        // text and needs no escape, which two of this repo's own tests already
        // pinned.
        let caret_opens_a_caption =
            ch == '^' && at_line_start && chars.peek().is_some_and(|c| *c == ' ' || *c == '\t');
        // A `:` opens something only where a marker can START: `:: term`,
        // `:  def` and `::: fence` are all recognized at the beginning of a
        // line, so the FIRST colon of that run is the one that has to be
        // escaped and the rest cannot open anything.
        //
        // The conservative pass used to escape every candidate character it
        // saw, so a literal `:::` came out `\:\:\:` where carve-js and
        // carve-php write `\:::`, and `\[x\]: /u` picked up an escape on a
        // colon that no rule can read (carve-rs#566). PART 11 §4 asks for the
        // minimal form when dropping the escape changes nothing, and it
        // changes nothing for every colon but the first.
        //
        // Same shape as the caret above: ask what the character could open
        // HERE, rather than escaping the class it belongs to.
        let colon_cannot_open = ch == ':' && !at_line_start;
        at_line_start = ch == '\n';
        let force_caption = caret_opens_a_caption && mode != EscapeMode::Minimal;
        let unconditional = matches!(ch, '\\' | '`' | '"' | '\'') || force_caption;
        let candidate = matches!(
            ch,
            '*' | '_'
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
                | '/'
                | '<'
                | '>'
                | '@'
                | '%'
                | '|'
                | '='
                | ':'
                | ';'
                | '^'
        );
        if unconditional || (mode == EscapeMode::Conservative && candidate && !colon_cannot_open) {
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

/// Which characters the destination scan would read differently if emitted
/// bare: a parenthesis with no partner, and a backslash sitting in front of one
/// of the three escapable characters. Balanced parentheses are deliberately
/// absent -- they re-parse as themselves, and escaping them would be churn
/// against the minimal-escaping rule in PART 11 section 4.
fn unbalanced_destination_chars(text: &str) -> std::collections::HashSet<usize> {
    let mut openers: Vec<usize> = Vec::new();
    let mut marked = std::collections::HashSet::new();
    for (i, ch) in text.char_indices() {
        if ch == '(' {
            openers.push(i);
        } else if ch == ')' && openers.pop().is_none() {
            marked.insert(i);
        }
    }
    marked.extend(openers);
    marked
}

fn escape_destination(text: &str) -> String {
    let sanitize_blank = dangerous_destination_scheme(text);
    // Almost every destination holds neither a parenthesis nor a backslash, so
    // there is nothing for the scan to misread and nothing to mark. Skipping
    // the walk keeps that case free of the set entirely.
    let needs_marking = text
        .as_bytes()
        .iter()
        .any(|&b| matches!(b, b'(' | b')' | b'\\'));
    let marked = if needs_marking {
        unbalanced_destination_chars(text)
    } else {
        std::collections::HashSet::new()
    };
    let bytes = text.as_bytes();
    let mut out = String::new();
    for (i, ch) in text.char_indices() {
        let escapable =
            ch == '\\' && matches!(bytes.get(i + 1), Some(b'(') | Some(b')') | Some(b'\\'));
        if (marked.contains(&i) || escapable) && !sanitize_blank {
            out.push('\\');
        }
        match ch {
            // Whitespace is percent-encoded (it would end the destination
            // otherwise). A backslash before anything the scan does not treat
            // as an escape is emitted verbatim, so URLs carrying backslashes
            // need no doubling.
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
        InlineNode::Text(text) => Some(&text.value),
        // The CHARACTER, not the backslash that precedes it in the output. A
        // text node holding `_b_` and an escaped-text node holding `_` describe
        // the same neighbour, and the writer has to brace an adjacent delimiter
        // the same way for both - otherwise the first pass (plain text) and the
        // second (escaped text) disagree and `fmt(fmt(x)) != fmt(x)`.
        InlineNode::EscapedText(text) => Some(&text.value),
        InlineNode::SmartPunctuation(s) => Some(&s.value),
        InlineNode::Code(text) => Some(&text.value),
        InlineNode::Abbreviation(abbr) => Some(&abbr.abbr),
        InlineNode::Mention(mention) => Some(&mention.user),
        InlineNode::Tag(tag) => Some(&tag.name),
        InlineNode::Symbol(symbol) => Some(&symbol.name),
        _ => None,
    }
}
