#[test]
fn a_sibling_ends_the_nested_comment_before_a_document_opener() {
    assert_eq!(
        carve::to_html("- a\n  - %%%\n  - m\n%%%\ntail\n"),
        "<ul>\n  <li>a\n    <ul>\n      <li></li>\n      <li>m</li>\n    </ul>\n  </li>\n</ul>"
    );
    assert_eq!(
        carve::to_html("1. 2) %%%\n    * m\n%%%\ntail\n"),
        "<ol>\n  <li>\n    <ol start=\"2\" data-delim=\")\">\n      <li></li>\n    </ol>\n    <ul>\n      <li>m</li>\n    </ul>\n  </li>\n</ol>"
    );
}
