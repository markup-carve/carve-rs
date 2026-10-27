#[test]
fn unresolved_reference_image_stops_at_second_caption_line() {
    assert_eq!(
        carve::to_html("![a][r]\n^ cap\n^ cap"),
        "<p>![a][r]\n^ cap</p>\n<p>^ cap</p>"
    );
    assert_eq!(
        carve::to_html("![a][r]\n^ cap\nmore\n^ next"),
        "<p>![a][r]\n^ cap\nmore</p>\n<p>^ next</p>"
    );
}

#[test]
fn later_reference_resolution_keeps_same_caption_boundary() {
    assert_eq!(
        carve::to_html("![a][r]\n^ cap\n^ cap\n\n[r]: u"),
        "<figure>\n  <img src=\"u\" alt=\"a\">\n  <figcaption>cap</figcaption>\n</figure>\n<p>^ cap</p>"
    );
}
