use carve::{to_html, to_html_with_options, Options};

#[test]
fn a_comment_retains_one_footnote_follower_without_a_lazy_extent() {
    for source in [
        "[^c]: para\n%%%\nc\ntail\n",
        "[^c]: ::: note\n      para\n%%%\nc\ntail\n",
        "[^c]: - para\n%%%\nc\ntail\n",
    ] {
        for html in [
            to_html(source),
            to_html_with_options(source, &Options::default().with_positions(true)),
        ] {
            assert_eq!(html.trim(), "<p>tail</p>", "{source:?}");
        }
    }
}
