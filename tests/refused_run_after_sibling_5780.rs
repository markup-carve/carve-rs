use carve::{to_html, to_html_with_options, Options};

fn check(source: &str, expected: &str) {
    assert_eq!(to_html(source), expected);
    assert_eq!(
        to_html_with_options(source, &Options::default().with_positions(true)),
        expected
    );
}

#[test]
fn a_refused_run_after_a_sibling_keeps_its_follower_and_lazy_line() {
    for (sibling, expected) in [
        (
            "  - q",
            "<ul>\n  <li>p\n    <ul>\n      <li>q</li>\n      <li>{% a\n%}\n        <h1 id=\"h\">h</h1>\n      </li>\n    </ul>\n  </li>\n</ul>\n<p>plain</p>",
        ),
        (
            "  > q",
            "<ul>\n  <li>p\n    <blockquote><p>q</p></blockquote>\n    <ul>\n      <li>{% a\n%}\n        <h1 id=\"h\">h</h1>\n      </li>\n    </ul>\n  </li>\n</ul>\n<p>plain</p>",
        ),
    ] {
        let source = format!("- p\n{sibling}\n  - {{% a\n    %}}\n    # h\nplain\n");
        check(&source, expected);
    }
}
