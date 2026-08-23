//! A blank line between an item's lead block and an attached container loosens
//! the item, the same way the same blank line loosens it before a plain block
//! (markup-carve/carve#1622).
//!
//! §17 L1 makes a blank line between an item's blocks the thing that loosens it.
//! This crate applied that before a plain block and not before a container, so
//! the blank line's meaning depended on the kind of block underneath it.
//!
//! The half that needs no ruling is that this crate contradicted ITSELF. A
//! container attached below a blank line read TIGHT with its closer written and
//! LOOSE without it, because an unterminated opener has no span to skip over.
//! markup-carve/carve#1602 settled that an explicit closer is a SPELLING change
//! and tightness may not move across it, and the canonical writer supplies the
//! missing closer -- so the two documents are `x` and `fmt(x)`, and PART 11 §1
//! requires them to parse alike. That fix bounded itself to an item whose whole
//! body IS the container; the attached configuration kept the defect.
//!
//! The contrast that does NOT move is the container attached with NO blank line
//! above it, which stays tight (markup-carve/carve-js#1376, corpus
//! `279-a-boundary-line-inside-an-open-fence-does-not-end-the-container-10`).
//! The distinction is the blank line, not the container.

fn tight(source: &str) -> bool {
    let carve::BlockNode::List(list) = &carve::parse(source).children[0] else {
        panic!("expected a list");
    };
    list.tight
}

/// The ticket's shape. A blank line, then a container holding two blocks.
#[test]
fn a_blank_line_above_an_attached_container_loosens() {
    let source = "- x\n\n  ::: d\n  a\n\n  b\n  :::\n";
    assert!(!tight(source));
    assert!(carve::render_html(&carve::parse(source))
        .unwrap()
        .contains("<p>x</p>"));
}

/// The same document with the closer dropped. It already read loose; the point
/// is that the pair now AGREES, so tightness no longer moves across a spelling
/// the writer itself supplies.
#[test]
fn the_closer_is_a_spelling_and_does_not_move_tightness() {
    let closed = "- x\n\n  ::: d\n  a\n\n  b\n  :::\n";
    let unclosed = "- x\n\n  ::: d\n  a\n\n  b\n";
    assert_eq!(tight(closed), tight(unclosed));
    assert!(!tight(unclosed));
}

/// THE CONTRAST. No blank line above the container, so nothing separates the
/// item's blocks and the item stays tight -- with the same container body that
/// loosens it in the case above.
#[test]
fn an_attached_container_with_no_blank_above_it_stays_tight() {
    let source = "- x\n  ::: d\n  a\n\n  b\n  :::\n";
    assert!(tight(source));
    let html = carve::render_html(&carve::parse(source)).unwrap();
    assert!(!html.contains("<p>x</p>"), "{html}");
}

/// A container with no interior blank has no second block inside it for the
/// separator to reach, so it stays tight either way. This is the bound on the
/// change: the blank above the container is necessary but not sufficient.
#[test]
fn a_container_holding_one_block_stays_tight_below_a_blank() {
    assert!(tight("- x\n\n  ::: d\n  a\n  :::\n"));
}

/// A CODE FENCE BODY IS VERBATIM, so a blank inside one is that block's own
/// content and never a separator between the item's blocks -- blank line above
/// it or not. Kept honest here because the fix walks past the fence branch.
#[test]
fn a_blank_inside_a_code_fence_still_does_not_loosen() {
    assert!(tight("- x\n\n  ```\n  a\n\n  b\n  ```\n"));
}

/// The kinds a blank line has always kept tight, swept so the change is known
/// not to have reached them. Every one of these agrees across carve-js and
/// carve-php today.
#[test]
fn a_blank_line_above_the_other_block_kinds_still_keeps_the_item_tight() {
    for source in [
        "- x\n\n  - y\n",
        "- x\n\n  # h\n",
        "- x\n\n  > q\n",
        "- x\n\n  | a |\n  |---|\n  | b |\n",
        "- x\n\n  ---\n",
    ] {
        assert!(tight(source), "{source:?} should still be tight");
    }
}

/// And the case the whole rule is named for, unchanged: a blank line above a
/// PLAIN block loosens, which is the answer the container case now converges on.
#[test]
fn a_blank_line_above_a_plain_block_still_loosens() {
    assert!(!tight("- x\n\n  y\n"));
}
