//! markup-carve/carve#1718 and the caption clause in markup-carve/carve#1742.
//! A fenced quote IS a block quote, so it captions like one: the slot hangs on
//! its CLOSING fence and a captioned quote is a figure either way. Asserted
//! against the prefixed spelling rather than pinned HTML, since the point is
//! that the two agree.

use carve::to_html;

#[test]
fn wraps_it_in_a_figure_exactly_as_the_prefixed_spelling_does() {
    assert_eq!(
        to_html("::: >\nStay hungry.\n:::\n^ Steve Jobs\n"),
        to_html("> Stay hungry.\n^ Steve Jobs\n"),
    );
}

#[test]
fn still_allows_one_blank_line_between_the_closer_and_the_caption() {
    assert_eq!(
        to_html("::: >\nStay hungry.\n:::\n\n^ Steve Jobs\n"),
        to_html("> Stay hungry.\n\n^ Steve Jobs\n"),
    );
}
