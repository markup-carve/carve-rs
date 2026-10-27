use carve::to_html;

#[test]
fn a_wrapped_attribute_without_the_quote_prefix_stays_text() {
    assert_eq!(
        to_html("> {.a\n.b}"),
        "<blockquote><p>{.a\n.b}</p></blockquote>"
    );
}
