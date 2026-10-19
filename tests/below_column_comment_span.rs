use carve::{to_html, to_html_with_options, Options};

fn check(source: &str, expected: &str) {
    for html in [
        to_html(source),
        to_html_with_options(source, &Options::default().with_positions(true)),
    ] {
        assert_eq!(
            html.trim_end_matches('\n'),
            expected.trim_end_matches('\n'),
            "{source:?}"
        );
    }
}

#[test]
fn exact_html() {
    check(
        "- x\n    %%%\n y\n   %%%",
        "<ul>\n  <li>x</li>\n</ul>\n<p>y</p>\n",
    );
    check(
        "- %%%\n%%%\n:  x\n   %%%",
        "<ul>\n  <li></li>\n</ul>\n<p>:  x</p>\n",
    );
}
