//! A fence that leaves a closed nested item opens in the box it lands in and,
//! unclosed, runs to the box's end: a colon run in its body stays code instead
//! of closing the box (CARVE-P0-014). Only a run on the box's own column
//! closes it.

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
fn a_colon_run_past_the_box_column_below_a_closed_note_stays_code() {
    let html =
        compact("- ::: box\n  - ::: note\n    y\n    :::\n   ```\n   :::bad\n   :::\n- tail\n");
    assert!(
        html.contains("<pre><code>:::bad:::</code></pre></div>"),
        "{html}"
    );
}

// A line-block payload line opens no fence, so `x` reads like the fence-shaped
// payload above (corpus 581 rows 8 and 9).
#[test]
fn a_plain_line_block_payload_keeps_the_colon_runs_code() {
    let html = compact("- ::: box\n  - ::: |\n    x\n    :::\n   ```\n   :::bad\n   :::\n- tail\n");
    assert!(
        html.contains("<pre><code>:::bad:::</code></pre></div>"),
        "{html}"
    );
    let html = compact(
        "- ::: box\n  -   ::: |\n      x\n      :::\n    ```\n    :::\n    :::bad\n    :::\n- tail\n",
    );
    assert!(
        html.contains("<pre><code>::::::bad:::</code></pre></div></li>"),
        "{html}"
    );
}

#[test]
fn a_colon_run_past_the_box_column_stays_code_with_no_line_block() {
    let html = compact("- ::: box\n   ```\n   :::bad\n   :::\n- tail\n");
    assert!(
        html.contains("<div class=\"box\"><pre><code>:::bad:::</code></pre></div>"),
        "{html}"
    );
}

#[test]
fn a_colon_run_on_the_box_column_still_closes_the_box() {
    let html = compact("- ::: box\n   ```\n   :::\n  :::\nafter\n");
    assert!(
        html.contains("<pre><code>::::::</code></pre></div></li></ul><p>after</p>"),
        "{html}"
    );
}

#[test]
fn a_run_at_the_childs_column_does_not_close_a_fence_that_left_it() {
    let html = compact("- a\n    - child\n    ```\n\n      ```\nafter\n");
    assert!(
        html.contains("<li>child</li></ul><pre><code>```</code></pre></li>"),
        "{html}"
    );
}
