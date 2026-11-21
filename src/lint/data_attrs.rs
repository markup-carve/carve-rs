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

/// The byte at `i`, or 0 past the end.
fn byte(b: &[u8], i: usize) -> u8 {
    b.get(i).copied().unwrap_or(0)
}

fn slice(s: &str, from: usize, to: usize) -> &str {
    s.get(from..to).unwrap_or("")
}

const fn name_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'-'
}

fn run(b: &[u8], mut i: usize, end: usize, f: impl Fn(u8) -> bool) -> usize {
    while i < end && f(byte(b, i)) {
        i += 1;
    }
    i
}

fn skip_spaces(b: &[u8], i: usize, end: usize) -> usize {
    run(b, i, end, |c| c == b' ')
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
        if byte(b, p) == b'}' {
            return (parts > 0).then_some(p + 1);
        }
        let after = read_part(s, p, end, out)?;
        p = skip_spaces(b, after, end);
        if p == after && p < end && byte(b, p) != b'}' {
            return None;
        }
        parts += 1;
    }
    None
}

fn part(kind: &'static str, key: &str, value: String, span: (usize, usize, usize)) -> Part {
    Part {
        owner: 0,
        kind,
        key: key.to_owned(),
        value,
        start: span.0,
        key_end: span.1,
        end: span.2,
        index: 0,
        last_key: kind == "key",
        kept: false,
    }
}

fn read_part(s: &str, p: usize, end: usize, out: &mut Vec<Part>) -> Option<usize> {
    let b = s.as_bytes();
    match byte(b, p) {
        mark @ (b'#' | b'.') => {
            let after = run(b, p + 1, end, name_char);
            if after == p + 1 {
                return None;
            }
            let kind = if mark == b'#' { "id" } else { "class" };
            let name = slice(s, p + 1, after).to_owned();
            out.push(part(kind, "", name, (p, after, after)));
            Some(after)
        }
        b':' => read_language(s, p, end, out),
        first if first.is_ascii_alphabetic() || first == b'_' => {
            let key_end = run(b, p + 1, end, name_char);
            if key_end < end && byte(b, key_end) == b'=' {
                return read_value(s, p, key_end, end, out);
            }
            // C2-I-031: a bare key does not start with `_`.
            if first == b'_' {
                return None;
            }
            out.push(part(
                "key",
                slice(s, p, key_end),
                String::new(),
                (p, key_end, key_end),
            ));
            Some(key_end)
        }
        _ => None,
    }
}

/// C2-I-030: a language tag is empty or runs of letters and digits joined by single hyphens.
fn read_language(s: &str, p: usize, end: usize, out: &mut Vec<Part>) -> Option<usize> {
    let b = s.as_bytes();
    let word = |c: u8| c.is_ascii_alphanumeric();
    let mut q = run(b, p + 1, end, word);
    while q > p + 1 && q < end && byte(b, q) == b'-' {
        let after = run(b, q + 1, end, word);
        if after == q + 1 {
            return None;
        }
        q = after;
    }
    out.push(part("lang", "", slice(s, p + 1, q).to_owned(), (p, q, q)));
    Some(q)
}

fn read_value(s: &str, p: usize, equals: usize, end: usize, out: &mut Vec<Part>) -> Option<usize> {
    let b = s.as_bytes();
    let key = slice(s, p, equals);
    if equals + 1 >= end {
        return None;
    }
    let first = byte(b, equals + 1);
    if matches!(first, b'"' | b'\'') {
        let mut q = equals + 2;
        while q < end {
            match byte(b, q) {
                c if c == first => {
                    let value = unescape(slice(s, equals + 2, q));
                    out.push(part("key", key, value, (p, equals, q + 1)));
                    return Some(q + 1);
                }
                b'\n' => return None,
                b'\\' if byte(b, q + 1) != b'\n' => q += 2,
                _ => q += 1,
            }
        }
        return None;
    }
    let after = run(b, equals + 1, end, |c| {
        !matches!(c, b' ' | b'\t' | b'"' | b'\'' | b'\\' | b'|' | b'}' | b'\n')
    });
    let next = (after < end).then(|| byte(b, after));
    (after > equals + 1 && matches!(next, Some(b' ' | b'}'))).then(|| {
        let value = slice(s, equals + 1, after).to_owned();
        out.push(part("key", key, value, (p, equals, after)));
        after
    })
}

/// The end of the brace block at `at` on its line: its first `}` outside a quoted value.
fn block_end(b: &[u8], at: usize, end: usize) -> Option<usize> {
    let mut quote = None;
    let mut i = at + 1;
    while i < end {
        match (byte(b, i), quote) {
            (b'\n', _) => return None,
            (b'\\', Some(_)) => i += 1,
            (c @ (b'"' | b'\''), None) if byte(b, i - 1) == b'=' => quote = Some(c),
            (c, Some(q)) if c == q => quote = None,
            (b'}', None) => return Some(i + 1),
            _ => {}
        }
        i += 1;
    }
    None
}

/// C2-B-182: the alignment marks at the start of the first block of a cell prefix: a horizontal
/// mark with an optional vertical one, or `?` before a vertical one.
fn cell_marks(b: &[u8], at: usize) -> usize {
    let vertical = usize::from(matches!(byte(b, at + 2), b'^' | b'~' | b'v'));
    match byte(b, at + 1) {
        b'<' | b'>' | b'~' => 1 + vertical,
        b'?' if vertical == 1 => 2,
        _ => 0,
    }
}

/// The run of glued valid blocks at `from`, its parts taken; returns where the run ends.
fn read_run(s: &str, from: usize, end: usize, cell: bool, out: &mut Vec<Part>) -> usize {
    let b = s.as_bytes();
    let mut at = from;
    while byte(b, at) == b'{' {
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
    while end > from && matches!(byte(b, end - 1), b' ' | b'\t') {
        end -= 1;
    }
    end
}

fn children(node: V<'_>) -> impl Iterator<Item = V<'_>> {
    node.get("children")
        .and_then(V::as_array)
        .into_iter()
        .flat_map(crate::view::Arr::iter)
}

/// The rows of a table's groups of one type.
fn group_rows(node: V<'_>, group: &str) -> usize {
    children(node)
        .filter(|child| child.ty().as_str() == Some(group))
        .map(|child| {
            children(child)
                .filter(|row| row.ty().as_str() == Some("table_row"))
                .count()
        })
        .sum()
}

/// C2-B-213: the columns where `aligns` or `valigns` and a mark of the first head row set one axis.
fn overlaps(node: V<'_>) -> (String, String) {
    let mut align = Vec::new();
    let mut valign = Vec::new();
    let first = children(node)
        .find(|child| child.ty().as_str() == Some("table_head"))
        .and_then(|head| children(head).find(|row| row.ty().as_str() == Some("table_row")));
    if let (Some(first), Some(columns)) = (first, node.get("columns").and_then(V::as_array)) {
        let cells = first.get("children").and_then(V::as_array);
        let has = |value: V<'_>, key: &str| value.get(key).is_some_and(|v| !matches!(v, V::Null));
        for (at, column) in columns.iter().enumerate() {
            let Some(cell) = cells.and_then(|cells| cells.get(at)) else {
                continue;
            };
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
    let semantic = |k: &str| matches!(k, "abbr" | "time" | "kbd");
    a == b || (a.eq_ignore_ascii_case(b) && (!span || semantic(a) == semantic(b)))
}

/// What the table builder needs of one node.
pub(super) struct Node<'k, 'v> {
    pub range: u32,
    pub kind: &'k str,
    pub value: V<'v>,
    pub start: usize,
    pub end: usize,
    pub inline: bool,
    /// The byte ranges of the node's source lines.
    pub lines: Vec<(usize, usize)>,
    pub quotes: usize,
}

/// C2-I-036: the run of glued blocks that ends at the end of an inline node.
fn inline_parts(source: &str, node: &Node<'_, '_>, out: &mut Vec<Part>) {
    let first = out.len();
    if node.end <= node.start || byte(source.as_bytes(), node.end - 1) != b'}' {
        return;
    }
    let mut at = node.start + 1;
    while let Some(found) = source.get(at..node.end).and_then(|rest| rest.find('{')) {
        let from = at + found;
        if read_run(source, from, node.end, false, out) == node.end {
            return;
        }
        out.truncate(first);
        at = from + 1;
    }
}

/// C2-B-101, C2-B-171: the run of glued blocks that abuts the marker of an item or a cell.
fn marker_parts(source: &str, node: &Node<'_, '_>, out: &mut Vec<Part>) {
    let b = source.as_bytes();
    let cell = node.kind == "table_cell";
    let mut at = node.start;
    if cell {
        at += usize::from(byte(b, at) == b'|');
        at += usize::from(byte(b, at) == b'=');
    } else {
        at = run(b, at, node.end, |c| c != b'{' && c > b' ');
    }
    if byte(b, at) == b'{' {
        read_run(source, at, node.end, cell, out);
    }
}

/// C2-B-090, C2-B-091, C2-B-102: the attribute lines at the start of a block, read behind the
/// prefixes of its quotes; returns where the block's own text starts.
fn line_parts(source: &str, node: &Node<'_, '_>, out: &mut Vec<Part>) -> usize {
    let b = source.as_bytes();
    for (number, &(line_start, line_end)) in node.lines.iter().enumerate() {
        let from = if number == 0 {
            node.start
        } else {
            let text = slice(source, line_start, line_end);
            line_start + super::data_model::quoted_start_pub(text, node.quotes)
        };
        if byte(b, from) != b'{' {
            return from;
        }
        let end = trimmed_end(b, from, line_end);
        let kept = out.len();
        if read_run(source, from, end, false, out) != end {
            out.truncate(kept);
            return from;
        }
    }
    node.start
}

/// C2-B-180, C2-B-186: `pipe` for a pipe table, `csv` or `tsv` for a delimited one.
fn table_format(source: &str, opener: usize) -> &'static str {
    let rest = source.get(opener..).unwrap_or("");
    if !rest.starts_with("```") {
        return "pipe";
    }
    let line = rest.split('\n').next().unwrap_or("");
    if line.contains("tsv") { "tsv" } else { "csv" }
}

fn owner_of(source: &str, node: &Node<'_, '_>, opener: usize) -> Owner {
    let table = node.kind == "table";
    let (align_overlap, valign_overlap) = if table {
        overlaps(node.value)
    } else {
        Default::default()
    };
    Owner {
        range: node.range,
        start: node.start,
        end: node.end,
        opener_start: opener,
        format: if table {
            table_format(source, opener)
        } else {
            ""
        },
        columns: if table {
            node.value
                .get("columns")
                .and_then(V::as_array)
                .map_or(0, crate::view::Arr::len)
        } else {
            0
        },
        rows: if table {
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
    }
}

/// The fields a part takes from its owner and from the later parts of that owner.
fn finish(parts: &mut [Part], owner: u32, node: &Node<'_, '_>) {
    let keys: Vec<&str> = node
        .value
        .get("attrs")
        .and_then(|attrs| attrs.get("keyValues"))
        .and_then(V::as_array)
        .into_iter()
        .flat_map(crate::view::Arr::iter)
        .filter_map(|pair| {
            pair.as_array()
                .and_then(|pair| pair.get(0))
                .and_then(V::as_str)
        })
        .collect();
    let span = node.kind == "span";
    for at in 0..parts.len() {
        let (head, later) = parts.split_at_mut(at + 1);
        let Some(this) = head.last_mut() else {
            continue;
        };
        this.owner = owner;
        this.index = at;
        if this.kind == "key" {
            this.kept = keys.contains(&this.key.as_str());
            this.last_key = !later
                .iter()
                .any(|other| other.kind == "key" && same_name(&this.key, &other.key, span));
        }
    }
}

/// The attribute records of the nodes, in source order within each owner.
pub(super) fn build<'k, 'v>(source: &str, nodes: impl Iterator<Item = Node<'k, 'v>>) -> Table {
    let mut table = Table::default();
    for node in nodes {
        let first = table.parts.len();
        let mut opener = node.start;
        if node.inline {
            inline_parts(source, &node, &mut table.parts);
        } else if matches!(node.kind, "list_item" | "table_cell") {
            marker_parts(source, &node, &mut table.parts);
        } else {
            opener = line_parts(source, &node, &mut table.parts);
        }
        if table.parts.len() == first {
            continue;
        }
        let owner = u32::try_from(table.owners.len()).unwrap_or(u32::MAX);
        if let Some(parts) = table.parts.get_mut(first..) {
            finish(parts, owner, &node);
        }
        table.owners.push(owner_of(source, &node, opener));
    }
    table
}

/// C2-B-211, C2-B-212: the comma-separated entries of a value and their sum, rounded up to a whole
/// number (the rows compare it only with whole numbers); 0 when an entry is no decimal number.
pub(super) fn entries(value: &str) -> (usize, i64) {
    let count = value.matches(',').count() + 1;
    let digits = |text: &str| !text.is_empty() && text.bytes().all(|c| c.is_ascii_digit());
    let mut parsed = Vec::with_capacity(count);
    for entry in value.split(',') {
        let entry = entry.trim_matches([' ', '\t']);
        let (int, frac) = entry
            .split_once('.')
            .map_or((entry, None), |(i, f)| (i, Some(f)));
        if !digits(int) || frac.is_some_and(|frac| !digits(frac)) {
            return (count, 0);
        }
        parsed.push((int, frac.unwrap_or("")));
    }
    // The fractions are added digit by digit, so the carry into the whole part is exact at any length.
    let scale = parsed.iter().map(|(_, frac)| frac.len()).max().unwrap_or(0);
    let mut carry = 0u64;
    let mut fraction = false;
    for place in (0..scale).rev() {
        let column: u64 = parsed
            .iter()
            .map(|(_, frac)| u64::from(byte(frac.as_bytes(), place).saturating_sub(b'0')))
            .sum::<u64>()
            + carry;
        fraction |= column % 10 != 0;
        carry = column / 10;
    }
    let whole = parsed
        .iter()
        .fold(i64::try_from(carry).unwrap_or(i64::MAX), |sum, (int, _)| {
            sum.saturating_add(int.parse::<i64>().unwrap_or(i64::MAX))
        });
    (count, whole.saturating_add(i64::from(fraction)))
}

#[cfg(test)]
mod tests {
    use super::{cell_marks, entries};

    #[test]
    fn entry_totals_round_up_exactly() {
        assert_eq!(entries("50, 50"), (2, 100));
        assert_eq!(entries("33.5,33.5,33.5"), (3, 101));
        assert_eq!(entries("100.0000000000000000000000000000000001"), (1, 101));
        assert_eq!(entries("9223372036854775807.1"), (1, i64::MAX));
        assert_eq!(entries("50,x"), (2, 0));
        assert_eq!(entries("1."), (1, 0));
    }

    #[test]
    fn cell_marks_follow_the_cell_prefix() {
        assert_eq!(cell_marks(b"{<^ .c}", 0), 2);
        assert_eq!(cell_marks(b"{?v .c}", 0), 2);
        assert_eq!(cell_marks(b"{? .c}", 0), 0);
        assert_eq!(cell_marks(b"{^ .c}", 0), 0);
    }
}
