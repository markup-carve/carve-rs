#[test]
fn invisible_line_comments_do_not_fill_a_plus_attachment() {
    for comments in ["%% hidden", "%% first\n%% second"] {
        let source = format!("- a\n+\n{comments}\n> q");
        let expected = "<ul>\n  <li>a\n    <blockquote><p>q</p></blockquote>\n  </li>\n</ul>";
        assert_eq!(carve::to_html(&source), expected, "{source}");
    }
}

#[test]
fn invisible_comment_before_a_quote_attachment_keeps_the_visible_block() {
    let source = "> a\n+\n%% hidden\n> q";
    let expected = "<blockquote>\n  <p>a</p>\n  <blockquote><p>q</p></blockquote>\n</blockquote>";
    assert_eq!(carve::to_html(source), expected);
}

#[test]
fn invisible_comment_before_a_heading_attachment_keeps_the_visible_block() {
    let source = "- a\n+\n%% hidden\n# h";
    let expected = "<ul>\n  <li>a\n    <h1 id=\"h\">h</h1>\n  </li>\n</ul>";
    assert_eq!(carve::to_html(source), expected);
}
