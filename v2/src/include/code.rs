//! A code include (C2-N-042 to C2-N-047): the child is the block an authored
//! fence with the include's attribute line, its language and the selected
//! payload gives, positioned in the included file.

use super::expand::{Failure, is_table, option, parse_file};
use super::lines::{FileLines, LineRange, bounds, is_marker_line};
use crate::Arena;
use crate::ast::{self, Node, Pos};
use crate::tree::Scope;
use serde_json::Value;

/// C2-N-046: for each line, whether it starts inside a quoted field of the
/// table's records, where a marker line is data.
pub(super) fn quoted_lines(text: &str, lines: &FileLines, node: &ast::Include<'_>) -> Vec<bool> {
    let sep = if option(node, "code").and_then(Value::as_str) == Some("tsv") {
        '\t'
    } else {
        ','
    };
    let mut inside = false;
    (0..lines.len())
        .map(|i| {
            let start = inside;
            inside = scan_quotes(lines.line(text, i), sep, inside);
            start
        })
        .collect()
}

/// The quote state at the end of a line, as the record reader of C2-B-181 reads fields.
fn scan_quotes(line: &str, sep: char, mut inside: bool) -> bool {
    let mut at_start = !inside;
    let mut it = line.chars().peekable();
    while let Some(c) = it.next() {
        if inside {
            if c == '"' {
                if it.peek() == Some(&'"') {
                    it.next();
                } else {
                    inside = false;
                }
            }
        } else if at_start && c == '"' {
            inside = true;
            at_start = false;
        } else {
            at_start = c == sep;
        }
    }
    inside
}

/// The attribute line of the include as a brace block (C2-N-042).
fn brace(node: &ast::Include<'_>) -> String {
    let mut parts = Vec::new();
    if let Some(a) = node.attrs {
        if let Some(id) = a.id {
            parts.push(format!("#{id}"));
        }
        parts.extend(a.classes.iter().map(|c| format!(".{c}")));
        parts.extend(a.key_values.iter().map(|(k, v)| pair(k, v)));
    }
    if let Some(lang) = node.lang {
        parts.push(pair("lang", lang));
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!("{{{}}}\n", parts.join(" "))
    }
}

fn pair(k: &str, v: &str) -> String {
    if v.is_empty() {
        return k.to_string();
    }
    format!("{k}=\"{}\"", v.replace('\\', "\\\\").replace('"', "\\\""))
}

fn longest_backticks(s: &str) -> usize {
    let mut best = 0;
    let mut run = 0;
    for b in s.bytes() {
        run = if b == b'`' { run + 1 } else { 0 };
        best = best.max(run);
    }
    best
}

/// The block a fence with the include's attribute line, language and payload
/// gives, and whether its quoting is malformed (C2-B-181).
fn fenced<'a>(
    node: &ast::Include<'_>,
    payload: &str,
    arena: &'a Arena,
) -> Option<(Node<'a>, bool)> {
    let fence = "`".repeat(3.max(longest_backticks(payload) + 1));
    let lang = option(node, "code").and_then(Value::as_str).unwrap_or("");
    let source = format!("{}{fence}{lang}\n{payload}{fence}\n", brace(node));
    let (doc, scope) = parse_file(&source, &Scope::default(), arena, None, true);
    let first = doc.children.first_mut()?;
    if !matches!(first, Node::CodeBlock(_) | Node::Table(_)) {
        return None;
    }
    let node = std::mem::replace(
        first,
        Node::SoftBreak(arena.alloc(ast::SoftBreak::default())),
    );
    Some((node, !scope.findings.is_empty()))
}

fn strip_positions(n: &mut Node<'_>) {
    if let Some(p) = n.pos_slot() {
        *p = None;
    }
    n.for_each_child_mut(&mut strip_positions);
}

/// C2-N-044: column 1 of the first selected line to the last character of the last one.
fn payload_pos(text: &str, lines: &FileLines, range: LineRange) -> Pos {
    let bom = if text.starts_with('\u{FEFF}') { 3 } else { 0 };
    if range.from >= range.to {
        let at = lines.starts.get(range.from).copied().unwrap_or(bom);
        let line = Pos::at(range.from + 1);
        return Pos {
            start_line: line,
            end_line: line,
            start_column: 1,
            end_column: 1,
            start_offset: Pos::at(at),
            end_offset: Pos::at(at),
        };
    }
    let start = lines.starts.get(range.from).copied().unwrap_or(0);
    let last = lines.line(text, range.to - 1);
    let end = lines.ends.get(range.to - 1).copied().unwrap_or(0);
    Pos {
        start_line: Pos::at(range.from + 1),
        end_line: Pos::at(range.to),
        start_column: 1,
        end_column: Pos::at(1 + last.chars().count()),
        start_offset: Pos::at(start),
        end_offset: Pos::at(end),
    }
}

/// C2-N-042 to C2-N-047: the one child of a code include.
pub(super) fn child<'a>(
    node: &ast::Include<'a>,
    text: &str,
    lines: &FileLines,
    range: LineRange,
    arena: &'a Arena,
) -> Result<Option<(Node<'a>, bool)>, Failure> {
    let table = is_table(node);
    let quoted = if table {
        quoted_lines(text, lines, node)
    } else {
        Vec::new()
    };
    let skip = |i: usize| {
        table && !quoted.get(i).copied().unwrap_or(false) && is_marker_line(lines.line(text, i))
    };
    // A table reads every record of the file: its head and foot come from the whole file (C2-N-046).
    let read = if table {
        LineRange {
            from: 0,
            to: lines.len(),
        }
    } else {
        range
    };
    let mut payload = String::new();
    for i in (read.from..read.to).filter(|&i| !skip(i)) {
        payload.push_str(lines.line(text, i));
        payload.push('\n');
    }
    let Some((mut child, malformed)) = fenced(node, &payload, arena) else {
        return Ok(None);
    };
    strip_positions(&mut child);
    if let Node::Table(t) = &mut child {
        let starts: Vec<usize> = (0..lines.len())
            .filter(|&i| !skip(i) && !quoted.get(i).copied().unwrap_or(false))
            .collect();
        select_records(node, t, &starts, range, arena)?;
    }
    if let Some(p) = child.pos_slot() {
        *p = Some(payload_pos(text, lines, range));
    }
    if let Some(caption) = node.caption.as_deref() {
        let mut copy = caption.owned().in_arena(arena);
        strip_positions(&mut copy);
        let copy = Some(arena.alloc(copy));
        match &mut child {
            Node::Table(t) => t.caption = copy,
            Node::CodeBlock(c) => c.caption = copy,
            _ => {}
        }
    }
    Ok(Some((child, malformed)))
}

fn take<'a>(n: &mut Node<'a>, arena: &'a Arena) -> Node<'a> {
    std::mem::replace(n, Node::SoftBreak(arena.alloc(ast::SoftBreak::default())))
}

/// The rows of a table by group: head, body and foot.
fn groups<'a>(t: &mut ast::Table<'a>, arena: &'a Arena) -> [Vec<Node<'a>>; 3] {
    let mut out = [Vec::new(), Vec::new(), Vec::new()];
    for g in t.children.iter_mut() {
        let (slot, rows) = match g {
            Node::TableHead(h) => (0, &mut *h.children),
            Node::TableBody(b) => (1, &mut *b.children),
            Node::TableFoot(f) => (2, &mut *f.children),
            _ => continue,
        };
        if let Some(v) = out.get_mut(slot) {
            v.extend(rows.iter_mut().map(|r| take(r, arena)));
        }
    }
    out
}

/// C2-N-046: a selection chooses body records only; the head and the foot of
/// the whole file are written under any selection.
fn select_records<'a>(
    node: &ast::Include<'a>,
    t: &mut ast::Table<'a>,
    starts: &[usize],
    range: LineRange,
    arena: &'a Arena,
) -> Result<(), Failure> {
    let [head, body, foot] = groups(t, arena);
    let (h, n) = (head.len(), head.len() + body.len() + foot.len());
    let rows = option(node, "rows").and_then(Value::as_str);
    let chosen: Box<dyn Fn(usize) -> bool> = if let Some((first, last)) = rows.and_then(bounds) {
        if first > n {
            return Err(Failure::new("rows_past_end"));
        }
        Box::new(move |i| i + 1 >= first && i < last)
    } else {
        let starts = starts.to_vec();
        Box::new(move |i| {
            starts
                .get(i)
                .is_some_and(|&l| l >= range.from && l < range.to)
        })
    };
    let mut body: Vec<Node<'a>> = body
        .into_iter()
        .enumerate()
        .filter(|(i, _)| chosen(h + i))
        .map(|(_, r)| r)
        .collect();
    let mut out = Vec::new();
    let mut push = |rows: Vec<Node<'a>>, kind: usize| {
        let mut rows = rows;
        if rows.is_empty() {
            return;
        }
        let children = arena.nodes(&mut rows);
        out.push(match kind {
            0 => Node::TableHead(arena.alloc(ast::TableHead {
                children,
                ..Default::default()
            })),
            1 => Node::TableBody(arena.alloc(ast::TableBody {
                children,
                ..Default::default()
            })),
            _ => Node::TableFoot(arena.alloc(ast::TableFoot {
                children,
                ..Default::default()
            })),
        });
    };
    push(head, 0);
    push(std::mem::take(&mut body), 1);
    push(foot, 2);
    t.children = arena.nodes(&mut out);
    match option(node, "cols") {
        Some(cols) => select_columns(node, t, cols, arena),
        None => Ok(()),
    }
}

fn cell_name(cell: &Node<'_>) -> String {
    let Node::TableCell(c) = cell else {
        return String::new();
    };
    match c.children.first() {
        Some(Node::CellParagraph(p)) => crate::resolve::plain::text_of(p.children),
        _ => String::new(),
    }
}

fn cols_invalid(reason: String) -> Failure {
    Failure::with("cols_invalid", "reason", reason)
}

/// C2-N-047: the column of each name in the first head record.
fn named(t: &ast::Table<'_>, names: &[Value]) -> Result<Vec<usize>, Failure> {
    let head = t.children.iter().find_map(|g| match g {
        Node::TableHead(h) => h.children.first(),
        _ => None,
    });
    let Some(Node::TableRow(row)) = head else {
        return Err(cols_invalid(
            "names columns of a table without a head row".into(),
        ));
    };
    let heads: Vec<String> = row.children.iter().map(cell_name).collect();
    let names: Vec<&str> = names.iter().filter_map(Value::as_str).collect();
    let mut out = Vec::new();
    for name in &names {
        let mut at = heads
            .iter()
            .enumerate()
            .filter(|(_, h)| h == name)
            .map(|(i, _)| i);
        let (Some(i), false) = (at.next(), name.is_empty()) else {
            return Err(cols_invalid(format!(
                "names the column {name}, which the head lacks"
            )));
        };
        if at.next().is_some() {
            return Err(cols_invalid(format!(
                "names the column {name}, which the head holds twice"
            )));
        }
        out.push(i);
    }
    if let Some(twice) = names
        .iter()
        .enumerate()
        .find(|(i, n)| names.iter().position(|m| m == *n) != Some(*i))
    {
        return Err(Failure::with("cols_duplicate", "name", *twice.1));
    }
    Ok(out)
}

/// C2-N-047: a range of columns by number, valid only with `header-rows=0`.
fn numbered(
    node: &ast::Include<'_>,
    t: &ast::Table<'_>,
    value: &str,
) -> Result<Vec<usize>, Failure> {
    let no_head = node.attrs.is_some_and(|a| {
        a.key_values
            .iter()
            .any(|(k, v)| *k == "header-rows" && *v == "0")
    });
    if !no_head {
        return Err(cols_invalid("takes a range only with header-rows=0".into()));
    }
    let (first, last) = bounds(value).unwrap_or((1, 0));
    if last > t.columns.len() {
        return Err(cols_invalid(format!(
            "reaches past the last column, {}",
            t.columns.len()
        )));
    }
    Ok((first.saturating_sub(1)..last).collect())
}

fn project<'a>(cells: &mut [Node<'a>], columns: &[usize], arena: &'a Arena) -> &'a mut [Node<'a>] {
    let mut out: Vec<Node<'a>> = columns
        .iter()
        .filter_map(|&i| cells.get_mut(i).map(|c| take(c, arena)))
        .collect();
    arena.nodes(&mut out)
}

fn select_columns<'a>(
    node: &ast::Include<'a>,
    t: &mut ast::Table<'a>,
    cols: &Value,
    arena: &'a Arena,
) -> Result<(), Failure> {
    let columns = match cols {
        Value::Array(names) => named(t, names)?,
        other => numbered(node, t, other.as_str().unwrap_or(""))?,
    };
    for g in t.children.iter_mut() {
        let Some(rows) = g.children_mut() else {
            continue;
        };
        for r in rows.iter_mut() {
            if let Node::TableRow(row) = r {
                row.children = project(row.children, &columns, arena);
            }
        }
    }
    t.columns = project(t.columns, &columns, arena);
    Ok(())
}
