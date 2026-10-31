#[test]
fn an_open_nested_fence_blank_separates_the_outer_items_next_paragraph() {
    let source = "- a\n  - ```\n    x\n\n  text\n";
    let expected = "<ul>\n  <li><p>a</p>\n    <ul>\n      <li>\n        <pre><code>x\n\n</code></pre>\n      </li>\n    </ul>\n    <p>text</p>\n  </li>\n</ul>";
    assert_eq!(carve::to_html(source), expected);
}

#[test]
fn a_line_below_the_nested_fence_ends_its_payload_before_a_blank() {
    let source = "- a\n  - ```\n   x\n\n   +\n > q\n";
    let expected = "<ul>\n  <li><p>a</p>\n    <ul>\n      <li>\n        <pre><code></code></pre>\n      </li>\n    </ul>\n    <p>x</p>\n    <p>+</p>\n  </li>\n</ul>\n<blockquote><p>q</p></blockquote>";
    assert_eq!(carve::to_html(source), expected);
}

#[test]
fn a_closed_nested_fence_also_leaves_the_outer_paragraph_separated() {
    let source = "- a\n  - ```\n    x\n    ```\n\n  text\n";
    let expected = "<ul>\n  <li><p>a</p>\n    <ul>\n      <li>\n        <pre><code>x\n</code></pre>\n      </li>\n    </ul>\n    <p>text</p>\n  </li>\n</ul>";
    assert_eq!(carve::to_html(source), expected);
}

#[test]
fn a_nested_fence_blank_before_a_sibling_keeps_the_list_loose() {
    let source = "- a\n  - ```\n    x\n\n- b\n";
    let expected = "<ul>\n  <li><p>a</p>\n    <ul>\n      <li>\n        <pre><code>x\n\n</code></pre>\n      </li>\n    </ul>\n  </li>\n  <li><p>b</p></li>\n</ul>";
    assert_eq!(carve::to_html(source), expected);
}
