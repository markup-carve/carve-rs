//! PART 9 §13 T5, the span walk, as ONE implementation.
//!
//! It lived inside the HTML renderer, which was fine while HTML was the only
//! consumer of a resolved span. PART 12 §26 publishes the counts on the tree,
//! so the encoder needs the same answer - and a second copy of a normative
//! total function with an orphan case and a blocked case is the divergence
//! markup-carve/carve#2190 exists to stop, whether the copies sit in two
//! repositories or two modules.

#[cfg(test)]
use std::collections::{BTreeMap, BTreeSet};

use crate::ast::{Table, TableCellSpan, TableRow};

pub(crate) struct CellGrid<T> {
    rows: Vec<Vec<T>>,
}

impl<T: Copy + Default + PartialEq> CellGrid<T> {
    fn new() -> Self {
        Self { rows: Vec::new() }
    }

    pub(crate) fn get(&self, &(row, col): &(usize, usize)) -> Option<&T> {
        self.rows
            .get(row)?
            .get(col)
            .filter(|value| **value != T::default())
    }

    fn set(&mut self, row: usize, col: usize, width: usize, value: T) {
        if self.rows.len() <= row {
            self.rows.resize_with(row + 1, Vec::new);
        }
        if self.rows[row].is_empty() {
            self.rows[row].resize(width, T::default());
        }
        self.rows[row][col] = value;
    }

    pub(crate) fn row(&self, row: usize) -> &[T] {
        self.rows.get(row).map_or(&[], Vec::as_slice)
    }

    pub(crate) fn contains(&self, key: &(usize, usize)) -> bool {
        self.get(key).is_some()
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = ((usize, usize), &T)> {
        self.rows.iter().enumerate().flat_map(|(row, cells)| {
            cells.iter().enumerate().filter_map(move |(col, value)| {
                (*value != T::default()).then_some(((row, col), value))
            })
        })
    }
}

fn compute_colspans(row: &TableRow, consumed_cols: &[bool]) -> ColspanPlan {
    let mut counts = Vec::new();
    let mut current_target: Option<usize> = None;
    let mut targets = Vec::new();
    for (i, cell) in row.cells.iter().enumerate() {
        if cell.span == Some(TableCellSpan::Colspan) {
            if targets.is_empty() {
                targets.resize(row.cells.len(), false);
            }
            targets[i] = current_target.is_some();
        }
        if consumed_cols.get(i) == Some(&true) {
            continue;
        }
        match cell.span {
            Some(TableCellSpan::Colspan) => {
                if let Some(target) = current_target {
                    if counts.is_empty() {
                        counts.resize(row.cells.len(), 1);
                    }
                    counts[target] += 1;
                }
            }
            Some(TableCellSpan::Rowspan) => current_target = None,
            None => current_target = Some(i),
        }
    }
    ColspanPlan { counts, targets }
}

/// Maps the origin cell `(row, col)` of each rowspan to its span count. Resolved
/// by carrying the current chain origin down per column. Each `^` extends
/// that origin without walking back through prior rows or merged columns.
/// Rowspan counts keyed by origin cell `(row, col)`.
pub(crate) type RowspanCols = CellGrid<usize>;
/// Positions `(row, cell-index)` of orphan `^` markers (nothing above to extend).
pub(crate) type OrphanCarets = CellGrid<bool>;

/// Returns (rowspan counts keyed by origin (row, col), positions of orphan `^`
/// markers). An orphan `^` has no cell above it to extend, so it renders as an
/// EMPTY cell rather than being dropped (spec PART 9 §5). Positions are keyed
/// by (row, cell-index), matching the render loop's cell enumeration.
pub(crate) fn compute_rowspans(t: &Table) -> (RowspanCols, OrphanCarets) {
    let mut spans = CellGrid::new();
    let mut orphan_carets = CellGrid::new();
    // Per column: the origin row of the current rowspan chain (the most recent
    // non-`^` cell above). A `^` extends that origin; a real cell starts a new
    // chain.
    let mut base_for_col: Vec<Option<(usize, Option<usize>)>> = Vec::new();
    for (row_idx, row) in t.rows.iter().enumerate() {
        if base_for_col.len() < row.cells.len() {
            base_for_col.resize(row.cells.len(), None);
        }
        let mut left = None;
        for (col, cell) in row.cells.iter().enumerate() {
            if cell.span == Some(TableCellSpan::Rowspan) {
                if let Some((base, source_left)) = base_for_col[col] {
                    let source = &t.rows[base].cells;
                    let covered_by_visible_span = if source[col].span
                        == Some(TableCellSpan::Colspan)
                    {
                        source_left.is_some_and(|left| {
                            source[left].span.is_none()
                                && base + spans.get(&(base, left)).copied().unwrap_or(1) > row_idx
                        })
                    } else {
                        true
                    };
                    if covered_by_visible_span {
                        let count = spans.get(&(base, col)).copied().unwrap_or(1) + 1;
                        spans.set(base, col, t.rows[base].cells.len(), count);
                    } else {
                        orphan_carets.set(row_idx, col, row.cells.len(), true);
                        base_for_col[col] = Some((row_idx, left));
                    }
                } else {
                    orphan_carets.set(row_idx, col, row.cells.len(), true);
                    base_for_col[col] = Some((row_idx, left));
                }
            } else {
                base_for_col[col] = Some((row_idx, left));
            }
            if cell.span != Some(TableCellSpan::Colspan) {
                left = Some(col);
            }
        }
    }
    (spans, orphan_carets)
}

pub(crate) struct ColspanPlan {
    pub(crate) counts: Vec<usize>,
    pub(crate) targets: Vec<bool>,
}

pub(crate) struct TableSpanPlan {
    pub(crate) rowspans: RowspanCols,
    pub(crate) orphan_carets: OrphanCarets,
    pub(crate) rows: Vec<ColspanPlan>,
}

impl TableSpanPlan {
    pub(crate) fn new(table: &Table) -> Self {
        let (rowspans, orphan_carets) = compute_rowspans(table);
        // Each covered column is visited once, rather than scanning every
        // span again for each row. Coverage is bounded by the authored carets.
        let mut consumed = vec![Vec::new(); table.rows.len()];
        for ((row, col), &span) in rowspans.iter() {
            for (covered, source) in consumed
                .iter_mut()
                .zip(&table.rows)
                .skip(row + 1)
                .take(span - 1)
            {
                if col < source.cells.len() {
                    if covered.is_empty() {
                        covered.resize(source.cells.len(), false);
                    }
                    covered[col] = true;
                }
            }
        }
        let rows = table
            .rows
            .iter()
            .zip(&consumed)
            .map(|(row, covered)| compute_colspans(row, covered))
            .collect();
        Self {
            rowspans,
            orphan_carets,
            rows,
        }
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
    let rowspan_cols = &plan.rowspans;
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
                if let Some(&n) = plan.rows[row_idx].counts.get(col) {
                    if n > 1 {
                        cell.colspan = Some(n);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::TableCell;

    fn table(markers: &[Vec<Option<TableCellSpan>>]) -> Table {
        Table {
            attrs: None,
            caption: None,
            short_caption: None,
            columns: vec![],
            row_groups: None,
            pos: None,
            rows: markers
                .iter()
                .map(|markers| TableRow {
                    attrs: None,
                    pos: None,
                    cells: markers
                        .iter()
                        .map(|&span| TableCell {
                            header: false,
                            span,
                            colspan: None,
                            rowspan: None,
                            align: None,
                            valign: None,
                            attrs: None,
                            children: vec![],
                            blocks: None,
                            pos: None,
                        })
                        .collect(),
                })
                .collect(),
        }
    }

    #[test]
    fn every_three_by_three_marker_grid_matches_the_reference_scans() {
        for mut code in 0..3_usize.pow(9) {
            let markers: Vec<_> = (0..3)
                .map(|_| {
                    (0..3)
                        .map(|_| {
                            let marker = [
                                None,
                                Some(TableCellSpan::Colspan),
                                Some(TableCellSpan::Rowspan),
                            ][code % 3];
                            code /= 3;
                            marker
                        })
                        .collect()
                })
                .collect();
            let t = table(&markers);
            let plan = TableSpanPlan::new(&t);
            let (spans, orphans) = reference_rowspans(&t);
            assert_eq!(
                plan.rowspans
                    .iter()
                    .map(|(key, &value)| (key, value))
                    .collect::<BTreeMap<_, _>>(),
                spans
            );
            assert_eq!(
                plan.orphan_carets
                    .iter()
                    .map(|(key, _)| key)
                    .collect::<BTreeSet<_>>(),
                orphans
            );
            for (r, row) in t.rows.iter().enumerate() {
                let consumed = spans
                    .iter()
                    .filter_map(|(&(origin, col), &span)| {
                        (r > origin && r < origin + span).then_some(col)
                    })
                    .collect();
                for c in 0..row.cells.len() {
                    if row.cells[c].span == Some(TableCellSpan::Colspan) {
                        assert_eq!(
                            plan.rows[r].targets[c],
                            reference_colspan_target(row, c, &consumed).is_some()
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn ragged_rows_keep_the_reference_origins_and_targets() {
        for widths in [[1, 4, 2], [4, 0, 2], [0, 2, 4]] {
            for seed in 0..256 {
                let markers: Vec<_> = widths
                    .iter()
                    .enumerate()
                    .map(|(r, &width)| {
                        (0..width)
                            .map(|c| {
                                [
                                    None,
                                    Some(TableCellSpan::Colspan),
                                    Some(TableCellSpan::Rowspan),
                                ][(seed + r * 7 + c * 11) % 3]
                            })
                            .collect()
                    })
                    .collect();
                let t = table(&markers);
                let plan = TableSpanPlan::new(&t);
                let (spans, orphans) = reference_rowspans(&t);
                assert_eq!(
                    plan.rowspans
                        .iter()
                        .map(|(key, &value)| (key, value))
                        .collect::<BTreeMap<_, _>>(),
                    spans
                );
                assert_eq!(
                    plan.orphan_carets
                        .iter()
                        .map(|(key, _)| key)
                        .collect::<BTreeSet<_>>(),
                    orphans
                );
                for (r, row) in t.rows.iter().enumerate() {
                    let consumed = spans
                        .iter()
                        .filter_map(|(&(origin, col), &span)| {
                            (r > origin && r < origin + span).then_some(col)
                        })
                        .collect();
                    for c in 0..row.cells.len() {
                        if row.cells[c].span == Some(TableCellSpan::Colspan) {
                            assert_eq!(
                                plan.rows[r].targets[c],
                                reference_colspan_target(row, c, &consumed).is_some()
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn wide_colspan_carets_keep_their_origin() {
        let width = 16000;
        let mut markers = vec![Some(TableCellSpan::Colspan); width];
        markers[0] = None;
        let t = table(&[markers, vec![Some(TableCellSpan::Rowspan); width]]);
        let plan = TableSpanPlan::new(&t);
        assert_eq!(plan.rows[0].counts.first(), Some(&width));
        assert_eq!(plan.rowspans.get(&(0, 0)), Some(&2));
        assert!(plan.orphan_carets.iter().next().is_none());
    }

    type ReferenceSpans = BTreeMap<(usize, usize), usize>;
    type ReferenceOrphans = BTreeSet<(usize, usize)>;

    fn reference_rowspans(t: &Table) -> (ReferenceSpans, ReferenceOrphans) {
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
                        let covered_by_visible_span = if source[col].span
                            == Some(TableCellSpan::Colspan)
                        {
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
    fn reference_colspan_target(
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
}
