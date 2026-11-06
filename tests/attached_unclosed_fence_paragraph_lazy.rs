#[test]
fn paragraph_after_an_attached_unclosed_fence_takes_a_lazy_line() {
    let source = "- a\n  - +\n  ```\n\n    p\ntail\n";
    let expected = "<ul>\n  <li>a\n    <ul>\n      <li>\n        <pre><code></code></pre>\n        <p>p\ntail</p>\n      </li>\n    </ul>\n  </li>\n</ul>";
    assert_eq!(carve::to_html(source), expected);
}

#[test]
fn line_left_of_attached_host_does_not_pull_a_later_closer() {
    let source = "- x\n+\n```\na\n\nb\n``\n>> \n  ```\nz\n";
    let expected = "<ul>\n  <li>x\n    <pre><code>a\n</code></pre>\n  </li>\n</ul>\n<p>b\n<code>\n&gt;&gt;\n```\nz</code></p>";
    assert_eq!(carve::to_html(source), expected);
}
