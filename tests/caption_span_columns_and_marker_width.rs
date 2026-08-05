//! Supplements `caption_span_excludes_the_marker` (carve-rs#623, which fixed
//! carve-rs#620) with three cases that fix could have passed while still being
//! wrong.
//!
//! That test compares each caption text node's value against the source it claims
//! and covers the reference form, the direct form, a multi-WORD caption and an
//! unresolved reference. What it does not pin:
//!
//!   1. a multi-SPACE marker. `^` plus a RUN of spaces is one marker (§4), so the
//!      span advances by the marker's whole width. Advancing by a fixed 2 passes
//!      every case in that file - including the multi-word one, whose marker is
//!      exactly `^ ` - and fails here.
//!   2. the END of the span. Shifting BOTH ends by the marker width still slices
//!      back to a string of the right length, at the wrong place.
//!   3. COLUMNS. Only offsets were compared, and the column is a separate field
//!      that has to move with them.
//!
//! Kept as its own file rather than merged into that one: these are assertions
//! about the same fix from a different angle, and a reader arriving from carve#620
//! should find the value-vs-source sweep there and the arithmetic here.

use carve::ast::{BlockNode, InlineNode};
use carve::to_html;

/// Positions are OPT-IN. Without this the nodes carry no `pos` at all and every
/// assertion below fails for the wrong reason - which is how the first draft of
/// this file failed.
fn caption_pos(src: &str) -> (String, usize, usize, usize) {
    let options = carve::Options {
        positions: true,
        ..Default::default()
    };
    let doc = carve::parse_with_options(src, &options);
    let BlockNode::Figure(figure) = &doc.children[0] else {
        panic!("expected a figure, got {:?}", doc.children[0]);
    };
    let InlineNode::Text(text) = &figure.caption[0] else {
        panic!("expected the caption to start with text");
    };
    let pos = text
        .pos
        .as_ref()
        .expect("the caption's text has a position");
    (
        text.value.clone(),
        pos.start_offset,
        pos.end_offset,
        pos.start_column,
    )
}

#[test]
fn a_multi_space_marker_advances_by_its_whole_width() {
    // `^` + THREE spaces: a fixed advance of 2 would leave two of them inside the
    // span, and would still slice back to something for a single-space marker.
    let (value, start, end, column) = caption_pos("![a][ok]\n^   cap\n\n[ok]: /p.png\n");
    assert_eq!(value, "cap");
    assert_eq!((start, end), (13, 16));
    assert_eq!(column, 5);
}

#[test]
fn only_the_start_moves() {
    // The both-ends-shifted failure: `(11, 14)` is right, `(11, 16)` and `(9, 12)`
    // both slice back to three characters and are both wrong.
    let (value, start, end, column) = caption_pos("![a][ok]\n^ cap\n\n[ok]: /p.png\n");
    assert_eq!(value, "cap");
    assert_eq!((start, end), (11, 14));
    assert_eq!(column, 3);
}

#[test]
fn the_direct_form_has_the_same_arithmetic() {
    // Different code path - the parser positions this one directly - so the two
    // must agree on the numbers, not merely each be self-consistent.
    let (value, start, end, column) = caption_pos("![a](/p.png)\n^ cap\n");
    assert_eq!(value, "cap");
    assert_eq!((start, end), (15, 18));
    assert_eq!(column, 3);
}

#[test]
fn a_position_fix_does_not_change_the_rendering() {
    // The HTML was correct throughout, in all three engines, which is why only a
    // span check ever saw this. Pinned so the reverse cannot happen either.
    let html = to_html("![a][ok]\n^ cap\n\n[ok]: /p.png\n");
    assert!(html.contains("<figcaption>cap</figcaption>"), "{html}");
    assert!(!html.contains('^'), "{html}");
}
