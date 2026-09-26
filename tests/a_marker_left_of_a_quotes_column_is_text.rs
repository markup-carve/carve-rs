//! A `+` ONE COLUMN LEFT OF AN IN-ITEM QUOTE'S MARKER IS TEXT
//! (markup-carve/carve-php#2470).
//!
//! `CARVE-P9-031` places the continuation marker at its container's MARKER
//! COLUMN and nowhere else. Under `- x` / `  > q` those columns are 0 and 2,
//! so a `+` at column 1 names no container and falls through to the quote's
//! lazy fold.
//!
//! The quote's body arrives stripped of the item's indentation, so the marker
//! at column 1 was spelled like one at the quote's own column and the
//! column-0 test on the stripped view could not tell them apart. The source
//! column can.

fn html(source: &str) -> String {
    carve::to_html(source)
}

fn with_marker_at(column: usize) -> String {
    html(&format!("- x\n  > q\n{}+\n", " ".repeat(column)))
}

const CONSUMED: &str = "<ul>\n  <li>x\n    <blockquote><p>q</p></blockquote>\n  </li>\n</ul>";
const KEPT: &str = "<ul>\n  <li>x\n    <blockquote><p>q\n+</p></blockquote>\n  </li>\n</ul>";

#[test]
fn the_marker_column_of_each_container_still_consumes_the_marker() {
    // Column 0 is the list's marker column, column 2 the quote's. Both were
    // already consuming and must keep doing so.
    assert_eq!(with_marker_at(0), CONSUMED);
    assert_eq!(with_marker_at(2), CONSUMED);
}

#[test]
fn a_column_naming_no_container_is_text() {
    assert_eq!(with_marker_at(1), KEPT);
}

#[test]
fn a_column_inside_the_quote_content_is_text() {
    // These two were already text: an indented `+` is not marker-shaped on the
    // stripped view either.
    assert_eq!(with_marker_at(3), KEPT);
    assert_eq!(with_marker_at(4), KEPT);
}

#[test]
fn a_top_level_quotes_own_marker_still_attaches_a_block() {
    assert_eq!(
        html("> q\n+\n- m\n"),
        "<blockquote>\n  <p>q</p>\n  <ul>\n    <li>m</li>\n  </ul>\n</blockquote>"
    );
}

#[test]
fn a_marker_one_column_right_of_a_top_level_quote_is_text() {
    assert_eq!(
        html("> q\n +\n- m\n"),
        "<blockquote><p>q\n+\n- m</p></blockquote>"
    );
}
