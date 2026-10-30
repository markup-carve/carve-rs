//! A definition in a fence is payload (PART 0 O1). A description body or note
//! body fence hides it from both prepasses; a below-column line ends a container
//! fence even with a closer ahead (CARVE-P0-014); an unclosed fence after an open
//! paragraph is text (PART 9 §10 I4).

use carve::to_html;

#[test]
fn a_description_continuation_fence_keeps_a_note_as_payload() {
    assert_eq!(
        to_html(":: t\n: o\n  ```x\n  [^d]: u\n  ```\n\nr[^d]\n"),
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <p>o</p>\n    <pre><code class=\"language-x\">[^d]: u\n</code></pre>\n  </dd>\n</dl>\n<p>r[^d]</p>"
    );
}

#[test]
fn a_description_fence_after_a_blank_keeps_a_link_as_payload() {
    assert_eq!(
        to_html(":: t\n: o\n\n  ~~~\n  [d]: u\n  ~~~\n\n[a][d]\n"),
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <p>o</p>\n    <pre><code>[d]: u\n</code></pre>\n  </dd>\n</dl>\n<p>[a][d]</p>"
    );
}

#[test]
fn an_unclosed_fence_after_a_note_paragraph_is_text() {
    assert_eq!(
        to_html("[^n]: o\n  ```x\n  [d]: u\n\n[x][d]\n"),
        "<p><a href=\"u\">x</a></p>"
    );
}

#[test]
fn a_below_column_line_ends_an_item_fence_before_its_closer() {
    assert_eq!(
        to_html("- term\n  ```\n  code\n[d]: note\npayload\n  ```\nafter\n\n[a][d]\n"),
        "<ul>\n  <li>term\n    <pre><code>code\n</code></pre>\n  </li>\n</ul>\n<p>payload</p>\n<pre><code>after\n\n[a][d]\n</code></pre>"
    );
}

#[test]
fn a_note_marker_fence_keeps_a_link_as_payload() {
    assert_eq!(
        to_html("[a][d]\n\n[^n]: ```x\n  [d]: u\n  ```\n\nr[^n]\n"),
        "<p>[a][d]</p>\n<p>r<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <pre><code class=\"language-x\">[d]: u\n</code></pre>\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>"
    );
}

#[test]
fn a_note_continuation_fence_keeps_a_link_as_payload() {
    assert_eq!(
        to_html("[a][d]\n\n[^n]: o\n  ```x\n  [d]: u\n  ```\n\nr[^n]\n"),
        "<p>[a][d]</p>\n<p>r<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>o</p>\n      <pre><code class=\"language-x\">[d]: u\n</code></pre>\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>"
    );
}
