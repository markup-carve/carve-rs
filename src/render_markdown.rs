use crate::ast::*;
use crate::extension::Options;
use crate::parse::unwrap_nested_anchors;
use crate::render_text::{strip_high_controls as strip_control_chars, trim_non_nbsp};
use std::collections::HashMap;
use std::collections::HashSet;

use crate::render::MAX_RENDER_DEPTH;

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
/// Render a tree that did NOT come from the parser, refusing at the ceiling.
///
/// `Err` when the tree nests deeper than [`crate::MAX_RENDER_DEPTH`], naming the
/// renderer and the bound (PART 9 §25). A parser-produced tree cannot reach it -
/// the parse cap sits below the ceiling - so this fails only for a tree built
/// through the API or read by `from_json`, which is the caller who can act on it.
pub fn render_markdown_with_options(
    doc: &Document,
    options: &Options<'_>,
) -> Result<String, crate::RenderDepthError> {
    let watch = crate::render_depth::RenderDepthWatch::new();
    watch.into_result(render_markdown_inner(
        doc,
        options.smart_typography,
        options.lowercase_heading_ids,
    ))
}

/// Render a document to Markdown with the default settings, so smart
/// typography renders as its glyph.
/// Render a tree that did NOT come from the parser, refusing at the ceiling.
///
/// `Err` when the tree nests deeper than [`crate::MAX_RENDER_DEPTH`], naming the
/// renderer and the bound (PART 9 §25). A parser-produced tree cannot reach it -
/// the parse cap sits below the ceiling - so this fails only for a tree built
/// through the API or read by `from_json`, which is the caller who can act on it.
pub fn render_markdown(doc: &Document) -> Result<String, crate::RenderDepthError> {
    let watch = crate::render_depth::RenderDepthWatch::new();
    watch.into_result(render_markdown_inner(
        doc,
        crate::extension::SmartTypographyMode::Glyph,
        Options::default().lowercase_heading_ids,
    ))
}

fn render_markdown_inner(
    doc: &Document,
    smart_typography: crate::extension::SmartTypographyMode,
    lowercase_heading_ids: bool,
) -> String {
    SMART_TYPOGRAPHY.with(|cell| cell.set(smart_typography));
    let _abbr_guard = crate::abbr_budget::AbbrBudgetGuard::for_document(doc);
    let mut heading_ids = HashSet::new();
    let mut referenced_heading_ids = HashSet::new();
    let crossref_index = crate::parse::crossref_index_for_document(doc, lowercase_heading_ids);
    // Footnote definition bodies are rendered as block content too, so their
    // headings and crossref links must be part of the heading-id / referenced-id
    // prepass; otherwise a heading referenced only from a footnote loses the
    // `{#id}` suffix needed to keep the link valid on reparse.
    // Ids are assigned with the SAME duplicate disambiguation the core uses, not
    // re-slugged per heading. Two headings reading `Setup` are `Setup` and
    // `Setup-2`; deriving the slug alone gave both `Setup`, so a reference to
    // `Setup-2` matched no heading - it lost its `{#id}` suffix here AND was
    // degraded to bare text by `render_link`, which drops a fragment link whose
    // target it does not know about (carve#352).
    let mut explicit_ids = HashSet::new();
    let mut explicit_pass = |block: &BlockNode, _: Option<&[InlineNode]>| {
        if let BlockNode::Heading(heading) = block {
            if let Some(id) = heading.attrs.as_ref().and_then(|attrs| attrs.id.as_ref()) {
                explicit_ids.insert(id.clone());
            }
        }
    };
    walk_blocks(&doc.children, 0, &mut explicit_pass);
    for body in doc.footnote_defs.values() {
        walk_blocks(body, 0, &mut explicit_pass);
    }

    let mut id_counts: HashMap<String, usize> = HashMap::new();
    let mut heading_pass = |block: &BlockNode, _: Option<&[InlineNode]>| {
        if let BlockNode::Heading(heading) = block {
            heading_ids.insert(next_heading_id(heading, &mut id_counts, &explicit_ids));
        }
    };
    walk_blocks(&doc.children, 0, &mut heading_pass);
    for body in doc.footnote_defs.values() {
        walk_blocks(body, 0, &mut heading_pass);
    }
    let mut ref_pass = |_: &BlockNode, inlines: Option<&[InlineNode]>| {
        if let Some(inlines) = inlines {
            walk_inlines(inlines, 0, false, &mut |node, in_link| {
                // A reference inside a link label is flattened to text by the
                // renderer, so it does not link anywhere and must not keep the
                // target heading's `{#id}` suffix alive: `# H {#H}` is not
                // Markdown, and with the reference gone the suffix anchors
                // nothing (carve-rs#436).
                if in_link {
                    return;
                }
                if let InlineNode::Link(link) = node {
                    if let Some(id) = fragment_id(&link.href) {
                        if heading_ids.contains(id) {
                            referenced_heading_ids.insert(id.to_string());
                        }
                    }
                } else if let InlineNode::CrossRef(crossref) = node {
                    if let Some((id, _)) = crossref_index.resolve(&crossref.target) {
                        referenced_heading_ids.insert(id.to_string());
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
        suppress_automatic_abbreviation: false,
        heading_ids,
        referenced_heading_ids,
        explicit_ids,
        // Rewound, because rendering walks the same headings in the same order
        // and has to reproduce the same sequence of ids.
        id_counts: HashMap::new(),
        list_depth: 0,
        defined_footnotes: doc.footnote_defs.keys().cloned().collect(),
        crossref_index,
        link_depth: 0,
    };
    let out = render_blocks(&doc.children, &mut ctx, 0);
    let footnotes = render_footnote_defs(doc, &mut ctx);
    normalize(&format!("{out}{footnotes}"))
}

struct MarkdownContext {
    /// Set while rendering a span that carries an authored `abbr`.
    ///
    /// PART 9 section 10 and carve#1127: the authored value OUTRANKS automatic
    /// expansion, and a resolved abbreviation inside such a span contributes only
    /// its visible text. The HTML renderer already carried this flag on its state;
    /// this target emitted the DEFINITION's text instead (carve#1176).
    suppress_automatic_abbreviation: bool,
    heading_ids: HashSet<String>,
    referenced_heading_ids: HashSet<String>,
    explicit_ids: HashSet<String>,
    id_counts: HashMap<String, usize>,
    list_depth: usize,
    /// Labels that actually have a definition. A reference without one did not
    /// form a footnote, so it is not a footnote marker. The HTML renderer decides
    /// this on the node's `number`, which numbering assigns -- this target does no
    /// numbering, so that field is always None here and there was nothing to
    /// check (carve#352).
    defined_footnotes: std::collections::BTreeSet<String>,
    crossref_index: crate::parse::CrossrefIndex,
    /// Nonzero while rendering a link's label. Links never nest, and the parser
    /// enforces that -- but a cross-reference becomes a link only HERE, after
    /// the parser has run, so this target has to apply the rule itself.
    /// `[see </#H>](/outer)` must render as `[see H](/outer)`, not as a link
    /// inside a link, which is not valid Markdown (carve-rs#436).
    link_depth: usize,
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
        crate::render_depth::record("markdown");
        return String::new();
    }
    blocks
        .iter()
        .map(|block| render_block(block, ctx, depth))
        .collect()
}

fn render_block(node: &BlockNode, ctx: &mut MarkdownContext, depth: usize) -> String {
    if depth > MAX_RENDER_DEPTH {
        crate::render_depth::record("markdown");
        return String::new();
    }
    match node {
        // Renders nothing, same as carve-js and carve-php on this target.
        BlockNode::LinkReferenceDefinition(_) => String::new(),
        BlockNode::Heading(heading) => {
            let id = next_heading_id(heading, &mut ctx.id_counts, &ctx.explicit_ids);
            let text = flatten_heading_text(&render_block_inlines(&heading.children, ctx));
            let mut suffix = String::new();
            if ctx.referenced_heading_ids.contains(&id) {
                suffix = format!(" {{#{id}}}");
            }
            format!("{} {text}{suffix}\n\n", "#".repeat(heading.level as usize))
        }
        BlockNode::Paragraph(paragraph) => {
            format!(
                "{}\n\n",
                protect_paragraph_list_markers(&render_block_inlines(&paragraph.children, ctx))
            )
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
            // The EFFECTIVE title, not the authored header. An attribute line above
            // the fence overrides a title written in the header, and the HTML
            // target uses the winner -- so emitting `code.title` here described the
            // document differently in the two targets, announcing a title that had
            // lost (carve#352, corpus 11-fenced-code-10). The parser resolves the
            // override into `attrs`, so that is where the answer already is.
            let effective_title = code
                .attrs
                .as_ref()
                .and_then(|attrs| attrs.key_values.get("title"))
                .or(code.title.as_ref());
            // A title needs a LANGUAGE in front of it. In Markdown the info
            // string's first token IS the language, so `` ``` "notes.txt" ``
            // makes a CommonMark reader emit
            // `class="language-&quot;notes.txt&quot;"` -- measured against
            // commonmark.js. Markdown cannot express a fence title on its own, so
            // dropping it beats emitting a bogus language; with a language present
            // the title is ignored by every consumer and rides along safely.
            // carve-php had this guard and was right about it (carve#352, corpus
            // 11-fenced-code-8).
            if let Some(title) = effective_title.filter(|_| !info.is_empty()) {
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
            let mut body = trim_block_output(&lines).to_string();
            // PART 11 §10c T1. The attribution is the quotation's SOURCE, so it
            // stays INSIDE the quote. It used to follow as a sibling paragraph,
            // which kept the words but not what they mean - read back it was
            // attached to nothing, and a round trip produced a blockquote with
            // no attribution.
            //
            // Markdown has no attribution syntax but does admit HTML, and this
            // target already writes <u>, <mark>, <sub>, <ins> and <del> for
            // constructs with no Markdown spelling. Through a CommonMark reader
            // <footer> opens an HTML BLOCK inside the quote (it is not wrapped
            // in a paragraph), so the rendered HTML matches the HTML target's.
            if let Some(attribution) = &quote.attribution {
                let text = render_inlines(attribution, ctx, depth + 1);
                body.push_str(&format!("\n\n<footer>{}</footer>", text.trim()));
            }
            let quoted = body
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
            format!("{quoted}\n\n")
        }
        BlockNode::List(list) => render_list(list, ctx, depth + 1),
        BlockNode::ThematicBreak(_) => "---\n\n".to_string(),
        BlockNode::Table(table) => render_table(table, ctx),
        BlockNode::Admonition(admonition) => {
            // Markdown has no admonition; preserve the title (otherwise lost)
            // as a leading bold line, then the body.
            let body = render_blocks(&admonition.children, ctx, depth + 1);
            // The LABEL goes on first so the TITLE ends up above it, which is the
            // order the source writes them (`::: tip "Pro Tip" [Build]`) and the
            // order the HTML renderer emits (carve#352, corpus 42-admonitions-4).
            let body = prepend_label(body, admonition.label.as_deref());
            match &admonition.title {
                Some(title) => {
                    let t = render_title_inlines(title, ctx);
                    if t.is_empty() {
                        body
                    } else {
                        format!("**{t}**\n\n{body}")
                    }
                }
                None => body,
            }
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
        // PART 10 §10a: a definition NOTHING references still reaches this
        // target. HTML drops it because it has nowhere to put one; Markdown,
        // plain text and the terminal do not get to drop content the author
        // wrote, and dropping it made the output depend on whether a reference
        // exists elsewhere in the document (carve#589).
        // The definition line goes through `escape_md_html` for the same reason
        // the `<abbr>` built from it does: an expansion is author content, and
        // this target's contract is that embedded HTML cannot become live
        // markup downstream. Writing the occurrence escaped and the definition
        // raw made one output disagree with itself (carve-rs#807).
        BlockNode::AbbreviationDef(def) => format!(
            "*[{}]: {}\n\n",
            escape_md_html(&strip_controls(&def.abbr)),
            escape_md_html(&strip_controls(&def.expansion))
        ),
        BlockNode::Comment(_) => String::new(),
    }
}

/// Keep paragraph continuation lines from becoming lists in Markdown readers.
fn protect_paragraph_list_markers(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut code_fence = 0usize;
    for (line_index, source_line) in text.split('\n').enumerate() {
        if line_index > 0 {
            output.push('\n');
        }
        let mut line = source_line.to_string();
        if code_fence == 0 {
            let bytes = line.as_bytes();
            let mut marker = 0usize;
            while marker < bytes.len() && marker < 3 && matches!(bytes[marker], b' ' | b'\t') {
                marker += 1;
            }
            let insert_at = if marker + 1 < bytes.len()
                && matches!(bytes[marker], b'-' | b'+')
                && matches!(bytes[marker + 1], b' ' | b'\t')
            {
                Some(marker)
            } else {
                let digit_start = marker;
                while marker < bytes.len()
                    && marker - digit_start < 9
                    && bytes[marker].is_ascii_digit()
                {
                    marker += 1;
                }
                if marker > digit_start
                    && marker + 1 < bytes.len()
                    && matches!(bytes[marker], b'.' | b')')
                    && matches!(bytes[marker + 1], b' ' | b'\t')
                {
                    Some(marker)
                } else {
                    None
                }
            };
            if let Some(at) = insert_at {
                line.insert(at, '\\');
            }
        }

        let bytes = line.as_bytes();
        let mut i = 0usize;
        while i < bytes.len() {
            if bytes[i] != b'`' {
                i += 1;
                continue;
            }
            let mut backslashes = 0usize;
            let mut before = i;
            while before > 0 && bytes[before - 1] == b'\\' {
                backslashes += 1;
                before -= 1;
            }
            let mut run = 1usize;
            while i + run < bytes.len() && bytes[i + run] == b'`' {
                run += 1;
            }
            if backslashes % 2 == 0 {
                if code_fence == 0 {
                    code_fence = run;
                } else if code_fence == run {
                    code_fence = 0;
                }
            }
            i += run;
        }
        output.push_str(&line);
    }
    output
}

fn render_list(node: &List, ctx: &mut MarkdownContext, depth: usize) -> String {
    if depth > MAX_RENDER_DEPTH {
        crate::render_depth::record("markdown");
        return String::new();
    }
    ctx.list_depth += 1;
    let mut out = String::new();
    let mut counter = node.start.unwrap_or(1);
    // The authored bullet, not a normalized one. A change of bullet is what
    // SEPARATES two adjacent lists in CommonMark, so emitting `-` for a `*` list
    // merges lists the source kept apart -- the same section 11 rule the AST
    // records `bullet_char` for and render_carve already honors (carve#352).
    let bullet = node.bullet_char.unwrap_or('-');
    // The authored ordered-list delimiter, for the same reason as the bullet
    // above: in CommonMark a change of delimiter SEPARATES two adjacent lists, so
    // emitting `1.` for a `1)` list merges lists the source kept apart. Measured
    // against commonmark.js -- `1. a` followed by `1) c` gives two `<ol>`
    // elements, the same input with one delimiter gives one. The AST records
    // `delim` and render_carve already reproduces it (carve#352, corpus 31).
    let delim = if node.delim == Some(')') { ')' } else { '.' };
    for item in &node.items {
        let prefix = if node.ordered {
            let prefix = format!("{counter}{delim} ");
            counter += 1;
            prefix
        } else if let Some(checked) = item.checked {
            if checked {
                format!("{bullet} [x] ")
            } else {
                format!("{bullet} [ ] ")
            }
        } else {
            format!("{bullet} ")
        };
        let content = trim_block_output(&render_blocks(&item.children, ctx, depth + 1)).to_string();
        let mut lines = content.split('\n');
        // NESTING COMES FROM THE PARENT'S CONTINUATION PAD ALONE. This used to
        // add `"  ".repeat(list_depth - 1)` as well, and the enclosing item then
        // padded the same lines again by its marker width, so every level was
        // indented twice: two levels landed at four spaces and three at ten.
        // Ten spaces under a marker whose content column is six is four PAST
        // it, which is where a reader opens an indented verbatim block -- so a
        // third level stopped being a list for every reader that is not Carve
        // itself. Carve's own content-column model is lenient enough to read it
        // back as a list, which is why this was invisible from inside the
        // engine and only pandoc showed it (carve#1069, carve-php#1142).
        out.push_str(&format!("{prefix}{}\n", lines.next().unwrap_or_default()));
        let continuation = " ".repeat(prefix.len());
        for line in lines {
            // A line with no content takes no pad: PART 11 section 7 emits such
            // a line empty, and trailing whitespace is what editors and
            // `git apply --whitespace=fix` rewrite behind the writer.
            if line.is_empty() {
                out.push('\n');
            } else {
                out.push_str(&format!("{continuation}{line}\n"));
            }
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
        crate::render_depth::record("markdown");
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
    let mut header_columns = 0usize;
    let mut rows = Vec::new();
    // Per-column alignment for the separator row, which is the only place Markdown
    // can express it.
    //
    // COLUMN alignment is declared on the HEADER cells -- that is where `|=> Age`
    // puts it, and the HTML renderer applies it to every cell in the column. This
    // used to read the first NON-header row, where `align` is set only by a
    // per-CELL override, so an ordinary aligned table lost its alignment outright
    // and a table with one overridden cell reported that cell's alignment as the
    // whole column's (carve#352, corpus 48/49/52/53).
    //
    // A per-cell override cannot be expressed in a Markdown table at all, so it is
    // deliberately not consulted here; the column keeps what the header declared.
    let mut aligns: Vec<Option<TableAlign>> = Vec::new();
    let take_aligns = |aligns: &mut Vec<Option<TableAlign>>, row: &TableRow| {
        for (i, cell) in row.cells.iter().enumerate() {
            if aligns.len() <= i {
                aligns.resize(i + 1, None);
            }
            if aligns[i].is_none() {
                aligns[i] = cell.align;
            }
        }
    };
    for row in &node.rows {
        let cells = row
            .cells
            .iter()
            .map(|cell| trim_non_nbsp(&render_block_inlines(&cell.children, ctx)).to_string())
            .collect::<Vec<_>>();
        let rendered = format!("| {} |", cells.join(" | "));
        if row.cells.iter().all(|cell| cell.header) {
            header = Some(rendered);
            header_columns = cells.len();
            take_aligns(&mut aligns, row);
        } else {
            rows.push(rendered);
            // A headerless table still declares its columns somewhere, so fall
            // back to the first row that carries an alignment.
            if header.is_none() {
                take_aligns(&mut aligns, row);
            }
        }
    }
    let mut out = String::new();
    if let Some(header) = header {
        out.push_str(&header);
        out.push('\n');
        // The delimiter promotes the header row, so its width must match that
        // row rather than a wider body row. Otherwise common Markdown readers
        // reject the whole table (carve#1042, PART 11 §10b).
        let sep = (0..header_columns)
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
    out.push('\n');
    // A caption is authored text, and Markdown has no table-caption syntax - so
    // it goes on its own line under the table rather than being dropped.
    // Dropping it was the only place a presentation target discarded authored
    // text outright, against the MUST in docs/graceful-degradation.md ("losing
    // the click is fine; losing the words is not"). An image and a listing
    // caption already degrade exactly this way, so the table stops being the odd
    // one out. Ported from carve-js#1044.
    if let Some(caption) = &node.caption {
        let text = render_inlines(caption, ctx, 0);
        let text = text.trim();
        if !text.is_empty() {
            out.push_str(text);
            out.push('\n');
        }
    }
    out.push('\n');
    out
}

fn render_figure(node: &Figure, ctx: &mut MarkdownContext, depth: usize) -> String {
    if depth > MAX_RENDER_DEPTH {
        crate::render_depth::record("markdown");
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
    // SOURCE ORDER, not label order (§7; carve-rs#686). The map is a BTreeMap.
    for (label, blocks) in crate::ast_json::footnote_defs_in_source_order(doc) {
        out.push_str(&format!(
            "[^{}]: {}\n",
            // A label is author content, and it is reproduced verbatim in two
            // places; both escape, so a reference still matches its definition
            // (carve-rs#807).
            escape_md_html(&strip_controls(label)),
            trim_non_nbsp(&render_blocks(blocks, ctx, 0))
        ));
    }
    out
}

fn render_inlines(nodes: &[InlineNode], ctx: &mut MarkdownContext, depth: usize) -> String {
    if depth > MAX_RENDER_DEPTH {
        crate::render_depth::record("markdown");
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
        crate::render_depth::record("markdown");
        return String::new();
    }
    match node {
        // Reproduce the author's escape. `\-\-` was written precisely so a
        // downstream processor with smart punctuation on would not read an en
        // dash; emitting the character bare loses exactly that (carve issue
        // 350).
        //
        // NO SENTINEL HERE, and PART 11 §8a M2 says why: M1b is a rule about a
        // character that reached this writer inside a TEXT node - one the Carve
        // grammar did not read as an opener and the author did not mark. This
        // is the other case. The author said which reading they meant, M2 gives
        // it back whatever the character, and the line test never sees it. The
        // underscore used to take the sentinel here and lose its backslash to
        // the intraword rule, which was M1b deciding a node M1 never governed.
        InlineNode::EscapedText(text) => format!("\\{}", text.value),
        InlineNode::Text(text) => {
            if is_literal_crossref(&text.value) {
                strip_controls(&text.value)
            } else {
                // The generated-NBSP placeholder (escaped space `\ ` / verse
                // indent) round-trips to a literal non-breaking space in
                // Markdown, matching the other renderers' source projection.
                escape_text(
                    &strip_controls(&text.value).replace(crate::NBSP_PLACEHOLDER, "\u{00a0}"),
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
        InlineNode::Code(code) => render_code(&strip_controls(&code.value)),
        InlineNode::Link(link) => render_link(link, ctx, depth + 1),
        InlineNode::Image(image) => render_image(image),
        InlineNode::Span(span) => {
            // A span has no Markdown spelling, so its content is rendered bare -
            // EXCEPT for an authored `abbr`, which outranks the document
            // definition (carve#1127). This target can carry a title, since it
            // already emits an `<abbr>` for an ordinary expansion, so it carries
            // the AUTHORED one (carve#1176).
            let authored = span
                .attrs
                .as_ref()
                .and_then(|a| a.key_values.get("abbr"))
                .cloned();
            let Some(authored) = authored else {
                return render_inlines(&span.children, ctx, depth + 1);
            };
            let previous = ctx.suppress_automatic_abbreviation;
            ctx.suppress_automatic_abbreviation = true;
            let inner = render_inlines(&span.children, ctx, depth + 1);
            ctx.suppress_automatic_abbreviation = previous;
            if authored.is_empty() || !crate::abbr_budget::try_spend(authored.len()) {
                return inner;
            }
            let title = escape_md_html(&strip_controls(&authored)).replace('"', "&quot;");
            format!("<abbr title=\"{title}\">{inner}</abbr>")
        }
        InlineNode::Math(math) => {
            // Escaped, exactly as the HTML target escapes the same content: a
            // consumer decodes the entity back to the character before its math
            // renderer sees it, so `a < b` still reaches KaTeX as `a < b` while
            // `<script>` cannot become a tag (carve-rs#807).
            let content = escape_md_html(&strip_controls(&math.content));
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
            let text = escape_md_html(&strip_controls(&abbr.abbr));
            // Inside a span carrying its own `abbr`, only the visible text
            // (carve#1127).
            if ctx.suppress_automatic_abbreviation {
                return text;
            }
            // Bound cumulative expansion bytes (memory-amplification DoS): once
            // the budget is exhausted, degrade to plain key text with no title.
            if crate::abbr_budget::try_spend(abbr.expansion.len()) {
                // The attribute context needs the quote too; the other three
                // characters come from the one helper.
                let title = escape_md_html(&strip_controls(&abbr.expansion)).replace('"', "&quot;");
                format!("<abbr title=\"{title}\">{text}</abbr>")
            } else {
                text
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
                format!("^[{rendered}]")
            } else {
                let id = strip_controls(footnote.id.as_deref().unwrap_or(""));
                if ctx.defined_footnotes.contains(&id) {
                    // Escaped like the definition above, so the pair still
                    // matches, and like the unresolved branch below, which
                    // already escapes -- that one was deciding the question for
                    // brackets and skipping it for HTML (carve-rs#807).
                    format!("[^{}]", escape_md_html(&id))
                } else {
                    // UNRESOLVED: ordinary text, and its brackets are Markdown
                    // metacharacters that PART 11 section 8 M1 escapes
                    // UNCONDITIONALLY. Bare, they hand the re-parser markup the
                    // document never had. The label between them is author
                    // content and gets the HTML pass for the same reason: this
                    // branch was deciding the question for brackets and
                    // skipping it for `<` (raised by codex review).
                    format!("\\[^{}\\]", escape_md_html(&id))
                }
            }
        }
        InlineNode::SoftBreak(_) => "\n".to_string(),
        // A BACKSLASH, not two trailing spaces (PART 11 section 9). Both mean
        // `<br />` to a CommonMark reader, but trailing whitespace is removed by
        // editors that strip on save, by `git apply --whitespace=fix` and by CI
        // whitespace checks -- and losing ONE of the two spaces is enough for the
        // break to vanish rather than degrade, silently, in a file nobody edited.
        InlineNode::HardBreak(_) => "\\\n".to_string(),
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
        // Visible content: the HTML target renders it as
        // `<span class="critic-comment"> note </span>`, so dropping it here made two
        // targets of one engine disagree about whether the document says it. Markdown
        // has no critic syntax, so the text is what degrades gracefully -- escaped
        // like any other text, since a comment carrying Markdown metacharacters must
        // not become live markup when the output is re-rendered. carve-php kept it
        // (carve#352, corpus 33-editorial-markup); plain and ANSI were fixed in
        // carve-rs#322.
        InlineNode::Comment(_) => String::new(),
        InlineNode::CriticComment(c) => escape_text(&strip_controls(&c.text)),
        // A RESOLVED cross-reference is a link to that target, so it renders as
        // one - `[Title](#id)` - under exactly the condition `render_link` uses:
        // only when this target will EMIT that id. A Markdown heading carries
        // `{#id}`; an image or a table does not, so a reference to a numbered
        // figure would produce a link to a fragment that is not in the output.
        // There the title alone is the honest rendering, which is what the
        // `numbered-figure-crossref` fixture pins.
        //
        // Emitting only the title in BOTH cases would silently drop every
        // heading link from the Markdown export - the mistake this arm was first
        // written with.
        InlineNode::CrossRef(crossref) => {
            // The label is the target's cloned inline NODES (PART 9R R4), so it
            // is written back out as Markdown markup rather than as flattened
            // text, and the source run reaches this renderer's typography mode.
            // A caption target has no nodes - its label is LABEL + NUMBER - so
            // that one is still a string.
            let resolved = ctx
                .crossref_index
                .resolve(&crossref.target)
                .map(|(id, title)| (id.to_string(), title.to_string()));
            match resolved {
                // UNRESOLVED: the authored marker, kept readable rather than
                // escaped into noise - a reader can still act on `</#nope>`.
                // The TARGET inside it is author content and can hold a `<`,
                // and `</#a<script>` is a complete opening tag once this
                // Markdown is rendered, so the target takes the HTML pass while
                // the writer's own delimiters stay literal (carve-rs#807).
                None => format!("</#{}>", escape_md_html(&strip_controls(&crossref.target))),
                Some((id, title)) => {
                    let label = ctx.crossref_index.label(&id);
                    let text = match &label {
                        Some(nodes) => render_inlines(nodes, ctx, depth + 1),
                        None => escape_text(&strip_controls(&title)),
                    };
                    // Same expansion budget the abbreviation arm below spends,
                    // degrading to the authored target (carve-rs#805). See
                    // `crate::abbr_budget`.
                    let text = if crate::abbr_budget::try_spend(text.len()) {
                        text
                    } else {
                        escape_text(&strip_controls(&crossref.target))
                    };
                    // Inside a link label the reference is already surrounded by
                    // an anchor, so it degrades to its display text -- the same
                    // rule the parser applies to every link it produces itself,
                    // and the HTML target applies to this node (carve-rs#436).
                    if ctx.link_depth == 0 && ctx.heading_ids.contains(&id) {
                        format!("[{text}](#{})", strip_controls(&id))
                    } else {
                        text
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

fn render_link(node: &Link, ctx: &mut MarkdownContext, depth: usize) -> String {
    if node.ref_label.is_some() && node.href.is_empty() {
        return escape_text(&strip_controls(node.raw_ref.as_deref().unwrap_or_default()));
    }
    ctx.link_depth += 1;
    // Render the label through the anchor-unwrapping view.
    let children = unwrap_nested_anchors(&node.children);
    let text = render_inlines(children.as_ref(), ctx, depth);
    ctx.link_depth -= 1;
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
    if node.ref_label.is_some() && node.src.is_empty() {
        return escape_text(&strip_controls(node.raw_ref.as_deref().unwrap_or_default()));
    }
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
    // PEEKABLE, because M1e below decides on the NEXT character.
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            // Neutralize embedded HTML so Markdown re-rendered to HTML cannot
            // execute it (carve's "HTML is text" guarantee for the Markdown
            // target too): a literal `<img onerror=…>` becomes inert.
            //
            // ONLY `<` AND `>` DO THAT WORK. A bare `&` cannot open a tag: an
            // entity in Markdown TEXT decodes to a CHARACTER, and a character in
            // text content is escaped again by whatever writes the HTML.
            // Measured against pandoc 3.5, commonmark.js and marked with raw
            // HTML ALLOWED - the entity and bare forms came out byte-identical
            // and inert, while a bare `<` was live in all three. Escaping every
            // ampersand cost every document its spelling for nothing
            // (carve#1071).
            //
            // NO EXCEPTION FOR A CHARACTER-REFERENCE OPENER, deliberately: text
            // authored as `&#65;` is emitted as itself. Whether an `&` opens a
            // reference depends on the EMITTED LINE, and Carve parses `#65` as a
            // tag, so this renderer sees two separate text nodes - answering it
            // here would be one node too early, the mistake section 8a
            // documents for `_`, `#` and `[`.
            // PART 11 section 8a M1e: a `<` is escaped only where the emitted
            // line would read it as markup - before an ASCII letter, `/`, `!`
            // or `?`, the four things that open raw HTML. Everything else is
            // inert, and so is `>` mid-line; at line start `>` is a block quote
            // marker M1 already covers.
            //
            // A BACKSLASH, not an entity. Both entities were written
            // unconditionally with no clause behind them (carve#1148), and that
            // is precisely because an entity is not the operation this section
            // describes: M2 and M3 protect a character so it survives as
            // itself, and an entity replaces it instead. Escaping the `<` alone
            // suffices - a tag that cannot open cannot be closed.
            '<' => {
                let opens_markup = matches!(
                    chars.peek(),
                    Some(n) if n.is_ascii_alphabetic() || matches!(n, '/' | '!' | '?')
                );
                if opens_markup {
                    out.push('\\');
                }
                out.push('<');
                continue;
            }
            // `_`, `#` and `[` are emitted as SENTINELS rather than as
            // backslashes: PART 11 §8a decides those three on the EMITTED LINE,
            // which only `resolve_narrowed_escapes` can see. See
            // `narrowed_sentinel`.
            '_' | '#' | '[' => {
                out.push(narrowed_sentinel(ch));
                continue;
            }
            // Markdown metacharacters. The ASTERISK keeps M1 unconditionally
            // (§8a M1a): this writer spells emphasis with `*`, so a literal
            // asterisk is not a character that MIGHT meet markup on the line -
            // it is the character the line's markup is made of. `*\*\**`
            // unescaped to `****`, which a CommonMark reader publishes as a
            // thematic break rather than as emphasis holding two asterisks.
            // `]` and the rest are M1c: nothing else narrows.
            '\\' | '`' | '*' | ']' => out.push('\\'),
            _ => {}
        }
        out.push(ch);
    }
    out
}

/// Sentinels standing in for the escapes PART 11 §8a decides on the LINE.
///
/// One per narrowed character. U+E000 is the NBSP sentinel and the Carve writer
/// claims U+E001..U+E003; this extends the scheme. Author content never carries
/// one: `strip_controls` drops the whole range on the way in, and every path to
/// the output runs through it.
const SENTINEL_FIRST: char = '\u{E004}';
const SENTINEL_LAST: char = '\u{E006}';

/// The sentinel for a narrowed character (`_`, `#`, `[`).
fn narrowed_sentinel(ch: char) -> char {
    match ch {
        '_' => '\u{E004}',
        '#' => '\u{E005}',
        '[' => '\u{E006}',
        other => other,
    }
}

/// The character a narrowed sentinel stands for, or `None` for anything else.
fn narrowed_character(ch: char) -> Option<char> {
    match ch {
        '\u{E004}' => Some('_'),
        '\u{E005}' => Some('#'),
        '\u{E006}' => Some('['),
        _ => None,
    }
}

/// Drop control characters from author content, and the §8a sentinels with them:
/// author content that carried one would otherwise reach
/// `resolve_narrowed_escapes` and be read as an escape this renderer emitted.
/// Every path to the output passes here.
///
/// THE CONTROL HALF STAYS AS BROAD AS IT WAS. It is `strip_control_chars`, which
/// is every `Cc` character bar tab and newline, NOT the non-whitespace C0 class:
/// DEL (U+007F) and the C1 controls have to keep going, because CSI (U+009B) and
/// OSC (U+009D) are single-character forms of the sequences PART 9 §25's terminal
/// rule exists to stop. Narrowing this guard is a security regression, and the
/// test suite pins it rather than leaving it to this comment.
fn strip_controls(input: &str) -> String {
    let sentinels_gone: String = input
        .chars()
        .filter(|c| !(SENTINEL_FIRST..=SENTINEL_LAST).contains(c))
        .collect();
    strip_control_chars(&sentinels_gone)
}

/// Escape `<>&` so embedded raw HTML cannot become live markup downstream.
fn escape_md_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Blank a URL whose (normalized) scheme is on the dangerous denylist, so it does
/// not survive into Markdown output and from there into whatever renders it.
///
/// The set and the probe filter come from `escape`, not restated here. A local copy
/// listed only `javascript`, `vbscript`, `data` and `file`, and filtered with an
/// ASCII-only test -- so the twenty OS protocol-handler schemes (`ms-msdt`,
/// `search-ms`, `shell`, `vscode`, `jar`, ...) reached the output while the HTML
/// renderer blanked them. A Markdown destination is resolved by the renderer
/// downstream, so that is the same sink one step removed (PART 9 section 25,
/// markup-carve/carve#385).
fn sanitize_md_url(url: &str) -> String {
    // The PROBE comes from `escape` too, not only the scheme set. The body used
    // to be restated here and had already drifted: it dropped the non-empty
    // prefix guard its original has, so `:x` was read as a scheme with an empty
    // name. One copy cannot drift from itself.
    crate::escape::sanitize_url(url).into_owned()
}

/// Encode a destination for the Markdown output, refusing a denied scheme.
///
/// The order is the whole point. This writer NORMALIZES the destination before
/// it emits it - it drops control characters, and its consumer decodes character
/// references - so the probe has to run on the normalized form. Probing the
/// authored form and normalizing afterwards means the writer itself
/// manufactures the live URL out of one the probe had already dismissed
/// (`markup-carve/carve-rs#806`).
fn encode_markdown_destination(url: &str) -> String {
    // 1. Strip first, probe second. The strip drops all of `\p{Cc}`, the probe
    //    skips only up to U+0020 plus whitespace, so `java<DEL>script:` and the
    //    C1 range walked straight through and came out clean on the far side.
    //    The ANSI target of this same engine already strips before it probes
    //    (`render_ansi.rs`), and carve-php strips inside its probe.
    let sanitized = sanitize_md_url(&strip_controls(url));
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
    // 2. Neutralize character references, so the bytes the consumer resolves are
    //    the bytes probed in step 1.
    neutralize_char_refs(&out)
}

/// Escape every ampersand that OPENS an HTML character reference.
///
/// A CommonMark consumer decodes character references inside a link
/// destination, so `&#106;avascript:alert1` reaches the browser as
/// `javascript:alert1` - a scheme the probe never saw, because the probe reads
/// the authored bytes. `&#x6A;` and `javascript&colon;alert1` are the same trick
/// (the second hides the colon, so there is no scheme to find at all).
///
/// Escaping the ampersand rather than percent-encoding it is what keeps this
/// honest: percent-encoding `&` would corrupt every legitimate query string,
/// while `&amp;` decodes back to `&` in the consumer, so the URL it resolves is
/// byte-for-byte the one probed here. It also stops the consumer from silently
/// rewriting an authored `&#106;` into `j`. An ampersand that opens nothing
/// (`?a=1&b=2`) is left exactly as authored.
fn neutralize_char_refs(url: &str) -> String {
    if !url.contains('&') {
        return url.to_string();
    }
    let mut out = String::with_capacity(url.len() + 8);
    for (i, ch) in url.char_indices() {
        if ch == '&' && opens_char_ref(&url[i + ch.len_utf8()..]) {
            out.push_str("&amp;");
        } else {
            out.push(ch);
        }
    }
    out
}

/// Does the text after an `&` spell a character reference?
///
/// The three forms a consumer decodes: `#DIGITS;`, `#xHEXDIGITS;` and `NAME;`.
/// An unknown NAME counts too - a consumer leaves it alone either way, so
/// escaping it changes nothing a reader sees, and guessing which names are known
/// would be a second denylist to keep in step with three engines.
fn opens_char_ref(rest: &str) -> bool {
    let mut chars = rest.chars();
    match chars.next() {
        Some('#') => {
            let digits = chars.as_str();
            let (digits, hex) = match digits.strip_prefix(['x', 'X']) {
                Some(after) => (after, true),
                None => (digits, false),
            };
            for (seen, c) in digits.chars().enumerate() {
                if c == ';' {
                    return seen > 0;
                }
                let is_digit = if hex {
                    c.is_ascii_hexdigit()
                } else {
                    c.is_ascii_digit()
                };
                if !is_digit || seen >= 8 {
                    return false;
                }
            }
            false
        }
        Some(c) if c.is_ascii_alphabetic() => {
            for (seen, c) in (1usize..).zip(chars) {
                if c == ';' {
                    return true;
                }
                if !c.is_ascii_alphanumeric() || seen >= 32 {
                    return false;
                }
            }
            false
        }
        _ => false,
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
    let collapsed = format!("{}\n", out.trim_matches(|c| c == '\n' || c == ' '));

    resolve_narrowed_escapes(&collapsed)
}

/// Whether the candidate at `i` is ADJACENT to an unescaped delimiter of the
/// same character, on the line the writer is building (PART 11 §8a M1b).
///
/// `line` is the assembled output with every candidate resolved to its BARE
/// character, so it is the line as it reads if nothing is escaped, and an index
/// into it is an index into the text being rewritten. "On the emitted line"
/// needs no line splitting: a neighbour across a newline IS a newline, which is
/// never the same character.
///
/// A neighbour BEFORE the candidate counts only if it is not itself behind a
/// backslash - the clause's "not behind a backslash" - so the run of backslashes
/// in front of it is counted and an odd run disqualifies it. A neighbour AFTER
/// never can be: the character in front of it is the candidate itself.
fn adjacent_to_live_delimiter(line: &[char], i: usize, ch: char) -> bool {
    if line.get(i + 1) == Some(&ch) {
        return true;
    }
    if i == 0 || line[i - 1] != ch {
        return false;
    }
    let mut backslashes = 0usize;
    let mut j = i.checked_sub(2);
    while let Some(k) = j {
        if line[k] != '\\' {
            break;
        }
        backslashes += 1;
        j = k.checked_sub(1);
    }
    backslashes % 2 == 0
}

/// Resolve the narrowed escapes: PART 11 §8a, M1b.
///
/// `_`, `#` and `[` are escaped IF AND ONLY IF the character is adjacent on the
/// emitted line to an unescaped delimiter of the same character. Adjacent, and
/// unescaping would MERGE THE TWO INTO ONE RUN, which every Markdown reader this
/// target answers to resolves by run length - so that escape is holding a run
/// boundary apart under all of them at once, and it is kept. Not adjacent, and
/// the escape protects nothing under any of them: `company_id`, `C#` and
/// `issue #123` are written as the author typed them, and a backslash inside an
/// identifier no longer breaks exact-match search in the published document.
///
/// THE ASTERISK IS NOT HERE, and that is M1a rather than an omission. See
/// `escape_text`.
///
/// IT RUNS ON THE ASSEMBLED OUTPUT because the test is over the LINE and not
/// over the node: the parser splits `company_id` into the text nodes `company`
/// and `_id`, so at escape time the underscore looks like it starts a word.
///
/// IT DECIDES ON THE SENTINEL rather than on a `\_` in the output, because the
/// assembled document also contains regions this renderer must reproduce
/// byte-exact - code spans, code blocks, link destinations, titles, raw HTML -
/// and a backslash there is content, not an escape. Matching `\_` rewrote those
/// too (carve-js issue 400). It also keeps M2 out of the question: an
/// author-escaped character is an `escaped_text` node emitted AS AN ESCAPE, and
/// it never carries a sentinel, so nothing here can unescape it.
fn resolve_narrowed_escapes(text: &str) -> String {
    if !text.chars().any(|c| narrowed_character(c).is_some()) {
        return text.to_string();
    }
    let line: Vec<char> = text
        .chars()
        .map(|c| narrowed_character(c).unwrap_or(c))
        .collect();
    let mut out = String::with_capacity(text.len());
    for (i, raw) in text.chars().enumerate() {
        match narrowed_character(raw) {
            Some(ch) => {
                if adjacent_to_live_delimiter(&line, i, ch) {
                    out.push('\\');
                }
                out.push(ch);
            }
            None => out.push(raw),
        }
    }
    out
}

fn flatten_heading_text(text: &str) -> String {
    // ASCII layout whitespace only. `str::trim` uses `char::is_whitespace`, which
    // includes U+00A0 - so a heading whose text began with a NO-BREAK SPACE lost
    // it here, on the Markdown target alone, while every other target in this
    // engine and both other engines kept it (carve-rs#614). A no-break space is
    // CONTENT: the author typed a character, not indentation.
    //
    // Mid-text was never affected, because only the ends are trimmed; this is
    // specifically the leading and trailing position.
    let layout = |c: char| c == ' ' || c == '\t' || c == '\r';

    text.split('\n')
        .map(|part| part.trim_matches(layout))
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// The id this heading gets, with duplicate disambiguation.
///
/// Mirrors `render::next_heading_id`: an explicit `{#id}` wins verbatim, an auto
/// slug takes a `-N` suffix on repeat, and an auto slug never lands on an id an
/// explicit one already claims. Called once per heading per pass, in document
/// order, so the prepass and the render agree on every id.
fn next_heading_id(
    heading: &Heading,
    counts: &mut HashMap<String, usize>,
    explicit_ids: &HashSet<String>,
) -> String {
    let explicit = heading.attrs.as_ref().and_then(|attrs| attrs.id.clone());
    let has_explicit = explicit.is_some();
    let base = explicit.unwrap_or_else(|| slugify(&plain_inlines(&heading.children)));

    let mut count = counts.get(&base).copied().unwrap_or(0);
    let id = loop {
        count += 1;
        let candidate = if count == 1 {
            base.clone()
        } else {
            format!("{base}-{count}")
        };
        if has_explicit || !explicit_ids.contains(&candidate) {
            break candidate;
        }
    };
    counts.insert(base, count);
    id
}

// Markdown-specific flattening. Node coverage is kept in lockstep with the
// core, including `CitationGroup` -> `raw`, so a citation heading's id is
// consistent here too.
fn plain_inlines(nodes: &[InlineNode]) -> String {
    let mut out = String::new();
    for node in nodes {
        match node {
            InlineNode::Text(text) => {
                out.push_str(&text.value.replace(crate::NBSP_PLACEHOLDER, " "))
            }
            // Visible prose, so it feeds this slug exactly as it feeds the core's
            // (carve-rs#800). This is the THIRD spelling of one derivation - the
            // parse-time index, the HTML renderer and this one - and the reason
            // the arm goes in all three at once is that a heading id derived two
            // ways has to be one id.
            InlineNode::EscapedText(escaped) => out.push_str(&escaped.value),
            InlineNode::SmartPunctuation(s) => out.push_str(smart_punctuation_text(s)),
            InlineNode::Emphasis(emphasis) => out.push_str(&plain_inlines(&emphasis.children)),
            InlineNode::Code(code) => out.push_str(&code.value),
            // An inline literal renders as visible prose (§27), so it feeds a
            // Markdown heading slug like a code span does.
            InlineNode::LiteralInline(lit) => out.push_str(&lit.content),
            // A cross-reference contributes NOTHING to the slug, exactly as in
            // `render::plain_inlines`. By this point resolution has turned it
            // into a Link carrying the target heading's text, so counting it
            // would slug `# A </#a>` as `A-A` and every id derived here would
            // disagree with the one the core assigned before resolution.
            InlineNode::Link(link) if link.from_crossref => {}
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
            InlineNode::SoftBreak(_) | InlineNode::HardBreak(_) => out.push(' '),
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
        crate::render_depth::record("markdown");
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

/// Walks inline content, telling the visitor whether the node sits inside a
/// link label. The flag matters because a reference inside a label renders as
/// plain text (links never nest), so it is not a live reference and must not
/// keep a heading's `{#id}` suffix alive. It resets at a footnote body, which
/// renders outside the anchor.
fn walk_inlines<F>(nodes: &[InlineNode], depth: usize, in_link: bool, visit: &mut F)
where
    F: FnMut(&InlineNode, bool),
{
    if depth > MAX_RENDER_DEPTH {
        crate::render_depth::record("markdown");
        return;
    }
    for node in nodes {
        visit(node, in_link);
        match node {
            InlineNode::Emphasis(emphasis) => {
                walk_inlines(&emphasis.children, depth + 1, in_link, visit)
            }
            InlineNode::Link(link) => walk_inlines(&link.children, depth + 1, true, visit),
            InlineNode::Span(span) => walk_inlines(&span.children, depth + 1, in_link, visit),
            InlineNode::Extension(extension) => {
                walk_inlines(&extension.children, depth + 1, in_link, visit)
            }
            InlineNode::Footnote(footnote) => {
                if let Some(inline) = &footnote.inline {
                    walk_inlines(inline, depth + 1, false, visit);
                }
            }
            InlineNode::CriticInsert(insert) => {
                walk_inlines(&insert.children, depth + 1, in_link, visit)
            }
            InlineNode::CriticDelete(delete) => {
                walk_inlines(&delete.children, depth + 1, in_link, visit)
            }
            _ => {}
        }
    }
}
