//! A fence line an item's lead paragraph absorbs, because no closer is written
//! in the item, opens no block but still opens a SPAN that runs to the item's
//! end. A blank line inside that span is interior to it, so it separates no two
//! blocks and cannot make the item loose (§17). A blank before the next sibling
//! is outside the item and still loosens (carve-rs#1800).
//!
//! Every row is the executable reference's reading at the 0.1.6 tag, which spec
//! main shares.

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
fn a_blank_inside_an_unopened_fence_span_leaves_the_item_tight() {
    let mut moved = Vec::new();
    row(
        "- a\n  ```\n  b\n\n  z\n",
        "<ul>\n  <li>a\n<code>\nb</code>\n    z\n  </li>\n</ul>",
        &mut moved,
    );
    row(
        "- a\n  ```\n  b\n\n  z\n- c\n",
        "<ul>\n  <li>a\n<code>\nb</code>\n    z\n  </li>\n  <li>c</li>\n</ul>",
        &mut moved,
    );
    row(
        "1. a\n   ```\n   b\n\n   z\n",
        "<ol>\n  <li>a\n<code>\nb</code>\n    z\n  </li>\n</ol>",
        &mut moved,
    );
    row(
        "- a\n  ~~~\n  b\n\n  z\n\n  w\n",
        "<ul>\n  <li>a\n~~~\nb\n    z\n    w\n  </li>\n</ul>",
        &mut moved,
    );
    row(
        "- a\n  p\n  ```\n  b\n\n  z\n",
        "<ul>\n  <li>a\np\n<code>\nb</code>\n    z\n  </li>\n</ul>",
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
        "<ul>\n  <li><p>a\n<code>\nb</code></p></li>\n  <li><p>c</p></li>\n</ul>",
        &mut moved,
    );
    row(
        "- a\n  ```\n  b\n  ```\n\n  z\n",
        "<ul>\n  <li><p>a</p>\n    <pre><code>b\n</code></pre>\n    <p>z</p>\n  </li>\n</ul>",
        &mut moved,
    );
    assert!(moved.is_empty(), "rows moved:\n{}", moved.join("\n"));
}
