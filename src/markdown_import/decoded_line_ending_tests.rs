#[test]
fn decoded_line_endings_cannot_open_blocks() {
    for (source, expected) in [
        ("foo&#10;&#10;bar", "<p>foo  bar</p>"),
        ("&#10;# b", "<p># b</p>"),
        ("a&#13;# b", "<p>a # b</p>"),
        ("a&NewLine;- b", "<p>a - b</p>"),
        ("a\nb", "<p>a\nb</p>"),
        ("`a&#10;b`", "<p><code>a&amp;#10;b</code></p>"),
        ("```\na\n\nb\n```", "<pre><code>a\n\nb\n</code></pre>"),
    ] {
        assert_eq!(
            crate::to_html(&super::markdown_to_carve(source)),
            expected,
            "{source}"
        );
    }
}
