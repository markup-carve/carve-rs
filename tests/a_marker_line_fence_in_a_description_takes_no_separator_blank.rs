//! A fence opened on a list item's marker line in a description body is not
//! a paragraph, so the line after it gets no manufactured separator blank.

use carve::to_html;

#[test]
fn a_closer_below_the_item_closes_nothing() {
    assert_eq!(
        to_html(":: t\n: - ```\n  ```\n").trim(),
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <ul>\n      <li>\n        <pre><code></code></pre>\n      </li>\n    </ul>\n    <pre><code></code></pre>\n  </dd>\n</dl>"
    );
}

#[test]
fn a_closer_at_the_body_column_after_a_marker_line_fence() {
    assert_eq!(
        to_html("::  t\n:  - ```\n    ```\n``text`rust\n").trim(),
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <ul>\n      <li>\n        <pre><code></code></pre>\n      </li>\n    </ul>\n    <pre><code></code></pre>\n  </dd>\n</dl>\n<p><code>text`rust</code></p>"
    );
}

#[test]
fn a_stacked_marker_line_fence() {
    assert_eq!(
        to_html(":: t\n: - - ```\n  ```\n  b\n      ```\n```\n").trim(),
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <ul>\n      <li>\n        <ul>\n          <li>\n            <pre><code></code></pre>\n          </li>\n        </ul>\n      </li>\n    </ul>\n    <pre><code>b\n    ```\n</code></pre>\n  </dd>\n</dl>\n<pre><code></code></pre>"
    );
}
