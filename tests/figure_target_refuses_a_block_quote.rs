use carve::{from_json, render_html};

const FIGURE_WITH_BLOCK_QUOTE: &str = r#"{"type":"document","srcByteLength":0,"children":[{"type":"figure","target":{"type":"block_quote","children":[{"type":"paragraph","children":[{"type":"text","value":"To be"}]}]},"caption":[{"type":"text","value":"Hamlet"}]}]}"#;

const FIGURE_WITH_IMAGE: &str = r#"{"type":"document","srcByteLength":0,"children":[{"type":"figure","target":{"type":"image","src":"/hamlet.png","alt":"Hamlet"},"caption":[{"type":"text","value":"The prince"}]}]}"#;

#[test]
fn a_figure_targeting_a_block_quote_is_refused() {
    let err = from_json(FIGURE_WITH_BLOCK_QUOTE)
        .expect_err("a figure whose target is a block_quote was accepted");
    let message = err.to_string();
    assert!(message.contains("block_quote"), "wrong message: {message}");
    assert!(
        message.contains("code_block, image, paragraph, table"),
        "the message does not name the admitted set: {message}"
    );
}

#[test]
fn a_figure_targeting_an_image_still_decodes_and_renders() {
    let doc = from_json(FIGURE_WITH_IMAGE).expect("a figure targeting an image was refused");
    assert_eq!(
        render_html(&doc).expect("the image figure exceeded the render ceiling"),
        "<figure>\n  <img src=\"/hamlet.png\" alt=\"Hamlet\">\n  <figcaption>The prince</figcaption>\n</figure>"
    );
}

#[test]
fn a_captioned_quote_in_source_stays_a_quote_with_an_attribution() {
    assert_eq!(
        carve::to_html("> To be\n^ Hamlet"),
        "<blockquote>\n  <p>To be</p>\n  <footer>Hamlet</footer>\n</blockquote>"
    );
}
