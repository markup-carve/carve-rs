//! A fence line absorbed as item paragraph text opens no block. Under the
//! current spec, a later blank can separate a second item paragraph and make
//! the item loose (carve#3495). A blank before the next sibling also loosens.
//!
//! Every row follows the current executable reference.

/// Record a row that did not render as expected, so one run names every row
/// that moved instead of stopping at the first.
fn row(source: &str, expected: &str, moved: &mut Vec<String>) {
    let actual = carve::to_html(source);
    let actual = actual.trim_end();
    if actual != expected {
        moved.push(format!(
            "{source:?}\n  expected: {expected}\n  actual:   {actual}"
        ));
    }
}

#[test]
fn a_blank_after_folded_fence_text_leaves_two_loose_paragraphs() {
    let mut moved = Vec::new();
    row(
        "- a\n  ```\n  b\n\n  z\n",
        "<ul>\n  <li>a\n    <pre><code>b\n\nz\n</code></pre>\n  </li>\n</ul>",
        &mut moved,
    );
    row(
        "- a\n  ```\n  b\n\n  z\n- c\n",
        "<ul>\n  <li>a\n    <pre><code>b\n\nz\n</code></pre>\n  </li>\n  <li>c</li>\n</ul>",
        &mut moved,
    );
    row(
        "1. a\n   ```\n   b\n\n   z\n",
        "<ol>\n  <li>a\n    <pre><code>b\n\nz\n</code></pre>\n  </li>\n</ol>",
        &mut moved,
    );
    row(
        "- a\n  ~~~\n  b\n\n  z\n\n  w\n",
        "<ul>\n  <li>a\n    <pre><code>b\n\nz\n\nw\n</code></pre>\n  </li>\n</ul>",
        &mut moved,
    );
    row(
        "- a\n  p\n  ```\n  b\n\n  z\n",
        "<ul>\n  <li>a\np\n    <pre><code>b\n\nz\n</code></pre>\n  </li>\n</ul>",
        &mut moved,
    );
    assert!(moved.is_empty(), "rows moved:\n{}", moved.join("\n"));
}

/// CONTROLS: a blank before a sibling, and a blank after a fence that closed,
/// still loosen.
#[test]
fn a_blank_outside_the_span_still_loosens() {
    let mut moved = Vec::new();
    row(
        "- a\n  ```\n  b\n\n- c\n",
        "<ul>\n  <li><p>a</p>\n    <pre><code>b\n\n</code></pre>\n  </li>\n  <li><p>c</p></li>\n</ul>",
        &mut moved,
    );
    row(
        "- a\n  ```\n  b\n  ```\n\n  z\n",
        "<ul>\n  <li><p>a</p>\n    <pre><code>b\n</code></pre>\n    <p>z</p>\n  </li>\n</ul>",
        &mut moved,
    );
    assert!(moved.is_empty(), "rows moved:\n{}", moved.join("\n"));
}
