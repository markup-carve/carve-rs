//! The findings the 0.1 lint pass still supplied, read off the 0.2 tree.
//!
//! Each record carries what a 0.1 `LintWarning` carried, so `lint_core` skips,
//! converts and deduplicates it exactly as it did the 0.1 warning.

use crate::view::{Arr, V};
use std::collections::HashSet;

pub(super) struct Warning {
    pub(super) rule: &'static str,
    pub(super) start: usize,
    pub(super) end: usize,
    pub(super) message: String,
}

struct Node<'a> {
    value: V<'a>,
    kind: &'a str,
    parent: Option<usize>,
    /// The key this node sits under in its parent (`children`, `caption`, ...).
    slot: &'a str,
}

fn bounds(node: V<'_>) -> Option<(usize, usize)> {
    node.offsets()
}

fn nodes(root: V<'_>) -> Vec<Node<'_>> {
    let mut out = Vec::new();
    let mut pending = vec![(root, None, "")];
    let mut entries = Vec::new();
    while let Some((value, parent, slot)) = pending.pop() {
        if !value.is_object() {
            continue;
        }
        let Some(kind) = value.ty().as_str() else {
            continue;
        };
        let at = out.len();
        out.push(Node {
            value,
            kind,
            parent,
            slot,
        });
        value.each(&mut |key, child| entries.push((key, child)));
        for (key, child) in entries.drain(..).rev() {
            if matches!(key, "data" | "props" | "pos" | "attrs") {
                continue;
            }
            if let Some(items) = child.as_array() {
                pending.extend(items.iter().rev().map(|item| (item, Some(at), key)));
            } else if child.is_object() {
                pending.push((child, Some(at), key));
            }
        }
    }
    out
}

fn ancestors<'n, 'a>(all: &'n [Node<'a>], at: usize) -> impl Iterator<Item = &'n Node<'a>> {
    std::iter::successors(all.get(at).and_then(|n| n.parent), |&i| {
        all.get(i).and_then(|n| n.parent)
    })
    .filter_map(|i| all.get(i))
}

pub(super) fn warnings(source: &str, tree: V<'_>) -> Vec<Warning> {
    let all = nodes(tree);
    let mut out = Vec::new();
    let mut braced_comments = HashSet::new();
    let mut footnote_labels = Vec::new();
    for (at, node) in all.iter().enumerate() {
        let value = node.value;
        match node.kind {
            "comment" if value.at("form") == "braced" => {
                braced_comments.extend(bounds(value).map(|(start, _)| start));
            }
            "footnote" => footnote_labels.extend(bounds(value).map(|(start, _)| start)),
            "table" => {
                wide_row(value, &mut out);
                table_columns(source, value, &mut out);
                span_markers(source, value, &mut out);
            }
            _ => visit(source, &all, at, &mut out),
        }
    }
    template_tags(source, &braced_comments, &mut out);
    differing_labels(source, &footnote_labels, &mut out);
    region_markers(source, &all, &mut out);
    out.sort_by(|a, b| (a.start, a.end, a.rule).cmp(&(b.start, b.end, b.rule)));
    out
}

fn visit(source: &str, all: &[Node<'_>], at: usize, out: &mut Vec<Warning>) {
    let Some(node) = all.get(at) else { return };
    let value = node.value;
    let in_footnote = || ancestors(all, at).any(|n| n.kind == "footnote");
    match node.kind {
        "div" => figure_nested(source, all, at, out),
        "figure_group" => figure_group(source, value, out),
        "caption_number" => panel_number(source, all, at, out),
        "strong" if !in_footnote() => glob_strong(source, value, out),
        "list" | "definition_list" => consumed_key(source, value, "loose", out),
        "code_block" if matches!(value.at("syntax").as_str(), Some("csv" | "tsv")) => {
            consumed_key(source, value, "table", out);
        }
        "table_cell" => reversed_marker(source, value, out),
        "paragraph" if !in_footnote() => {
            html_comment(source, all, at, out);
            bare_tags(source, all, at, out);
        }
        "list_item" => marker_between_lists(source, all, at, out),
        _ => {}
    }
}

fn push(out: &mut Vec<Warning>, rule: &'static str, (start, end): (usize, usize)) {
    out.push(Warning {
        rule,
        start,
        end,
        message: String::new(),
    });
}

/// The opener line at `at` reads `::: figure` and nothing else.
fn bare_figure_opener(source: &str, at: usize) -> bool {
    let line = source
        .get(at..)
        .unwrap_or("")
        .split('\n')
        .next()
        .unwrap_or("");
    let rest = line.trim_start_matches(':');
    line.len() - rest.len() >= 3 && rest.trim_end() == " figure"
}

/// PART 9 §4c: a bare `::: figure` inside an open group stays a generic container.
fn figure_nested(source: &str, all: &[Node<'_>], at: usize, out: &mut Vec<Warning>) {
    let Some(node) = all.get(at) else { return };
    let attrs = &node.value.at("attrs");
    let figure = attrs.object_len() == Some(1)
        && attrs
            .at("classes")
            .as_array()
            .is_some_and(|c| c.len() == 1 && c.first().is_some_and(|f| f == "figure"));
    if figure
        && bounds(node.value).is_some_and(|(start, _)| bare_figure_opener(source, start))
        && node.value.get("title").is_none()
        && node.value.get("label").is_none()
        && ancestors(all, at).any(|n| n.kind == "figure_group")
        && let Some(span) = bounds(node.value)
    {
        push(out, "figure-group-nested", span);
    }
}

fn panels(group: V<'_>) -> impl Iterator<Item = V<'_>> {
    group
        .at("children")
        .as_array()
        .into_iter()
        .flatten()
        .filter(|child| child.get("caption").is_some() || child.ty() == "table")
}

fn figure_group(source: &str, group: V<'_>, out: &mut Vec<Warning>) {
    if panels(group).count() == 1
        && let Some(span) = bounds(group)
        && bare_figure_opener(source, span.0)
    {
        push(out, "figure-group-single-panel", span);
    }
}

/// A `#` among the top-level inlines of a panel caption.
fn panel_number(source: &str, all: &[Node<'_>], at: usize, out: &mut Vec<Warning>) {
    let mut chain = ancestors(all, at);
    let (Some(paragraph), Some(caption), Some(panel), Some(group)) =
        (chain.next(), chain.next(), chain.next(), chain.next())
    else {
        return;
    };
    let Some(node) = all.get(at) else { return };
    if node.slot == "children"
        && paragraph.kind == "paragraph"
        && paragraph.slot == "children"
        && caption.kind == "caption"
        && caption.slot == "caption"
        && group.kind == "figure_group"
        && panel.slot == "children"
        && (panel.value.get("caption").is_some() || panel.kind == "table")
        && let Some(span) = bounds(node.value)
        && bounds(caption.value)
            .and_then(|(from, _)| source.get(from..span.0))
            .is_some_and(|before| !before.contains('\n'))
    {
        push(out, "figure-group-panel-number", span);
    }
}

fn glob_strong(source: &str, strong: V<'_>, out: &mut Vec<Warning>) {
    if replaced!("glob-shaped-strong") {
        return;
    }
    let Some((start, end)) = bounds(strong) else {
        return;
    };
    let raw = source.get(start..end).unwrap_or("");
    if raw.starts_with("*.") && raw.ends_with('*') {
        push(out, "glob-shaped-strong", (start, start + 1));
    }
}

/// The offset of a block's first line behind its attribute lines.
fn behind_attribute_lines(source: &str, mut at: usize, end: usize) -> usize {
    while at < end {
        let line = source
            .get(at..end)
            .unwrap_or("")
            .split('\n')
            .next()
            .unwrap_or("");
        let trimmed = line.trim_matches([' ', '\t', '\r']);
        if !(trimmed.starts_with('{') && trimmed.ends_with('}')) {
            break;
        }
        at += line.len() + 1;
    }
    at.min(end)
}

/// `consumed-key-near-miss`: a bare key one edit from the key its block reads.
fn consumed_key(source: &str, block: V<'_>, key: &'static str, out: &mut Vec<Warning>) {
    let Some((start, end)) = bounds(block) else {
        return;
    };
    let mut seen = HashSet::new();
    let pairs = block.get("attrs").and_then(|attrs| attrs.get("keyValues"));
    for pair in pairs.and_then(V::as_array).into_iter().flatten() {
        let word = pair.item(0).as_str().unwrap_or("");
        if !seen.insert(word) {
            continue;
        }
        if pair.item(1).as_str() == Some("") && near_miss(word, key) {
            out.push(Warning {
                rule: "consumed-key-near-miss",
                start: behind_attribute_lines(source, start, end),
                end,
                message: format!("\"{word}\" is not a consumed key here; did you mean {{{key}}}?"),
            });
        }
    }
}

/// A case-only match, or one insertion, deletion, substitution or adjacent swap.
fn near_miss(word: &str, key: &str) -> bool {
    if word == key {
        return false;
    }
    if word.to_ascii_lowercase() == key {
        return true;
    }
    let (w, k) = (word.as_bytes(), key.as_bytes());
    let prefix = w.iter().zip(k).take_while(|(a, b)| a == b).count();
    let suffix = |from_w: usize, from_k: usize| {
        w.get(from_w..)
            .zip(k.get(from_k..))
            .is_some_and(|(a, b)| a == b)
    };
    match w.len().cmp(&k.len()) {
        // One substitution, or two neighbors swapped.
        std::cmp::Ordering::Equal => {
            suffix(prefix + 1, prefix + 1)
                || (w.get(prefix) == k.get(prefix + 1)
                    && w.get(prefix + 1) == k.get(prefix)
                    && suffix(prefix + 2, prefix + 2))
        }
        // One insertion or deletion.
        std::cmp::Ordering::Greater => w.len() == k.len() + 1 && suffix(prefix + 1, prefix),
        std::cmp::Ordering::Less => k.len() == w.len() + 1 && suffix(prefix, prefix + 1),
    }
}

fn rows(table: V<'_>) -> impl Iterator<Item = V<'_>> {
    table
        .at("children")
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|group| group.at("children").as_array().into_iter().flatten())
        .filter(|row| row.ty() == "table_row")
}

fn cells(row: V<'_>) -> Arr<'_> {
    row.at("children").as_array().unwrap_or_else(Arr::empty)
}

/// A CSV or TSV table, whose rows carry no source position, with a record wider than its first.
fn wide_row(table: V<'_>, out: &mut Vec<Warning>) {
    let mut records = rows(table);
    let Some(first) = records.next() else { return };
    if first.pos().is_some() {
        return;
    }
    let width = cells(first).len();
    if records.any(|row| cells(row).len() > width)
        && let Some(span) = bounds(table)
    {
        push(out, "delimited-table-wide-row", span);
    }
}

/// `aligns`, `valigns` and `widths` on the attribute line above a pipe table.
fn table_columns(source: &str, table: V<'_>, out: &mut Vec<Warning>) {
    let Some((start, _)) = bounds(table) else {
        return;
    };
    let line_start = source
        .get(..start)
        .and_then(|s| s.rfind('\n'))
        .map_or(0, |at| at + 1);
    let mut lines = source.get(line_start..).unwrap_or("").split('\n');
    let line = lines.next().unwrap_or("").trim_end_matches('\r');
    let next = lines.next().unwrap_or("").trim_end_matches('\r');
    let trimmed = line.trim_start();
    let configured = ["aligns", "valigns", "widths"]
        .iter()
        .any(|k| line.contains(&format!("{k}=")));
    let attribute_line = trimmed.starts_with('{') && trimmed.trim_end().ends_with('}');
    if !configured || !attribute_line || !next.trim_start().starts_with('|') {
        return;
    }
    let columns = next.matches('|').count().saturating_sub(1);
    let mut marked: Vec<(bool, bool)> = Vec::new();
    for row in rows(table) {
        let axes: Vec<(bool, bool)> = cells(row)
            .iter()
            .map(|cell| marker_axes(source, cell))
            .collect();
        if !axes.iter().any(|&(align, valign)| align || valign) {
            continue;
        }
        marked.resize(marked.len().max(axes.len()), (false, false));
        for (slot, cell) in marked.iter_mut().zip(&axes) {
            slot.0 |= cell.0;
            slot.1 |= cell.1;
        }
    }
    let mut set_aligns: Vec<bool> = Vec::new();
    for key in ["aligns", "valigns", "widths"] {
        // `aligns=` also ends `valigns=`; the key starts at a word boundary.
        let Some(key_at) = line
            .match_indices(&format!("{key}="))
            .map(|(at, _)| at)
            .find(|&at| {
                !line[..at]
                    .chars()
                    .next_back()
                    .is_some_and(|c| c.is_alphanumeric() || c == '-' || c == '_')
            })
        else {
            continue;
        };
        let rest = &line[key_at + key.len() + 1..];
        let raw = if let Some(q) = rest.strip_prefix('"') {
            q.split('"').next().unwrap_or("")
        } else {
            rest.split(|c: char| c.is_whitespace() || c == '}')
                .next()
                .unwrap_or("")
        };
        let values: Vec<&str> = raw.split(',').collect();
        let span = (line_start + key_at, line_start + key_at + key.len());
        let mut emit = |rule, message: String| {
            out.push(Warning {
                rule,
                start: span.0,
                end: span.1,
                message,
            });
        };
        if values.len() < columns {
            emit(
                "table-column-arity",
                format!(
                    "{key} supplies {} column entries for a {columns}-column table; the unset tail is valid but may be accidental.",
                    values.len()
                ),
            );
        }
        if key == "widths"
            && values
                .iter()
                .filter_map(|v| v.trim().parse::<f64>().ok())
                .sum::<f64>()
                > 100.0
        {
            emit(
                "table-width-total",
                "The specified table column widths total more than 100%.".to_owned(),
            );
        }
        for (c, &(align, valign)) in marked.iter().enumerate() {
            let set = values.get(c).is_some_and(|v| !v.trim().is_empty());
            let overlap = match key {
                "aligns" => align && set,
                "valigns" => {
                    valign && set && !(align && set_aligns.get(c).copied().unwrap_or(false))
                }
                _ => false,
            };
            if overlap {
                emit(
                    "table-column-overlap",
                    format!(
                        "Column {} supplies the same alignment axis both in the table and in a table attribute; the in-table marker wins.",
                        c + 1
                    ),
                );
            }
        }
        if key == "aligns" {
            set_aligns = values.iter().map(|v| !v.trim().is_empty()).collect();
        }
    }
}

/// The alignment axes a cell's own marker run sets; an attribute block that also holds other keys sets none.
fn marker_axes(source: &str, cell: V<'_>) -> (bool, bool) {
    let raw = bounds(cell)
        .and_then(|(from, to)| source.get(from..to))
        .unwrap_or("")
        .trim_start_matches(['|', ' ', '\t'])
        .trim_start_matches('=');
    let plain = raw
        .strip_prefix('{')
        .and_then(|rest| rest.split_once('}'))
        .is_some_and(|(inside, _)| {
            !inside.is_empty() && inside.bytes().all(|b| b"<>~^v?-".contains(&b))
        });
    if !plain {
        return (false, false);
    }
    (cell.get("align").is_some(), cell.get("valign").is_some())
}

/// A `^` or `<` the parser kept as cell text: the span it asks for cannot be placed.
fn span_markers(source: &str, table: V<'_>, out: &mut Vec<Warning>) {
    for row in rows(table) {
        for cell in cells(row) {
            if cell.get("span").is_some() {
                continue;
            }
            let Some(paragraph) = cell.at("children").as_array().and_then(Arr::single) else {
                continue;
            };
            let Some(text) = paragraph.at("children").as_array().and_then(Arr::single) else {
                continue;
            };
            let marker = match text.at("value").as_str() {
                Some("^") => '^',
                Some("<") => '<',
                _ => continue,
            };
            let Some((from, to)) = bounds(cell) else {
                continue;
            };
            let raw = source.get(from..to).unwrap_or("");
            if raw.contains('\\') {
                continue;
            }
            if let Some(at) = raw.find(marker) {
                push(out, "table-span-marker-blocked", (from + at, from + at + 1));
            }
        }
    }
}

/// `{…}>`: a cell attribute block glued to an alignment mark that follows it.
fn reversed_marker(source: &str, cell: V<'_>, out: &mut Vec<Warning>) {
    if cell.get("attrs").is_none() {
        return;
    }
    let Some((from, to)) = bounds(cell) else {
        return;
    };
    let raw = source.get(from..to).unwrap_or("");
    let lead = raw.len() - raw.trim_start_matches([' ', '\t']).len();
    let Some(body) = raw.get(lead..).filter(|body| body.starts_with('{')) else {
        return;
    };
    let Some(close) = body.find('}') else { return };
    if body
        .as_bytes()
        .get(close + 1)
        .is_some_and(|b| matches!(b, b'<' | b'>' | b'~'))
    {
        push(
            out,
            "table-marker-run-padding",
            (from + lead, from + lead + close + 2),
        );
    }
}

/// The source ranges of the text a reader sees inside the node at `at`.
fn visible_ranges(all: &[Node<'_>], at: usize) -> Vec<(usize, usize)> {
    let inside = |mut i: usize| loop {
        match all.get(i).and_then(|n| n.parent) {
            Some(parent) if parent == at => return true,
            Some(parent) => i = parent,
            None => return false,
        }
    };
    (at + 1..all.len())
        .take_while(|&i| inside(i))
        .filter_map(|i| all.get(i))
        .filter(|node| matches!(node.kind, "text" | "smart_punctuation"))
        .filter_map(|node| bounds(node.value))
        .collect()
}

/// `<!--` closed by a visible `-->` later in the same paragraph.
fn html_comment(source: &str, all: &[Node<'_>], at: usize, out: &mut Vec<Warning>) {
    let Some(node) = all.get(at) else { return };
    let Some((start, end)) = bounds(node.value) else {
        return;
    };
    let raw = source.get(start..end).unwrap_or("");
    if !raw.contains("<!--") {
        return;
    }
    let visible = visible_ranges(all, at);
    let holds = |at: usize| visible.iter().any(|&(a, b)| a <= at && at < b);
    let shown = |from: usize, len: usize| holds(from) && holds(from + len - 1);
    for (offset, _) in raw.match_indices("<!--") {
        let open = start + offset;
        let escapes = raw[..offset]
            .bytes()
            .rev()
            .take_while(|b| *b == b'\\')
            .count();
        if escapes % 2 == 1 || !shown(open, 4) {
            continue;
        }
        let closed = raw
            .get(offset + 4..)
            .into_iter()
            .flat_map(|tail| tail.match_indices("-->"))
            .any(|(rel, _)| shown(open + 4 + rel, 3));
        if closed {
            push(out, "html-comment-renders-as-text", (open, open + 4));
        }
    }
}

/// `{% … %}` braced comments shaped like template tags.
fn template_tags(source: &str, comments: &HashSet<usize>, out: &mut Vec<Warning>) {
    if replaced!("braced-comment-in-a-template-source") {
        return;
    }
    if comments.is_empty() {
        return;
    }
    let mut at = 0;
    while let Some(rel) = source.get(at..).and_then(|s| s.find("{%")) {
        let start = at + rel;
        let Some(close_rel) = source.get(start + 2..).and_then(|s| s.find("%}")) else {
            break;
        };
        let end = start + 2 + close_rel + 2;
        at = end;
        if !comments.contains(&start) {
            continue;
        }
        let tag = source.get(start + 2..end - 2).unwrap_or("").trim();
        let shaped = matches!(tag, "raw" | "endraw" | "endif" | "endfor" | "endblock")
            || ["if", "for", "block"].iter().any(|head| {
                tag.strip_prefix(head)
                    .is_some_and(|rest| rest.starts_with(char::is_whitespace))
            });
        if shaped {
            push(out, "braced-comment-in-a-template-source", (start, end));
        }
    }
}

/// Footnote definitions whose labels fold to one key but differ as written.
fn differing_labels(source: &str, starts: &[usize], out: &mut Vec<Warning>) {
    let mut sorted = starts.to_vec();
    sorted.sort_unstable();
    let mut labels = HashSet::new();
    let mut keys = HashSet::new();
    for start in sorted {
        let Some(label) = source
            .get(start..)
            .and_then(|s| s.strip_prefix("[^"))
            .and_then(|s| s.split_once("]:"))
            .map(|(label, _)| label)
            .filter(|label| !label.contains('\n'))
        else {
            continue;
        };
        let key = crate::chars::fold_label(label);
        if labels.contains(label) {
            continue;
        }
        if keys.contains(&key) {
            push(
                out,
                "footnote-labels-differ-only-in-whitespace",
                (start, start + label.len() + 4),
            );
        }
        keys.insert(key);
        labels.insert(label);
    }
}

fn region_keyword(body: &str) -> Option<bool> {
    let keyword = body.strip_prefix('#')?;
    let keyword = if keyword.get(..3)?.eq_ignore_ascii_case("end") {
        &keyword[3..]
    } else {
        keyword
    };
    if !keyword.get(..6)?.eq_ignore_ascii_case("region") {
        return None;
    }
    if keyword
        .as_bytes()
        .get(6)
        .is_some_and(|c| c.is_ascii_alphanumeric() || *c == b'_')
    {
        return None;
    }
    Some(body.starts_with("#region") || body.starts_with("#endregion"))
}

/// A region-marker-shaped line that is not a `%% #region` marker.
fn region_markers(source: &str, all: &[Node<'_>], out: &mut Vec<Warning>) {
    if !source.contains('#') {
        return;
    }
    let opaque: Vec<(usize, usize, bool)> = all
        .iter()
        .filter(|node| {
            matches!(node.kind, "code_block" | "raw_block" | "frontmatter")
                || (node.kind == "comment" && node.value.at("form") != "braced")
        })
        .filter_map(|node| bounds(node.value).map(|(a, b)| (a, b, node.value.at("form") == "line")))
        .collect();
    let mut line_start = 0;
    for piece in source.split_inclusive('\n') {
        let row = piece.trim_end_matches(['\n', '\r']);
        let trimmed = row.trim_start_matches([' ', '\t']);
        let percent = trimmed.starts_with("%%") && !trimmed.starts_with("%%%");
        let body = if percent {
            trimmed[2..].trim_start_matches([' ', '\t'])
        } else if let Some(rest) = trimmed.strip_prefix("<!--") {
            rest.trim_start_matches([' ', '\t'])
        } else if let Some(rest) = trimmed.strip_prefix("/*") {
            rest.trim_start_matches([' ', '\t'])
        } else if let Some(rest) = trimmed
            .strip_prefix("//")
            .or_else(|| trimmed.strip_prefix("--"))
        {
            rest.trim_start_matches([' ', '\t'])
        } else {
            trimmed
        };
        if let Some(lowercase) = region_keyword(body)
            && (!percent || !lowercase)
        {
            let at = line_start + row.len() - trimmed.len();
            let mut covering = opaque.iter().filter(|(a, b, _)| *a <= at && at < *b);
            let allowed = match covering.next() {
                None => true,
                Some(&(_, _, line)) => percent && line && covering.next().is_none(),
            };
            if allowed {
                push(
                    out,
                    "region-marker-renders-as-text",
                    (at, at + trimmed.len()),
                );
            }
        }
        line_start += piece.len();
    }
}

/// The first tag-shaped run on each line of a paragraph, where a reader sees it as text.
fn bare_tags(source: &str, all: &[Node<'_>], at: usize, out: &mut Vec<Warning>) {
    if replaced!("bare-html-tag") {
        return;
    }
    let Some((start, end)) = all.get(at).and_then(|node| bounds(node.value)) else {
        return;
    };
    let raw = source.get(start..end).unwrap_or("");
    if !raw.contains('<') {
        return;
    }
    let visible = visible_ranges(all, at);
    let holds = |at: usize| visible.iter().any(|&(a, b)| a <= at && at < b);
    let mut line_start = start;
    for line in raw.split('\n') {
        if let Some((from, to)) = first_tag(line)
            && line[..from]
                .bytes()
                .rev()
                .take_while(|b| *b == b'\\')
                .count()
                % 2
                == 0
            && holds(line_start + from)
            && holds(line_start + to - 1)
        {
            push(out, "bare-html-tag", (line_start + from, line_start + to));
        }
        line_start += line.len() + 1;
    }
}

/// The first match of `</?[A-Za-z][\w:-]*(?:\s+[^<>]*)?/?>` in `line`.
fn first_tag(line: &str) -> Option<(usize, usize)> {
    let word = |c: char| c.is_alphanumeric() || c == '_' || c == ':' || c == '-';
    line.match_indices('<').find_map(|(at, _)| {
        let rest = &line[at + 1..];
        let rest = rest.strip_prefix('/').unwrap_or(rest);
        if !rest.starts_with(|c: char| c.is_ascii_alphabetic()) {
            return None;
        }
        let name = rest.find(|c: char| !word(c)).unwrap_or(rest.len());
        let tail = &rest[name..];
        let base = line.len() - tail.len();
        if tail.starts_with(char::is_whitespace)
            && let Some(close) = tail.find(['<', '>'])
            && tail[close..].starts_with('>')
        {
            return Some((at, base + close + 1));
        }
        let close = usize::from(tail.starts_with('/'));
        tail[close..]
            .starts_with('>')
            .then_some((at, base + close + 1))
    })
}

fn column(source: &str, at: usize) -> usize {
    let line = source.get(..at).unwrap_or("");
    line.rsplit('\n').next().unwrap_or("").chars().count() + 1
}

/// A list item whose marker sits left of its list's first marker and right of the item that holds the list.
fn marker_between_lists(source: &str, all: &[Node<'_>], at: usize, out: &mut Vec<Warning>) {
    let Some(node) = all.get(at) else { return };
    let mut chain = ancestors(all, at);
    let (Some(list), Some(outer)) = (chain.next(), chain.next()) else {
        return;
    };
    if list.kind != "list" || outer.kind != "list_item" {
        return;
    }
    let (Some((start, _)), Some((first, _)), Some((outer_at, _))) = (
        bounds(node.value),
        list.value
            .at("children")
            .as_array()
            .and_then(Arr::first)
            .and_then(bounds),
        bounds(outer.value),
    ) else {
        return;
    };
    let mark = column(source, start);
    let outer_mark = column(source, outer_at);
    let width = source
        .get(outer_at..)
        .unwrap_or("")
        .split(['\n', ' ', '\t'])
        .next()
        .map_or(1, |token| token.chars().count());
    if outer_mark < mark && mark < outer_mark + width + 1 && mark < column(source, first) {
        push(out, "list-marker-below-content-column", (start, start + 1));
    }
}
