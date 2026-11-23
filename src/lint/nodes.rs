//! Checks tied to parsed nodes and their source positions (C2-W-107).

use super::walk::{PathNode, Walk, path_text};
use super::{LintOptions, active, drop_replaced};
use crate::diagnostic::Diagnostic;
use crate::source::Source;
use crate::view::V;
use std::cell::OnceCell;

fn field<'a>(value: V<'a>, path: &str) -> V<'a> {
    value.field(path)
}

pub(super) struct Item<'a> {
    pub(super) node: V<'a>,
    /// The node's start and end offsets.
    pub(super) bounds: Option<(usize, usize)>,
    pub(super) path: OnceCell<String>,
    pub(super) path_node: Option<&'a PathNode<'a>>,
    pub(super) parents: Option<&'a Parent<'a>>,
}

impl Item<'_> {
    fn path(&self) -> &str {
        self.path.get_or_init(|| path_text(self.path_node))
    }
}

pub(super) struct Parent<'a> {
    pub(super) kind: &'a str,
    pub(super) depth: usize,
    pub(super) previous: Option<&'a Parent<'a>>,
}

fn parent_nodes<'i, 'a>(item: &'i Item<'a>) -> impl Iterator<Item = &'i Parent<'a>> {
    std::iter::successors(item.parents, |parent| parent.previous)
}

fn parent_kinds<'a>(item: &Item<'a>) -> impl Iterator<Item = &'a str> {
    parent_nodes(item).map(|parent| parent.kind)
}

struct Sink<'a> {
    source: &'a str,
    positions: Source<'a>,
    options: &'a LintOptions,
    output: &'a mut Vec<Diagnostic>,
    replacing: Vec<usize>,
}

impl Sink<'_> {
    fn emit(
        &mut self,
        code: &'static str,
        item: &Item<'_>,
        values: &[(&str, &str)],
        message: usize,
    ) {
        let Some((from, to)) = item.bounds else {
            return;
        };
        self.emit_range(code, from, to, item.path(), values, message);
    }

    fn emit_range(
        &mut self,
        code: &'static str,
        from: usize,
        to: usize,
        path: &str,
        values: &[(&str, &str)],
        message: usize,
    ) {
        if !active(code, self.options) {
            return;
        }
        let pos = self.positions.pos(from, to);
        if code != "table-attribute-invalid" {
            self.replacing.push(self.output.len());
        }
        let mut finding = Diagnostic::nth(code, message, values, None, Some(pos));
        finding.path = Some(path.to_string());
        self.output.push(finding);
    }

    fn raw<'a>(&'a self, item: &Item<'_>) -> &'a str {
        item.bounds
            .and_then(|(a, b)| self.source.get(a..b))
            .unwrap_or("")
    }
}

fn bounds(node: V<'_>) -> Option<(usize, usize)> {
    node.offsets()
}

pub(super) fn scan(
    source: &str,
    options: &LintOptions,
    base_items: Vec<Item<'_>>,
    output: &mut Vec<Diagnostic>,
) {
    if base_items.is_empty() {
        return;
    }
    if options.heading_ids == crate::options::HeadingIds::default() {
        scan_tree(source, options, base_items, output);
        return;
    }
    let opts = crate::Options {
        heading_ids: options.heading_ids,
        raw_html: options.raw_html,
        ..crate::Options::default()
    };
    let (scanned, _) = crate::with_lint_tree(source, &opts, &opts, true, |alternate, arena| {
        alternate
            .map(|tree| scan_tree(source, options, Walk::items(tree, arena), output))
            .is_some()
    });
    if !scanned {
        scan_tree(source, options, base_items, output);
    }
}

/// The active rules absent from the data catalog need only these node kinds and source marks.
/// All other node rules are supplied by the interpreter. A kind not named here stays on the
/// hand scanner, so adding a new node kind cannot silently bypass a hand-written rule.
pub(super) fn needs_hand_node(kind: &str, raw: &str) -> bool {
    match kind {
        "text" => {
            !raw.is_ascii()
                || raw
                    .bytes()
                    .any(|byte| matches!(byte, b'{' | b'@' | b'#' | b'_' | b'*' | b'/'))
        }
        "code" | "math" => !raw.is_ascii(),
        "strong" | "emphasis" => raw.contains('{'),
        "strike" => raw.contains("~>"),
        "link" => {
            // Both hand-only link labels, "here" and "read more", contain an r.
            // Inspect only the authored label, before any destination URL.
            let label = raw.split(']').next().unwrap_or("");
            label.bytes().any(|byte| matches!(byte, b'r' | b'R'))
        }
        // Paragraph-only hand rules inspect multiline delimiter pairs and lone emphasis.
        // A refused braced closer can also be literal text inside a paragraph.
        "paragraph" => {
            raw.contains('\n')
                || raw.contains("{/")
                || raw
                    .trim_start()
                    .bytes()
                    .next()
                    .is_some_and(|byte| matches!(byte, b'*' | b'/' | b'_' | b'{'))
        }
        "document" | "list" | "list_item" | "table" | "table_head" | "table_body"
        | "table_foot" | "table_row" | "table_cell" | "table_column" | "cell_paragraph"
        | "soft_break" | "hard_break" | "line_break" => false,
        _ => true,
    }
}

fn scan_tree(
    source: &str,
    options: &LintOptions,
    mut items: Vec<Item<'_>>,
    output: &mut Vec<Diagnostic>,
) {
    items.sort_by_cached_key(|item| item.bounds.map_or(0, |(start, _)| start));
    let mut sink = Sink {
        source,
        positions: Source::new(source),
        options,
        output,
        replacing: Vec::new(),
    };
    for item in &items {
        check_node(item, &mut sink);
    }
    drop_replaced(sink.output, &sink.replacing);
}

pub(super) fn has_authored_name(node: V<'_>) -> bool {
    node.at("attrs")
        .at("keyValues")
        .as_array()
        .is_some_and(|pairs| {
            pairs.iter().any(|pair| {
                pair.as_array()
                    .and_then(crate::view::Arr::first)
                    .and_then(V::as_str)
                    .is_some_and(|key| {
                        key.eq_ignore_ascii_case("aria-label")
                            || key.eq_ignore_ascii_case("aria-labelledby")
                    })
            })
        })
}

fn check_node(item: &Item<'_>, sink: &mut Sink<'_>) {
    check_block_node(item, sink);
    check_inline_node(item, sink);
}

fn check_block_node(item: &Item<'_>, sink: &mut Sink<'_>) {
    match item.node.ty().as_str().unwrap_or("") {
        "link" => link(item, sink),
        "heading" => {
            let raw = sink.raw(item).to_string();
            heading_continuation(item, &raw, sink);
        }
        "frontmatter" => frontmatter(item, sink),
        "code" | "math" => bidi(item, sink),
        _ => {}
    }
}

fn check_inline_node(item: &Item<'_>, sink: &mut Sink<'_>) {
    match item.node.ty().as_str().unwrap_or("") {
        "text" => text_node(item, sink),
        "paragraph" => paragraph_braced_opener(item, sink),
        "strong" => strong(item, sink),
        "strike" => strike(item, sink),
        "comment" => comment(item, sink),
        "mention" => mention(item, sink),
        _ => {}
    }
}

fn link(item: &Item<'_>, sink: &mut Sink<'_>) {
    let node = item.node;
    let label = node
        .at("children")
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|child| child.at("value").as_str())
                .collect::<String>()
        })
        .unwrap_or_default();
    let folded = label
        .split_ascii_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();
    let aria = field(node, "/attrs/keyValues")
        .as_array()
        .and_then(|pairs| {
            pairs.iter().find_map(|pair| {
                (pair
                    .item(0)
                    .as_str()
                    .is_some_and(|key| key.eq_ignore_ascii_case("aria-label")))
                .then(|| pair.item(1).as_str())
                .flatten()
            })
        })
        .unwrap_or("")
        .split_ascii_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();
    if ["click here", "here", "read more"].contains(&folded.as_str())
        && (aria.is_empty() || ["click here", "here", "read more"].contains(&aria.as_str()))
    {
        sink.emit("non-descriptive-link-text", item, &[], 0);
    }
}

fn heading_continuation(item: &Item<'_>, raw: &str, sink: &mut Sink<'_>) {
    let Some((from, _)) = item.bounds else {
        return;
    };
    let mut offset = 0;
    let quote_depth = parent_kinds(item)
        .filter(|parent| *parent == "block_quote")
        .count();
    let mut seen_heading = false;
    for line in raw.split_inclusive('\n') {
        let at = offset;
        offset += line.len();
        if !seen_heading {
            let trimmed = line.trim_start_matches([' ', '\t']);
            let marker = trimmed.as_bytes().first().copied();
            let run = trimmed
                .bytes()
                .take_while(|b| Some(*b) == marker && matches!(b, b'#' | b'='))
                .count();
            seen_heading = (1..=6).contains(&run) && trimmed.as_bytes().get(run) == Some(&b' ');
            continue;
        }
        if line.trim_start().starts_with("%%") {
            continue;
        }
        let mut content = line;
        for _ in 0..quote_depth {
            content = content.trim_start_matches([' ', '\t']);
            content = content.strip_prefix("> ").unwrap_or(content);
        }
        if content.starts_with([' ', '\t']) && !content.trim().is_empty() {
            sink.emit_range(
                "deprecated-indented-line-under-heading",
                from + at,
                from + at + line.trim_end_matches(['\n', '\r']).len(),
                item.path(),
                &[],
                0,
            );
        }
        break;
    }
}

fn frontmatter(item: &Item<'_>, sink: &mut Sink<'_>) {
    // C2-B-231: only the line-1 block declares the version.
    let leading = field(item.node, "/pos/startLine").as_u64() == Some(1);
    let raw = sink.raw(item).to_string();
    if let Some((from, _)) = item.bounds.filter(|_| leading) {
        let mut offset = 0;
        for line in raw.split_inclusive('\n') {
            if let Some((key, rest)) = line.split_once(':') {
                let key = key.trim();
                let version = rest.trim();
                if matches!(key, "carve" | "carve-version")
                    && !version.is_empty()
                    && !version.contains(char::is_whitespace)
                    && let Some(message) = unsupported_carve_version(version)
                {
                    let start = from + offset + line.len() - rest.trim_start().len();
                    sink.emit_range(
                        "carve-version-unsupported",
                        start,
                        start + version.len(),
                        item.path(),
                        &[("version", version), ("SPEC_VERSION", "0.2.0")],
                        message,
                    );
                    break;
                }
            }
            offset += line.len();
        }
    }
}

fn unsupported_carve_version(version: &str) -> Option<usize> {
    let parts: Option<Vec<u64>> = version
        .split('.')
        .map(|part| {
            (!part.is_empty())
                .then(|| part.parse::<u64>().ok())
                .flatten()
        })
        .collect();
    let Some(parts) = parts else { return Some(0) };
    (parts.first().copied().unwrap_or(0) > 0
        || parts.get(1).copied().unwrap_or(0) > 2
        || (parts.get(1).copied().unwrap_or(0) == 2 && parts.iter().skip(2).any(|part| *part > 0)))
    .then_some(1)
}

fn text_node(item: &Item<'_>, sink: &mut Sink<'_>) {
    let value = sink.raw(item).to_string();
    let Some((from, _)) = item.bounds else {
        return;
    };
    if let Some(at) = value.find("{%").filter(|_| !value.contains("%}")) {
        sink.emit_range(
            "unclosed-braced-comment",
            from + at,
            from + value.len(),
            item.path(),
            &[],
            0,
        );
    }
    emphasis_before_attrs(item, sink, &value, from);
    text_inline_attr(item, sink, &value, from);
    sigils(item, sink, &value, from);
    text_platform_refs(item, sink, &value, from);
    djot_text(item, sink, &value, from);
    unclosed_emphasis(item, sink, from);
    let parent = item.parents.map_or("", |parent| parent.kind);
    let delimiter = if parent == "strong" {
        "{*"
    } else if parent == "emphasis" {
        "{/"
    } else {
        ""
    };
    if !delimiter.is_empty()
        && let Some(at) = value.find(delimiter)
    {
        sink.emit_range(
            "braced-span-opener-as-text",
            from + at,
            from + at + 2,
            item.path(),
            &[],
            0,
        );
    }
    bidi(item, sink);
}

fn text_platform_refs(item: &Item<'_>, sink: &mut Sink<'_>, value: &str, from: usize) {
    if sink.options.platforms.iter().any(|p| p == "github")
        && !parent_kinds(item).any(|kind| kind == "footnote")
    {
        for (at, _) in value.match_indices('#') {
            let digits = value
                .get(at + 1..)
                .unwrap_or("")
                .bytes()
                .take_while(u8::is_ascii_digit)
                .count();
            if digits > 0 {
                let token = value.get(at..at + digits + 1).unwrap_or("");
                sink.emit_range(
                    "platform-issue-reference",
                    from + at,
                    from + at + token.len(),
                    item.path(),
                    &[("host", "GitHub"), ("text", token)],
                    0,
                );
            }
        }
    }
}

/// A line block's line is no inline host, nor is a code fence title, which is plain text (C2-B-120).
fn outside_inline_hosts(item: &Item<'_>) -> bool {
    parent_kinds(item).any(|kind| kind == "line" || kind == "code_block")
}

/// Each unattached glued run once, in the inline hosts the rule names.
fn text_inline_attr(item: &Item<'_>, sink: &mut Sink<'_>, value: &str, from: usize) {
    if outside_inline_hosts(item) {
        return;
    }
    let mut next = 0;
    for (at, _) in value.match_indices('{') {
        if at < next {
            continue;
        }
        let Some((block, end)) = crate::attr::parse_glued(value, at) else {
            continue;
        };
        let Some(before) = sink
            .source
            .get(..from + at)
            .and_then(|prefix| prefix.rsplit('\n').next())
            .map(str::trim)
        else {
            continue;
        };
        let quote_prefix = before == ">"
            && !value.starts_with('>')
            && sink
                .source
                .get(..from + at)
                .is_some_and(|prefix| prefix.ends_with("> "));
        if at > 0
            && !block.empty
            && !before.is_empty()
            && !quote_prefix
            && !escaped(value.as_bytes(), at)
        {
            let mut run_end = end;
            while let Some((following, following_end)) = crate::attr::parse_glued(value, run_end) {
                if following.empty {
                    break;
                }
                run_end = following_end;
            }
            sink.emit_range(
                "unattached-inline-attribute",
                from + at,
                from + run_end,
                item.path(),
                &[],
                0,
            );
            next = run_end;
        }
    }
}

fn emphasis_before_attrs(item: &Item<'_>, sink: &mut Sink<'_>, raw: &str, from: usize) {
    for (at, ch) in raw.char_indices() {
        if !matches!(ch, '*' | '/' | '_' | '~' | '=')
            || raw.as_bytes().get(at + 1) != Some(&b'{')
            || escaped(raw.as_bytes(), at)
        {
            continue;
        }
        let Some((_, end)) = crate::attr::parse_block(raw, at + 1) else {
            continue;
        };
        if end <= at + 1 {
            continue;
        }
        sink.emit_range(
            "emphasis-marker-before-attributes",
            from + at,
            from + end,
            item.path(),
            &[("ch", &ch.to_string())],
            0,
        );
    }
}

fn sigils(item: &Item<'_>, sink: &mut Sink<'_>, value: &str, from: usize) {
    for (at, ch) in value.char_indices() {
        if at == 0 || !matches!(ch, '@' | '#') {
            continue;
        }
        let before = value.get(..at).and_then(|prefix| prefix.chars().last());
        let after = value
            .get(at + ch.len_utf8()..)
            .and_then(|tail| tail.chars().next());
        if !before.is_some_and(|c| c.is_alphanumeric() && !c.is_ascii())
            || !after.is_some_and(|c| c.is_ascii_alphanumeric())
        {
            continue;
        }
        let mut name = value
            .get(at + ch.len_utf8()..)
            .unwrap_or("")
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'));
        if !name.any(|c| c.is_ascii_alphabetic()) {
            continue;
        }
        sink.emit_range(
            "sigil-after-letter",
            from + at,
            from + at + ch.len_utf8(),
            item.path(),
            &[],
            0,
        );
    }
}

fn unclosed_emphasis(item: &Item<'_>, sink: &mut Sink<'_>, from: usize) {
    let raw = sink.raw(item).to_string();
    let bytes = raw.as_bytes();
    for at in 0..bytes.len().saturating_sub(1) {
        let mark = bytes.get(at).copied().unwrap_or_default();
        if !matches!(mark, b'*' | b'/')
            || escaped(bytes, at)
            || !bytes.get(at + 1).is_some_and(u8::is_ascii_alphanumeric)
            || (at > 0 && bytes.get(at - 1) == Some(&b'{'))
            || at > 0
                && !bytes
                    .get(at - 1)
                    .is_some_and(|byte| matches!(byte, b' ' | b'\t' | b'(' | b'[' | b'{'))
            || bytes.get(at + 1..).is_some_and(|tail| tail.contains(&mark))
        {
            continue;
        }
        let span = if mark == b'*' { "strong" } else { "emphasis" };
        sink.emit_range(
            "unclosed-inline-emphasis",
            from + at,
            from + at + 1,
            item.path(),
            &[("span", span)],
            0,
        );
        break;
    }
}

fn bidi(item: &Item<'_>, sink: &mut Sink<'_>) {
    if item.node.ty() == "code_block"
        || parent_kinds(item).any(|kind| kind == "code_block") && item.path().contains("/title/")
    {
        return;
    }
    let Some((from, _)) = item.bounds else {
        return;
    };
    let raw = sink.raw(item).to_string();
    for (at, ch) in raw.char_indices() {
        if matches!(ch as u32, 0x202a..=0x202e | 0x2066..=0x2069) {
            let code = format!("{:04X}", ch as u32);
            sink.emit_range(
                "bidi-control-in-source",
                from + at,
                from + at + ch.len_utf8(),
                item.path(),
                &[("code", &code)],
                0,
            );
        }
    }
}

fn djot_text(item: &Item<'_>, sink: &mut Sink<'_>, value: &str, from: usize) {
    let bytes = value.as_bytes();
    for (open, _) in value.match_indices('_') {
        if open == 0
            || bytes
                .get(open - 1)
                .is_none_or(|c| !c.is_ascii_alphanumeric())
            || bytes.get(open + 1).is_none_or(u8::is_ascii_whitespace)
            || escaped(bytes, open)
        {
            continue;
        }
        if let Some((close, _)) = value[open + 1..].match_indices('_').find(|(at, _)| {
            let close = open + 1 + at;
            close > open + 1
                && bytes
                    .get(close - 1)
                    .is_some_and(|c| !c.is_ascii_whitespace())
                && bytes.get(close + 1).is_some_and(u8::is_ascii_alphanumeric)
                && !escaped(bytes, close)
        }) {
            sink.emit_range(
                "djot-intraword-underscore",
                from + open,
                from + open + 1 + close + 1,
                item.path(),
                &[],
                0,
            );
            break;
        }
    }
}

fn paragraph_braced_opener(item: &Item<'_>, sink: &mut Sink<'_>) {
    let raw = sink.raw(item).to_string();
    if let Some((from, _)) = item.bounds
        && let Some(start) = raw.find("{/")
        && raw
            .get(start + 2..)
            .and_then(|tail| tail.find("*/}"))
            .is_some_and(|end| !escaped(raw.as_bytes(), start + 2 + end))
        && raw.get(..start).is_some_and(|head| head.contains('*'))
    {
        sink.emit_range(
            "braced-span-opener-as-text",
            from + start,
            from + start + 2,
            item.path(),
            &[],
            0,
        );
    }
}

fn strong(item: &Item<'_>, sink: &mut Sink<'_>) {
    if sink.raw(item).starts_with("{*") && sink.raw(item).contains("{**}") {
        sink.emit("braced-span-opener-as-text", item, &[], 0);
    }
    span_before_attrs(item, sink, '*', "strong");
}

fn span_before_attrs(item: &Item<'_>, sink: &mut Sink<'_>, marker: char, name: &str) {
    if item.node.get("attrs").is_none() {
        return;
    }
    let raw = sink.raw(item).to_string();
    if !raw.starts_with(marker) || raw.starts_with("{*") || raw.starts_with("{/") {
        return;
    }
    let Some(close) = raw.rfind(&format!("{marker}{{")) else {
        return;
    };
    let inner = raw.get(1..close).unwrap_or("");
    if !inner
        .char_indices()
        .any(|(at, ch)| ch == marker && !escaped(inner.as_bytes(), at))
    {
        return;
    }
    let Some((from, _)) = item.bounds else {
        return;
    };
    let opening = sink.positions.pos(from, from);
    sink.emit_range(
        "emphasis-marker-before-attributes",
        from + close,
        from + close + marker.len_utf8(),
        item.path(),
        &[
            ("marker", &marker.to_string()),
            ("name", name),
            ("line", &opening.start_line.to_string()),
            ("column", &opening.start_column.to_string()),
        ],
        1,
    );
}

fn strike(item: &Item<'_>, sink: &mut Sink<'_>) {
    if item.node.at("children").as_array().is_some_and(|children| {
        children.iter().any(|child| {
            child.ty() == "text"
                && bounds(child)
                    .and_then(|(from, to)| sink.source.get(from..to))
                    .is_some_and(|raw| {
                        raw.match_indices("~>")
                            .any(|(at, _)| !escaped(raw.as_bytes(), at))
                    })
        })
    }) {
        sink.emit("deprecated-substitution-arrow", item, &[], 0);
    }
}

fn mention(item: &Item<'_>, sink: &mut Sink<'_>) {
    if parent_kinds(item).any(|kind| kind == "footnote") {
        return;
    }
    let raw = sink.raw(item).to_string();
    sink.emit(
        "platform-mention-token",
        item,
        &[("host", "GitHub"), ("what", "a mention"), ("text", &raw)],
        0,
    );
}

fn comment(item: &Item<'_>, sink: &mut Sink<'_>) {
    if item.node.at("form") == "line" {
        let Some((from, to)) = item.bounds else {
            return;
        };
        let prefix = sink.source.get(..from).unwrap_or("");
        let line_start = prefix.rfind('\n').map_or(0, |at| at + 1);
        let before = prefix.get(line_start..).unwrap_or("");
        if let Some(open) = before.rfind('[')
            && !before.get(open..).unwrap_or("").contains(']')
            && before
                .get(..open)
                .is_some_and(|head| head.trim_end().ends_with(']'))
        {
            sink.emit_range("comment-in-reference-label", from, to, item.path(), &[], 0);
        }
    }
}

fn escaped(text: &[u8], at: usize) -> bool {
    text.get(..at)
        .unwrap_or(&[])
        .iter()
        .rev()
        .take_while(|b| **b == b'\\')
        .count()
        % 2
        == 1
}
