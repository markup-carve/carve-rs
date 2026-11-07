#[test]
fn a_blank_before_a_direct_definition_list_loosens_the_parent() {
    let html = carve::to_html("- a\n  * x\n\n   :: t\n");
    assert_eq!(html, "<ul>\n  <li><p>a</p>\n    <ul>\n      <li>x</li>\n    </ul>\n    <dl>\n      <dt>t</dt>\n    </dl>\n  </li>\n</ul>");
}

#[test]
fn a_child_list_without_a_blank_keeps_the_parent_tight() {
    let html = carve::to_html("- a\n  * x\n   :: t\n");
    assert!(html.starts_with("<ul>\n  <li>a\n"), "{html}");
}
