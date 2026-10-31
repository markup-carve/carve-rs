// carve#4559: removing a comment must not change the output, so a caret line
// whose content is only comments opens no caption.
#[test]
fn a_comment_only_caret_line_is_paragraph_text() {
    let cases = [
        (
            "| a |\n^ %% x\n",
            "<table>\n  <tbody>\n    <tr><td>a</td></tr>\n  </tbody>\n</table>\n<p>^</p>\n",
        ),
        ("> q\n^ %% x\n", "<blockquote><p>q\n^</p></blockquote>\n"),
        ("> q\n\n^ %% x\n", "<blockquote><p>q</p></blockquote>\n<p>^</p>\n"),
        (
            "| a |\n^ {% x %}\n",
            "<table>\n  <tbody>\n    <tr><td>a</td></tr>\n  </tbody>\n</table>\n<p>^ </p>\n",
        ),
        (
            "| a |\n^ {% x %} %% y\n",
            "<table>\n  <tbody>\n    <tr><td>a</td></tr>\n  </tbody>\n</table>\n<p>^ </p>\n",
        ),
        ("![a](b)\n^ {% x %}\n", "<p><img src=\"b\" alt=\"a\">\n^ </p>\n"),
    ];
    for (source, expected) in cases {
        assert_eq!(carve::to_html(source).trim_end(), expected.trim_end(), "{source:?}");
    }
}

#[test]
fn a_comment_followed_by_text_still_captions() {
    for source in ["| a |\n^ {% x %}y\n", "| a |\n^ {% x %}y %% z\n"] {
        let html = carve::to_html(source);
        assert!(html.contains("<caption>y</caption>"), "{source:?}\n{html}");
    }
    // `%%` needs a space before it to open a comment.
    let html = carve::to_html("| a |\n^ {%x%}%% y\n");
    assert!(html.contains("<caption>%% y</caption>"), "{html}");
}
