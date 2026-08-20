//! A HEAD row resolves its continuation cells the way a body row does.
//!
//! It used to resolve neither. A `<` rendered an empty `<th>` instead of
//! widening the cell to its left, so a header cell spanning columns lost the
//! span and the table gained a column it does not have; a `^` rendered an empty
//! `<th>` BESIDE the `rowspan` its origin already carried, so the row under a
//! header rowspan came out one cell too wide. carve-js resolves both, which is
//! what these pin.

/// `| Group | < |` under a delimiter row: one header cell, two columns.
#[test]
fn a_colspan_in_the_head_widens_the_cell_to_its_left() {
    assert_eq!(
        carve::to_html("| Group | < |\n|---|---|\n| 1 | 2 |"),
        "<table>\n  <thead>\n    <tr><th scope=\"col\" colspan=\"2\">Group</th></tr>\n  </thead>\n  <tbody>\n    <tr><td>1</td><td>2</td></tr>\n  </tbody>\n</table>"
    );
}

/// A rowspan WITHIN the head: the second head row keeps only its own cell, and
/// the row stays in the head - a resolved continuation renders nothing, so it
/// cannot be the non-header cell that ends the run.
#[test]
fn a_rowspan_inside_the_head_keeps_its_row_in_the_head() {
    assert_eq!(
        carve::to_html("|= H |= A |\n| ^ |= B |\n| 1 | 2 |"),
        "<table>\n  <thead>\n    <tr><th scope=\"col\" rowspan=\"2\">H</th><th scope=\"col\">A</th></tr>\n    <tr><th scope=\"col\">B</th></tr>\n  </thead>\n  <tbody>\n    <tr><td>1</td><td>2</td></tr>\n  </tbody>\n</table>"
    );
}

/// CONTROL: an ORPHAN marker in the head has nothing to resolve to, so it still
/// renders an empty cell rather than disappearing (PART 9 §5). Both markers,
/// because each has its own arm: a `^` in the first row has no row above it, and
/// a `<` in the first column has no cell to its left.
#[test]
fn an_orphan_marker_in_the_head_still_renders_a_cell() {
    assert_eq!(
        carve::to_html("| < | b |\n|---|---|\n| 1 | 2 |"),
        "<table>\n  <thead>\n    <tr><th scope=\"col\"></th><th scope=\"col\">b</th></tr>\n  </thead>\n  <tbody>\n    <tr><td>1</td><td>2</td></tr>\n  </tbody>\n</table>"
    );
    assert_eq!(
        carve::to_html("| ^ | b |\n|---|---|\n| 1 | 2 |"),
        "<table>\n  <thead>\n    <tr><th scope=\"col\"></th><th scope=\"col\">b</th></tr>\n  </thead>\n  <tbody>\n    <tr><td>1</td><td>2</td></tr>\n  </tbody>\n</table>"
    );
}
