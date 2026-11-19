//! The sidecar facts the writer reads (C2-W-083, C2-W-084): from a supplied
//! sidecar (C2-L-009), or read from the source of a `format` call by the
//! positions of its parse.

use super::node::{N, children, classes, id, list, s, span, text, ty};
use super::state::{Facts, Wrap, Writer};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet};

/// One node entry of a sidecar, as far as the writer reads it.
#[derive(Debug, Clone, Default)]
pub struct Entry {
    pub path: String,
    pub facts: Facts,
}

/// The entries of a sidecar JSON value (`nodes`).
pub fn from_layout(layout: &Value) -> Vec<Entry> {
    list(layout, "nodes")
        .unwrap_or(&[])
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
                            row: w.get("row").and_then(Value::as_i64),
                        })
                    })
                    .collect()
            });
            let spelling = e.get("optionSpelling").and_then(Value::as_object).map(|m| {
                m.iter()
                    .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string())))
                    .collect()
            });
            Some(Entry {
                path,
                facts: Facts {
                    bare: e.get("bare").and_then(Value::as_bool),
                    fenced: e.get("fenced").and_then(Value::as_bool),
                    hard_break_opener: e.get("hardBreakOpener").and_then(Value::as_bool),
                    continuation_lines: e.get("continuationLines").and_then(Value::as_i64),
                    wraps,
                    raw_ref: s(e, "rawRef").map(str::to_string),
                    option_spelling: spelling,
                    source_format: s(e, "sourceFormat").map(str::to_string),
                    header_rows: e.get("headerRows").and_then(Value::as_i64),
                },
            })
        })
        .collect()
}

fn node_at<'t>(doc: N<'t>, path: &str) -> Option<N<'t>> {
    doc.pointer(path).filter(|v| v.get("type").is_some())
}

/// C2-L-021: whether the wrap at offset 0 of the node at `path` can mark its start.
fn starts_fragment(doc: &Value, cell: &Value, path: &str) -> bool {
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
    facts: &mut HashMap<usize, Facts>,
    doc: N<'t>,
    cell: N<'t>,
    entry: &Entry,
) {
    let mut cursors: HashMap<usize, Vec<usize>> = HashMap::new();
    let mut starts: HashSet<usize> = HashSet::new();
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
    let points = w.wrap_points.get_or_insert_with(HashMap::new);
    points.extend(cursors);
    w.wrap_starts
        .get_or_insert_with(HashSet::new)
        .extend(starts);
}

/// Takes the facts of `entries` into the writer.
pub fn read<'t>(w: &mut Writer<'t>, doc: N<'t>, entries: &[Entry]) {
    let mut facts: HashMap<usize, Facts> = HashMap::new();
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
pub fn from_source(doc: &Value, src: &str) -> Vec<Entry> {
    let mut out = Vec::new();
    walk(doc, src, "", false, &mut out);
    out
}

fn walk(node: &Value, src: &str, path: &str, quoted: bool, out: &mut Vec<Entry>) {
    let mut facts = Facts::default();
    let mut any = true;
    match ty(node) {
        "list" => {
            let ordered = node.get("marker").and_then(|m| s(m, "kind")) == Some("ordered");
            let first = children(node).first().and_then(span);
            match (ordered, first) {
                (true, Some((start, _))) => {
                    facts.bare = Some(src.as_bytes().get(start) == Some(&b'.'));
                }
                _ => any = false,
            }
        }
        "block_quote" => facts.fenced = Some(opener(node, src).is_some_and(|o| o.starts_with(':'))),
        "div" if classes(node).contains(&super::blocks::HARD_BREAKS_CLASS) => {
            facts.hard_break_opener = opener(node, src).map(|o| {
                o.trim_start_matches(':')
                    .trim_start_matches([' ', '\t'])
                    .starts_with('\\')
            });
        }
        "table" => table_facts(node, src, path, out, &mut facts),
        "link" | "image" if node.get("key").is_some() => facts.raw_ref = raw_ref(node, src, quoted),
        "include" => facts.option_spelling = option_spelling(node, src),
        _ => any = false,
    }
    if any {
        out.push(Entry {
            path: path.to_string(),
            facts,
        });
    }
    let inner = quoted || ty(node) == "block_quote";
    super::node::for_each_slot(node, true, &mut |nodes, step, single| {
        if ty(node) == "include" && step == "children" {
            return;
        }
        for (i, c) in nodes.iter().enumerate() {
            let p = if single {
                format!("{path}/{step}")
            } else {
                format!("{path}/{step}/{i}")
            };
            walk(c, src, &p, inner, out);
        }
    });
}

/// The opener line of a block, behind its attribute lines, from the column of its start.
fn opener<'s>(node: &Value, src: &'s str) -> Option<&'s str> {
    let (start, end) = span(node)?;
    let line_start = src.get(..start)?.rfind('\n').map_or(0, |p| p + 1);
    let column = start - line_start;
    let body = src.get(line_start..end.max(start))?;
    for line in body.split('\n') {
        let rest = line.get(column..).unwrap_or("");
        if rest.trim_start_matches([' ', '\t']).starts_with('{') {
            continue;
        }
        return Some(rest.trim_start_matches([' ', '\t']));
    }
    None
}

fn attribute_text<'s>(node: &Value, src: &'s str) -> &'s str {
    let Some((start, end)) = span(node) else {
        return "";
    };
    let line_start = src
        .get(..start)
        .and_then(|p| p.rfind('\n'))
        .map_or(0, |p| p + 1);
    let column = start - line_start;
    let body = src.get(line_start..end).unwrap_or("");
    let mut len = 0;
    for line in body.split('\n') {
        if !line
            .get(column..)
            .unwrap_or("")
            .trim_start()
            .starts_with('{')
        {
            break;
        }
        len += line.len() + 1;
    }
    src.get(line_start..line_start + len.min(body.len()))
        .unwrap_or("")
}

fn table_facts(node: &Value, src: &str, path: &str, out: &mut Vec<Entry>, facts: &mut Facts) {
    let Some(head) = opener(node, src) else {
        return;
    };
    if head.starts_with('|') {
        facts.source_format = Some("pipe".into());
        rows(node, src, path, out);
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
    if attribute_text(node, src).contains("header-rows=") {
        let count = children(node)
            .iter()
            .filter(|g| ty(g) == "table_head")
            .map(|g| children(g).iter().filter(|r| ty(r) == "table_row").count())
            .sum::<usize>();
        facts.header_rows = i64::try_from(count).ok();
    }
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

fn rows(table: &Value, src: &str, path: &str, out: &mut Vec<Entry>) {
    let Some((start, end)) = span(table) else {
        return;
    };
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
        let l = line.get(column..).unwrap_or("").trim_end_matches('\r');
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
                rows.push((format!("{path}/children/{g}/children/{r}"), row));
            }
        }
    }
    if rows.len() != lines.len() {
        return;
    }
    for ((row_path, row), (first, more)) in rows.into_iter().zip(lines) {
        row_facts(row, first, &more, &row_path, out);
    }
}

fn row_facts(row: &Value, line: &str, continuation: &[&str], path: &str, out: &mut Vec<Entry>) {
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
        let [block] = children(cell) else { continue };
        let [t] = children(block) else { continue };
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

fn raw_ref(node: &Value, src: &str, quoted: bool) -> Option<String> {
    let (start, end) = span(node)?;
    let start = if ty(node) == "image" {
        start + 1
    } else {
        start
    };
    let raw = src.get(start..end)?;
    let raw = if node.get("attrs").is_some() {
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
            if quoted {
                while let Some(rest) = t.strip_prefix('>') {
                    t = rest.trim_start_matches([' ', '\t']);
                }
            }
            t.to_string()
        })
        .collect();
    Some(lines.join("\n"))
}

fn option_spelling(node: &Value, src: &str) -> Option<BTreeMap<String, String>> {
    let (start, end) = span(node)?;
    let line = src.get(start..end)?.split('\n').next()?;
    let b = line.as_bytes();
    let options = node.get("options")?.as_object()?;
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
        if options.contains_key(key) && !out.contains_key(key) {
            out.insert(
                key.to_string(),
                line.get(v..v_end).unwrap_or("").to_string(),
            );
        }
        at = v_end.max(at + 1);
    }
    (!out.is_empty()).then_some(out)
}
