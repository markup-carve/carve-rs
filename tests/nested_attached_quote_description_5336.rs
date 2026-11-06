use carve::to_html;

#[test]
fn marker_column_description_ends_a_nested_item_with_an_attached_unquoted_definition() {
    let source = "- - > +\n    :: t\n    : d\n: d\n";
    let expected = "<ul>\n  <li>\n    <ul>\n      <li>\n        <blockquote>\n          <dl>\n            <dt>t</dt>\n            <dd>d</dd>\n          </dl>\n        </blockquote>\n      </li>\n    </ul>\n  </li>\n</ul>\n<p>: d</p>";
    assert_eq!(to_html(source), expected);
}

#[test]
fn a_quoted_definition_keeps_the_marker_column_line_lazy() {
    let source = "- - > +\n    > :: t\n    > : d\n: d\n";
    let html = to_html(source);
    assert!(html.contains("<dd>d\n: d</dd>"), "{html}");
    assert!(!html.contains("<p>: d</p>"), "{html}");
}
