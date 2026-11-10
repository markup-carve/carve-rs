// carve#5842: a closed marker-line attribute run leaves no paragraph in the
// deepest nested note for a line short of the note's body column to continue.

use carve::{to_html, to_html_with_options, Options};

#[test]
fn a_closed_run_in_three_nested_notes_releases_the_flush_left_follower() {
    let source = "[^q]: [^r]: [^s]: {.x\n               .y}\nz\n\nx[^q][^r][^s]\n";
    let expected = "<p>z</p>\n<p>x<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a><a id=\"fnref2\" href=\"#fn2\" role=\"doc-noteref\"><sup>2</sup></a><a id=\"fnref3\" href=\"#fn3\" role=\"doc-noteref\"><sup>3</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n    <li id=\"fn2\">\n      <p><a href=\"#fnref2\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n    <li id=\"fn3\">\n      <p><a href=\"#fnref3\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>";
    assert_eq!(to_html(source).trim(), expected);
    assert_eq!(
        to_html_with_options(source, &Options::default().with_positions(true)).trim(),
        expected
    );
}
