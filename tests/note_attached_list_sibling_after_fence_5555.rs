//! A blank inside an attached list child's open fence does not end the list.

#[test]
fn sibling_after_child_fence_blank_stays_in_attached_note_list() {
    let source = "[^a]: +\n- li\n  ```\n  > quoted\n\n- sibling\n\nsee[^a]\n";
    let expected = "<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <ul>\n        <li><p>li</p>\n          <pre><code>&gt; quoted\n\n</code></pre>\n        </li>\n        <li><p>sibling</p></li>\n      </ul>\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>";
    assert_eq!(carve::to_html(source), expected);
}

#[test]
fn nested_sibling_after_child_fence_blank_stays_in_nested_list() {
    let source = "[^a]: +\n- outer\n  - li\n    ```\n    > quoted\n\n  - sibling\n\nsee[^a]\n";
    let out = carve::to_html(source);
    assert!(out.contains("<li>outer\n          <ul>"), "{out}");
    assert!(out.contains("<li><p>sibling</p></li>"), "{out}");
    assert!(!out.contains("<li><p>outer</p>"), "{out}");
}

#[test]
fn ordered_marker_below_inner_column_stays_in_outer_fence() {
    let source = "[^a]: +\n- outer\n  1. li\n    ```\n    > quoted\n\n  2. sibling\n\nsee[^a]\n";
    let out = carve::to_html(source);
    assert!(
        out.contains("<pre><code>&gt; quoted\n\n2. sibling\n</code></pre>"),
        "{out}"
    );
    assert!(!out.contains("<ol start=\"2\">"), "{out}");
}
