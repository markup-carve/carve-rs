#[test]
fn empty_markdown_headings_keep_their_elements() {
    for (source, expected) in [
        ("## \n#\n### ###\n", "<h2></h2>\n<h1></h1>\n<h3></h3>"),
        ("before\n#\nafter", "<p>before</p>\n<h1></h1>\n<p>after</p>"),
    ] {
        let converted = carve::markdown_to_carve(source);
        assert_eq!(carve::to_html(&converted).trim(), expected, "{source}");
    }
}
