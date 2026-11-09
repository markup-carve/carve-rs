use carve::to_html;

#[test]
fn a_figure_numbers_target_notes_before_caption_notes() {
    let html = to_html("> [^n]\n^ [^n]\n\n[^n]: x\n");
    let body = html.find("<blockquote>").unwrap();
    let caption = html.find("<figcaption>").unwrap();
    assert!(html[body..caption].contains("id=\"fnref1\""), "{html}");
    assert!(html[caption..].contains("id=\"fnref1-2\""), "{html}");
}
