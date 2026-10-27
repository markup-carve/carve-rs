use carve::ast::BlockNode;

#[test]
fn a_hoisted_link_definition_leaves_its_quote_in_a_footnote() {
    let source = "[^n]: a\n    > [r]: /x\n";
    let document = carve::parse(source);
    let body = document
        .footnote_defs
        .get("n")
        .expect("footnote definition");
    assert!(matches!(&body[0], BlockNode::Paragraph(_)));
    assert!(matches!(&body[1], BlockNode::BlockQuote(quote) if quote.children.is_empty()));
    assert!(carve::to_carve(source).contains("\n  >\n"));
}
