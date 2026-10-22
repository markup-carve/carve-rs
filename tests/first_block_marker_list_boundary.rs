fn html(source: &str) -> String {
    carve::to_html(source).trim().to_string()
}

#[test]
fn footnote_first_block_marker_leaves_a_column_zero_list_outside() {
    assert_eq!(
        html("see[^a]\n\n[^a]: +\n- a list\n"),
        "<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n\
<ul>\n  <li>a list</li>\n</ul>\n\
<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>+<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>"
    );
}

#[test]
fn description_first_block_marker_leaves_a_column_zero_list_outside() {
    assert_eq!(
        html(":: t\n: +\n- a list\n"),
        "<dl>\n  <dt>t</dt>\n  <dd></dd>\n</dl>\n<ul>\n  <li>a list</li>\n</ul>"
    );
}
