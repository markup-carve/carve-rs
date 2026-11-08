//! A marker-line `+` grants one block; the following fence returns to the item.

use carve::to_html;

#[test]
fn unclosed_fence_after_attached_note_paragraph_belongs_to_item() {
    let source = "- [^a]: +\n  p\n    ``` x\n    [^z]: code\n\nsee[^a]\n";
    let expected = "<ul>\n  <li>\n    <pre><code class=\"language-x\">[^z]: code\n\n</code></pre>\n  </li>\n</ul>\n<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>p<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>";
    assert_eq!(to_html(source).trim_end(), expected);
}
