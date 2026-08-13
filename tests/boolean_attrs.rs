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
    // The consumed name renames the span and `foo` rides the element it was
    // written on (PART 9 §9).
    assert_eq!(html("[x]{kbd foo}"), r#"<p><kbd foo="">x</kbd></p>"#);
}

#[test]
fn digit_first_bare_word_stays_literal() {
    assert_eq!(html("[x]{2bad}"), "<p>[x]{2bad}</p>");
}
