use carve::{to_html, to_html_with_options, Options};

fn check(source: &str, expected: &str) {
    assert_eq!(to_html(source), expected);
    assert_eq!(
        to_html_with_options(source, &Options::default().with_positions(true)),
        expected
    );
}

#[test]
fn last_of_three_plus_notes_owns_its_paragraph_and_quote() {
    check(
        "[^a]: +\n[^b]: +\n[^c]: +\n  para\n  > q\n\nsee[^a][^b][^c]\n",
        "<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a><a id=\"fnref2\" href=\"#fn2\" role=\"doc-noteref\"><sup>2</sup></a><a id=\"fnref3\" href=\"#fn3\" role=\"doc-noteref\"><sup>3</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n    <li id=\"fn2\">\n      <p><a href=\"#fnref2\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n    <li id=\"fn3\">\n      <p>para</p>\n      <blockquote><p>q</p></blockquote>\n      <p><a href=\"#fnref3\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>",
    );
}

#[test]
fn quoted_plus_note_passes_a_plain_sibling_note_and_its_lazy_body() {
    check(
        "> [^a]: +\n> [^b]: x\n> para\n> > q\n\nsee[^a][^b]\n",
        "<blockquote>\n\n</blockquote>\n<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a><a id=\"fnref2\" href=\"#fn2\" role=\"doc-noteref\"><sup>2</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <blockquote><p>q</p></blockquote>\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n    <li id=\"fn2\">\n      <p>x\npara<a href=\"#fnref2\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>",
    );
}
#[test]
fn later_note_keeps_blank_separated_continuation_after_pass() {
    let source = "[^n]: +\n[^m]: d1\n[^k]: +\n  para\n\n  more\n\nsee[^n][^m][^k]\n";
    let html = to_html(source);
    assert!(html.contains("<p>para</p>\n      <p>more"), "{html}");
    assert!(!html.contains("<li id=\"fn1\">\n      <p>more"), "{html}");
}

#[test]
fn losing_later_plus_leaves_the_blank_separated_block_with_the_first_note() {
    let source = "[^n]: +\n[^m]: d1\n[^m]: +\n\n  para\n\nsee[^n][^m]\n";
    let html = to_html(source);
    assert!(html.contains("<li id=\"fn1\">\n      <p>para"), "{html}");
}
