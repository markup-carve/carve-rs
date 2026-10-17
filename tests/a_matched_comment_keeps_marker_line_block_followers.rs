use carve::{to_html, to_html_with_options, Options};

#[test]
fn a_matched_comment_keeps_a_fence_in_a_marker_line_description() {
    let source = "1. :: t\n   : para\n  %%%\n  c\n  %%%\n```\nx\n";
    for html in [
        to_html(source),
        to_html_with_options(source, &Options::default().with_positions(true)),
    ] {
        let code = html
            .find("<pre><code></code></pre>")
            .expect("empty code fence");
        assert!(code < html.find("</dd>").unwrap(), "{html}");
        assert!(html.ends_with("<p>x</p>"), "{html}");
    }
}

#[test]
fn a_matched_comment_keeps_a_lazy_tail_in_a_marker_line_colon_body() {
    let source = "1. ::: note\n   para\n   more\n %%%\n c\n %%%\ntail\n";
    for html in [
        to_html(source),
        to_html_with_options(source, &Options::default().with_positions(true)),
    ] {
        assert!(html.contains("<p>tail</p>\n    </aside>"), "{html}");
    }
}
