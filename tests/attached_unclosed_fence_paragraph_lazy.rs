#[test]
fn paragraph_after_an_attached_unclosed_fence_takes_a_lazy_line() {
    let source = "- a\n  - +\n  ```\n\n    p\ntail\n";
    let expected = "<ul>\n  <li>a\n    <ul>\n      <li>\n        <pre><code></code></pre>\n        <p>p\ntail</p>\n      </li>\n    </ul>\n  </li>\n</ul>";
    assert_eq!(carve::to_html(source), expected);
}
