// carve-rs#3844: pending attributes survive blank lines and attach to the next
// block (PART 9 §15 A2 FLOAT FORWARD), a nested list in the same item included.

use carve::{to_html, to_html_with_options, Options};

fn check(source: &str, expected: &str) {
    assert_eq!(to_html(source), expected);
    let positioned = to_html_with_options(source, &Options::default().with_positions(true));
    assert_eq!(positioned, expected);
}

#[test]
fn a_class_reaches_a_bullet_sub_list() {
    check(
        "- a\n  {.x}\n\n  - b\n",
        "<ul>\n  <li><p>a</p>\n    <ul class=\"x\">\n      <li>b</li>\n    </ul>\n  </li>\n</ul>",
    );
}

#[test]
fn a_class_reaches_an_ordered_sub_list() {
    check(
        "- a\n  {.x}\n\n  1. b\n",
        "<ul>\n  <li><p>a</p>\n    <ol class=\"x\">\n      <li>b</li>\n    </ol>\n  </li>\n</ul>",
    );
}

#[test]
fn inside_a_quote() {
    check(
        "> - a\n>   {.x}\n>\n>   - b\n",
        "<blockquote>\n  <ul>\n    <li><p>a</p>\n      <ul class=\"x\">\n        <li>b</li>\n      </ul>\n    </li>\n  </ul>\n</blockquote>",
    );
}

#[test]
fn across_blanks_on_both_sides() {
    check(
        "- a\n\n  {#i}\n\n\n  - b\n",
        "<ul>\n  <li><p>a</p>\n    <ul id=\"i\">\n      <li>b</li>\n    </ul>\n  </li>\n</ul>",
    );
}
