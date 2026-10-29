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
        "> :: c\n  # H",
        "<blockquote>\n  <dl>\n    <dt>c</dt>\n  </dl>\n</blockquote>\n<section id=\"H\">\n  <h1>H</h1>\n</section>\n",
    );
    check("```\n```\n- > :: t\n```", "<pre><code></code></pre>\n<ul>\n  <li>\n    <blockquote>\n      <dl>\n        <dt>t\n<code></code></dt>\n      </dl>\n    </blockquote>\n  </li>\n</ul>\n");
}
