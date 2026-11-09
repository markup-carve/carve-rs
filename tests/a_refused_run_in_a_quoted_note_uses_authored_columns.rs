// carve-rs#4130, CARVE-P2-035: the continuation is compared with the opening
// brace in source columns after quote prefixes, even after a note body dedents.
use carve::{to_html, to_html_with_options, Options};

#[test]
fn refused_run_in_quoted_note_stays_literal() {
    let source = "> [^a]: +\n>     {#i\n>  .y}\n\nsee[^a]\n";
    let expected = "<blockquote>\n\n</blockquote>\n<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>{#i\n.y}<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>";
    assert_eq!(to_html(source), expected);
    assert_eq!(
        to_html_with_options(source, &Options::default().with_positions(true)),
        expected
    );
}
