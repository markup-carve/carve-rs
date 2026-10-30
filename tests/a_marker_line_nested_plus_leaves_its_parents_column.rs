//! CARVE-P9-067: a block in an enclosing item's column stays with that item.
//! An item opened on its parent's marker line (`- - +`) therefore takes nothing
//! between its own marker and content columns (carve-rs#3577 shape A).

fn html(source: &str) -> String {
    carve::to_html(source)
}

const OUTER_TAKES_IT: &str = "<ul>\n  <li>\n    <ul>\n      <li>+</li>\n    </ul>\n    <pre><code></code></pre>\n  </li>\n</ul>";

#[test]
fn a_fence_between_the_nested_marker_and_content_columns_opens_in_the_outer_item() {
    assert_eq!(html("- - +\n   ```\n").trim(), OUTER_TAKES_IT);
    assert!(html("- - +\n  ```\n").contains("<li>\n        <pre><code></code></pre>"));
}

#[test]
fn a_quote_there_opens_in_the_outer_item_too() {
    assert_eq!(
        html("- - +\n  > q\n").trim(),
        "<ul>\n  <li>\n    <ul>\n      <li>\n        <blockquote><p>q</p></blockquote>\n      </li>\n    </ul>\n  </li>\n</ul>"
    );
}

#[test]
fn a_nested_item_on_its_own_line_still_takes_the_block() {
    assert_eq!(
        html("- a\n  - +\n  ```\n").trim(),
        "<ul>\n  <li>a\n    <ul>\n      <li>\n        <pre><code></code></pre>\n      </li>\n    </ul>\n  </li>\n</ul>"
    );
}

#[test]
fn a_task_item_on_its_own_line_still_takes_the_block() {
    let out = html("- a\n  - [ ] +\n   > q\n");
    assert!(out.contains("aria-label=\"+\"> +</li>"), "{out}");
}
