//! Table layout: the derived span counts of C2-B-172 to C2-B-174 and the
//! resolved column alignment of C2-B-213 and C2-A-089.

use super::Res;
use super::order::addr;
use crate::ast::{Node, Table, TableCellAlign, TableCellSpan, TableColumnResolvedAlign};
use crate::hash::AddrMap;

/// One cell placed on the grid.
pub struct Placed<'r, 'a> {
    pub node: &'r Node<'a>,
    pub row: usize,
    pub col: usize,
    pub colspan: i64,
    pub rowspan: i64,
    /// A span mark cell, covered by another cell.
    pub mark: bool,
    pub head: bool,
    extended_in: usize,
}

/// The rows of a table in group order, with the group kind of each.
#[must_use]
pub fn rows<'r, 'a>(t: &'r Table<'a>) -> Vec<(&'r Node<'a>, GroupKind)> {
    let mut out = Vec::new();
    for g in t.children.iter() {
        let (kind, rows) = match g {
            Node::TableHead(x) => (GroupKind::Head, &*x.children),
            Node::TableBody(x) => (GroupKind::Body, &*x.children),
            Node::TableFoot(x) => (GroupKind::Foot, &*x.children),
            _ => continue,
        };
        out.extend(
            rows.iter()
                .filter(|r| matches!(r, Node::TableRow(_)))
                .map(|r| (r, kind)),
        );
    }
    out
}

/// The kind of a table group.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GroupKind {
    Head,
    Body,
    Foot,
}

/// Places every cell of `t` on the grid. Where the table holds span marks the
/// counts derive from the marks; otherwise the counts of the tree stand.
#[must_use]
pub fn layout<'r, 'a>(t: &'r Table<'a>) -> Vec<Placed<'r, 'a>> {
    let rows = rows(t);
    let marks = rows
        .iter()
        .any(|(r, _)| cells(r).any(|c| matches!(c, Node::TableCell(x) if x.span.is_some())));
    let mut grid: Vec<Vec<Option<usize>>> = vec![Vec::new(); rows.len()];
    let mut placed: Vec<Placed<'r, 'a>> = Vec::new();
    for (r, (row, kind)) in rows.iter().enumerate() {
        let mut c = 0usize;
        for cell in cells(row) {
            let Node::TableCell(x) = cell else { continue };
            while occupied(&grid, r, c) {
                c += 1;
            }
            let base = Placed {
                node: cell,
                row: r,
                col: c,
                colspan: 1,
                rowspan: 1,
                mark: false,
                head: *kind == GroupKind::Head,
                extended_in: r,
            };
            if let (true, Some(span)) = (marks, x.span) {
                let owner = match span {
                    TableCellSpan::Colspan => c.checked_sub(1).and_then(|p| at(&grid, r, p)),
                    TableCellSpan::Rowspan => r.checked_sub(1).and_then(|p| at(&grid, p, c)),
                };
                extend(&mut placed, owner, span, r);
                let idx = placed.len();
                placed.push(Placed { mark: true, ..base });
                fill(&mut grid, r, c, 1, 1, owner.unwrap_or(idx));
                c += 1;
            } else {
                let (cs, rs) = if marks {
                    (1, 1)
                } else {
                    (x.colspan.unwrap_or(1).max(1), x.rowspan.unwrap_or(1).max(1))
                };
                let idx = placed.len();
                placed.push(Placed {
                    colspan: cs,
                    rowspan: rs,
                    ..base
                });
                let (width, height) = (
                    usize::try_from(cs).unwrap_or(1),
                    usize::try_from(rs).unwrap_or(1),
                );
                fill(&mut grid, r, c, width, height, idx);
                c += width;
            }
        }
    }
    placed
}

fn cells<'r, 'a>(row: &'r Node<'a>) -> impl Iterator<Item = &'r Node<'a>> {
    row.children().into_iter().flatten()
}

fn occupied(grid: &[Vec<Option<usize>>], r: usize, c: usize) -> bool {
    at(grid, r, c).is_some()
}

fn at(grid: &[Vec<Option<usize>>], r: usize, c: usize) -> Option<usize> {
    grid.get(r).and_then(|row| row.get(c)).copied().flatten()
}

fn fill(grid: &mut [Vec<Option<usize>>], r: usize, c: usize, w: usize, h: usize, idx: usize) {
    for row in grid.iter_mut().skip(r).take(h) {
        if row.len() < c + w {
            row.resize(c + w, None);
        }
        for slot in row.iter_mut().skip(c).take(w) {
            *slot = Some(idx);
        }
    }
}

/// A valid mark adds one to the count of the cell it joins; several marks of
/// one row under a wide cell extend its row span once.
fn extend(placed: &mut [Placed<'_, '_>], owner: Option<usize>, span: TableCellSpan, r: usize) {
    let Some(o) = owner.and_then(|o| placed.get_mut(o)) else {
        return;
    };
    match span {
        TableCellSpan::Colspan => o.colspan += 1,
        TableCellSpan::Rowspan => {
            if o.extended_in < r {
                o.rowspan += 1;
                o.extended_in = r;
            }
        }
    }
}

/// The resolved alignment of each column: the mark of the last head cell that
/// starts in it, else the authored `align` (C2-A-089, C2-B-213).
#[must_use]
pub fn resolved_aligns(
    t: &Table<'_>,
    placed: &[Placed<'_, '_>],
) -> Vec<Option<TableColumnResolvedAlign>> {
    let mut out: Vec<Option<TableColumnResolvedAlign>> = vec![None; t.columns.len()];
    for p in placed.iter().filter(|p| p.head && !p.mark) {
        if let (Node::TableCell(x), Some(slot)) = (p.node, out.get_mut(p.col)) {
            if let Some(a) = x.align {
                *slot = Some(convert(a));
            }
        }
    }
    for (slot, col) in out.iter_mut().zip(t.columns.iter()) {
        if let (None, Node::TableColumn(c)) = (&slot, col) {
            *slot = c.align.map(convert_column);
        }
    }
    out
}

fn convert(a: TableCellAlign) -> TableColumnResolvedAlign {
    match a {
        TableCellAlign::Left => TableColumnResolvedAlign::Left,
        TableCellAlign::Right => TableColumnResolvedAlign::Right,
        TableCellAlign::Center => TableColumnResolvedAlign::Center,
    }
}

fn convert_column(a: crate::ast::TableColumnAlign) -> TableColumnResolvedAlign {
    match a {
        crate::ast::TableColumnAlign::Left => TableColumnResolvedAlign::Left,
        crate::ast::TableColumnAlign::Right => TableColumnResolvedAlign::Right,
        crate::ast::TableColumnAlign::Center => TableColumnResolvedAlign::Center,
    }
}

/// Writes the table results of `n` when it is a table.
pub(super) fn resolve_table(n: &Node, res: &mut AddrMap<usize, Res>) {
    let Node::Table(t) = n else { return };
    let placed = layout(t);
    let marks = placed.iter().any(|p| p.mark);
    if marks {
        for p in placed.iter().filter(|p| !p.mark) {
            let c = (p.colspan > 1).then_some(p.colspan);
            let r = (p.rowspan > 1).then_some(p.rowspan);
            res.insert(addr(p.node), Res::Spans(c, r));
        }
    }
    for (col, a) in t.columns.iter().zip(resolved_aligns(t, &placed)) {
        if let Some(a) = a {
            res.insert(addr(col), Res::Align(a));
        }
    }
}
