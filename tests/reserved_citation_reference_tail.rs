use carve::{parse_with_options, to_html_with_options, BlockNode, Citations, InlineNode, Options};

#[test]
fn reserved_reference_tails_are_literal_with_or_without_citations() {
    let citations = Citations::new();
    for options in [Options::new(), Options::new().with_extension(&citations)] {
        for source in ["[t][@a]", "![t][@a]", "[][@a]", "![][@a]"] {
            let doc = parse_with_options(source, &options);
            let BlockNode::Paragraph(paragraph) = &doc.children[0] else {
                panic!("{source}")
            };
            assert_eq!(paragraph.children.len(), 1, "{source}");
            assert!(
                matches!(&paragraph.children[0], InlineNode::Text(text) if text.value == source),
                "{source}: {:?}",
                paragraph.children
            );
            assert_eq!(
                to_html_with_options(source, &options).trim(),
                format!("<p>{source}</p>")
            );
        }
    }
}

#[test]
fn ordinary_labels_and_standalone_citations_still_parse() {
    let doc = parse_with_options("[t][a] and ![t][a]", &Options::new());
    let BlockNode::Paragraph(paragraph) = &doc.children[0] else {
        panic!()
    };
    assert!(paragraph
        .children
        .iter()
        .any(|node| matches!(node, InlineNode::Link(_))));
    assert!(paragraph
        .children
        .iter()
        .any(|node| matches!(node, InlineNode::Image(_))));

    let citations = Citations::new();
    let doc = parse_with_options("[@a]", &Options::new().with_extension(&citations));
    let BlockNode::Paragraph(paragraph) = &doc.children[0] else {
        panic!()
    };
    assert!(matches!(
        &paragraph.children[0],
        InlineNode::CitationGroup(_)
    ));
}
