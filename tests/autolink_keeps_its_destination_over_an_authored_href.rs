use carve::to_html;

#[test]
fn autolink_keeps_its_destination_over_an_authored_href() {
    for source in [
        "<https://ok/>{href=\"javascript:y\"}",
        "<https://ok/>{href=\"/v\"}",
    ] {
        assert_eq!(to_html(source).trim(), "<p><a href=\"https://ok/\">https://ok/</a></p>");
    }
    assert_eq!(
        to_html("<https://ok/>{href=\"/v\" .c}").trim(),
        "<p><a href=\"https://ok/\" class=\"c\">https://ok/</a></p>"
    );
}
