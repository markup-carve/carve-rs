#[test]
fn nested_note_keeps_its_attached_quote_follower() {
    let source = "- z\n  - a\n    [^n]: +\n    > q\n      > t\n\nsee[^n]\n";
    let expected = "<ul>\n  <li>z\n    <ul>\n      <li>a</li>\n    </ul>\n  </li>\n</ul>\n<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <blockquote><p>q\nt</p></blockquote>\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>";
    assert_eq!(carve::to_html(source), expected);
}

#[test]
fn flat_note_keeps_the_same_quote_continuation() {
    let source = "- a\n  [^n]: +\n  > q\n    > t\n\nsee[^n]\n";
    let html = carve::to_html(source);
    assert!(
        html.contains("<blockquote><p>q\nt</p></blockquote>"),
        "{html}"
    );
    assert!(!html.contains("<li>a\n"), "{html}");
}

#[test]
fn quote_continuation_one_column_below_the_note_body_floor() {
    let source = "- a\n  [^n]: +\n  > q\n   > t\n\nsee[^n]\n";
    let html = carve::to_html(source);
    assert!(
        html.contains("<blockquote><p>q\nt</p></blockquote>"),
        "{html}"
    );
    assert!(!html.contains("<li>a\n"), "{html}");
}
