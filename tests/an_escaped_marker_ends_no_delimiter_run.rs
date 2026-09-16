//! PART 11 section 8a, M1: a delimiter the writer escaped is TEXT. The seam
//! pass counted it as part of the run it sits next to, so a span ending in
//! `\*` merged with the next span's opener (markup-carve/carve-rs#1654).

use carve::{parse, render_markdown};

fn md(src: &str) -> String {
    render_markdown(&parse(src)).expect("the document writes")
}

#[test]
fn a_span_ending_in_an_escaped_marker_does_not_merge_with_the_next() {
    for (src, want) in [
        ("{/x*/}{/~y/}", "*x\\**<em>\\~y</em>\n"),
        ("{/x**/}{/~y/}", "*x\\*\\**<em>\\~y</em>\n"),
        ("{*x**}{*~y*}", "**x\\***<strong>\\~y</strong>\n"),
        ("{*x***}{*~y*}", "**x\\*\\***<strong>\\~y</strong>\n"),
    ] {
        assert_eq!(md(src), want, "{src}");
    }
}

#[test]
fn a_text_node_ending_in_an_escaped_marker_leaves_the_next_span_alone() {
    // The escaped marker ends no run at all, so the span after it keeps its
    // delimiters instead of falling back to inline HTML.
    assert_eq!(md("a*{/~x/}"), "a\\**\\~x*\n");
}

#[test]
fn two_ordinary_adjacent_spans_are_untouched() {
    // CONTROL. Neither side ends in an escape, so the seam pass answers as it
    // did before: the later span takes the inline-HTML form.
    assert_eq!(md("{/x/}{/y/}"), "*x*<em>y</em>\n");
    assert_eq!(md("{~x~}{~y~}"), "~~x~~<del>y</del>\n");
}

#[test]
fn a_literal_backslash_in_front_of_a_marker_is_not_an_escape() {
    // Two backslashes are one literal backslash, so the marker after them
    // still closes the run and the seam is the ordinary one.
    assert_eq!(md("{/x\\\\/}{/~y/}"), "*x\\\\*<em>\\~y</em>\n");
}
