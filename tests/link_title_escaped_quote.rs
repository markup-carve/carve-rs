//! A backslash-escaped quote inside a link/image title is a literal quote, not
//! the title terminator. Matches carve-php and carve-js.

#[test]
fn escaped_quote_in_link_title() {
    assert_eq!(
        carve::to_html("[t](u \"a \\\"b\\\" c\")"),
        "<p><a href=\"u\" title=\"a &quot;b&quot; c\">t</a></p>"
    );
}

#[test]
fn escaped_quote_in_image_title() {
    assert_eq!(
        carve::to_html("![a](i \"t\\\"i\")"),
        "<img src=\"i\" alt=\"a\" title=\"t&quot;i\">"
    );
}

#[test]
fn plain_title_unaffected() {
    assert_eq!(
        carve::to_html("[t](u \"plain\")"),
        "<p><a href=\"u\" title=\"plain\">t</a></p>"
    );
}
