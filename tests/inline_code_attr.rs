//! Inline attributes attach to a code span (`` `code`{.cls} ``), matching
//! carve-php / carve-js. The `{=html}` / `{=latex}` raw-inline form is separate.

#[test]
fn class_on_inline_code() {
    assert_eq!(
        carve::to_html("`code`{.cls}"),
        "<p><code class=\"cls\">code</code></p>"
    );
}

#[test]
fn id_and_classes_on_inline_code() {
    assert_eq!(
        carve::to_html("`x`{#i .a .b}"),
        "<p><code id=\"i\" class=\"a b\">x</code></p>"
    );
}

#[test]
fn raw_inline_form_is_unaffected() {
    assert_eq!(carve::to_html("Use `<br>`{=html} ok"), "<p>Use <br> ok</p>");
}
