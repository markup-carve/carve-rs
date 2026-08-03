//! `hard_break = '\', newline`, so its span covers BOTH characters.
//!
//! This engine placed the newline alone, leaving the backslash inside the text
//! node before it or in no node at all - a break that reports one character
//! where the construct is two. carve-js and carve-php both cover the pair
//! (carve#549).
//!
//! Nothing rendered differently, which is why it survived: a break is a `<br>`
//! whatever its span says, and the only checker that reads spans compares them
//! against source text for `text` nodes alone.

use carve::ast::{BlockNode, InlineNode};

fn document(source: &str) -> carve::ast::Document {
    let options = carve::Options {
        positions: true,
        ..Default::default()
    };
    carve::parse_with_options(source, &options)
}

fn slice(source: &str, pos: carve::ast::Pos) -> String {
    source
        .chars()
        .skip(pos.start_offset)
        .take(pos.end_offset - pos.start_offset)
        .collect()
}

fn first_hard_break(doc: &carve::ast::Document) -> carve::ast::Pos {
    for block in &doc.children {
        if let BlockNode::Paragraph(p) = block {
            for inline in &p.children {
                if let InlineNode::HardBreak(b) = inline {
                    return b.pos.expect("hard break carries no position");
                }
            }
        }
    }
    panic!("no hard break in the document");
}

#[test]
fn a_hard_break_span_covers_the_backslash_and_the_newline() {
    let source = format!("a{}\nb\n", '\\');
    let pos = first_hard_break(&document(&source));

    assert_eq!(slice(&source, pos), format!("{}\n", '\\'));
}

#[test]
fn a_hard_break_at_end_of_input_covers_its_backslash() {
    // `para\` at EOF is a hard break with no newline to cover, so the span is
    // the backslash alone - one character either way, and it was already right.
    let source = format!("a{}", '\\');
    let pos = first_hard_break(&document(&source));

    assert_eq!(slice(&source, pos), format!("{}", '\\'));
}

#[test]
fn the_text_before_a_hard_break_stops_at_the_backslash() {
    // The other half of the same question: if the break covers the backslash,
    // no other node may.
    let source = format!("ab{}\nc\n", '\\');
    let doc = document(&source);
    let BlockNode::Paragraph(p) = &doc.children[0] else {
        panic!("expected a paragraph");
    };
    let InlineNode::Text(text) = &p.children[0] else {
        panic!("expected text first");
    };

    assert_eq!(
        slice(&source, text.pos.expect("text carries no position")),
        "ab"
    );
}
