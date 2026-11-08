//! carve#5499 and #5531: a sibling marker is payload until the fence's first blank.

#[test]
fn sibling_marker_in_nested_attached_fence_is_code() {
    let html = carve::to_html("- - +\n  ```\n  - z\n\nq\n");
    assert!(html.contains("<pre><code>- z\n</code></pre>"), "{html}");
    assert!(!html.contains("<li>z</li>"), "{html}");
}

#[test]
fn sibling_marker_in_top_level_attached_fence_is_code() {
    let html = carve::to_html("- +\n```\n- z\n\nq\n");
    assert!(html.contains("<pre><code>- z\n</code></pre>"), "{html}");
    assert!(!html.contains("<li>z</li>"), "{html}");
}

#[test]
fn first_blank_ends_fence_and_following_item_paragraph_stays_in_item() {
    let html = carve::to_html("- - +\n  ```\n  - z\n\n    q\n");
    assert!(
        html.contains("<pre><code>- z\n</code></pre>\n        <p>q</p>"),
        "{html}"
    );
}

#[test]
fn line_below_the_fence_owner_ends_its_payload() {
    let html = carve::to_html("- - +\n  ```\n- z\n\nq\n");
    assert!(html.contains("<pre><code></code></pre>"), "{html}");
    assert!(html.contains("<li>z</li>"), "{html}");
}
