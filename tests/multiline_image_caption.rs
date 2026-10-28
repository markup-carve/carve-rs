use carve::to_html;

#[test]
fn numbered_caption_follows_a_multiline_image_label() {
    for source in [
        " ![a\nb](c)\n^ #\n",
        ">  ![a\n> b](c)\n> ^ #\n",
        "- item\n\n   ![a\n  b](c)\n  ^ #\n",
    ] {
        let html = to_html(source);
        assert!(html.contains("<figure>"), "{source:?}: {html}");
        assert!(html.contains("<img src=\"c\""), "{source:?}: {html}");
        assert!(
            html.contains("<figcaption>1</figcaption>"),
            "{source:?}: {html}"
        );
    }
}

#[test]
fn malformed_image_opener_keeps_a_caption_line_as_text() {
    let html = to_html(" ![a\nb\n^ #\n");
    assert!(!html.contains("<figure>"), "{html}");
    assert!(html.contains("^ #"), "{html}");
}
