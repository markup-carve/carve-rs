//! 0.1 output lost on next2, found by the 0.2.0 regression sweep.

use carve::api::{Carve, Options};

fn html(source: &str) -> String {
    Carve::new().render(source, &Options::default()).unwrap()
}

#[test]
fn authored_cell_style_keeps_its_value() {
    assert_eq!(
        html("|{style=\"text-align: right;\"} a |"),
        "<table>\n  <tbody>\n    <tr><td style=\"text-align: right;\">a</td></tr>\n  </tbody>\n</table>"
    );
    assert_eq!(
        html("|{>}{style=\"color: red ;  \"} a |"),
        "<table>\n  <tbody>\n    <tr><td style=\"color: red ; text-align: right;\">a</td></tr>\n  </tbody>\n</table>"
    );
}

#[test]
fn blocks_glued_after_cell_marks_are_attributes() {
    for source in ["| a |{>}{.num} 9 |\n", "| a |{> }{.num} 9 |\n"] {
        assert_eq!(
            html(source),
            "<table>\n  <tbody>\n    <tr><td>a</td><td class=\"num\" style=\"text-align: right;\">9</td></tr>\n  </tbody>\n</table>"
        );
    }
}

#[test]
fn block_image_alt_skips_atoms() {
    assert_eq!(html("![a </#x]> b](u)"), "<img src=\"u\" alt=\"a &lt;/#x]&gt; b\">");
    assert_eq!(html("![a <http://x]> b](u)"), "<img src=\"u\" alt=\"a &lt;http://x]&gt; b\">");
    assert_eq!(html("![a [b](c]) d](u)"), "<img src=\"u\" alt=\"a [b](c]) d\">");
    assert_eq!(html("![`](u)"), "<img src=\"u\" alt=\"`\">");
}

#[test]
fn backlink_skips_a_dropped_raw_block() {
    assert_eq!(
        html("x[^1]\n\n[^1]: a\n    ```=latex\n    \\x\n    ```"),
        "<p>x<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>a<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>"
    );
}
