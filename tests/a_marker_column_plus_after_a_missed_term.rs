use carve::to_html;

#[test]
fn a_separator_after_a_missed_quote_term_is_text() {
    for indent in ["", " "] {
        let source = format!("{indent}. > :: t\n{indent}:  # H\n{indent}+\n{indent}:::\n");
        let html = to_html(&source);
        // A term is one line (carve#4861): the separator misses the quote and
        // is text, so the `+` below it is too.
        assert!(
            html.contains("</ol>\n<p>:  # H\n+</p>\n<div>"),
            "{source:?}: {html}"
        );
    }
}
