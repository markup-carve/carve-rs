//! A standard table row whose every cell is blank is not a table
//! (markup-carve/carve#1954; spec markup-carve/carve#1950).
//!
//! The detector counted delimiter slots and never asked whether a slot held
//! content, so a multi-cell all-empty row and an empty header cell opened a
//! table where the ruling makes them paragraphs. A cell is blank when it has no
//! inline content AND no span, alignment, valign or attribute run - constructs
//! the author spelled keep the row a table. A glued `=` header marker does not
//! save an otherwise-empty cell.
//!
//! ORACLE: the executable spec (`scripts/spec/layout.mjs` + `html.mjs`) at spec
//! main `a9a2fa79`, which produces the paragraph on every rejected row and the
//! table on every kept one.

use carve::{to_html, to_html_with_options, Options};

/// The library facade and the position-tracking path must agree - the #908
/// guard.
fn flat(source: &str) -> String {
    let facade = to_html(source);
    let positions = to_html_with_options(source, &Options::default().with_positions(true));
    assert_eq!(
        facade, positions,
        "the library path and the position-tracking path disagree on {source:?}"
    );
    facade.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[test]
fn a_multi_cell_all_empty_row_is_a_paragraph() {
    assert_eq!(flat("|||\ntail\n"), "<p>||| tail</p>");
    assert_eq!(flat("||||\ntail\n"), "<p>|||| tail</p>");
}

#[test]
fn a_whitespace_only_row_is_a_paragraph() {
    assert_eq!(flat("| | |\ntail\n"), "<p>| | | tail</p>");
}

#[test]
fn an_empty_header_cell_is_a_paragraph() {
    assert_eq!(flat("|= |\ntail\n"), "<p>|= | tail</p>");
}

#[test]
fn the_bare_double_pipe_stays_a_paragraph() {
    assert_eq!(flat("||\ntail\n"), "<p>|| tail</p>");
}

#[test]
fn one_filled_cell_is_enough_to_keep_the_table() {
    assert_eq!(
        flat("| a | |\n"),
        "<table> <tbody> <tr><td>a</td><td></td></tr> </tbody> </table>"
    );
}

#[test]
fn a_marker_the_author_spelled_keeps_the_table() {
    // Attribute, alignment, and colspan cells each stay a table though empty.
    assert_eq!(
        flat("|{.x} |\n"),
        "<table> <tbody> <tr><td class=\"x\"></td></tr> </tbody> </table>"
    );
    assert_eq!(
        flat("|> |\n"),
        "<table> <tbody> <tr><td style=\"text-align: right;\"></td></tr> </tbody> </table>"
    );
    assert_eq!(
        flat("|<|\n"),
        "<table> <tbody> <tr><td></td></tr> </tbody> </table>"
    );
}

#[test]
fn a_glued_equals_with_empty_body_is_content_not_a_header() {
    // `|=|` is `<td>=</td>` - the `=` is content, so the cell is not blank.
    assert_eq!(
        flat("|=|\n"),
        "<table> <tbody> <tr><td>=</td></tr> </tbody> </table>"
    );
}

#[test]
fn a_mid_table_all_blank_row_ends_the_table() {
    assert_eq!(
        flat("|a|b|\n|||\n"),
        "<table> <tbody> <tr><td>a</td><td>b</td></tr> </tbody> </table> <p>|||</p>"
    );
}
