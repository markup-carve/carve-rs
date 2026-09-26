//! Source diagnostics for silent degradations.
//!
//! [`lint_carve_with_options`] should receive the same [`Options`] used for
//! rendering. Under PART 9 §9-10, core reserves `abbr`, `time`, and `kbd`;
//! `samp`, `var`, `cite`, and `dfn` become semantic elements only with the
//! `SemanticSpan` extension.
//! The parser records unattached block attributes, which leave no AST node;
//! this pass reports that record instead of re-reading source attachment.
//! The figure-group empty and single-panel findings in §4c are strict-profile
//! only; reporting them unconditionally would flag valid documents. They await
//! a profile/severity axis. Unattached attributes follow PART 9 §15 A4.
//! See `docs/linting.md` for rule ids and examples.

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
/// that instead whenever the caller renders with extensions.
pub fn lint_carve(source: &str) -> Vec<LintWarning> {
    lint_carve_with_options(source, &Options::default())
}

/// Lint `source` for the render `options` describe.
///
/// Only `options.extensions` is read: it selects extension-specific rules and
/// decides which reserved names become elements. Positions are
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
    // The one rule the AST cannot answer. An unattached block attribute leaves
    // NOTHING behind - no node, no attrs on a neighbour - which is precisely why
    // §15 A4 needs a diagnostic, and why the parser has to say where it dropped
    // one rather than the linter guessing from the source.
    let (doc, unattached) = crate::parse::collecting_unattached_block_attrs(|| {
        parse_with_options(source, &parse_options)
    });

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
    collect_quote_fence_warnings(&doc.children, &to_byte, &mut out);
    // A footnote definition hoists to the document (PART 9 §7), so its body is
    // not reachable from `children`. The walk reports nothing there yet, for a
    // reason outside this rule: a block opener below a quote line in a footnote
    // body is folded into the quote's paragraph as lazy text
    // (markup-carve/carve-rs#1415), so the construct is never parsed. Walking
    // it anyway is what makes the rule fire on the day the parse agrees, rather
    // than leaving a second defect behind the first.
    for body in doc.footnote_defs.values() {
        collect_quote_fence_warnings(body, &to_byte, &mut out);
    }
    if options
        .extensions
        .iter()
        .any(|ext| ext.name() == "citations")
    {
        collect_contained_reference_placements(&doc.children, false, &to_byte, &mut out);
        for body in doc.footnote_defs.values() {
            collect_contained_reference_placements(body, true, &to_byte, &mut out);
        }
    }
    collect_template_source_warning(source, &doc, &mut out);
    collect_unattached_block_attribute_warnings(source, &unattached, &to_byte, &mut out);
    collect_table_column_warnings(source, &mut out);
    out.sort_by_key(|w| (w.start, w.end, w.rule));
    out
}

fn collect_contained_reference_placements(
    blocks: &[BlockNode],
    contained: bool,
    to_byte: &dyn Fn(usize) -> usize,
    out: &mut Vec<LintWarning>,
) {
    for block in blocks {
        match block {
            BlockNode::Directive(d) => {
                if contained && d.kind == "references" {
                    out.push(warning(
                        d.pos.clone(),
                        to_byte,
                        "references-placement-in-container",
                        "This \"::: references\" marker is inside a container and does not place the reference list. Move it to document level, or remove it if no placement is needed.".to_string(),
                    ));
                }
                collect_contained_reference_placements(&d.children, true, to_byte, out);
            }
            BlockNode::Admonition(n) => {
                collect_contained_reference_placements(&n.children, true, to_byte, out)
            }
            BlockNode::Div(n) => {
                collect_contained_reference_placements(&n.children, true, to_byte, out)
            }
            BlockNode::Section(n) => {
                collect_contained_reference_placements(&n.children, true, to_byte, out)
            }
            BlockNode::BlockQuote(n) => {
                collect_contained_reference_placements(&n.children, true, to_byte, out)
            }
            BlockNode::LineBlock(n) => {
                collect_contained_reference_placements(&n.children, true, to_byte, out)
            }
            BlockNode::FigureGroup(n) => {
                collect_contained_reference_placements(&n.children, true, to_byte, out)
            }
            BlockNode::ExtensionCarrier(n) => {
                collect_contained_reference_placements(&n.children, true, to_byte, out)
            }
            BlockNode::List(n) => {
                for item in &n.items {
                    collect_contained_reference_placements(&item.children, true, to_byte, out);
                }
            }
            BlockNode::DefinitionList(n) => {
                for item in &n.items {
                    for definition in &item.definitions {
                        collect_contained_reference_placements(
                            &definition.children,
                            true,
                            to_byte,
                            out,
                        );
                    }
                }
            }
            BlockNode::Figure(n) => {
                if let FigureTarget::BlockQuote(quote) = &*n.target {
                    collect_contained_reference_placements(&quote.children, true, to_byte, out);
                }
            }
            BlockNode::Table(n) => {
                for row in &n.rows {
                    for cell in &row.cells {
                        if let Some(blocks) = &cell.blocks {
                            collect_contained_reference_placements(blocks, true, to_byte, out);
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

fn collect_table_column_warnings(source: &str, out: &mut Vec<LintWarning>) {
    let lines: Vec<&str> = source.lines().collect();
    let mut line_start = 0usize;
    for (index, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with('|') {
            let bytes = line.as_bytes();
            let mut i = 0;
            while i < bytes.len() {
                if bytes[i] == b'|' {
                    let mut start = i + 1;
                    if bytes.get(start) == Some(&b'=') {
                        start += 1;
                    }
                    let mut end = start;
                    while matches!(bytes.get(end), Some(b'<' | b'>' | b'~' | b'^' | b'v'))
                        && end - start < 2
                    {
                        end += 1;
                    }
                    if end > start
                        && bytes.get(end).is_some_and(|b| {
                            !b.is_ascii_whitespace()
                                && *b != b'{'
                                && !matches!(b, b'<' | b'>' | b'~' | b'^' | b'v')
                        })
                    {
                        out.push(LintWarning { line: index + 1, column: start + 1, rule: "table-alignment-run-padding", message: format!("The table alignment run {:?} has no terminating space, so it is literal cell content. Add a space after the run to make it alignment.", &line[start..end]), start: line_start + start, end: line_start + end });
                    }
                }
                i += 1;
            }
        } else if ["aligns", "valigns", "widths"]
            .iter()
            .any(|k| line.contains(&format!("{k}=")))
        {
            let next = lines.get(index + 1).copied().unwrap_or("");
            if next.trim_start().starts_with('|') {
                let columns = next.matches('|').count().saturating_sub(1);
                for key in ["aligns", "valigns", "widths"] {
                    let Some(key_at) = line.find(&format!("{key}=")) else {
                        continue;
                    };
                    let value_at = key_at + key.len() + 1;
                    let rest = &line[value_at..];
                    let raw = if let Some(q) = rest.strip_prefix('"') {
                        q.split('"').next().unwrap_or("")
                    } else {
                        rest.split(|c: char| c.is_whitespace() || c == '}')
                            .next()
                            .unwrap_or("")
                    };
                    let values: Vec<&str> = raw.split(',').collect();
                    let push = |out: &mut Vec<LintWarning>, rule, message: String| {
                        out.push(LintWarning {
                            line: index + 1,
                            column: key_at + 1,
                            rule,
                            message,
                            start: line_start + key_at,
                            end: line_start + key_at + key.len(),
                        })
                    };
                    if values.len() < columns {
                        push(out, "table-column-arity", format!("{key} supplies {} column entries for a {columns}-column table; the unset tail is valid but may be accidental.", values.len()));
                    }
                    if key == "widths"
                        && values
                            .iter()
                            .filter_map(|v| v.trim().parse::<f64>().ok())
                            .sum::<f64>()
                            > 100.0
                    {
                        push(
                            out,
                            "table-width-total",
                            "The specified table column widths total more than 100%.".to_owned(),
                        );
                    }
                    let overlap = (key == "aligns"
                        && (next.contains("|=<") || next.contains("|=>") || next.contains("|=~")))
                        || (key == "valigns"
                            && (next.contains("|=^")
                                || next.contains("|=v")
                                || next.contains("|=~")));
                    if overlap {
                        push(out, "table-column-overlap", "A column supplies the same alignment axis both in the table and in a table attribute; the in-table marker wins.".to_owned());
                    }
                }
            }
        }
        line_start += line.len() + 1;
    }
}

/// PART 9 §15 A4: a floating `{…}` that ran out of blocks to attach to.
///
/// Both ways of running out are the same finding - the DOCUMENT ended, or the
/// CONTAINER holding the attribute did. Nothing is emitted for it either way, so
/// `> {.k}` written on a quote's last line renders neither on the quote nor on
/// anything after it, and without this the loss is invisible.
fn collect_unattached_block_attribute_warnings(
    source: &str,
    spans: &[Pos],
    to_byte: &dyn Fn(usize) -> usize,
    out: &mut Vec<LintWarning>,
) {
    if spans.is_empty() {
        return;
    }
    let line_starts = line_start_offsets(source);
    for pos in spans {
        let start = to_byte(codepoint_offset_at(
            &line_starts,
            pos.start_line,
            pos.start_column,
        ));
        let end = to_byte(codepoint_offset_at(
            &line_starts,
            pos.end_line,
            pos.end_column,
        ))
        .max(start);
        out.push(LintWarning {
            line: pos.start_line.max(1),
            column: pos.start_column.max(1),
            rule: "unattached-block-attribute",
            message: "This block attribute reaches no block: the document or the container \
                      holding it ended first, so nothing is emitted for it. Move it in front of \
                      the block it belongs to, or delete it."
                .to_string(),
            start,
            end,
        });
    }
}

/// Codepoint offset of the start of each 1-based source line, in the ORIGINAL
/// text.
///
/// THE PARSER'S OWN TABLE, not a second one. Turning a (line, column) pair back
/// into an offset is a question about NORMALIZATION: a leading BOM is stripped
/// and both CRLF and a lone CR collapse to LF before the parser sees a line
/// (PART 1), so a table that counts only `\n` and keeps the BOM disagrees with
/// the positions it is being asked to place. The disagreement is silent -
/// `para\r\r{.k}\r` produced an empty span at end of input, and `<BOM>{.k}`
/// highlighted the mark and lost the closing brace. Reusing
/// `original_line_start_offsets` is what keeps one answer to the question, and
/// it is the same table `fill_offsets` places every node with.
fn line_start_offsets(source: &str) -> Vec<usize> {
    crate::parse::original_line_start_offsets(source)
}

/// The CODEPOINT offset of a 1-based (line, column) pair.
///
/// `line_starts` holds codepoint offsets and [`Pos`] counts columns in
/// codepoints too (PART 12 §4), so this is one addition; the conversion to bytes
/// is the caller's `to_byte`, the same map every other rule here goes through.
///
/// Computed rather than read off a node, because these spans are taken DURING
/// the parse and `fill_offsets` only ever runs over nodes that reached the tree.
/// An unattached attribute reaches none, by definition.
fn codepoint_offset_at(line_starts: &[usize], line: usize, column: usize) -> usize {
    let Some(&line_start) = line_starts.get(line.saturating_sub(1)) else {
        return line_starts.last().copied().unwrap_or(0);
    };
    line_start + column.saturating_sub(1)
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
                            a.pos.clone(),
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
                            a.pos.clone(),
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
                                    n.pos.clone(),
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
            BlockNode::Directive(d) => {
                collect_figure_group_warnings(&d.children, in_group, to_byte, out)
            }
            BlockNode::Div(d) => collect_figure_group_warnings(&d.children, in_group, to_byte, out),
            BlockNode::Section(d) => {
                collect_figure_group_warnings(&d.children, in_group, to_byte, out)
            }
            BlockNode::Table(t) => {
                for row in &t.rows {
                    for cell in &row.cells {
                        if let Some(blocks) = &cell.blocks {
                            collect_figure_group_warnings(blocks, in_group, to_byte, out);
                        }
                    }
                }
            }
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
                if let FigureTarget::BlockQuote(b) = &*f.target {
                    collect_figure_group_warnings(&b.children, in_group, to_byte, out);
                }
            }
            BlockNode::ExtensionCarrier(e) => {
                collect_figure_group_warnings(&e.children, in_group, to_byte, out)
            }
            _ => {}
        }
    }
}

/// The one authoring hazard around the fenced block quote that no other rule
/// here reaches (markup-carve/carve#1718).
fn collect_quote_fence_warnings(
    blocks: &[BlockNode],
    to_byte: &dyn Fn(usize) -> usize,
    out: &mut Vec<LintWarning>,
) {
    for pair in blocks.windows(2) {
        let (BlockNode::BlockQuote(above), BlockNode::BlockQuote(fence)) = (&pair[0], &pair[1])
        else {
            continue;
        };
        if above.fenced || !fence.fenced {
            continue;
        }
        let (Some(above_pos), Some(opener)) = (above.pos.clone(), fence.pos.clone()) else {
            continue;
        };
        if opener.start_line != above_pos.end_line + 1 {
            continue;
        }
        out.push(warning(
            fence.pos.clone(),
            to_byte,
            "quote-fence-ends-the-quote-above",
            "A \"::: >\" opener at the column of the quote above it ENDS that quote and opens a \
             sibling one; the two render as adjacent blockquotes, not one nested in the other. \
             Write \"> ::: >\" to nest it, or leave a blank line to make two quotes deliberate."
                .to_string(),
        ));
    }

    // The same mistake sits at a different column under a list item, inside a
    // container body and in a definition, so every block list is walked rather
    // than the document's own children alone.
    for block in blocks {
        match block {
            BlockNode::BlockQuote(b) => collect_quote_fence_warnings(&b.children, to_byte, out),
            BlockNode::Admonition(a) => collect_quote_fence_warnings(&a.children, to_byte, out),
            BlockNode::Directive(d) => collect_quote_fence_warnings(&d.children, to_byte, out),
            BlockNode::Div(d) => collect_quote_fence_warnings(&d.children, to_byte, out),
            BlockNode::Section(d) => collect_quote_fence_warnings(&d.children, to_byte, out),
            BlockNode::Table(t) => {
                for row in &t.rows {
                    for cell in &row.cells {
                        if let Some(blocks) = &cell.blocks {
                            collect_quote_fence_warnings(blocks, to_byte, out);
                        }
                    }
                }
            }
            BlockNode::LineBlock(lb) => collect_quote_fence_warnings(&lb.children, to_byte, out),
            BlockNode::FigureGroup(g) => collect_quote_fence_warnings(&g.children, to_byte, out),
            BlockNode::ExtensionCarrier(e) => {
                collect_quote_fence_warnings(&e.children, to_byte, out)
            }
            BlockNode::List(l) => {
                for item in &l.items {
                    collect_quote_fence_warnings(&item.children, to_byte, out);
                }
            }
            BlockNode::DefinitionList(dl) => {
                for item in &dl.items {
                    for def in &item.definitions {
                        collect_quote_fence_warnings(&def.children, to_byte, out);
                    }
                }
            }
            BlockNode::Figure(f) => {
                if let FigureTarget::BlockQuote(b) = &*f.target {
                    collect_quote_fence_warnings(&b.children, to_byte, out);
                }
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
                pos.clone(),
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
            pos.clone(),
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
            report("heading", &n.attrs, n.pos.clone(), visit);
            walk_inlines(&n.children, visit);
        }
        BlockNode::CitationDefinition(n) => {
            report("citation_definition", &n.attrs, n.pos.clone(), visit);
            walk_inlines(&n.children, visit);
        }
        BlockNode::Paragraph(n) => {
            report("paragraph", &n.attrs, n.pos.clone(), visit);
            walk_inlines(&n.children, visit);
        }
        BlockNode::CodeBlock(n) => report("code_block", &n.attrs, n.pos.clone(), visit),
        BlockNode::List(n) => {
            report("list", &n.attrs, n.pos.clone(), visit);
            for item in &n.items {
                report("list_item", &item.attrs, item.pos.clone(), visit);
                walk_blocks(&item.children, visit);
            }
        }
        BlockNode::BlockQuote(n) => walk_block_quote(n, visit),
        BlockNode::Table(n) => walk_table(n, visit),
        BlockNode::Admonition(n) => {
            report("admonition", &n.attrs, n.pos.clone(), visit);
            if let Some(title) = &n.title {
                walk_inlines(title, visit);
            }
            walk_blocks(&n.children, visit);
        }
        BlockNode::Directive(n) => {
            report("directive", &n.attrs, n.pos.clone(), visit);
            if let Some(title) = &n.title {
                walk_inlines(title, visit);
            }
            walk_blocks(&n.children, visit);
        }
        BlockNode::Div(n) => {
            report("div", &n.attrs, n.pos.clone(), visit);
            walk_blocks(&n.children, visit);
        }
        BlockNode::Section(n) => {
            report("section", &n.attrs, n.pos.clone(), visit);
            walk_blocks(&n.children, visit);
        }
        BlockNode::LineBlock(n) => {
            report("line_block", &n.attrs, n.pos.clone(), visit);
            walk_blocks(&n.children, visit);
        }
        BlockNode::DefinitionList(n) => {
            report("definition_list", &n.attrs, n.pos.clone(), visit);
            for item in &n.items {
                for term in &item.terms {
                    report("definition_term", &term.attrs, term.pos.clone(), visit);
                    walk_inlines(&term.children, visit);
                }
                for def in &item.definitions {
                    report("definition_description", &def.attrs, def.pos.clone(), visit);
                    walk_blocks(&def.children, visit);
                }
            }
        }
        BlockNode::Figure(n) => {
            report("figure", &n.attrs, n.pos.clone(), visit);
            match &*n.target {
                FigureTarget::Image(image) => {
                    report("image", &image.attrs, image.pos.clone(), visit)
                }
                FigureTarget::BlockQuote(quote) => walk_block_quote(quote, visit),
                FigureTarget::Table(table) => walk_table(table, visit),
                FigureTarget::CodeBlock(block) => {
                    report("code_block", &block.attrs, block.pos.clone(), visit)
                }
                FigureTarget::Paragraph(para) => {
                    report("paragraph", &para.attrs, para.pos.clone(), visit);
                    walk_inlines(&para.children, visit);
                }
            }
            walk_inlines(&n.caption, visit);
            if let Some(short) = &n.short_caption {
                walk_inlines(short, visit);
            }
        }
        BlockNode::FigureGroup(n) => {
            report("figure_group", &n.attrs, n.pos.clone(), visit);
            walk_blocks(&n.children, visit);
            if let Some(caption) = &n.caption {
                walk_inlines(caption, visit);
            }
        }
        // Carries no `attrs` field and no children.
        BlockNode::AbbreviationDef(_) => {}
        BlockNode::LinkReferenceDefinition(n) => {
            report("link_reference_definition", &n.attrs, n.pos.clone(), visit)
        }
        // Verbatim: no `attrs` field, and its content is not markup.
        BlockNode::RawBlock(_) => {}
        BlockNode::Comment(_) => {}
        BlockNode::BlockExtension(n) => {
            report("block_extension", &n.attrs, n.pos.clone(), visit);
            walk_blocks(n.fallback_slice(), visit);
        }
        // Reported under the feature that gates it - see
        // `profile::canonical_block_type`. The carrier is not the schema's
        // `block_extension`; it never reaches the wire under that name.
        BlockNode::ExtensionCarrier(n) => {
            report("inline_extension", &n.attrs, n.pos.clone(), visit);
            if let Some(summary) = &n.summary {
                walk_inlines(summary, visit);
            }
            walk_blocks(&n.children, visit);
        }
        BlockNode::BlockImage(n) => report("image", &n.attrs, n.pos.clone(), visit),
        BlockNode::ThematicBreak(n) => report("thematic_break", &n.attrs, n.pos.clone(), visit),
    }
}

fn walk_block_quote(n: &BlockQuote, visit: &mut Visit<'_>) {
    report("block_quote", &n.attrs, n.pos.clone(), visit);
    walk_blocks(&n.children, visit);
}

fn walk_table(n: &Table, visit: &mut Visit<'_>) {
    report("table", &n.attrs, n.pos.clone(), visit);
    if let Some(caption) = &n.caption {
        walk_inlines(caption, visit);
    }
    if let Some(short) = &n.short_caption {
        walk_inlines(short, visit);
    }
    for row in &n.rows {
        report("table_row", &row.attrs, row.pos.clone(), visit);
        for cell in &row.cells {
            report("table_cell", &cell.attrs, cell.pos.clone(), visit);
            walk_inlines(&cell.children, visit);
            if let Some(blocks) = &cell.blocks {
                walk_blocks(blocks, visit);
            }
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
        | InlineNode::NonBreakingSpace(_)
        | InlineNode::SoftBreak(_)
        | InlineNode::HardBreak(_)
        | InlineNode::CriticComment(_)
        | InlineNode::Comment(_) => {}
        InlineNode::Emphasis(n) => {
            report(emphasis_type(n.kind), &n.attrs, n.pos.clone(), visit);
            walk_inlines(&n.children, visit);
        }
        InlineNode::Code(n) => report("code", &n.attrs, n.pos.clone(), visit),
        InlineNode::Link(n) => {
            report("link", &n.attrs, n.pos.clone(), visit);
            walk_inlines(&n.children, visit);
        }
        InlineNode::Image(n) => report("image", &n.attrs, n.pos.clone(), visit),
        InlineNode::Span(n) => {
            report("span", &n.attrs, n.pos.clone(), visit);
            walk_inlines(&n.children, visit);
        }
        InlineNode::Ruby(n) => {
            report("ruby", &n.attrs, n.pos.clone(), visit);
            for pair in &n.pairs {
                walk_inlines(&pair.base, visit);
                walk_inlines(&pair.annotation, visit);
            }
        }
        InlineNode::Math(n) => report("math", &n.attrs, n.pos.clone(), visit),
        InlineNode::LiteralInline(n) => report("literal_inline", &n.attrs, n.pos.clone(), visit),
        InlineNode::Symbol(n) => report("symbol", &n.attrs, n.pos.clone(), visit),
        InlineNode::AutoLink(n) => report("autolink", &n.attrs, n.pos.clone(), visit),
        InlineNode::Mention(n) => report("mention", &n.attrs, n.pos.clone(), visit),
        InlineNode::Tag(n) => report("tag", &n.attrs, n.pos.clone(), visit),
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
            report("inline_extension", &n.attrs, n.pos.clone(), visit);
            walk_inlines(&n.children, visit);
        }
        InlineNode::Footnote(n) => {
            let node_type = if n.inline.is_some() {
                "inline_footnote"
            } else {
                "footnote_ref"
            };
            report(node_type, &n.attrs, n.pos.clone(), visit);
            if let Some(inline) = &n.inline {
                walk_inlines(inline, visit);
            }
        }
        InlineNode::CriticInsert(n) => {
            report("insert", &n.attrs, n.pos.clone(), visit);
            walk_inlines(&n.children, visit);
        }
        InlineNode::CriticDelete(n) => {
            report("delete", &n.attrs, n.pos.clone(), visit);
            walk_inlines(&n.children, visit);
        }
        InlineNode::CriticSubstitute(n) => {
            walk_inlines(&n.old, visit);
            walk_inlines(&n.new, visit);
        }
    }
}
