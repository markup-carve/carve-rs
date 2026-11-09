// CARVE-P2-035 (carve#5080, carve#5561): under a first-block `+`, the line after
// a refused brace run continues its paragraph even when it opens a list or a
// block at the body column.

use carve::{to_html, to_html_with_options, Options};

fn check(source: &str, expected: &str) {
    assert_eq!(to_html(source), expected);
    let positioned = to_html_with_options(source, &Options::default().with_positions(true));
    assert_eq!(positioned, expected);
}

#[test]
fn a_list_marker_after_a_refused_run_stays_in_the_note() {
    check(
        "[^a]: +\n{.x\n.y}\n- li\n\nsee[^a]\n",
        "<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>{.x\n.y}\n- li<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>",
    );
}

#[test]
fn only_the_first_marker_after_the_run_joins() {
    check(
        "[^a]: +\n{.x\n.y}\n- li\n- l2\n\nsee[^a]\n",
        "<ul>\n  <li>l2</li>\n</ul>\n<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>{.x\n.y}\n- li<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>",
    );
}

#[test]
fn an_ordered_marker_after_a_refused_run_stays_in_the_note() {
    check(
        "[^a]: +\n{.x\n.y}\n1. li\n\nsee[^a]\n",
        "<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>{.x\n.y}\n1. li<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>",
    );
}

#[test]
fn a_quoted_note_keeps_the_marker_after_its_refused_run() {
    check(
        "> [^a]: +\n> {.x\n> .y}\n> - li\n\nsee[^a]\n",
        "<blockquote>\n\n</blockquote>\n<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>{.x\n.y}\n- li<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>",
    );
}

#[test]
fn a_quoted_note_keeps_a_body_column_heading_after_its_refused_run() {
    check(
        "> [^a]: +\n> {#i\n> .y}\n>   # h\n\nsee[^a]\n",
        "<blockquote>\n\n</blockquote>\n<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>{#i\n.y}\n# h<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>",
    );
}

#[test]
fn a_description_keeps_the_marker_after_its_refused_run() {
    check(
        ":: t\n: +\n{.x\n.y}\n- li\n",
        "<dl>\n  <dt>t</dt>\n  <dd>{.x\n.y}\n- li</dd>\n</dl>",
    );
}

#[test]
fn a_description_keeps_a_body_column_heading_after_its_refused_run() {
    check(
        ":: t\n: +\n{.x\n.y}\n  # h\n",
        "<dl>\n  <dt>t</dt>\n  <dd>{.x\n.y}\n# h</dd>\n</dl>",
    );
}

#[test]
fn a_marker_below_a_body_column_run_still_ends_the_note() {
    check(
        "[^a]: +\n  {.x\n  .y}\n- li\n\nsee[^a]\n",
        "<ul>\n  <li>li</li>\n</ul>\n<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>{.x\n.y}<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>",
    );
}

#[test]
fn a_nested_note_keeps_a_marker_column_follower() {
    check(
        "- x\n  [^a]: +\n  {.x\n  .y}\n  - li\n\nsee[^a]\n",
        "<ul>\n  <li>x</li>\n</ul>\n<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>{.x\n.y}\n- li<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>",
    );
}

#[test]
fn a_lazy_marker_below_an_item_note_still_ends_it() {
    check(
        "- [^a]: +\n  {#i\n  .y}\n - li\n\nsee[^a]\n",
        "<ul>\n  <li></li>\n  <li>li</li>\n</ul>\n<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>{#i\n.y}<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>",
    );
}

#[test]
fn a_quoted_lazy_marker_below_an_item_note_still_ends_it() {
    check(
        "> - [^a]: +\n>   {#i\n>   .y}\n>  - li\n\nsee[^a]\n",
        "<blockquote>\n  <ul>\n    <li></li>\n    <li>li</li>\n  </ul>\n</blockquote>\n<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>{#i\n.y}<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>",
    );
}
