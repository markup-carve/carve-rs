use carve::to_html;

#[test]
fn a_captioned_quote_promotes_its_lone_image() {
    assert_eq!(
        to_html("> ![a](x)\n^ c\n").trim(),
        "<figure>\n  <blockquote>\n    <img src=\"x\" alt=\"a\">\n  </blockquote>\n  <figcaption>c</figcaption>\n</figure>"
    );
}
