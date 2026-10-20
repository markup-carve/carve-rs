use carve::to_html;

#[test]
fn closed_fence_after_nested_marker_stays_a_document_block() {
    let source = "- - ```\n  +\n~~~\ny\n~~~\ntail";
    let expected = "<ul>\n  <li>\n    <ul>\n      <li>\n        <pre><code></code></pre>\n      </li>\n    </ul>\n  </li>\n</ul>\n<pre><code>y\n</code></pre>\n<p>tail</p>";
    assert_eq!(to_html(source), expected);
}

#[test]
fn comment_before_attached_paragraph_keeps_paragraph_in_item() {
    let source = "- - ```\n  +\n%% c\ntail";
    let expected = "<ul>\n  <li>\n    <ul>\n      <li>\n        <pre><code></code></pre>\n        tail\n      </li>\n    </ul>\n  </li>\n</ul>";
    assert_eq!(to_html(source), expected);
}
