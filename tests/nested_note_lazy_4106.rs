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
