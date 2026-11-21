//! C2-H-168: static mode writes core math as interactive mode does, inside a
//! list item too (carve-rs#4333).

#[test]
fn display_math_in_a_list_item_is_a_math_span_in_static_mode() {
    let response = carve::api::json::call(
        r#"{"call":"render","source":"- $$`x`","options":{"mode":"static"}}"#,
    );
    assert_eq!(
        response,
        r#"{"ok":{"html":"<ul>\n  <li><span class=\"math display\" role=\"math\">\\[x\\]</span></li>\n</ul>"}}"#
    );
}
