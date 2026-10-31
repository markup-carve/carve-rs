#[test]
fn a_quoted_brace_in_an_attribute_value_needs_no_extra_escape() {
    let source = "{~p~>[x](/u){k=\"~}\"}b~}\n";
    let written = carve::to_carve(source);
    assert_eq!(written, source);
    assert_eq!(carve::to_html(&written), carve::to_html(source));
    assert_eq!(carve::to_carve(&written), written);
}
