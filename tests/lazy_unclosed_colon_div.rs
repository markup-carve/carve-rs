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
    check("- - ::: |\n  :::\n  para\ntail", "<ul>\n  <li>\n    <ul>\n      <li>\n        <div class=\"line-block\">\n\n        </div>\n      </li>\n    </ul>\n    <div>\n      <p>para\ntail</p>\n    </div>\n  </li>\n</ul>\n");
}
