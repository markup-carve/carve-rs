#[test]
fn bullet_at_content_column_after_continuation_paragraph_nests() {
    assert_eq!(
        carve::to_html("- first\n\n  second\n  - nested"),
        "<ul>\n  <li><p>first</p>\n    <p>second</p>\n    <ul>\n      <li>nested</li>\n    </ul>\n  </li>\n</ul>"
    );
}

#[test]
fn ordered_at_content_column_after_continuation_paragraph_nests() {
    assert_eq!(
        carve::to_html("- first\n\n  second\n  1. nested"),
        "<ul>\n  <li><p>first</p>\n    <p>second</p>\n    <ol>\n      <li>nested</li>\n    </ol>\n  </li>\n</ul>"
    );
}

#[test]
fn top_level_marker_after_paragraph_still_folds() {
    assert_eq!(carve::to_html("para\n- item"), "<p>para\n- item</p>");
}
