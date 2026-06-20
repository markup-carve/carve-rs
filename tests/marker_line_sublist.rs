//! A sub-list opened on a list item's MARKER LINE (`- - A`) parses as a normal,
//! persistent nested list: following same-indent markers MERGE into it, and a
//! post-blank indented block is ABSORBED into its items. This matches reference
//! djot.js (@djot/djot 0.3.2) and CommonMark; it corrects Carve's prior
//! line-scoping (a bug inherited from djot-php) which split the sub-list from
//! following items and leaked later indented blocks to the parent row.

#[test]
fn following_same_indent_markers_merge_into_marker_line_sublist() {
    // `- - A` opens an inner list on the marker line; `  - B` / `  - C` at the
    // sub-list's column merge in as siblings -> outer item with ONE inner list
    // [A, B, C] (previously: A split off, only [B, C] nested).
    assert_eq!(
        carve::to_html("- - A\n  - B\n  - C\n"),
        "<ul>\n  <li>\n    <ul>\n      <li>A</li>\n      <li>B</li>\n      <li>C</li>\n    </ul>\n  </li>\n</ul>"
    );
}

#[test]
fn post_blank_indented_block_is_absorbed_into_marker_line_sublist_item() {
    // The blank + 4-space-indented `second` is absorbed as a second paragraph of
    // the inner item A (not leaked to the parent row); `  - B` is a sibling.
    assert_eq!(
        carve::to_html("- - A\n\n    second\n  - B\n"),
        "<ul>\n  <li>\n    <ul>\n      <li><p>A</p>\n        <p>second</p>\n      </li>\n      <li><p>B</p></li>\n    </ul>\n  </li>\n</ul>"
    );
}

#[test]
fn star_bullet_marker_line_sublist_merges() {
    assert_eq!(
        carve::to_html("* - A\n  - B\n"),
        "<ul>\n  <li>\n    <ul>\n      <li>A</li>\n      <li>B</li>\n    </ul>\n  </li>\n</ul>"
    );
}

#[test]
fn ordered_outer_marker_line_sublist_merges() {
    assert_eq!(
        carve::to_html("1. - A\n   - B\n"),
        "<ol>\n  <li>\n    <ul>\n      <li>A</li>\n      <li>B</li>\n    </ul>\n  </li>\n</ol>"
    );
}

#[test]
fn flush_left_lazy_after_marker_line_sublist_folds_into_inner_item() {
    // A column-0 lazy-continuation line folds into the marker-line sub-list's
    // open paragraph instead of leaking to the top level -- the marker-line
    // sub-list behaves like a normal nested list (matches reference djot.js).
    assert_eq!(
        carve::to_html("- - b\nlazy\n"),
        "<ul>\n  <li>\n    <ul>\n      <li>b\nlazy</li>\n    </ul>\n  </li>\n</ul>"
    );
}
