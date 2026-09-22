//! §10 I4 for a fence written after an item's open paragraph: it interrupts
//! only when its closer is written in the item's OWN body. The search stops
//! where the item ends - a sibling or outer marker, or a blank followed by a
//! line below the content column - and a closer counts only at the content
//! column. A closer anywhere later in the document used to count, so the item
//! ended at the next below-column line as though a code block were open, while
//! its own parse read the fence as paragraph text (carve-rs#1800, #1802).
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
fn a_closer_outside_the_items_body_does_not_open_the_fence() {
    let mut moved = Vec::new();
    row(
        "- a\n  ```\n  b\n y\n   ```\n",
        "<ul>\n  <li>a\n<code>\nb\ny\n</code></li>\n</ul>",
        &mut moved,
    );
    row(
        "1. a\n   ```\n   b\n y\n    ```\n",
        "<ol>\n  <li>a\n<code>\nb\ny\n</code></li>\n</ol>",
        &mut moved,
    );
    row(
        "- a\n  ```\n  b\n y\n\nz\n  ```\n",
        "<ul>\n  <li>a\n<code>\nb\ny</code></li>\n</ul>\n<p>z\n<code></code></p>",
        &mut moved,
    );
    row(
        "1. a\n   ```\n   b\n y\n\nz\n   ```\n",
        "<ol>\n  <li>a\n<code>\nb\ny</code></li>\n</ol>\n<p>z\n<code></code></p>",
        &mut moved,
    );
    row(
        "- a\n  ```\n  b\n y\n- q\n  ```\n",
        "<ul>\n  <li>a\n<code>\nb\ny</code></li>\n  <li>q\n<code></code></li>\n</ul>",
        &mut moved,
    );
    row(
        ":: t\n: a\n  ```\n  b\n y\n   ```\n",
        "<dl>\n  <dt>t</dt>\n  <dd>a\n<code>\nb\ny\n</code></dd>\n</dl>",
        &mut moved,
    );
    row(
        ":: t\n: a\n  ```\n  b\n y\n\nz\n  ```\n",
        "<dl>\n  <dt>t</dt>\n  <dd>a\n<code>\nb\ny</code></dd>\n</dl>\n<p>z\n<code></code></p>",
        &mut moved,
    );
    row(
        ":: t\n: a\n  ~~~\n  b\n y\n\nz\n  ~~~\n",
        "<dl>\n  <dt>t</dt>\n  <dd>a\n~~~\nb\ny</dd>\n</dl>\n<p>z\n~~~</p>",
        &mut moved,
    );
    assert!(moved.is_empty(), "rows moved:\n{}", moved.join("\n"));
}

/// CONTROLS: a closer written in the body still opens the fence.
#[test]
fn a_closer_in_the_items_body_still_opens_it() {
    let mut moved = Vec::new();
    row(
        "- a\n  ```\n  b\n  ```\n",
        "<ul>\n  <li>a\n    <pre><code>b\n</code></pre>\n  </li>\n</ul>",
        &mut moved,
    );
    row(
        ":: t\n: a\n  ```\n  b\n  ```\n",
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <p>a</p>\n    <pre><code>b\n</code></pre>\n  </dd>\n</dl>",
        &mut moved,
    );
    assert!(moved.is_empty(), "rows moved:\n{}", moved.join("\n"));
}
