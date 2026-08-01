//! Source positions (spec PART 12 section 4).
//!
//! These assert the two properties that make a position usable: it is measured
//! in the ORIGINAL document (not in the prefix-stripped text the parser sees),
//! and it is ABSENT rather than wrong when the mapping is unknown.

use carve::ast::{BlockNode, Document, FigureTarget, Pos};
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

#[test]
fn a_table_cell_spans_its_own_columns() {
    let source = "| a | b |\n|---|---|\n| c | dd |\n";
    let doc = parse_with_positions(source);
    let BlockNode::Table(table) = &doc.children[0] else {
        panic!("expected a table");
    };

    let cells: Vec<String> = table
        .rows
        .iter()
        .flat_map(|r| r.cells.iter())
        .map(|c| slice(source, c.pos.expect("every cell carries a position")))
        .collect();

    assert_eq!(cells, vec![" a ", " b ", " c ", " dd "]);
}

#[test]
fn a_cell_holding_an_escaped_pipe_spans_the_source_not_the_text() {
    // `\|` resolves to one character, so the cell's text is shorter than the
    // source it came from. A span derived from the text would stop early.
    let source = "|= A |= B |\n| x \\| y | z |\n";
    let doc = parse_with_positions(source);
    let BlockNode::Table(table) = &doc.children[0] else {
        panic!("expected a table");
    };

    let cell = table.rows[1].cells[0]
        .pos
        .expect("the cell carries a position");
    assert_eq!(slice(source, cell), " x \\| y ");
}

#[test]
fn a_quoted_table_cell_is_measured_in_the_document() {
    // The parser sees these lines with `> ` already removed; the columns have
    // to come back from the container's stripped width.
    let source = "> | x | y |\n> |---|---|\n> | z | w |\n";
    let doc = parse_with_positions(source);
    let BlockNode::BlockQuote(quote) = &doc.children[0] else {
        panic!("expected a blockquote");
    };
    let BlockNode::Table(table) = &quote.children[0] else {
        panic!("expected a quoted table");
    };

    let cell = table.rows[0].cells[0]
        .pos
        .expect("the quoted cell carries a position");
    assert_eq!(cell.start_column, 4, "past `> |`");
    assert_eq!(slice(source, cell), " x ");
}

#[test]
fn a_block_image_carries_its_own_span() {
    // An INLINE image gets its span from the inline parser. A lone image
    // paragraph is promoted to a block image and never goes through it, so it
    // had none at all.
    let source = "![alt](/i.png)\n";
    let doc = parse_with_positions(source);
    let BlockNode::BlockImage(image) = &doc.children[0] else {
        panic!("expected a block image, got {:?}", doc.children[0]);
    };

    assert_eq!(
        slice(source, image.pos.expect("the image carries a position")),
        "![alt](/i.png)"
    );
}

#[test]
fn a_captioned_image_places_the_figure_and_its_target() {
    let source = "![alt](/i.png)\n^ cap\n";
    let doc = parse_with_positions(source);
    let BlockNode::Figure(figure) = &doc.children[0] else {
        panic!("expected a figure");
    };

    // The figure runs from the image through the caption.
    assert_eq!(
        slice(source, figure.pos.expect("the figure carries a position")),
        "![alt](/i.png)\n^ cap"
    );

    let FigureTarget::Image(image) = &figure.target else {
        panic!("expected an image target");
    };
    // And the target keeps its own, filled rather than left at 0..0 - a span
    // that reads as present and selects nothing is worse than none.
    assert_eq!(
        slice(source, image.pos.expect("the target carries a position")),
        "![alt](/i.png)"
    );
}

#[test]
fn an_unresolved_reference_link_keeps_its_span_when_it_reverts() {
    // `[text][missing]` has no definition, so it reverts to the literal source
    // it occupied. That source IS the link's extent - only the node type
    // changes - and rebuilding it as a bare text node dropped the span.
    //
    // The reverted form is where a position is wanted most: it is exactly the
    // case where an author wrote a reference that does not resolve, and a tool
    // reporting that has to say where.
    let source = "see [text][missing] here\n";
    let doc = parse_with_positions(source);
    let BlockNode::Paragraph(paragraph) = &doc.children[0] else {
        panic!("expected a paragraph");
    };

    let spans: Vec<String> = paragraph
        .children
        .iter()
        .filter_map(|node| match node {
            carve::ast::InlineNode::Text(t) => Some(slice(source, t.pos.expect("text position"))),
            _ => None,
        })
        .collect();

    assert_eq!(spans, vec!["see ", "[text][missing]", " here"]);
}

#[test]
fn a_resolved_reference_link_is_unaffected() {
    let source = "see [text][ok] here\n\n[ok]: /u\n";
    let doc = parse_with_positions(source);
    let BlockNode::Paragraph(paragraph) = &doc.children[0] else {
        panic!("expected a paragraph");
    };

    let link = paragraph
        .children
        .iter()
        .find_map(|node| match node {
            carve::ast::InlineNode::Link(l) => Some(l),
            _ => None,
        })
        .expect("a link");

    assert_eq!(
        slice(source, link.pos.expect("link position")),
        "[text][ok]"
    );
}

#[test]
fn a_quoted_figure_spans_the_quote_and_its_caption() {
    // The image path placed its figure; a blockquote wrapped in one did not.
    let source = "> Stay hungry\n^ Steve Jobs\n";
    let doc = parse_with_positions(source);
    let BlockNode::Figure(figure) = &doc.children[0] else {
        panic!("expected a figure, got {:?}", doc.children[0]);
    };

    assert_eq!(
        slice(source, figure.pos.expect("the figure carries a position")),
        "> Stay hungry\n^ Steve Jobs"
    );
}

#[test]
fn a_nested_autolink_unwraps_to_text_that_keeps_its_own_span() {
    // A link cannot contain a link, so the autolink keeps only its DISPLAY
    // text - a sub-slice of what it occupied. Handing over the autolink's whole
    // span would cover the `<` and `>` as well, and a text node's span has to
    // select the text it belongs to.
    let source = "[pre <http://h> post](/u)\n";
    let doc = parse_with_positions(source);
    let BlockNode::Paragraph(paragraph) = &doc.children[0] else {
        panic!("expected a paragraph");
    };
    let carve::ast::InlineNode::Link(link) = &paragraph.children[0] else {
        panic!("expected a link");
    };

    let spans: Vec<String> = link
        .children
        .iter()
        .filter_map(|node| match node {
            carve::ast::InlineNode::Text(t) => Some(slice(source, t.pos.expect("text position"))),
            _ => None,
        })
        .collect();

    assert_eq!(spans, vec!["pre ", "http://h", " post"]);
}

#[test]
fn an_unwrapped_autolink_declines_when_the_text_is_not_the_source() {
    // `<mailto:x@y.z>` displays `x@y.z`: the source carries a scheme the text
    // does not, so no sub-slice equals it and the honest answer is none.
    let source = "[a <mailto:x@y.z> b](/u)\n";
    let doc = parse_with_positions(source);
    let BlockNode::Paragraph(paragraph) = &doc.children[0] else {
        panic!("expected a paragraph");
    };
    let carve::ast::InlineNode::Link(link) = &paragraph.children[0] else {
        panic!("expected a link");
    };

    let unplaced = link.children.iter().any(|node| match node {
        carve::ast::InlineNode::Text(t) => t.value == "x@y.z" && t.pos.is_none(),
        _ => false,
    });
    assert!(unplaced, "a rewritten display text must not claim a span");
}
