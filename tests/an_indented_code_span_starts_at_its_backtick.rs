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

use carve::ast::{BlockNode, Pos};

fn slice(source: &str, pos: &Pos) -> String {
    source
        .chars()
        .skip(pos.start_offset)
        .take(pos.end_offset - pos.start_offset)
        .collect()
}

/// Corpus 367, row 2: the fence opens with no closer (carve#4018), and its block
/// starts at the backtick run, not at the three spaces in front of it.
#[test]
fn an_unterminated_fence_opens_its_span_at_the_backtick() {
    let source = ":: t\n:  a\n   ```\ntail\n";
    let code = description_code_block_pos(source);
    assert_eq!((code.start_offset, code.end_offset), (13, 16));
    assert_eq!((code.start_line, code.start_column), (3, 4));
}

/// Corpus 450, row 3: a longer body, the same opening.
#[test]
fn the_span_opens_at_the_backtick_below_a_longer_description() {
    let source = ":: term\n:  definition\n   ```\n   c\ntail\n";
    let code = description_code_block_pos(source);
    assert_eq!((code.start_offset, code.end_offset), (25, 33));
    assert_eq!(slice(source, &code), "```\n   c");
}

fn description_code_block_pos(source: &str) -> Pos {
    let options = carve::Options {
        positions: true,
        ..Default::default()
    };
    let doc = carve::parse_with_options(source, &options);
    let BlockNode::DefinitionList(list) = &doc.children[0] else {
        panic!("the fixture did not parse as a definition list");
    };
    match &list.items[0].definitions[0].children[1] {
        BlockNode::CodeBlock(code) => code.pos.clone().expect("a code block must be placed"),
        other => panic!("expected a code block, got {other:?}"),
    }
}
