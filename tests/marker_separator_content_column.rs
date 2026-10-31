#[test]
fn a_tab_after_a_list_separator_stays_content_without_moving_the_column() {
    let source = "- \tx\n  # H\n";
    assert_eq!(
        carve::to_html(source).trim_end(),
        "<ul>\n  <li>\tx\n    <h1 id=\"H\">H</h1>\n  </li>\n</ul>"
    );
}
