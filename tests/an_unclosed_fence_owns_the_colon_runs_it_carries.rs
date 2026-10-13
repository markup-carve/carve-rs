//! A fence that leaves a closed nested item opens in the box it lands in and,
//! unclosed, runs to the box's end: a colon run in its body stays code instead
//! of closing the box (CARVE-P0-014).

fn compact(source: &str) -> String {
    carve::to_html(source).lines().map(str::trim).collect()
}

#[test]
fn a_colon_run_in_the_fence_body_stays_code() {
    let html =
        compact("- ::: box\n  - ::: |\n    ```\n    :::\n   ```\n   :::bad\n   :::\n- tail\n");
    assert!(
        html.contains("<pre><code>:::bad:::</code></pre></div>"),
        "{html}"
    );
}

#[test]
fn a_fence_below_a_closed_note_still_lets_the_box_close() {
    let html =
        compact("- ::: box\n  - ::: note\n    y\n    :::\n   ```\n   :::bad\n   :::\n- tail\n");
    assert!(
        html.contains("<pre><code>:::bad</code></pre></div>"),
        "{html}"
    );
}

#[test]
fn a_run_at_the_childs_column_does_not_close_a_fence_that_left_it() {
    let html = compact("- a\n    - child\n    ```\n\n      ```\nafter\n");
    assert!(
        html.contains("<li>child<code></code><pre><code></code></pre></li>"),
        "{html}"
    );
}
