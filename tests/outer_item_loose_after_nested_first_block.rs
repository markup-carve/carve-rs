use carve::to_html;

#[test]
fn a_paragraph_after_a_nested_fence_loosens_the_outer_item() {
    assert_eq!(
        to_html("- - ```\n\n  x"),
        "<ul>\n  <li>\n    <ul>\n      <li>\n        <pre><code>\n</code></pre>\n      </li>\n    </ul>\n    <p>x</p>\n  </li>\n</ul>"
    );
}

#[test]
fn a_paragraph_after_nested_text_loosens_the_outer_item() {
    assert_eq!(
        to_html("- - x\n\n  y"),
        "<ul>\n  <li>\n    <ul>\n      <li>x</li>\n    </ul>\n    <p>y</p>\n  </li>\n</ul>"
    );
}
