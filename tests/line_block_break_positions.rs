//! A hard break in a line block is placed even when the stanza's TEXT is not.
//!
//! A tab expands to placeholders and shifts every column after it WITHIN the
//! line, so the anchor machinery refuses that line and its inlines come out
//! unplaced. That is right for text: the value stops being a slice of the
//! source, and a consumer asked to highlight it would get a mismatch.
//!
//! A break is not content on the line, though - it is the newline ENDING it,
//! and tab expansion does not move a line ending. The k-th break is the newline
//! after stanza line k, which is pure line geometry, so it stays derivable
//! exactly where the text is not (carve-rs#480).

use carve::ast::{BlockNode, InlineNode, Pos};

fn breaks(source: &str) -> Vec<Option<Pos>> {
    let options = carve::Options {
        positions: true,
        ..Default::default()
    };
    let doc = carve::parse_with_options(source, &options);
    let mut out = Vec::new();
    let mut visit = |blocks: &[BlockNode]| {
        for block in blocks {
            if let BlockNode::LineBlock(lb) = block {
                for child in &lb.children {
                    if let BlockNode::Paragraph(p) = child {
                        for inline in &p.children {
                            if let InlineNode::HardBreak(b) = inline {
                                out.push(b.pos);
                            }
                        }
                    }
                }
            }
        }
    };
    visit(&doc.children);
    out
}

#[test]
fn a_tab_bearing_stanza_still_places_its_breaks() {
    let spans = breaks("::: |\ntab\tgap\nwide\t\tgap\n\tlead\n:::\n");

    assert_eq!(spans.len(), 2, "expected two breaks, got {spans:?}");
    for (i, span) in spans.iter().enumerate() {
        assert!(span.is_some(), "break {i} has no position");
    }
}

#[test]
fn the_break_spans_the_newline_it_stands_for() {
    // `tab\tgap` is 7 characters, so the newline ending it starts at column 8
    // and the next line begins at column 1. Tab expansion must not move either.
    let spans = breaks("::: |\ntab\tgap\nwide\t\tgap\n\tlead\n:::\n");
    let first = spans[0].expect("first break placed");

    assert_eq!(first.start_line, 2);
    assert_eq!(first.start_column, 8);
    assert_eq!(first.end_line, 3);
    assert_eq!(first.end_column, 1);

    // `wide\t\tgap` is 9 characters.
    let second = spans[1].expect("second break placed");
    assert_eq!(second.start_line, 3);
    assert_eq!(second.start_column, 10);
    assert_eq!(second.end_line, 4);
    assert_eq!(second.end_column, 1);
}

#[test]
fn a_stanza_without_tabs_is_unchanged() {
    // This one was already placed by the anchors; the fill must not move it.
    let spans = breaks("::: |\nplain gap\nmore gap\n:::\n");
    let only = spans[0].expect("break placed");

    assert_eq!(only.start_line, 2);
    assert_eq!(only.start_column, 10);
    assert_eq!(only.end_line, 3);
    assert_eq!(only.end_column, 1);
}

#[test]
fn the_stanza_text_stays_unplaced_on_a_tab_line() {
    // The other half of the rule: a tab really does make the TEXT's columns
    // underivable, and this fix must not pretend otherwise.
    let options = carve::Options {
        positions: true,
        ..Default::default()
    };
    let doc = carve::parse_with_options("::: |\ntab\tgap\nwide\t\tgap\n:::\n", &options);
    let mut texts = 0;
    for block in &doc.children {
        if let BlockNode::LineBlock(lb) = block {
            for child in &lb.children {
                if let BlockNode::Paragraph(p) = child {
                    for inline in &p.children {
                        if let InlineNode::Text(t) = inline {
                            texts += 1;
                            assert!(t.pos.is_none(), "tab-bearing text should stay unplaced");
                        }
                    }
                }
            }
        }
    }
    assert!(texts > 0, "expected some text nodes");
}
