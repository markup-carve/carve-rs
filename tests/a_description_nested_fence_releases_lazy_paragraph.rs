use carve::to_html;

#[test]
fn an_unclosed_item_fence_releases_a_description_paragraph() {
    let source = ":: t\n: - ~~~\n  x\ntail\n";
    let expected = "<dl>\n  <dt>t</dt>\n  <dd>\n    <ul>\n      <li>\n        <pre><code></code></pre>\n      </li>\n    </ul>\n    <p>x\ntail</p>\n  </dd>\n</dl>";
    assert_eq!(to_html(source), expected);
}

#[test]
fn the_same_boundary_applies_after_two_nested_items() {
    let source = ":: t\n: - - ~~~\n  x\ntail\n";
    let html = to_html(source);
    assert!(html.contains("<p>x\ntail</p>"), "{html}");
}

#[test]
fn a_line_at_the_fence_column_remains_its_payload() {
    let source = ":: t\n: - ~~~\n    x\n";
    let html = to_html(source);
    assert!(html.contains("<pre><code>x\n</code></pre>"), "{html}");
}
