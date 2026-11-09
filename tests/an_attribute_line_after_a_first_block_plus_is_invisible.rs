// carve#5549 sweep: an attribute line between a first-block `+` and its block
// is invisible (PART 9 §17 L3, carve#4449), and a wrapped attribute run below a
// refused note `+` is an attribute line, not a lazy note line (CARVE-P0-024).

use carve::{to_html, to_html_with_options, Options};

fn check(source: &str, expected: &str) {
    assert_eq!(to_html(source), expected);
    let positioned = to_html_with_options(source, &Options::default().with_positions(true));
    assert_eq!(positioned, expected);
}

#[test]
fn a_description_plus_takes_a_list_past_an_attribute_line() {
    check(
        ":: t\n: +\n{.x}\n- li\n",
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <ul class=\"x\">\n      <li>li</li>\n    </ul>\n  </dd>\n</dl>",
    );
}

#[test]
fn a_description_plus_takes_a_list_past_a_wrapped_attribute_line() {
    check(
        ":: t\n: +\n{.x\n .y}\n- li\n",
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <ul class=\"x y\">\n      <li>li</li>\n    </ul>\n  </dd>\n</dl>",
    );
}

#[test]
fn a_description_plus_takes_an_ordered_list_past_an_attribute_line() {
    check(
        ":: t\n: +\n{.x}\n1. li\n",
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <ol class=\"x\">\n      <li>li</li>\n    </ol>\n  </dd>\n</dl>",
    );
}

#[test]
fn a_quoted_description_plus_takes_a_list_past_an_attribute_line() {
    check(
        "> :: t\n> : +\n> {.x}\n> - li\n",
        "<blockquote>\n  <dl>\n    <dt>t</dt>\n    <dd>\n      <ul class=\"x\">\n        <li>li</li>\n      </ul>\n    </dd>\n  </dl>\n</blockquote>",
    );
}

#[test]
fn a_description_plus_takes_a_list_past_attribute_and_comment_lines() {
    check(
        ":: t\n: +\n{.x}\n%% c\n- li\n",
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <ul class=\"x\">\n      <li>li</li>\n    </ul>\n  </dd>\n</dl>",
    );
}

#[test]
fn a_refused_brace_run_still_ends_at_a_list_in_a_paragraph_host() {
    check(
        ":: t\n: +\n{x\n- li\n",
        "<dl>\n  <dt>t</dt>\n  <dd>{x</dd>\n</dl>\n<ul>\n  <li>li</li>\n</ul>",
    );
}

#[test]
fn a_wrapped_attribute_line_below_a_refused_note_plus_leaves_the_note() {
    check(
        "[^a]: +\n {.x\n  .y}\npara\n\nsee[^a]\n",
        "<p class=\"x y\">para</p>\n<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>",
    );
}

#[test]
fn a_wrapped_attribute_line_below_a_refused_note_plus_keeps_unreferenced_text() {
    check("[^a]: +\n {.x\n  .y}\npara\n", "<p class=\"x y\">para</p>");
}

#[test]
fn a_quoted_wrapped_attribute_line_below_a_refused_note_plus_leaves_the_note() {
    check(
        "> [^a]: +\n>  {.x\n>   .y}\n> para\n\nsee[^a]\n",
        "<blockquote><p class=\"x y\">para</p></blockquote>\n<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>",
    );
}

#[test]
fn plain_text_below_a_refused_note_plus_still_joins_the_note() {
    check(
        "[^a]: +\n foo\n\nsee[^a]\n",
        "<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>foo<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>",
    );
}
