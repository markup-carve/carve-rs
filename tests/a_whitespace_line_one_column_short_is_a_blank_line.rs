// carve-rs#3845: a whitespace-only line is a blank line at any width (PART 4
// `blank_line`), so one column short of the content column it still separates
// siblings instead of ending the list after a block that leaves no paragraph.

use carve::{to_html, to_html_with_options, Options};

fn check(source: &str, expected: &str) {
    assert_eq!(to_html(source), expected);
    let positioned = to_html_with_options(source, &Options::default().with_positions(true));
    assert_eq!(positioned, expected);
}

#[test]
fn after_a_closed_fence() {
    check(
        "- a\n  ```\n  ```\n \n- s\n",
        "<ul>\n  <li><p>a</p>\n    <pre><code></code></pre>\n  </li>\n  <li><p>s</p></li>\n</ul>",
    );
}

#[test]
fn after_an_attribute_line() {
    check(
        "- a\n  {.x}\n \n- s\n",
        "<ul>\n  <li><p>a</p></li>\n  <li><p>s</p></li>\n</ul>",
    );
}

#[test]
fn at_depth_two() {
    check(
        "- a\n  - b\n    ```\n    ```\n   \n  - c\n",
        "<ul>\n  <li>a\n    <ul>\n      <li><p>b</p>\n        <pre><code></code></pre>\n      </li>\n      <li><p>c</p></li>\n    </ul>\n  </li>\n</ul>",
    );
}

#[test]
fn inside_a_quote() {
    check(
        "> - a\n>   ```\n>   ```\n>  \n> - s\n",
        "<blockquote>\n  <ul>\n    <li><p>a</p>\n      <pre><code></code></pre>\n    </li>\n    <li><p>s</p></li>\n  </ul>\n</blockquote>",
    );
}

#[test]
fn after_a_plus_attached_fence() {
    check(
        "- a\n+\n```\n```\n \n- s\n",
        "<ul>\n  <li><p>a</p>\n    <pre><code></code></pre>\n  </li>\n  <li><p>s</p></li>\n</ul>",
    );
}
