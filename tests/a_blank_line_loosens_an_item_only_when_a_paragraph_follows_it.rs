//! A blank line inside a list item loosens it ONLY when a PARAGRAPH follows the
//! blank (markup-carve/carve#1633, narrowing markup-carve/carve#1622).
//!
//! The mechanism is markup-carve/carve#1266's: an attached block CONSUMES the
//! blank the gap held. A container has an opener to absorb the separation, so
//! nothing is left for §17 L1 to read as a separator; a paragraph has no opener,
//! so the blank survives and L1's "blank-line-separated second paragraph" is
//! exactly what is there. The dividing line is PARAGRAPH versus every other
//! block kind - not plain block versus container, which is what the ruling's
//! first wording said and what markup-carve/carve#1630 measured as too wide.
//!
//! WHAT THIS FILE USED TO SAY. It pinned the wide reading: that a blank line
//! above an attached container loosens the item. That was carve#1622 as
//! originally worded, and markup-carve/carve-rs#1294 implemented it here. The
//! narrowing reverses those two assertions, and this file is rewritten in place
//! rather than deleted so the pair stays visible - the shapes are the same, the
//! answers are the other way round (markup-carve/carve-rs#1307).
//!
//! A BLANK INSIDE A CONTAINER IS THAT CONTAINER'S OWN CONTENT, and the paragraph
//! below it is the container's child rather than the item's second block. The
//! scan misattributed it, which is the whole defect: it walked INTO the
//! container and read the container's interior as the item's structure.

fn tight(source: &str) -> bool {
    let carve::BlockNode::List(list) = &carve::parse(source).children[0] else {
        panic!("expected a list");
    };
    list.tight
}

fn html(source: &str) -> String {
    carve::render_html(&carve::parse(source)).unwrap()
}

/// Corpus `409-a-blank-line-loosens-an-item-only-when-a-paragraph-follows-it-2`.
/// A blank line, then a container holding two blocks: the container's opener
/// takes the blank, so the item stays TIGHT. This crate published `tight=false`
/// here and was the only engine that did.
#[test]
fn a_blank_line_above_an_attached_container_does_not_loosen() {
    let source = "- x\n\n  ::: d\n  a\n\n  b\n  :::\n- z\n";
    assert!(tight(source));
    assert!(!html(source).contains("<p>x</p>"), "{}", html(source));
}

/// Corpus `362-an-unterminated-container-does-not-extend-the-item-past-a-blank-line-5`.
/// No blank above, an UNTERMINATED container, and the blank is inside it. The
/// missing closer cannot move the reading (markup-carve/carve#1632), and an
/// opener with no closer runs to the end of the item body, so its interior is
/// skipped whole exactly as a written closer's would be.
#[test]
fn an_unterminated_attached_container_keeps_its_blank_to_itself() {
    let source = "- x\n  :::\n  a\n\n  b\n";
    assert!(tight(source));
    assert!(!html(source).contains("<p>x</p>"), "{}", html(source));
}

/// THE CLOSER IS A SPELLING. The canonical writer supplies a missing closer, so
/// the two documents are `x` and `fmt(x)` and PART 11 §1 requires them to parse
/// alike (markup-carve/carve#1602). They agree - and they agree on TIGHT now,
/// where this file previously required them to agree on loose.
#[test]
fn the_closer_is_a_spelling_and_does_not_move_tightness() {
    let closed = "- x\n\n  ::: d\n  a\n\n  b\n  :::\n";
    let unclosed = "- x\n\n  ::: d\n  a\n\n  b\n";
    assert_eq!(tight(closed), tight(unclosed));
    assert!(tight(unclosed));
}

/// THE CONTRAST, and the one shape that still loosens: a PARAGRAPH after the
/// blank. It has no opener to consume the separation.
#[test]
fn a_blank_line_above_a_paragraph_still_loosens() {
    assert!(!tight("- x\n\n  y\n"));
    assert!(html("- x\n\n  y\n").contains("<p>x</p>"));
}

/// No blank line above the container either - unchanged, and unchanged for the
/// same reason it always was (corpus `279-...-10`, markup-carve/carve-js#1376).
#[test]
fn an_attached_container_with_no_blank_above_it_stays_tight() {
    let source = "- x\n  ::: d\n  a\n\n  b\n  :::\n";
    assert!(tight(source));
    assert!(!html(source).contains("<p>x</p>"), "{}", html(source));
}

/// A container with no interior blank has no second block inside it at all, so
/// it stays tight for a second, independent reason. Kept as a control: it is
/// tight both before and after the narrowing, so it discriminates nothing on its
/// own and is here to show the change did not reach it.
#[test]
fn a_container_holding_one_block_stays_tight_below_a_blank() {
    assert!(tight("- x\n\n  ::: d\n  a\n  :::\n"));
}

/// A CODE FENCE BODY IS VERBATIM, so a blank inside one is that block's own
/// content, blank line above it or not.
#[test]
fn a_blank_inside_a_code_fence_still_does_not_loosen() {
    assert!(tight("- x\n\n  ```\n  a\n\n  b\n  ```\n"));
}

/// The kinds a blank line has always kept tight, swept so the narrowing is known
/// not to have reached them. Every one agrees across carve-js and carve-php.
#[test]
fn a_blank_line_above_the_other_block_kinds_still_keeps_the_item_tight() {
    for source in [
        "- x\n\n  - y\n",
        "- x\n\n  # h\n",
        "- x\n\n  > q\n",
        "- x\n\n  | a |\n  |---|\n  | b |\n",
        "- x\n\n  ---\n",
        "- x\n\n  {.c}\n  > q\n",
    ] {
        assert!(tight(source), "{source:?} should still be tight");
    }
}
