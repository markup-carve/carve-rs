// carve#5542 sweep: a block at the first note's marker column below a sibling
// definition whose `+` the walk consumed belongs to the first note, and the
// block is measured from its own lead (carve#5502, carve#4449).

use carve::{to_html, to_html_with_options, Options};

fn check(source: &str, expected: &str) {
    assert_eq!(to_html(source), expected);
    let positioned = to_html_with_options(source, &Options::default().with_positions(true));
    assert_eq!(positioned, expected);
}

#[test]
fn a_quoted_sibling_plus_note_leaves_a_marker_column_paragraph_to_the_first() {
    check(
        "> [^a]: +\n> [^b]: +\n> para\n\nsee[^a][^b]\n",
        "<blockquote>\n\n</blockquote>\n<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a><a id=\"fnref2\" href=\"#fn2\" role=\"doc-noteref\"><sup>2</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>para<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n    <li id=\"fn2\">\n      <p><a href=\"#fnref2\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>",
    );
}

#[test]
fn a_quoted_losing_plus_duplicate_is_read_as_absent() {
    check(
        "> [^a]: +\n> [^a]: +\n> para\n\nsee[^a]\n",
        "<blockquote>\n\n</blockquote>\n<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>para<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>",
    );
}

#[test]
fn a_quoted_sibling_plus_note_passes_an_attribute_line_and_its_paragraph() {
    check(
        "> [^a]: +\n> [^b]: +\n> {.x}\n> para\n\nsee[^a][^b]\n",
        "<blockquote>\n\n</blockquote>\n<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a><a id=\"fnref2\" href=\"#fn2\" role=\"doc-noteref\"><sup>2</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p class=\"x\">para<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n    <li id=\"fn2\">\n      <p><a href=\"#fnref2\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>",
    );
}

#[test]
fn a_quoted_sibling_plus_note_keeps_an_indented_block_for_itself() {
    check(
        "> [^a]: +\n> [^b]: +\n>   para\n\nsee[^a][^b]\n",
        "<blockquote>\n\n</blockquote>\n<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a><a id=\"fnref2\" href=\"#fn2\" role=\"doc-noteref\"><sup>2</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n    <li id=\"fn2\">\n      <p>para<a href=\"#fnref2\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>",
    );
}

#[test]
fn a_quote_at_the_marker_column_ends_a_paragraph_past_a_sibling_plus_note() {
    check(
        "[^a]: +\n[^b]: +\nfoo\n> q\n\nsee[^a][^b]\n",
        "<blockquote><p>q</p></blockquote>\n<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a><a id=\"fnref2\" href=\"#fn2\" role=\"doc-noteref\"><sup>2</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>foo<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n    <li id=\"fn2\">\n      <p><a href=\"#fnref2\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>",
    );
}
#[test]
fn a_content_column_quote_below_a_losing_plus_duplicate_stays_one_quote() {
    check(
        "[^a]: +\n[^a]: +\n  > q\n  > r\n\nsee[^a]\n",
        "<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <blockquote><p>q\nr</p></blockquote>\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>",
    );
}

#[test]
fn a_quoted_content_column_quote_below_a_losing_plus_duplicate_stays_one_quote() {
    check(
        "> [^a]: +\n> [^a]: +\n>   > q\n>   > r\n\nsee[^a]\n",
        "<blockquote>\n\n</blockquote>\n<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <blockquote><p>q\nr</p></blockquote>\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>",
    );
}
#[test]
fn a_content_column_heading_below_a_losing_plus_duplicate_is_a_heading() {
    check(
        "[^a]: +\n[^a]: +\n  # h\n\nsee[^a]\n",
        "<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <h1 id=\"h\">h</h1>\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>",
    );
}

#[test]
fn an_indented_term_below_a_losing_plus_duplicate_opens_a_list() {
    check(
        "[^a]: +\n[^a]: +\n    :: t\n    : d\n\nsee[^a]\n",
        "<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <dl>\n        <dt>t</dt>\n        <dd>d</dd>\n      </dl>\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>",
    );
}
