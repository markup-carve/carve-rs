use crate::ast::*;
use crate::extension::Options;
use crate::parse::unwrap_nested_anchors;
use crate::render_text::{strip_high_controls as strip_controls, trim_non_nbsp};

use crate::render::MAX_RENDER_DEPTH;

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
    static CROSSREF_INDEX: std::cell::RefCell<crate::parse::CrossrefIndex> =
        std::cell::RefCell::new(crate::parse::CrossrefIndex::default());
    /// Mode for the current render, carried the way `render_markdown` carries
    /// its own: set once per render entry point, read only by the
    /// smart-punctuation arm, off every signature in between.
    ///
    /// This target keeps its OWN cell rather than sharing the Markdown one. No
    /// entry point restores the previous value, so a shared cell would let a
    /// nested render of another target leave its mode behind in this one.
    static SMART_TYPOGRAPHY: std::cell::Cell<crate::extension::SmartTypographyMode> =
        const { std::cell::Cell::new(crate::extension::SmartTypographyMode::Glyph) };
    static LIST_DEPTH: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

fn footnote_is_defined(id: &str) -> bool {
    DEFINED_FOOTNOTES.with(|set| set.borrow().contains(id))
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

/// Render a document to plain text, honouring `Options::smart_typography`. See
/// `render_markdown_with_options` for why the options-taking wrapper exists;
/// the profile transform runs upstream.
/// Render a tree that did NOT come from the parser, refusing at the ceiling.
///
/// `Err` when the tree nests deeper than [`crate::MAX_RENDER_DEPTH`], naming the
/// renderer and the bound (PART 9 §25). A parser-produced tree cannot reach it -
/// the parse cap sits below the ceiling - so this fails only for a tree built
/// through the API or read by `from_json`, which is the caller who can act on it.
pub fn render_plain_text_with_options(
    doc: &Document,
    options: &Options<'_>,
) -> Result<String, crate::RenderDepthError> {
    let watch = crate::render_depth::RenderDepthWatch::new();
    watch.into_result(render_plain_text_inner(
        doc,
        options.smart_typography,
        options.lowercase_heading_ids,
    ))
}

/// Render a tree that did NOT come from the parser, refusing at the ceiling.
///
/// `Err` when the tree nests deeper than [`crate::MAX_RENDER_DEPTH`], naming the
/// renderer and the bound (PART 9 §25). A parser-produced tree cannot reach it -
/// the parse cap sits below the ceiling - so this fails only for a tree built
/// through the API or read by `from_json`, which is the caller who can act on it.
pub fn render_plain_text(doc: &Document) -> Result<String, crate::RenderDepthError> {
    let watch = crate::render_depth::RenderDepthWatch::new();
    watch.into_result(render_plain_text_inner(
        doc,
        crate::extension::SmartTypographyMode::Glyph,
        Options::default().lowercase_heading_ids,
    ))
}

fn render_plain_text_inner(
    doc: &Document,
    smart_typography: crate::extension::SmartTypographyMode,
    lowercase_heading_ids: bool,
) -> String {
    SMART_TYPOGRAPHY.with(|cell| cell.set(smart_typography));
    LIST_DEPTH.with(|cell| cell.set(0));
    // The plain target expands a crossref label exactly as the other three do,
    // so it needs the same bound; without a guard installed `try_spend` would
    // fall back to the floor budget and clip a large document earlier here than
    // in HTML (`markup-carve/carve-rs#805`).
    let _abbr_guard = crate::abbr_budget::AbbrBudgetGuard::for_document(doc);
    DEFINED_FOOTNOTES.with(|set| {
        *set.borrow_mut() = doc.footnote_defs.keys().cloned().collect();
    });
    CROSSREF_INDEX.with(|index| {
        *index.borrow_mut() = crate::parse::crossref_index_for_document(doc, lowercase_heading_ids);
    });
    let out = render_blocks(&doc.children, 0);
    let footnotes = render_footnote_defs(doc);
    normalize(&format!("{out}{footnotes}"))
}

fn render_crossref(target: &str, depth: usize) -> String {
    // The label is the target's cloned inline NODES (PART 9R R4), so the source
    // run survives to here and this renderer's own typography mode applies to
    // it. A caption target has no nodes - its label is LABEL + NUMBER - so that
    // one is still a string.
    let resolved = CROSSREF_INDEX.with(|index| {
        index
            .borrow()
            .resolve(target)
            .map(|(id, title)| (id.to_string(), title.to_string()))
    });
    let Some((id, title)) = resolved else {
        return format!("</#{}>", strip_controls(target));
    };
    let text = match CROSSREF_INDEX.with(|index| index.borrow().label(&id)) {
        Some(nodes) => render_inlines_stateful(&nodes, depth + 1),
        None => strip_controls(&title),
    };
    // Same expansion budget the abbreviation construct spends in the other
    // targets, degrading to the authored target (carve-rs#805). See
    // `crate::abbr_budget`.
    if crate::abbr_budget::try_spend(text.len()) {
        text
    } else {
        strip_controls(target)
    }
}

fn render_blocks(blocks: &[BlockNode], depth: usize) -> String {
    if depth > MAX_RENDER_DEPTH {
        crate::render_depth::record("plain");
        return String::new();
    }
    blocks
        .iter()
        .map(|block| render_block(block, depth))
        .collect()
}

fn render_block(node: &BlockNode, depth: usize) -> String {
    if depth > MAX_RENDER_DEPTH {
        crate::render_depth::record("plain");
        return String::new();
    }
    match node {
        // Renders nothing: a definition line is not prose.
        BlockNode::LinkReferenceDefinition(_) => String::new(),
        BlockNode::Heading(heading) => format!("{}\n\n", render_inlines(&heading.children)),
        BlockNode::Paragraph(paragraph) => {
            format!("{}\n\n", render_inlines(&paragraph.children))
        }
        BlockNode::CodeBlock(code) => format!("{}\n\n", strip_controls(&code.content)),
        BlockNode::BlockQuote(quote) => {
            let quoted = format!(
                "\"{}\"",
                trim_block_output(&render_blocks(&quote.children, depth + 1))
            );
            // Visible content, so a text target keeps it - as a separate block,
            // which is the spacing the renderer-parity fixtures pin.
            match &quote.attribution {
                Some(attribution) => {
                    format!("{quoted}\n\n{}\n\n", render_inlines(attribution))
                }
                None => format!("{quoted}\n\n"),
            }
        }
        BlockNode::List(list) => render_list(list, depth + 1),
        BlockNode::ThematicBreak(_) => "---\n\n".to_string(),
        BlockNode::Table(table) => render_table(table),
        BlockNode::Admonition(admonition) => {
            let body = render_blocks(&admonition.children, depth + 1);
            // The LABEL goes on first so the TITLE ends up above it, which is the
            // order the source writes them (`::: tip "Pro Tip" [Build]`) and the
            // order the HTML renderer emits. Prepending the title first and the
            // label second put the label above the title (carve#352, corpus
            // 42-admonitions-4).
            let body = prepend_label(body, admonition.label.as_deref());
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
        BlockNode::LineBlock(lb) => render_blocks(&lb.children, depth + 1),
        BlockNode::Div(div) => {
            let body = render_blocks(&div.children, depth + 1);
            prepend_label(body, div.label.as_deref())
        }
        BlockNode::DefinitionList(list) => render_definition_list(&list.items, true, depth + 1),
        BlockNode::Figure(figure) => render_figure(figure, depth + 1),
        // Terminate the block image so the next block is not glued onto it.
        BlockNode::BlockImage(image) => format!("{}\n\n", render_image(image)),
        BlockNode::Extension(extension) => render_blocks(&extension.children, depth + 1),
        // PART 10 §10a - see the note in render_markdown.
        BlockNode::AbbreviationDef(def) => format!(
            "*[{}]: {}\n\n",
            strip_controls(&def.abbr),
            strip_controls(&def.expansion)
        ),
        BlockNode::RawBlock(_) | BlockNode::Comment(_) => String::new(),
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
        crate::render_depth::record("plain");
        return String::new();
    }
    let mut out = String::new();
    let mut counter = node.start.unwrap_or(1);
    let list_depth = LIST_DEPTH.with(|cell| {
        let next = cell.get() + 1;
        cell.set(next);
        next
    });
    let indent = "  ".repeat(list_depth - 1);
    for item in &node.items {
        out.push_str(&indent);
        if node.ordered {
            out.push_str(&format!("{counter}. "));
            counter += 1;
        } else {
            out.push_str("- ");
        }
        let rendered = render_blocks(&item.children, depth + 1);
        let mut content = trim_block_output(&rendered).to_string();
        if node.tight {
            let nested_indent = "  ".repeat(list_depth);
            let ordered_prefix = |line: &str| {
                let digits = line.bytes().take_while(u8::is_ascii_digit).count();
                digits > 0
                    && line
                        .as_bytes()
                        .get(digits..digits + 2)
                        .is_some_and(|tail| matches!(tail, [b'.' | b')', b' ']))
            };
            let mut lines = content.split('\n').peekable();
            let mut compact = String::with_capacity(content.len());
            while let Some(line) = lines.next() {
                compact.push_str(line);
                if let Some(next) = lines.peek() {
                    let marker = next
                        .strip_prefix(&nested_indent)
                        .is_some_and(|tail| tail.starts_with("- ") || ordered_prefix(tail));
                    if !line.is_empty() || !marker {
                        compact.push('\n');
                    }
                }
            }
            content = compact;
        }
        out.push_str(&content);
        out.push('\n');
    }
    LIST_DEPTH.with(|cell| cell.set(cell.get() - 1));
    out.push('\n');
    out
}

fn render_definition_list(items: &[DefinitionItem], trailing_blank: bool, depth: usize) -> String {
    if depth > MAX_RENDER_DEPTH {
        crate::render_depth::record("plain");
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
                .map(|cell| trim_non_nbsp(&render_inlines(&cell.children)).to_string())
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
        crate::render_depth::record("plain");
        return String::new();
    }
    let target = match &node.target {
        FigureTarget::Image(image) => render_image(image),
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
    // SOURCE ORDER, not label order (§7; carve-rs#686). The map is a BTreeMap.
    for (label, blocks) in crate::ast_json::footnote_defs_in_source_order(doc) {
        // The MARKER AS WRITTEN (PART 10 §10a): `[n]: …` is a LINK reference
        // definition, so emitting one where the author wrote a footnote
        // definition turns it into a different construct on the way back.
        out.push_str(&format!(
            "[^{}]: {}\n",
            strip_controls(label),
            trim_non_nbsp(&render_blocks(blocks, 0))
        ));
    }
    out
}

fn render_inlines(nodes: &[InlineNode]) -> String {
    render_inlines_stateful(nodes, 0)
}

fn render_inlines_stateful(nodes: &[InlineNode], depth: usize) -> String {
    if depth > MAX_RENDER_DEPTH {
        crate::render_depth::record("plain");
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
        crate::render_depth::record("plain");
        return String::new();
    }
    match node {
        InlineNode::Text(text) => strip_controls(&text.value),
        InlineNode::EscapedText(text) => strip_controls(&text.value),
        InlineNode::SmartPunctuation(s) => strip_controls(smart_punctuation_text(s)),
        InlineNode::Emphasis(emphasis) => match emphasis.kind {
            EmphasisKind::Strike => render_inlines_stateful(&emphasis.children, depth + 1),
            _ => render_inlines_stateful(&emphasis.children, depth + 1),
        },
        InlineNode::Code(code) => strip_controls(&code.value),
        InlineNode::Link(link) => {
            if link.ref_label.is_some() && link.href.is_empty() {
                strip_controls(link.raw_ref.as_deref().unwrap_or_default())
            } else {
                // Render the label through the anchor-unwrapping view.
                let children = unwrap_nested_anchors(&link.children);
                render_inlines_stateful(children.as_ref(), depth + 1)
            }
        }
        InlineNode::Image(image) => render_image(image),
        InlineNode::Span(span) => {
            // An AUTHORED `abbr` is the one expansion this target has to print
            // inline. The automatic case does not need it: the
            // `*[TERM]: expansion` definition line is emitted verbatim, so the
            // mapping survives once at the definition rather than at every
            // occurrence. An authored value has NO definition line to carry it,
            // so dropping it loses the text outright - `[HTML]{abbr="Custom"}`
            // came out as bare `HTML` with "Custom" nowhere (carve#1176).
            //
            // Parentheses are already this target's idiom for an aside: an
            // inline footnote renders `(content)` here.
            //
            // No suppression flag is needed, unlike the Markdown and ANSI
            // targets: the `Abbreviation` arm below already prints the key
            // alone, so a resolved abbreviation inside the span contributes
            // only its visible text by construction (carve#1127).
            let inner = render_inlines_stateful(&span.children, depth + 1);
            let authored = span
                .attrs
                .as_ref()
                .and_then(|a| a.key_values.get("abbr"))
                .filter(|v| !v.is_empty());
            match authored {
                Some(value) if crate::abbr_budget::try_spend(value.len()) => {
                    format!("{inner} ({})", strip_controls(value))
                }
                _ => inner,
            }
        }
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
        InlineNode::SoftBreak(_) => " ".to_string(),
        InlineNode::HardBreak(_) => "\n".to_string(),
        InlineNode::CriticInsert(insert) => render_inlines_stateful(&insert.children, depth + 1),
        InlineNode::CriticDelete(delete) => {
            format!("~{}~", render_inlines_stateful(&delete.children, depth + 1))
        }
        InlineNode::CriticSubstitute(sub) => format!(
            "~{}~{}",
            strip_controls(&sub.old_text),
            strip_controls(&sub.new_text)
        ),
        // A critic comment is VISIBLE content: the HTML target renders it as
        // `<span class="critic-comment"> note </span>`, so dropping it here made
        // two targets of one engine disagree about whether the document says it.
        // carve-php kept it (carve#352, corpus 33-editorial-markup).
        InlineNode::Comment(_) => String::new(),
        InlineNode::CriticComment(c) => strip_controls(&c.text),
        InlineNode::CrossRef(crossref) => render_crossref(&crossref.target, depth),
        // Tier-2 ext node; the core renderer has no numbering, so emit the source.
        InlineNode::CitationGroup(group) => strip_controls(&group.raw),
        InlineNode::CaptionNumber(number) => number
            .number
            .map(|n| n.to_string())
            .unwrap_or_else(|| "#".to_string()),
    }
}

fn render_image(image: &Image) -> String {
    if image.ref_label.is_some() && image.src.is_empty() {
        strip_controls(image.raw_ref.as_deref().unwrap_or_default())
    } else {
        strip_controls(&image.alt)
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
    // The two document edges need different rules.
    //
    // At the START, trim only NEWLINES. Whitespace on the first content line is
    // data: a table row whose first cell is empty renders as ` | b`, and that space
    // IS the empty field -- eating it leaves a line that reads as a leading pipe and
    // splits into one field instead of two (carve#352, corpus
    // 96-table-span-marker-in-first-column and 09-tables-7). The generated-NBSP
    // placeholders carrying line-block and escaped-space indentation are excluded
    // for the same reason, and a leading TAB from a code block survived this before
    // only because the character class happened to omit it.
    //
    // At the END, trailing spaces go as before: there they are layout, not content.
    let trimmed = out.trim_start_matches('\n').trim_end_matches(['\n', ' ']);
    // A generated-NBSP placeholder (escaped space / verse indent) becomes a
    // plain space in display output; a LITERAL U+00A0 typed in the source is
    // preserved as-is. Only the HTML renderer folds both to `&nbsp;`.
    format!("{trimmed}\n").replace(crate::NBSP_PLACEHOLDER, " ")
}
