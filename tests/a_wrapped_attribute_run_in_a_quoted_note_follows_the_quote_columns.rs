// carve-rs#4124 and carve#5569: columns of a wrapped run under a quoted note
// are compared after the quote prefixes (CARVE-P2-035, PART 9 §15 A5).

use carve::{to_html, to_html_with_options, Options};

fn check(source: &str, expected: &str) {
    assert_eq!(to_html(source), expected);
    let positioned = to_html_with_options(source, &Options::default().with_positions(true));
    assert_eq!(positioned, expected);
}

#[test]
fn a_nested_quoted_run_below_the_note_column_labels_the_quote_paragraph() {
    check(
        "> > [^a]: +\n> >  {.x\n> >   .y}\n> > para\n\nsee[^a]\n",
        "<blockquote>\n  <blockquote><p class=\"x y\">para</p></blockquote>\n</blockquote>\n<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>",
    );
}

#[test]
fn a_quoted_run_at_the_note_column_labels_the_note_paragraph() {
    check(
        "> [^a]: +\n>   {.x\n>    .y}\n> para\n\nsee[^a]\n",
        "<blockquote>\n\n</blockquote>\n<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p class=\"x y\">para<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>",
    );
}
