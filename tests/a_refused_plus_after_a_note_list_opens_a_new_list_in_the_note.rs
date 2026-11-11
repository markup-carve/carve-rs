//! carve#5890 ruling B: after a refused `+` ends a note's attached list, a line at
//! the note's body column belongs to the note and opens a new block there.

use carve::to_html;

#[test]
fn a_stacked_note_opens_a_second_list_after_a_fence() {
    assert_eq!(
        to_html("- [^a]: +\n  - li\n    ```\n  +\n    - x\n\nsee[^a]\n").trim_end(),
        "<ul>\n  <li></li>\n</ul>\n<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <ul>\n        <li>li\n          <pre><code></code></pre>\n        </li>\n      </ul>\n      <ul>\n        <li>x</li>\n      </ul>\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>"
    );
}

#[test]
fn a_paragraph_line_stays_out_of_the_ended_item() {
    assert_eq!(
        to_html("[^a]: +\n- li\n+\n  p\n\nsee[^a]\n").trim_end(),
        "<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <ul>\n        <li>li</li>\n      </ul>\n      <p>p<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>"
    );
}

#[test]
fn a_second_tight_list_past_a_blank_run() {
    assert_eq!(
        to_html("- [^a]: +\n  - li\n  +\n\n    - x\n\nsee[^a]\n").trim_end(),
        "<ul>\n  <li></li>\n</ul>\n<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <ul>\n        <li>li</li>\n      </ul>\n      <ul>\n        <li>x</li>\n      </ul>\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>"
    );
}

#[test]
fn a_second_list_past_a_blank_after_a_fence() {
    assert_eq!(
        to_html("- [^a]: +\n  - li\n    ```\n  +\n\n    - x\n\nsee[^a]\n").trim_end(),
        "<ul>\n  <li></li>\n</ul>\n<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <ul>\n        <li>li\n          <pre><code></code></pre>\n        </li>\n      </ul>\n      <ul>\n        <li>x</li>\n      </ul>\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>"
    );
}

#[test]
fn a_top_level_note_opens_a_second_list_after_a_fence() {
    assert_eq!(
        to_html("[^a]: +\n- li\n  ```\n+\n  - x\n\nsee[^a]\n").trim_end(),
        "<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <ul>\n        <li>li\n          <pre><code></code></pre>\n        </li>\n      </ul>\n      <ul>\n        <li>x</li>\n      </ul>\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>"
    );
}

#[test]
fn a_quoted_note_ends_its_list() {
    assert_eq!(
        to_html("> [^a]: +\n> - li\n>   - n\n> +\n>   - x\n\nsee[^a]\n").trim_end(),
        "<blockquote>\n\n</blockquote>\n<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <ul>\n        <li>li\n          <ul>\n            <li>n</li>\n          </ul>\n        </li>\n      </ul>\n      <ul>\n        <li>x</li>\n      </ul>\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>"
    );
}

#[test]
fn a_marker_below_the_body_leaves_the_note() {
    assert_eq!(
        to_html("[^a]: +\n- li\n+\n - sib\n\nsee[^a]\n").trim_end(),
        "<ul>\n  <li>sib</li>\n</ul>\n<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <ul>\n        <li>li</li>\n      </ul>\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>"
    );
}
