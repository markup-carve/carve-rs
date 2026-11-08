//! A literal `+` after a note's first-block marker keeps the note's ownership.

use carve::to_html;

#[test]
fn an_off_column_plus_starts_the_attached_paragraph() {
    let source = "x[^a]\n\n[^a]: +\n +\nq\n";
    let expected = "<p>x<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>+\nq<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>";
    assert_eq!(to_html(source).trim_end(), expected);
}
