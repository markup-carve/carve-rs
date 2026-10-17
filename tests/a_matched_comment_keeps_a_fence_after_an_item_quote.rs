use carve::{to_html, to_html_with_options, Options};

#[test]
fn a_matched_comment_keeps_an_empty_fence_after_a_quoted_item() {
    let source = "- > para\n  > more\n %%%\n c\n %%%\n```\nx\n";
    for html in [
        to_html(source),
        to_html_with_options(source, &Options::default().with_positions(true)),
    ] {
        let code = html
            .find("<pre><code></code></pre>")
            .expect("empty code fence");
        assert!(code < html.find("</li>").unwrap(), "{html}");
        assert!(html.ends_with("<p>x</p>"), "{html}");
    }
}
