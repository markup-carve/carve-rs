//! Source lint under C2-W-103 to C2-W-109.

use crate::diagnostic::Diagnostic;
use crate::source::Source;
use serde::Deserialize as _;
use serde_json::Value;
use std::cell::OnceCell;
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

mod controls;
mod includes;
mod lines;
mod nodes;

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
        heading_ids: opts.heading_ids,
        ..crate::Options::default()
    };
    out.extend(crate::parse_diagnostics(source, &parse));
    if opts.includes.is_some() {
        includes::scan(source, opts, &mut out);
    }
    out.retain(|finding| {
        finding.code != "table-shaped-paragraph"
            || !finding.pos.is_some_and(|pos| {
                source
                    .get(pos.start_offset as usize..pos.end_offset as usize)
                    .is_some_and(|span| span.contains("%%"))
            })
    });
    select_message_variants(source, &mut out);
    out
}

/// Selects catalog wording whose trigger depends on the surrounding source,
/// after legacy, line and node scanners have contributed their findings.
fn select_message_variants(source: &str, findings: &mut [Diagnostic]) {
    for finding in findings {
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
        let blank_next = source
            .get(line_end..)
            .and_then(|tail| tail.strip_prefix('\n'))
            .is_some_and(|tail| tail.starts_with('\n') || tail.starts_with("\r\n"));
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

fn lint_core(source: &str, opts: &LintOptions) -> Vec<Diagnostic> {
    let parsed = Source::new(source);
    let positions = Ranges::new(source, node_ranges(source));
    let mut out = Vec::new();
    for warning in carve_0_1::lint_carve(source) {
        if skip_legacy_warning(&warning, source, opts, &parsed, &positions) {
            continue;
        }
        out.push(convert_legacy_warning(warning, source, &parsed, &positions));
    }
    lines::scan(source, opts, &positions.all, &mut out);
    nodes::scan(source, opts, &mut out);
    referenced_footnote_platform_tokens(source, opts, &positions, &mut out);
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
    controls::filter(source, out)
}

fn skip_legacy_warning(
    warning: &carve_0_1::LintWarning,
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
    skip_legacy_context(warning, source, parsed, positions)
}

fn skip_legacy_context(
    warning: &carve_0_1::LintWarning,
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
    ]
    .contains(&warning.rule)
    {
        return true;
    }
    false
}

fn overlap_in_centered_table(
    source: &str,
    warning: &carve_0_1::LintWarning,
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
    warning: carve_0_1::LintWarning,
    source: &str,
    parsed: &Source<'_>,
    positions: &Ranges<'_>,
) -> Diagnostic {
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
    path: String,
    kind: String,
    start_line: usize,
    end_line: usize,
    has_attrs: bool,
    footnote_key: Option<String>,
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
                        footnote_key: (kind == "footnote_ref")
                            .then(|| value["key"].as_str().map(str::to_string))
                            .flatten(),
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

fn path_at(ranges: &Ranges<'_>, start: usize, end: usize) -> Option<String> {
    ranges
        .around(start)
        .filter(|r| r.start <= start && end <= r.end)
        .max_by_key(|r| r.path.len())
        .map(|r| r.path.clone())
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
