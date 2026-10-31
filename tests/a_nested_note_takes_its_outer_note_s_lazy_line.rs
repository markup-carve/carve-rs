//! A line the outer note reads as lazy continues the open paragraph, and that
//! paragraph is the nested note's when one is open (carve#3885: a footnote body
//! takes a lazy line like every container).

use carve::to_html;

fn notes(outer: &str, inner: &str) -> String {
    format!("<p>x<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a> <a id=\"fnref2\" href=\"#fn2\" role=\"doc-noteref\"><sup>2</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>{outer}<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n    <li id=\"fn2\">\n      <p>{inner}<a href=\"#fnref2\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>")
}

#[test]
fn the_lazy_line_joins_the_nested_note() {
    for source in [
        "[^f]: outer\n  [^g]: mid\nlazy\n\nx[^f] [^g]\n",
        "[^f]: outer\n\n  [^g]: mid\nlazy\n\nx[^f] [^g]\n",
    ] {
        assert_eq!(
            to_html(source).trim(),
            notes("outer", "mid\nlazy"),
            "{source:?}"
        );
    }
}

#[test]
fn past_a_deeper_paragraph_line() {
    assert_eq!(
        to_html("[^f]: outer\n\n       [^g]: mid\n         TAIL\nlazy\n\nx[^f] [^g]\n").trim(),
        notes("outer", "mid\nTAIL\nlazy")
    );
}
