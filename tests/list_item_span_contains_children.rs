//! A list item's span covers everything in the item, not just its marker line.
//!
//! Blocks attached to an item after its first - an indented paragraph, a nested
//! list, a quote, a `+` continuation - were pushed onto the item AFTER its span
//! was fixed, so the item claimed its marker line and every later block sat
//! outside it. 55 nodes across the spec corpus (carve#565).
//!
//! Nothing rendered differently, and no checker could see it: a span is
//! compared against source text for `text` nodes alone, so a block's span is
//! checked for being present and in range and never for containing what the
//! node contains.

use carve::ast::{BlockNode, Pos};

fn document(source: &str) -> carve::ast::Document {
    let options = carve::Options {
        positions: true,
        ..Default::default()
    };
    carve::parse_with_options(source, &options)
}

/// The first list item's span, and the spans of the blocks inside it.
fn item_and_children(source: &str) -> (Pos, Vec<Pos>) {
    let doc = document(source);
    let BlockNode::List(list) = &doc.children[0] else {
        panic!("expected a list first, got {:?}", doc.children[0]);
    };
    let item = list.items[0].pos.expect("list item carries no position");
    let children = list.items[0]
        .children
        .iter()
        .filter_map(child_pos)
        .collect();
    (item, children)
}

fn child_pos(node: &BlockNode) -> Option<Pos> {
    match node {
        BlockNode::Paragraph(n) => n.pos,
        BlockNode::List(n) => n.pos,
        BlockNode::BlockQuote(n) => n.pos,
        BlockNode::Heading(n) => n.pos,
        BlockNode::CodeBlock(n) => n.pos,
        BlockNode::Table(n) => n.pos,
        BlockNode::DefinitionList(n) => n.pos,
        BlockNode::Admonition(n) => n.pos,
        _ => None,
    }
}

#[track_caller]
fn assert_contains_children(source: &str) {
    let (item, children) = item_and_children(source);
    assert!(
        !children.is_empty(),
        "the item has no placed children, so this proves nothing"
    );
    for child in children {
        assert!(
            child.start_offset >= item.start_offset && child.end_offset <= item.end_offset,
            "child [{}, {}] is outside its item [{}, {}] for source {source:?}",
            child.start_offset,
            child.end_offset,
            item.start_offset,
            item.end_offset,
        );
    }
}

#[test]
fn an_indented_quote_is_inside_its_item() {
    assert_contains_children("1. one\n\n    > q\n");
}

#[test]
fn a_nested_list_is_inside_its_item() {
    assert_contains_children("- one\n\n  - inner\n");
}

#[test]
fn a_second_paragraph_is_inside_its_item() {
    assert_contains_children("- one\n\n  second\n");
}

#[test]
fn a_continuation_marker_block_is_inside_its_item() {
    assert_contains_children("- one\n+\nattached\n");
}

#[test]
fn an_item_with_only_its_marker_line_is_unchanged() {
    let (item, children) = item_and_children("- one\n");
    assert_eq!(item.start_offset, 0);
    assert_eq!(item.end_offset, 5);
    assert_eq!(children.len(), 1);
}

/// A captioned quote and its target both carry real, nested offsets.
#[test]
fn a_captioned_quote_carries_real_offsets() {
    let source = "Intro\n\n> Stay hungry\n^ Steve Jobs\n";
    let doc = document(source);
    let BlockNode::Figure(figure) = &doc.children[1] else {
        panic!("expected a figure second, got {:?}", doc.children[1]);
    };
    let carve::ast::FigureTarget::BlockQuote(quote) = &figure.target else {
        panic!("expected a block quote target");
    };
    let pos = figure.pos.expect("the figure carries no position");

    assert!(
        pos.end_offset > pos.start_offset,
        "figure span is empty: [{}, {}]",
        pos.start_offset,
        pos.end_offset,
    );
    // The figure span runs from the quote through its caption.
    for child in &quote.children {
        let child_pos = child_pos(child).expect("a child of the quote carries no position");
        assert!(
            child_pos.start_offset >= pos.start_offset && child_pos.end_offset <= pos.end_offset,
            "child [{}, {}] is outside its quote [{}, {}]",
            child_pos.start_offset,
            child_pos.end_offset,
            pos.start_offset,
            pos.end_offset,
        );
    }
}
