#[test]
fn a_later_stanza_releases_a_shifted_items_definition_boundary() {
    let source = " - a\n   :: t\n   ::: |\n     prose\n : z\n";
    let expected = "<ul>\n  <li>a\n    <dl>\n      <dt>t</dt>\n    </dl>\n    <div class=\"line-block\">\n      <p>&nbsp;&nbsp;prose<br>\n: z</p>\n    </div>\n  </li>\n</ul>";
    assert_eq!(carve::to_html(source), expected);
}

#[test]
fn a_stanza_without_a_definition_keeps_its_lazy_description() {
    let source = " - a\n   ::: |\n     prose\n : z\n";
    let html = carve::to_html(source);
    assert!(html.contains("prose<br>\n: z</p>"), "{html}");
}
