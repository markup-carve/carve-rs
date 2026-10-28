use carve::to_html;

#[test]
fn indented_images_number_the_first_bare_caption_marker() {
    for spaces in 1..=3 {
        let source = format!("{}![a](/u)\n^ Figure #\n", " ".repeat(spaces));
        let html = to_html(&source);
        assert!(
            html.contains("<figcaption>Figure 1</figcaption>"),
            "{source:?}: {html}"
        );
    }
}
