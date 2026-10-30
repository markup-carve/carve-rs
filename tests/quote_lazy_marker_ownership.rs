#[test]
fn plus_after_a_lazy_quote_line_above_a_blank_level_is_a_letter() {
    for lazy in ["x", "  x"] {
        let source = format!("> a\n{lazy}\n+\n>");
        let expected = "<blockquote><p>a\nx\n+</p></blockquote>";
        assert_eq!(carve::to_html(&source), expected, "{source}");
    }
}
