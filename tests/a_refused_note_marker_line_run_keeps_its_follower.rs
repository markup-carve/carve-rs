// CARVE-P2-035: a refused run on a note marker line protects its next line,
// including a definition-shaped line, and keeps the paragraph open for S4.

use carve::{to_html, to_html_with_options, Options};

fn check(source: &str, expected: &str) {
    assert_eq!(to_html(source), expected);
    assert_eq!(
        to_html_with_options(source, &Options::default().with_positions(true)),
        expected
    );
}

#[test]
fn definition_shaped_follower_and_lazy_line_stay_in_the_note() {
    check(
        "- [^q]: {.x\n}\n[^m]: n\n  z\n\nx[^q]\n",
        "<ul>\n  <li></li>\n</ul>\n<p>x<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>{.x\n}<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>",
    );
}
