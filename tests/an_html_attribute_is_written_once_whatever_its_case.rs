//! markup-carve/carve#4457 (PART 10 R1), corpus rows
//! 857-an-html-attribute-is-written-once-whatever-its-case.

#[test]
fn a_structural_attribute_wins_and_names_fold_by_case() {
    for (source, html) in [
        ("{type=text}\na. alpha\n", "<ol type=\"a\">\n  <li>alpha</li>\n</ol>"),
        ("{start=9}\n3. x\n", "<ol start=\"3\">\n  <li>x</li>\n</ol>"),
        ("{start=9}\n1. x\n", "<ol start=\"9\">\n  <li>x</li>\n</ol>"),
        ("![a](/u.png){alt=z}\n", "<img src=\"/u.png\" alt=\"a\">"),
        ("{K=1 k=2}\npara\n", "<p K=\"2\">para</p>"),
        (
            "{CLASS=z}\n::: |\nverse\n:::\n",
            "<div CLASS=\"z line-block\">\n  <p>verse</p>\n</div>",
        ),
        (
            "[x]{.a CLASS=b ID=c #d}\n",
            "<p><span class=\"a b\" ID=\"d\">x</span></p>",
        ),
        (
            "|>{style=\"color:red\"} a |\n",
            "<table>\n  <tbody>\n    <tr><td style=\"color:red; text-align: right;\">a</td></tr>\n  </tbody>\n</table>",
        ),
    ] {
        assert_eq!(carve::to_html(source), html, "{source:?}");
    }
}
