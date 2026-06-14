//! A column-0 plain continuation line after a nested sublist folds into the
//! DEEPEST open item (no blank line), matching carve-js and carve-php. A blank
//! line still ends the list; a block-opener or a sibling marker is not absorbed.

#[test]
fn lazy_after_sublist_folds_into_deepest() {
    assert_eq!(
        carve::to_html("- a\n  - b\nlazy"),
        "<ul>\n  <li>a\n    <ul>\n      <li>b\nlazy</li>\n    </ul>\n  </li>\n</ul>"
    );
}

#[test]
fn sibling_marker_after_lazy_starts_new_item() {
    assert_eq!(
        carve::to_html("- a\n  - b\nlazy\n- c"),
        "<ul>\n  <li>a\n    <ul>\n      <li>b\nlazy</li>\n    </ul>\n  </li>\n  <li>c</li>\n</ul>"
    );
}

#[test]
fn blank_line_after_sublist_ends_the_list() {
    assert_eq!(
        carve::to_html("- a\n  - b\n\ntext"),
        "<ul>\n  <li>a\n    <ul>\n      <li>b</li>\n    </ul>\n  </li>\n</ul>\n<p>text</p>"
    );
}
