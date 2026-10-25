use carve::to_markdown;

#[test]
fn markdown_links_and_images_percent_encode_destination_pipes() {
    let source = "[link](/a|b) ![image](/p|q.png)\n";
    assert_eq!(to_markdown(source), "[link](/a%7Cb) ![image](/p%7Cq.png)\n");
}
