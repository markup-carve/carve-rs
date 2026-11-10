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
    for (sibling, preceding) in [
        ("  - q", "    <ul>\n      <li>q</li>\n      <li>"),
        (
            "  > q",
            "    <blockquote><p>q</p></blockquote>\n    <ul>\n      <li>",
        ),
    ] {
        let source = format!("- p\n{sibling}\n  - {{% a\n    %}}\n    # h\nplain\n");
        let expected = format!(
            "<ul>\n  <li>p\n{preceding}{{% a\n%}}\n# h\nplain</li>\n    </ul>\n  </li>\n</ul>"
        );
        check(&source, &expected);
    }
}
