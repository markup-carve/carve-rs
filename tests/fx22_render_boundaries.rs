#[test]
fn math_uses_the_html_text_escape_rule() {
    for source in ["$`x\u{a0}y`", "$$`x\u{a0}y`"] {
        let html = carve::to_html(source);
        assert!(html.contains("&nbsp;"), "{source:?}: {html}");
        assert!(!html.contains('\u{a0}'), "{source:?}: {html}");
    }
}

#[test]
fn authored_footnote_carrier_survives_default_html() {
    let html = carve::to_html("[^a]{data-carve-footnote-label=z}\n[^a]: n");
    assert!(html.contains("data-carve-footnote-label=\"z\""), "{html}");
}
