//! A descendant comment span finishes before the attached note host resumes.

use carve::to_html;

#[test]
fn line_after_nested_comment_note_stays_in_ancestor_note() {
    let source = "[^n0]: +\n> q\n  [^n1]: %%%\n    [^z]: c\n    %%%\n  ind1\n\nsee[^n0][^n1]\n";
    let expected = "<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a><a id=\"fnref2\" href=\"#fn2\" role=\"doc-noteref\"><sup>2</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <blockquote><p>q</p></blockquote>\n      <p>ind1<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n    <li id=\"fn2\">\n      <p><a href=\"#fnref2\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>";
    assert_eq!(to_html(source).trim_end(), expected);
}

#[test]
fn flush_line_after_descendant_code_fence_stays_in_document() {
    let source = "[^n0]: +\n> q\n  [^n1]: ```\n    [^z]: c\n    ```\nind1\n\nsee[^n0][^n1]\n";
    let expected = "<p>ind1</p>\n<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a><a id=\"fnref2\" href=\"#fn2\" role=\"doc-noteref\"><sup>2</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <blockquote><p>q</p></blockquote>\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n    <li id=\"fn2\">\n      <pre><code>[^z]: c\n</code></pre>\n      <p><a href=\"#fnref2\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>";
    assert_eq!(to_html(source).trim_end(), expected);
}
