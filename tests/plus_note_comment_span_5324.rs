const NOTE: &str = "<p>x<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <pre><code>\n</code></pre>\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>";

#[test]
fn a_note_plus_attaches_the_fence_after_a_comment_span() {
    let source = "[^m]: +\n  %%%\n  q\n  %%%\n```\n\n```\n\nx[^m]\n";
    assert_eq!(carve::to_html(source), NOTE);
}

#[test]
fn a_note_plus_attaches_the_fence_after_a_line_comment() {
    let source = "[^m]: +\n%% c\n```\n\n```\n\nx[^m]\n";
    assert_eq!(carve::to_html(source), NOTE);
}
