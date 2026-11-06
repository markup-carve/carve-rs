#[test]
fn term_at_nested_item_column_after_blank_stays_in_nested_item() {
    let source = "- - :: t\n\n     a\n    :: v\n";
    let html = carve::to_html(source);
    assert_eq!(html, "<ul>\n  <li>\n    <ul>\n      <li>\n        <dl>\n          <dt>t</dt>\n        </dl>\n        <p>a</p>\n        <dl>\n          <dt>v</dt>\n        </dl>\n      </li>\n    </ul>\n  </li>\n</ul>");
}
