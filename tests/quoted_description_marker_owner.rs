#[test]
fn marker_under_quoted_description_in_item_is_paragraph_text() {
    let source = "- > :: t\n  > : d\n  > - z\n";
    let expected = "<ul>\n  <li>\n    <blockquote>\n      <dl>\n        <dt>t</dt>\n        <dd>d</dd>\n      </dl>\n      <ul>\n        <li>z</li>\n      </ul>\n    </blockquote>\n  </li>\n</ul>";
    assert_eq!(carve::to_html(source), expected);
}
