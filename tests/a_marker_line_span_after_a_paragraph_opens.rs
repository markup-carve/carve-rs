// carve#5314: a `%%%` on a nested item's marker line opens the span even
// right after the body's paragraph, so its column-0 closer ends it.

#[test]
fn a_note_marker_line_span_closed_at_column_0_leaves_the_document() {
    let html = carve::to_html("[^n]: i\n  . %%%\n    n\n%%%\nzz\n\n[^n]\n");
    assert_eq!(html, "<p>zz</p>\n<p><a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>i</p>\n      <ol>\n        <li></li>\n      </ol>\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>");
}

#[test]
fn a_note_bullet_marker_line_span_closed_at_column_0_leaves_the_document() {
    let html = carve::to_html("[^n]: i\n  - %%%\n    n\n%%%\nzz\n\n[^n]\n");
    assert_eq!(html, "<p>zz</p>\n<p><a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>i</p>\n      <ul>\n        <li></li>\n      </ul>\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>");
}

#[test]
fn an_item_marker_line_span_closed_at_column_0_ends_the_list() {
    let html = carve::to_html("- i\n  . %%%\n    n\n%%%\nzz\n");
    assert_eq!(html, "<ul>\n  <li>i\n    <ol>\n      <li></li>\n    </ol>\n  </li>\n</ul>\n<p>zz</p>");
}
