use carve::to_html;

#[test]
fn marker_line_item_fence_keeps_tabbed_payload_and_closer() {
    let source = "[^n]: - ~~~js\n\t\ta\n\t\t~~~\n\n[^n]\n";
    let expected = "<p><a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <ul>\n        <li>\n          <pre><code class=\"language-js\">a\n</code></pre>\n        </li>\n      </ul>\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>";
    assert_eq!(to_html(source).trim(), expected);
}
#[test]
fn note_attached_marker_fence_keeps_its_trailing_blank() {
    let source = "x[^a]\n\n[^a]: +\n- ```\n  - nest\n\ny\n";
    let html = carve::to_html(source);
    assert!(
        html.contains("<pre><code>- nest\n\n</code></pre>"),
        "{html}"
    );
}
