//! PART 0 S1: a line that misses the comment fence's owner ends the span.

use carve::to_html;

#[test]
fn a_quote_comment_does_not_hide_an_unprefixed_line() {
    assert_eq!(
        to_html("> %%%\n> x\ny\n> %%%\n").trim(),
        "<blockquote><p>x</p></blockquote>\n<p>y</p>\n<blockquote>\n\n</blockquote>"
    );
}

#[test]
fn a_blank_stops_the_search_for_a_quoted_comment_closer() {
    assert_eq!(
        to_html("> %%%\n> x\ny\n\n> %%%\n").trim(),
        "<blockquote><p>x\ny</p></blockquote>\n<blockquote>\n\n</blockquote>"
    );
}

#[test]
fn a_description_comment_does_not_fold_a_line_below_its_column() {
    assert_eq!(
        to_html(":: t\n: %%%\n  x\ny\n  %%%\n").trim(),
        "<dl>\n  <dt>t</dt>\n  <dd>x</dd>\n</dl>\n<p>y</p>"
    );
}

#[test]
fn a_note_comment_does_not_hide_a_line_below_its_column() {
    assert_eq!(
        to_html("[^n]: %%%\n  x\ny\n  %%%\n\nr[^n]\n").trim(),
        "<p>y</p>\n<p>r<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>x<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>"
    );
}
