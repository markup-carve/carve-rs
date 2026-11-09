use carve::to_html;

#[test]
fn over_indented_image_stays_in_the_open_item_paragraph() {
    for source in ["- b\n\t![a](x.png)\n", "- b\n   ![a](x.png)\n"] {
        assert_eq!(
            to_html(source),
            "<ul>\n  <li>b\n<img src=\"x.png\" alt=\"a\"></li>\n</ul>",
            "{source:?}"
        );
    }
}
