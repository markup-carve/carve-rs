#[test]
fn a_closer_below_a_deep_attached_fence_opens_at_the_outer_item() {
    let source = "1. a\n   - b\n     - +\n     ```\n     x\n    ```\n";
    let expected = "<ol>\n  <li>a\n    <ul>\n      <li>b\n        <ul>\n          <li>\n            <pre><code>x\n</code></pre>\n          </li>\n        </ul>\n      </li>\n    </ul>\n    <pre><code></code></pre>\n  </li>\n</ol>";
    assert_eq!(carve::to_html(source), expected);
}
