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

// Rule A (markup-carve/carve#3885): the comment below the item retains nothing,
// so the closerless fence opens at the document and runs to its end.
#[test]
fn exact_html() {
    check(
        "- para\n%% c\n```\n- a\n  [^f]: t",
        "<ul>\n  <li>para</li>\n</ul>\n<pre><code>- a\n  [^f]: t</code></pre>\n",
    );
    check(
        "- a\n  [^f]: t\n[^f]: a\n     [r]: /url\n[r][]",
        "<ul>\n  <li>a</li>\n</ul>\n",
    );
}
