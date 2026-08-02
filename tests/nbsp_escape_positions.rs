//! A text run holding a no-break-space escape carried no position
//! (carve-rs#333).
//!
//! `flush_text` measured the span as `start + buf.len()`, which is the BUFFER's
//! length, not the source run's. Two source bytes become one placeholder
//! character, so that arithmetic was wrong here - and the branch refused a
//! position rather than fixing the arithmetic.
//!
//! Refusing was the wrong call. The span covers exactly the source the run came
//! from, which is what the reference publishes for the same input; only the
//! VALUE differs from the slice, and the conformance checker already exempts a
//! slice containing a backslash for precisely this reason.

use carve::ast::{BlockNode, InlineNode};

fn first_text(source: &str) -> carve::ast::Text {
    let options = carve::Options {
        positions: true,
        ..Default::default()
    };
    let doc = carve::parse_with_options(source, &options);
    let BlockNode::Paragraph(para) = &doc.children[0] else {
        panic!("expected a paragraph");
    };
    for node in &para.children {
        if let InlineNode::Text(text) = node {
            return text.clone();
        }
    }
    panic!("no text node");
}

#[test]
fn a_run_holding_an_escape_is_placed_over_its_source() {
    let source = format!("say{} x\n", '\\');
    let text = first_text(&source);
    let pos = text.pos.expect("the run must carry a position");

    // `say\ x` is six codepoints; the value is five, because the escape
    // resolved. The SPAN is the source extent, not the value's length.
    assert_eq!(pos.start_offset, 0);
    assert_eq!(pos.end_offset, 6);
    assert_eq!(text.value.chars().count(), 5);
}

/// Two escapes in one run compound the difference, so a fix that subtracted a
/// fixed one would pass the single case and fail here.
#[test]
fn two_escapes_in_one_run_compound() {
    let bs = '\\';
    let source = format!("a{} b{} c\n", bs, bs);
    let text = first_text(&source);
    let pos = text.pos.expect("the run must carry a position");

    // `a\ b\ c` is seven codepoints; the value is five.
    assert_eq!((pos.start_offset, pos.end_offset), (0, 7));
    assert_eq!(text.value.chars().count(), 5);
}

/// A run with no escape must be unaffected - the delta has to reset per run, or
/// the next span inherits the previous one's correction.
#[test]
fn a_plain_run_after_an_escaped_one_is_still_exact() {
    let source = format!("a{} b\n\nplain text\n", '\\');
    let options = carve::Options {
        positions: true,
        ..Default::default()
    };
    let doc = carve::parse_with_options(&source, &options);
    let BlockNode::Paragraph(para) = &doc.children[1] else {
        panic!("expected the second paragraph");
    };
    let InlineNode::Text(text) = &para.children[0] else {
        panic!("expected a text node");
    };
    let pos = text.pos.expect("the plain run must carry a position");

    let slice: String = source
        .chars()
        .skip(pos.start_offset)
        .take(pos.end_offset - pos.start_offset)
        .collect();
    assert_eq!(slice, text.value, "the delta leaked into the next run");
}
