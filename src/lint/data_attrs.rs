//! Attribute records of the lint model (C2-W-148): each part of an attribute block that a node took,
//! with the facts of its owner.

use crate::view::V;

/// The node an attribute block attaches to, with the facts of `attribute_owner_fields`.
pub(super) struct Owner {
    pub range: u32,
    pub start: usize,
    pub end: usize,
    pub opener_start: usize,
    pub format: &'static str,
    pub columns: usize,
    pub rows: [usize; 3],
    pub align_overlap: String,
    pub valign_overlap: String,
}

/// One part of an attribute block.
pub(super) struct Part {
    pub owner: u32,
    pub kind: &'static str,
    pub key: String,
    pub value: String,
    pub start: usize,
    pub key_end: usize,
    pub end: usize,
    pub index: usize,
    pub last_key: bool,
    pub kept: bool,
}

#[derive(Default)]
pub(super) struct Table {
    pub owners: Vec<Owner>,
    pub parts: Vec<Part>,
}

const fn name_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'-'
}

fn run(b: &[u8], mut i: usize, end: usize, f: impl Fn(u8) -> bool) -> usize {
    while i < end && f(b[i]) {
        i += 1;
    }
    i
}

fn skip_spaces(b: &[u8], mut i: usize, end: usize) -> usize {
    while i < end && b[i] == b' ' {
        i += 1;
    }
    i
}

/// C2-I-010: the text with each backslash before ASCII punctuation dropped.
fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' && chars.peek().is_some_and(char::is_ascii_punctuation) {
            continue;
        }
        out.push(c);
    }
    out
}

/// The parts of one block at `at`, ending before `end`; the offset behind its `}`, or `None` when it
/// is no valid block (C2-I-028 to C2-I-034).
fn read_block(s: &str, at: usize, end: usize, out: &mut Vec<Part>) -> Option<usize> {
    let b = s.as_bytes();
    let mut p = skip_spaces(b, at + 1, end);
    let mut parts = 0;
    while p < end {
        if b[p] == b'}' {
            return (parts > 0).then_some(p + 1);
        }
        let after = read_part(s, p, end, out)?;
        p = skip_spaces(b, after, end);
        if p == after && p < end && b[p] != b'}' {
            return None;
        }
        parts += 1;
    }
    None
}

fn part(
    kind: &'static str,
    key: &str,
    value: String,
    start: usize,
    key_end: usize,
    end: usize,
) -> Part {
    Part {
        owner: 0,
        kind,
        key: key.to_owned(),
        value,
        start,
        key_end,
        end,
        index: 0,
        last_key: kind == "key",
        kept: false,
    }
}

fn read_part(s: &str, p: usize, end: usize, out: &mut Vec<Part>) -> Option<usize> {
    let b = s.as_bytes();
    match b[p] {
        mark @ (b'#' | b'.') => {
            let after = run(b, p + 1, end, name_char);
            if after == p + 1 {
                return None;
            }
            let kind = if mark == b'#' { "id" } else { "class" };
            out.push(part(kind, "", s[p + 1..after].to_owned(), p, after, after));
            Some(after)
        }
        b':' => {
            let word = |c: u8| c.is_ascii_alphanumeric();
            let mut q = run(b, p + 1, end, word);
            while q > p + 1 && q < end && b[q] == b'-' {
                let after = run(b, q + 1, end, word);
                if after == q + 1 {
                    return None;
                }
                q = after;
            }
            out.push(part("lang", "", s[p + 1..q].to_owned(), p, q, q));
            Some(q)
        }
        first if first.is_ascii_alphabetic() || first == b'_' => {
            let key_end = run(b, p + 1, end, name_char);
            if key_end < end && b[key_end] == b'=' {
                return read_value(s, p, key_end, end, out);
            }
            // C2-I-031: a bare key does not start with `_`.
            if first == b'_' {
                return None;
            }
            out.push(part(
                "key",
                &s[p..key_end],
                String::new(),
                p,
                key_end,
                key_end,
            ));
            Some(key_end)
        }
        _ => None,
    }
}

fn read_value(s: &str, p: usize, equals: usize, end: usize, out: &mut Vec<Part>) -> Option<usize> {
    let b = s.as_bytes();
    let key = &s[p..equals];
    let first = *b.get(equals + 1).filter(|_| equals + 1 < end)?;
    if matches!(first, b'"' | b'\'') {
        let mut q = equals + 2;
        while q < end {
            match b[q] {
                c if c == first => {
                    out.push(part(
                        "key",
                        key,
                        unescape(&s[equals + 2..q]),
                        p,
                        equals,
                        q + 1,
                    ));
                    return Some(q + 1);
                }
                b'\n' => return None,
                b'\\' if b.get(q + 1) != Some(&b'\n') => q += 2,
                _ => q += 1,
            }
        }
        return None;
    }
    let after = run(b, equals + 1, end, |c| {
        !matches!(c, b' ' | b'\t' | b'"' | b'\'' | b'\\' | b'|' | b'}' | b'\n')
    });
    let next = b.get(after).filter(|_| after < end);
    (after > equals + 1 && matches!(next, Some(b' ' | b'}'))).then(|| {
        out.push(part(
            "key",
            key,
            s[equals + 1..after].to_owned(),
            p,
            equals,
            after,
        ));
        after
    })
}

/// The end of the brace block at `at` on its line: its first `}` outside a quoted value.
fn block_end(b: &[u8], at: usize, end: usize) -> Option<usize> {
    let mut quote = None;
    let mut i = at + 1;
    while i < end {
        match (b[i], quote) {
            (b'\n', _) => return None,
            (b'\\', Some(_)) => i += 1,
            (c @ (b'"' | b'\''), None) if i > 0 && b[i - 1] == b'=' => quote = Some(c),
            (c, Some(q)) if c == q => quote = None,
            (b'}', None) => return Some(i + 1),
            _ => {}
        }
        i += 1;
    }
    None
}

/// C2-B-182: the alignment marks at the start of the first block of a cell prefix.
fn cell_marks(b: &[u8], at: usize) -> usize {
    let mut len = 0;
    if b.get(at + 1)
        .is_some_and(|c| matches!(c, b'<' | b'>' | b'~'))
    {
        len += 1;
    }
    if b.get(at + 1 + len)
        .is_some_and(|c| matches!(c, b'^' | b'~' | b'v'))
    {
        len += 1;
    }
    len
}

/// The run of glued valid blocks at `from`, its parts taken; returns where the run ends.
fn read_run(s: &str, from: usize, end: usize, cell: bool, out: &mut Vec<Part>) -> usize {
    let b = s.as_bytes();
    let mut at = from;
    while b.get(at) == Some(&b'{') {
        let Some(stop) = block_end(b, at, end) else {
            return at;
        };
        let marks = if cell && at == from {
            cell_marks(b, at)
        } else {
            0
        };
        let kept = out.len();
        let marks_only = marks > 0 && skip_spaces(b, at + 1 + marks, stop) == stop - 1;
        if !marks_only && read_block(s, at + marks, stop, out) != Some(stop) {
            out.truncate(kept);
            return at;
        }
        at = stop;
    }
    at
}

fn trimmed_end(b: &[u8], from: usize, mut end: usize) -> usize {
    while end > from && matches!(b[end - 1], b' ' | b'\t') {
        end -= 1;
    }
    end
}

/// The nodes of a table group type and their rows.
fn group_rows(node: V<'_>, group: &str) -> usize {
    node.get("children")
        .and_then(V::as_array)
        .into_iter()
        .flat_map(|items| items.iter())
        .filter(|child| child.ty().as_str() == Some(group))
        .map(|child| {
            child
                .get("children")
                .and_then(V::as_array)
                .into_iter()
                .flat_map(|rows| rows.iter())
                .filter(|row| row.ty().as_str() == Some("table_row"))
                .count()
        })
        .sum()
}

/// C2-B-213: the columns where `aligns` or `valigns` and a mark of the first head row set one axis.
fn overlaps(node: V<'_>) -> (String, String) {
    let mut align = Vec::new();
    let mut valign = Vec::new();
    let children = node.get("children").and_then(V::as_array);
    let first = children
        .into_iter()
        .flat_map(|items| items.iter())
        .find(|child| child.ty().as_str() == Some("table_head"))
        .and_then(|head| head.get("children").and_then(V::as_array))
        .into_iter()
        .flat_map(|rows| rows.iter())
        .find(|row| row.ty().as_str() == Some("table_row"));
    if let (Some(first), Some(columns)) = (first, node.get("columns").and_then(V::as_array)) {
        let cells = first.get("children").and_then(V::as_array);
        for (at, column) in columns.iter().enumerate() {
            let Some(cell) = cells.and_then(|cells| cells.get(at)) else {
                continue;
            };
            let has =
                |value: V<'_>, key: &str| value.get(key).is_some_and(|v| !matches!(v, V::Null));
            if has(column, "align") && has(cell, "align") {
                align.push((at + 1).to_string());
            } else if has(column, "valign") && has(cell, "valign") {
                valign.push((at + 1).to_string());
            }
        }
    }
    (align.join(" "), valign.join(" "))
}

/// C2-I-039, C2-A-035: whether two keys of one owner have the name the tree merges by.
fn same_name(a: &str, b: &str, span: bool) -> bool {
    if a == b {
        return true;
    }
    if !a.eq_ignore_ascii_case(b) {
        return false;
    }
    let semantic = |k: &str| matches!(k, "abbr" | "time" | "kbd");
    !span || (semantic(a)) == (semantic(b))
}

/// What the table builder needs of one node.
pub(super) struct Node<'k, 'v> {
    pub range: u32,
    pub kind: &'k str,
    pub value: V<'v>,
    pub start: usize,
    pub end: usize,
    pub inline: bool,
    /// The source lines of the node: start offset of each line and its end.
    pub lines: Vec<(usize, usize)>,
    pub quotes: usize,
}

/// The attribute records of the nodes, in source order within each owner.
pub(super) fn build<'k, 'v>(source: &str, nodes: impl Iterator<Item = Node<'k, 'v>>) -> Table {
    let mut table = Table::default();
    let b = source.as_bytes();
    for node in nodes {
        let first = table.parts.len();
        let mut opener = node.start;
        if node.inline {
            if node.end > node.start && b[node.end - 1] == b'}' {
                let mut at = node.start + 1;
                while let Some(found) = source.get(at..node.end).and_then(|rest| rest.find('{')) {
                    let from = at + found;
                    if read_run(source, from, node.end, false, &mut table.parts) == node.end {
                        break;
                    }
                    table.parts.truncate(first);
                    at = from + 1;
                }
            }
        } else if matches!(node.kind, "list_item" | "table_cell") {
            let cell = node.kind == "table_cell";
            let mut at = node.start;
            if cell {
                if b.get(at) == Some(&b'|') {
                    at += 1;
                }
                if b.get(at) == Some(&b'=') {
                    at += 1;
                }
            } else {
                while at < node.end && b[at] != b'{' && b[at] > b' ' {
                    at += 1;
                }
            }
            if b.get(at) == Some(&b'{') {
                read_run(source, at, node.end, cell, &mut table.parts);
            }
        } else {
            for (number, &(line_start, line_end)) in node.lines.iter().enumerate() {
                let from = if number == 0 {
                    node.start
                } else {
                    let text = &source[line_start..line_end];
                    line_start + super::data_model::quoted_start_pub(text, node.quotes)
                };
                if b.get(from) != Some(&b'{') {
                    opener = from;
                    break;
                }
                let end = trimmed_end(b, from, line_end);
                let kept = table.parts.len();
                if read_run(source, from, end, false, &mut table.parts) != end {
                    table.parts.truncate(kept);
                    opener = from;
                    break;
                }
            }
        }
        if table.parts.len() == first {
            continue;
        }
        let owner = table.owners.len() as u32;
        let is_table = node.kind == "table";
        let (align_overlap, valign_overlap) = if is_table {
            overlaps(node.value)
        } else {
            Default::default()
        };
        let keys: Vec<&str> = node
            .value
            .get("attrs")
            .and_then(|attrs| attrs.get("keyValues"))
            .and_then(V::as_array)
            .into_iter()
            .flat_map(|pairs| pairs.iter())
            .filter_map(|pair| {
                pair.as_array()
                    .and_then(|pair| pair.get(0))
                    .and_then(V::as_str)
            })
            .collect();
        let span = node.kind == "span";
        let parts = &mut table.parts[first..];
        for at in 0..parts.len() {
            parts[at].owner = owner;
            parts[at].index = at;
            if parts[at].kind == "key" {
                parts[at].kept = keys.contains(&parts[at].key.as_str());
                parts[at].last_key = !parts[at + 1..].iter().any(|later| {
                    later.kind == "key" && same_name(&parts[at].key, &later.key, span)
                });
            }
        }
        table.owners.push(Owner {
            range: node.range,
            start: node.start,
            end: node.end,
            opener_start: opener,
            format: if !is_table {
                ""
            } else if source[opener..].starts_with("```") {
                let line_end = source[opener..]
                    .find('\n')
                    .map_or(source.len(), |at| opener + at);
                if source[opener..line_end].contains("tsv") {
                    "tsv"
                } else {
                    "csv"
                }
            } else {
                "pipe"
            },
            columns: if is_table {
                node.value
                    .get("columns")
                    .and_then(V::as_array)
                    .map_or(0, |items| items.len())
            } else {
                0
            },
            rows: if is_table {
                [
                    group_rows(node.value, "table_head"),
                    group_rows(node.value, "table_body"),
                    group_rows(node.value, "table_foot"),
                ]
            } else {
                [0; 3]
            },
            align_overlap,
            valign_overlap,
        });
    }
    table
}

/// C2-B-211, C2-B-212: the comma-separated entries of a value and their sum, rounded up to a whole
/// number (the rows compare it only with whole numbers); 0 when an entry is no decimal number.
pub(super) fn entries(value: &str) -> (usize, i64) {
    let count = value.matches(',').count() + 1;
    let mut whole: i64 = 0;
    let mut fraction = false;
    let mut scale = 0usize;
    let parsed: Option<Vec<(&str, &str)>> = value
        .split(',')
        .map(|entry| {
            let entry = entry.trim_matches([' ', '\t']);
            let (int, frac) = entry.split_once('.').unwrap_or((entry, ""));
            let digits = |text: &str| !text.is_empty() && text.bytes().all(|c| c.is_ascii_digit());
            (digits(int) && (frac.is_empty() && !entry.ends_with('.') || digits(frac)))
                .then_some((int, frac))
        })
        .collect();
    let Some(parsed) = parsed else {
        return (count, 0);
    };
    for (_, frac) in &parsed {
        scale = scale.max(frac.len());
    }
    // Sum the fractions as integers at one scale, so the carry into the whole part is exact.
    let mut carry: u128 = 0;
    for (int, frac) in &parsed {
        whole = whole.saturating_add(int.parse::<i64>().unwrap_or(i64::MAX));
        if scale > 0 && scale <= 30 {
            let padded = format!("{frac:0<scale$}");
            carry += padded.parse::<u128>().unwrap_or(0);
        }
    }
    if scale > 0 && scale <= 30 {
        let unit = 10u128.pow(scale as u32);
        whole = whole.saturating_add((carry / unit) as i64);
        fraction = carry % unit != 0;
    }
    (count, whole + i64::from(fraction))
}
