//! An invisible note definition with an attached nonparagraph block opens no
//! paragraph in its host item for a later below-column line to continue.

#[test]
fn flush_reference_after_item_hosted_attached_list_is_document_text() {
    let source = "- [^a]: +\n  - li\n    ```\n    > quoted\nsee[^a]\n";
    let expected = "<ul>\n  <li></li>\n</ul>\n<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <ul>\n        <li>li\n          <pre><code>&gt; quoted\n</code></pre>\n        </li>\n      </ul>\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>";
    assert_eq!(carve::to_html(source), expected);
}
