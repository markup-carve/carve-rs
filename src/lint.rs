//! Source lint under C2-W-103 to C2-W-109.

use crate::diagnostic::Diagnostic;
use crate::source::Source;
use serde_json::Value;
use std::cell::OnceCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::OnceLock;

mod controls;
mod data_interpreter;
mod data_model;
mod data_rules;
mod includes;
mod legacy;
mod lines;
mod nodes;
use crate::view::V;

/// Rule switches and platform selection for a lint run.
#[derive(Debug, Clone, Default)]
pub struct LintOptions {
    /// Heading id policy used to resolve fragment targets.
    pub heading_ids: crate::options::HeadingIds,
    /// A rule id mapped to `on` or `off`.
    pub rules: HashMap<String, String>,
    /// Platform names, currently `github`.
    pub platforms: Vec<String>,
    /// Registered extensions that may contribute generated IDs.
    pub extensions: Vec<String>,
    /// Include expansion for lint of an entry document (C2-N-085).
    pub includes: Option<crate::include::Includes>,
}

fn catalog() -> &'static Value {
    static CATALOG: OnceLock<Value> = OnceLock::new();
    CATALOG.get_or_init(|| {
        serde_json::from_str(include_str!("../spec-data/lint-rules.json")).unwrap_or(Value::Null)
    })
}

fn active(code: &str, opts: &LintOptions) -> bool {
    if data_rules::data_mode() && data_rules::modeled_ids().contains(code) {
        return false;
    }
    active_catalog(code, opts)
}

fn active_catalog(code: &str, opts: &LintOptions) -> bool {
    let Some(row) = catalog()["rules"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|rule| rule["id"] == code)
    else {
        return false;
    };
    if row["channel"] != "lint" && !row["channel"].is_null() {
        return false;
    }
    if matches!(row["status"].as_str(), Some("obsolete" | "void")) {
        return false;
    }
    if code.starts_with("platform-") && !opts.platforms.iter().any(|p| p == "github") {
        return false;
    }
    opts.rules.get(code).map_or_else(
        || row["default"].as_str().unwrap_or("on") == "on",
        |setting| setting == "on",
    )
}

/// Runs enabled lint rules and returns catalog diagnostics in source order.
#[must_use]
pub fn lint(source: &str, opts: &LintOptions) -> Vec<Diagnostic> {
    let parse = crate::Options {
        includes: opts.includes.clone(),
        heading_ids: opts.heading_ids,
        ..crate::Options::default()
    };
    let same = opts.includes.is_none() && opts.heading_ids == crate::options::HeadingIds::default();
    let (mut out, diagnostics) = crate::with_lint_tree(
        source,
        &crate::Options::default(),
        &parse,
        same,
        |tree, arena| lint_core(source, opts, tree, arena),
    );
    out.extend(diagnostics.into_iter().filter(|diagnostic| {
        !data_rules::data_mode() || !data_rules::modeled_ids().contains(diagnostic.code)
    }));
    if opts.includes.is_some() {
        includes::scan(source, opts, &mut out);
    }
    out.retain(|finding| {
        (data_rules::data_mode() && data_rules::modeled_ids().contains(finding.code))
            || finding.code != "table-shaped-paragraph"
            || !finding.pos.is_some_and(|pos| {
                source
                    .get(pos.start_offset as usize..pos.end_offset as usize)
                    .is_some_and(|span| span.contains("%%"))
            })
    });
    select_message_variants(source, &mut out);
    out
}

/// C2-W-143: whether the line after `line_end` is blank for the rules inside the quotes that open before `start` on
/// its line, that is, holds only spaces and tabs behind that many quote marks; `>>` at column 1 is text (C2-B-031).
fn blank_within(source: &str, line_start: usize, start: usize, line_end: usize) -> bool {
    let depth = source
        .get(line_start..start)
        .unwrap_or("")
        .bytes()
        .filter(|byte| *byte == b'>')
        .count();
    let Some(tail) = source
        .get(line_end..)
        .and_then(|tail| tail.strip_prefix('\n'))
        .filter(|tail| !tail.is_empty())
    else {
        return false;
    };
    let mut rest = tail.split('\n').next().unwrap_or("").trim_end_matches('\r');
    for _ in 0..depth {
        let trimmed = rest.trim_start_matches([' ', '\t']);
        match trimmed.strip_prefix('>') {
            Some(after) if after.is_empty() || after.starts_with(' ') => rest = after,
            // A line that lacks a quote mark of the attribute line is outside its quote, which ended above it.
            _ => return false,
        }
    }
    rest.trim_matches([' ', '\t']).is_empty()
}

/// Selects catalog wording whose trigger depends on the surrounding source,
/// after legacy, line and node scanners have contributed their findings.
fn select_message_variants(source: &str, findings: &mut [Diagnostic]) {
    for finding in findings {
        if data_rules::data_mode() && data_rules::modeled_ids().contains(finding.code) {
            continue;
        }
        let Some(pos) = finding.pos else { continue };
        let start = usize::try_from(pos.start_offset).unwrap_or(usize::MAX);
        let Some(prefix) = source.get(..start) else {
            continue;
        };
        let line_start = prefix.rfind('\n').map_or(0, |at| at + 1);
        let line_end = source
            .get(start..)
            .and_then(|tail| tail.find('\n').map(|at| start + at))
            .unwrap_or(source.len());
        let line = source.get(line_start..line_end).unwrap_or("").trim();
        let previous = prefix
            .get(..line_start)
            .unwrap_or("")
            .trim_end_matches(['\r', '\n'])
            .rsplit('\n')
            .next()
            .unwrap_or("")
            .trim_start();
        let blank_next = blank_within(source, line_start, start, line_end);
        let variant = match finding.code {
            "list-marker-without-blank-line"
                if previous.starts_with(":: ") || previous.starts_with(": ") =>
            {
                Some(1)
            }
            "unattached-block-attribute" if blank_next => Some(1),
            "region-marker-renders-as-text" if line.starts_with("%%") => Some(1),
            _ => None,
        };
        if let Some(variant) = variant {
            finding.message = Diagnostic::nth(
                finding.code,
                variant,
                &[],
                finding.file.as_deref(),
                finding.pos,
            )
            .message;
        }
    }
}

fn lint_core<'a>(
    source: &str,
    opts: &LintOptions,
    tree: Option<V<'a>>,
    arena: &'a crate::Arena,
) -> Vec<Diagnostic> {
    let parsed = Source::new(source);
    let positions = Ranges::new(source, tree.map_or_else(Vec::new, node_ranges));
    let mut out = Vec::new();
    for warning in tree.map_or_else(Vec::new, |tree| legacy::warnings(source, tree)) {
        if skip_legacy_warning(&warning, source, opts, &parsed, &positions) {
            continue;
        }
        out.push(convert_legacy_warning(warning, source, &parsed, &positions));
    }
    lines::scan(source, opts, &positions.all, &mut out);
    if let Some(tree) = tree {
        nodes::scan(source, opts, tree, arena, &mut out);
    }
    colon_fence_mismatches(source, opts, &positions, &mut out);
    referenced_footnote_platform_tokens(source, opts, &positions, &mut out);
    if data_rules::data_mode() {
        if tree.is_some() {
            let records = data_model::records(source, &positions.all);
            out.retain(|finding| !data_rules::modeled_ids().contains(finding.code));
            out.extend(data_rules::run(&records, source, opts));
        }
    }
    let invalid_tables: Vec<_> = out
        .iter()
        .filter(|finding| finding.code == "table-attribute-invalid")
        .filter_map(|finding| finding.path.as_deref())
        .map(str::to_string)
        .collect();
    out.retain(|finding| {
        !matches!(
            finding.code,
            "table-column-arity" | "table-column-overlap" | "table-width-total"
        ) || !invalid_tables.iter().any(|path| {
            finding
                .path
                .as_deref()
                .is_some_and(|actual| actual.starts_with(path))
        })
    });
    out.sort_by_key(|d| d.pos.map_or(0, |p| p.start_offset));
    if let Some(tree) = tree {
        controls::filter(source, out, tree)
    } else {
        out
    }
}

fn skip_legacy_warning(
    warning: &legacy::Warning,
    source: &str,
    opts: &LintOptions,
    parsed: &Source<'_>,
    positions: &Ranges<'_>,
) -> bool {
    if !active(warning.rule, opts) {
        return true;
    }
    if warning.rule == "broken-fragment-link"
        && (!opts.extensions.is_empty() || opts.heading_ids != crate::options::HeadingIds::KeepCase)
    {
        return true;
    }
    if warning.rule == "table-shaped-paragraph"
        && source
            .get(warning.start..warning.end)
            .is_some_and(|line| line.contains("%%"))
    {
        return true;
    }
    if warning.rule == "table-marker-run-padding"
        && !table_marker_run_padding(source, warning.start)
    {
        return true;
    }
    if warning.rule == "bare-html-tag" && ineligible_html_tag(warning, positions) {
        return true;
    }
    if warning.rule == "list-marker-below-content-column" {
        if source
            .get(warning.start..)
            .is_some_and(|tail| tail.starts_with('+'))
        {
            return true;
        }
        let column = parsed.pos(warning.start, warning.start).start_column;
        let previous = source
            .get(..warning.start)
            .unwrap_or("")
            .trim_end_matches([' ', '\t', '\n']);
        if !previous.len().checked_sub(1).is_some_and(|at| {
            positions.around(at).any(|range| {
                range.kind == "list" && parsed.pos(range.start, range.start).start_column > column
            })
        }) {
            return true;
        }
    }
    skip_legacy_context(warning, source, parsed, positions)
}

fn ineligible_html_tag(warning: &legacy::Warning, positions: &Ranges<'_>) -> bool {
    positions.around(warning.start).any(|range| {
        matches!(range.kind.as_str(), "comment" | "caption")
            && range.start <= warning.start
            && warning.end <= range.end
    })
}

fn skip_legacy_context(
    warning: &legacy::Warning,
    source: &str,
    parsed: &Source<'_>,
    positions: &Ranges<'_>,
) -> bool {
    if warning.rule == "footnote-labels-differ-only-in-whitespace"
        && parsed
            .text()
            .get(parsed.line_range(parsed.line_of(warning.start)).0..warning.start)
            .is_some_and(|prefix| prefix.trim_start().starts_with('>'))
    {
        return true;
    }
    if warning.rule == "bidi-control-in-source"
        && positions.around(warning.start).any(|range| {
            range.kind == "code_block"
                && range.start_line == parsed.line_of(warning.start) + 1
                && range.start <= warning.start
                && warning.start < range.end
        })
    {
        return true;
    }
    if warning.rule == "table-column-overlap"
        && overlap_in_centered_table(source, warning, positions)
    {
        return true;
    }
    if [
        "deprecated-cell-alignment",
        "empty-reference-definition",
        "carve-version-unsupported",
        "table-attribute-invalid",
        "table-row-cell-count",
        "thematic-break-spelling",
        "unclosed-inline-emphasis",
        "unresolved-footnote",
        "unresolved-reference-link",
        "broken-crossref",
        "markdown-strong-double-star",
        "blockquote-marker-without-space",
        "bidi-control-in-source",
        "table-shaped-paragraph",
        "unattached-block-attribute",
        "deprecated-bare-highlight",
        "deprecated-bare-strike",
        "deprecated-bare-underline",
        "fence-closer-indented",
        "block-opener-overindented",
        "block-marker-as-text",
        "comment-shaped-emphasis",
        "unused-link-reference-definition",
        "unused-footnote-definition",
        "unattached-inline-attribute",
        "emphasis-marker-before-attributes",
        "comment-in-reference-label",
        "uneven-list-indent",
        "markdown-definition-paragraph",
        "block-opener-without-blank-line",
        "list-marker-without-blank-line",
        "semantic-attribute-outside-span",
        "empty-include-path",
        "heading-trailing-attribute",
        "indented-code-block",
        "heading-closing-hashes",
        "setext-heading-underline",
        "list-item-block-overindented",
        "github-alert-marker",
        "description-separator-below-content-column",
        "colon-fence-length-mismatch",
        "heading-level-skip",
    ]
    .contains(&warning.rule)
    {
        return true;
    }
    false
}

fn overlap_in_centered_table(
    source: &str,
    warning: &legacy::Warning,
    positions: &Ranges<'_>,
) -> bool {
    positions.around(warning.start).any(|range| {
        range.kind == "table"
            && range.start <= warning.start
            && warning.start < range.end
            && source.get(range.start..range.end).is_some_and(|table| {
                table.contains("|={~}") && !table.contains("|={<}") && !table.contains("|={>}")
            })
    })
}

fn convert_legacy_warning(
    warning: legacy::Warning,
    source: &str,
    parsed: &Source<'_>,
    positions: &Ranges<'_>,
) -> Diagnostic {
    let mut diagnostic = Diagnostic::new(
        warning.rule,
        &[],
        None,
        Some(parsed.pos(warning.start, warning.end)),
    );
    if diagnostic.message.is_empty() || catalog_message_has_slots(warning.rule) {
        diagnostic.message = warning.message;
    }
    normalize_root_message(
        &mut diagnostic,
        source,
        parsed,
        positions,
        warning.start,
        warning.end,
    );
    diagnostic.path = path_at(positions, warning.start, warning.end);
    if warning.rule == "delimited-table-wide-row"
        && let Some(table) = positions.around(warning.start).find(|range| {
            range.kind == "table" && range.start <= warning.start && warning.end <= range.end
        })
    {
        diagnostic.pos = Some(parsed.pos(table.start, table.end));
    }
    if warning.rule == "list-marker-below-content-column" {
        let end = source
            .get(warning.start..)
            .and_then(|tail| tail.find('\n'))
            .map_or(source.len(), |length| warning.start + length);
        diagnostic.pos = Some(parsed.pos(warning.start, end));
    }
    if warning.rule == "figure-group-panel-number"
        && let Some(panel) = positions.last_image_before(warning.start)
    {
        diagnostic.pos = Some(parsed.pos(panel.start, panel.start + 1));
    }
    diagnostic
}
fn referenced_footnote_platform_tokens(
    source: &str,
    opts: &LintOptions,
    ranges: &Ranges<'_>,
    output: &mut Vec<Diagnostic>,
) {
    if !opts.platforms.iter().any(|platform| platform == "github") {
        return;
    }
    let positions = Source::new(source);
    let references: HashSet<&str> = ranges
        .all
        .iter()
        .filter_map(|range| range.footnote_key.as_deref())
        .collect();
    let mut offset = 0;
    for piece in source.split_inclusive('\n') {
        let line = piece.trim_end_matches('\n');
        if let Some(rest) = line.strip_prefix("[^")
            && let Some((label, body)) = rest.split_once("]:")
            && references.contains(label)
            && !opaque_definition(ranges, offset)
        {
            let body_start = offset + 2 + label.len() + 2;
            let trimmed = body.trim_start();
            let start = body_start + body.len() - trimmed.len();
            let (code, token) = if let Some(after_hash) = trimmed.strip_prefix('#') {
                (
                    "platform-issue-reference",
                    trimmed
                        .get(..1 + after_hash.bytes().take_while(u8::is_ascii_digit).count())
                        .unwrap_or(""),
                )
            } else if trimmed.starts_with('@') {
                (
                    "platform-mention-token",
                    trimmed.split_whitespace().next().unwrap_or(""),
                )
            } else {
                offset += piece.len();
                continue;
            };
            if token.len() > 1 && active(code, opts) {
                let mut diagnostic = Diagnostic::new(
                    code,
                    &[("host", "GitHub"), ("text", token), ("what", "a mention")],
                    None,
                    Some(positions.pos(start, start + token.len())),
                );
                diagnostic.path = Some(String::new());
                output.push(diagnostic);
            }
        }
        offset += piece.len();
    }
}

fn opaque_definition(ranges: &Ranges<'_>, offset: usize) -> bool {
    ranges.around(offset).any(|range| {
        ["code_block", "raw_block", "frontmatter", "comment"].contains(&range.kind.as_str())
            && range.start <= offset
            && offset < range.end
    })
}

fn table_marker_run_padding(source: &str, at: usize) -> bool {
    let start = source
        .get(..at)
        .and_then(|s| s.rfind('\n'))
        .map_or(0, |index| index + 1);
    let end = source
        .get(at..)
        .and_then(|s| s.find('\n'))
        .map_or(source.len(), |index| at + index);
    source
        .get(start..end)
        .unwrap_or("")
        .as_bytes()
        .windows(2)
        .any(|pair| {
            pair.first() == Some(&b'}')
                && pair
                    .get(1)
                    .is_some_and(|next| matches!(next, b'<' | b'>' | b'~'))
        })
}

fn normalize_root_message(
    diagnostic: &mut Diagnostic,
    source: &str,
    parsed: &Source<'_>,
    ranges: &Ranges<'_>,
    start: usize,
    end: usize,
) {
    let raw = source.get(start..end).unwrap_or("");
    let (code, message, values): (&'static str, usize, Vec<(&str, &str)>) = match diagnostic.code {
        "bidi-control-in-source" => {
            let point = raw
                .chars()
                .next()
                .map_or(String::new(), |c| format!("{:04X}", c as u32));
            diagnostic.message =
                Diagnostic::new("bidi-control-in-source", &[("code", &point)], None, None).message;
            return;
        }
        "broken-crossref" => {
            let target = raw
                .strip_prefix("</#")
                .and_then(|s| s.strip_suffix('>'))
                .unwrap_or("");
            ("broken-crossref", 2, vec![("target", target)])
        }
        "unresolved-reference-link" => ("unresolved-reference-link", 1, vec![("rawRef", raw)]),
        "unused-footnote-definition" => {
            let label = raw
                .strip_prefix("[^")
                .and_then(|s| s.split_once(']'))
                .map_or("", |(id, _)| id);
            ("unused-footnote-definition", 0, vec![("label", label)])
        }
        "footnote-labels-differ-only-in-whitespace" => {
            let label = raw
                .strip_prefix("[^")
                .and_then(|text| text.split_once(']'))
                .map_or("", |(label, _)| label);
            let key = crate::chars::fold_label(label);
            let twin = footnote_twin(source.get(..start).unwrap_or(""), &key).unwrap_or_default();
            diagnostic.message = Diagnostic::new(
                "footnote-labels-differ-only-in-whitespace",
                &[("label", label), ("twin", &twin), ("key", &key)],
                None,
                None,
            )
            .message;
            return;
        }
        "table-marker-run-padding" => {
            let (head, tail) = raw.split_once('}').unwrap_or((raw, ""));
            let block = format!("{head}}}");
            let marker = tail
                .chars()
                .next()
                .or_else(|| source.get(end..)?.chars().next())
                .unwrap_or(' ');
            let marker = marker.to_string();
            let spelling = format!("{block}{marker}");
            let contents = block
                .strip_prefix('{')
                .and_then(|value| value.strip_suffix('}'))
                .unwrap_or("");
            let corrected_prefix = format!("{{{marker} {contents}}}");
            diagnostic.message = Diagnostic::new(
                "table-marker-run-padding",
                &[
                    ("spelling", &spelling),
                    ("marker", &marker),
                    ("block", &block),
                    ("correctedPrefix", &corrected_prefix),
                ],
                None,
                None,
            )
            .message;
            return;
        }
        "colon-fence-length-mismatch" => {
            diagnostic.message = colon_fence_message(source, ranges, start, end, raw);
            return;
        }
        "list-marker-below-content-column" => {
            let (column, base, content) = list_columns(source, parsed, ranges, start);
            diagnostic.message = Diagnostic::new(
                "list-marker-below-content-column",
                &[
                    ("column", &column.to_string()),
                    ("baseColumn", &base.to_string()),
                    ("contentColumn", &content.to_string()),
                ],
                None,
                None,
            )
            .message;
            return;
        }
        "list-item-block-overindented" => {
            let content = list_columns(source, parsed, ranges, start).2.to_string();
            diagnostic.message = Diagnostic::new(
                "list-item-block-overindented",
                &[("contentColumn", &content)],
                None,
                None,
            )
            .message;
            return;
        }
        _ => return,
    };
    diagnostic.message = Diagnostic::nth(code, message, &values, None, None).message;
}

fn colon_fence_message(
    source: &str,
    ranges: &Ranges<'_>,
    start: usize,
    end: usize,
    raw: &str,
) -> String {
    let authored = raw.len().to_string();
    let before = source.get(..start).unwrap_or("");
    let opener = last_colon_opener(before);
    let expected = before
        .get(opener..)
        .unwrap_or("")
        .chars()
        .take_while(|c| *c == ':')
        .count()
        .to_string();
    let line = (ranges.newlines_before(opener) + 1).to_string();
    let column = (opener
        - before
            .get(..opener)
            .unwrap_or("")
            .rfind('\n')
            .map_or(0, |at| at + 1)
        + 1)
    .to_string();
    let authored_width = authored.parse::<usize>().unwrap_or(0);
    let expected_width = expected.parse::<usize>().unwrap_or(0);
    let closes_enclosing = ranges.around(opener).any(|range| {
        range.start < opener
            && range.end >= end
            && matches!(
                range.kind.as_str(),
                "div"
                    | "directive"
                    | "admonition"
                    | "figure_group"
                    | "line_block"
                    | "hard_break_block"
            )
            && source
                .get(range.start..)
                .unwrap_or("")
                .bytes()
                .take_while(|byte| *byte == b':')
                .count()
                <= authored_width
    });
    let variant = if authored_width > expected_width {
        2
    } else {
        usize::from(closes_enclosing)
    };
    Diagnostic::nth(
        "colon-fence-length-mismatch",
        variant,
        &[
            ("authoredWidth", &authored),
            ("expectedWidth", &expected),
            ("kind", "div"),
            ("line", &line),
            ("column", &column),
            ("outcome", "closes the container"),
        ],
        None,
        None,
    )
    .message
}

/// A run inside an opaque block, or one that `run_in_inner_owner` keeps from the div, is not compared with it.
fn run_is_skipped(
    source: &str,
    ranges: &Ranges<'_>,
    at: usize,
    end: usize,
    container: &str,
) -> bool {
    ranges.around(at).any(|range| {
        range.start <= at
            && end <= range.end
            && matches!(range.kind.as_str(), "code_block" | "raw_block" | "comment")
    }) || run_in_inner_owner(source, ranges, at, container)
}

/// The run cannot close a div outside its own quote, item, footnote or description (C2-B-109, carve#6194), so that div is
/// not compared with it.
fn run_in_inner_owner(source: &str, ranges: &Ranges<'_>, at: usize, container: &str) -> bool {
    let inner = format!("{container}/");
    ranges.around(at).any(|range| {
        range.start < at
            && at <= range.end
            && range.path.starts_with(&inner)
            && matches!(
                range.kind.as_str(),
                "block_quote"
                    | "list_item"
                    | "footnote"
                    | "definition_description"
                    | "citation_definition"
            )
            && !source.get(range.start..).unwrap_or("").starts_with(":::")
    })
}

fn colon_fence_mismatches(
    source: &str,
    opts: &LintOptions,
    ranges: &Ranges<'_>,
    output: &mut Vec<Diagnostic>,
) {
    if !active("colon-fence-length-mismatch", opts) {
        return;
    }
    let positions = Source::new(source);
    let mut line_start = 0;
    for piece in source.split_inclusive('\n') {
        let line = piece.trim_end_matches(['\n', '\r']);
        let mut tail = line.trim_start_matches([' ', '\t']);
        while let Some(rest) = tail.strip_prefix("> ") {
            tail = rest.trim_start_matches([' ', '\t']);
        }
        let candidate = tail.trim_end_matches([' ', '\t']);
        if candidate.len() >= 3 && candidate.bytes().all(|byte| byte == b':') {
            let at = line_start + line.len() - tail.len();
            let end = at + candidate.len();
            if ranges.around(at).any(|range| {
                range.kind == "paragraph" && range.start < line_start && at < range.end
            }) {
                line_start += piece.len();
                continue;
            }
            let container = colon_candidate_container(source, ranges, line_start, at);
            if let Some(container) = container {
                let width = colon_opener_at(source, container.start)
                    .and_then(|open| source.get(open..))
                    .unwrap_or("")
                    .bytes()
                    .take_while(|byte| *byte == b':')
                    .count();
                let skipped = run_is_skipped(source, ranges, at, end, &container.path);
                if width >= 3 && width != candidate.len() && !skipped {
                    let mut finding = Diagnostic::new(
                        "colon-fence-length-mismatch",
                        &[],
                        None,
                        Some(positions.pos(at, end)),
                    );
                    finding.message = colon_fence_message(source, ranges, at, end, candidate);
                    finding.path = Some(container.path.to_string());
                    output.push(finding);
                }
            }
        }
        line_start += piece.len();
    }
}

fn colon_candidate_container<'r>(
    source: &str,
    ranges: &'r Ranges<'_>,
    line_start: usize,
    at: usize,
) -> Option<&'r Range> {
    let previous_at = source
        .get(..line_start)
        .unwrap_or("")
        .trim_end_matches([' ', '\t', '\r', '\n'])
        .len()
        .checked_sub(1)?;
    ranges
        .around(previous_at)
        .filter(|range| colon_range_open_at(source, ranges, range, at))
        .max_by_key(|range| range.path.len())
}

fn colon_range_open_at(source: &str, ranges: &Ranges<'_>, range: &Range, at: usize) -> bool {
    if !matches!(
        range.kind.as_str(),
        "div" | "directive" | "admonition" | "figure_group" | "line_block" | "hard_break_block"
    ) || range.start >= at
        || colon_opener_at(source, range.start) == Some(at)
    {
        return false;
    }
    !colon_crosses_owner(ranges, range, at)
        && (at <= range.end
            || (!colon_closed_at_end(source, range) && colon_parent_survives(ranges, range, at)))
}

fn colon_crosses_owner(ranges: &Ranges<'_>, range: &Range, at: usize) -> bool {
    ranges.around(at).any(|current| {
        matches!(
            current.kind.as_str(),
            "list_item" | "block_quote" | "footnote" | "definition_description"
        ) && current
            .path
            .strip_prefix(range.path.as_str())
            .is_some_and(|rest| rest.starts_with("/children/"))
            && current.start <= at
            && at <= current.end
    })
}

fn colon_parent_survives(ranges: &Ranges<'_>, range: &Range, at: usize) -> bool {
    range
        .path
        .rsplit_once("/children/")
        .is_some_and(|(parent, _)| {
            ranges
                .around(at)
                .any(|current| current.path == parent && current.start <= at && at <= current.end)
        })
}

/// The opener of an attributed colon container follows its attribute lines.
fn colon_opener_at(source: &str, start: usize) -> Option<usize> {
    let mut at = start;
    loop {
        let raw_line = source.get(at..)?.split('\n').next()?;
        let line = raw_line.trim_end_matches('\r');
        let trimmed = line.trim_start_matches([' ', '\t']);
        if trimmed.starts_with(":::") {
            return Some(at + line.len() - trimmed.len());
        }
        if !trimmed.starts_with('{') || !trimmed.ends_with('}') {
            return None;
        }
        at += raw_line.len() + 1;
    }
}

fn colon_closed_at_end(source: &str, range: &Range) -> bool {
    let Some(open) = colon_opener_at(source, range.start) else {
        return false;
    };
    let width = source
        .get(open..)
        .unwrap_or("")
        .bytes()
        .take_while(|b| *b == b':')
        .count();
    let final_line = source
        .get(..range.end)
        .unwrap_or("")
        .rsplit('\n')
        .next()
        .unwrap_or("");
    let run = final_line.trim_start_matches([' ', '\t', '>']).trim();
    run.len() >= width && run.bytes().all(|b| b == b':')
}

fn list_columns(
    source: &str,
    positions: &Source<'_>,
    ranges: &Ranges<'_>,
    at: usize,
) -> (u32, u32, u32) {
    let column = positions.pos(at, at).start_column;
    let mut columns: Vec<u32> = ranges
        .around(at)
        .filter(|range| range.kind == "list" && range.start <= at && at <= range.end)
        .map(|range| positions.pos(range.start, range.start).start_column)
        .collect();
    columns.sort_unstable();
    let base = columns
        .iter()
        .copied()
        .filter(|value| *value < column)
        .max()
        .unwrap_or(1);
    let content = columns
        .iter()
        .copied()
        .filter(|value| *value > column)
        .min()
        .unwrap_or_else(|| marker_content_column(source, positions, ranges, at, base));
    (column, base, content)
}

fn marker_content_column(
    source: &str,
    positions: &Source<'_>,
    ranges: &Ranges<'_>,
    at: usize,
    base: u32,
) -> u32 {
    ranges
        .around(at)
        .filter(|range| range.kind == "list" && range.start <= at && at <= range.end)
        .find(|range| positions.pos(range.start, range.start).start_column == base)
        .map_or(base.saturating_add(2), |range| {
            let marker = source
                .get(range.start..)
                .unwrap_or("")
                .split('\n')
                .next()
                .unwrap_or("");
            let width = marker
                .bytes()
                .take_while(|byte| !matches!(byte, b' ' | b'\t'))
                .count();
            base.saturating_add(u32::try_from(width + 1).unwrap_or(u32::MAX))
        })
}

fn last_colon_opener(before: &str) -> usize {
    before
        .rmatch_indices(":::")
        .find_map(|(at, _)| {
            let mut at = at;
            while at > 0 && before.as_bytes().get(at - 1) == Some(&b':') {
                at -= 1;
            }
            let run = before
                .get(at..)
                .unwrap_or("")
                .bytes()
                .take_while(|byte| *byte == b':')
                .count();
            let next = before.as_bytes().get(at + run).copied();
            next.is_none_or(|byte| matches!(byte, b' ' | b'\t' | b'\n'))
                .then_some(at)
        })
        .unwrap_or(0)
}

fn footnote_twin(before: &str, key: &str) -> Option<String> {
    before.lines().find_map(|line| {
        let label = line.split_once("[^")?.1.split_once("]:")?.0;
        (crate::chars::fold_label(label) == key).then(|| label.to_string())
    })
}

fn catalog_message_has_slots(code: &str) -> bool {
    catalog()["rules"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|rule| rule["id"] == code)
        .and_then(|row| row["messages"].as_array())
        .and_then(|messages| messages.first())
        .and_then(Value::as_str)
        .is_some_and(|message| message.contains("${"))
}

/// Drops every finding that a later finding of the same code at `replacing`
/// overlaps: one pass in place of a scan of all findings per emit.
fn drop_replaced(output: &mut Vec<Diagnostic>, replacing: &[usize]) {
    let mut later: HashMap<&'static str, Vec<(u32, u32, usize)>> = HashMap::new();
    for &at in replacing {
        if let Some(finding) = output.get(at)
            && let Some(pos) = finding.pos
        {
            later
                .entry(finding.code)
                .or_default()
                .push((pos.start_offset, pos.end_offset, at));
        }
    }
    if later.is_empty() {
        return;
    }
    let mut dropped = vec![false; output.len()];
    for (code, mut spans) in later {
        // A Fenwick tree over the starts of the replacing findings holds the
        // furthest end; filled from the last finding back, a query sees only later ones.
        let mut starts: Vec<u32> = spans.iter().map(|span| span.0).collect();
        starts.sort_unstable();
        starts.dedup();
        let mut furthest = vec![0u32; starts.len() + 1];
        spans.sort_unstable_by_key(|span| span.2);
        let mut spans = spans.into_iter().rev().peekable();
        for (at, finding) in output.iter().enumerate().rev() {
            if finding.code == code
                && let Some(pos) = finding.pos
            {
                let mut slot = starts.partition_point(|start| *start < pos.end_offset);
                while slot > 0 {
                    if furthest
                        .get(slot)
                        .is_some_and(|end| *end > pos.start_offset)
                    {
                        if let Some(flag) = dropped.get_mut(at) {
                            *flag = true;
                        }
                        break;
                    }
                    slot &= slot - 1;
                }
            }
            while let Some((start, end, _)) = spans.next_if(|span| span.2 == at) {
                let mut slot = starts.partition_point(|s| *s < start) + 1;
                while let Some(slot_end) = furthest.get_mut(slot) {
                    *slot_end = (*slot_end).max(end);
                    slot += slot & slot.wrapping_neg();
                }
            }
        }
    }
    let mut flags = dropped.into_iter();
    output.retain(|_| !flags.next().unwrap_or(false));
}

/// The node ranges, with the ranges that touch each source line, so a lookup
/// at one offset reads the ranges around it instead of all of them.
struct Ranges<'a> {
    all: Vec<Range>,
    source: &'a str,
    index: OnceCell<RangeIndex>,
}

struct RangeIndex {
    line_starts: Vec<usize>,
    by_line: Vec<Vec<usize>>,
    /// Image starts with their range index, in ascending order.
    images: Vec<(usize, usize)>,
}

impl<'a> Ranges<'a> {
    const fn new(source: &'a str, all: Vec<Range>) -> Self {
        Self {
            all,
            source,
            index: OnceCell::new(),
        }
    }

    fn index(&self) -> &RangeIndex {
        self.index.get_or_init(|| {
            let mut line_starts = vec![0];
            line_starts.extend(self.source.match_indices('\n').map(|(at, _)| at + 1));
            let line_of = |at: usize| line_starts.partition_point(|s| *s <= at).saturating_sub(1);
            let mut by_line = vec![Vec::new(); line_starts.len()];
            let mut images = Vec::new();
            for (at, range) in self.all.iter().enumerate() {
                let lines = line_of(range.start)..=line_of(range.end.max(range.start));
                for line in by_line.get_mut(lines).into_iter().flatten() {
                    line.push(at);
                }
                if range.kind == "image" {
                    images.push((range.start, at));
                }
            }
            images.sort_unstable();
            RangeIndex {
                line_starts,
                by_line,
                images,
            }
        })
    }

    /// The ranges on the line of `at`, in tree order: every range that holds `at` is among them.
    fn around(&self, at: usize) -> impl Iterator<Item = &Range> {
        let index = self.index();
        let line = index
            .line_starts
            .partition_point(|s| *s <= at)
            .saturating_sub(1);
        index
            .by_line
            .get(line)
            .into_iter()
            .flatten()
            .filter_map(|at| self.all.get(*at))
    }

    fn newlines_before(&self, at: usize) -> usize {
        self.index()
            .line_starts
            .partition_point(|s| *s <= at)
            .saturating_sub(1)
    }

    /// The image that starts last before `at`; the later one in tree order on a tie.
    fn last_image_before(&self, at: usize) -> Option<&Range> {
        let images = &self.index().images;
        let before = images.partition_point(|(start, _)| *start < at);
        before
            .checked_sub(1)
            .and_then(|last| images.get(last))
            .and_then(|(_, range)| self.all.get(*range))
    }
}

struct Range {
    start: usize,
    end: usize,
    path: LazyPath,
    kind: String,
    start_line: usize,
    end_line: usize,
    has_attrs: bool,
    form_block: bool,
    image_line: bool,
    footnote_key: Option<String>,
}

/// A node's pointer, written out only when a rule reads it.
struct LazyPath {
    step: Option<Rc<Step>>,
    len: usize,
    text: OnceCell<String>,
}

struct Step {
    segment: Segment,
    previous: Option<Rc<Step>>,
}

enum Segment {
    Key(Rc<str>),
    Index(usize),
}

impl LazyPath {
    const fn len(&self) -> usize {
        self.len
    }

    fn as_str(&self) -> &str {
        self.text.get_or_init(|| {
            use std::fmt::Write as _;
            let mut steps = Vec::new();
            let mut current = self.step.as_deref();
            while let Some(step) = current {
                steps.push(&step.segment);
                current = step.previous.as_deref();
            }
            let mut text = String::with_capacity(self.len);
            for segment in steps.into_iter().rev() {
                text.push('/');
                match segment {
                    Segment::Key(key) => text.push_str(key),
                    Segment::Index(index) => {
                        let _ = write!(text, "{index}");
                    }
                }
            }
            text
        })
    }
}

impl std::ops::Deref for LazyPath {
    type Target = str;

    fn deref(&self) -> &str {
        self.as_str()
    }
}

impl PartialEq<&str> for LazyPath {
    fn eq(&self, other: &&str) -> bool {
        self.len == other.len() && self.as_str() == *other
    }
}

fn node_ranges(tree: V<'_>) -> Vec<Range> {
    let mut out = Vec::new();
    let mut keys: HashMap<&str, Rc<str>> = HashMap::new();
    let mut pending: Vec<(V<'_>, Option<Rc<Step>>, usize)> = vec![(tree, None, 0)];
    while let Some((value, step, len)) = pending.pop() {
        let child = |segment: Segment, width: usize| {
            (
                Some(Rc::new(Step {
                    segment,
                    previous: step.clone(),
                })),
                len + 1 + width,
            )
        };
        if let Some(items) = value.as_array() {
            for (i, item) in items.iter().enumerate() {
                if item.is_container() {
                    let digits = i.checked_ilog10().map_or(1, |d| d as usize + 1);
                    let (next, width) = child(Segment::Index(i), digits);
                    pending.push((item, next, width));
                }
            }
        } else if value.is_object() {
            if let (Some(pos), Some(kind)) = (value.pos(), value.ty().as_str()) {
                let number =
                    |key: &str| usize::try_from(pos.at(key).as_u64().unwrap_or(0)).unwrap_or(0);
                out.push(Range {
                    start: number("startOffset"),
                    end: number("endOffset"),
                    path: LazyPath {
                        step: step.clone(),
                        len,
                        text: OnceCell::new(),
                    },
                    kind: kind.into(),
                    start_line: number("startLine"),
                    end_line: number("endLine"),
                    has_attrs: value.get("attrs").is_some(),
                    form_block: value.at("form").as_str() == Some("block"),
                    image_line: kind == "paragraph"
                        && value
                            .at("children")
                            .as_array()
                            .and_then(|children| children.single())
                            .is_some_and(|child| child.ty().as_str() == Some("image")),
                    footnote_key: (kind == "footnote_ref")
                        .then(|| value.at("key").as_str().map(str::to_string))
                        .flatten(),
                });
            }
            value.each(&mut |key, item| {
                if item.is_container() {
                    let name = keys.entry(key).or_insert_with(|| Rc::from(key));
                    let (next, width) = child(Segment::Key(Rc::clone(name)), key.len());
                    pending.push((item, next, width));
                }
            });
        }
    }
    out
}

fn path_at(ranges: &Ranges<'_>, start: usize, end: usize) -> Option<String> {
    ranges
        .around(start)
        .filter(|r| r.start <= start && end <= r.end)
        .max_by_key(|r| r.path.len())
        .map(|r| r.path.to_string())
}

#[cfg(test)]
mod tests {
    use super::{Diagnostic, drop_replaced};
    use crate::ast::Pos;

    fn finding(code: &'static str, start: u32, end: u32) -> Diagnostic {
        let pos = Pos {
            start_offset: start,
            end_offset: end,
            ..Pos::default()
        };
        Diagnostic::new(code, &[], None, Some(pos))
    }

    /// The one-pass filter keeps exactly what a `retain` before each replacing push kept.
    #[test]
    fn drop_replaced_matches_retain_per_emit() {
        let mut seed = 7u64;
        let mut next = |bound: u64| {
            seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            u32::try_from((seed >> 33) % bound).unwrap_or(0)
        };
        for _ in 0..500 {
            let mut expected: Vec<Diagnostic> = Vec::new();
            let mut output: Vec<Diagnostic> = Vec::new();
            let mut replacing = Vec::new();
            for _ in 0..next(40) {
                let code = match next(3) {
                    0 => "a",
                    1 => "b",
                    _ => "c",
                };
                let start = next(60);
                let item = finding(code, start, start + next(6));
                if next(4) == 0 {
                    expected.push(item.clone());
                } else {
                    let pos = item.pos.unwrap_or_default();
                    expected.retain(|d| {
                        !(d.code == code
                            && d.pos.is_some_and(|p| {
                                p.start_offset < pos.end_offset && pos.start_offset < p.end_offset
                            }))
                    });
                    expected.push(item.clone());
                    replacing.push(output.len());
                }
                output.push(item);
            }
            drop_replaced(&mut output, &replacing);
            assert_eq!(output, expected);
        }
    }
}
