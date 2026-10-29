#[test]
fn plain_lines_after_a_list_comment_share_the_retained_paragraph() {
    for (tail, expected_tail) in [(" y", "y"), (" y\n z", "y\nz")] {
        let source = format!("- a\n%%\nx\n{tail}");
        let expected = format!("<ul>\n  <li>a</li>\n</ul>\n<p>x\n{expected_tail}</p>");
        assert_eq!(carve::to_html(&source), expected, "{source}");
    }
}
