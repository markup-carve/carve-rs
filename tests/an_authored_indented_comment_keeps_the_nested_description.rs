use carve::{to_html, to_html_with_options, Options};

#[test]
fn a_rebased_comment_keeps_its_authored_description_owner() {
    let source = "1. :: t\n   : para\n %%%\n c\n %%%\n     tail\n";
    for html in [
        to_html(source),
        to_html_with_options(source, &Options::default().with_positions(true)),
    ] {
        assert!(
            html.contains("<dd>para</dd>\n    </dl>\n  </li>\n</ol>\n<p>tail</p>"),
            "{html}"
        );
    }
}
