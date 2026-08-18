//! PART 11 S1: `to_html(fmt(x)) == to_html(x)`. Two shapes broke it, and both
//! broke it the same way - the writer emitted bytes that the next parse reads as
//! a DIFFERENT construct (carve-rs#819).
//!
//! 1. A `+`-attached block whose first line opens no block of its own lost its
//!    continuation marker, so the block folded into the paragraph above it. The
//!    comment at that site said "only a paragraph reaches this: no other
//!    attached kind can fold into an open paragraph", and in the same breath
//!    said why nothing caught it: the corpus pins a FENCE and a QUOTE, and both
//!    of those OPEN a block at the item's content column. An image line opens
//!    nothing.
//!
//! 2. A header cell's `=` and the content's first character merged into a longer
//!    marker run, because the alignment sigil is read GLUED after the header
//!    marker off the untrimmed cell.
//!
//! The THIRD defect on that ticket - the canonical spelling of a footnote
//! definition with an empty body - is NOT here. It is a maintainer question at
//! `markup-carve/carve#999`, because carve-js writes an `{empty}` sentinel that
//! no clause names.

fn html(src: &str) -> String {
    carve::to_html(src)
}

fn fmt(src: &str) -> String {
    carve::to_carve(src)
}

/// PART 11 S1, asserted directly: formatting must not change the document.
fn round_trips(src: &str) -> bool {
    html(&fmt(src)) == html(src)
}

// ---------------------------------------------------------------------------
// 1. The `+`-attached block that folds.
// ---------------------------------------------------------------------------

#[test]
fn a_plus_attached_captioned_image_keeps_its_figure() {
    let src = "- x\n+\n![a](i.png)\n^ cap\n";
    assert!(
        html(src).contains("<figcaption>cap</figcaption>"),
        "the premise: {}",
        html(src)
    );
    assert_eq!(fmt(src), "- x\n+\n![a](i.png)\n^ cap\n");
    assert!(round_trips(src), "{}", fmt(src));
}

#[test]
fn a_plus_attached_bare_image_keeps_its_block() {
    // Not on the ticket. The caption is not what breaks: an image line opens no
    // block at the content column, so it folds with or without one.
    let src = "- x\n+\n![a](i.png)\n";
    assert_eq!(fmt(src), "- x\n+\n![a](i.png)\n");
    assert!(round_trips(src), "{}", fmt(src));
}

#[test]
fn control_a_plus_attached_paragraph_still_gets_the_marker() {
    // The case the site was written for (carve#861); it must not move.
    let src = "- x\n+\np2\n";
    assert_eq!(fmt(src), "- x\n+\np2\n");
    assert!(round_trips(src));
}

#[test]
fn control_a_plus_attached_block_opener_still_gets_no_marker() {
    // These are the corpus's own shapes, and the reason it never caught the
    // defect: each opens a block at the item's content column, so the marker
    // would be noise. The writer must keep writing them WITHOUT it.
    for (src, formatted) in [
        ("- x\n+\n> q\n", "- x\n  > q\n"),
        ("- x\n+\n```\nc\n```\n", "- x\n  ```\n  c\n  ```\n"),
        ("- x\n+\n# H\n", "- x\n  # H\n"),
        ("- x\n+\n---\n", "- x\n  ---\n"),
        (
            "- x\n+\n::: note\nb\n:::\n",
            "- x\n  ::: note\n  b\n  :::\n",
        ),
    ] {
        assert_eq!(fmt(src), formatted, "for {src:?}");
        assert!(round_trips(src), "for {src:?}: {}", fmt(src));
    }
}

// ---------------------------------------------------------------------------
// 2. The header cell whose content starts with a marker character.
// ---------------------------------------------------------------------------

#[test]
fn a_header_cell_does_not_hand_its_first_character_to_the_alignment_reader() {
    let src = "| ~x~ |\n|---|\n| y |\n";
    assert!(html(src).contains("<s>x</s>"), "the premise: {}", html(src));
    assert!(
        !html(src).contains("text-align"),
        "the premise: nothing here is aligned - {}",
        html(src)
    );
    assert_eq!(fmt(src), "|= ~x~ |\n| y |\n");
    assert!(round_trips(src), "{}", fmt(src));
}

#[test]
fn the_other_two_alignment_sigils_answer_the_same_way() {
    // `<` and `>` reach the writer already escaped as literal text, which is why
    // `~` was the only spelling that broke in practice. The separator does not
    // depend on that: it is about the marker run, not about which character
    // happens to survive escaping.
    for src in ["| <x |\n|---|\n| y |\n", "| >x |\n|---|\n| y |\n"] {
        assert!(round_trips(src), "for {src:?}: {}", fmt(src));
        assert!(
            !html(&fmt(src)).contains("text-align"),
            "for {src:?}: {}",
            html(&fmt(src))
        );
    }
}

#[test]
fn control_the_header_marker_stays_glued_to_the_pipe() {
    // The `=` is only a header marker while it is GLUED to the pipe, so the
    // padding goes AFTER the prefix and never before it (PART 11 §6e).
    assert_eq!(fmt("| a |\n|---|\n| y |\n"), "|= a |\n| y |\n");
    assert!(round_trips("| a |\n|---|\n| y |\n"));
}

#[test]
fn control_a_header_cell_that_already_carries_alignment_is_padded_too() {
    // The alignment marker is part of the PREFIX, so it stays glued to the `=`
    // and the padding follows the whole run.
    let src = "| a |\n|:-:|\n| ~y~ |\n";
    assert_eq!(fmt(src), "|=~ a |\n| ~y~ |\n");
    assert!(round_trips(src));
}

#[test]
fn a_cell_that_already_carries_alignment_pads_its_sigil_content() {
    // The prefix has consumed the reader's alignment slot here, so the content
    // could have been written glued and still round-trip. It is padded anyway:
    // PART 11 §6e is about every cell, and one form per document is the point.
    for (src, want) in [
        ("|~ ~x~|\n| y |\n", "|~ ~x~ |\n| y |\n"),
        ("|=~ ~x~|\n| y |\n", "|=~ ~x~ |\n| y |\n"),
    ] {
        assert_eq!(fmt(src), want, "for {src:?}");
        assert!(round_trips(src), "for {src:?}");
        assert!(html(src).contains("<s>x</s>"), "the premise: {}", html(src));
    }
}

#[test]
fn control_a_body_cell_is_padded_and_needs_nothing() {
    // A cell with no prefix was always written with its padding spaces, which
    // already part the pipe from the content; §6e extends that to every cell.
    let src = "| a |\n|---|\n| ~y~ |\n";
    assert_eq!(fmt(src), "|= a |\n| ~y~ |\n");
    assert!(round_trips(src));
}
