#[test]
fn tab_after_list_marker_keeps_following_constructs_literal() {
    for (source, expected) in [
        ("- \t- child", "<ul>\n  <li>- child</li>\n</ul>"),
        ("1. \t2. child", "<ol>\n  <li>2. child</li>\n</ol>"),
        ("- \t# heading", "<ul>\n  <li># heading</li>\n</ul>"),
        ("- \t> quote", "<ul>\n  <li>&gt; quote</li>\n</ul>"),
        ("- \t::: note", "<ul>\n  <li>::: note</li>\n</ul>"),
    ] {
        assert_eq!(
            carve::to_html(source).trim(),
            expected,
            "source: {source:?}"
        );
    }
}
