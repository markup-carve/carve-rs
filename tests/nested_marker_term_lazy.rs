use carve::to_html;

#[test]
fn lazy_line_stays_with_nested_marker_line_term() {
    for spaces in ["", " ", "  ", "   "] {
        let source = format!("- - :: t\n{spaces}: a\n");
        let html = to_html(&source);
        if spaces == "  " {
            assert!(html.contains("<dt>t</dt>"), "{source:?}: {html}");
            assert!(!html.contains("<dt>t\n: a</dt>"), "{source:?}: {html}");
        } else {
            assert!(html.contains("<dt>t\n: a</dt>"), "{source:?}: {html}");
        }
    }
    let reached = to_html("- - :: t\n    : a\n");
    assert!(reached.contains("<dd>"), "{reached}");
    for depth in 3..=4 {
        let source = format!("{}:: t\n: a\n", "- ".repeat(depth));
        let html = to_html(&source);
        assert!(html.contains("<dt>t\n: a</dt>"), "{source:?}: {html}");
        assert!(!html.contains("&nbsp;L&nbsp;"), "{source:?}: {html}");
    }
}
