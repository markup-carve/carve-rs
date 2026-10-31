#[test]
fn tab_after_list_marker_keeps_following_constructs_literal() {
    for (source, expected) in [
        ("- \t- child", "<ul>\n  <li>\t- child</li>\n</ul>"),
        ("1. \t2. child", "<ol>\n  <li>\t2. child</li>\n</ol>"),
        ("- \t# heading", "<ul>\n  <li>\t# heading</li>\n</ul>"),
        ("- \t> quote", "<ul>\n  <li>\t&gt; quote</li>\n</ul>"),
        ("- \t::: note", "<ul>\n  <li>\t::: note</li>\n</ul>"),
    ] {
        assert_eq!(
            carve::to_html(source).trim(),
            expected,
            "source: {source:?}"
        );
    }
}
