//! Nested note ownership in the definition prepass.

use carve::to_html;

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

fn source(marker: &str) -> String {
    format!("> [^n]: p\n>   [^m]: x\n>    {marker} y\n>\n> ref[^m]\n>\n> see[^n]\n")
}

#[test]
fn a_list_marker_below_the_inner_note_opens_in_the_outer_note() {
    let html = to_html(&source("-"));
    assert!(html.contains("<p>x<a href=\"#fnref1\""), "{html}");
    assert!(
        html.contains("<p>p</p>\n      <ul>\n        <li>y</li>\n      </ul>"),
        "{html}"
    );
}

#[test]
fn the_same_boundary_applies_to_an_ordered_marker() {
    let html = to_html(&source("1."));
    assert!(html.contains("<p>x<a href=\"#fnref1\""), "{html}");
    assert!(
        html.contains("<p>p</p>\n      <ol>\n        <li>y</li>\n      </ol>"),
        "{html}"
    );
}

#[test]
fn a_marker_at_the_inner_notes_written_column_also_reaches_the_outer_note() {
    let html = to_html("> [^n]: p\n>   [^m]: x\n>   - y\n>\n> ref[^m]\n>\n> see[^n]\n");
    assert!(html.contains("<p>x<a href=\"#fnref1\""), "{html}");
    assert!(
        html.contains("<p>p</p>\n      <ul>\n        <li>y</li>\n      </ul>"),
        "{html}"
    );
}

#[test]
fn plain_prose_still_continues_the_inner_note() {
    let html = to_html(&source("more"));
    assert!(html.contains("<p>x\nmore y<a href=\"#fnref1\""), "{html}");
    assert!(html.contains("<p>p<a href=\"#fnref2\""), "{html}");
}
