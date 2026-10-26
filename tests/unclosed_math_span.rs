#[test]
fn math_uses_unclosed_and_empty_code_spans() {
    for source in ["$`", "$`\n", "$`\r", "$``", "$```"] {
        assert_eq!(
            carve::to_html(source).trim(),
            "<p><span class=\"math inline\" role=\"math\">\\(\\)</span></p>",
            "source: {source:?}"
        );
    }
    for source in ["$$`", "$$``"] {
        assert_eq!(
            carve::to_html(source).trim(),
            "<p><span class=\"math display\" role=\"math\">\\[\\]</span></p>",
            "source: {source:?}"
        );
    }
}
