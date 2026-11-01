#[test]
fn image_declined_at_comment_bracket_falls_back_to_link() {
    for source in ["![a{% ] %}b](/u)\n", "![a{% ] %}b][r]\n\n[r]: /u\n"] {
        assert_eq!(
            carve::to_html(source).trim(),
            "<p>!<a href=\"/u\">ab</a></p>",
            "{source}"
        );
    }
}

#[test]
fn image_with_comment_without_bracket_stays_an_image() {
    assert_eq!(
        carve::to_html("![a{% c %}b](/u)\n").trim(),
        "<img src=\"/u\" alt=\"a{% c %}b\">"
    );
}

#[test]
fn image_with_tail_at_comment_bracket_keeps_image_close() {
    assert_eq!(
        carve::to_html("![a{% ](/v) %}b](/u)\n").trim(),
        "<p><img src=\"/v\" alt=\"a{% \"> %}b](/u)</p>"
    );
}
