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
        "- para\n%% c\n```\n- a\n  [^f]: t",
        "<ul>\n  <li>para\n    <pre><code></code></pre>\n  </li>\n  <li>a</li>\n</ul>\n",
    );
    check(
        "- a\n  [^f]: t\n[^f]: a\n     [r]: /url\n[r][]",
        "<ul>\n  <li>a</li>\n</ul>\n<p><a href=\"/url\">r</a></p>\n",
    );
}
