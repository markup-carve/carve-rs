use carve::to_html;

#[test]
fn caret_below_note_column_remains_paragraph_text() {
    let html = to_html("[r][] [^p] [^c] [^d]\n\n- [^c]: para\n    more\n^ cap\ntail\n");
    assert!(
        html.contains("<p>para\nmore\n^ cap\ntail<a href=\"#fnref1\""),
        "{html}"
    );
    assert!(!html.contains("&nbsp;L&nbsp;"), "{html}");
}
