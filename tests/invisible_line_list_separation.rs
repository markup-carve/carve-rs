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
    check("- - a\n\n  %% just a note\n- b", "<ul>\n  <li>\n    <ul>\n      <li>a</li>\n    </ul>\n  </li>\n  <li><p>b</p></li>\n</ul>\n");
    check("- - body\n\n  [^g]: n\n   prose", "<ul>\n  <li>\n    <ul>\n      <li>body</li>\n    </ul>\n    <p>prose</p>\n  </li>\n</ul>\n");
    check(
        "- :: t\n:  ",
        "<ul>\n  <li>\n    <dl>\n      <dt>t\n:</dt>\n    </dl>\n  </li>\n</ul>\n",
    );
    check("- - ```\n    x\n\n[^1]: a", "<ul>\n  <li>\n    <ul>\n      <li>\n        <pre><code>x\n\n</code></pre>\n      </li>\n    </ul>\n  </li>\n</ul>\n");
}
