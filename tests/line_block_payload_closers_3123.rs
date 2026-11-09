use carve::{to_html, to_html_with_options, Options};

fn check(source: &str, expected: &str) {
    assert_eq!(to_html(source), expected);
    assert_eq!(
        to_html_with_options(source, &Options::default().with_positions(true)),
        expected
    );
}

#[test]
fn code_fence_spelling_inside_a_line_block_cannot_hide_its_closer() {
    check(
        "::: note\n::: |\n```\n:::\n:::\n",
        "<aside class=\"admonition note\" aria-label=\"Note\">\n  <div class=\"line-block\">\n    <p><code></code></p>\n  </div>\n</aside>",
    );
}

#[test]
fn longer_colon_spelling_inside_a_line_block_cannot_hide_its_closer() {
    check(
        "::: note\n::: |\na\nb\n:::::: note\na\n:::\n:::\n",
        "<aside class=\"admonition note\" aria-label=\"Note\">\n  <div class=\"line-block\">\n    <p>a<br>\nb<br>\n:::::: note<br>\na</p>\n  </div>\n</aside>",
    );
}
