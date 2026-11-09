//! A blank in an open child fence stays in a description-attached list.
#[test]
fn open_child_fence_keeps_sibling_in_description_attached_list() {
    let source = ":: t\n: +\n- li\n  ```\n\n- sib\n";
    assert_eq!(
        carve::to_html(source),
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <ul>\n      <li><p>li</p>\n        <pre><code>\n</code></pre>\n      </li>\n      <li><p>sib</p></li>\n    </ul>\n  </dd>\n</dl>"
    );
}
