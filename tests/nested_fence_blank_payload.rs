#[test]
fn unclosed_nested_fence_keeps_blank_payload_before_outer_text() {
    let source = "- - ```\n\n\n  tail";
    assert_eq!(
        carve::to_html(source),
        "<ul>\n  <li>\n    <ul>\n      <li>\n        <pre><code>\n\n</code></pre>\n      </li>\n    </ul>\n    <p>tail</p>\n  </li>\n</ul>"
    );
}
