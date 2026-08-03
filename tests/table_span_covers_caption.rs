//! A table's span covers its caption, because the caption is one of its
//! children.
//!
//! The span was taken BEFORE the caption was consumed - struct-field order, in
//! one expression - so the table stopped at its last row and the caption's
//! inlines sat outside their own parent (carve#565). carve-js covers both;
//! carve-php did not either, until markup-carve/carve-php#686.
//!
//! Nothing rendered differently: a span is compared against source text for
//! `text` nodes alone, so a block's span is checked for being present and in
//! range and never for containing what the block contains.

use carve::ast::{BlockNode, InlineNode};

fn document(source: &str) -> carve::ast::Document {
    let options = carve::Options {
        positions: true,
        ..Default::default()
    };
    carve::parse_with_options(source, &options)
}

fn table_of(source: &str) -> carve::ast::Table {
    let doc = document(source);
    match &doc.children[0] {
        BlockNode::Table(t) => t.clone(),
        other => panic!("expected a table first, got {other:?}"),
    }
}

fn inline_pos(node: &InlineNode) -> Option<carve::ast::Pos> {
    match node {
        InlineNode::Text(n) => n.pos,
        InlineNode::Emphasis(n) => n.pos,
        InlineNode::CaptionNumber(n) => n.pos,
        _ => None,
    }
}

#[test]
fn the_table_span_reaches_the_end_of_its_caption() {
    let source = "| a |\n^ cap\n";
    let table = table_of(source);
    let pos = table.pos.expect("the table carries no position");
    let caption = table.caption.expect("no caption");
    let last = inline_pos(caption.last().expect("empty caption")).expect("caption inline unplaced");

    assert_eq!(pos.end_offset, last.end_offset);
}

#[test]
fn every_caption_inline_is_inside_the_table_span() {
    let source = "|= H |\n| a |\n^ a /slanted/ caption\n";
    let table = table_of(source);
    let pos = table.pos.expect("the table carries no position");

    for inline in table.caption.expect("no caption").iter() {
        let Some(child) = inline_pos(inline) else {
            continue;
        };
        assert!(
            child.start_offset >= pos.start_offset && child.end_offset <= pos.end_offset,
            "caption inline [{}, {}] is outside its table [{}, {}]",
            child.start_offset,
            child.end_offset,
            pos.start_offset,
            pos.end_offset,
        );
    }
}

#[test]
fn a_table_without_a_caption_is_unchanged() {
    let table = table_of("| a |\n");
    let pos = table.pos.expect("the table carries no position");

    assert_eq!((pos.start_offset, pos.end_offset), (0, 5));
}
