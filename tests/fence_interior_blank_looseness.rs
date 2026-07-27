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
