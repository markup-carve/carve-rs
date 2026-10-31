use carve::to_html;

#[test]
fn a_nested_item_releases_a_line_below_its_own_column_after_a_comment_span() {
    let source = "- a\n  - %%%\n    hidden\n%%%\n { - x\n";
    assert_eq!(
        to_html(source).trim(),
        "<ul>\n  <li>a\n    <ul>\n      <li></li>\n    </ul>\n  </li>\n</ul>\n<p>{ - x</p>"
    );
}

#[test]
fn a_note_releases_a_line_below_its_body_after_a_comment_span() {
    let source = "[^n]: a\n  %%%\n  hidden\n%%%\nx\n\nr[^n]\n";
    assert_eq!(
        to_html(source).trim(),
        "<p>x</p>\n<p>r<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>a<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>"
    );
}
