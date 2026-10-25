#[test]
fn markdown_destinations_escape_each_parenthesis() {
    for (source, expected) in [
        ("[link](a(b))", "[link](a\\(b\\))\n"),
        ("[link](a\\(b)", "[link](a\\(b)\n"),
        ("[link](a\\)b)", "[link](a\\)b)\n"),
        ("[link](a(b(c)))", "[link](a\\(b\\(c\\)\\))\n"),
    ] {
        assert_eq!(carve::to_markdown(source), expected, "{source}");
    }
}
