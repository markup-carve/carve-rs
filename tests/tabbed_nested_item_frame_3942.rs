use carve::to_html;

#[test]
fn tab_after_nested_item_prefix_reaches_its_content_frame() {
    let source = "- a\n   - a\n   \t:::\n";
    assert_eq!(
        to_html(source),
        "<ul>\n  <li>a\n    <ul>\n      <li>a\n        <div>\n\n        </div>\n      </li>\n    </ul>\n  </li>\n</ul>"
    );
}
