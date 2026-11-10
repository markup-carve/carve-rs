// carve#5723: a nested note hosts a line block at its body column, two
// columns past its marker (§24 C3), so a line below that column ends it.

use carve::{to_html, to_html_with_options, Options};

fn check(source: &str, expected: &str) {
    assert_eq!(to_html(source).trim_end(), expected);
    let positioned = to_html_with_options(source, &Options::default().with_positions(true));
    assert_eq!(positioned.trim_end(), expected);
}

#[test]
fn a_heading_below_a_losing_nested_note_line_block_returns_to_the_outer_note() {
    check(
        "[^n]: a\n  [^n]: a\n      ::: |\n  # h\n[^n]\n",
        "<p><a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>a</p>\n      <h1 id=\"h\">h</h1>\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>",
    );
}

#[test]
fn a_heading_below_a_nested_note_line_block_returns_to_the_outer_note() {
    check(
        "[^n]: a\n  [^m]: a\n      ::: |\n  # h\n[^n]\n",
        "<p><a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>a</p>\n      <h1 id=\"h\">h</h1>\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>",
    );
}

#[test]
fn a_paragraph_below_a_nested_note_line_block_ends_the_outer_note() {
    check(
        "- a\n  [^m]: a\n      ::: |\n  # h\nz\n\n[^m]\n",
        "<ul>\n  <li>a\n    <h1 id=\"h\">h</h1>\n  </li>\n</ul>\n<p>z</p>\n<p><a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>a</p>\n      <div class=\"line-block\">\n\n      </div>\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>",
    );
}
