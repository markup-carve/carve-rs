use carve::{to_html, to_html_with_options, Options};

#[test]
fn a_closed_comment_at_the_description_column_keeps_its_follower() {
    let source = "1. :: t\n   : para\n  %%%\n  %%%\ntail\n";
    for html in [
        to_html(source),
        to_html_with_options(source, &Options::default().with_positions(true)),
    ] {
        assert!(html.contains("</ol>\n<p>tail</p>"), "{html}");
    }
}
