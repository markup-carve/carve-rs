//! Source positions (spec PART 12 section 4).
//!
//! These assert the two properties that make a position usable: it is measured
//! in the ORIGINAL document (not in the prefix-stripped text the parser sees),
//! and it is ABSENT rather than wrong when the mapping is unknown.

use carve::ast::{BlockNode, Document, Pos};
use carve::Options;

fn parse_with_positions(source: &str) -> Document {
    let options = Options {
        positions: true,
        ..Default::default()
    };
    carve::parse_with_options(source, &options)
}

fn pos_of(block: &BlockNode) -> Option<Pos> {
    match block {
        BlockNode::Heading(h) => h.pos,
        BlockNode::Paragraph(p) => p.pos,
        _ => None,
    }
}

/// Slice the source with a position, so a wrong span shows up as wrong text.
fn slice(source: &str, pos: Pos) -> String {
    source
        .chars()
        .skip(pos.start_offset)
        .take(pos.end_offset - pos.start_offset)
        .collect()
}

#[test]
fn top_level_blocks_carry_their_span() {
    let source = "# Title\n\nhello\n";
    let doc = parse_with_positions(source);

    let heading = pos_of(&doc.children[0]).expect("heading position");
    assert_eq!(heading.start_line, 1);
    assert_eq!(heading.end_line, 1);
    assert_eq!(heading.start_column, 1);
    assert_eq!(slice(source, heading), "# Title");

    let para = pos_of(&doc.children[1]).expect("paragraph position");
    assert_eq!(para.start_line, 3);
    assert_eq!(slice(source, para), "hello");
}

#[test]
fn positions_are_off_by_default() {
    // The spec permits gating tracking behind a parse option; a caller that
    // never asked must not pay for it.
    let doc = carve::parse("# Title\n\nhello\n");
    assert!(pos_of(&doc.children[0]).is_none());
    assert!(pos_of(&doc.children[1]).is_none());
}

#[test]
fn a_quoted_block_is_measured_in_the_document_not_in_the_stripped_text() {
    // The parser sees "# Quoted" with the "> " already removed. A column taken
    // from that text would say 1; the document says 3.
    let source = "> # Quoted\n>\n> body text\n";
    let doc = parse_with_positions(source);

    let BlockNode::BlockQuote(quote) = &doc.children[0] else {
        panic!("expected a blockquote, got {:?}", doc.children[0]);
    };

    let heading = pos_of(&quote.children[0]).expect("quoted heading position");
    assert_eq!(heading.start_line, 1);
    assert_eq!(
        heading.start_column, 3,
        "the quote marker and its space are part of the document column"
    );
    assert_eq!(slice(source, heading), "# Quoted");

    let para = pos_of(&quote.children[1]).expect("quoted paragraph position");
    assert_eq!(para.start_line, 3);
    assert_eq!(para.start_column, 3);
    assert_eq!(slice(source, para), "body text");
}

#[test]
fn a_doubly_quoted_block_accumulates_both_strips() {
    let source = "> > deep\n";
    let doc = parse_with_positions(source);

    let BlockNode::BlockQuote(outer) = &doc.children[0] else {
        panic!("expected a blockquote");
    };
    let BlockNode::BlockQuote(inner) = &outer.children[0] else {
        panic!("expected a nested blockquote, got {:?}", outer.children[0]);
    };

    let para = pos_of(&inner.children[0]).expect("doubly quoted paragraph position");
    assert_eq!(
        para.start_column, 5,
        "both quote markers count toward the document column"
    );
    assert_eq!(slice(source, para), "deep");
}

#[test]
fn offsets_count_codepoints_not_bytes() {
    // The astral character is four bytes and one codepoint. An offset computed
    // in bytes would overshoot and slice the wrong text.
    let source = "\u{1F600} first\n\nsecond\n";
    let doc = parse_with_positions(source);

    let first = pos_of(&doc.children[0]).expect("first paragraph position");
    assert_eq!(first.start_offset, 0);
    assert_eq!(slice(source, first), "\u{1F600} first");

    let second = pos_of(&doc.children[1]).expect("second paragraph position");
    assert_eq!(
        second.start_offset, 9,
        "8 codepoints on line 1 plus the newline"
    );
    assert_eq!(slice(source, second), "second");
}

#[test]
fn an_unmappable_line_yields_no_position_rather_than_a_wrong_one() {
    // A line block rewrites its lines (leading whitespace becomes placeholder
    // characters), so no column in the rewritten text maps back to a column in
    // the document. The contract is silence, not a guess.
    let source = "::: |\n  indented verse\n:::\n";
    let doc = parse_with_positions(source);

    fn assert_no_bogus_positions(blocks: &[BlockNode]) {
        for block in blocks {
            assert!(
                pos_of(block).is_none(),
                "a line block cannot know its columns, so it must report none: {block:?}"
            );
            if let BlockNode::Div(div) = block {
                assert_no_bogus_positions(&div.children);
            }
        }
    }
    assert_no_bogus_positions(&doc.children);
}

#[test]
fn a_list_item_paragraph_starts_at_its_text_not_at_the_bullet() {
    let source = "- item one\n- item two\n";
    let doc = parse_with_positions(source);
    let BlockNode::List(list) = &doc.children[0] else {
        panic!("expected a list");
    };

    let first = pos_of(&list.items[0].children[0]).expect("first item position");
    assert_eq!(first.start_line, 1);
    assert_eq!(
        first.start_column, 3,
        "the paragraph is the text, so it starts past the bullet"
    );
    assert_eq!(slice(source, first), "item one");

    let second = pos_of(&list.items[1].children[0]).expect("second item position");
    assert_eq!(second.start_line, 2);
    assert_eq!(slice(source, second), "item two");
}

#[test]
fn a_nested_item_accumulates_the_outer_indent() {
    let source = "- outer\n  - inner\n";
    let doc = parse_with_positions(source);
    let BlockNode::List(list) = &doc.children[0] else {
        panic!("expected a list");
    };
    // The inner list is a child block of the outer item.
    let inner = list.items[0]
        .children
        .iter()
        .find_map(|c| match c {
            BlockNode::List(l) => Some(l),
            _ => None,
        })
        .expect("a nested list");

    let para = pos_of(&inner.items[0].children[0]).expect("nested item position");
    assert_eq!(para.start_line, 2);
    assert_eq!(
        para.start_column, 5,
        "two columns of indent plus the inner bullet"
    );
    assert_eq!(slice(source, para), "inner");
}

#[test]
fn a_list_item_spans_its_marker_and_body() {
    // The item includes its bullet - unlike the paragraph inside it, which
    // starts at the text. Both are right: the marker belongs to the item.
    let source = "- one\n- two\n";
    let doc = parse_with_positions(source);
    let BlockNode::List(list) = &doc.children[0] else {
        panic!("expected a list");
    };

    let first = list.items[0].pos.expect("the item carries a position");
    assert_eq!(slice(source, first), "- one");
    assert_eq!(
        slice(source, list.items[1].pos.expect("second item")),
        "- two"
    );
}

#[test]
fn a_table_row_spans_its_line() {
    let source = "| a | b |\n|---|---|\n| c | d |\n";
    let doc = parse_with_positions(source);
    let BlockNode::Table(table) = &doc.children[0] else {
        panic!("expected a table");
    };

    assert_eq!(
        slice(source, table.rows[0].pos.expect("header row")),
        "| a | b |"
    );
    assert_eq!(
        slice(source, table.rows[1].pos.expect("body row")),
        "| c | d |"
    );
}

#[test]
fn a_continued_row_spans_every_line_it_runs_to() {
    // A `+` continuation extends the row. The row stays ONE contiguous range
    // that no sibling row overlaps, so it keeps a position - the cell it
    // extends does not, because that cell's content sits in two column ranges
    // with another column's content between them.
    let source = "|= F |= D |\n| Cx | A long |\n+    | that cont |\n| S | one |\n";
    let doc = parse_with_positions(source);
    let BlockNode::Table(table) = &doc.children[0] else {
        panic!("expected a table");
    };

    let continued = table.rows[1]
        .pos
        .expect("the continued row carries a position");
    assert_eq!(continued.start_line, 2);
    assert_eq!(continued.end_line, 3);
    assert!(slice(source, continued).contains("A long"));
    assert!(slice(source, continued).contains("that cont"));
}
