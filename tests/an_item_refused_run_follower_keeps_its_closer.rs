// CARVE-P2-035: in a list item, a fence-shaped follower of a refused brace run
// keeps its closer in the paragraph (carve-rs#4148).

use carve::{to_html, to_html_with_options, Options};

fn check(source: &str, expected: &str) {
    assert_eq!(to_html(source), expected);
    let positioned = to_html_with_options(source, &Options::default().with_positions(true));
    assert_eq!(positioned, expected);
}

#[test]
fn a_marker_line_run_follower_keeps_its_next_line_closer() {
    check(
        "- {#i\n  .y}\n  ```\n  ```\n",
        "<ul>\n  <li>{#i\n.y}\n<code>\n</code></li>\n</ul>",
    );
}

#[test]
fn the_lines_before_the_closer_are_paragraph_text() {
    check(
        "- {#i\n  .y}\n  ```\n  - x\n  ```\n",
        "<ul>\n  <li>{#i\n.y}\n<code>\n- x\n</code></li>\n</ul>",
    );
}

#[test]
fn a_nested_item_run_keeps_its_follower() {
    check(
        "- p\n  - {#i\n    .y}\n    ```\n    x\n",
        "<ul>\n  <li>p\n    <ul>\n      <li>{#i\n.y}\n<code>\nx</code></li>\n    </ul>\n  </li>\n</ul>",
    );
}

#[test]
fn a_closer_past_an_unclosed_follower_is_text() {
    check(
        "- a\n  {#i\n  .y}\n  ~~~\n  x\n   ~~~\n",
        "<ul>\n  <li>a\n{#i\n.y}\n~~~\nx\n~~~</li>\n</ul>",
    );
}

#[test]
fn a_quoted_ordered_item_keeps_the_paragraph_after_the_closer() {
    check(
        "> 1. {#i\n>    .y}\n>    ```\n>    x\n>    ```\n>    after\n",
        "<blockquote>\n  <ol>\n    <li>{#i\n.y}\n<code>\nx\n</code>\nafter</li>\n  </ol>\n</blockquote>",
    );
}
