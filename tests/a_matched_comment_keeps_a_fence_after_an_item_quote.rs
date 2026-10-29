use carve::{to_html, to_html_with_options, Options};

#[test]
fn a_matched_comment_keeps_an_empty_fence_after_a_quoted_item() {
    let source = "- > para\n  > more\n %%%\n c\n %%%\n```\nx\n";
    for html in [
        to_html(source),
        to_html_with_options(source, &Options::default().with_positions(true)),
    ] {
        let code = html.find("x\n</code></pre>").expect("code fence");
        assert!(code > html.find("</li>").unwrap(), "{html}");
        assert!(!html.contains("<p>x</p>"), "{html}");
    }
}
