//! Nested note ownership in the definition prepass.

#[test]
fn lazy_line_stays_in_nested_note_before_unclosed_comment() {
    let html = carve::to_html("[^n0]: x\n  [^n1]: nb\n  c\n    %%%\n  > ind1\n\nsee[^n0][^n1]\n");
    assert!(html.contains("<p>nb\nc<a href=\"#fnref2\""), "{html}");
    assert!(
        html.contains("<blockquote><p>ind1</p></blockquote>"),
        "{html}"
    );
    assert!(!html.contains("<p>c<a href=\"#fnref1\""), "{html}");
}

#[test]
fn a_losing_nested_note_owns_and_discards_its_lazy_reference() {
    let source = "[^n]: a\n   [^n]: a\n  [^n]\n\n[^n]\n";
    let html = carve::to_html(source);
    assert_eq!(
        html,
        "<p><a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>a<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>"
    );
}
