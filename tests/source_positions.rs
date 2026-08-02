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

#[test]
fn a_resolved_cross_reference_keeps_the_span_of_its_source() {
    // `</#id>` is a real span. Resolving it is render-time behavior now, so the
    // parsed node and its position survive unchanged.
    let source = "# Some Title\n\nSee </#some-title> here.\n";
    let doc = parse_with_positions(source);
    let BlockNode::Paragraph(paragraph) = &doc.children[1] else {
        panic!("expected a paragraph, got {:?}", doc.children[1]);
    };

    let crossref = paragraph
        .children
        .iter()
        .find_map(|node| match node {
            carve::ast::InlineNode::CrossRef(c) => Some(c),
            _ => None,
        })
        .expect("the cross-reference");

    assert_eq!(
        slice(source, crossref.pos.expect("crossref position")),
        "</#some-title>"
    );
    assert_eq!(crossref.target, "some-title");
}

#[test]
fn an_unresolved_cross_reference_keeps_its_span() {
    // Nothing to resolve at parse time, so the cross-reference node survives
    // with the exact characters the author wrote as its span.
    let source = "See </#nope> here.\n";
    let doc = parse_with_positions(source);
    let BlockNode::Paragraph(paragraph) = &doc.children[0] else {
        panic!("expected a paragraph");
    };

    let crossref = paragraph
        .children
        .iter()
        .find_map(|node| match node {
            carve::ast::InlineNode::CrossRef(c) => Some(c),
            _ => None,
        })
        .expect("the cross-reference");

    assert_eq!(
        slice(source, crossref.pos.expect("crossref position")),
        "</#nope>"
    );
    assert_eq!(crossref.target, "nope");
}

/// A `+` continuation resets what a blockquote's lines look like, so the cursor
/// cannot say how wide the stripped prefix was and refuses a span. The items
/// were placed by other means, and a list that runs from its first item to its
/// last is not a guess - so the list takes their extent.
///
/// Both ends have to exist, or the range would start or stop somewhere
/// arbitrary: a consumer can handle a missing span, but cannot tell a wrong one
/// from a right one.
#[test]
fn a_list_in_a_continued_blockquote_spans_its_items() {
    let source = "> quoted\n+\n- item\n> more\n";
    let doc = parse_with_positions(source);

    let BlockNode::BlockQuote(quote) = &doc.children[0] else {
        panic!("the document opens with a blockquote");
    };
    let list = quote
        .children
        .iter()
        .find_map(|child| match child {
            BlockNode::List(list) => Some(list),
            _ => None,
        })
        .expect("the list is inside the quote");

    let pos = list.pos.expect("the list is placed by its items");
    assert_eq!(slice(source, pos), "- item");
    assert_eq!(pos, list.items[0].pos.expect("the item is placed"));
}

/// A definition list was never placed at all, and its items with it.
#[test]
fn a_definition_list_spans_its_terms_and_definitions() {
    let source = ":: term\n:  A definition can now hold\n\n   more than one paragraph.\n";
    let doc = parse_with_positions(source);

    let BlockNode::DefinitionList(list) = &doc.children[0] else {
        panic!("the document is a definition list");
    };
    let pos = list.pos.expect("the list is placed");
    assert_eq!(
        slice(source, pos),
        ":: term\n:  A definition can now hold\n\n   more than one paragraph."
    );

    // The ITEM is deliberately not placed: the wire format flattens items into
    // a flat run of terms and descriptions, so a span here would be lost on the
    // way back in and the round-trip would stop being an identity.
    assert!(list.items[0].pos.is_none());
}

/// The span stops at the last definition, not at the blank line the parser
/// looked through for another item.
#[test]
fn a_definition_list_span_excludes_the_gap_after_it() {
    let source = ":: term\n:  its definition\n\nA later paragraph.\n";
    let doc = parse_with_positions(source);

    let BlockNode::DefinitionList(list) = &doc.children[0] else {
        panic!("the document opens with a definition list");
    };
    let pos = list.pos.expect("the list is placed");
    assert_eq!(slice(source, pos), ":: term\n:  its definition");
}

/// Offsets are filled in a second pass, so a node the pass does not reach keeps
/// its line and column but reports 0..0 - present, and selecting nothing. That
/// is what a definition list did.
#[test]
fn a_definition_list_span_has_real_offsets() {
    let source = "Intro\n\n:: term\n:  its definition\n";
    let doc = parse_with_positions(source);

    let BlockNode::DefinitionList(list) = &doc.children[1] else {
        panic!("the definition list follows the paragraph");
    };
    let pos = list.pos.expect("the list is placed");
    assert_ne!(pos.start_offset, 0);
    assert_eq!(slice(source, pos), ":: term\n:  its definition");
}

/// Each stanza of a line block is its own paragraph, and each was unplaced.
#[test]
fn each_line_block_stanza_spans_its_own_lines() {
    let source = "::: |\nStanza one,\nstill one.\n\nStanza two.\n:::\n";
    let doc = parse_with_positions(source);

    let BlockNode::LineBlock(block) = &doc.children[0] else {
        panic!("the document is a line block");
    };
    let spans: Vec<String> = block
        .children
        .iter()
        .map(|child| {
            let pos = pos_of(child).expect("every stanza is placed");
            slice(source, pos)
        })
        .collect();
    assert_eq!(spans, vec!["Stanza one,\nstill one.", "Stanza two."]);
}

/// The stanza ends at its last line, not at the blank line that closed it and
/// not at the `:::` that closed the block.
#[test]
fn a_line_block_stanza_span_excludes_the_blank_that_ends_it() {
    let source = "::: |\nverse\n\n:::\n";
    let doc = parse_with_positions(source);

    let BlockNode::LineBlock(block) = &doc.children[0] else {
        panic!("the document is a line block");
    };
    let pos = pos_of(&block.children[0]).expect("the stanza is placed");
    assert_eq!(slice(source, pos), "verse");
}

/// Frontmatter had no span at all - the struct had no field to put one in.
/// The span covers the whole block, fences included, which is what the
/// reference publishes.
#[test]
fn frontmatter_spans_the_whole_block_including_fences() {
    let source = "---\ntitle: x\n---\n\nBody\n";
    let doc = parse_with_positions(source);

    let raw = doc.frontmatter_raw.as_ref().expect("the document has one");
    let pos = raw.pos.expect("frontmatter is placed");
    assert_eq!(slice(source, pos), "---\ntitle: x\n---");
    assert_eq!(pos.start_line, 1);
    assert_eq!(pos.start_column, 1);
}

/// The span stops at the closing fence, not at the blank line after it.
#[test]
fn frontmatter_span_excludes_the_blank_after_the_fence() {
    let source = "---\n---\n\n\nBody\n";
    let doc = parse_with_positions(source);

    let raw = doc.frontmatter_raw.as_ref().expect("the document has one");
    let pos = raw.pos.expect("frontmatter is placed");
    assert_eq!(slice(source, pos), "---\n---");
}

/// Columns and offsets are CODEPOINTS, so an astral character counts once.
#[test]
fn frontmatter_offsets_are_codepoints() {
    let source = "---\ntitle: \u{1f600}\n---\n\nBody\n";
    let doc = parse_with_positions(source);

    let raw = doc.frontmatter_raw.as_ref().expect("the document has one");
    let pos = raw.pos.expect("frontmatter is placed");
    assert_eq!(slice(source, pos), "---\ntitle: \u{1f600}\n---");
    // 4 + 9 + 3, newlines included - not the 20 a UTF-8 byte count would give.
    assert_eq!(pos.end_offset, 16);
}

/// An admonition's title is a slice of the opener line, so its inlines can be
/// placed - but `inline_anchor_for_line` cannot do it: that helper works by
/// SUFFIX, and a title sits in the middle of its line, between quotes. The
/// opener records the column instead.
#[test]
fn an_admonition_title_places_its_inlines() {
    let source = "::: note \"Install *now* via `npm`\"\nBody.\n:::\n";
    let doc = parse_with_positions(source);

    let BlockNode::Admonition(note) = &doc.children[0] else {
        panic!("the document is an admonition");
    };
    let title = note.title.as_ref().expect("it has a title");
    let spans: Vec<String> = title
        .iter()
        .map(|inline| match inline {
            carve::ast::InlineNode::Text(t) => slice(source, t.pos.expect("text is placed")),
            carve::ast::InlineNode::Emphasis(e) => {
                slice(source, e.pos.expect("emphasis is placed"))
            }
            carve::ast::InlineNode::Code(c) => slice(source, c.pos.expect("code is placed")),
            other => panic!("unexpected inline in the title: {other:?}"),
        })
        .collect();
    assert_eq!(spans, vec!["Install ", "*now*", " via ", "`npm`"]);
}

/// A title carrying an ESCAPE is rebuilt rather than sliced, so no column in it
/// maps back and the inlines stay unplaced. Absent beats wrong: the rebuilt
/// string is shorter than the source it came from, so every column after the
/// escape would be off by one.
#[test]
fn an_escaped_title_leaves_its_inlines_unplaced() {
    let source = "::: note \"a \\\" b *x*\"\nBody.\n:::\n";
    let doc = parse_with_positions(source);

    let BlockNode::Admonition(note) = &doc.children[0] else {
        panic!("the document is an admonition");
    };
    let title = note.title.as_ref().expect("it has a title");
    let placed = title.iter().any(|inline| match inline {
        carve::ast::InlineNode::Text(t) => t.pos.is_some(),
        carve::ast::InlineNode::Emphasis(e) => e.pos.is_some(),
        _ => false,
    });
    assert!(!placed, "a rebuilt title cannot place anything");
}

/// A `+` continuation attaches a flush-left block to the item above it. The
/// sub-cursor that parsed that block was built with a line map but no COLUMN
/// map, so the block and everything inside it came out unplaced - a code block,
/// a quote, a table, its rows, its cells and their text.
///
/// The attached lines are taken verbatim, so the parent's widths apply
/// unchanged.
#[test]
fn a_plus_continuation_places_the_block_it_attaches() {
    let source = "- Build the image\n+\n```sh\ndocker build -t app .\n```\n- Push it\n";
    let doc = parse_with_positions(source);

    let BlockNode::List(list) = &doc.children[0] else {
        panic!("the document is a list");
    };
    let attached = list.items[0]
        .children
        .iter()
        .find(|child| matches!(child, BlockNode::CodeBlock(_)))
        .expect("the code block is attached to the first item");
    let BlockNode::CodeBlock(code) = attached else {
        unreachable!("just matched")
    };
    let pos = code.pos.expect("the attached block is placed");
    assert_eq!(slice(source, pos), "```sh\ndocker build -t app .\n```");
}

/// The same for a table, down to its cells - the deepest thing the old
/// behavior left unplaced.
#[test]
fn a_plus_continuation_places_a_table_down_to_its_cells() {
    let source = "- +\n| a | b |\n| c | d |\n- next\n";
    let doc = parse_with_positions(source);

    let BlockNode::List(list) = &doc.children[0] else {
        panic!("the document is a list");
    };
    let BlockNode::Table(table) = &list.items[0].children[0] else {
        panic!("the first item holds the table");
    };
    assert_eq!(
        slice(source, table.pos.expect("the table is placed")),
        "| a | b |\n| c | d |"
    );
    let row = &table.rows[0];
    assert_eq!(
        slice(source, row.pos.expect("the row is placed")),
        "| a | b |"
    );
    assert_eq!(
        slice(source, row.cells[0].pos.expect("the cell is placed")),
        " a "
    );
}

/// `- +` means the item's first block IS the attached one - there is no inline
/// paragraph. That item was built with a hardcoded `None`, so it had no span
/// while its siblings and its own contents did.
#[test]
fn an_item_whose_content_is_only_a_continuation_is_placed() {
    let source = "- +\n| a | b |\n- next\n";
    let doc = parse_with_positions(source);

    let BlockNode::List(list) = &doc.children[0] else {
        panic!("the document is a list");
    };
    assert_eq!(
        slice(
            source,
            list.items[0].pos.expect("the bare-plus item is placed")
        ),
        "- +\n| a | b |"
    );
    // Its sibling is unaffected - the span stops at the attached block.
    assert_eq!(
        slice(source, list.items[1].pos.expect("the next item is placed")),
        "- next"
    );
}

/// A `+` line extends the cell above it. The text it adds is a verbatim slice
/// of that line, but it was parsed with no anchor at all, so every continued
/// cell's later text came out unplaced.
#[test]
fn a_continued_table_cell_places_the_text_it_adds() {
    let source = concat!(
        "|= Feature |= Description        |\n",
        "| Complex  | A long description |\n",
        "+          | that continues     |\n",
        "+          | across lines.      |\n",
    );
    let doc = parse_with_positions(source);

    let BlockNode::Table(table) = &doc.children[0] else {
        panic!("the document is a table");
    };
    let cell = &table.rows[1].cells[1];
    let texts: Vec<Option<String>> = cell
        .children
        .iter()
        .map(|inline| match inline {
            carve::ast::InlineNode::Text(t) => t.pos.map(|pos| slice(source, pos)),
            other => panic!("unexpected inline: {other:?}"),
        })
        .collect();

    assert_eq!(
        texts,
        vec![
            Some("A long description".to_string()),
            // The joiner is MANUFACTURED - the source has a line break here,
            // not a space - so it carries no position and must not borrow one.
            None,
            Some("that continues".to_string()),
            None,
            Some("across lines.".to_string()),
        ]
    );
}

/// Verse lines were refused their columns WHOLESALE: the stanza's column map
/// was left empty because leading whitespace becomes NBSP placeholders. But
/// only a line that CARRIES leading whitespace is rewritten - one without any
/// is passed through untouched, and its columns are still the document's.
#[test]
fn verse_lines_that_were_not_rewritten_keep_their_columns() {
    let source = "::: |\n*Bold* and /italic/,\nplain line.\n:::\n";
    let doc = parse_with_positions(source);

    let BlockNode::LineBlock(block) = &doc.children[0] else {
        panic!("the document is a line block");
    };
    let BlockNode::Paragraph(stanza) = &block.children[0] else {
        panic!("a stanza is a paragraph");
    };
    let spans: Vec<Option<String>> = stanza
        .children
        .iter()
        .map(|inline| match inline {
            carve::ast::InlineNode::Text(t) => t.pos.map(|p| slice(source, p)),
            carve::ast::InlineNode::Emphasis(e) => e.pos.map(|p| slice(source, p)),
            carve::ast::InlineNode::HardBreak(b) => b.pos.map(|p| slice(source, p)),
            other => panic!("unexpected inline: {other:?}"),
        })
        .collect();
    assert_eq!(
        spans,
        vec![
            Some("*Bold*".to_string()),
            Some(" and ".to_string()),
            Some("/italic/".to_string()),
            Some(",".to_string()),
            // The break IS the source's line ending, so it spans it.
            Some("\n".to_string()),
            Some("plain line.".to_string()),
        ]
    );
}

/// A line that IS rewritten stays unplaced, and only that line - its neighbor
/// keeps its span. Before this, one indented line cost the whole stanza.
#[test]
fn an_indented_verse_line_loses_only_its_own_span() {
    let source = "::: |\nRoses are red,\n  Violets are blue.\n:::\n";
    let doc = parse_with_positions(source);

    let BlockNode::LineBlock(block) = &doc.children[0] else {
        panic!("the document is a line block");
    };
    let BlockNode::Paragraph(stanza) = &block.children[0] else {
        panic!("a stanza is a paragraph");
    };
    let carve::ast::InlineNode::Text(first) = &stanza.children[0] else {
        panic!("the stanza opens with text");
    };
    assert_eq!(
        slice(source, first.pos.expect("the plain line is placed")),
        "Roses are red,"
    );
    // The rewritten line's text is not the source - it holds NBSP placeholders
    // where the indent was - so no span can select it.
    let last = stanza.children.last().expect("a second line");
    let carve::ast::InlineNode::Text(indented) = last else {
        panic!("it ends with text");
    };
    assert!(indented.pos.is_none());
}

/// A trailing `%%` comment is dropped, and the whitespace before it is popped
/// off the text buffer. That pop used to mark the buffer unplaceable, so every
/// line ending in a comment lost its text position.
///
/// Popping from the END keeps the buffer equal to the source it started at, so
/// the span is shorter, not wrong.
#[test]
fn text_before_a_trailing_comment_keeps_its_span() {
    let source = "Also visible. %% this tail is a comment\n";
    let doc = parse_with_positions(source);

    let BlockNode::Paragraph(para) = &doc.children[0] else {
        panic!("the document is a paragraph");
    };
    let carve::ast::InlineNode::Text(text) = &para.children[0] else {
        panic!("it holds one text node");
    };
    let pos = text.pos.expect("the text is placed");
    assert_eq!(slice(source, pos), "Also visible.");
    // The span must stop at the text, not run into the dropped comment.
    assert_eq!(slice(source, pos), text.value);
}

/// Same for a heading, and for text that follows an inline construct - the
/// `%%` inside the code span is content, not a comment.
#[test]
fn a_comment_after_a_code_span_leaves_both_texts_placed() {
    let source = "Run `a %% b` then done. %% gone\n";
    let doc = parse_with_positions(source);

    let BlockNode::Paragraph(para) = &doc.children[0] else {
        panic!("the document is a paragraph");
    };
    let placed: Vec<String> = para
        .children
        .iter()
        .filter_map(|inline| match inline {
            carve::ast::InlineNode::Text(t) => Some(slice(source, t.pos.expect("text placed"))),
            _ => None,
        })
        .collect();
    assert_eq!(placed, vec!["Run ", " then done."]);
}

/// A captioned code block becomes a numbered listing - a figure wrapping the
/// block. The figure was built with `pos: None`, and the block inside it kept
/// offsets of 0..0: the offset pass matched the figure's other targets by name
/// and let a code block fall through its catch-all arm.
///
/// 0..0 is worse than absent. It reads as present and selects the empty string
/// at the start of the document, and a test asserting only `is_some()` would
/// have passed.
#[test]
fn a_captioned_code_block_places_the_figure_and_the_block() {
    let source = "```python\ndef greet():\n    return 1\n```\n^ Listing #: a greeting\n";
    let doc = parse_with_positions(source);

    let BlockNode::Figure(figure) = &doc.children[0] else {
        panic!("a captioned code block is a figure");
    };
    assert_eq!(
        slice(source, figure.pos.expect("the figure is placed")),
        "```python\ndef greet():\n    return 1\n```\n^ Listing #: a greeting"
    );

    let FigureTarget::CodeBlock(code) = &figure.target else {
        panic!("its target is the code block");
    };
    let pos = code.pos.expect("the block is placed");
    assert_ne!(pos.start_offset, pos.end_offset, "0..0 selects nothing");
    assert_eq!(
        slice(source, pos),
        "```python\ndef greet():\n    return 1\n```"
    );
}

/// Standalone display math becomes a paragraph, built with
/// `..Default::default()` and so with no span - whether or not a caption
/// follows and turns it into a figure.
#[test]
fn standalone_display_math_places_its_paragraph() {
    let source = "Intro.\n\n$$`\\int_0^1 x\\,dx`\n";
    let doc = parse_with_positions(source);

    let BlockNode::Paragraph(math) = &doc.children[1] else {
        panic!("the display math is a paragraph");
    };
    assert_eq!(
        slice(source, math.pos.expect("it is placed")),
        "$$`\\int_0^1 x\\,dx`"
    );
}
