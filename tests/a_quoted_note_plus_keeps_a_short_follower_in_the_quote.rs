// markup-carve/carve#5627: a follower one column past a quoted note's marker
// still misses the note's body column.
use carve::{to_html, to_html_with_options, Options};

#[test]
fn the_short_follower_is_the_quotes_paragraph() {
    let source = "> [^a]: +\n>  v0\n\nsee[^a]\n";
    let expected = "<blockquote><p>v0</p></blockquote>\n<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>";
    assert_eq!(to_html(source), expected);
    assert_eq!(
        to_html_with_options(source, &Options::default().with_positions(true)),
        expected
    );
}
