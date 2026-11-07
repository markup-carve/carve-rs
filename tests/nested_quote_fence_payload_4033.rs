use carve::to_html;

#[test]
fn the_nested_attached_fence_has_no_extra_payload_blank() {
    let source = "- a\n  - b\n    > +\n    ```\n    a\n\n    b\n    ```\n";
    let expected = "<ul>\n  <li>a\n    <ul>\n      <li>b\n        <blockquote>\n          <pre><code>a\n\nb\n</code></pre>\n        </blockquote>\n      </li>\n    </ul>\n  </li>\n</ul>";
    assert_eq!(to_html(source).trim(), expected);
}
