//! A GFM delimiter row sets COLUMN alignment, and that lands on the header cells
//! only -- the same tree the native `|=<` markers produce.
//!
//! It used to be applied to every body row as well, so the same logical table
//! parsed to two different trees depending on which separator syntax the author
//! used, and the writer then serialized those propagated values as per-cell
//! markers nobody wrote (carve#352, corpus 09-tables-3).
//!
//! The two spellings no longer format alike: `table.separatorWidths` records
//! which one the author wrote (carve#2894). What this file still pins is that
//! the alignment does not reappear as a per-cell marker either way.

const GFM: &str = "| Name | Age |\n|:-----|----:|\n| Alice | 28  |\n";
const NATIVE: &str = "|=< Name |=> Age |\n| Alice | 28 |\n";

#[test]
fn formatting_does_not_invent_per_cell_markers() {
    // The alignment stays on the separator row that carried it, so no body cell
    // and no header cell grows a `<` or `>` nobody wrote.
    assert_eq!(
        carve::to_carve(GFM),
        "| Name | Age |\n|:-----|----:|\n| Alice | 28 |\n"
    );
    assert!(!carve::to_carve(GFM).contains("|=<"));
    assert!(!carve::to_carve(GFM).contains("|=>"));
}

#[test]
fn each_separator_syntax_formats_to_its_own_spelling() {
    // The two used to format alike, because the writer normalized the delimiter
    // row away. `table.separatorWidths` records which one the author wrote
    // (carve#2894), so each keeps its own form - while the TREES still agree
    // apart from that one field, and the HTML is identical either way.
    assert_ne!(carve::to_carve(GFM), carve::to_carve(NATIVE));
    assert_eq!(carve::to_carve(NATIVE), NATIVE);
    assert_eq!(carve::to_html(&carve::to_carve(GFM)), carve::to_html(GFM));
}

#[test]
fn body_cells_still_render_aligned_via_column_inheritance() {
    // Nothing is lost by dropping the propagation: the HTML renderer inherits
    // column alignment for a body cell whose own align is unset.
    let html = carve::to_html(GFM);
    assert!(
        html.contains("<td style=\"text-align: left;\">Alice</td>"),
        "got: {html}"
    );
    assert!(
        html.contains("<td style=\"text-align: right;\">28</td>"),
        "got: {html}"
    );
}

#[test]
fn the_html_is_unchanged_between_the_two_syntaxes() {
    assert_eq!(carve::to_html(GFM), carve::to_html(NATIVE));
}

#[test]
fn a_genuine_per_cell_override_survives() {
    // The header says right; one body cell overrides to left. That marker is not
    // redundant, and suppressing it would look identical from the outside to
    // suppressing the redundant ones.
    // Written with the space that ends each marker run (§20 T11); `|=Item|`
    // would be a data cell whose text is `=Item`.
    let src = "|= Item |=> Qty |\n| Apple | 12 |\n| Subtotal |< 12 |\n";
    assert_eq!(
        carve::to_carve(src),
        "|= Item |=> Qty |\n| Apple | 12 |\n| Subtotal |< 12 |\n"
    );
}
