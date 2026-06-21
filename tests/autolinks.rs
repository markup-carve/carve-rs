#[test]
fn dangerous_scheme_angle_autolink_is_recognized_then_sanitized() {
    assert_eq!(
        carve::to_html("<vbscript:msgbox>"),
        "<p><a href=\"\">vbscript:msgbox</a></p>"
    );
}
