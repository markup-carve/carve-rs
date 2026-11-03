use carve::to_html;

#[test]
fn a_separator_below_a_nested_marker_line_term_is_text() {
    // A term is one line (carve#4861): the separator is text in the item it
    // reaches, or after the list when it reaches none.
    for spaces in ["", " ", "  ", "   "] {
        let source = format!("- - :: t\n{spaces}: a\n");
        let html = to_html(&source);
        assert!(html.contains("<dt>t</dt>"), "{source:?}: {html}");
        if spaces.len() >= 2 {
            assert!(html.contains("</ul>\n    : a\n  </li>"), "{source:?}: {html}");
        } else {
            assert!(html.ends_with("</ul>\n<p>: a</p>"), "{source:?}: {html}");
        }
    }
    let reached = to_html("- - :: t\n    : a\n");
    assert!(reached.contains("<dd>"), "{reached}");
    for depth in 3..=4 {
        let source = format!("{}:: t\n: a\n", "- ".repeat(depth));
        let html = to_html(&source);
        assert!(html.ends_with("</ul>\n<p>: a</p>"), "{source:?}: {html}");
        assert!(!html.contains("&nbsp;L&nbsp;"), "{source:?}: {html}");
    }
}
