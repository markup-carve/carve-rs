use carve::{to_html, to_html_with_options, Options};

#[test]
fn a_matched_comment_keeps_the_following_fence_in_the_footnote_item() {
    let source = "[^c]: - para\n        more\n %%%\n c\n %%%\n```\nx\nsee [^c]\n";
    for html in [
        to_html(source),
        to_html_with_options(source, &Options::default().with_positions(true)),
    ] {
        let code = html
            .find("<pre><code></code></pre>")
            .expect("empty code fence");
        assert!(code < html.find("</ul>").unwrap(), "{html}");
        assert!(!html.contains("<p>c</p>"), "{html}");
    }
}
