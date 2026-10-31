#[test]
fn a_tab_after_a_list_separator_stays_content_without_moving_the_column() {
    let source = "- \tx\n  # H\n";
    assert_eq!(
        carve::to_html(source).trim_end(),
        "<ul>\n  <li>\tx\n    <h1 id=\"H\">H</h1>\n  </li>\n</ul>"
    );
}

#[test]
fn a_task_checkbox_does_not_shrink_a_wide_marker_separator() {
    let source = "-   [ ] q\n  # H\n";
    assert_eq!(
        carve::to_html(source).trim_end(),
        "<ul class=\"task-list\">\n  <li><input type=\"checkbox\" disabled aria-label=\"q\"> q</li>\n</ul>\n<section id=\"H\">\n  <h1>H</h1>\n</section>"
    );
}
