//! PART 0 S1: a line that misses the comment fence's owner ends the span.

use carve::to_html;

#[test]
fn a_quote_comment_does_not_hide_an_unprefixed_line() {
    assert_eq!(
        to_html("> %%%\n> x\ny\n> %%%\n").trim(),
        "<blockquote>\n\n</blockquote>\n<p>y</p>\n<blockquote>\n\n</blockquote>"
    );
}

#[test]
fn a_blank_stops_the_search_for_a_quoted_comment_closer() {
    assert_eq!(
        to_html("> %%%\n> x\ny\n\n> %%%\n").trim(),
        "<blockquote>\n\n</blockquote>\n<p>y</p>\n<blockquote>\n\n</blockquote>"
    );
}

#[test]
fn a_description_comment_does_not_fold_a_line_below_its_column() {
    assert_eq!(
        to_html(":: t\n: %%%\n  x\ny\n  %%%\n").trim(),
        "<dl>\n  <dt>t</dt>\n  <dd></dd>\n</dl>\n<p>y</p>"
    );
}

#[test]
fn a_note_comment_does_not_hide_a_line_below_its_column() {
    assert_eq!(
        to_html("[^n]: %%%\n  x\ny\n  %%%\n\nr[^n]\n").trim(),
        "<p>y</p>"
    );
}
