//! A link definition in a fence a list item opens on its marker line inside a
//! footnote body is code content: it is not hoisted, and no placeholder is left.

use carve::to_html;

#[test]
fn an_unclosed_fence_on_a_note_item_marker_line() {
    assert_eq!(
        to_html("[^n]: - ```\n    [r]: /u\n\nsee[^n] [r][]\n").trim(),
        "<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a> [r][]</p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <ul>\n        <li>\n          <pre><code>[r]: /u\n\n</code></pre>\n        </li>\n      </ul>\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>"
    );
}

#[test]
fn a_closed_fence_on_a_note_item_marker_line() {
    assert_eq!(
        to_html("[^n]: - ```\n    [r]: /u\n    ```\n\nsee[^n] [r][]\n").trim(),
        "<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a> [r][]</p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <ul>\n        <li>\n          <pre><code>[r]: /u\n</code></pre>\n        </li>\n      </ul>\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>"
    );
}

#[test]
fn a_sibling_item_after_an_item_paragraph() {
    assert_eq!(
        to_html("[^n]: - a\n  - ```\n    [r]: /u\n\nsee[^n] [r][]\n").trim(),
        "<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a> [r][]</p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <ul>\n        <li>a</li>\n        <li>\n          <pre><code>[r]: /u\n\n</code></pre>\n        </li>\n      </ul>\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>"
    );
}

#[test]
fn a_definition_after_the_fenced_item_registers() {
    assert_eq!(
        to_html("[^n]: - ```\n    x\n  [r]: /u\n\nsee[^n] [r][]\n").trim(),
        "<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a> <a href=\"/u\">r</a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <ul>\n        <li>\n          <pre><code>x\n</code></pre>\n        </li>\n      </ul>\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>"
    );
}

#[test]
fn a_definition_after_the_fenced_item_in_a_hosted_note_registers() {
    assert_eq!(
        to_html("- [^n]: - ```\n      x\n    [r]: /u\n\nsee[^n] [r][]\n").trim(),
        "<ul>\n  <li></li>\n</ul>\n<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a> <a href=\"/u\">r</a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <ul>\n        <li>\n          <pre><code>x\n</code></pre>\n        </li>\n      </ul>\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>"
    );
}

#[test]
fn a_description_hosted_note() {
    assert_eq!(
        to_html(":: t\n: [^n]: - ```\n      [r]: /u\n      ```\n\nsee[^n] [r][]\n").trim(),
        "<dl>\n  <dt>t</dt>\n  <dd></dd>\n</dl>\n<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a> [r][]</p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <ul>\n        <li>\n          <pre><code>[r]: /u\n</code></pre>\n        </li>\n      </ul>\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>"
    );
}

#[test]
fn a_sibling_item_s_fence_after_a_fenced_item() {
    assert_eq!(
        to_html("[^n]: - ```\n    x\n  - ```\n    [r]: /u\n\nsee[^n] [r][]\n").trim(),
        "<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a> [r][]</p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <ul>\n        <li>\n          <pre><code>x\n</code></pre>\n        </li>\n        <li>\n          <pre><code>[r]: /u\n\n</code></pre>\n        </li>\n      </ul>\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>"
    );
}

#[test]
fn a_sibling_item_s_fence_after_a_closed_fence() {
    assert_eq!(
        to_html("[^n]: - ```\n    x\n    ```\n  - ```\n    [r]: /u\n    ```\n\nsee[^n] [r][]\n").trim(),
        "<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a> [r][]</p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <ul>\n        <li>\n          <pre><code>x\n</code></pre>\n        </li>\n        <li>\n          <pre><code>[r]: /u\n</code></pre>\n        </li>\n      </ul>\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>"
    );
}
