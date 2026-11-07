#[test]
fn pulled_description_heading_closes_missed_list_item() {
    let source = "- item\n\n  :: t\n  : +\n  h\n    # H\nz\n";
    let expected = "<ul>\n  <li><p>item</p>\n    <dl>\n      <dt>t</dt>\n      <dd>\n        <p>h</p>\n        <h1 id=\"H\">H</h1>\n      </dd>\n    </dl>\n  </li>\n</ul>\n<p>z</p>";
    assert_eq!(carve::to_html(source), expected);
}
