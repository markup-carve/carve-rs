//! An attribute name (id, class, key) is a grammar identifier and may not
//! start with a digit; a digit-first name makes the whole `{...}` an invalid
//! attribute block, so it stays literal (§14). Stricter than djot.

#[test]
fn digit_first_attribute_name_stays_literal() {
    for src in ["[x]{.123}", "[x]{123=v}", "x{.1a}"] {
        let html = carve::to_html(src);
        assert!(!html.contains("<span"), "{src:?} should be literal: {html}");
    }
}

#[test]
fn digit_first_block_attribute_line_stays_literal() {
    assert_eq!(carve::to_html("{.123}\np"), "<p>{.123}\np</p>");
}

#[test]
fn digit_after_first_character_is_valid() {
    assert_eq!(
        carve::to_html("[x]{.a1 #b2 k3=v}"),
        "<p><span class=\"a1\" id=\"b2\" k3=\"v\">x</span></p>"
    );
}
