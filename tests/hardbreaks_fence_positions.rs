//! A colon fence whose opener carries a lone backslash makes every newline in
//! it a hard break. Nothing in one could be placed (carve-rs#333).
//!
//! Three separate causes in one function, and they hid each other:
//!
//! 1. The div refused a span, on the stated grounds that it was a synthesized
//!    wrapper. That had it backwards - the `.hardbreaks` class is synthesized,
//!    the fence is not. The author wrote both delimiters.
//! 2. The body lines were pushed with no stripped-column record, so `span_of`
//!    refused every block inside, exactly as it did for ordinary colon fences
//!    before carve-rs#350.
//! 3. Converting a soft break to a hard one built a FRESH node, dropping the
//!    span. Invisible in output, because the two render identically here.

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

/// The real fixture spelling, kept in one place so a typo cannot make these
/// tests silently exercise an ordinary div instead.
fn fixture() -> String {
    format!("::: {}\none\ntwo\n:::\n", '\\')
}

#[test]
fn the_fence_carries_a_span_over_both_delimiters() {
    let source = fixture();
    let doc = document(&source);
    let BlockNode::Div(div) = &doc.children[0] else {
        panic!("expected the hardbreaks div, got {:?}", doc.children[0]);
    };

    let pos = div.pos.expect("the fence must carry a position");
    let text = slice(&source, pos);
    assert!(
        text.starts_with(":::"),
        "the span misses the opener: {text:?}"
    );
    assert!(
        text.trim_end().ends_with(":::"),
        "the span misses the closer: {text:?}"
    );
}

#[test]
fn a_paragraph_inside_the_fence_is_placed() {
    let source = fixture();
    let doc = document(&source);
    let BlockNode::Div(div) = &doc.children[0] else {
        panic!("expected the hardbreaks div");
    };
    let BlockNode::Paragraph(para) = &div.children[0] else {
        panic!("expected a paragraph inside the fence");
    };

    let pos = para
        .pos
        .expect("a block inside the fence must carry a position");
    assert_eq!(slice(&source, pos), "one\ntwo");
}

/// The break's span is the one a fresh node threw away. It is also the case a
/// rendering comparison cannot catch, since a soft and a hard break look the
/// same in this block.
#[test]
fn the_converted_hard_break_keeps_its_span() {
    let source = fixture();
    let doc = document(&source);
    let BlockNode::Div(div) = &doc.children[0] else {
        panic!("expected the hardbreaks div");
    };
    let BlockNode::Paragraph(para) = &div.children[0] else {
        panic!("expected a paragraph");
    };

    let breaks: Vec<&InlineNode> = para
        .children
        .iter()
        .filter(|n| matches!(n, InlineNode::HardBreak(_)))
        .collect();
    assert_eq!(breaks.len(), 1, "the newline did not become a hard break");

    let InlineNode::HardBreak(brk) = breaks[0] else {
        unreachable!()
    };
    let pos = brk.pos.expect("the converted break must keep its span");
    assert_eq!(slice(&source, pos), "\n");
}

/// Every inline in the fence is placed and slices back to itself, so a fix that
/// placed the wrapper while leaving its content bare would not pass.
#[test]
fn every_inline_in_the_fence_slices_back() {
    let source = fixture();
    let doc = document(&source);
    let BlockNode::Div(div) = &doc.children[0] else {
        panic!("expected the hardbreaks div");
    };
    let BlockNode::Paragraph(para) = &div.children[0] else {
        panic!("expected a paragraph");
    };

    let mut checked = 0usize;
    for node in &para.children {
        if let InlineNode::Text(text) = node {
            let pos = text.pos.expect("a text node in the fence must be placed");
            assert_eq!(slice(&source, pos), text.value);
            checked += 1;
        }
    }
    assert_eq!(checked, 2, "expected both verse lines");
}
