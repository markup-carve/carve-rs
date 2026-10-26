use carve::{BlockNode, Citations, Options};

#[test]
fn titles_accept_ascii_padding_after_either_destination() {
    for (source, expected) in [
        ("[t]( \"T\"  )", "<p><a href=\"\" title=\"T\">t</a></p>"),
        ("[t](/u \"T\" )", "<p><a href=\"/u\" title=\"T\">t</a></p>"),
        ("[t](/u 'T'   )", "<p><a href=\"/u\" title=\"T\">t</a></p>"),
        (
            "![t](/img \"T\"  )",
            "<img src=\"/img\" alt=\"t\" title=\"T\">",
        ),
    ] {
        assert_eq!(carve::to_html(source), expected, "{source}");
    }
}

#[test]
fn tab_is_not_title_padding() {
    assert!(!carve::to_html("[t](/u \"T\"\t)").contains("<a "));
}

#[test]
fn citation_definition_requires_ascii_space_run() {
    let citations = Citations::new();
    let options = Options::new().with_extension(&citations);
    let accepted = carve::parse_with_options("[@a]\n\n[@a]:  Alpha.", &options);
    assert!(accepted
        .children
        .iter()
        .any(|node| matches!(node, BlockNode::CitationDefinition(_))));
    for source in ["[@a]:\tAlpha.", "[@a]:Alpha."] {
        let parsed = carve::parse_with_options(source, &options);
        assert!(
            !parsed
                .children
                .iter()
                .any(|node| matches!(node, BlockNode::CitationDefinition(_))),
            "{source}"
        );
    }
}
