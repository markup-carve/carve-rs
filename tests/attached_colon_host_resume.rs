#[test]
fn nested_item_resumes_after_closed_attached_colon_with_blank() {
    let source = "- a\n  - +\n  ::: w\n  x\n\n  y\n  :::\n\n    # g\n";
    let expected = "<ul>\n  <li>a\n    <ul>\n      <li>\n        <div class=\"w\">\n          <p>x</p>\n          <p>y</p>\n        </div>\n        <h1 id=\"g\">g</h1>\n      </li>\n    </ul>\n  </li>\n</ul>";
    assert_eq!(carve::to_html(source), expected);
}
