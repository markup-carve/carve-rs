//! Expanding abbreviations splits a text node into pieces, and the pieces are
//! CONTIGUOUS SLICES of the node being split - so their spans follow from the
//! original's without re-scanning the document (carve-rs#333).
//!
//! The guards matter more than the arithmetic. A text node whose span length
//! disagrees with its char count is not a verbatim slice of the source, and a
//! node spanning several lines has no single column to count from. Deriving a
//! span in either case would invent one.

use carve::ast::{BlockNode, InlineNode};

fn inlines(source: &str) -> Vec<InlineNode> {
    let options = carve::Options {
        positions: true,
        ..Default::default()
    };
    let doc = carve::parse_with_options(source, &options);
    for block in &doc.children {
        if let BlockNode::Paragraph(p) = block {
            return p.children.clone();
        }
    }
    panic!("no paragraph in the fixture");
}

fn span_of(node: &InlineNode) -> Option<carve::ast::Pos> {
    match node {
        InlineNode::Text(t) => t.pos,
        InlineNode::Abbreviation(a) => a.pos,
        _ => None,
    }
}

#[test]
fn every_piece_of_a_split_text_node_slices_back() {
    let source = "*[HTML]: Hyper Text\n\nThe HTML spec.\n";
    let codepoints: Vec<char> = source.chars().collect();

    let children = inlines(source);
    assert_eq!(children.len(), 3, "expected text + abbreviation + text");

    for node in &children {
        let pos = span_of(node).expect("every piece must carry a span");
        let slice: String = codepoints[pos.start_offset..pos.end_offset]
            .iter()
            .collect();
        let want = match node {
            InlineNode::Text(t) => t.value.clone(),
            InlineNode::Abbreviation(a) => a.abbr.clone(),
            _ => unreachable!(),
        };
        assert_eq!(slice, want, "a piece's span points elsewhere");
    }
}

/// The pieces must ABUT: the end of one is the start of the next. A per-piece
/// span that is individually correct can still leave a gap if the running
/// offset is advanced by bytes where it should count characters.
#[test]
fn the_pieces_abut_with_no_gap() {
    let children = inlines("*[HTML]: Hyper Text\n\nThe HTML spec.\n");

    let spans: Vec<_> = children.iter().filter_map(span_of).collect();
    assert_eq!(spans.len(), 3);
    for pair in spans.windows(2) {
        assert_eq!(
            pair[0].end_offset, pair[1].start_offset,
            "a gap opened between two pieces of one text node"
        );
    }
}

/// An astral character ahead of the abbreviation shifts byte and char offsets
/// apart. Counting bytes here would place the abbreviation three codepoints
/// late while still looking plausible.
#[test]
fn an_astral_character_does_not_shift_the_split() {
    let source = "*[HTML]: Hyper Text\n\n\u{1F600} HTML spec.\n";
    let codepoints: Vec<char> = source.chars().collect();

    let children = inlines(source);
    let abbr = children
        .iter()
        .find(|n| matches!(n, InlineNode::Abbreviation(_)))
        .expect("the abbreviation");
    let pos = span_of(abbr).expect("the abbreviation must carry a span");

    let slice: String = codepoints[pos.start_offset..pos.end_offset]
        .iter()
        .collect();
    assert_eq!(
        slice, "HTML",
        "the split counted bytes rather than codepoints"
    );
}
