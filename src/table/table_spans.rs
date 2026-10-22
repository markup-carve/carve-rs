//! PART 9 §13 T5, the span walk, as ONE implementation.
//!
//! It lived inside the HTML renderer, which was fine while HTML was the only
//! consumer of a resolved span. PART 12 §26 publishes the counts on the tree,
//! so the encoder needs the same answer - and a second copy of a normative
//! total function with an orphan case and a blocked case is the divergence
//! markup-carve/carve#2190 exists to stop, whether the copies sit in two
//! repositories or two modules.

use crate::ast::{Table, TableCellSpan};

/// One authored cell's place in the resolved grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SpanEntry {
    pub(crate) rowspan: usize,
    pub(crate) colspan: usize,
    /// A marker absorbed into the cell it extended: it renders nothing.
    pub(crate) skip: bool,
}

/// T5 over any grid of markers, so the pipe table and the list-table extension
/// resolve them with one rule.
///
/// Markers resolve in row-major order and every position belongs to the cell
/// whose rectangle covers it, so a span never covers another cell
/// (markup-carve/carve#3047). A `<` widens the cell to its left only when that
/// cell starts in this row; a `^` lengthens the cell above only when its left
/// edge is this column and the rest of the new row strip holds unresolved
/// markers or no cell. A marker that finds no origin, or is blocked, stays a
/// cell of the grid: it renders empty.
pub(crate) fn resolve_span_grid(
    widths: &[usize],
    marker: impl Fn(usize, usize) -> Option<TableCellSpan>,
) -> Vec<Vec<SpanEntry>> {
    resolve_span_grid_absorbing(widths, marker, |_, _| {})
}

/// [`resolve_span_grid`], reporting each marker as the walk absorbs it into
/// an origin, as `(origin, marker)` grid coordinates in row-major order.
pub(crate) fn resolve_span_grid_absorbing(
    widths: &[usize],
    marker: impl Fn(usize, usize) -> Option<TableCellSpan>,
    mut on_absorb: impl FnMut((usize, usize), (usize, usize)),
) -> Vec<Vec<SpanEntry>> {
    let mut grid: Vec<Vec<SpanEntry>> = widths
        .iter()
        .map(|&width| {
            vec![
                SpanEntry {
                    rowspan: 1,
                    colspan: 1,
                    skip: false,
                };
                width
            ]
        })
        .collect();
    // The origin of the cell covering each position of the previous row.
    let mut above: Vec<Option<(usize, usize)>> = Vec::new();
    for (r, &width) in widths.iter().enumerate() {
        let mut own: Vec<Option<(usize, usize)>> = vec![None; width];
        for c in 0..width {
            if grid[r][c].skip {
                continue;
            }
            match marker(r, c) {
                Some(TableCellSpan::Colspan) if c > 0 => {
                    if let Some(at) = own[c - 1].filter(|at| at.0 == r) {
                        grid[r][at.1].colspan += 1;
                        grid[r][c].skip = true;
                        own[c] = Some(at);
                        on_absorb(at, (r, c));
                        continue;
                    }
                }
                Some(TableCellSpan::Rowspan) => {
                    if let Some(at) = above.get(c).copied().flatten().filter(|at| at.1 == c) {
                        let colspan = grid[at.0][at.1].colspan;
                        let strip_free = (c + 1..(c + colspan).min(width))
                            .all(|k| !grid[r][k].skip && marker(r, k).is_some());
                        if strip_free {
                            grid[at.0][at.1].rowspan += 1;
                            for k in c..(c + colspan).min(width) {
                                grid[r][k].skip = true;
                                own[k] = Some(at);
                                on_absorb(at, (r, k));
                            }
                            continue;
                        }
                    }
                }
                _ => {}
            }
            own[c] = Some((r, c));
        }
        above = own;
    }
    grid
}

/// The resolved grid of a pipe table, one entry per authored cell.
pub(crate) struct TableSpanPlan {
    grid: Vec<Vec<SpanEntry>>,
}

impl TableSpanPlan {
    pub(crate) fn new(table: &Table) -> Self {
        let widths: Vec<usize> = table.rows.iter().map(|row| row.cells.len()).collect();
        Self {
            grid: resolve_span_grid(&widths, |r, c| table.rows[r].cells[c].span),
        }
    }

    pub(crate) fn entry(&self, row: usize, col: usize) -> SpanEntry {
        self.grid[row][col]
    }

    pub(crate) fn row(&self, row: usize) -> &[SpanEntry] {
        &self.grid[row]
    }
}

/// Fill in each origin cell's resolved `colspan` and `rowspan` (PART 12 §26).
///
/// A count of 1 is not recorded: §26 makes absent mean 1, on the reasoning §22
/// gives for a list's `start`. A count already on the cell WINS and is not
/// recomputed - a tree that reached here through an ingest may carry counts an
/// importer resolved from markers this engine never saw, and §23 settles that
/// shape of question the same way for `blockImage`.
///
/// A marker that found no origin extends nobody, so no count moves. T5 is
/// total: it renders as an empty cell rather than being dropped.
pub(crate) fn resolve_table_spans(t: &mut Table) {
    let plan = TableSpanPlan::new(t);
    for (row_idx, row) in t.rows.iter_mut().enumerate() {
        for (col, cell) in row.cells.iter_mut().enumerate() {
            let entry = plan.entry(row_idx, col);
            if cell.rowspan.is_none() && entry.rowspan > 1 {
                cell.rowspan = Some(entry.rowspan);
            }
            if cell.colspan.is_none() && entry.colspan > 1 {
                cell.colspan = Some(entry.colspan);
            }
        }
    }
}
