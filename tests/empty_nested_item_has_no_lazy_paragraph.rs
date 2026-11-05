#[test]
fn an_empty_nested_item_has_no_paragraph_for_a_below_column_line() {
    assert_eq!(
        carve::to_html("- > a\n  - +\nH\n"),
        "<ul>\n  <li>\n    <blockquote><p>a</p></blockquote>\n    <ul>\n      <li></li>\n    </ul>\n  </li>\n</ul>\n<p>H</p>"
    );
}

#[test]
fn a_nested_item_with_an_attached_paragraph_still_takes_lazy_text() {
    assert!(carve::to_html("- > a\n  - +\n  p\nH\n").contains("<li>p\nH</li>"));
}
