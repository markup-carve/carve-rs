#[test]
fn a_sublist_chunk_leaves_its_trailing_attribute_for_the_fence() {
    let source = "- a\n  - b\n  {.x}\n  ```\n  code\n  ```\n";
    let expected = "<ul>\n  <li>a\n    <ul>\n      <li>b</li>\n    </ul>\n    <pre class=\"x\"><code>code\n</code></pre>\n  </li>\n</ul>\n";
    assert_eq!(carve::to_html(source), expected.trim_end());
}

#[test]
fn a_trailing_attribute_survives_a_refused_continuation_before_the_sublist() {
    let source = "- a\n  +\n  - b\n  {.x}\n  ~~~\n  code\n  ~~~\n";
    let expected = "<ul>\n  <li>a\n+\n    <ul>\n      <li>b</li>\n    </ul>\n    <pre class=\"x\"><code>code\n</code></pre>\n  </li>\n</ul>\n";
    assert_eq!(carve::to_html(source), expected.trim_end());
}
