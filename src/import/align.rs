//! C2-B-213 read backwards: a render writes on every cell the alignment it
//! resolves from the cell, the head and the column.

use serde_json::Value;
use std::collections::BTreeMap;

pub(crate) struct Placed {
    at: (usize, usize, usize),
    column: usize,
    colspan: usize,
    head: bool,
}

/// The cells of a table's groups with the column each starts in, as a render places them (C2-H-056).
pub(crate) fn placed(groups: &[Value]) -> Vec<Placed> {
    let mut out = Vec::new();
    for (g, group) in groups.iter().enumerate() {
        if group["type"] == "comment" {
            continue;
        }
        let head = group["type"] == "table_head";
        // Per column, the first row of the group that a row span from above no longer covers.
        let mut free_at: Vec<u64> = Vec::new();
        let mut index = 0u64;
        for (r, row) in group["children"]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
        {
            if row["type"] != "table_row" {
                continue;
            }
            let mut column = 0usize;
            for (c, cell) in row["children"].as_array().into_iter().flatten().enumerate() {
                while free_at.get(column).is_some_and(|f| *f > index) {
                    column += 1;
                }
                let colspan = usize::try_from(cell["colspan"].as_u64().unwrap_or(1)).unwrap_or(1);
                out.push(Placed {
                    at: (g, r, c),
                    column,
                    colspan,
                    head,
                });
                let end = index.saturating_add(cell["rowspan"].as_u64().unwrap_or(1));
                if free_at.len() < column + colspan {
                    free_at.resize(column + colspan, 0);
                }
                for slot in free_at.iter_mut().skip(column).take(colspan) {
                    *slot = (*slot).max(end);
                }
                column += colspan;
            }
            index += 1;
        }
    }
    out
}

/// C2-M-039: the columns a parse counts (C2-B-172), with the slots that row
/// spans from above take in each row.
pub(crate) fn grid_width(cells: &[Placed]) -> usize {
    cells
        .iter()
        .map(|p| p.column + p.colspan)
        .max()
        .unwrap_or(0)
}

fn cell_mut(
    table: &mut Value,
    (g, r, c): (usize, usize, usize),
) -> Option<&mut serde_json::Map<String, Value>> {
    table
        .get_mut("children")?
        .get_mut(g)?
        .get_mut("children")?
        .get_mut(r)?
        .get_mut("children")?
        .get_mut(c)?
        .as_object_mut()
}

fn mark(table: &Value, (g, r, c): (usize, usize, usize), axis: &str) -> Option<String> {
    table
        .get("children")?
        .get(g)?
        .get("children")?
        .get(r)?
        .get("children")?
        .get(c)?
        .get(axis)?
        .as_str()
        .map(str::to_string)
}

/// A head cell keeps its own mark; a cell outside the head that shows the
/// mark of its column's head cell takes it from there; without a head mark,
/// a column whose cells all show one alignment has it as its column entry,
/// while every column before it has one on that axis.
pub(crate) fn resolve(table: &mut Value, cells: &[Placed]) {
    resolve_axis(table, cells, "align");
    resolve_axis(table, cells, "valign");
}

fn resolve_axis(table: &mut Value, cells: &[Placed], axis: &str) {
    let mut marks: BTreeMap<usize, String> = BTreeMap::new();
    for p in cells.iter().filter(|p| p.head) {
        if let Some(m) = mark(table, p.at, axis) {
            marks.insert(p.column, m);
        }
    }
    let mut by_column: Vec<(usize, Vec<&Placed>)> = Vec::new();
    for p in cells.iter().filter(|p| !p.head) {
        match marks.get(&p.column) {
            Some(m) => {
                if mark(table, p.at, axis).as_ref() == Some(m)
                    && let Some(cell) = cell_mut(table, p.at)
                {
                    cell.remove(axis);
                }
            }
            None => match by_column.iter_mut().find(|(c, _)| *c == p.column) {
                Some((_, list)) => list.push(p),
                None => by_column.push((p.column, vec![p])),
            },
        }
    }
    if !cells.iter().any(|p| p.head) {
        give_columns(table, by_column, axis);
    }
}

/// A column gives its alignment only after columns that all give one:
/// an `aligns` or `valigns` list has no empty entry (C2-B-211).
fn give_columns(table: &mut Value, mut by_column: Vec<(usize, Vec<&Placed>)>, axis: &str) {
    by_column.sort_by_key(|(c, _)| *c);
    for (expected, (index, entries)) in by_column.into_iter().enumerate() {
        if index != expected {
            return;
        }
        let first = entries.first().and_then(|p| mark(table, p.at, axis));
        let Some(value) = first.filter(|f| {
            entries
                .iter()
                .all(|p| mark(table, p.at, axis).as_ref() == Some(f))
        }) else {
            return;
        };
        // `baseline` has no `valigns` entry (C2-B-211).
        if value == "baseline" {
            return;
        }
        let Some(column) = table
            .get_mut("columns")
            .and_then(|c| c.get_mut(index))
            .and_then(Value::as_object_mut)
        else {
            return;
        };
        column.insert(axis.to_string(), value.into());
        for p in &entries {
            if let Some(cell) = cell_mut(table, p.at) {
                cell.remove(axis);
            }
        }
    }
}
