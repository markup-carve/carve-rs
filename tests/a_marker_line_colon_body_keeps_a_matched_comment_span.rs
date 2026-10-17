use carve::{to_html, to_html_with_options, Options};

#[test]
fn a_matched_comment_span_keeps_its_payload_inside_the_marker_line_colon_body() {
    for source in [
        "- ::: note\n  para\n %%%\n c\n %%%\n",
        "1. ::: note\n   more\n %%%\n c\n %%%\n",
    ] {
        for html in [
            to_html(source),
            to_html_with_options(source, &Options::default().with_positions(true)),
        ] {
            assert!(
                html.contains("<aside class=\"admonition note\""),
                "{source:?}: {html}"
            );
            assert!(!html.contains("<p>c</p>"), "{source:?}: {html}");
            assert!(!html.contains("&gt;"), "{source:?}: {html}");
        }
    }
}
