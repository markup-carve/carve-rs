//! The sidecar facts the writer reads (C2-W-083, C2-W-084): from a supplied
//! sidecar (C2-L-009), or read from the source of a `format` call by the
//! positions of its parse.

use super::node::{N, children, classes, id, list, s, span, text, ty};
use super::state::{Facts, Wrap, Writer};
use crate::hash::{AddrMap, AddrSet};
use crate::view::{Arr, V};
use std::collections::BTreeMap;

/// One node entry of a sidecar, as far as the writer reads it.
#[derive(Debug, Clone, Default)]
pub struct Entry {
    pub path: String,
    pub facts: Facts,
}

/// The entries of a sidecar JSON value (`nodes`).
pub fn from_layout(layout: V<'_>) -> Vec<Entry> {
    list(layout, "nodes")
        .unwrap_or_else(Arr::empty)
        .iter()
        .filter_map(|e| {
            let path = s(e, "path")?.to_string();
            let wraps = list(e, "wraps").map(|ws| {
                ws.iter()
                    .filter_map(|w| {
                        Some(Wrap {
                            line: w.get("line")?.as_i64()?,
                            path: s(w, "path")?.to_string(),
                            offset: usize::try_from(w.get("offset")?.as_u64()?).ok()?,
                            row: w.get("row").and_then(V::as_i64),
                        })
                    })
                    .collect()
            });
            let spelling = e.get("optionSpelling").filter(|m| m.is_object()).map(|m| {
                let mut out = BTreeMap::new();
                m.each(&mut |k, v| {
                    if let Some(v) = v.as_str() {
                        out.insert(k.to_string(), v.to_string());
                    }
                });
                out
            });
            Some(Entry {
                path,
                facts: Facts {
                    bare: e.get("bare").and_then(V::as_bool),
                    fenced: e.get("fenced").and_then(V::as_bool),
                    hard_break_opener: e.get("hardBreakOpener").and_then(V::as_bool),
                    continuation_lines: e.get("continuationLines").and_then(V::as_i64),
                    wraps,
                    raw_ref: s(e, "rawRef").map(str::to_string),
                    option_spelling: spelling,
                    source_format: s(e, "sourceFormat").map(str::to_string),
                    header_rows: e.get("headerRows").and_then(V::as_i64),
                },
            })
        })
        .collect()
}

fn node_at<'t>(doc: N<'t>, path: &str) -> Option<N<'t>> {
    super::node::pointer(doc, path).filter(|v| v.get("type").is_some())
}

/// C2-L-021: whether the wrap at offset 0 of the node at `path` can mark its start.
fn starts_fragment(doc: V<'_>, cell: V<'_>, path: &str) -> bool {
    let Some(cut) = path.rfind("/children/") else {
        return false;
    };
    let Some(parent) = path.get(..cut).and_then(|p| node_at(doc, p)) else {
        return false;
    };
    if children(cell).first().map(id) != Some(id(parent)) {
        return false;
    }
    let Ok(at) = path
        .get(cut + "/children/".len()..)
        .unwrap_or("")
        .parse::<usize>()
    else {
        return false;
    };
    at == 0
        || children(parent)
            .get(at - 1)
            .is_some_and(|b| ty(b) == "text" && text(b).ends_with(' '))
}

/// C2-L-020, C2-L-021, C2-L-006: the wrap places of a cell; a contradiction drops all of its wraps.
fn read_wraps<'t>(
    w: &mut Writer<'t>,
    facts: &mut AddrMap<usize, Facts>,
    doc: N<'t>,
    cell: N<'t>,
    entry: &Entry,
) {
    let mut cursors: AddrMap<usize, Vec<usize>> = AddrMap::default();
    let mut starts: AddrSet<usize> = AddrSet::default();
    for wrap in entry.facts.wraps.iter().flatten() {
        let ok = node_at(doc, &wrap.path).is_some_and(|node| {
            if ty(node) != "text" {
                if wrap.offset != 0
                    || starts.contains(&id(node))
                    || !starts_fragment(doc, cell, &wrap.path)
                {
                    return false;
                }
                starts.insert(id(node));
                return true;
            }
            let value = text(node);
            let points = cursors.entry(id(node)).or_default();
            let at = wrap.offset;
            if at > value.len() || !value.is_char_boundary(at) {
                return false;
            }
            let fits = if at == 0 {
                points.is_empty()
            } else {
                at > points.last().copied().unwrap_or(0)
                    && value.as_bytes().get(at - 1) == Some(&b' ')
            };
            if fits {
                points.push(at);
            }
            fits
        });
        if !ok {
            if let Some(f) = facts.get_mut(&id(cell)) {
                f.wraps = Some(Vec::new());
            }
            return;
        }
    }
    let points = w.wrap_points.get_or_insert_with(AddrMap::default);
    points.extend(cursors);
    w.wrap_starts
        .get_or_insert_with(AddrSet::default)
        .extend(starts);
}

/// Takes the facts of `entries` into the writer.
pub fn read<'t>(w: &mut Writer<'t>, doc: N<'t>, entries: &[Entry]) {
    let mut facts: AddrMap<usize, Facts> = AddrMap::default();
    for entry in entries {
        let Some(node) = node_at(doc, &entry.path) else {
            continue;
        };
        facts.insert(id(node), entry.facts.clone());
        if entry.facts.wraps.as_ref().is_some_and(|ws| !ws.is_empty()) {
            read_wraps(w, &mut facts, doc, node, entry);
        }
    }
    w.facts = Some(facts);
}

/// The facts the writer reads, read from `src` by the positions of its parse `doc`.
pub fn from_source(doc: V<'_>, src: &str) -> Vec<Entry> {
    let mut out = Vec::new();
    walk(doc, src, &mut String::new(), 0, &mut out);
    out
}

/// `path` is the pointer of `node`, one buffer extended and cut back per step.
fn walk(node: V<'_>, src: &str, path: &mut String, quote_depth: usize, out: &mut Vec<Entry>) {
    let mut prefixed = ty(node) == "block_quote";
    if let Some(facts) = node_facts(node, src, path, quote_depth, out) {
        prefixed &= facts.fenced != Some(true);
        out.push(Entry {
            path: path.clone(),
            facts,
        });
    }
    let inner = quote_depth + usize::from(prefixed);
    let include = ty(node) == "include";
    super::node::for_each_slot(node, true, &mut |nodes, step, single| {
        if include && step == "children" {
            return;
        }
        for (i, c) in nodes.iter().enumerate() {
            let len = path.len();
            path.push('/');
            path.push_str(step);
            if !single {
                path.push('/');
                push_index(path, i);
            }
            walk(c, src, path, inner, out);
            path.truncate(len);
        }
    });
}

fn push_index(path: &mut String, i: usize) {
    let mut digits = [0u8; 20];
    let mut at = digits.len();
    let mut n = i;
    loop {
        at -= 1;
        if let Some(d) = digits.get_mut(at) {
            *d = b'0' + u8::try_from(n % 10).unwrap_or(0);
        }
        n /= 10;
        if n == 0 {
            break;
        }
    }
    for &d in digits.get(at..).unwrap_or(&[]) {
        path.push(char::from(d));
    }
}

/// The facts of one node, or `None` when its type records none.
fn node_facts(
    node: V<'_>,
    src: &str,
    path: &str,
    quote_depth: usize,
    out: &mut Vec<Entry>,
) -> Option<Facts> {
    let mut facts = Facts::default();
    match ty(node) {
        "list" => {
            let ordered = node.get("marker").and_then(|m| s(m, "kind")) == Some("ordered");
            let (start, _) = children(node).first().and_then(span).filter(|_| ordered)?;
            facts.bare = Some(src.as_bytes().get(start) == Some(&b'.'));
        }
        "block_quote" => {
            facts.fenced = Some(opener(node, src, quote_depth).is_some_and(|o| o.starts_with(':')));
        }
        "div" if classes(node).contains(&super::blocks::HARD_BREAKS_CLASS) => {
            facts.hard_break_opener = opener(node, src, quote_depth).map(|o| {
                o.trim_start_matches(':')
                    .trim_start_matches([' ', '\t'])
                    .starts_with('\\')
            });
        }
        "table" => table_facts(node, src, path, quote_depth, out, &mut facts),
        "link" | "image" if node.get("key").is_some() => {
            facts.raw_ref = raw_ref(node, src, quote_depth);
        }
        "include" => facts.option_spelling = option_spelling(node, src, quote_depth),
        _ => return None,
    }
    Some(facts)
}

/// The offset of a block's opener, behind the attribute and comment lines its span starts with.
/// A later line can start left of the first one, so it is read behind the prefixes of the
/// `quotes` enclosing `>` quotes rather than from the column of the span's start.
fn opener_at(node: V<'_>, src: &str, quotes: usize) -> Option<usize> {
    let (start, end) = span(node)?;
    let b = src.as_bytes();
    let skip = |mut at: usize, set: &[u8]| {
        while b.get(at).is_some_and(|c| set.contains(c)) {
            at += 1;
        }
        at
    };
    let mut at = skip(start, b" \t");
    loop {
        let rest = src.get(at..)?;
        let attribute = rest.starts_with('{') && !rest.starts_with("{{");
        if !attribute && !rest.starts_with("%%") {
            return (at < end.max(start + 1)).then_some(at);
        }
        at += rest.find('\n')? + 1;
        if at >= end {
            return None;
        }
        for _ in 0..quotes {
            at = skip(at, b" \t");
            if b.get(at) != Some(&b'>') {
                break;
            }
            at += 1;
        }
        at = skip(at, b" \t");
    }
}

/// The opener line of a block, behind its attribute lines.
fn opener<'s>(node: V<'_>, src: &'s str, quotes: usize) -> Option<&'s str> {
    let at = opener_at(node, src, quotes)?;
    let rest = src.get(at..)?;
    Some(
        rest.get(..rest.find('\n').unwrap_or(rest.len()))?
            .trim_end_matches('\r'),
    )
}

fn attribute_text<'s>(node: V<'_>, src: &'s str, quotes: usize) -> &'s str {
    let (Some((start, _)), Some(at)) = (span(node), opener_at(node, src, quotes)) else {
        return "";
    };
    src.get(start..at).unwrap_or("")
}

fn table_facts(
    node: V<'_>,
    src: &str,
    path: &str,
    quotes: usize,
    out: &mut Vec<Entry>,
    facts: &mut Facts,
) {
    let Some(head) = opener(node, src, quotes) else {
        return;
    };
    if head.starts_with('|') {
        facts.source_format = Some("pipe".into());
        rows(node, src, opener_at(node, src, quotes), path, out);
        return;
    }
    let word = head.trim_start_matches('`');
    facts.source_format = Some(
        if word.starts_with("tsv") {
            "tsv"
        } else {
            "csv"
        }
        .into(),
    );
    if declares_header_rows(attribute_text(node, src, quotes)) {
        let count = children(node)
            .iter()
            .filter(|g| ty(g) == "table_head")
            .map(|g| children(g).iter().filter(|r| ty(r) == "table_row").count())
            .sum::<usize>();
        facts.header_rows = i64::try_from(count).ok();
    }
}

/// C2-L-057: whether the attribute lines give `header-rows` a valid count (C2-B-210).
fn declares_header_rows(text: &str) -> bool {
    if !text.contains("header-rows") {
        return false;
    }
    let mut block = crate::attr::AttrBlock::default();
    for line in text.lines() {
        if let Some((parsed, _)) = line
            .find('{')
            .filter(|&at| line.get(..at).is_some_and(|p| !p.contains("%%")))
            .and_then(|at| crate::attr::parse_block(line, at))
        {
            block.merge(parsed);
        }
    }
    block
        .attrs
        .get("header-rows")
        .is_some_and(|v| v.bytes().all(|b| b.is_ascii_digit()))
}

/// The cell fragments of a row line or a continuation line: the text between unescaped pipes.
fn fragments(line: &str) -> Vec<&str> {
    let b = line.as_bytes();
    let mut out = Vec::new();
    let mut start = 1;
    let mut at = 1;
    while at < b.len() {
        match b.get(at) {
            Some(b'\\') => at += 2,
            Some(b'`') => {
                let run = b
                    .get(at..)
                    .map_or(0, |r| r.iter().take_while(|&&x| x == b'`').count());
                let mut k = at + run;
                let mut closed = None;
                while k < b.len() {
                    if b.get(k) == Some(&b'`') {
                        let n = b
                            .get(k..)
                            .map_or(0, |r| r.iter().take_while(|&&x| x == b'`').count());
                        if n == run {
                            closed = Some(k + n);
                            break;
                        }
                        k += n;
                    } else {
                        k += 1;
                    }
                }
                at = closed.unwrap_or(at + run);
            }
            Some(b'|') => {
                out.push(line.get(start..at).unwrap_or(""));
                start = at + 1;
                at += 1;
            }
            _ => at += 1,
        }
    }
    out
}

fn unescaped_len(fragment: &str) -> usize {
    let b = fragment.as_bytes();
    let mut len = 0;
    let mut at = 0;
    while at < b.len() {
        if b.get(at) == Some(&b'\\') && b.get(at + 1).is_some_and(u8::is_ascii_punctuation) {
            at += 1;
        }
        len += 1;
        at += 1;
    }
    len
}

/// The content of the first fragment of a cell on its row line: without its header mark and its brace block.
fn content_fragment(fragment: &str) -> &str {
    let t = fragment.strip_prefix('=').unwrap_or(fragment);
    let t = if t.starts_with('{') {
        t.find('}').and_then(|p| t.get(p + 1..)).unwrap_or(t)
    } else {
        t
    };
    t.trim_matches([' ', '\t'])
}

/// A table's later lines can start in another column than its first row.
fn table_source_line(line: &str, column: usize, first: bool) -> &str {
    let anchored = line.get(column..).unwrap_or("");
    if first {
        return if anchored.starts_with('|') {
            anchored.trim_end_matches('\r')
        } else {
            ""
        };
    }
    let early_continuation = line.get(..column).and_then(|prefix| prefix.find('+'));
    let at = early_continuation
        .or_else(|| anchored.starts_with(['|', '+']).then_some(column))
        .or_else(|| line.find(['|', '+']));
    let Some(at) = at else { return "" };
    let prefix = line.get(..at).unwrap_or("");
    if prefix.contains("%%") || prefix.contains('{') {
        return "";
    }
    line.get(at..).unwrap_or("").trim_end_matches('\r')
}

fn rows(table: V<'_>, src: &str, first_row: Option<usize>, path: &str, out: &mut Vec<Entry>) {
    let (Some((_, table_end)), Some(start)) = (span(table), first_row) else {
        return;
    };
    // A table's source span includes its caption. Caption text cannot supply
    // row or continuation spelling facts, even when it contains `+` or `|`.
    let end = table
        .get("caption")
        .and_then(span)
        .map_or(table_end, |(caption_start, _)| caption_start.min(table_end));
    let line_start = src
        .get(..start)
        .and_then(|p| p.rfind('\n'))
        .map_or(0, |p| p + 1);
    let column = start - line_start;
    let Some(body) = src.get(line_start..end) else {
        return;
    };
    let mut lines: Vec<(&str, Vec<&str>)> = Vec::new();
    for line in body.split('\n') {
        let l = table_source_line(line, column, lines.is_empty());
        if l.starts_with('|') {
            lines.push((l, Vec::new()));
        } else if l.starts_with('+') {
            if let Some(last) = lines.last_mut() {
                last.1.push(l);
            }
        }
    }
    let mut rows = Vec::new();
    for (g, group) in children(table).iter().enumerate() {
        for (r, row) in children(group).iter().enumerate() {
            if ty(row) == "table_row" {
                rows.push((g, r, row));
            }
        }
    }
    if rows.len() != lines.len() {
        return;
    }
    for ((g, r, row), (first, more)) in rows.into_iter().zip(lines) {
        if !more.is_empty() {
            let row_path = format!("{path}/children/{g}/children/{r}");
            row_facts(row, first, &more, &row_path, out);
        }
    }
}

fn row_facts(row: V<'_>, line: &str, continuation: &[&str], path: &str, out: &mut Vec<Entry>) {
    if continuation.is_empty() {
        return;
    }
    out.push(Entry {
        path: path.to_string(),
        facts: Facts {
            continuation_lines: i64::try_from(continuation.len()).ok(),
            ..Facts::default()
        },
    });
    let first = fragments(line);
    let more: Vec<Vec<&str>> = continuation.iter().map(|l| fragments(l)).collect();
    for (c, cell) in children(row).iter().enumerate() {
        let Some(block) = children(cell).single() else {
            continue;
        };
        let Some(t) = children(block).single() else {
            continue;
        };
        if ty(t) != "text" {
            continue;
        }
        let mut parts = vec![first.get(c).map_or("", |f| content_fragment(f))];
        parts.extend(
            more.iter()
                .map(|m| m.get(c).map_or("", |f| f.trim_matches([' ', '\t']))),
        );
        let mut wraps = Vec::new();
        let mut offset = 0usize;
        let mut seen = false;
        for (k, part) in parts.iter().enumerate() {
            if part.is_empty() {
                continue;
            }
            if seen {
                offset += 1;
            }
            if k > 0 {
                wraps.push(Wrap {
                    line: i64::try_from(k).unwrap_or(0),
                    path: format!("{path}/children/{c}/children/0/children/0"),
                    offset,
                    row: None,
                });
            }
            offset += unescaped_len(part);
            seen = true;
        }
        if !wraps.is_empty() {
            out.push(Entry {
                path: format!("{path}/children/{c}"),
                facts: Facts {
                    wraps: Some(wraps),
                    ..Facts::default()
                },
            });
        }
    }
}

fn raw_ref(node: V<'_>, src: &str, quote_depth: usize) -> Option<String> {
    let (start, end) = span(node)?;
    let start = if ty(node) == "image" {
        start + 1
    } else {
        start
    };
    let raw = src.get(start..end)?;
    // The brace block is written from the fields; `{:lang}` alone sets no `attrs` (C2-W-048).
    let raw = if node.get("attrs").is_some() || node.get("lang").is_some() {
        let b = raw.as_bytes();
        let cut = (0..b.len())
            .rev()
            .find(|&p| b.get(p) == Some(&b']') && b.get(p + 1) == Some(&b'{'))?;
        raw.get(..=cut)?
    } else {
        raw
    };
    if !raw.contains('\n') {
        return Some(raw.to_string());
    }
    let lines: Vec<String> = raw
        .split('\n')
        .enumerate()
        .map(|(i, l)| {
            if i == 0 {
                return l.to_string();
            }
            let mut t = l.trim_start_matches([' ', '\t']);
            for _ in 0..quote_depth {
                let Some(rest) = t.strip_prefix('>') else {
                    break;
                };
                t = rest.trim_start_matches([' ', '\t']);
            }
            t.to_string()
        })
        .collect();
    Some(lines.join("\n"))
}

fn option_spelling(node: V<'_>, src: &str, quotes: usize) -> Option<BTreeMap<String, String>> {
    let line = opener(node, src, quotes)?;
    let b = line.as_bytes();
    let options = node.get("options").filter(|o| o.is_object())?;
    let mut out = BTreeMap::new();
    let mut at = 0;
    while at < b.len() {
        if b.get(at) != Some(&b'@') {
            at += 1;
            continue;
        }
        let key_end = at
            + 1
            + b.get(at + 1..).map_or(0, |r| {
                r.iter()
                    .take_while(|x| x.is_ascii_alphanumeric() || **x == b'_' || **x == b'-')
                    .count()
            });
        let key = line.get(at + 1..key_end).unwrap_or("");
        if b.get(key_end) != Some(&b':') {
            at = key_end.max(at + 1);
            continue;
        }
        let v = key_end + 1;
        let v_end = match b.get(v) {
            Some(&q @ (b'"' | b'\'')) => {
                let mut k = v + 1;
                while k < b.len() && b.get(k) != Some(&q) {
                    k += if b.get(k) == Some(&b'\\') { 2 } else { 1 };
                }
                (k + 1).min(b.len())
            }
            _ => {
                v + b.get(v..).map_or(0, |r| {
                    r.iter()
                        .take_while(|x| !matches!(x, b'@' | b'}' | b' ' | b'\t'))
                        .count()
                })
            }
        };
        if options.get(key).is_some() && !out.contains_key(key) {
            out.insert(
                key.to_string(),
                line.get(v..v_end).unwrap_or("").to_string(),
            );
        }
        at = v_end.max(at + 1);
    }
    (!out.is_empty()).then_some(out)
}
