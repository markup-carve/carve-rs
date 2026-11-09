// CARVE-P2-035: the line after a refused wrapped brace run continues its
// paragraph even when it resembles a list marker under a first-block grant.
use carve::to_html;

#[test]
fn description_grant_keeps_the_marker_as_paragraph_text() {
    assert_eq!(
        to_html(":: t\n: +\n{.x\n.y}\n- li\n"),
        "<dl>\n  <dt>t</dt>\n  <dd>{.x\n.y}\n- li</dd>\n</dl>"
    );
}

#[test]
fn note_grant_keeps_the_marker_as_paragraph_text() {
    assert_eq!(
        to_html("[^a]: +\n{.x\n.y}\n- li\n\nsee[^a]\n"),
        "<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>{.x\n.y}\n- li<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>"
    );
}
