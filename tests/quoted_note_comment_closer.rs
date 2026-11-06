#[test]
fn unmarked_comment_closer_stays_with_a_quoted_note() {
    let input = "> [^n]: i\n>   . %%%\n%%%\n\n[^n]\n";
    assert_eq!(
        carve::to_html(input),
        "<blockquote>\n\n</blockquote>\n<p><a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>i</p>\n      <ol>\n        <li></li>\n      </ol>\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>"
    );
}

#[test]
fn held_span_closers_precede_quote_ownership_for_item_markers() {
    for marker in [".", "-", "1.", "*"] {
        for payload in ["", ">     payload\n"] {
            let input = format!("> [^n]: i\n>   {marker} %%%\n{payload}%%%\n\n[^n]\n");
            let html = carve::to_html(&input);
            assert!(html.contains("id=\"fnref1\""), "{input}: {html}");
            assert!(html.contains("<p>i</p>"), "{input}: {html}");
            assert!(!html.contains("<pre>"), "{input}: {html}");
        }
    }
}
