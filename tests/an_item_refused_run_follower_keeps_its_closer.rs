// CARVE-P2-035: in a list item, a fence-shaped follower of a refused brace run
// keeps its closer in the paragraph (carve-rs#4148).

use carve::{to_html, to_html_with_options, to_json_with_options, Options};
use serde_json::Value;

fn check(source: &str, expected: &str) {
    assert_eq!(to_html(source), expected);
    let positioned = to_html_with_options(source, &Options::default().with_positions(true));
    assert_eq!(positioned, expected);
}

#[test]
fn a_marker_line_run_follower_keeps_its_next_line_closer() {
    check(
        "- {#i\n  .y}\n  ```\n  ```\n",
        "<ul>\n  <li>{<span class=\"tag\"><strong>#i</strong></span>\n.y}\n    <pre><code></code></pre>\n  </li>\n</ul>",
    );
}

#[test]
fn the_lines_before_the_closer_are_paragraph_text() {
    check(
        "- {#i\n  .y}\n  ```\n  - x\n  ```\n",
        "<ul>\n  <li>{<span class=\"tag\"><strong>#i</strong></span>\n.y}\n    <pre><code>- x\n</code></pre>\n  </li>\n</ul>",
    );
}

#[test]
fn a_nested_item_run_keeps_its_follower() {
    check(
        "- p\n  - {#i\n    .y}\n    ```\n    x\n",
        "<ul>\n  <li>p\n    <ul>\n      <li>{<span class=\"tag\"><strong>#i</strong></span>\n.y}\n        <pre><code>x\n</code></pre>\n      </li>\n    </ul>\n  </li>\n</ul>",
    );
}

#[test]
fn a_nested_item_run_keeps_a_flush_lazy_payload() {
    check(
        "- p\n  - {.x\n   }\n    ```\nplain\n",
        "<ul>\n  <li>p\n    <ul>\n      <li>{.x\n}\n        <pre><code></code></pre>\n      </li>\n    </ul>\n  </li>\n</ul>\n<p>plain</p>",
    );
}

#[test]
fn a_deeper_nested_run_keeps_a_marker_column_closer_and_lazy_payload() {
    check(
        "- p0\n  - p1\n    - {.x\n    }\n      ```\nplain\n",
        "<ul>\n  <li>p0\n    <ul>\n      <li>p1\n        <ul>\n          <li>{.x\n}\n            <pre><code></code></pre>\n          </li>\n        </ul>\n      </li>\n    </ul>\n  </li>\n</ul>\n<p>plain</p>",
    );
}

#[test]
fn a_rebased_refused_follower_keeps_its_authored_column() {
    let source = "- p\n  - {.x\n   }\n      ```\nplain\n";
    let json = to_json_with_options(source, &Options::default().with_positions(true));
    let ast: Value = serde_json::from_str(&json).unwrap();
    fn code_position(node: &Value) -> Option<&Value> {
        if node["type"] == "code_block" {
            return node.get("pos");
        }
        node.get("children")
            .and_then(Value::as_array)
            .and_then(|children| children.iter().find_map(code_position))
            .or_else(|| {
                node.get("items")
                    .and_then(Value::as_array)
                    .and_then(|items| items.iter().find_map(code_position))
            })
    }
    let code = code_position(&ast).expect("fenced code block");
    assert_eq!(code["startLine"], 4);
    assert_eq!(code["startColumn"], 7);
}

#[test]
fn a_closer_past_an_unclosed_follower_is_text() {
    check(
        "- a\n  {#i\n  .y}\n  ~~~\n  x\n   ~~~\n",
        "<ul>\n  <li>a\n{<span class=\"tag\"><strong>#i</strong></span>\n.y}\n    <pre><code>x\n ~~~\n</code></pre>\n  </li>\n</ul>",
    );
}

#[test]
fn a_quoted_ordered_item_keeps_the_paragraph_after_the_closer() {
    check(
        "> 1. {#i\n>    .y}\n>    ```\n>    x\n>    ```\n>    after\n",
        "<blockquote>\n  <ol>\n    <li>{<span class=\"tag\"><strong>#i</strong></span>\n.y}\n      <pre><code>x\n</code></pre>\n      after\n    </li>\n  </ol>\n</blockquote>",
    );
}
