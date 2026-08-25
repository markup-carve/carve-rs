//! Explicit ids/classes preserve digit-leading HTML values. Attribute keys and
//! booleans keep the narrower identifier grammar.

#[test]
fn digit_first_explicit_id_and_class_are_valid() {
    assert_eq!(
        carve::to_html("[x]{.123}"),
        "<p><span class=\"123\">x</span></p>"
    );
    assert_eq!(
        carve::to_html("[x]{#1a}"),
        "<p><span id=\"1a\">x</span></p>"
    );
    assert!(!carve::to_html("[x]{123=v}").contains("<span"));
}

#[test]
fn digit_first_block_class_attaches() {
    assert_eq!(carve::to_html("{.123}\np"), "<p class=\"123\">p</p>");
}

#[test]
fn invalid_char_after_first_character_stays_literal() {
    // The identifier rule constrains every character, not just the first: a
    // non-identifier char anywhere (`!`, `.`, ...) invalidates the whole block.
    for src in ["[x]{.a!b}", "[x]{a!b=v}", "[x]{.ok .-1}"] {
        let html = carve::to_html(src);
        assert!(!html.contains("<span"), "{src:?} should be literal: {html}");
    }
}

#[test]
fn digit_after_first_character_is_valid() {
    assert_eq!(
        carve::to_html("[x]{.a1 #b2 k3=v}"),
        "<p><span class=\"a1\" id=\"b2\" k3=\"v\">x</span></p>"
    );
}
