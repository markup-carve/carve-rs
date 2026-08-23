//! A LINE BLOCK'S HARD BREAK OWNS THE BOUNDARY, NOT THE LINE IT ENDS
//! (carve-rs#1246).
//!
//! Where the line a break ends is a comment-only line the block layer removes,
//! the break stands for the newline the AUTHOR wrote - which is at the end of
//! the comment they wrote, not at column 1 where the emptied line ends. PART 12
//! §4 ends a span "immediately after the last source codepoint the construct
//! owns", and what a break owns is the boundary. The `comment` node is what owns
//! those bytes, and it is already published at exactly them.
//!
//! `line_block_break_positions.rs` already pins the TOP-LEVEL case. This is the
//! same boundary reached through an inline container that opens before the
//! emptied line and closes after it, so the break is a child of the `strong`
//! rather than of the stanza - and the walk that re-poses breaks from line
//! geometry only ever visited the stanza's top level. The break kept the span
//! the inline parser gave it, measured from the emptied TEXT: `3:1-4:1`, offsets
//! `9..19`, which starts where the `comment` starts and covers all nine
//! codepoints of it before reaching the terminator. Two siblings held the same
//! bytes, which §4's span tree is meant to rule out.
//!
//! carve-js and carve-php both publish `18..19` here, so this was a divergence
//! as well as a containment defect. carve-js reached the same defect from the
//! other direction in markup-carve/carve-js#1305 and fixed it with the comment
//! beginning "A COMMENT LINE IS MEASURED FROM ITS SOURCE".

use carve::ast::{BlockNode, InlineNode, Pos};

/// Every hard break in the document, AT ANY DEPTH, in source order. The helper
/// in `line_block_break_positions.rs` reads the stanza's top level only, which
/// is exactly the blind spot this file is about.
fn breaks(source: &str) -> Vec<Option<Pos>> {
    let options = carve::Options {
        positions: true,
        ..Default::default()
    };
    let doc = carve::parse_with_options(source, &options);
    let mut out = Vec::new();
    collect_blocks(&doc.children, &mut out);
    out
}

fn collect_blocks(blocks: &[BlockNode], out: &mut Vec<Option<Pos>>) {
    for block in blocks {
        match block {
            BlockNode::LineBlock(lb) => collect_blocks(&lb.children, out),
            BlockNode::Paragraph(p) => collect_inlines(&p.children, out),
            _ => {}
        }
    }
}

fn collect_inlines(inlines: &[InlineNode], out: &mut Vec<Option<Pos>>) {
    for inline in inlines {
        match inline {
            InlineNode::HardBreak(b) => out.push(b.pos),
            InlineNode::Emphasis(n) => collect_inlines(&n.children, out),
            InlineNode::Link(n) => collect_inlines(&n.children, out),
            InlineNode::Span(n) => collect_inlines(&n.children, out),
            _ => {}
        }
    }
}

/// The reported document. `%% secret` is nine characters starting at offset 9,
/// so the newline ending it starts at column 10 and the next line begins at
/// column 1 - which is what carve-js and carve-php publish.
#[test]
fn a_nested_break_after_a_comment_line_ends_where_the_comment_does() {
    let spans = breaks("::: |\n*a\n%% secret\nc*\n:::\n");

    assert_eq!(spans.len(), 2, "expected two breaks, got {spans:?}");
    let after = spans[1].expect("break after the comment line placed");

    assert_eq!(after.start_line, 3);
    assert_eq!(after.start_column, 10);
    assert_eq!(after.end_line, 4);
    assert_eq!(after.end_column, 1);
}

/// THE CONTAINMENT READING, asserted directly: the break must not reach into
/// the `comment` beside it. Before the fix the break started at the comment's
/// own start, so the two nodes held the same nine codepoints.
#[test]
fn a_nested_break_does_not_reach_into_the_comment_it_follows() {
    let options = carve::Options {
        positions: true,
        ..Default::default()
    };
    let doc = carve::parse_with_options("::: |\n*a\n%% secret\nc*\n:::\n", &options);
    let mut comment = None;
    let mut after = None;
    let mut walk = |inlines: &[InlineNode]| {
        for inline in inlines {
            match inline {
                InlineNode::Comment(c) => comment = c.pos,
                InlineNode::HardBreak(b) if comment.is_some() && after.is_none() => after = b.pos,
                _ => {}
            }
        }
    };
    if let Some(BlockNode::LineBlock(lb)) = doc.children.first() {
        if let Some(BlockNode::Paragraph(p)) = lb.children.first() {
            if let Some(InlineNode::Emphasis(strong)) = p.children.first() {
                walk(&strong.children);
            }
        }
    }
    let comment = comment.expect("the comment is placed");
    let after = after.expect("the break after it is placed");
    // The comment's own columns are unchanged by this fix.
    assert_eq!((comment.start_column, comment.end_column), (1, 10));
    // And the break starts where the comment ends rather than where it starts.
    assert_eq!(after.start_column, comment.end_column);
}

/// The break BEFORE the emptied line is untouched - `*a` is two characters, so
/// its boundary starts at column 3. A fix that re-posed every break rather than
/// the ones that need it would still have to agree here.
#[test]
fn the_break_before_the_comment_line_is_unchanged() {
    let spans = breaks("::: |\n*a\n%% secret\nc*\n:::\n");
    let before = spans[0].expect("first break placed");

    assert_eq!(before.start_line, 2);
    assert_eq!(before.start_column, 3);
    assert_eq!(before.end_line, 3);
    assert_eq!(before.end_column, 1);
}

/// A NESTED BREAK ON AN ORDINARY LINE keeps the span the anchors gave it. The
/// walk now reaches these too, so this is the row that says reaching them did
/// not move anything that was already right.
#[test]
fn a_nested_break_on_an_ordinary_line_is_unchanged() {
    let spans = breaks("::: |\n*a\nb\nc*\n:::\n");

    assert_eq!(spans.len(), 2, "expected two breaks, got {spans:?}");
    let first = spans[0].expect("first break placed");
    assert_eq!((first.start_line, first.start_column), (2, 3));
    assert_eq!((first.end_line, first.end_column), (3, 1));
    let second = spans[1].expect("second break placed");
    assert_eq!((second.start_line, second.start_column), (3, 2));
    assert_eq!((second.end_line, second.end_column), (4, 1));
}

/// TWO EMPTIED LINES IN A ROW, so the line counter has to survive more than one
/// correction: the second comment is eight characters, the third line's is six.
#[test]
fn consecutive_emptied_lines_each_end_at_their_own_comment() {
    let spans = breaks("::: |\n*a\n%% one\n%% two\nc*\n:::\n");

    assert_eq!(spans.len(), 3, "expected three breaks, got {spans:?}");
    let after_first = spans[1].expect("break after the first comment placed");
    assert_eq!((after_first.start_line, after_first.start_column), (3, 7));
    let after_second = spans[2].expect("break after the second comment placed");
    assert_eq!((after_second.start_line, after_second.start_column), (4, 7));
}
