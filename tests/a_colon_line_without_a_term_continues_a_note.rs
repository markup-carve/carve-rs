use carve::to_html;

#[test]
fn a_colon_line_without_a_term_continues_a_note_paragraph() {
    let source = "[^a]: x\n: \nz\n\n[^a]\n";
    let expected = "<p><a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>x\n:\nz<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>";
    assert_eq!(to_html(source).trim(), expected);
}

#[test]
fn colon_continuation_after_a_note_plus_stays_in_the_note() {
    let source = "[^a]: +\n  lush\n: \n  seee[^a]\n";
    assert_eq!(to_html(source).trim(), "");
}

#[test]
fn colon_continuation_after_a_nested_definition_stays_in_the_note() {
    let source = "[^f]: outer\n> \n       [^g]: mid\n\n         [r]: /url\n         TAILWORD\n: \nx[^f] [^g] [t][r]\n";
    assert_eq!(to_html(source).trim(), "<blockquote>\n\n</blockquote>");
}

#[test]
fn consecutive_colon_lines_keep_the_following_block_in_the_note() {
    let source = "[^n]: intro\n: \n: \n   ::: >\n   > quote\n   :::\n\nsee[^n]\n";
    let expected = "<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>intro\n:\n:</p>\n      <blockquote>\n        <blockquote><p>quote</p></blockquote>\n      </blockquote>\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>";
    assert_eq!(to_html(source).trim(), expected);
}
