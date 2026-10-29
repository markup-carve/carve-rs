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
    check("1. :: t\n   : para\n%% c\n```\n```", "<ol>\n  <li>\n    <dl>\n      <dt>t</dt>\n      <dd>para</dd>\n    </dl>\n  </li>\n</ol>\n<pre><code></code></pre>\n");
    check(
        "- - item\n %%%\n  %%%\ntail",
        "<ul>\n  <li>\n    <ul>\n      <li>item</li>\n    </ul>\n  </li>\n</ul>\n<p>tail</p>\n",
    );
}
