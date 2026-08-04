//! A blank line ends a COMPACT nested item too (carve-rs#581, corpus 190).
//!
//! `- - a` puts the outer item's content on its marker line, so the
//! continuation collector had nothing collected when the blank arrived and its
//! "does the next line still reach the content column" guard never ran. The
//! line after the blank then joined the item whatever its column - carve-js and
//! carve-php end the list there and parse the line at document level.
//!
//! The comment in the corpus case is a red herring: the same document without
//! it diverged the same way.

use carve::to_html;

const CLOSED: &str =
    "<ul>\n  <li>\n    <ul>\n      <li>a</li>\n    </ul>\n  </li>\n</ul>\n<p>b</p>";

#[test]
fn a_blank_then_a_below_column_line_ends_the_list() {
    assert_eq!(to_html("- - a\n\n b\n"), CLOSED);
}

#[test]
fn a_comment_before_the_blank_changes_nothing() {
    assert_eq!(to_html("- - a\n %% c\n\n b\n"), CLOSED);
}

#[test]
fn a_line_at_the_outer_content_column_still_belongs_to_the_item() {
    // Column 2 IS the outer item's content column, so this one stays inside -
    // the rule is the column, not the blank.
    assert_eq!(
        to_html("- - a\n\n  b\n"),
        "<ul>\n  <li>\n    <ul>\n      <li>a</li>\n    </ul>\n    <p>b</p>\n  </li>\n</ul>"
    );
}

#[test]
fn the_simple_item_shape_is_unchanged() {
    assert_eq!(
        to_html("- a\n\n b\n"),
        "<ul>\n  <li>a</li>\n</ul>\n<p>b</p>"
    );
}
