use carve::{to_html, to_html_with_options, Options};

#[test]
fn a_matched_comment_at_the_nested_opener_ends_the_description() {
    let source = ":: t\n: :: t\n  : para\n    more\n  %%%\n  c\n  %%%\n    tail\n";
    for html in [
        to_html(source),
        to_html_with_options(source, &Options::default().with_positions(true)),
    ] {
        assert!(html.contains("</dl>\n    <p>tail</p>"), "{html}");
    }
}
