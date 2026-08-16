//! Source diagnostics for silent degradations (`lint_carve`).
//!
//! This is carve-rs' lint surface, and it starts with the two rules PART 9 §10
//! implies and has nowhere else to say (markup-carve/carve#1131,
//! markup-carve/carve#1132). Neither describes an engine defect: carve-rs,
//! carve-js and carve-php render these byte-identically and exactly as the
//! clause reads. They report the two places where the clause's own scope loses
//! something an author wrote, with nothing else marking it.
//!
//! `semantic-attribute-value-ignored`
//! : a value on a reserved name that only SELECTS a wrapper. `[x]{kbd="V"}`
//!   renders `<kbd>x</kbd>` and `V` reaches no output.
//!
//! `semantic-attribute-outside-span`
//! : a reserved name on any target other than an ordinary `[content]{attrs}`
//!   span, where §10 does not apply and it stays a raw attribute. `` `c`{kbd} ``
//!   renders `<code kbd="">c</code>`.
//!
//! Both rules are TIER-AWARE. PART 9 §9 reserves `abbr`, `time` and `kbd` in
//! core; `samp`, `var`, `cite` and `dfn` only become elements once the
//! `SemanticSpan` extension is registered. In a core render those four stay
//! ordinary attributes and their value reaches the output intact, so reporting
//! it as discarded would report a loss that is not happening - the same defect
//! these rules exist to catch, pointed the other way. Hence
//! [`lint_carve_with_options`]: pass the same [`Options`] you pass to
//! `render_html_with_options`, and the diagnostics describe the output the
//! author will actually get.
//!
//! PART 9 §4c adds the composite-figure findings (markup-carve/carve#1122),
//! each a diagnostic over a valid parse:
//!
//! `figure-group-opener-metadata`
//! : a `::: figure` opener carrying a quoted title or a `[label]` stays a
//!   generic container - the group has no title or label slot by design.
//!
//! `figure-group-nested`
//! : a bare `::: figure` opener inside an open group's body stays a generic
//!   container; groups do not nest.
//!
//! `figure-group-panel-number`
//! : a `#` placeholder in a PANEL caption has nothing to resolve against and
//!   stays literal - panels are not sequence units.
//!
//! §4c also names `figure-group-empty` and `figure-group-single-panel` as
//! STRICT-PROFILE findings. This surface has no profile or severity axis yet,
//! and reporting them unconditionally would flag documents the clause calls
//! valid and ordinary, so they wait for that axis rather than shipping under
//! the wrong severity.
//!
//! Rule ids and messages match carve-js' `lintCarve` (`src/lint.ts`) - "same
//! rule, same id" is what parity means here, and a consumer reading
//! diagnostics from two engines must not see one warning under two spellings.

use crate::ast::*;
use crate::ast_json::emphasis_type;
use crate::escape::{escape_attr, sanitize_attr_value};
use crate::extension::Options;
use crate::parse::parse_with_options;
use crate::render::{semantic_span_order, semantic_value_target, EXTENDED_SEMANTIC_SPAN_ORDER};

/// One diagnostic: a silent degradation, located in the source the caller
/// passed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LintWarning {
    /// 1-based line number.
    pub line: usize,
    /// 1-based column number.
    pub column: usize,
    /// Stable rule id, e.g. `"semantic-attribute-outside-span"`. Shared with
    /// carve-js and carve-php: the same trigger reports the same id everywhere.
    pub rule: &'static str,
    /// Human-readable explanation of the degradation.
    pub message: String,
    /// 0-based start offset in the source, inclusive, in BYTES.
    ///
    /// Deliberately NOT the CODEPOINT offsets PART 12 §4 pins for a serialized
    /// AST, which is what [`Pos`] carries. This struct is a diagnostic a Rust
    /// caller slices its own `&str` with, and a codepoint offset handed to
    /// `&source[start..end]` PANICS on the first non-ASCII character before it.
    /// That is a crash, not a wrong highlight. carve-js converts the same
    /// positions to UTF-16 for the same reason; the unit follows the host
    /// language.
    pub start: usize,
    /// 0-based end offset in the source, exclusive, in bytes.
    pub end: usize,
}

/// Lint `source` as a CORE render, with no extensions registered.
///
/// Equivalent to [`lint_carve_with_options`] with [`Options::default`]. Use
/// that instead whenever the caller renders with extensions - see the module
/// docs for why the tier matters to both rules.
pub fn lint_carve(source: &str) -> Vec<LintWarning> {
    lint_carve_with_options(source, &Options::default())
}

/// Lint `source` for the render `options` describe.
///
/// Only `options.extensions` is read, because it is the only field either rule
/// depends on: it decides which reserved names become elements. Positions are
/// forced on regardless of what the caller set, since a diagnostic with no
/// location is not one. Nothing here renders, so the render-only fields
/// (`profile`, `mode`, `symbols`, ...) are deliberately ignored rather than
/// half-applied.
pub fn lint_carve_with_options(source: &str, options: &Options<'_>) -> Vec<LintWarning> {
    let mut parse_options = Options {
        positions: true,
        ..Options::default()
    };
    parse_options.extensions.clone_from(&options.extensions);
    let doc = parse_with_options(source, &parse_options);

    let element_names = semantic_span_order(options);
    let byte_at = codepoint_to_byte_map(source);
    let to_byte = |offset: usize| -> usize {
        match &byte_at {
            Some(map) => map.get(offset).copied().unwrap_or(source.len()),
            None => offset.min(source.len()),
        }
    };
    let mut out = Vec::new();
    let mut visit = |node_type: &'static str, attrs: &Attrs, pos: Option<Pos>| {
        collect_semantic_attribute_warnings(
            node_type,
            attrs,
            pos,
            &element_names,
            &to_byte,
            &mut out,
        );
    };
    walk_blocks(&doc.children, &mut visit);
    // A footnote definition hoists to the document (PART 9 §7), so its body is
    // not reachable from `children` and a rule that only walked those would be
    // silent inside every footnote.
    for body in doc.footnote_defs.values() {
        walk_blocks(body, &mut visit);
    }
    collect_figure_group_warnings(&doc.children, false, &to_byte, &mut out);
    for body in doc.footnote_defs.values() {
        collect_figure_group_warnings(body, false, &to_byte, &mut out);
    }
    collect_template_source_warning(source, &doc, &mut out);
    out.sort_by_key(|w| (w.start, w.end, w.rule));
    out
}

fn collect_template_source_warning(source: &str, doc: &Document, out: &mut Vec<LintWarning>) {
    // The AST test keeps verbatim contexts opaque. The source scan then asks
    // the separate, document-level question: does this look like a template
    // file that reached Carve before its template engine ran?
    let json = crate::ast_json::to_json(doc);
    let mut comment_starts = Vec::new();
    let mut json_at = 0;
    while let Some(rel) = json[json_at..].find("\"delimited\":true") {
        let field = json_at + rel;
        let Some(offset_rel) = json[field..].find("\"startOffset\":") else {
            break;
        };
        let digits = &json[field + offset_rel + "\"startOffset\":".len()..];
        let digit_len = digits.bytes().take_while(u8::is_ascii_digit).count();
        if let Ok(offset) = digits[..digit_len].parse::<usize>() {
            comment_starts.push(offset);
        }
        json_at = field + "\"delimited\":true".len();
    }
    if comment_starts.is_empty() {
        return;
    }
    let mut at = 0;
    while let Some(rel) = source[at..].find("{%") {
        let start = at + rel;
        let Some(close_rel) = source[start + 2..].find("%}") else {
            break;
        };
        let end = start + 2 + close_rel + 2;
        let codepoint_start = source[..start].chars().count();
        if !comment_starts.contains(&codepoint_start) {
            at = end;
            continue;
        }
        let tag = source[start + 2..end - 2].trim();
        let shaped = matches!(tag, "raw" | "endraw" | "endif" | "endfor" | "endblock")
            || ["if", "for", "block"].iter().any(|head| {
                tag.strip_prefix(head)
                    .is_some_and(|rest| rest.starts_with(char::is_whitespace))
            });
        if shaped {
            let line_start = source[..start].rfind('\n').map_or(0, |p| p + 1);
            out.push(LintWarning {
                line: source[..start].bytes().filter(|b| *b == b'\n').count() + 1,
                column: source[line_start..start].chars().count() + 1,
                rule: "braced-comment-in-a-template-source",
                message: "This `{% … %}` comment is a template tag. Liquid and Nunjucks render before the converter runs, so a page that wraps its tags in `{% raw %}` hands Carve bare template text - and PART 9 §21a makes that text a comment. Reported, never rewritten: only the author knows which of the two the document meant.".to_string(),
                start,
                end,
            });
        }
        at = end;
    }
}

/// The PART 9 §4c findings: what a `::: figure` spelling silently is not.
///
/// `in_group` says the walk is inside an open composite figure's body, at any
/// depth - the state that turns a BARE `figure` opener from a group into the
/// demoted generic container `figure-group-nested` reports. The parser makes
/// the same decision with the same scope (`IN_FIGURE_GROUP`), which is why a
/// bare `figure` admonition is only reported where that flag would have been
/// set: anywhere else the node can only come from an ingested tree, which this
/// surface never sees (it parses the source itself).
fn collect_figure_group_warnings(
    blocks: &[BlockNode],
    in_group: bool,
    to_byte: &dyn Fn(usize) -> usize,
    out: &mut Vec<LintWarning>,
) {
    for block in blocks {
        match block {
            BlockNode::Admonition(a) => {
                if a.kind == "figure" {
                    if a.title.is_some() || a.label.is_some() {
                        out.push(warning(
                            a.pos,
                            to_byte,
                            "figure-group-opener-metadata",
                            "A `::: figure` opener carrying a quoted title or a [label] stays a \
                             generic container, not a composite figure: the group has no title or \
                             label slot (PART 9 \u{a7}4c). Its one authored metadata channel is \
                             the `^ ` caption after the closing fence."
                                .to_string(),
                        ));
                    } else if in_group {
                        out.push(warning(
                            a.pos,
                            to_byte,
                            "figure-group-nested",
                            "Composite figures do not nest (PART 9 \u{a7}4c): a bare `::: figure` \
                             opener inside an open group's body stays a generic container. Close \
                             the outer group first, or drop the inner fence."
                                .to_string(),
                        ));
                    }
                }
                collect_figure_group_warnings(&a.children, in_group, to_byte, out);
            }
            BlockNode::FigureGroup(g) => {
                for child in &g.children {
                    let panel_caption = match child {
                        BlockNode::Figure(f) => Some(&f.caption),
                        BlockNode::Table(t) => t.caption.as_ref(),
                        _ => None,
                    };
                    if let Some(caption) = panel_caption {
                        for node in caption.iter() {
                            if let InlineNode::CaptionNumber(n) = node {
                                out.push(warning(
                                    n.pos,
                                    to_byte,
                                    "figure-group-panel-number",
                                    "A `#` placeholder in a panel caption stays literal: panels \
                                     are not sequence units, so it has nothing to resolve against \
                                     (PART 9 \u{a7}4c). Number the GROUP caption instead, or drop \
                                     the `#`."
                                        .to_string(),
                                ));
                            }
                        }
                    }
                }
                collect_figure_group_warnings(&g.children, true, to_byte, out);
            }
            BlockNode::Div(d) => collect_figure_group_warnings(&d.children, in_group, to_byte, out),
            BlockNode::BlockQuote(b) => {
                collect_figure_group_warnings(&b.children, in_group, to_byte, out)
            }
            BlockNode::LineBlock(lb) => {
                collect_figure_group_warnings(&lb.children, in_group, to_byte, out)
            }
            BlockNode::List(l) => {
                for item in &l.items {
                    collect_figure_group_warnings(&item.children, in_group, to_byte, out);
                }
            }
            BlockNode::DefinitionList(dl) => {
                for item in &dl.items {
                    for def in &item.definitions {
                        collect_figure_group_warnings(&def.children, in_group, to_byte, out);
                    }
                }
            }
            BlockNode::Figure(f) => {
                if let FigureTarget::BlockQuote(b) = &f.target {
                    collect_figure_group_warnings(&b.children, in_group, to_byte, out);
                }
            }
            BlockNode::Extension(e) => {
                collect_figure_group_warnings(&e.children, in_group, to_byte, out)
            }
            _ => {}
        }
    }
}

/// Byte offset of each codepoint in `source`, plus one past the end.
///
/// `None` for an all-ASCII source, where the two units coincide and the table
/// would be a per-lint allocation the size of the document for nothing.
fn codepoint_to_byte_map(source: &str) -> Option<Vec<usize>> {
    if source.is_ascii() {
        return None;
    }
    let mut map: Vec<usize> = source.char_indices().map(|(byte, _)| byte).collect();
    map.push(source.len());
    Some(map)
}

/// Reserved names that ARE valid HTML attributes on a given node, so finding
/// one there is the author getting what they asked for rather than a silent
/// failure.
///
/// `cite` on a block quote is the case that matters: it is a URL attribute of
/// `blockquote` and `q` in HTML, and `{cite="https://…"}` on a quote renders
/// `<blockquote cite="https://…">`. Reporting that would be telling an author
/// their correct markup is wrong.
fn is_valid_html_attribute_on(node_type: &str, name: &str) -> bool {
    matches!((node_type, name), ("block_quote", "cite"))
}

fn collect_semantic_attribute_warnings(
    node_type: &'static str,
    attrs: &Attrs,
    pos: Option<Pos>,
    element_names: &[&'static str],
    to_byte: &dyn Fn(usize) -> usize,
    out: &mut Vec<LintWarning>,
) {
    for name in EXTENDED_SEMANTIC_SPAN_ORDER {
        let Some(value) = attrs.key_values.get(name) else {
            continue;
        };

        if node_type == "span" {
            // §10 applies only to a name that becomes an ELEMENT in this
            // render. One that does not stays an ordinary attribute and carries
            // its value to the output, so there is nothing to report.
            if value.is_empty()
                || !element_names.contains(&name)
                || semantic_value_target(name).is_some()
            {
                continue;
            }
            out.push(warning(
                pos,
                to_byte,
                "semantic-attribute-value-ignored",
                format!(
                    "Value on the semantic attribute \"{name}\" is discarded: it selects the \
                     <{name}> element and reaches no output. Only abbr, dfn and time carry a \
                     value (as title or datetime)."
                ),
            ));
            continue;
        }

        // Same tier test for the other rule: a name this render leaves an
        // ordinary attribute is an ordinary attribute everywhere, so it is not
        // "outside the span" - it is exactly what the author asked for.
        if !element_names.contains(&name) {
            continue;
        }
        if is_valid_html_attribute_on(node_type, name) {
            continue;
        }
        // The tail quotes the value the RENDERER emits, escaped the way it
        // escapes it. A fixed `name=""` is true only for the boolean form and
        // false the moment a value is authored - `` `c`{kbd="V"} `` renders
        // `<code kbd="V">` - and the valued case is precisely the one where a
        // reader needs the sentence to describe their own input back to them.
        let emitted = quoted_attribute_value(name, value);
        out.push(warning(
            pos,
            to_byte,
            "semantic-attribute-outside-span",
            format!(
                "\"{name}\" is a semantic span attribute (PART 9 §10) and only applies to an \
                 ordinary [content]{{attrs}} span; on {node_type} it stays a raw attribute and \
                 renders as {name}=\"{emitted}\"."
            ),
        ));
    }
}

/// The attribute value as it reaches the output, ready to sit inside the
/// message's own quotes.
///
/// Put through the SAME two steps the renderer puts an attribute value
/// through, in the same order: [`sanitize_attr_value`] then [`escape_attr`].
/// Sanitizing is not decoration here - a value carrying a dangerous URL scheme
/// is blanked on the way out, so `` `c`{kbd="javascript:alert(1)"} `` really
/// does render `kbd=""`, and a message that quoted the authored text would be
/// wrong in exactly the way this rule was fixed to stop being wrong.
///
/// Capped, because the value is author text and a diagnostic is read in a
/// terminal or an editor gutter: an attribute carrying a paragraph would push
/// the part of the sentence that explains the problem off the line.
///
/// THE THREE STEPS RUN IN EXACTLY THAT ORDER AND NONE OF THEM COMMUTES:
///
/// The sanitizer runs FIRST. It reads the WHOLE value, so cutting first can
/// quote a payload back as a harmless-looking prefix while the output holds an
/// empty attribute.
///
/// The cut counts CHARACTERS rather than bytes, so it never lands inside a
/// UTF-8 sequence and quotes a broken character back at the author.
///
/// Escaping happens LAST, so the cut cannot land inside an entity and print
/// `&qu` as though it were authored.
fn quoted_attribute_value(name: &str, value: &str) -> String {
    let emitted = sanitize_attr_value(name, value);
    match emitted.char_indices().nth(QUOTED_VALUE_LIMIT) {
        Some((byte, _)) => format!("{}{QUOTED_VALUE_ELLIPSIS}", escape_attr(&emitted[..byte])),
        None => escape_attr(&emitted),
    }
}

/// Longest rendered value quoted back whole, in CHARACTERS.
///
/// The number is not the spec's - the ruling says the diagnostic quotes the
/// value the renderer emits, truncated if long, and fixes no length. It is
/// carve-js' and carve-php's, so that one authored value produces one message
/// whichever engine a consumer reads it from (markup-carve/carve-js#1058).
const QUOTED_VALUE_LIMIT: usize = 120;

/// Marks a value the diagnostic cut, inside the quotes it was cut from.
const QUOTED_VALUE_ELLIPSIS: char = '…';

/// A node whose `pos` the parser could not determine still gets a diagnostic -
/// dropping it would make the rule silent on exactly the constructs whose
/// positions are hardest to derive. It points at the start of the document,
/// which is where a reader with no better information should start looking.
fn warning(
    pos: Option<Pos>,
    to_byte: &dyn Fn(usize) -> usize,
    rule: &'static str,
    message: String,
) -> LintWarning {
    let pos = pos.unwrap_or_default();
    let start = to_byte(pos.start_offset);
    LintWarning {
        line: pos.start_line.max(1),
        column: pos.start_column.max(1),
        rule,
        message,
        start,
        end: to_byte(pos.end_offset.max(pos.start_offset)).max(start),
    }
}

type Visit<'f> = dyn FnMut(&'static str, &Attrs, Option<Pos>) + 'f;

/// Hand the node to `visit` when it carries attributes at all. A node without
/// them can hold no reserved name, so it is not worth a call.
fn report(node_type: &'static str, attrs: &Option<Attrs>, pos: Option<Pos>, visit: &mut Visit<'_>) {
    if let Some(attrs) = attrs {
        visit(node_type, attrs, pos);
    }
}

fn walk_blocks(nodes: &[BlockNode], visit: &mut Visit<'_>) {
    for node in nodes {
        walk_block(node, visit);
    }
}

/// Every block variant, with NO wildcard arm.
///
/// That is the point of writing it out: a variant added to [`BlockNode`] breaks
/// this build instead of silently becoming a place the rules never fire. A lint
/// rule that cannot fire is the failure mode these two rules exist to remove,
/// so the walker must not reintroduce it one node type at a time.
///
/// The type strings are the PART 12 wire names `ast_json` publishes, because
/// they are what the `outside-span` message names and what carve-js reports.
/// `tests/lint.rs` pins each one against `to_json` output rather than trusting
/// this list.
fn walk_block(node: &BlockNode, visit: &mut Visit<'_>) {
    match node {
        BlockNode::Heading(n) => {
            report("heading", &n.attrs, n.pos, visit);
            walk_inlines(&n.children, visit);
        }
        BlockNode::CitationDefinition(n) => {
            report("citation_definition", &n.attrs, n.pos, visit);
            walk_inlines(&n.children, visit);
        }
        BlockNode::Paragraph(n) => {
            report("paragraph", &n.attrs, n.pos, visit);
            walk_inlines(&n.children, visit);
        }
        BlockNode::CodeBlock(n) => report("code_block", &n.attrs, n.pos, visit),
        BlockNode::List(n) => {
            report("list", &n.attrs, n.pos, visit);
            for item in &n.items {
                report("list_item", &item.attrs, item.pos, visit);
                walk_blocks(&item.children, visit);
            }
        }
        BlockNode::BlockQuote(n) => walk_block_quote(n, visit),
        BlockNode::Table(n) => walk_table(n, visit),
        BlockNode::Admonition(n) => {
            report("admonition", &n.attrs, n.pos, visit);
            if let Some(title) = &n.title {
                walk_inlines(title, visit);
            }
            walk_blocks(&n.children, visit);
        }
        BlockNode::Div(n) => {
            report("div", &n.attrs, n.pos, visit);
            walk_blocks(&n.children, visit);
        }
        BlockNode::LineBlock(n) => {
            report("line_block", &n.attrs, n.pos, visit);
            walk_blocks(&n.children, visit);
        }
        BlockNode::DefinitionList(n) => {
            report("definition_list", &n.attrs, n.pos, visit);
            for item in &n.items {
                for term in &item.terms {
                    report("definition_term", &term.attrs, term.pos, visit);
                    walk_inlines(&term.children, visit);
                }
                for def in &item.definitions {
                    report("definition_description", &def.attrs, def.pos, visit);
                    walk_blocks(&def.children, visit);
                }
            }
        }
        BlockNode::Figure(n) => {
            report("figure", &n.attrs, n.pos, visit);
            match &n.target {
                FigureTarget::Image(image) => report("image", &image.attrs, image.pos, visit),
                FigureTarget::BlockQuote(quote) => walk_block_quote(quote, visit),
                FigureTarget::Table(table) => walk_table(table, visit),
                FigureTarget::CodeBlock(block) => {
                    report("code_block", &block.attrs, block.pos, visit)
                }
                FigureTarget::Paragraph(para) => {
                    report("paragraph", &para.attrs, para.pos, visit);
                    walk_inlines(&para.children, visit);
                }
            }
            walk_inlines(&n.caption, visit);
            if let Some(short) = &n.short_caption {
                walk_inlines(short, visit);
            }
        }
        BlockNode::FigureGroup(n) => {
            report("figure_group", &n.attrs, n.pos, visit);
            walk_blocks(&n.children, visit);
            if let Some(caption) = &n.caption {
                walk_inlines(caption, visit);
            }
        }
        // Carries no `attrs` field and no children.
        BlockNode::AbbreviationDef(_) => {}
        BlockNode::LinkReferenceDefinition(n) => {
            report("link_reference_definition", &n.attrs, n.pos, visit)
        }
        // Verbatim: no `attrs` field, and its content is not markup.
        BlockNode::RawBlock(_) => {}
        BlockNode::Comment(_) => {}
        BlockNode::Extension(n) => {
            report("block_extension", &n.attrs, n.pos, visit);
            if let Some(summary) = &n.summary {
                walk_inlines(summary, visit);
            }
            walk_blocks(&n.children, visit);
        }
        BlockNode::BlockImage(n) => report("image", &n.attrs, n.pos, visit),
        BlockNode::ThematicBreak(n) => report("thematic_break", &n.attrs, n.pos, visit),
    }
}

fn walk_block_quote(n: &BlockQuote, visit: &mut Visit<'_>) {
    report("block_quote", &n.attrs, n.pos, visit);
    walk_blocks(&n.children, visit);
}

fn walk_table(n: &Table, visit: &mut Visit<'_>) {
    report("table", &n.attrs, n.pos, visit);
    if let Some(caption) = &n.caption {
        walk_inlines(caption, visit);
    }
    if let Some(short) = &n.short_caption {
        walk_inlines(short, visit);
    }
    for row in &n.rows {
        report("table_row", &row.attrs, row.pos, visit);
        for cell in &row.cells {
            report("table_cell", &cell.attrs, cell.pos, visit);
            walk_inlines(&cell.children, visit);
        }
    }
}

fn walk_inlines(nodes: &[InlineNode], visit: &mut Visit<'_>) {
    for node in nodes {
        walk_inline(node, visit);
    }
}

/// Every inline variant, with NO wildcard arm - see [`walk_block`].
fn walk_inline(node: &InlineNode, visit: &mut Visit<'_>) {
    match node {
        // No `attrs` field: the parser has nowhere to put a reserved name.
        InlineNode::Text(_)
        | InlineNode::EscapedText(_)
        | InlineNode::SmartPunctuation(_)
        | InlineNode::RawInline(_)
        | InlineNode::CrossRef(_)
        | InlineNode::CaptionNumber(_)
        | InlineNode::Abbreviation(_)
        | InlineNode::SoftBreak(_)
        | InlineNode::HardBreak(_)
        | InlineNode::CriticSubstitute(_)
        | InlineNode::CriticComment(_)
        | InlineNode::Comment(_) => {}
        InlineNode::Emphasis(n) => {
            report(emphasis_type(n.kind), &n.attrs, n.pos, visit);
            walk_inlines(&n.children, visit);
        }
        InlineNode::Code(n) => report("code", &n.attrs, n.pos, visit),
        InlineNode::Link(n) => {
            report("link", &n.attrs, n.pos, visit);
            walk_inlines(&n.children, visit);
        }
        InlineNode::Image(n) => report("image", &n.attrs, n.pos, visit),
        InlineNode::Span(n) => {
            report("span", &n.attrs, n.pos, visit);
            walk_inlines(&n.children, visit);
        }
        InlineNode::Math(n) => report("math", &n.attrs, n.pos, visit),
        InlineNode::LiteralInline(n) => report("literal_inline", &n.attrs, n.pos, visit),
        InlineNode::Symbol(n) => report("symbol", &n.attrs, n.pos, visit),
        InlineNode::AutoLink(n) => report("autolink", &n.attrs, n.pos, visit),
        InlineNode::Mention(n) => report("mention", &n.attrs, n.pos, visit),
        InlineNode::Tag(n) => report("tag", &n.attrs, n.pos, visit),
        InlineNode::CitationGroup(n) => {
            for item in &n.items {
                for part in [&item.prefix, &item.locator, &item.suffix]
                    .into_iter()
                    .flatten()
                {
                    walk_inlines(part, visit);
                }
            }
        }
        InlineNode::Extension(n) => {
            report("inline_extension", &n.attrs, n.pos, visit);
            walk_inlines(&n.children, visit);
        }
        InlineNode::Footnote(n) => {
            let node_type = if n.inline.is_some() {
                "inline_footnote"
            } else {
                "footnote_ref"
            };
            report(node_type, &n.attrs, n.pos, visit);
            if let Some(inline) = &n.inline {
                walk_inlines(inline, visit);
            }
        }
        InlineNode::CriticInsert(n) => {
            report("insert", &n.attrs, n.pos, visit);
            walk_inlines(&n.children, visit);
        }
        InlineNode::CriticDelete(n) => {
            report("delete", &n.attrs, n.pos, visit);
            walk_inlines(&n.children, visit);
        }
    }
}
