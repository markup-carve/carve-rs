#[test]
fn plus_after_a_lazy_quote_line_attaches_an_empty_quote() {
    for lazy in ["x", "  x"] {
        let source = format!("> a\n{lazy}\n+\n>");
        let expected =
            "<blockquote>\n  <p>a\nx</p>\n  <blockquote>\n\n  </blockquote>\n</blockquote>";
        assert_eq!(carve::to_html(&source), expected, "{source}");
    }
}
