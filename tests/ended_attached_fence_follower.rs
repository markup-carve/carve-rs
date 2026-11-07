// carve#5401: the blank ends an unclosed attached fence before the next line
// selects an owner. The follower's column then closes or resumes the item.

#[test]
fn a_band_heading_after_an_ended_attached_fence_opens_outside_the_item() {
    let source = "* +\n```\n\n # q\n";
    let expected = "<ul>\n  <li>\n    <pre><code></code></pre>\n  </li>\n</ul>\n<section id=\"q\">\n  <h1>q</h1>\n</section>";
    assert_eq!(carve::to_html(source), expected);
}

#[test]
fn a_nested_band_quote_after_an_ended_attached_fence_opens_in_the_outer_item() {
    let source = "- a\n  * +\n  ```\n\n   > q\n";
    let expected = "<ul>\n  <li><p>a</p>\n    <ul>\n      <li>\n        <pre><code></code></pre>\n      </li>\n    </ul>\n    <blockquote><p>q</p></blockquote>\n  </li>\n</ul>";
    assert_eq!(carve::to_html(source), expected);
}

#[test]
fn a_closed_attached_fence_keeps_its_blank_as_payload() {
    let source = "* +\n```\n\n # q\n```\n";
    assert!(carve::to_html(source).contains("<pre><code>\n # q\n</code></pre>"));
}

#[test]
fn a_band_sibling_after_an_ended_attached_fence_joins_the_list() {
    let source = "* +\n```\n\n * q\n";
    let expected =
        "<ul>\n  <li>\n    <pre><code></code></pre>\n  </li>\n  <li><p>q</p></li>\n</ul>";
    assert_eq!(carve::to_html(source), expected);
}
