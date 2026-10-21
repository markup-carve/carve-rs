//! A blank line INSIDE a fenced code block is verbatim content, not a
//! list-loosening separator. carve-rs's looseness scan
//! (continuation_source_loosens) walked the item's raw lines without tracking
//! fences, so an interior blank in a marker-line fence wrongly loosened the
//! list. A blank AFTER the fence closes still loosens against a following
//! paragraph. Matches carve-js / carve-php.

#[test]
fn interior_fence_blank_does_not_loosen_the_list() {
    // No blank line separates the two items (the blank is inside the fence), so
    // the list is tight: the sibling text is not wrapped in <p>.
    assert_eq!(
        carve::to_html("- ```\n  a\n\n  b\n  ```\n- c\n"),
        "<ul>\n  <li>\n    <pre><code>a\n\nb\n</code></pre>\n  </li>\n  <li>c</li>\n</ul>"
    );
}

#[test]
fn blank_after_the_fence_still_loosens() {
    // A genuine blank line between the code block and the sibling loosens.
    assert_eq!(
        carve::to_html("- ```\n  a\n  ```\n\n- c\n"),
        "<ul>\n  <li>\n    <pre><code>a\n</code></pre>\n  </li>\n  <li><p>c</p></li>\n</ul>"
    );
}

#[test]
fn no_interior_blank_stays_tight() {
    assert_eq!(
        carve::to_html("- ```\n  code\n  ```\n- c\n"),
        "<ul>\n  <li>\n    <pre><code>code\n</code></pre>\n  </li>\n  <li>c</li>\n</ul>"
    );
}

#[test]
fn folded_fence_text_before_a_blank_leaves_two_item_paragraphs() {
    for run in ["```", "~~~"] {
        let before_closer = format!("- h\n  {run}\n\n  b\n{run}\n");
        let after_letter = format!("- o\n  {run}\nx\n\n  t\n");
        let first = carve::to_html(&before_closer);
        let second = carve::to_html(&after_letter);
        assert!(first.contains("<li><p>h"), "{run}: {first}");
        assert!(first.contains("<p>b"), "{run}: {first}");
        assert!(second.contains("<li><p>o"), "{run}: {second}");
        assert!(second.contains("<p>t"), "{run}: {second}");
        if run == "```" {
            assert_eq!(
                second.trim_end(),
                "<ul>\n  <li><p>o\n<code>\nx</code></p>\n    <p>t</p>\n  </li>\n</ul>"
            );
        }
    }
}

#[test]
fn unclosed_folded_fence_without_a_later_closer_still_leaves_two_paragraphs() {
    let html = carve::to_html("- h\n  ```\n\n  b\n");
    assert!(html.contains("<li><p>h"), "{html}");
    assert!(html.contains("<p>b"), "{html}");
    assert_eq!(
        carve::to_html("- h\n  ```\n\n  b\n```\n").trim_end(),
        "<ul>\n  <li><p>h\n<code></code></p>\n    <p>b\n<code></code></p>\n  </li>\n</ul>"
    );
}

#[test]
fn folded_fence_canonical_neighbors_keep_their_ownership() {
    assert_eq!(
        carve::to_html("- o\n  ```\n```\n\n  t\n"),
        "<ul>\n  <li><p>o\n<code>\n</code></p>\n    <p>t</p>\n  </li>\n</ul>"
    );
    assert_eq!(
        carve::to_html("- o\n  - h\n    ```\n```\n"),
        "<ul>\n  <li>o\n    <ul>\n      <li>h\n<code>\n</code></li>\n    </ul>\n  </li>\n</ul>"
    );
}
