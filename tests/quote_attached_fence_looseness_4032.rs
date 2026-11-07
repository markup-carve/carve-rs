use carve::to_html;

#[test]
fn a_later_quote_after_the_attached_fence_loosens_the_outer_item() {
    let source = "- a\n  > +\n  ```\n  a\n\n  b\n  ```\n\n  > r\n";
    let expected = "<ul>\n  <li><p>a</p>\n    <blockquote>\n      <pre><code>a\n\nb\n</code></pre>\n    </blockquote>\n    <blockquote><p>r</p></blockquote>\n  </li>\n</ul>";
    assert_eq!(to_html(source).trim(), expected);
}
