//! A `%%` line below an item's content column is not a comment inside the
//! item's open fence: the body is verbatim, so §24 C3 has nothing to exempt and
//! the item ends there (markup-carve/carve-rs#2096).
//!
//! Expectations are the executable spec's reading at markup-carve/carve
//! 38829a97. The last case is the control: with no fence open the comment is
//! column-exempt and the item goes on.

#[test]
fn a_comment_below_the_content_column_ends_a_fenced_item() {
    assert_eq!(
        carve::to_html(". ~~~\n%% c\n"),
        "<ol>\n  <li>\n    <pre><code></code></pre>\n  </li>\n</ol>"
    );
}

#[test]
fn and_what_follows_it_parses_at_the_document_level() {
    assert_eq!(
        carve::to_html(". ~~~\n%% c\nz\n"),
        "<ol>\n  <li>\n    <pre><code></code></pre>\n  </li>\n</ol>\n<p>z</p>"
    );
}

#[test]
fn a_closer_below_the_column_does_not_bring_the_item_back() {
    assert_eq!(
        carve::to_html("- a\n\n  ```\n%% c\n  ```\n"),
        "<ul>\n  <li>a\n    <pre><code></code></pre>\n  </li>\n</ul>\n<p><code></code></p>"
    );
}

#[test]
fn payload_then_a_comment_then_a_closer() {
    assert_eq!(
        carve::to_html("- a\n\n  ```\n  p\n%% c\n  q\n  ```\n"),
        "<ul>\n  <li>a\n    <pre><code>p\n</code></pre>\n  </li>\n</ul>\n<p>q\n<code></code></p>"
    );
}

#[test]
fn a_marker_line_fence_over_a_definition() {
    assert_eq!(
        carve::to_html(". ~~~\n[d]: u\n"),
        "<ol>\n  <li>\n    <pre><code></code></pre>\n  </li>\n</ol>"
    );
}

#[test]
fn a_comment_with_no_open_fence_still_keeps_the_item() {
    assert_eq!(
        carve::to_html("- a\n%% c\n  b\n"),
        "<ul>\n  <li>a\n    b\n  </li>\n</ul>"
    );
}
