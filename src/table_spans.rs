//! PART 9 §13 T5, the span walk, as ONE implementation.
//!
//! It lived inside the HTML renderer, which was fine while HTML was the only
//! consumer of a resolved span. PART 12 §26 publishes the counts on the tree,
//! so the encoder needs the same answer - and a second copy of a normative
//! total function with an orphan case and a blocked case is the divergence
//! markup-carve/carve#2190 exists to stop, whether the copies sit in two
//! repositories or two modules.

use std::collections::{BTreeMap, BTreeSet};

use crate::ast::{Table, TableCell, TableCellSpan, TableRow};

pub(crate) fn consumed_rowspan_cols(row_idx: usize, rowspan_cols: &RowspanCols) -> BTreeSet<usize> {
    rowspan_cols
        .iter()
        .filter_map(|(&(origin_row, col), &span)| {
            (row_idx > origin_row && row_idx < origin_row + span).then_some(col)
        })
        .collect()
}

/// Per-cell colspan counts for a row, keyed by the origin cell index. Computed in
/// a single left-to-right pass (mirroring `compute_rowspans`) so a row is
/// O(cells) rather than O(cells^2): each `<` extends the current chain origin
/// instead of every cell re-scanning the rest of the row.
pub(crate) type ColspanCounts = BTreeMap<usize, usize>;

/// Resolve every colspan origin in `row` to its total colspan count in one pass.
/// A real cell (`None` span, not consumed by a rowspan from above) starts a new
/// chain; each following `<` (Colspan) extends it; a rowspan cell or an orphan
/// `<` (no preceding real cell) breaks the chain so the next `<` resolves to
/// nothing. Consumed columns are transparent, matching `colspan_target`.
pub(crate) fn compute_colspans(row: &TableRow, consumed_cols: &BTreeSet<usize>) -> ColspanCounts {
    let mut counts: ColspanCounts = BTreeMap::new();
    let mut current_target: Option<usize> = None;
    for (i, cell) in row.cells.iter().enumerate() {
        if consumed_cols.contains(&i) {
            continue;
        }
        match cell.span {
            Some(TableCellSpan::Colspan) => {
                if let Some(target) = current_target {
                    *counts.entry(target).or_insert(1) += 1;
                }
            }
            Some(TableCellSpan::Rowspan) => {
                current_target = None;
            }
            None => {
                current_target = Some(i);
            }
        }
    }
    counts
}

/// Maps the origin cell `(row, col)` of each rowspan to its span count. Resolved
/// by carrying the current chain origin down per column, so an all-`^` table is
/// O(cells) rather than O(rows^2) (each `^` previously walked up every prior row
/// and the result list was scanned linearly per marker).
/// Rowspan counts keyed by origin cell `(row, col)`.
pub(crate) type RowspanCols = BTreeMap<(usize, usize), usize>;
/// Positions `(row, cell-index)` of orphan `^` markers (nothing above to extend).
pub(crate) type OrphanCarets = BTreeSet<(usize, usize)>;

/// Returns (rowspan counts keyed by origin (row, col), positions of orphan `^`
/// markers). An orphan `^` has no cell above it to extend, so it renders as an
/// EMPTY cell rather than being dropped (spec PART 9 §5). Positions are keyed
/// by (row, cell-index), matching the render loop's cell enumeration.
pub(crate) fn compute_rowspans(t: &Table) -> (RowspanCols, OrphanCarets) {
    let mut spans: BTreeMap<(usize, usize), usize> = BTreeMap::new();
    let mut orphan_carets: BTreeSet<(usize, usize)> = BTreeSet::new();
    // Per column: the origin row of the current rowspan chain (the most recent
    // non-`^` cell above). A `^` extends that origin; a real cell starts a new
    // chain.
    let mut base_for_col: BTreeMap<usize, usize> = BTreeMap::new();
    for (row_idx, row) in t.rows.iter().enumerate() {
        for (col, cell) in row.cells.iter().enumerate() {
            if cell.span == Some(TableCellSpan::Rowspan) {
                if let Some(&base) = base_for_col.get(&col) {
                    let source = &t.rows[base].cells;
                    let covered_by_visible_span =
                        if source[col].span == Some(TableCellSpan::Colspan) {
                            let mut left = col;
                            while left > 0 && source[left].span == Some(TableCellSpan::Colspan) {
                                left -= 1;
                            }
                            source[left].span.is_none()
                                && base + spans.get(&(base, left)).copied().unwrap_or(1) > row_idx
                        } else {
                            true
                        };
                    if covered_by_visible_span {
                        *spans.entry((base, col)).or_insert(1) += 1;
                    } else {
                        orphan_carets.insert((row_idx, col));
                        base_for_col.insert(col, row_idx);
                    }
                } else {
                    orphan_carets.insert((row_idx, col));
                    base_for_col.insert(col, row_idx);
                }
            } else {
                base_for_col.insert(col, row_idx);
            }
        }
    }
    (spans, orphan_carets)
}

/// Resolve a `<` colspan marker by walking left to the nearest real cell that is
/// not already occupied by a rowspan from above. Contiguous `<` markers are
/// transparent, as are columns consumed by rowspans. If the scan reaches the
/// table edge, the marker is orphaned and renders as an empty cell (spec §5).
pub(crate) fn colspan_target(
    row: &TableRow,
    i: usize,
    consumed_cols: &BTreeSet<usize>,
) -> Option<usize> {
    let mut j = i;
    while j > 0 {
        j -= 1;
        if consumed_cols.contains(&j) {
            continue;
        }
        match row.cells[j].span {
            Some(TableCellSpan::Colspan) => continue,
            Some(TableCellSpan::Rowspan) => return None,
            None => return Some(j),
        }
    }
    None
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
    let (rowspan_cols, _orphans) = compute_rowspans(t);
    let mut colspans: BTreeMap<(usize, usize), usize> = BTreeMap::new();
    for (row_idx, row) in t.rows.iter().enumerate() {
        let consumed = consumed_rowspan_cols(row_idx, &rowspan_cols);
        for (col, count) in compute_colspans(row, &consumed) {
            colspans.insert((row_idx, col), count);
        }
    }
    for (row_idx, row) in t.rows.iter_mut().enumerate() {
        for (col, cell) in row.cells.iter_mut().enumerate() {
            if cell.rowspan.is_none() {
                if let Some(&n) = rowspan_cols.get(&(row_idx, col)) {
                    if n > 1 {
                        cell.rowspan = Some(n);
                    }
                }
            }
            if cell.colspan.is_none() {
                if let Some(&n) = colspans.get(&(row_idx, col)) {
                    if n > 1 {
                        cell.colspan = Some(n);
                    }
                }
            }
        }
    }
}

/// `TableCell` is referenced by the signatures above; the import keeps rustc
/// from warning when only the aliases are used.
#[allow(unused)]
type CellAlias = TableCell;
