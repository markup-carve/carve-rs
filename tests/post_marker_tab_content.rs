#[test]
fn a_tab_after_a_list_separator_stays_content() {
    for (source, expected) in [
        ("- \t> a", "<ul>\n  <li>\t&gt; a</li>\n</ul>"),
        ("1. \t> a", "<ol>\n  <li>\t&gt; a</li>\n</ol>"),
        ("- \t# h", "<ul>\n  <li>\t# h</li>\n</ul>"),
        ("- \t  > a", "<ul>\n  <li>\t  &gt; a</li>\n</ul>"),
        ("-  \t> a", "<ul>\n  <li>\t&gt; a</li>\n</ul>"),
    ] {
        assert_eq!(
            carve::to_html(source).trim(),
            expected,
            "source: {source:?}"
        );
    }
}
