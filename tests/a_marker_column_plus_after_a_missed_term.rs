use carve::to_html;

#[test]
fn a_marker_column_plus_attaches_after_a_missed_quote_term() {
    for indent in ["", " "] {
        let source = format!("{indent}. > :: t\n{indent}:  # H\n{indent}+\n{indent}:::\n");
        let html = to_html(&source);
        assert!(html.contains("    <div>"), "{source:?}: {html}");
        assert!(!html.contains("<p>+</p>"), "{source:?}: {html}");
        assert!(!html.contains("</ol>\n<div>"), "{source:?}: {html}");
    }
}
