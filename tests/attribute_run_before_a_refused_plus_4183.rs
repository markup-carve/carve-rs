// carve-rs#4183: after a lone `+`, a marker-column attribute run closes its
// host when a content line or a refused `+` follows (plus-ownership A02).

use carve::{to_html, to_html_with_options, Options};

fn check(source: &str, expected: &str) {
    assert_eq!(to_html(source).trim_end(), expected);
    let positioned = to_html_with_options(source, &Options::default().with_positions(true));
    assert_eq!(positioned.trim_end(), expected);
}

#[test]
fn a_refused_plus_after_a_marker_column_attribute_run_stays_a_letter() {
    check(
        "- p\n+\n{#i\n .y}\n+\n  zq\n",
        "<ul>\n  <li>p</li>\n</ul>\n<p id=\"i\" class=\"y\">+\nzq</p>",
    );
}

#[test]
fn a_trailing_plus_after_a_marker_column_attribute_run_stays_a_letter() {
    check(
        "- p\n+\n{.y}\n+\n",
        "<ul>\n  <li>p</li>\n</ul>\n<p class=\"y\">+</p>",
    );
}

#[test]
fn a_description_run_before_a_refused_plus_closes_the_body() {
    check(
        ":: t\n: p\n+\n{#i .y}\n+\n  zq\n",
        "<dl>\n  <dt>t</dt>\n  <dd>p</dd>\n</dl>\n<p id=\"i\" class=\"y\">+\nzq</p>",
    );
}

#[test]
fn a_note_run_before_a_refused_plus_closes_the_body() {
    check(
        "see[^a]\n\n[^a]: p\n+\n{#i\n .y}\n+\n  zq\n",
        "<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<p id=\"i\" class=\"y\">+\nzq</p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>p<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>",
    );
}
