use carve::to_html;

#[test]
fn braced_comments_do_not_change_quote_direction() {
    for (source, expected) in [
        ("\"a-\"{%%}\n", "<p>“a-”</p>\n"),
        ("'a-'{% comment %}\n", "<p>‘a-’</p>\n"),
        ("\"a-\"{%%}{% more %}\n", "<p>“a-”</p>\n"),
        ("\"a-\"{%%} next\n", "<p>“a-” next</p>\n"),
        ("\"a-\"{%%}, next\n", "<p>“a-”, next</p>\n"),
        ("\"a-\"{%%}\\ b\n", "<p>“a-”&nbsp;b</p>\n"),
        ("\"a-\"{% unclosed\n", "<p>“a-“{% unclosed</p>\n"),
    ] {
        assert_eq!(
            to_html(source).trim_end(),
            expected.trim_end(),
            "{source:?}"
        );
    }
}
