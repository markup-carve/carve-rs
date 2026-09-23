//! A code span opened on an indented continuation line began at the
//! INDENTATION rather than at its backtick run, and the `soft_break` in front
//! of it stopped short by the same amount (carve-rs#1832).
//!
//! PART 12 §4 puts a span at the markup that opens the construct. The latitude
//! to begin part way into a line's leading indentation belongs to CONTAINERS,
//! whose indent run places a nested marker; a leaf has no marker to place.
//!
//! The indentation had gone to the code span because a description body frames
//! a fence-shaped line it refuses with the LAZY sentinel, and the sentinel's
//! three codepoints were being charged against the line's column. carve-js and
//! carve-php both start the span at the backtick.

use carve::ast::{BlockNode, InlineNode, Pos};

fn description_inlines(source: &str) -> Vec<InlineNode> {
    let options = carve::Options {
        positions: true,
        ..Default::default()
    };
    let doc = carve::parse_with_options(source, &options);
    let BlockNode::DefinitionList(list) = &doc.children[0] else {
        panic!("the fixture did not parse as a definition list");
    };
    match &list.items[0].definitions[0].children[0] {
        BlockNode::Paragraph(paragraph) => paragraph.children.clone(),
        other => panic!("expected a paragraph, got {other:?}"),
    }
}

fn soft_break_pos(node: &InlineNode) -> Pos {
    let InlineNode::SoftBreak(node) = node else {
        panic!("expected a soft break, got {node:?}");
    };
    node.pos.clone().expect("a soft break must be placed")
}

fn code_pos(node: &InlineNode) -> Pos {
    let InlineNode::Code(node) = node else {
        panic!("expected a code span, got {node:?}");
    };
    node.pos.clone().expect("a code span must be placed")
}

fn slice(source: &str, pos: &Pos) -> String {
    source
        .chars()
        .skip(pos.start_offset)
        .take(pos.end_offset - pos.start_offset)
        .collect()
}

/// Corpus 367, row 2: the fence has no closer, so it stays inline text.
#[test]
fn an_unterminated_fence_opens_its_span_at_the_backtick() {
    let source = ":: t\n:  a\n   ```\ntail\n";
    let inlines = description_inlines(source);

    let brk = soft_break_pos(&inlines[1]);
    assert_eq!(
        (brk.start_offset, brk.end_offset),
        (9, 13),
        "the break carries the indentation the code span used to take"
    );

    let code = code_pos(&inlines[2]);
    assert_eq!(
        (code.start_offset, code.end_offset),
        (13, 21),
        "the span opens at the backtick run, not at the three spaces in front of it"
    );
    assert_eq!(slice(source, &code), "```\ntail");
    assert_eq!((code.start_line, code.start_column), (3, 4));

    let InlineNode::Code(node) = &inlines[2] else {
        unreachable!("already matched above");
    };
    assert_eq!(node.value, "\ntail");
}

/// Corpus 450, row 3: a longer body, the same opening.
#[test]
fn the_span_opens_at_the_backtick_below_a_longer_description() {
    let source = ":: term\n:  definition\n   ```\n   c\ntail\n";
    let inlines = description_inlines(source);

    let brk = soft_break_pos(&inlines[1]);
    assert_eq!((brk.start_offset, brk.end_offset), (21, 25));

    let code = code_pos(&inlines[2]);
    assert_eq!((code.start_offset, code.end_offset), (25, 38));
    assert_eq!(slice(source, &code), "```\n   c\ntail");
}
