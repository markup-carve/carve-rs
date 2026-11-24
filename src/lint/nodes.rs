//! Checks tied to parsed nodes and their source positions (C2-W-107).

use super::braced;
use super::walk::{PathNode, Walk, path_text};
use super::{LintOptions, active, drop_replaced};
use crate::diagnostic::Diagnostic;
use crate::source::Source;
use crate::view::V;
use std::cell::OnceCell;
use std::collections::HashSet;

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
    /// The key of a footnote definition.
    pub(super) key: Option<&'a str>,
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
    /// The footnote keys a `footnote_ref` names.
    references: &'a HashSet<&'a str>,
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
        self.replacing.push(self.output.len());
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
    references: &HashSet<&str>,
    output: &mut Vec<Diagnostic>,
) {
    if base_items.is_empty() {
        return;
    }
    if options.heading_ids == crate::options::HeadingIds::default() {
        scan_tree(source, options, base_items, references, output);
        return;
    }
    let opts = crate::Options {
        heading_ids: options.heading_ids,
        raw_html: options.raw_html,
        ..crate::Options::default()
    };
    let (scanned, _) = crate::with_lint_tree(source, &opts, &opts, true, |alternate, arena| {
        alternate
            .map(|tree| {
                scan_tree(
                    source,
                    options,
                    Walk::items(tree, arena),
                    references,
                    output,
                );
            })
            .is_some()
    });
    if !scanned {
        scan_tree(source, options, base_items, references, output);
    }
}

/// The active rules absent from the data catalog need only these node kinds and source marks.
/// All other node rules are supplied by the interpreter. A kind not named here stays on the
/// hand scanner, so adding a new node kind cannot silently bypass a hand-written rule.
pub(super) fn needs_hand_node(kind: &str, raw: &str) -> bool {
    match kind {
        "text" => !raw.is_ascii() || raw.bytes().any(|byte| matches!(byte, b'{' | b'#')),
        "code" | "math" => !raw.is_ascii(),
        "strike" => raw.contains("~>"),
        "link" => {
            // Both hand-only link labels, "here" and "read more", contain an r.
            // Inspect only the authored label, before any destination URL.
            let label = raw.split(']').next().unwrap_or("");
            label.bytes().any(|byte| matches!(byte, b'r' | b'R'))
        }
        _ if braced::LEAVES.contains(&kind) => braced::may_hold_opener(raw),
        "document" | "frontmatter" | "list" | "list_item" | "table" | "table_head"
        | "table_body" | "table_foot" | "table_row" | "table_cell" | "table_column"
        | "soft_break" | "hard_break" | "line_break" | "strong" | "emphasis" => false,
        _ => true,
    }
}

fn scan_tree(
    source: &str,
    options: &LintOptions,
    mut items: Vec<Item<'_>>,
    references: &HashSet<&str>,
    output: &mut Vec<Diagnostic>,
) {
    items.sort_by_cached_key(|item| item.bounds.map_or(0, |(start, _)| start));
    let mut sink = Sink {
        source,
        positions: Source::new(source),
        options,
        output,
        replacing: Vec::new(),
        references,
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
        "code" | "math" => bidi(item, sink),
        _ => {}
    }
}

fn check_inline_node(item: &Item<'_>, sink: &mut Sink<'_>) {
    match item.node.ty().as_str().unwrap_or("") {
        "text" => text_node(item, sink),
        kind if braced::LEAVES.contains(&kind) => braced_openers(item, sink),
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

fn text_node(item: &Item<'_>, sink: &mut Sink<'_>) {
    let value = sink.raw(item).to_string();
    let Some((from, _)) = item.bounds else {
        return;
    };
    text_platform_refs(item, sink, &value, from);
    bidi(item, sink);
}

fn text_platform_refs(item: &Item<'_>, sink: &mut Sink<'_>, value: &str, from: usize) {
    if sink.options.platforms.iter().any(|p| p == "github") && !unreferenced_footnote(item, sink) {
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

fn braced_openers(item: &Item<'_>, sink: &mut Sink<'_>) {
    for (at, path) in braced::openers(sink.source, item.node, item.path()) {
        sink.emit_range("braced-span-opener-as-text", at, at + 2, &path, &[], 0);
    }
}

/// A platform token in a footnote nothing references is not published, so it is not reported.
fn unreferenced_footnote(item: &Item<'_>, sink: &Sink<'_>) -> bool {
    parent_nodes(item)
        .filter(|parent| parent.kind == "footnote")
        .last()
        .is_some_and(|footnote| {
            !footnote
                .key
                .is_some_and(|key| sink.references.contains(key))
        })
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
    if unreferenced_footnote(item, sink) {
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
