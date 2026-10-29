use carve::{to_html, to_html_with_options, Options};

#[test]
fn a_matched_comment_keeps_the_following_fence_in_the_footnote_item() {
    let source = "[^c]: - para\n        more\n %%%\n c\n %%%\n```\nx\nsee [^c]\n";
    for html in [
        to_html(source),
        to_html_with_options(source, &Options::default().with_positions(true)),
    ] {
        let code = html
            .find("<pre><code>x\nsee [^c]\n</code></pre>")
            .expect("code fence");
        assert_eq!(code, 0, "{html}");
        assert!(!html.contains("</ul>"), "{html}");
        assert!(!html.contains("<p>c</p>"), "{html}");
    }
}
