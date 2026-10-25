#[test]
fn literal_backslashes_follow_the_reparse_rule() {
    for source in ["\\a and \\« and a\\b\n", "a\\\\b\n", "a\\!\n", "a\\\n"] {
        let written = carve::to_carve(source);
        assert_eq!(written, source, "{source:?}");
        assert_eq!(carve::to_carve(&written), written);
    }
}

#[test]
fn padded_table_cell_drops_an_idle_marker_escape() {
    let source = "|\\= a |\n";
    let expected = "| = a |\n";
    assert_eq!(carve::to_carve(source), expected);
    assert_eq!(carve::to_carve(expected), expected);
}
