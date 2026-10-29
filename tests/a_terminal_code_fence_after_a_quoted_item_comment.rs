use carve::{to_html, to_html_with_options, Options};

#[test]
fn a_terminal_code_fence_stays_in_the_comment_retained_quoted_item() {
    let source = "- > para\n  > more\n %%%\n c\n %%%\n```\n";
    for html in [
        to_html(source),
        to_html_with_options(source, &Options::default().with_positions(true)),
    ] {
        assert!(
            !html.contains("<pre><code></code></pre>\n  </li>"),
            "{html}"
        );
        assert!(html.contains("</ul>\n<pre><code></code></pre>"), "{html}");
    }
}
