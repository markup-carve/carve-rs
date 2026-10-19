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
    check("> 1. :: t\n   : para", "<blockquote>\n  <ol>\n    <li>\n      <dl>\n        <dt>t\n: para</dt>\n      </dl>\n    </li>\n  </ol>\n</blockquote>\n");
    check("> - :: term\n  : definition", "<blockquote>\n  <ul>\n    <li>\n      <dl>\n        <dt>term\n: definition</dt>\n      </dl>\n    </li>\n  </ul>\n</blockquote>\n");
    check("> - :: t\n: d", "<blockquote>\n  <ul>\n    <li>\n      <dl>\n        <dt>t</dt>\n      </dl>\n    </li>\n  </ul>\n</blockquote>\n<p>: d</p>");
}
