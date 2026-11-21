//! C2-W-145: a span count that no mark cell covers, as an import or a bridge gives it (C2-A-093),
//! is written with the marks a parse reads back as that count. Marks the tree already holds stay.
//! The writer writes a copy of the tree with the marks added; a loss path is mapped back to the
//! supplied tree, and an added mark reports at its covering cell.

use crate::view::V;
use serde_json::{Value, json};

/// The copy of the tree with the marks, and how its rows map back to the supplied ones.
pub struct Spans {
    pub doc: Value,
    /// For each changed row, by its path: the path in the supplied tree of each cell of the copy.
    rows: Vec<(String, Vec<String>)>,
    /// The paths of the cells whose row span has no spelling.
    pub unspelled: Vec<String>,
}

/// A row span from above that holds a column: its cell path, the rows it still takes, and its width.
struct Pending {
    at: String,
    rows: i64,
    width: usize,
}

fn count(cell: &Value, field: &str) -> i64 {
    cell.get(field).and_then(Value::as_i64).unwrap_or(1)
}

fn span_of(cell: Option<&Value>) -> Option<&str> {
    cell.and_then(|c| c.get("span")).and_then(Value::as_str)
}

fn mark(span: &str) -> Value {
    json!({ "type": "table_cell", "header": false, "span": span, "children": [] })
}

fn has_counts(table: &Value) -> bool {
    groups(table).any(|(_, group)| {
        rows(group).any(|(_, row)| {
            cells(row)
                .iter()
                .any(|c| count(c, "colspan") > 1 || count(c, "rowspan") > 1)
        })
    })
}

fn list(node: &Value) -> &[Value] {
    node.get("children")
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

fn groups(table: &Value) -> impl Iterator<Item = (usize, &Value)> {
    list(table)
        .iter()
        .enumerate()
        .filter(|(_, g)| g.get("type").and_then(Value::as_str) != Some("comment"))
}

fn rows(group: &Value) -> impl Iterator<Item = (usize, &Value)> {
    list(group)
        .iter()
        .enumerate()
        .filter(|(_, r)| r.get("type").and_then(Value::as_str) == Some("table_row"))
}

fn cells(row: &Value) -> &[Value] {
    list(row)
}

#[derive(Default)]
struct GroupMarks {
    pending: Vec<Option<Pending>>,
}

struct RowOut {
    cells: Vec<Value>,
    from: Vec<String>,
    column: usize,
    added: bool,
}

impl GroupMarks {
    /// The cells of `row` with the marks of its counts, by the column counting of C2-B-172.
    fn row(
        &mut self,
        row: &Value,
        path: &str,
        unspelled: &mut Vec<String>,
    ) -> Option<(Vec<Value>, Vec<String>)> {
        let given = cells(row);
        let mut out = RowOut {
            cells: Vec::new(),
            from: Vec::new(),
            column: 0,
            added: false,
        };
        for (k, cell) in given.iter().enumerate() {
            self.take(&mut out, Some(cell), unspelled);
            let at = format!("{path}/children/{k}");
            out.cells.push(cell.clone());
            out.from.push(at.clone());
            if span_of(Some(cell)) == Some("rowspan")
                && let Some(Some(held)) = self.pending.get_mut(out.column)
            {
                held.rows -= 1;
            }
            let colspan = usize::try_from(count(cell, "colspan").max(1)).unwrap_or(1);
            // The marks the tree holds behind the cell count toward its colspan; the rest are added.
            let held = if cell.get("span").is_some() {
                colspan - 1
            } else {
                (1..colspan)
                    .take_while(|n| span_of(given.get(k + n)) == Some("colspan"))
                    .count()
            };
            let missing = colspan - 1 - held;
            for _ in 0..missing {
                out.cells.push(mark("colspan"));
                out.from.push(at.clone());
            }
            out.added |= missing > 0;
            if count(cell, "rowspan") > 1 {
                if self.pending.len() <= out.column {
                    self.pending.resize_with(out.column + 1, || None);
                }
                if let Some(slot) = self.pending.get_mut(out.column) {
                    *slot = Some(Pending {
                        at,
                        rows: count(cell, "rowspan") - 1,
                        width: colspan,
                    });
                }
            }
            out.column += 1 + missing;
        }
        self.take(&mut out, None, unspelled);
        // A row span that no cell of this row reaches has no place in it.
        for held in self.pending.iter_mut().skip(out.column).flatten() {
            if held.rows > 0 {
                held.rows -= 1;
                if !unspelled.contains(&held.at) {
                    unspelled.push(held.at.clone());
                }
            }
        }
        out.added.then_some((out.cells, out.from))
    }

    /// The marks of the row spans from above at the column, before the cell `next` of the row.
    /// A `^` that the tree holds there is the mark of its row, so none is added.
    fn take(&mut self, out: &mut RowOut, next: Option<&Value>, unspelled: &mut Vec<String>) {
        while let Some(Some(held)) = self.pending.get_mut(out.column) {
            if held.rows <= 0 || span_of(next) == Some("rowspan") {
                break;
            }
            // A `^` below a cell of more than one column joins nothing, so that row span stays unspelled.
            if held.width == 1 {
                out.cells.push(mark("rowspan"));
                out.from.push(held.at.clone());
                out.added = true;
            } else if !unspelled.contains(&held.at) {
                unspelled.push(held.at.clone());
            }
            held.rows -= 1;
            out.column += held.width;
        }
    }

    fn finish(&self, unspelled: &mut Vec<String>) {
        // A row span that reaches past the last row of its group has no row for its marks.
        for held in self.pending.iter().flatten() {
            if held.rows > 0 && !unspelled.contains(&held.at) {
                unspelled.push(held.at.clone());
            }
        }
    }
}

fn mark_tables(
    node: &mut Value,
    path: &str,
    rows_out: &mut Vec<(String, Vec<String>)>,
    unspelled: &mut Vec<String>,
) {
    if node.get("type").and_then(Value::as_str) == Some("table") && has_counts(node) {
        let mut changes = Vec::new();
        for (g, group) in groups(node) {
            let mut marks = GroupMarks::default();
            for (r, row) in rows(group) {
                let row_path = format!("{path}/children/{g}/children/{r}");
                if let Some((cells, from)) = marks.row(row, &row_path, unspelled) {
                    changes.push((g, r, cells, row_path, from));
                }
            }
            marks.finish(unspelled);
        }
        for (g, r, cells, row_path, from) in changes {
            if let Some(slot) = node.pointer_mut(&format!("/children/{g}/children/{r}/children")) {
                *slot = Value::Array(cells);
            }
            rows_out.push((row_path, from));
        }
    }
    match node {
        Value::Object(map) => {
            for (key, value) in map.iter_mut() {
                mark_tables(value, &format!("{path}/{key}"), rows_out, unspelled);
            }
        }
        Value::Array(items) => {
            for (k, value) in items.iter_mut().enumerate() {
                mark_tables(value, &format!("{path}/{k}"), rows_out, unspelled);
            }
        }
        _ => {}
    }
}

/// The tree with the marks of C2-W-145, or `None` where no count lacks them.
pub fn add(doc: V<'_>) -> Option<Spans> {
    if !has_any_counts(doc) {
        return None;
    }
    let mut copy = doc.to_value();
    let mut rows_out = Vec::new();
    let mut unspelled = Vec::new();
    mark_tables(&mut copy, "", &mut rows_out, &mut unspelled);
    rows_out.sort_by_key(|(row, _)| std::cmp::Reverse(row.len()));
    (!rows_out.is_empty() || !unspelled.is_empty()).then_some(Spans {
        doc: copy,
        rows: rows_out,
        unspelled,
    })
}

fn has_any_counts(node: V<'_>) -> bool {
    // A typed node's other views (positions, attrs, markers, scalars) hold no node.
    if let V::Node(n) = node {
        if let crate::ast::Node::TableCell(cell) = n
            && (cell.colspan.unwrap_or(1) > 1 || cell.rowspan.unwrap_or(1) > 1)
        {
            return true;
        }
        let mut found = false;
        node.each(&mut |_, value| {
            found = found
                || (matches!(value, V::Node(_) | V::Nodes(_) | V::Json(_)) && has_any_counts(value));
        });
        return found;
    }
    if let Some(items) = node.as_array() {
        return items.iter().any(has_any_counts);
    }
    if !node.is_object() {
        return false;
    }
    let counted = |field| node.get(field).and_then(V::as_i64).unwrap_or(1) > 1;
    if node.get("type").and_then(V::as_str) == Some("table_cell")
        && (counted("colspan") || counted("rowspan"))
    {
        return true;
    }
    let mut found = false;
    node.each(&mut |_, value| found = found || has_any_counts(value));
    found
}

impl Spans {
    /// The path in the supplied tree of a path of the copy: a cell behind an added mark moves back,
    /// and an added mark names its covering cell.
    pub fn remap(&self, path: &str) -> String {
        // A table in a cell: the inner row first, then the row around it.
        let mut out = path.to_string();
        for (row, from) in &self.rows {
            let Some(rest) = out
                .strip_prefix(row.as_str())
                .and_then(|r| r.strip_prefix("/children/"))
            else {
                continue;
            };
            let (index, tail) = rest.split_once('/').map_or((rest, ""), |(i, t)| (i, t));
            let Some(orig) = index.parse::<usize>().ok().and_then(|k| from.get(k)) else {
                continue;
            };
            out = if tail.is_empty() {
                orig.clone()
            } else {
                format!("{orig}/{tail}")
            };
        }
        out
    }
}
