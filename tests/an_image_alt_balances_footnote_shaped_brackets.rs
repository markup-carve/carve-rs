use carve::to_html;

#[test]
fn an_image_alt_balances_footnote_shaped_brackets() {
    assert_eq!(to_html("![^[]()\n").trim(), "<p>![^[]()</p>");
    assert_eq!(to_html("![^[]](x)\n").trim(), "<img src=\"x\" alt=\"^[]\">");
}
