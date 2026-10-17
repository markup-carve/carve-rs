use carve::{to_html, to_html_with_options, Options};

#[test]
fn a_code_fence_after_a_matched_comment_opens_in_the_description() {
    for source in [
        ":: t\n: para\n %%%\n c\n %%%\n```\n",
        ":: t\n: ::: note\n  para\n %%%\n c\n %%%\n```\n",
    ] {
        for html in [
            to_html(source),
            to_html_with_options(source, &Options::default().with_positions(true)),
        ] {
            assert!(
                html.contains("<pre><code></code></pre>"),
                "{source:?}: {html}"
            );
            assert!(!html.contains("<p><code></code></p>"), "{source:?}: {html}");
        }
    }
}
