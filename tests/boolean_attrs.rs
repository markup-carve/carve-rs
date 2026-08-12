//! Boolean (bare-word) attributes: a name with no value in a `{…}` block is a
//! value-less attribute, rendered `name=""` (the djot-php / carve-php form).

fn html(src: &str) -> String {
    carve::to_html(src).trim().to_string()
}

#[test]
fn inline_span() {
    assert_eq!(
        html("[x]{disabled}"),
        r#"<p><span disabled="">x</span></p>"#
    );
}

#[test]
fn block_attribute_line() {
    assert_eq!(html("{disabled}\nText"), r#"<p disabled="">Text</p>"#);
}

#[test]
fn mixes_with_class_keeps_both() {
    assert_eq!(
        html("[x]{.c disabled}"),
        r#"<p><span class="c" disabled="">x</span></p>"#,
    );
}

#[test]
fn mixes_with_key_value() {
    assert_eq!(
        html("[x]{disabled k=v}"),
        r#"<p><span disabled="" k="v">x</span></p>"#,
    );
}

#[test]
fn multiple_bare_words() {
    // `kbd` is reserved sugar (PART 10 §10), so it becomes the element rather
    // than staying an attribute; `foo` is an ordinary boolean attribute and
    // stays on the outer span. This case used to render both as attributes.
    assert_eq!(
        html("[x]{kbd foo}"),
        r#"<p><span foo=""><kbd>x</kbd></span></p>"#
    );
}

#[test]
fn a_bare_word_outside_the_semantic_registry_stays_an_attribute() {
    assert_eq!(
        html("[x]{foo bar}"),
        r#"<p><span foo="" bar="">x</span></p>"#
    );
}

#[test]
fn digit_first_bare_word_stays_literal() {
    assert_eq!(html("[x]{2bad}"), "<p>[x]{2bad}</p>");
}
