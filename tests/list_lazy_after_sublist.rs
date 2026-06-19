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
fn indented_marker_after_lazy_resumes_same_sublist() {
    // The `lazy` line folds into the inner item; the following `2. sibling`,
    // indented to the sublist's content column, resumes the SAME sublist rather
    // than opening a fresh one (no stray `<ol start="2">`). Matches carve-php
    // and carve-js (carve spec corpus 05-lists-17).
    assert_eq!(
        carve::to_html("1. outer\n   1. inner\nlazy\n   2. sibling"),
        "<ol>\n  <li>outer\n    <ol>\n      <li>inner\nlazy</li>\n      <li>sibling</li>\n    </ol>\n  </li>\n</ol>"
    );
}

#[test]
fn blank_line_after_sublist_ends_the_list() {
    assert_eq!(
        carve::to_html("- a\n  - b\n\ntext"),
        "<ul>\n  <li>a\n    <ul>\n      <li>b</li>\n    </ul>\n  </li>\n</ul>\n<p>text</p>"
    );
}
