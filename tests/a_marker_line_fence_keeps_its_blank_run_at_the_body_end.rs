//! A fence opened on a list marker line inside a description or note body
//! keeps the blank run that ends the body while it is still open (PART 0 S2,
//! corpus 291). Expected HTML is the spec model's.

use carve::to_html;

fn last_code(source: &str) -> String {
    let html = to_html(source);
    let start = html.rfind("<pre><code>").expect("a code block") + "<pre><code>".len();
    let end = html[start..].find("</code></pre>").expect("closed") + start;
    html[start..end].to_string()
}

#[test]
fn a_reopened_marker_line_fence_keeps_the_blank() {
    // conformance pack fences-9caa332a8a41
    let html = to_html(" :: t\n:  - ```\n     ```\n     ```\n  \n```rust\n");
    assert!(
        html.contains("<pre><code></code></pre>\n        <pre><code>\n</code></pre>"),
        "{html}"
    );
    assert_eq!(
        last_code(":: t\n: - ```\n    ```\n    ```\n    y\n\nz\n"),
        "y\n\n"
    );
    assert!(to_html("> :: t\n> : - ```\n>     ```\n>     ```\n>\nz\n")
        .contains("<pre><code>\n</code></pre>"));
}

#[test]
fn a_closer_right_of_the_fence_column_is_code_text() {
    let html = to_html(":: t\n: - ```\n     ```\n\nz\n");
    assert!(html.contains("<pre><code> ```\n\n</code></pre>"), "{html}");
}

#[test]
fn a_line_left_of_the_item_ends_the_fence_and_opens_its_own() {
    let html = to_html("[^1]: - ```\n      ```\n\nz\n\nsee [^1]\n");
    assert!(
        html.contains("<pre><code></code></pre>\n        </li>\n      </ul>\n      <pre><code>\n</code></pre>"),
        "{html}"
    );
}

#[test]
fn a_nested_description_body_marker_opens_the_fence() {
    let html = to_html(":: t\n: :: u\n  : - ```\n      ```\n      ```\n\nz\n");
    assert!(
        html.contains("<pre><code></code></pre>\n            <pre><code>\n</code></pre>"),
        "{html}"
    );
}
