// carve-rs#4145: an unmarked lazy continuation may finish a refused brace
// run in a quoted note's first attached paragraph (CARVE-P2-035).
use carve::{to_html, to_html_with_options, Options};

fn check(source: &str, note_text: &str) {
    let expected = format!(
        "<blockquote>\n\n</blockquote>\n<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>{note_text}<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>"
    );
    assert_eq!(to_html(source), expected);
    assert_eq!(
        to_html_with_options(source, &Options::default().with_positions(true)),
        expected
    );
}

#[test]
fn lazy_continuation_finishes_refused_run() {
    check("> [^a]: +\n> {#i\n.y}\n\nsee[^a]\n", "{<span class=\"tag\"><strong>#i</strong></span>\n.y}");
}

#[test]
fn next_line_is_still_in_the_same_paragraph() {
    check("> [^a]: +\n> {#i\n.y}\nnext\n\nsee[^a]\n", "{<span class=\"tag\"><strong>#i</strong></span>\n.y}\nnext");
    let list_after_run = "> [^a]: +\n> {#i\n.y}\n- child\n\nsee[^a]\n";
    let expected = "<blockquote>\n\n</blockquote>\n<ul>\n  <li>child</li>\n</ul>\n<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>{<span class=\"tag\"><strong>#i</strong></span>\n.y}<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>";
    assert_eq!(to_html(list_after_run), expected);
    assert_eq!(
        to_html_with_options(list_after_run, &Options::default().with_positions(true)),
        expected
    );
    check(
        "> [^a]: +\n> {#i\n> .y}\nnext\n\nsee[^a]\n",
        "{<span class=\"tag\"><strong>#i</strong></span>\n.y}\nnext",
    );
    check(
        "> [^a]: +\n> {#i\n.y}\n> next\n\nsee[^a]\n",
        "{<span class=\"tag\"><strong>#i</strong></span>\n.y}\nnext",
    );
}
