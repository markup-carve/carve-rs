use carve::{to_html, to_html_with_options, Options};

#[test]
fn a_below_column_comment_keeps_the_colon_body_open() {
    let source = "- ::: note\n  para\n c\n %%%\ntail\n";
    for html in [
        to_html(source),
        to_html_with_options(source, &Options::default().with_positions(true)),
    ] {
        assert!(html.contains("<p>para\nc</p>"), "{html}");
        assert!(html.contains("<p>tail</p>\n    </aside>"), "{html}");
        assert!(!html.contains("</ul>\n<p>tail</p>"), "{html}");
    }
}
