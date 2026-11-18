//! Source lint under C2-W-103 to C2-W-109.

use crate::diagnostic::Diagnostic;
use crate::source::Source;
use serde::Deserialize as _;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::OnceLock;

mod controls;
mod includes;
mod lines;
mod nodes;

/// Rule switches and platform selection for a lint run.
#[derive(Debug, Clone, Default)]
pub struct LintOptions {
    /// A rule id mapped to `on` or `off`.
    pub rules: HashMap<String, String>,
    /// Platform names, currently `github`.
    pub platforms: Vec<String>,
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
    let mut out = lint_core(source, opts);
    let parse = crate::Options {
        includes: opts.includes.clone(),
        ..crate::Options::default()
    };
    out.extend(crate::parse_diagnostics(source, &parse));
    if opts.includes.is_some() {
        includes::scan(source, opts, &mut out);
    }
    out
}

fn lint_core(source: &str, opts: &LintOptions) -> Vec<Diagnostic> {
    let parsed = Source::new(source);
    let positions = node_ranges(source);
    let mut out = Vec::new();
    for warning in carve::lint_carve(source) {
        if !active(warning.rule, opts) {
            continue;
        }
        if warning.rule == "table-marker-run-padding"
            && !table_marker_run_padding(source, warning.start)
        {
            continue;
        }
        if warning.rule == "list-item-block-overindented"
            && !positions.iter().any(|range| {
                range.start == warning.start
                    && matches!(
                        range.kind.as_str(),
                        "heading"
                            | "block_quote"
                            | "code_block"
                            | "definition_term"
                            | "div"
                            | "image"
                            | "thematic_break"
                            | "table"
                    )
            })
        {
            continue;
        }
        if matches!(
            warning.rule,
            "deprecated-cell-alignment"
                | "carve-version-unsupported"
                | "table-attribute-invalid"
                | "table-row-cell-count"
                | "thematic-break-spelling"
                | "unclosed-inline-emphasis"
                | "unresolved-footnote"
                | "unresolved-reference-link"
                | "broken-crossref"
                | "markdown-strong-double-star"
                | "blockquote-marker-without-space"
                | "bidi-control-in-source"
                | "table-shaped-paragraph"
                | "unattached-block-attribute"
                | "deprecated-bare-highlight"
                | "deprecated-bare-strike"
                | "deprecated-bare-underline"
                | "fence-closer-indented"
                | "block-opener-overindented"
                | "block-marker-as-text"
                | "comment-shaped-emphasis"
                | "unused-link-reference-definition"
                | "unused-footnote-definition"
                | "unattached-inline-attribute"
                | "emphasis-marker-before-attributes"
                | "comment-in-reference-label"
                | "uneven-list-indent"
                | "markdown-definition-paragraph"
                | "block-opener-without-blank-line"
                | "list-marker-without-blank-line"
                | "semantic-attribute-outside-span"
                | "empty-include-path"
                | "heading-trailing-attribute"
        ) {
            continue;
        }
        let mut diagnostic = Diagnostic::new(
            warning.rule,
            &[],
            warning.file.as_deref(),
            Some(parsed.pos(warning.start, warning.end)),
        );
        if diagnostic.message.is_empty() || catalog_message_has_slots(warning.rule) {
            diagnostic.message = warning.message;
        }
        normalize_root_message(
            &mut diagnostic,
            source,
            &positions,
            warning.start,
            warning.end,
        );
        diagnostic.path = path_at(&positions, warning.start, warning.end);
        out.push(diagnostic);
    }
    lines::scan(source, opts, &positions, &mut out);
    nodes::scan(source, opts, &mut out);
    out.sort_by_key(|d| d.pos.map_or(0, |p| p.start_offset));
    controls::filter(source, out)
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
    ranges: &[Range],
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
            let (column, base, content) = list_columns(source, ranges, start);
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
            let content = list_columns(source, ranges, start).2.to_string();
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
    ranges: &[Range],
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
    let line = (before
        .get(..opener)
        .unwrap_or("")
        .bytes()
        .filter(|b| *b == b'\n')
        .count()
        + 1)
    .to_string();
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
    let closes_enclosing = ranges.iter().any(|range| {
        range.start < opener
            && range.end >= end
            && matches!(
                range.kind.as_str(),
                "div" | "admonition" | "figure_group" | "line_block" | "hard_break_block"
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

fn list_columns(source: &str, ranges: &[Range], at: usize) -> (u32, u32, u32) {
    let positions = Source::new(source);
    let column = positions.pos(at, at).start_column;
    let mut columns: Vec<u32> = ranges
        .iter()
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
        .unwrap_or_else(|| marker_content_column(source, ranges, at, base));
    (column, base, content)
}

fn marker_content_column(source: &str, ranges: &[Range], at: usize, base: u32) -> u32 {
    ranges
        .iter()
        .filter(|range| range.kind == "list" && range.start <= at && at <= range.end)
        .find(|range| {
            Source::new(source)
                .pos(range.start, range.start)
                .start_column
                == base
        })
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

struct Range {
    start: usize,
    end: usize,
    path: String,
    kind: String,
    start_line: usize,
    end_line: usize,
    has_attrs: bool,
}

fn node_ranges(source: &str) -> Vec<Range> {
    let text = crate::tree_json(source, &crate::Options::default());
    let Some(tree) = parse_tree_value(&text) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut pending = vec![(&tree, String::new())];
    while let Some((value, path)) = pending.pop() {
        match value {
            Value::Object(map) => {
                if let (Some(pos), Some(kind)) = (value.get("pos"), value["type"].as_str()) {
                    let start =
                        usize::try_from(pos["startOffset"].as_u64().unwrap_or(0)).unwrap_or(0);
                    let end = usize::try_from(pos["endOffset"].as_u64().unwrap_or(0)).unwrap_or(0);
                    let start_line =
                        usize::try_from(pos["startLine"].as_u64().unwrap_or(0)).unwrap_or(0);
                    let end_line =
                        usize::try_from(pos["endLine"].as_u64().unwrap_or(0)).unwrap_or(0);
                    out.push(Range {
                        start,
                        end,
                        path: path.clone(),
                        kind: kind.into(),
                        start_line,
                        end_line,
                        has_attrs: value.get("attrs").is_some(),
                    });
                }
                pending.extend(map.iter().map(|(k, v)| (v, format!("{path}/{k}"))));
            }
            Value::Array(items) => {
                pending.extend(
                    items
                        .iter()
                        .enumerate()
                        .map(|(i, v)| (v, format!("{path}/{i}"))),
                );
            }
            _ => {}
        }
    }
    out
}

fn parse_tree_value(text: &str) -> Option<Value> {
    let mut parser = serde_json::Deserializer::from_str(text);
    parser.disable_recursion_limit();
    let value = Value::deserialize(&mut parser).ok()?;
    parser.end().ok()?;
    Some(value)
}

fn path_at(ranges: &[Range], start: usize, end: usize) -> Option<String> {
    ranges
        .iter()
        .filter(|r| r.start <= start && end <= r.end)
        .max_by_key(|r| r.path.len())
        .map(|r| r.path.clone())
}
