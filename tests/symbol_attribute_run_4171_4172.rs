use carve::to_html;

#[test]
fn symbol_consumes_all_glued_attribute_blocks() {
    assert_eq!(
        to_html(":s:{.a}{.b}"),
        "<p><span class=\"a b\">:s:</span></p>"
    );
}

#[test]
fn symbol_attributes_are_opaque_to_highlight_pairing() {
    for source in ["=:s:{t=x}=", "=:s:{t=\"x\"}="] {
        assert_eq!(
            to_html(source),
            "<p><mark><span t=\"x\">:s:</span></mark></p>",
            "{source}"
        );
    }
}
