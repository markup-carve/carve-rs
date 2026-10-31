//! A definition-shaped line inside a fence is payload, and neither definition
//! prepass may register it: an unclosed fence on a `:` marker line opens (no
//! paragraph is open in a new body), and a fence written at a list item's
//! content column hides its lines from the footnote prepass too.

use carve::to_html;

#[test]
fn a_description_marker_fence_keeps_its_payload() {
    assert_eq!(
        to_html(":: t\n: ```x\n  [d]: u\n\n[a][d]\n"),
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <pre><code class=\"language-x\">[d]: u\n\n</code></pre>\n  </dd>\n</dl>\n<p>[a][d]</p>"
    );
    assert_eq!(
        to_html(":: t\n: ```x\n  [^d]: u```\n\nr[^d]\n"),
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <pre><code class=\"language-x\">[^d]: u```\n\n</code></pre>\n  </dd>\n</dl>\n<p>r[^d]</p>"
    );
    assert_eq!(
        to_html(":: t\n: ~~~\n  [^d]: u\n  ~~~\n\nr[^d]\n"),
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <pre><code>[^d]: u\n</code></pre>\n  </dd>\n</dl>\n<p>r[^d]</p>"
    );
}

#[test]
fn a_fence_at_an_item_content_column_keeps_its_payload() {
    assert_eq!(
        to_html("- o\n  ```x\n  [^d]: u\n  ```\n\nr[^d]\n"),
        "<ul>\n  <li>o\n    <pre><code class=\"language-x\">[^d]: u\n</code></pre>\n  </li>\n</ul>\n<p>r[^d]</p>"
    );
    assert_eq!(
        to_html("- o\n\n  ~~~\n  [^d]: u\n\nr[^d]\n"),
        "<ul>\n  <li>o\n    <pre><code>[^d]: u\n\n</code></pre>\n  </li>\n</ul>\n<p>r[^d]</p>"
    );
    assert_eq!(
        to_html("1. o\n\n   ```\n   [^d]: u\n   ```\n\nr[^d]\n"),
        "<ol>\n  <li>o\n    <pre><code>[^d]: u\n</code></pre>\n  </li>\n</ol>\n<p>r[^d]</p>"
    );
    assert_eq!(
        to_html("- o\n\n  ```x\n  [d]: u\n\n[a][d]\n"),
        "<ul>\n  <li>o\n    <pre><code class=\"language-x\">[d]: u\n\n</code></pre>\n  </li>\n</ul>\n<p>[a][d]</p>"
    );
}
