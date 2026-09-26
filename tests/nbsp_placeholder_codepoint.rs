use carve::{ast::BlockNode, ast::InlineNode, parse, to_carve, to_html};

#[test]
fn escaped_spaces_are_nodes_and_private_use_characters_are_literal() {
    let source = "a\u{e000}\\ b\u{00a0}c\n";
    let doc = parse(source);
    let BlockNode::Paragraph(p) = &doc.children[0] else {
        panic!("paragraph")
    };
    assert!(matches!(p.children[1], InlineNode::NonBreakingSpace(_)));
    let InlineNode::Text(t) = &p.children[0] else {
        panic!("text")
    };
    assert_eq!(t.value, "a\u{e000}");
    assert_eq!(to_carve(source), source);
    assert_eq!(to_html(source), "<p>a\u{e000}&nbsp;b&nbsp;c</p>");
}

#[test]
fn line_block_indentation_uses_one_node_per_column() {
    let doc = parse("::: |\n  verse\n:::\n");
    let BlockNode::LineBlock(b) = &doc.children[0] else {
        panic!("line block")
    };
    let BlockNode::Paragraph(p) = &b.children[0] else {
        panic!("paragraph")
    };
    assert_eq!(
        p.children
            .iter()
            .filter(|n| matches!(n, InlineNode::NonBreakingSpace(_)))
            .count(),
        2
    );
}
