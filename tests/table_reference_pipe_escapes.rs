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
        assert_eq!(to_html(&carve::to_carve(source)), expected, "{source:?}");
    }
}

#[test]
fn imported_collapsed_reference_keeps_its_cell_on_round_trip() {
    let source = carve::migrate_djot("| [a\\|b][] |\n|---|\n\n[a|b]: /u\n").value;
    let before = to_html(&source);
    assert!(before.contains("href=\"/u\">a|b</a>"));
    assert_eq!(to_html(&carve::to_carve(&source)), before);
}
