fn html(source: &str) -> String {
    carve::to_html(source)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace("> <", "><")
}

#[test]
fn a_marked_quote_line_in_a_note_registers_a_nested_definition() {
    // The later #5012 ruling keeps footnote definitions block-level, and
    // CARVE-P9R-002 makes them interrupt a quoted open paragraph.
    let expected = "<p>[r][] <a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a><a id=\"fnref2\" href=\"#fn2\" role=\"doc-noteref\"><sup>2</sup></a> [^d]</p><section role=\"doc-endnotes\" aria-label=\"Footnotes\"><hr><ol><li id=\"fn1\"><p>n<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p></li><li id=\"fn2\"><blockquote><p>para</p></blockquote><p><a href=\"#fnref2\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p></li></ol></section>";
    for spaces in ["  ", "     "] {
        let source = format!("[r][] [^p] [^c] [^d]\n[^c]: > para\n  >{spaces}[^p]: n\n");
        assert_eq!(html(&source), expected, "{source:?}");
    }
}
