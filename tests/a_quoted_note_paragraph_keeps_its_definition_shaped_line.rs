fn html(source: &str) -> String {
    carve::to_html(source)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace("> <", "><")
}

#[test]
fn a_marked_quote_line_in_a_note_keeps_a_definition_as_text() {
    let expected = "<p>[r][] [^p] <a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a> [^d]</p><section role=\"doc-endnotes\" aria-label=\"Footnotes\"><hr><ol><li id=\"fn1\"><blockquote><p>para [^p]: n</p></blockquote><p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p></li></ol></section>";
    for spaces in ["  ", "     "] {
        let source = format!("[r][] [^p] [^c] [^d]\n[^c]: > para\n  >{spaces}[^p]: n\n");
        assert_eq!(html(&source), expected, "{source:?}");
    }
}
