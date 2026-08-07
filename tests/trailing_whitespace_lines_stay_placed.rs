//! A content line ending in whitespace still places what is on it.
//!
//! Trailing ASCII whitespace is dropped from a content line (PART 2, corpus
//! 268), so the line the inline parser sees is not a SUFFIX of the source
//! line: it is that line with characters taken off BOTH ends. `stripped_col`
//! knew only the suffix shape, so it answered `None`, and every inline anchored on
//! the line lost its position: `abc<SP>` published a paragraph with an unplaced
//! `text`, while the identical document without the space placed it.
//!
//! A trailing trim moves nothing in front of it, so the constant that maps a
//! column back to the document is the leading width alone and an honest span
//! plainly exists - PART 12 §4's exemption is for a node that CANNOT be placed.
//! Fifteen of this engine's position findings were this one line, across a
//! paragraph, a heading-shaped paragraph, a list item, a block quote and a line
//! block (markup-carve/carve#961).
//!
//! The line block needed a second change for the same reason, in the check that
//! decides whether a verse line is placeable at all: §23 leaves a lone trailing
//! space as an ordinary space and PART 2 then drops it, so the rewritten line
//! is SHORTER than its source, where an equal-length test read a refusal.

use carve::{BlockNode, InlineNode, Options};

/// Every text node in the tree as `(value, span)`, in walk order.
fn texts(source: &str) -> Vec<(String, Option<(usize, usize)>)> {
    let doc = carve::parse_with_options(source, &Options::default().with_positions(true));
    let mut out = Vec::new();
    fn inlines(nodes: &[InlineNode], out: &mut Vec<(String, Option<(usize, usize)>)>) {
        for node in nodes {
            if let InlineNode::Text(t) = node {
                out.push((
                    t.value.clone(),
                    t.pos.as_ref().map(|p| (p.start_offset, p.end_offset)),
                ));
            }
        }
    }
    fn blocks(nodes: &[BlockNode], out: &mut Vec<(String, Option<(usize, usize)>)>) {
        for node in nodes {
            match node {
                BlockNode::Paragraph(p) => inlines(&p.children, out),
                BlockNode::Heading(h) => inlines(&h.children, out),
                BlockNode::BlockQuote(b) => blocks(&b.children, out),
                BlockNode::LineBlock(l) => blocks(&l.children, out),
                BlockNode::List(l) => {
                    for item in &l.items {
                        blocks(&item.children, out);
                    }
                }
                _ => {}
            }
        }
    }
    blocks(&doc.children, &mut out);
    out
}

/// Every text node's span names its own value, and none is missing.
fn assert_all_placed_and_exact(source: &str) {
    let chars: Vec<char> = source.chars().collect();
    for (value, pos) in texts(source) {
        let (start, end) = pos.unwrap_or_else(|| panic!("{value:?} carries no pos in {source:?}"));
        let slice: String = chars[start..end].iter().collect();
        assert_eq!(
            slice, value,
            "the span names {slice:?} and the node says {value:?} in {source:?}"
        );
    }
}

#[test]
fn a_paragraph_line_ending_in_a_space_is_placed() {
    assert_all_placed_and_exact("abc \n");
    assert_eq!(texts("abc \n"), vec![("abc".to_string(), Some((0, 3)))]);
}

#[test]
fn a_tab_at_the_end_of_a_paragraph_line_is_placed_too() {
    // The tab spelling of the same rule: it is trailing whitespace, not
    // indentation, so the front of the line is untouched either way.
    assert_all_placed_and_exact("abc\t\ndef\t\n");
}

#[test]
fn the_soft_break_between_two_trimmed_lines_is_placed() {
    let doc = carve::parse_with_options("abc \ndef\n", &Options::default().with_positions(true));
    let BlockNode::Paragraph(p) = &doc.children[0] else {
        panic!("not a paragraph");
    };
    let unplaced: Vec<_> = p
        .children
        .iter()
        .filter(|n| match n {
            InlineNode::Text(t) => t.pos.is_none(),
            InlineNode::SoftBreak(b) => b.pos.is_none(),
            _ => false,
        })
        .collect();
    assert!(unplaced.is_empty(), "unplaced inlines: {unplaced:?}");
}

#[test]
fn a_list_item_and_a_block_quote_line_are_placed() {
    // The container spellings: the marker or the quote prefix is removed from
    // the front and the whitespace from the back, which is the pair the suffix
    // test could not express.
    assert_all_placed_and_exact("# Title \n\n- item \n\n> quoted \n");
}

#[test]
fn a_byte_order_mark_between_two_spaces_is_placed_as_content() {
    // A BOM is CONTENT, not whitespace, so it survives the trim at both ends
    // and its span is the one character it is.
    assert_eq!(
        texts(" \u{feff} \n"),
        vec![("\u{feff}".to_string(), Some((1, 2)))]
    );
}

#[test]
fn a_verse_line_ending_in_one_space_is_placed() {
    // §23 leaves a lone trailing space an ordinary space and PART 2 drops it,
    // so the rewritten line is one character shorter than its source. Nothing
    // in front of it moved.
    assert_eq!(
        texts("::: |\ndef \n:::\n"),
        vec![("def".to_string(), Some((6, 9)))]
    );
}

#[test]
fn a_verse_line_ending_in_two_spaces_is_unchanged() {
    // THE CONTROL next to it. Two columns are a medial gap, so they become NBSP
    // CONTENT rather than being dropped, and the line was always placeable.
    let found = texts("::: |\ndef  \n:::\n");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].1, Some((6, 11)));
    assert!(found[0].0.starts_with("def"));
    assert_eq!(found[0].0.chars().count(), 5, "the two gaps are content");
}

#[test]
fn a_tab_inside_a_verse_line_still_refuses() {
    // THE CONTROL THE PLACEABILITY CHECK EXISTS FOR. A tab expands to up to
    // four placeholders from one source character, so the value stops being a
    // slice of the source and no honest span exists - carve-js publishes none
    // either, and the spec corpus declares this one permitted. Loosening the
    // check for a dropped trailing run must not loosen it for a tab.
    let found = texts("::: |\ntab\tgap\n:::\n");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].1, None, "a tab-bearing verse line was placed");
}

#[test]
fn the_documents_without_trailing_whitespace_are_unchanged() {
    // The neighbouring case throughout: with nothing to trim there is nothing
    // to reason about, and these always worked.
    for source in ["abc\n", "abc\ndef\n", "# Title\n\n- item\n\n> quoted\n"] {
        assert_all_placed_and_exact(source);
    }
    assert_eq!(texts("abc\n"), vec![("abc".to_string(), Some((0, 3)))]);
}
