use carve::to_html;

#[test]
fn references_use_the_logical_cell_pipe() {
    for (source, cell) in [
        ("| [a\\|b][m] |\n", "[a|b][m]"),
        ("| ![a\\|b][m] |\n", "![a|b][m]"),
        ("| [a\\|b][] |\n", "[a|b][]"),
        ("| [`a\\|b`][m] |\n", "[`a\\|b`][m]"),
        ("[r|s]: /u\n\n| [x][r\\|s] |\n", "<a href=\"/u\">x</a>"),
        (
            "[r|s]: /u\n\n| ![x][r\\|s] |\n",
            "<img src=\"/u\" alt=\"x\">",
        ),
    ] {
        let expected =
            format!("<table>\n  <tbody>\n    <tr><td>{cell}</td></tr>\n  </tbody>\n</table>");
        assert_eq!(to_html(source), expected, "{source:?}");
    }
}
